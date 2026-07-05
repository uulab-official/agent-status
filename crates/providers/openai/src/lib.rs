use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::BasePluginState;
use async_trait::async_trait;

/// OpenAI (ChatGPT + platform API). See README.md for the confidence tiers
/// `fetch_status` should target once implemented.
pub struct OpenAiPlugin {
    state: BasePluginState,
}

impl OpenAiPlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("openai", "ChatGPT") }
    }
}

impl Default for OpenAiPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderPlugin for OpenAiPlugin {
    fn id(&self) -> &str {
        "openai"
    }
    fn display_name(&self) -> &str {
        "ChatGPT"
    }
    fn refresh_interval_ms(&self) -> u64 {
        60_000
    }

    async fn detect(&self) -> bool {
        std::env::var("OPENAI_API_KEY").is_ok()
    }

    async fn refresh(&mut self) {
        // TODO(v1.0): call GET /v1/usage with OPENAI_API_KEY for ★★★★★ cost
        // data, and fall back to scraping chat.openai.com's usage panel for
        // ChatGPT plan message caps (★★☆☆☆). See README.md.
        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        status.state = ConnectionState::Unknown;
        status.detail = Some("fetch_status() not yet implemented — see crates/providers/openai/README.md".into());
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

    // process::env is global — serialize tests that mutate OPENAI_API_KEY.
    // A tokio (not std) Mutex because the guard must span an `.await`.
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    #[tokio::test]
    async fn detect_is_true_when_api_key_is_set() {
        let _guard = ENV_LOCK.lock().await;
        std::env::set_var("OPENAI_API_KEY", "sk-test");
        assert!(OpenAiPlugin::new().detect().await);
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[tokio::test]
    async fn detect_is_false_when_no_key_is_configured() {
        let _guard = ENV_LOCK.lock().await;
        std::env::remove_var("OPENAI_API_KEY");
        assert!(!OpenAiPlugin::new().detect().await);
    }
}
