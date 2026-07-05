use agent_core::{Confidence, ConnectionState, CostSnapshot, ModelInfo, ProviderPlugin, ProviderStatus};
use agent_plugins::BasePluginState;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

/// A generic plugin for any server that speaks the OpenAI-compatible
/// `GET /v1/models` shape — covers LM Studio, AnythingLLM, Open WebUI, Local
/// AI, and truly custom user-defined endpoints without a plugin per tool.
pub struct CustomPluginConfig {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

pub struct CustomPlugin {
    config: CustomPluginConfig,
    state: BasePluginState,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

impl CustomPlugin {
    pub fn new(config: CustomPluginConfig) -> Self {
        let state = BasePluginState::new(config.id.clone(), config.display_name.clone());
        Self { config, state, client: reqwest::Client::new() }
    }

    fn request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url);
        if let Some(key) = &self.config.api_key {
            req = req.bearer_auth(key);
        }
        req
    }

    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        let url = format!("{}/models", self.config.base_url);
        let response = self.request(&url).timeout(Duration::from_secs(3)).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{url} returned {}", response.status()));
        }
        let body: ModelsResponse = response.json().await.map_err(|e| e.to_string())?;
        let models: Vec<ModelInfo> =
            body.data.into_iter().map(|m| ModelInfo { id: m.id.clone(), label: m.id, is_active: None }).collect();

        Ok(ProviderStatus {
            provider_id: self.config.id.clone(),
            display_name: self.config.display_name.clone(),
            state: ConnectionState::Online,
            limits: vec![],
            models,
            cost: Some(CostSnapshot {
                currency: "usd".into(),
                today: None,
                this_week: None,
                this_month: None,
                credits_remaining: None,
                confidence: Confidence::UserInput,
            }),
            observed_at: chrono::Utc::now().to_rfc3339(),
            detail: None,
        })
    }
}

#[async_trait]
impl ProviderPlugin for CustomPlugin {
    fn id(&self) -> &str {
        &self.config.id
    }
    fn display_name(&self) -> &str {
        &self.config.display_name
    }
    fn refresh_interval_ms(&self) -> u64 {
        30_000
    }

    async fn detect(&self) -> bool {
        let url = format!("{}/models", self.config.base_url);
        match self.request(&url).timeout(Duration::from_millis(1500)).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config(base_url: String, api_key: Option<&str>) -> CustomPluginConfig {
        CustomPluginConfig {
            id: "lmstudio".into(),
            display_name: "LM Studio".into(),
            base_url,
            api_key: api_key.map(String::from),
        }
    }

    #[tokio::test]
    async fn uses_the_configured_id_and_display_name() {
        let plugin = CustomPlugin::new(config("http://x".into(), None));
        assert_eq!(plugin.id(), "lmstudio");
        assert_eq!(plugin.display_name(), "LM Studio");
    }

    #[tokio::test]
    async fn lists_models_from_models_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": [{"id": "llama-3-8b"}]})))
            .mount(&server)
            .await;

        let mut plugin = CustomPlugin::new(config(server.uri(), None));
        plugin.refresh().await;
        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Online);
        assert_eq!(status.models.len(), 1);
        assert_eq!(status.models[0].id, "llama-3-8b");
    }

    #[tokio::test]
    async fn sends_the_bearer_token_when_an_api_key_is_configured() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("Authorization", "Bearer secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .mount(&server)
            .await;

        let plugin = CustomPlugin::new(config(server.uri(), Some("secret")));
        assert!(plugin.detect().await);
    }
}
