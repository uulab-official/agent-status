use agent_core::{ConnectionState, ModelInfo, ProviderPlugin, ProviderStatus};
use agent_plugins::BasePluginState;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

#[derive(Deserialize)]
struct PsResponse {
    #[serde(default)]
    models: Vec<PsModel>,
}

#[derive(Deserialize)]
struct PsModel {
    name: String,
    #[serde(default)]
    size_vram: Option<u64>,
}

/// Reference implementation: the simplest provider with a fully implemented
/// `fetch_status`, because Ollama's local REST API needs no auth, no
/// scraping, and no CLI-log parsing. Read this one first when writing a new
/// plugin — see ROADMAP.md for which other providers also have a real
/// `fetch_status()` and what each one's data source looks like —
/// see docs/plugin-development.md.
pub struct OllamaPlugin {
    state: BasePluginState,
    client: reqwest::Client,
    base_url: String,
}

impl Default for OllamaPlugin {
    fn default() -> Self {
        let base_url = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        Self { state: BasePluginState::new("ollama", "Ollama"), client: reqwest::Client::new(), base_url }
    }
}

impl OllamaPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Used by tests (and anyone pointing this at a non-default Ollama host)
    /// to avoid mutating the process-wide `OLLAMA_HOST` env var, which would
    /// race across parallel test threads.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), ..Self::default() }
    }

    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        let base = &self.base_url;
        let tags_fut = self.client.get(format!("{base}/api/tags")).timeout(Duration::from_secs(3)).send();
        let ps_fut = self.client.get(format!("{base}/api/ps")).timeout(Duration::from_secs(3)).send();
        let (tags_result, ps_result) = tokio::join!(tags_fut, ps_fut);

        let tags_response = tags_result.map_err(|e| e.to_string())?;
        if !tags_response.status().is_success() {
            return Err(format!("Ollama /api/tags returned {}", tags_response.status()));
        }
        let tags: TagsResponse = tags_response.json().await.map_err(|e| e.to_string())?;

        let ps: PsResponse = match ps_result {
            Ok(resp) if resp.status().is_success() => resp.json().await.unwrap_or(PsResponse { models: vec![] }),
            _ => PsResponse { models: vec![] },
        };
        let running_names: Vec<&str> = ps.models.iter().map(|m| m.name.as_str()).collect();

        let models: Vec<ModelInfo> = tags
            .models
            .iter()
            .map(|m| ModelInfo { id: m.name.clone(), label: m.name.clone(), is_active: Some(running_names.contains(&m.name.as_str())) })
            .collect();

        let total_vram: u64 = ps.models.iter().filter_map(|m| m.size_vram).sum();
        let detail = if ps.models.is_empty() {
            "No models currently loaded".to_string()
        } else {
            let names: Vec<&str> = ps.models.iter().map(|m| m.name.as_str()).collect();
            format!("Running: {} ({:.1} GB VRAM)", names.join(", "), total_vram as f64 / 1024f64.powi(3))
        };

        Ok(ProviderStatus {
            provider_id: "ollama".into(),
            display_name: "Ollama".into(),
            state: if ps.models.is_empty() { ConnectionState::Online } else { ConnectionState::Busy },
            limits: vec![],
            models,
            cost: None,
            observed_at: chrono::Utc::now().to_rfc3339(),
            detail: Some(detail),
        })
    }
}

#[async_trait]
impl ProviderPlugin for OllamaPlugin {
    fn id(&self) -> &str {
        "ollama"
    }
    fn display_name(&self) -> &str {
        "Ollama"
    }
    fn refresh_interval_ms(&self) -> u64 {
        15_000
    }

    async fn detect(&self) -> bool {
        let base = &self.base_url;
        match self.client.get(format!("{base}/api/tags")).timeout(Duration::from_millis(1500)).send().await {
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mock_server(tags_status: u16, tags_body: serde_json::Value, ps_body: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(tags_status).set_body_json(tags_body))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/ps"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ps_body))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn detects_when_the_server_responds() {
        let server = mock_server(200, serde_json::json!({"models": []}), serde_json::json!({"models": []})).await;
        assert!(OllamaPlugin::with_base_url(server.uri()).detect().await);
    }

    #[tokio::test]
    async fn fails_detection_when_unreachable() {
        assert!(!OllamaPlugin::with_base_url("http://127.0.0.1:1").detect().await);
    }

    #[tokio::test]
    async fn reports_models_and_marks_running_ones_active() {
        let server = mock_server(
            200,
            serde_json::json!({"models": [{"name": "llama3"}, {"name": "qwen2.5"}]}),
            serde_json::json!({"models": [{"name": "llama3", "size_vram": 4294967296u64}]}),
        )
        .await;
        let mut plugin = OllamaPlugin::with_base_url(server.uri());
        plugin.refresh().await;

        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Busy);
        assert_eq!(status.models.len(), 2);
        assert_eq!(status.models[0].is_active, Some(true));
        assert_eq!(status.models[1].is_active, Some(false));
        assert!(status.detail.unwrap().contains("4.0 GB"));
    }

    #[tokio::test]
    async fn degrades_to_unknown_when_api_tags_errors() {
        let server = mock_server(500, serde_json::json!({}), serde_json::json!({"models": []})).await;
        let mut plugin = OllamaPlugin::with_base_url(server.uri());
        plugin.refresh().await;

        assert_eq!(plugin.get_status().state, ConnectionState::Unknown);
    }
}
