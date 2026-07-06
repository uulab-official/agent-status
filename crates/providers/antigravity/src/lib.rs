use agent_core::{Confidence, ConnectionState, LimitWindow, ProviderPlugin, ProviderStatus};
use agent_plugins::{file_exists, BasePluginState};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Google Antigravity (agentic VS Code-fork IDE). See README.md for the
/// confidence tier and why this doesn't upgrade the way Codex/Cursor did.
///
/// There's no `antigravity` CLI on `$PATH` to shell out to for a sanctioned
/// "am I logged in" check (unlike `codex login status` / `cursor-agent
/// status` / `gh auth token`), and `~/.antigravity_cockpit/credentials.json`
/// is a credential file this crate will not open directly (see SECURITY.md).
/// But `~/.antigravity_cockpit/cache/quota/local/*.json` is a different
/// thing: a non-credential cache Antigravity's own UI maintains for itself
/// (per-model `remainingPercentage`/`resetTime`), the same class of source
/// as Codex's session-log rate limits — read it the same way.
pub struct AntigravityPlugin {
    state: BasePluginState,
    config_dir: Option<PathBuf>,
    quota_cache_dir: Option<PathBuf>,
}

impl Default for AntigravityPlugin {
    fn default() -> Self {
        let home = dirs::home_dir();
        Self {
            state: BasePluginState::new("antigravity", "Antigravity"),
            config_dir: home.as_ref().map(|home| home.join(".antigravity")),
            quota_cache_dir: home.map(|home| home.join(".antigravity_cockpit").join("cache").join("quota").join("local")),
        }
    }
}

impl AntigravityPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Used by tests to point at a fixture directory instead of the real
    /// `~/.antigravity`.
    pub fn with_config_dir(config_dir: impl Into<PathBuf>) -> Self {
        Self { config_dir: Some(config_dir.into()), ..Self::default() }
    }

    /// Used by tests to point at a fixture directory instead of the real
    /// `~/.antigravity_cockpit/cache/quota/local`.
    pub fn with_quota_cache_dir(quota_cache_dir: impl Into<PathBuf>) -> Self {
        Self { quota_cache_dir: Some(quota_cache_dir.into()), ..Self::default() }
    }
}

#[derive(serde::Deserialize, Clone)]
struct QuotaCache {
    #[serde(rename = "updatedAt")]
    updated_at: i64,
    models: Vec<ModelQuota>,
}

#[derive(serde::Deserialize, Clone)]
struct ModelQuota {
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "remainingPercentage")]
    remaining_percentage: f64,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
    #[serde(rename = "isRecommended")]
    is_recommended: Option<bool>,
}

/// The cache holds one entry per model (10+ on a real account) — too many
/// for a single popover row. Reports the one model closest to running out
/// (lowest `remainingPercentage`) among the models Antigravity itself flags
/// `isRecommended` (the ones actually offered for use), falling back to all
/// models if none are flagged, so a single labeled window can stand in for
/// "how close am I to being throttled."
fn worst_model(cache: &QuotaCache) -> Option<&ModelQuota> {
    let recommended: Vec<&ModelQuota> = cache.models.iter().filter(|m| m.is_recommended == Some(true)).collect();
    let pool = if recommended.is_empty() { cache.models.iter().collect() } else { recommended };
    pool.into_iter().min_by(|a, b| a.remaining_percentage.total_cmp(&b.remaining_percentage))
}

fn read_quota_cache(dir: &std::path::Path) -> Option<QuotaCache> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|contents| serde_json::from_str::<QuotaCache>(&contents).ok())
        .max_by_key(|cache| cache.updated_at)
}

/// Antigravity's own resets observed roughly a day apart (see README.md), so
/// a cache reading is treated as current for 24 hours after Antigravity
/// itself wrote it — well past that, it's more likely stale than a real
/// current reading, exactly the failure mode found and fixed for Codex.
const QUOTA_FRESHNESS_HOURS: i64 = 24;

fn is_fresh(updated_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(updated_at).num_hours() < QUOTA_FRESHNESS_HOURS
}

#[async_trait]
impl ProviderPlugin for AntigravityPlugin {
    fn id(&self) -> &str {
        "antigravity"
    }
    fn display_name(&self) -> &str {
        "Antigravity"
    }
    fn refresh_interval_ms(&self) -> u64 {
        5 * 60_000
    }

    async fn detect(&self) -> bool {
        self.config_dir.as_ref().map(|dir| file_exists(dir)).unwrap_or(false)
    }

    async fn refresh(&mut self) {
        let now = Utc::now();
        let cache = self.quota_cache_dir.as_deref().and_then(read_quota_cache);
        let reading = cache.as_ref().and_then(|cache| {
            let updated_at = DateTime::from_timestamp_millis(cache.updated_at)?;
            let model = worst_model(cache)?;
            Some((updated_at, model))
        });

        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        match reading {
            Some((updated_at, _)) if !is_fresh(updated_at, now) => {
                status.state = ConnectionState::Unknown;
                status.detail = Some(format!(
                    "Antigravity's local quota cache is older than {QUOTA_FRESHNESS_HOURS}h (last updated {}), so it's too stale to trust — see README.md",
                    updated_at.to_rfc3339()
                ));
            }
            Some((updated_at, model)) => {
                status.state = ConnectionState::Online;
                status.observed_at = updated_at.to_rfc3339();
                status.limits = vec![LimitWindow {
                    id: "antigravity:quota".into(),
                    label: model.display_name.clone(),
                    period: "rolling".into(),
                    unit: "percent".into(),
                    limit: None,
                    used: 100.0 - model.remaining_percentage,
                    percent_used: Some(100.0 - model.remaining_percentage),
                    resets_at: model.reset_time.clone(),
                    confidence: Confidence::CliLog,
                }];
                status.detail = Some(
                    "From Antigravity's own local quota cache (as of its last check-in); the model closest to its limit among those it currently recommends — see README.md".into(),
                );
            }
            None => {
                status.state = ConnectionState::Unknown;
                status.detail = Some(
                    "Antigravity detected (~/.antigravity exists), but no quota cache found at ~/.antigravity_cockpit/cache/quota/local — see README.md".into(),
                );
            }
        }
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
    async fn detect_is_false_when_the_config_dir_is_missing() {
        let dir = std::env::temp_dir().join(format!("agent-status-antigravity-test-missing-{}", std::process::id()));
        let plugin = AntigravityPlugin::with_config_dir(dir);
        assert!(!plugin.detect().await);
    }

    #[tokio::test]
    async fn detect_is_true_when_the_config_dir_exists() {
        let dir = std::env::temp_dir().join(format!("agent-status-antigravity-test-present-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let plugin = AntigravityPlugin::with_config_dir(&dir);
        assert!(plugin.detect().await);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn fixture_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agent-status-antigravity-test-{test_name}-{}", std::process::id()))
    }

    fn quota_cache_json(updated_at_ms: i64, models: &[(&str, f64, bool)]) -> String {
        let models: Vec<_> = models
            .iter()
            .map(|(name, remaining, recommended)| {
                serde_json::json!({
                    "id": format!("MODEL_{name}"),
                    "displayName": name,
                    "remainingPercentage": remaining,
                    "remainingFraction": remaining / 100.0,
                    "resetTime": "2026-07-07T06:19:04.000Z",
                    "isRecommended": recommended,
                })
            })
            .collect();
        serde_json::json!({
            "version": 1,
            "source": "local",
            "updatedAt": updated_at_ms,
            "isForbidden": false,
            "models": models,
        })
        .to_string()
    }

    #[tokio::test]
    async fn refresh_reports_unknown_when_no_quota_cache_exists() {
        let dir = fixture_dir("missing-cache");
        let mut plugin = AntigravityPlugin::with_quota_cache_dir(dir.join("does-not-exist"));
        plugin.refresh().await;
        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Unknown);
        assert!(status.limits.is_empty());
        assert!(status.detail.unwrap().contains("no quota cache"));
    }

    #[tokio::test]
    async fn refresh_reports_the_recommended_model_closest_to_its_limit() {
        let dir = fixture_dir("happy-path");
        fs::create_dir_all(&dir).unwrap();
        let now_ms = Utc::now().timestamp_millis();
        fs::write(
            dir.join("account.json"),
            quota_cache_json(now_ms, &[("Gemini 3 Pro", 100.0, true), ("Claude Sonnet 4.5", 22.0, true), ("Placeholder", 5.0, false)]),
        )
        .unwrap();

        let mut plugin = AntigravityPlugin::with_quota_cache_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        assert_eq!(status.limits.len(), 1);
        // "Placeholder" is lower but not recommended, so the recommended
        // "Claude Sonnet 4.5" (the real closest-to-limit *usable* model)
        // should win instead.
        assert_eq!(status.limits[0].label, "Claude Sonnet 4.5");
        assert_eq!(status.limits[0].percent_used, Some(78.0));
        assert_eq!(status.limits[0].confidence, Confidence::CliLog);

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_ignores_a_quota_cache_older_than_the_freshness_window() {
        let dir = fixture_dir("stale-cache");
        fs::create_dir_all(&dir).unwrap();
        let two_days_ago_ms = (Utc::now() - chrono::Duration::hours(48)).timestamp_millis();
        fs::write(dir.join("account.json"), quota_cache_json(two_days_ago_ms, &[("Claude Sonnet 4.5", 22.0, true)])).unwrap();

        let mut plugin = AntigravityPlugin::with_quota_cache_dir(&dir);
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Unknown);
        assert!(status.limits.is_empty());
        assert!(status.detail.unwrap().contains("too stale"));

        fs::remove_dir_all(&dir).ok();
    }
}
