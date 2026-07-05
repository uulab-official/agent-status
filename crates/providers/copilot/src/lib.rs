use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, BasePluginState};
use async_trait::async_trait;

/// GitHub Copilot (premium request quota). See README.md for the confidence
/// tiers `fetch_status` should target once implemented.
pub struct CopilotPlugin {
    state: BasePluginState,
}

impl CopilotPlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("copilot", "GitHub Copilot") }
    }
}

impl Default for CopilotPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderPlugin for CopilotPlugin {
    fn id(&self) -> &str {
        "copilot"
    }
    fn display_name(&self) -> &str {
        "GitHub Copilot"
    }
    fn refresh_interval_ms(&self) -> u64 {
        5 * 60_000
    }

    async fn detect(&self) -> bool {
        let has_token = std::env::var("GITHUB_TOKEN").is_ok() || std::env::var("GH_TOKEN").is_ok();
        has_token || command_exists_on_path("gh")
    }

    async fn refresh(&mut self) {
        // TODO(v1.5): call GET /user/copilot/usage with a token from `gh auth
        // token` or GITHUB_TOKEN (★★★★★). Note: as of this writing that
        // endpoint 404s for individual (non-org) accounts even with a valid
        // token — verify against an org-level Copilot Business/Enterprise
        // seat before assuming this is a parsing bug. See README.md.
        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        status.state = ConnectionState::Unknown;
        status.detail = Some("fetch_status() not yet implemented — see crates/providers/copilot/README.md".into());
        self.state.set_status(status);
    }

    fn get_status(&self) -> ProviderStatus {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    // process::env is global — serialize tests that mutate GITHUB_TOKEN.
    // A tokio (not std) Mutex because the guard must span an `.await`.
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    #[tokio::test]
    async fn detect_is_true_when_github_token_is_set_regardless_of_gh_cli() {
        let _guard = ENV_LOCK.lock().await;
        std::env::set_var("GITHUB_TOKEN", "ghp_test");
        assert!(CopilotPlugin::new().detect().await);
        std::env::remove_var("GITHUB_TOKEN");
    }
}
