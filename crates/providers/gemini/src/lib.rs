use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, BasePluginState};
use async_trait::async_trait;

/// Google Gemini (CLI + web). See README.md for the confidence tiers
/// `fetch_status` should target once implemented.
pub struct GeminiPlugin {
    state: BasePluginState,
}

impl GeminiPlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("gemini", "Gemini") }
    }
}

impl Default for GeminiPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderPlugin for GeminiPlugin {
    fn id(&self) -> &str {
        "gemini"
    }
    fn display_name(&self) -> &str {
        "Gemini"
    }
    fn refresh_interval_ms(&self) -> u64 {
        60_000
    }

    async fn detect(&self) -> bool {
        let has_cli = command_exists_on_path("gemini");
        let has_api_key = std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok();
        has_cli || has_api_key
    }

    async fn refresh(&mut self) {
        // TODO(v1.0): prefer the AI Studio usage endpoint when an API key is
        // set (★★★★★); otherwise parse the Gemini CLI's local rate-limit
        // state (★★★☆☆). See README.md.
        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        status.state = ConnectionState::Unknown;
        status.detail = Some("fetch_status() not yet implemented — see crates/providers/gemini/README.md".into());
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
    async fn refresh_reports_unknown_with_a_todo_detail() {
        let mut plugin = GeminiPlugin::new();
        plugin.refresh().await;
        assert_eq!(plugin.get_status().state, ConnectionState::Unknown);
    }
}
