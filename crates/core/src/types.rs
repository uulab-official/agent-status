//! Agent Status — Standard Status Model
//!
//! Every provider (Claude, GPT, Gemini, Cursor, Copilot, Ollama, OpenRouter, ...)
//! exposes usage/limits/cost through wildly different surfaces: REST APIs, HTML
//! scraping, CLI log parsing, local sockets. Plugins translate whatever they can
//! observe into this shared shape so the tray, popover, and notification engine
//! never need to know a provider exists.

use serde::{Deserialize, Serialize};

/// How a number was obtained. Always attach one — never assume 100% accuracy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Confidence {
    /// Manually entered or estimated by the user.
    UserInput = 1,
    /// Scraped from a logged-in browser session (fragile, breaks on redesigns).
    BrowserScrape = 2,
    /// Parsed from a CLI tool's own logs/state files (e.g. `~/.claude`, `~/.codex`).
    CliLog = 3,
    /// Parsed from an official first-party status/usage screen response.
    OfficialScreen = 4,
    /// Official metering API (e.g. OpenAI usage API, OpenRouter's auth/key endpoint).
    OfficialApi = 5,
}

pub type ProviderId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Responding normally, under any soft limits.
    Online,
    /// Reachable but degraded (queueing, elevated latency).
    Busy,
    /// Hit a hard rate/usage limit; requests are being rejected.
    RateLimited,
    /// Not reachable, not configured, or the user turned it off.
    Offline,
    /// Actively fetching a fresh reading.
    Waiting,
    /// Plugin/provider is mid self-update (token refresh, model list refresh, etc).
    Updating,
    /// Comfortably within budget but a reset/rollover is imminent.
    ResetSoon,
    /// Plugin could not classify the state (e.g. unparseable response).
    Unknown,
}

/// The unit a given limit window is measured in. Kept as a plain string so odd
/// providers aren't forced into a fixed enum (mirrors the TS model's open union).
pub type LimitUnit = String;

/// e.g. "session" (Claude's rolling 5-hour window), "daily", "weekly", "monthly",
/// "fixed" (one-time allotment), or any provider-specific value.
pub type LimitPeriod = String;

/// One quota window. A provider may report several in parallel — Claude reports
/// a 5-hour session limit AND a weekly limit at the same time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitWindow {
    /// Stable key so the UI can diff/animate across refreshes, e.g. "claude:session".
    pub id: String,
    /// Human label for the window, e.g. "5-hour", "Weekly", "Daily tokens".
    pub label: String,
    pub period: LimitPeriod,
    pub unit: LimitUnit,
    /// Total allotment for this window. `None` when the provider has no fixed ceiling.
    pub limit: Option<f64>,
    /// Amount consumed so far in this window.
    pub used: f64,
    /// 0-100. Computed from used/limit if not provided directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_used: Option<f64>,
    /// RFC 3339 timestamp of the next reset/rollover, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSnapshot {
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub this_week: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub this_month: Option<f64>,
    /// Remaining pre-paid credits, if the provider works on a credit balance instead of a bill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_remaining: Option<f64>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNotification {
    pub id: String,
    pub provider_id: ProviderId,
    pub severity: NotificationSeverity,
    /// Short machine-stable reason so the notification engine can dedupe, e.g. "limit_threshold_10".
    pub reason: String,
    pub message: String,
    pub created_at: String,
}

/// The full standardized reading for one provider at one point in time.
/// This is the only shape the tray, popover, and notification engine consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub state: ConnectionState,
    pub limits: Vec<LimitWindow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostSnapshot>,
    /// RFC 3339 timestamp of when this reading was produced.
    pub observed_at: String,
    /// Free-form debug detail surfaced in the popover's "why is this unknown" affordance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ProviderStatus {
    pub fn unknown(provider_id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            display_name: display_name.into(),
            state: ConnectionState::Unknown,
            limits: Vec::new(),
            models: Vec::new(),
            cost: None,
            observed_at: chrono::Utc::now().to_rfc3339(),
            detail: None,
        }
    }
}
