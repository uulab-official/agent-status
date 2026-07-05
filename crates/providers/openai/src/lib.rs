use agent_core::{Confidence, ConnectionState, CostSnapshot, ProviderPlugin, ProviderStatus};
use agent_plugins::BasePluginState;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use std::time::Duration as StdDuration;

#[derive(serde::Deserialize)]
struct CostsPage {
    #[serde(default)]
    data: Vec<CostBucket>,
}

#[derive(serde::Deserialize)]
struct CostBucket {
    #[serde(default)]
    results: Vec<CostResult>,
}

#[derive(serde::Deserialize)]
struct CostResult {
    amount: CostAmount,
}

#[derive(serde::Deserialize)]
struct CostAmount {
    value: f64,
}

fn sum_costs(page: &CostsPage) -> f64 {
    page.data.iter().flat_map(|bucket| bucket.results.iter()).map(|r| r.amount.value).sum()
}

fn start_of_today_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc()
}

fn start_of_month_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0).unwrap()
}

/// OpenAI (platform API cost tracking via the Admin Costs API). See
/// README.md for why this needs an *admin* key, not a regular one, and for
/// the ChatGPT-plan-caps gap this doesn't cover.
pub struct OpenAiPlugin {
    state: BasePluginState,
    client: reqwest::Client,
    admin_key: Option<String>,
    api_base: String,
}

impl Default for OpenAiPlugin {
    fn default() -> Self {
        Self {
            state: BasePluginState::new("openai", "ChatGPT"),
            client: reqwest::Client::new(),
            admin_key: std::env::var("OPENAI_ADMIN_KEY").ok(),
            api_base: "https://api.openai.com/v1".to_string(),
        }
    }
}

impl OpenAiPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Used by tests to point at a mock server instead of the real API,
    /// and by callers avoiding a global env var mutation.
    pub fn with_api_base_and_key(api_base: impl Into<String>, admin_key: impl Into<String>) -> Self {
        Self { api_base: api_base.into(), admin_key: Some(admin_key.into()), ..Self::default() }
    }

    async fn costs_since(&self, key: &str, start_time: DateTime<Utc>) -> Result<f64, String> {
        let response = self
            .client
            .get(format!("{}/organization/costs", self.api_base))
            .bearer_auth(key)
            .query(&[("start_time", start_time.timestamp().to_string()), ("bucket_width", "1d".to_string()), ("limit", "31".to_string())])
            .timeout(StdDuration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("OpenAI /organization/costs returned {}", response.status()));
        }
        let page: CostsPage = response.json().await.map_err(|e| e.to_string())?;
        Ok(sum_costs(&page))
    }

    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        let key = self.admin_key.as_ref().ok_or("OPENAI_ADMIN_KEY is not set")?;
        let now = Utc::now();
        let (today_result, month_result) =
            tokio::join!(self.costs_since(key, start_of_today_utc(now)), self.costs_since(key, start_of_month_utc(now)));
        let today = today_result?;
        let this_month = month_result?;

        Ok(ProviderStatus {
            provider_id: self.id().into(),
            display_name: self.display_name().into(),
            state: ConnectionState::Online,
            limits: vec![],
            models: vec![],
            cost: Some(CostSnapshot {
                currency: "usd".into(),
                today: Some(today),
                this_week: None,
                this_month: Some(this_month),
                credits_remaining: None,
                confidence: Confidence::OfficialApi,
            }),
            observed_at: now.to_rfc3339(),
            detail: None,
        })
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
        self.admin_key.is_some()
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
    use tokio::sync::Mutex;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // process::env is global — serialize tests that mutate OPENAI_ADMIN_KEY.
    // A tokio (not std) Mutex because the guard must span an `.await`.
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    fn costs_body(total: f64) -> serde_json::Value {
        serde_json::json!({
            "object": "page",
            "data": [{
                "object": "bucket",
                "results": [{"object": "organization.costs.result", "amount": {"value": total, "currency": "usd"}, "line_item": null, "project_id": null}]
            }],
            "has_more": false,
            "next_page": null,
        })
    }

    #[tokio::test]
    async fn detect_is_true_when_admin_key_is_set() {
        let _guard = ENV_LOCK.lock().await;
        std::env::set_var("OPENAI_ADMIN_KEY", "sk-admin-test");
        assert!(OpenAiPlugin::new().detect().await);
        std::env::remove_var("OPENAI_ADMIN_KEY");
    }

    #[tokio::test]
    async fn detect_is_false_when_no_admin_key_is_configured() {
        let _guard = ENV_LOCK.lock().await;
        std::env::remove_var("OPENAI_ADMIN_KEY");
        assert!(!OpenAiPlugin::new().detect().await);
    }

    #[tokio::test]
    async fn maps_daily_and_monthly_cost_buckets_into_a_cost_snapshot() {
        let server = MockServer::start().await;
        let today_start = start_of_today_utc(Utc::now()).timestamp().to_string();
        let month_start = start_of_month_utc(Utc::now()).timestamp().to_string();

        Mock::given(method("GET"))
            .and(path("/organization/costs"))
            .and(header("Authorization", "Bearer sk-admin-test"))
            .and(query_param("start_time", today_start.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(costs_body(1.23)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/organization/costs"))
            .and(query_param("start_time", month_start.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(costs_body(45.67)))
            .mount(&server)
            .await;

        let mut plugin = OpenAiPlugin::with_api_base_and_key(server.uri(), "sk-admin-test");
        plugin.refresh().await;
        let status = plugin.get_status();

        assert_eq!(status.state, ConnectionState::Online);
        let cost = status.cost.expect("cost snapshot");
        assert_eq!(cost.today, Some(1.23));
        assert_eq!(cost.this_month, Some(45.67));
        assert_eq!(cost.confidence, Confidence::OfficialApi);
        assert!(status.limits.is_empty());
    }

    #[tokio::test]
    async fn sums_multiple_buckets_and_results_within_a_single_page() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "object": "page",
            "data": [
                {"object": "bucket", "results": [{"object": "organization.costs.result", "amount": {"value": 1.0, "currency": "usd"}}]},
                {"object": "bucket", "results": [
                    {"object": "organization.costs.result", "amount": {"value": 2.5, "currency": "usd"}},
                    {"object": "organization.costs.result", "amount": {"value": 0.5, "currency": "usd"}}
                ]}
            ],
            "has_more": false,
        });
        Mock::given(method("GET")).and(path("/organization/costs")).respond_with(ResponseTemplate::new(200).set_body_json(body)).mount(&server).await;

        let plugin = OpenAiPlugin::with_api_base_and_key(server.uri(), "sk-admin-test");
        let total = plugin.costs_since("sk-admin-test", Utc::now()).await.unwrap();
        assert_eq!(total, 4.0);
    }

    #[tokio::test]
    async fn degrades_to_unknown_when_the_api_call_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/organization/costs")).respond_with(ResponseTemplate::new(401)).mount(&server).await;

        let mut plugin = OpenAiPlugin::with_api_base_and_key(server.uri(), "sk-admin-test");
        plugin.refresh().await;
        let status = plugin.get_status();
        assert_eq!(status.state, ConnectionState::Unknown);
        assert!(status.detail.unwrap().contains("401"));
    }

    #[test]
    fn start_of_month_is_the_first_day_at_midnight_utc() {
        let now = Utc.with_ymd_and_hms(2026, 3, 17, 14, 30, 0).unwrap();
        let start = start_of_month_utc(now);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap());
    }

    #[test]
    fn start_of_today_zeroes_out_the_time_of_day() {
        let now = Utc.with_ymd_and_hms(2026, 3, 17, 14, 30, 0).unwrap();
        let start = start_of_today_utc(now);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 3, 17, 0, 0, 0).unwrap());
    }
}
