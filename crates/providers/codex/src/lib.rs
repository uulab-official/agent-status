use agent_core::{Confidence, ConnectionState, LimitWindow, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, BasePluginState};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// OpenAI Codex CLI. See README.md for the confidence tiers `fetch_status`
/// targets.
///
/// Real rate-limit percentages come from `~/.codex/sessions/**/*.jsonl` —
/// Codex CLI logs a `token_count` event with a `rate_limits` object (real
/// server-computed `used_percent`/`window_minutes`/`resets_at` for a
/// "primary" ~5-hour and "secondary" ~7-day window) every time it checks in
/// with OpenAI. This is the same class of source as `provider-claude`'s
/// `~/.claude/projects/**/*.jsonl` parsing — a CLI's own local session log,
/// never its credential store (`~/.codex/auth.json` is never opened; see
/// SECURITY.md). Falls back to the `codex login status` connectivity-only
/// check (this crate's original implementation) when no session log has a
/// rate-limit reading yet.
pub struct CodexPlugin {
    state: BasePluginState,
    sessions_dir: Option<PathBuf>,
}

impl Default for CodexPlugin {
    fn default() -> Self {
        Self { state: BasePluginState::new("codex", "Codex"), sessions_dir: dirs::home_dir().map(|home| home.join(".codex").join("sessions")) }
    }
}

impl CodexPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Used by tests to point at a fixture directory instead of the real
    /// `~/.codex/sessions`.
    pub fn with_sessions_dir(sessions_dir: impl Into<PathBuf>) -> Self {
        Self { sessions_dir: Some(sessions_dir.into()), ..Self::default() }
    }
}

#[derive(serde::Deserialize, Clone)]
struct RateLimits {
    limit_id: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

/// A real account can log `token_count` events under more than one
/// `limit_id` — confirmed live: alongside the account-wide `"codex"` bucket,
/// this machine's logs also have `"codex_bengalfox"` (a specific experimental
/// model, tagged with its own `limit_name`) and `"premium"` (no window data
/// at all). These are separate quotas, not alternate readings of the same
/// one — picking "whichever token_count line is newest" without checking
/// this ends up reporting an unrelated bucket's percentage. Hit exactly this
/// live: the newest file on disk had a fresh `codex_bengalfox` reading at
/// 0%/0% (that model simply hadn't been used recently) at the same moment
/// the real `"codex"` bucket — the one ChatGPT's own UI was showing "usage
/// exhausted" for — was sitting at 100%. `"codex"` is the account-wide
/// bucket every session reports (226k of ~345k observed readings on this
/// machine, vs. ~119k for the next most common id), so it's the one that
/// answers "am I rate-limited," not a specific model's separate allowance.
const ACCOUNT_WIDE_LIMIT_ID: &str = "codex";

#[derive(serde::Deserialize, Clone)]
struct RateLimitWindow {
    used_percent: f64,
    window_minutes: i64,
    resets_at: Option<i64>,
}

/// A window's name isn't included in the log line (`limit_name` is always
/// null in practice) — derived from `window_minutes` instead of hardcoding
/// "primary = 5-hour" so this keeps working if OpenAI ever changes the
/// window sizes.
fn window_label(minutes: i64) -> String {
    if minutes > 0 && minutes % (24 * 60) == 0 {
        let days = minutes / (24 * 60);
        if days == 7 { "Weekly".to_string() } else { format!("{days}-day") }
    } else if minutes > 0 && minutes % 60 == 0 {
        format!("{}-hour", minutes / 60)
    } else {
        format!("{minutes}-minute")
    }
}

fn to_limit_window(id: &str, window: &RateLimitWindow) -> LimitWindow {
    LimitWindow {
        id: id.into(),
        label: window_label(window.window_minutes),
        period: "rolling".into(),
        unit: "percent".into(),
        limit: None,
        used: window.used_percent,
        percent_used: Some(window.used_percent),
        resets_at: window.resets_at.and_then(|ts| Utc.timestamp_opt(ts, 0).single()).map(|dt| dt.to_rfc3339()),
        confidence: Confidence::CliLog,
    }
}

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

fn parse_rate_limits(line: &str) -> Option<(DateTime<Utc>, RateLimits)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let timestamp = DateTime::parse_from_rfc3339(value.get("timestamp")?.as_str()?).ok()?.with_timezone(&Utc);
    let rate_limits: RateLimits = serde_json::from_value(payload.get("rate_limits")?.clone()).ok()?;
    // See `ACCOUNT_WIDE_LIMIT_ID` — a token_count line for a different
    // limit_id (a specific model's own separate quota) isn't a reading of
    // the account-wide rate limit at all, so it isn't a candidate here.
    if rate_limits.limit_id.as_deref() != Some(ACCOUNT_WIDE_LIMIT_ID) {
        return None;
    }
    Some((timestamp, rate_limits))
}

/// Scans the most recently modified session files (newest first) for the
/// last `token_count` line in each — `resets_at`/`used_percent` are only
/// meaningful as of the most recent check-in, and files can run to tens of
/// thousands of lines, so this reads from the end rather than the start.
/// Stops at the first file that has any reading at all: file mtime already
/// orders "most recently written to," so that file's own last reading is
/// the freshest available without needing to compare timestamps across files.
fn latest_rate_limits(sessions_dir: &Path) -> Option<(DateTime<Utc>, RateLimits)> {
    let mut files = Vec::new();
    find_jsonl_files(sessions_dir, &mut files);
    files.sort_by_key(|f| std::cmp::Reverse(std::fs::metadata(f).and_then(|m| m.modified()).ok()));

    for file in files.iter().take(8) {
        let Ok(contents) = std::fs::read_to_string(file) else { continue };
        if let Some(reading) = contents.lines().rev().find_map(parse_rate_limits) {
            return Some(reading);
        }
    }
    None
}

/// A reading is only trustworthy for a given window while that window
/// hasn't had a chance to fully roll over since it was taken — Codex only
/// updates `rate_limits` when the CLI actually runs, so a percentage from
/// hours ago for the 5-hour window could be showing a completely different
/// cycle than the one currently active. Verified this matters live: a
/// reading ~22 hours old (over 4x the 5-hour window's own length) was still
/// being shown as if current, which is not a reasonable estimate — for the
/// 7-day secondary window the same 22-hour-old reading is still a
/// reasonable approximation, which is why staleness is checked per-window
/// rather than once for the whole reading.
fn is_fresh(reading_at: DateTime<Utc>, now: DateTime<Utc>, window_minutes: i64) -> bool {
    now.signed_duration_since(reading_at).num_minutes() < window_minutes
}

async fn is_logged_in() -> bool {
    let output = tokio::time::timeout(Duration::from_secs(5), tokio::process::Command::new("codex").arg("login").arg("status").output()).await;
    match output {
        // `codex login status` prints its result to stderr, not stdout —
        // confirmed by capturing both streams directly; check both so this
        // doesn't silently regress if that ever changes.
        Ok(Ok(output)) => {
            let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            text.to_lowercase().contains("logged in")
        }
        _ => false,
    }
}

#[async_trait]
impl ProviderPlugin for CodexPlugin {
    fn id(&self) -> &str {
        "codex"
    }
    fn display_name(&self) -> &str {
        "Codex"
    }
    fn refresh_interval_ms(&self) -> u64 {
        5 * 60_000
    }

    async fn detect(&self) -> bool {
        command_exists_on_path("codex")
    }

    async fn refresh(&mut self) {
        let now = Utc::now();
        let reading = self.sessions_dir.as_deref().and_then(latest_rate_limits);
        let mut limits = Vec::new();
        let mut stale_count = 0;
        if let Some((reading_at, rate_limits)) = &reading {
            if let Some(primary) = &rate_limits.primary {
                if is_fresh(*reading_at, now, primary.window_minutes) {
                    limits.push(to_limit_window("codex:primary", primary));
                } else {
                    stale_count += 1;
                }
            }
            if let Some(secondary) = &rate_limits.secondary {
                if is_fresh(*reading_at, now, secondary.window_minutes) {
                    limits.push(to_limit_window("codex:secondary", secondary));
                } else {
                    stale_count += 1;
                }
            }
        }

        let logged_in = if limits.is_empty() { is_logged_in().await } else { true };

        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        // `observed_at` means "when this reading was produced," not "when
        // we happened to poll" — for a rate limit surviving the freshness
        // check, that's the session log's own timestamp, which the popover
        // surfaces as "updated Xh ago" so a real-but-hours-old percentage
        // (Codex only updates rate_limits when the CLI actually runs)
        // doesn't read as if it were just fetched live.
        if let Some((reading_at, _)) = &reading {
            if !limits.is_empty() {
                status.observed_at = reading_at.to_rfc3339();
            }
        }
        status.state = if logged_in { ConnectionState::Online } else { ConnectionState::Unknown };
        status.detail = Some(if !limits.is_empty() && stale_count > 0 {
            format!(
                "Rate limits from Codex CLI's local session log (as of the last time it was used); {stale_count} window(s) omitted as too stale to trust — see README.md"
            )
        } else if !limits.is_empty() {
            "Rate limits from Codex CLI's local session log (as of the last time it was used) — see README.md".into()
        } else if stale_count > 0 {
            format!("Rate-limit reading found but too stale to trust ({stale_count} window(s) older than their own window length) — see README.md")
        } else if logged_in {
            "Logged in via Codex CLI — no rate-limit reading found in local session logs yet".into()
        } else {
            "codex login status did not report a logged-in session".into()
        });
        status.limits = limits;
        self.state.set_status(status);
    }

    fn get_status(&self) -> ProviderStatus {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn detect_is_false_when_command_is_made_up() {
        // We can't assume `codex` is installed in CI, but we can assert the
        // detection function itself doesn't panic and returns a bool.
        let _ = CodexPlugin::new().detect().await;
    }

    #[test]
    fn window_label_recognizes_five_hour_and_weekly() {
        assert_eq!(window_label(300), "5-hour");
        assert_eq!(window_label(10_080), "Weekly");
        assert_eq!(window_label(1_440), "1-day");
        assert_eq!(window_label(90), "90-minute");
    }

    fn fixture_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agent-status-codex-test-{test_name}-{}", std::process::id()))
    }

    fn token_count_line(timestamp: &str, primary_percent: f64, secondary_percent: f64) -> String {
        token_count_line_for(timestamp, "codex", primary_percent, secondary_percent)
    }

    fn token_count_line_for(timestamp: &str, limit_id: &str, primary_percent: f64, secondary_percent: f64) -> String {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "rate_limits": {
                    "limit_id": limit_id,
                    "primary": {"used_percent": primary_percent, "window_minutes": 300, "resets_at": 1783230674},
                    "secondary": {"used_percent": secondary_percent, "window_minutes": 10080, "resets_at": 1783578987},
                    "plan_type": "pro"
                }
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn refresh_reports_real_percentages_from_the_latest_session_log() {
        let dir = fixture_dir("happy-path");
        let session_dir = dir.join("2026/07/02");
        fs::create_dir_all(&session_dir).unwrap();
        let now = Utc::now();
        let ten_minutes_ago = (now - chrono::Duration::minutes(10)).to_rfc3339();
        let one_minute_ago = (now - chrono::Duration::minutes(1)).to_rfc3339();
        fs::write(
            session_dir.join("rollout-1.jsonl"),
            [
                "not json".to_string(),
                token_count_line(&ten_minutes_ago, 5.0, 56.0),
                serde_json::json!({"timestamp": one_minute_ago, "type": "event_msg", "payload": {"type": "other_event"}}).to_string(),
                token_count_line(&one_minute_ago, 5.0, 98.0),
            ]
            .join("\n"),
        )
        .unwrap();

        let mut plugin = CodexPlugin::with_sessions_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        assert_eq!(status.limits.len(), 2);
        let primary = status.limits.iter().find(|w| w.id == "codex:primary").unwrap();
        let secondary = status.limits.iter().find(|w| w.id == "codex:secondary").unwrap();
        assert_eq!(primary.label, "5-hour");
        assert_eq!(primary.percent_used, Some(5.0));
        assert_eq!(secondary.label, "Weekly");
        // The last token_count line in the file wins, not the first.
        assert_eq!(secondary.percent_used, Some(98.0));
        assert!(secondary.resets_at.is_some());
        assert_eq!(primary.confidence, Confidence::CliLog);
        // observed_at should be the reading's own timestamp (one_minute_ago),
        // not "whenever refresh() happened to run" — otherwise a hours-old
        // percentage would misleadingly claim to be from just now.
        assert_eq!(status.observed_at, one_minute_ago);

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_skips_a_more_recently_modified_file_reporting_a_different_limit_id() {
        // Real bug, hit live: a session file can be the most recently
        // modified on disk while its last token_count reading is for a
        // *different* limit_id (e.g. a specific model's own separate quota)
        // than the account-wide "codex" bucket — reported live as
        // codex_bengalfox at 0%/0% while the real "codex" bucket (which
        // ChatGPT's own UI was showing as exhausted) sat at 100%/28% in an
        // older, less-recently-touched file. Picking "whichever file was
        // modified last" without checking limit_id silently showed the
        // wrong quota's numbers.
        let dir = fixture_dir("mixed-limit-ids");
        fs::create_dir_all(&dir).unwrap();
        let real_reading_time = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        fs::write(dir.join("older-but-real.jsonl"), token_count_line_for(&real_reading_time, "codex", 100.0, 28.0)).unwrap();

        // Force a distinct, later mtime on the second file regardless of
        // filesystem timestamp resolution.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let other_model_reading_time = (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        fs::write(dir.join("newer-other-model.jsonl"), token_count_line_for(&other_model_reading_time, "codex_bengalfox", 0.0, 0.0)).unwrap();

        let mut plugin = CodexPlugin::with_sessions_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        let primary = status.limits.iter().find(|w| w.id == "codex:primary").unwrap();
        let secondary = status.limits.iter().find(|w| w.id == "codex:secondary").unwrap();
        assert_eq!(primary.percent_used, Some(100.0), "must report the account-wide codex bucket, not codex_bengalfox's 0%");
        assert_eq!(secondary.percent_used, Some(28.0));

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_omits_a_window_whose_reading_is_older_than_the_window_itself() {
        // A reading taken 6 hours ago can't be trusted for the 5-hour
        // (300-minute) primary window -- it's definitely rolled over at
        // least once since -- but is still a reasonable estimate for the
        // 7-day secondary window.
        let dir = fixture_dir("stale-primary");
        fs::create_dir_all(&dir).unwrap();
        let six_hours_ago = (Utc::now() - chrono::Duration::hours(6)).to_rfc3339();
        fs::write(dir.join("rollout.jsonl"), token_count_line(&six_hours_ago, 7.0, 40.0)).unwrap();

        let mut plugin = CodexPlugin::with_sessions_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.limits.len(), 1, "only the still-fresh secondary window should survive");
        assert_eq!(status.limits[0].id, "codex:secondary");
        assert!(status.detail.unwrap().contains("stale"));

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_ignores_lines_that_are_not_token_count_events() {
        let dir = fixture_dir("no-token-count");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("rollout.jsonl"),
            serde_json::json!({"timestamp": "2026-07-02T15:19:41Z", "type": "event_msg", "payload": {"type": "session_meta"}}).to_string(),
        )
        .unwrap();

        let mut plugin = CodexPlugin::with_sessions_dir(&dir);
        plugin.refresh().await;
        assert!(plugin.get_status().limits.is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_reports_unknown_when_no_sessions_exist_and_not_logged_in() {
        let dir = fixture_dir("missing");
        let missing = dir.join("does-not-exist");
        let mut plugin = CodexPlugin::with_sessions_dir(missing);
        plugin.refresh().await;
        let status = plugin.get_status();
        assert!(status.limits.is_empty());
        // Can't assert Unknown unconditionally — a real `codex` CLI on this
        // machine could genuinely be logged in, which is a correct Online
        // reading even with no rate-limit data yet.
        assert!(status.detail.is_some());
    }
}
