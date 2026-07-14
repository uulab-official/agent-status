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
#[derive(Debug, Clone)]
struct UsageEntry {
    timestamp: DateTime<Utc>,
    tokens: f64,
    model: String,
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
    let message = value.get("message")?;
    let usage = message.get("usage")?;
    let model = message.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    Some(UsageEntry { timestamp, tokens: tokens_from_usage(usage), model })
}

/// Turns a raw model id (`"claude-sonnet-4-6"`, `"claude-haiku-4-5-20251001"`,
/// `"<synthetic>"`) into what the popover's per-model breakdown should show
/// (`"Sonnet 4.6"`, `"Haiku 4.5"`, `"Other"`). Synthetic entries are internal
/// bookkeeping (e.g. summarization), not a model a user chose, so they're
/// grouped under "Other" rather than shown as a confusing literal id.
fn format_model_name(model: &str) -> String {
    let Some(rest) = model.strip_prefix("claude-") else { return "Other".to_string() };
    let mut parts = rest.split('-');
    let Some(name) = parts.next() else { return "Other".to_string() };
    let mut label = name.chars().next().map_or(String::new(), |c| c.to_uppercase().to_string()) + &name[name.chars().next().map_or(0, char::len_utf8)..];
    let version: Vec<&str> = parts.take_while(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() < 8).collect();
    if !version.is_empty() {
        label.push(' ');
        label.push_str(&version.join("."));
    }
    label
}

/// Groups effective tokens within `window` by model, as a percentage of
/// that window's own total — a proportion of *known, already-computed*
/// usage, not a percentage of any quota, so it doesn't run into the
/// "inventing a cap" problem this plugin otherwise avoids. Models under 1%
/// are folded into "Other" so a long tail of one-off entries doesn't turn
/// the popover's hover text into a wall of near-zero percentages.
fn model_breakdown(entries: &[UsageEntry], now: DateTime<Utc>, window: Duration) -> Vec<(String, f64)> {
    let cutoff = now - window;
    let mut totals: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut grand_total = 0.0;
    for entry in entries.iter().filter(|entry| entry.timestamp > cutoff) {
        *totals.entry(format_model_name(&entry.model)).or_insert(0.0) += entry.tokens;
        grand_total += entry.tokens;
    }
    if grand_total <= 0.0 {
        return Vec::new();
    }
    let mut other = 0.0;
    let mut breakdown: Vec<(String, f64)> = Vec::new();
    for (model, tokens) in totals {
        let percent = tokens / grand_total * 100.0;
        if percent < 1.0 {
            other += percent;
        } else {
            breakdown.push((model, percent));
        }
    }
    breakdown.sort_by(|a, b| b.1.total_cmp(&a.1));
    if other > 0.0 {
        breakdown.push(("Other".to_string(), other));
    }
    breakdown
}

fn format_model_breakdown(entries: &[UsageEntry], now: DateTime<Utc>, window: Duration, window_label: &str) -> Option<String> {
    let breakdown = model_breakdown(entries, now, window);
    // A single model (the common case) isn't a "breakdown" worth stating —
    // it's just "all of it," which the caption's own total already implies.
    if breakdown.len() < 2 {
        return None;
    }
    let parts: Vec<String> = breakdown.iter().map(|(model, percent)| format!("{model} {percent:.0}%")).collect();
    Some(format!("By model ({window_label}): {}", parts.join(", ")))
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
    // `Iterator::sum::<f64>()` on an empty iterator yields *negative* zero
    // (confirmed: `Vec::<f64>::new().iter().sum()` is `-0.0`, unlike a plain
    // `fold(0.0, Add::add)`), which `format!("{value:.0}")` then renders as
    // the literal string "-0" — a real bug hit live: an idle 5-hour window
    // showed "-0 tokens" instead of "0 tokens". `fold` avoids the identity
    // Rust's `Sum` impl uses for floats.
    entries.iter().filter(|entry| entry.timestamp > cutoff).fold(0.0, |total, entry| total + entry.tokens)
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

        let mut detail = format!(
            "~{:.0} effective tokens in the last {SESSION_WINDOW_HOURS}h, ~{:.0} in the last {WEEKLY_WINDOW_DAYS}d (cache reads discounted) — no official cap available, see README.md",
            session_tokens, weekly_tokens
        );
        // The weekly window is the one likely to have actually seen more
        // than one model in play; the 5-hour window is usually a single
        // work session with a single model, where a "breakdown" of one
        // entry would just restate the total.
        if let Some(breakdown) = format_model_breakdown(&entries, now, Duration::days(WEEKLY_WINDOW_DAYS), "7d") {
            detail.push_str(" | ");
            detail.push_str(&breakdown);
        }

        Ok(ProviderStatus {
            provider_id: self.id().into(),
            display_name: self.display_name().into(),
            state: ConnectionState::Online,
            limits,
            models: vec![],
            cost: None,
            observed_at: now.to_rfc3339(),
            detail: Some(detail),
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
        usage_line_for_model(timestamp, "claude-sonnet-5", input_tokens, output_tokens)
    }

    fn usage_line_for_model(timestamp: &str, model: &str, input_tokens: u64, output_tokens: u64) -> String {
        serde_json::json!({
            "timestamp": timestamp,
            "message": {
                "model": model,
                "usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                }
            }
        })
        .to_string()
    }

    #[test]
    fn tokens_in_window_is_positive_zero_when_nothing_falls_inside_it() {
        // Rust's `Iterator::sum::<f64>()` on an empty iterator is negative
        // zero, which `format!("{value:.0}")` renders as the string "-0" —
        // a real bug hit live (an idle 5-hour window showed "-0 tokens").
        // `is_sign_positive()` is the actual assertion that would have
        // caught it; `assert_eq!(0.0, -0.0)` passes in Rust (IEEE equality
        // treats them as equal) and would not have.
        let now = Utc::now();
        let old_entry = UsageEntry { timestamp: now - Duration::hours(10), tokens: 500.0, model: "claude-sonnet-5".into() };
        let total = tokens_in_window(&[old_entry], now, Duration::hours(SESSION_WINDOW_HOURS));
        assert_eq!(total, 0.0);
        assert!(total.is_sign_positive(), "expected +0.0, got -0.0");
    }

    #[test]
    fn format_model_name_turns_raw_ids_into_readable_labels() {
        assert_eq!(format_model_name("claude-sonnet-4-6"), "Sonnet 4.6");
        assert_eq!(format_model_name("claude-opus-4-8"), "Opus 4.8");
        assert_eq!(format_model_name("claude-sonnet-5"), "Sonnet 5");
        // A trailing date-like segment (not a version number) is dropped.
        assert_eq!(format_model_name("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(format_model_name("<synthetic>"), "Other");
        assert_eq!(format_model_name("unknown"), "Other");
    }

    #[test]
    fn model_breakdown_is_none_when_only_one_model_was_used() {
        let now = Utc::now();
        let entries = vec![
            UsageEntry { timestamp: now, tokens: 100.0, model: "claude-sonnet-5".into() },
            UsageEntry { timestamp: now, tokens: 50.0, model: "claude-sonnet-5".into() },
        ];
        // A single model isn't a "breakdown" -- the caption's own total
        // already implies "all of it was this model."
        assert!(format_model_breakdown(&entries, now, Duration::days(7), "7d").is_none());
    }

    #[test]
    fn model_breakdown_reports_percent_of_the_windows_own_total_and_folds_small_shares_into_other() {
        let now = Utc::now();
        let entries = vec![
            UsageEntry { timestamp: now, tokens: 700.0, model: "claude-sonnet-5".into() },
            UsageEntry { timestamp: now, tokens: 290.0, model: "claude-opus-4-8".into() },
            // Under 1% of the 1000-token total -- folded into "Other" rather
            // than cluttering the breakdown with a near-zero percentage.
            UsageEntry { timestamp: now, tokens: 5.0, model: "claude-haiku-4-5-20251001".into() },
            UsageEntry { timestamp: now, tokens: 5.0, model: "<synthetic>".into() },
        ];
        let text = format_model_breakdown(&entries, now, Duration::days(7), "7d").unwrap();
        assert_eq!(text, "By model (7d): Sonnet 5 70%, Opus 4.8 29%, Other 1%");
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
