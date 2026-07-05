use agent_core::{Confidence, ConnectionState, CostSnapshot, LimitWindow, ProviderPlugin, ProviderStatus};
use agent_plugins::BasePluginState;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
struct KeyResponse {
    data: KeyData,
}

#[derive(Deserialize)]
struct KeyData {
    usage: f64,
    limit: Option<f64>,
}

pub struct OpenRouterPlugin {
    state: BasePluginState,
    client: reqwest::Client,
    api_key: Option<String>,
    api_base: String,
}

impl Default for OpenRouterPlugin {
    fn default() -> Self {
        Self {
            state: BasePluginState::new("openrouter", "OpenRouter"),
            client: reqwest::Client::new(),
            api_key: std::env::var("OPENROUTER_API_KEY").ok(),
            api_base: "https://openrouter.ai/api/v1".to_string(),
        }
    }
}

impl OpenRouterPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Used by tests to point at a mock server instead of the real API.
    pub fn with_api_base_and_key(api_base: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self { api_base: api_base.into(), api_key: Some(api_key.into()), ..Self::default() }
    }

    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        let key = self.api_key.as_ref().ok_or("OPENROUTER_API_KEY is not set")?;
        let response = self
            .client
            .get(format!("{}/auth/key", self.api_base))
            .bearer_auth(key)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("OpenRouter /auth/key returned {}", response.status()));
        }
        let body: KeyResponse = response.json().await.map_err(|e| e.to_string())?;
        let usage = body.data.usage;
        let limit = body.data.limit;

        let mut limits = Vec::new();
        if let Some(limit) = limit {
            limits.push(LimitWindow {
                id: "credit".into(),
                label: "Credit limit".into(),
                period: "fixed".into(),
                unit: "usd".into(),
                limit: Some(limit),
                used: usage,
                percent_used: None,
                resets_at: None,
                confidence: Confidence::OfficialApi,
            });
        }

        let state = if limit.map(|l| usage >= l).unwrap_or(false) { ConnectionState::RateLimited } else { ConnectionState::Online };

        Ok(ProviderStatus {
            provider_id: "openrouter".into(),
            display_name: "OpenRouter".into(),
            state,
            limits,
            models: vec![],
            cost: Some(CostSnapshot {
                currency: "usd".into(),
                today: None,
                this_week: None,
                this_month: Some(usage),
                credits_remaining: limit.map(|l| (l - usage).max(0.0)),
                confidence: Confidence::OfficialApi,
            }),
            observed_at: chrono::Utc::now().to_rfc3339(),
            detail: None,
        })
    }
}

#[async_trait]
impl ProviderPlugin for OpenRouterPlugin {
    fn id(&self) -> &str {
        "openrouter"
    }
    fn display_name(&self) -> &str {
        "OpenRouter"
    }
    fn refresh_interval_ms(&self) -> u64 {
        60_000
    }

    async fn detect(&self) -> bool {
        self.api_key.is_some()
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

    #[tokio::test]
    async fn detect_is_true_only_with_a_key() {
        assert!(OpenRouterPlugin::with_api_base_and_key("http://x", "sk-or-test").detect().await);
        assert!(!OpenRouterPlugin { api_key: None, ..OpenRouterPlugin::default() }.detect().await);
    }

    #[tokio::test]
    async fn maps_usage_and_limit_into_a_credit_window_and_cost_snapshot() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/auth/key"))
            .and(header("Authorization", "Bearer sk-or-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"label": "x", "usage": 4.2, "limit": 20.0, "is_free_tier": false}
            })))
            .mount(&server)
            .await;

        let mut plugin = OpenRouterPlugin::with_api_base_and_key(server.uri(), "sk-or-test");
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        assert_eq!(status.limits.len(), 1);
        assert_eq!(status.limits[0].used, 4.2);
        assert_eq!(status.limits[0].limit, Some(20.0));
        assert_eq!(status.cost.as_ref().unwrap().this_month, Some(4.2));
        assert_eq!(status.cost.as_ref().unwrap().credits_remaining, Some(15.8));
    }

    #[tokio::test]
    async fn reports_no_limit_window_for_an_unlimited_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/auth/key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"label": "x", "usage": 10.0, "limit": null, "is_free_tier": false}
            })))
            .mount(&server)
            .await;

        let mut plugin = OpenRouterPlugin::with_api_base_and_key(server.uri(), "sk-or-test");
        plugin.refresh().await;
        let status = plugin.get_status();
        assert!(status.limits.is_empty());
        assert_eq!(status.cost.unwrap().credits_remaining, None);
    }

    #[tokio::test]
    async fn reports_rate_limited_once_usage_reaches_the_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/auth/key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"label": "x", "usage": 20.0, "limit": 20.0, "is_free_tier": false}
            })))
            .mount(&server)
            .await;

        let mut plugin = OpenRouterPlugin::with_api_base_and_key(server.uri(), "sk-or-test");
        plugin.refresh().await;
        assert_eq!(plugin.get_status().state, ConnectionState::RateLimited);
    }

    #[tokio::test]
    async fn degrades_to_unknown_when_the_api_call_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/auth/key")).respond_with(ResponseTemplate::new(401)).mount(&server).await;

        let mut plugin = OpenRouterPlugin::with_api_base_and_key(server.uri(), "sk-or-test");
        plugin.refresh().await;
        assert_eq!(plugin.get_status().state, ConnectionState::Unknown);
    }
}
