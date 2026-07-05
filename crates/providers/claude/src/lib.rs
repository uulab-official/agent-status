use agent_core::{Confidence, ConnectionState, LimitWindow, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, file_exists, BasePluginState};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use std::path::{Path, PathBuf};

const SESSION_WINDOW_HOURS: i64 = 5;
const WEEKLY_WINDOW_DAYS: i64 = 7;

fn claude_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude"))
}

fn claude_projects_dir() -> Option<PathBuf> {
    claude_config_dir().map(|dir| dir.join("projects"))
}

/// One usage-bearing line from a Claude Code session transcript
/// (`~/.claude/projects/**/*.jsonl`). Mirrors the subset of fields `ccusage`
/// relies on — everything else in the transcript is conversation content
/// this plugin has no reason to read.
#[derive(Debug, Clone, Copy)]
struct UsageEntry {
    timestamp: DateTime<Utc>,
    tokens: f64,
}

/// A long agentic session re-sends its cached system prompt/context on
/// *every* turn, so `cache_read_input_tokens` alone can reach the hundreds
/// of millions in a single 5-hour window — summing it 1:1 with fresh input
/// tokens made the reported "used" count wildly overstate real usage
/// pressure (a session with a 150k-token cached context and 200 turns would
/// report 30M+ tokens for what's actually a much smaller amount of new work).
/// Weight each field the way Anthropic bills it (cache writes cost 1.25x a
/// fresh input token, cache reads cost 0.1x) so the total approximates
/// *effective* tokens consumed rather than raw bytes re-read from cache.
fn tokens_from_usage(usage: &serde_json::Value) -> f64 {
    let field = |key: &str| usage.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    field("input_tokens") + field("output_tokens") + field("cache_creation_input_tokens") * 1.25 + field("cache_read_input_tokens") * 0.1
}

fn parse_entry(line: &str) -> Option<UsageEntry> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let timestamp_str = value.get("timestamp")?.as_str()?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp_str).ok()?.with_timezone(&Utc);
    let usage = value.get("message")?.get("usage")?;
    Some(UsageEntry { timestamp, tokens: tokens_from_usage(usage) })
}

/// `~/.claude/projects/<project>/<session>.jsonl`, arbitrarily nested under
/// `projects/` (subagent transcripts live one level deeper still) — walk
/// every `.jsonl` file found rather than assuming a fixed depth.
fn find_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_jsonl_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn read_usage_entries(projects_dir: &Path) -> Vec<UsageEntry> {
    let mut files = Vec::new();
    find_jsonl_files(projects_dir, &mut files);
    files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .flat_map(|contents| contents.lines().filter_map(parse_entry).collect::<Vec<_>>().into_iter())
        .collect()
}

/// Sums tokens for entries newer than `now - window`. Anthropic's own docs
/// describe both the 5-hour session cap and the weekly cap as *rolling*
/// windows (usage from N hours/days ago drops off continuously, not at a
/// fixed boundary), so a simple rolling sum is the accurate model here —
/// unlike `ccusage`'s fixed 5-hour "blocks" (built for historical reporting,
/// not for mirroring the live rolling cap).
fn tokens_in_window(entries: &[UsageEntry], now: DateTime<Utc>, window: Duration) -> f64 {
    let cutoff = now - window;
    entries.iter().filter(|entry| entry.timestamp > cutoff).map(|entry| entry.tokens).sum()
}

/// Claude (claude.ai + Claude Code CLI). See README.md for the confidence
/// tiers `fetch_status` targets.
pub struct ClaudePlugin {
    state: BasePluginState,
    projects_dir: Option<PathBuf>,
}

impl ClaudePlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("claude", "Claude"), projects_dir: claude_projects_dir() }
    }

    /// Used by tests to point at a fixture directory instead of the real
    /// `~/.claude/projects` — never let a test read another project's actual
    /// session transcripts.
    pub fn with_projects_dir(projects_dir: impl Into<PathBuf>) -> Self {
        Self { projects_dir: Some(projects_dir.into()), ..Self::new() }
    }

    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        let projects_dir = self.projects_dir.as_ref().ok_or("could not resolve ~/.claude/projects")?;
        if !file_exists(projects_dir) {
            return Err(format!("{} does not exist — no Claude Code session transcripts yet", projects_dir.display()));
        }

        let entries = read_usage_entries(projects_dir);
        let now = Utc::now();

        let session_tokens = tokens_in_window(&entries, now, Duration::hours(SESSION_WINDOW_HOURS));
        let weekly_tokens = tokens_in_window(&entries, now, Duration::days(WEEKLY_WINDOW_DAYS));

        let limits = vec![
            LimitWindow {
                id: "claude:session".into(),
                label: "5-hour".into(),
                period: "session".into(),
                unit: "tokens".into(),
                limit: None,
                used: session_tokens,
                percent_used: None,
                resets_at: None,
                confidence: Confidence::CliLog,
            },
            LimitWindow {
                id: "claude:weekly".into(),
                label: "Weekly".into(),
                period: "weekly".into(),
                unit: "tokens".into(),
                limit: None,
                used: weekly_tokens,
                percent_used: None,
                resets_at: None,
                confidence: Confidence::CliLog,
            },
        ];

        Ok(ProviderStatus {
            provider_id: self.id().into(),
            display_name: self.display_name().into(),
            state: ConnectionState::Online,
            limits,
            models: vec![],
            cost: None,
            observed_at: now.to_rfc3339(),
            detail: Some(format!(
                "~{:.0} effective tokens in the last {SESSION_WINDOW_HOURS}h, ~{:.0} in the last {WEEKLY_WINDOW_DAYS}d (cache reads discounted) — no official cap available, see README.md",
                session_tokens, weekly_tokens
            )),
        })
    }
}

impl Default for ClaudePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderPlugin for ClaudePlugin {
    fn id(&self) -> &str {
        "claude"
    }
    fn display_name(&self) -> &str {
        "Claude"
    }
    fn refresh_interval_ms(&self) -> u64 {
        60_000
    }

    async fn detect(&self) -> bool {
        let has_cli = command_exists_on_path("claude");
        let has_config_dir = claude_config_dir().map(|dir| file_exists(&dir)).unwrap_or(false);
        has_cli || has_config_dir
    }

    async fn refresh(&mut self) {
        match self.fetch_status().await {
            Ok(status) => self.state.set_status(status),
            Err(e) => self.state.set_error(e),
        }
    }

    fn get_status(&self) -> ProviderStatus {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Scoped by test name + pid so parallel tests never share a fixture dir;
    /// callers must `fs::remove_dir_all` it when done.
    fn fixture_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agent-status-claude-test-{test_name}-{}", std::process::id()))
    }

    fn write_fixture(dir: &Path, subpath: &str, lines: &[String]) {
        let file_path = dir.join(subpath);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, lines.join("\n")).unwrap();
    }

    fn usage_line(timestamp: &str, input_tokens: u64, output_tokens: u64) -> String {
        serde_json::json!({
            "timestamp": timestamp,
            "message": {
                "usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                }
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn refresh_reports_unknown_when_projects_dir_is_missing() {
        let dir = fixture_dir("missing-dir");
        let missing = dir.join("does-not-exist");
        let mut plugin = ClaudePlugin::with_projects_dir(missing);
        plugin.refresh().await;
        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Unknown);
    }

    #[tokio::test]
    async fn refresh_sums_tokens_within_rolling_windows() {
        let dir = fixture_dir("rolling-windows");
        let now = Utc::now();
        let recent = now - Duration::minutes(30);
        let three_days_ago = now - Duration::days(3);
        let ten_days_ago = now - Duration::days(10);

        write_fixture(
            &dir,
            "proj-a/session-1.jsonl",
            &[
                usage_line(&recent.to_rfc3339(), 100, 50),
                usage_line(&three_days_ago.to_rfc3339(), 200, 20),
                usage_line(&ten_days_ago.to_rfc3339(), 9000, 9000),
            ],
        );

        let mut plugin = ClaudePlugin::with_projects_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        let session = status.limits.iter().find(|w| w.id == "claude:session").unwrap();
        let weekly = status.limits.iter().find(|w| w.id == "claude:weekly").unwrap();

        assert_eq!(session.used, 150.0);
        assert_eq!(weekly.used, 370.0);
        assert_eq!(session.confidence, Confidence::CliLog);
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn malformed_lines_are_skipped_without_failing() {
        let dir = fixture_dir("malformed-lines");
        let now = Utc::now();
        write_fixture(
            &dir,
            "proj-a/session-1.jsonl",
            &["not json at all".to_string(), usage_line(&now.to_rfc3339(), 10, 5), "{\"no\":\"usage\"}".to_string()],
        );

        let mut plugin = ClaudePlugin::with_projects_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        let session = status.limits.iter().find(|w| w.id == "claude:session").unwrap();
        assert_eq!(session.used, 15.0);
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn cache_read_tokens_are_discounted_instead_of_counted_at_full_weight() {
        // A long agentic session re-sends its cached context on every turn:
        // 100 turns re-reading a 200k-token cached context would otherwise
        // report 20M tokens for what's actually a few hundred new tokens of
        // real work. `tokens_from_usage` weights cache reads at 0.1x and
        // cache writes at 1.25x, matching Anthropic's own cache pricing.
        let dir = fixture_dir("cache-discount");
        let now = Utc::now();
        let line = serde_json::json!({
            "timestamp": now.to_rfc3339(),
            "message": {
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "cache_creation_input_tokens": 1000,
                    "cache_read_input_tokens": 200_000,
                }
            }
        })
        .to_string();
        write_fixture(&dir, "proj-a/session-1.jsonl", &[line]);

        let mut plugin = ClaudePlugin::with_projects_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        let session = status.limits.iter().find(|w| w.id == "claude:session").unwrap();
        // 10 + 5 + 1000*1.25 + 200_000*0.1 = 21_265
        assert_eq!(session.used, 21_265.0);
        fs::remove_dir_all(&dir).ok();
    }
}
