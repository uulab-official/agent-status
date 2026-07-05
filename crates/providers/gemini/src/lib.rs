use agent_core::{ConnectionState, ModelInfo, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, BasePluginState};
use async_trait::async_trait;
use std::time::Duration;

#[derive(serde::Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    models: Vec<Model>,
}

#[derive(serde::Deserialize)]
struct Model {
    name: String,
    #[serde(default)]
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

/// Google Gemini (CLI + web + API key). See README.md for the confidence
/// tiers `fetch_status` targets — and why there's no `LimitWindow` yet:
/// Google exposes no simple API-key-authenticated usage/quota endpoint
/// (checking quota requires Cloud Billing/Monitoring APIs, which need a
/// full OAuth/service-account flow, out of scope for a bearer-key check).
pub struct GeminiPlugin {
    state: BasePluginState,
    client: reqwest::Client,
    api_key: Option<String>,
    api_base: String,
}

impl Default for GeminiPlugin {
    fn default() -> Self {
        Self {
            state: BasePluginState::new("gemini", "Gemini"),
            client: reqwest::Client::new(),
            api_key: std::env::var("GEMINI_API_KEY").or_else(|_| std::env::var("GOOGLE_API_KEY")).ok(),
            api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }
}

impl GeminiPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Used by tests to point at a mock server instead of the real API.
    pub fn with_api_base_and_key(api_base: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self { api_base: api_base.into(), api_key: Some(api_key.into()), ..Self::default() }
    }

    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        // The CLI-only case (no API key) has no verified sanctioned status
        // command to shell out to yet — Gemini CLI wasn't available to test
        // against in this environment, and shipping an unverified
        // subcommand guess would violate "verify before assuming it works"
        // (docs/plugin-development.md). Left as a documented gap rather
        // than a silent wrong reading.
        let key = self.api_key.as_ref().ok_or("no GEMINI_API_KEY/GOOGLE_API_KEY set — CLI-only status check not yet implemented")?;
        let response = self
            .client
            .get(format!("{}/models", self.api_base))
            .query(&[("key", key.as_str())])
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Gemini /models returned {}", response.status()));
        }
        let body: ModelsResponse = response.json().await.map_err(|e| e.to_string())?;

        let models: Vec<ModelInfo> = body
            .models
            .iter()
            .map(|m| ModelInfo { id: m.name.clone(), label: m.display_name.clone().unwrap_or_else(|| m.name.clone()), is_active: None })
            .collect();

        Ok(ProviderStatus {
            provider_id: self.id().into(),
            display_name: self.display_name().into(),
            state: ConnectionState::Online,
            limits: vec![],
            models,
            cost: None,
            observed_at: chrono::Utc::now().to_rfc3339(),
            detail: Some("API key valid — no queryable usage/quota endpoint available, see README.md".into()),
        })
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
        has_cli || self.api_key.is_some()
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn refresh_reports_unknown_when_no_key_is_configured() {
        let mut plugin = GeminiPlugin { api_key: None, ..GeminiPlugin::default() };
        plugin.refresh().await;
        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Unknown);
        assert!(status.detail.unwrap().contains("CLI-only status check not yet implemented"));
    }

    #[tokio::test]
    async fn maps_the_models_list_when_the_key_is_valid() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(query_param("key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "models/gemini-2.5-pro", "displayName": "Gemini 2.5 Pro"},
                    {"name": "models/gemini-2.5-flash", "displayName": "Gemini 2.5 Flash"}
                ]
            })))
            .mount(&server)
            .await;

        let mut plugin = GeminiPlugin::with_api_base_and_key(server.uri(), "test-key");
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        assert_eq!(status.models.len(), 2);
        assert_eq!(status.models[0].id, "models/gemini-2.5-pro");
        assert_eq!(status.models[0].label, "Gemini 2.5 Pro");
        assert!(status.limits.is_empty());
    }

    #[tokio::test]
    async fn degrades_to_unknown_on_an_invalid_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/models")).respond_with(ResponseTemplate::new(400)).mount(&server).await;

        let mut plugin = GeminiPlugin::with_api_base_and_key(server.uri(), "bad-key");
        plugin.refresh().await;
        assert_eq!(plugin.get_status().state, ConnectionState::Unknown);
    }

    #[tokio::test]
    async fn detect_is_true_with_either_env_var_or_cli() {
        assert!(GeminiPlugin::with_api_base_and_key("http://x", "k").detect().await);
    }
}
