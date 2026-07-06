use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::{file_exists, BasePluginState};
use async_trait::async_trait;
use std::path::PathBuf;

/// Google Antigravity (agentic VS Code-fork IDE). See README.md for why
/// this stays detection-only, unlike Codex/Cursor/Copilot's connectivity
/// upgrade.
///
/// There's no `antigravity` CLI on `$PATH` to shell out to for a sanctioned
/// "am I logged in" check (unlike `codex login status` / `cursor-agent
/// status` / `gh auth token`) — Antigravity's only auth state lives in
/// `~/.antigravity_cockpit/credentials.json`, a credential file this crate
/// will not open directly (see SECURITY.md and the Claude/Cursor entries in
/// ROADMAP.md for the same line drawn elsewhere in this codebase). Without
/// a sanctioned status source, `detect()` finding the config directory
/// doesn't imply a logged-in session, so `fetch_status()` reports `Unknown`
/// rather than guessing `Online`.
pub struct AntigravityPlugin {
    state: BasePluginState,
    config_dir: Option<PathBuf>,
}

impl Default for AntigravityPlugin {
    fn default() -> Self {
        Self {
            state: BasePluginState::new("antigravity", "Antigravity"),
            config_dir: dirs::home_dir().map(|home| home.join(".antigravity")),
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
        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        status.state = ConnectionState::Unknown;
        status.detail = Some(
            "Antigravity detected (~/.antigravity exists), but there's no CLI to check login/usage status without opening its credential file directly — see crates/providers/antigravity/README.md".into(),
        );
        self.state.set_status(status);
    }

    fn get_status(&self) -> ProviderStatus {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn refresh_reports_unknown_with_a_clear_reason() {
        let mut plugin = AntigravityPlugin::new();
        plugin.refresh().await;
        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Unknown);
        assert!(status.detail.unwrap().contains("no CLI"));
    }
}
