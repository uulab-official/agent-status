use agent_core::{AgentNotification, LimitWindow, NotificationSeverity, ProviderStatus};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct NotificationThresholds {
    /// Fire once when remaining budget crosses below each of these percentages, e.g. [10, 5].
    pub low_remaining_percents: Vec<u32>,
    /// Fire a "resets soon" heads-up when a reset is within this many ms.
    pub reset_soon_within_ms: i64,
    /// Monthly USD budget; fire once when cost.this_month exceeds it.
    pub monthly_budget_usd: Option<f64>,
}

impl Default for NotificationThresholds {
    fn default() -> Self {
        Self {
            low_remaining_percents: vec![10, 5],
            reset_soon_within_ms: 15 * 60_000,
            monthly_budget_usd: None,
        }
    }
}

fn percent_used(window: &LimitWindow) -> f64 {
    if let Some(p) = window.percent_used {
        return p;
    }
    match window.limit {
        Some(limit) if limit > 0.0 => (window.used / limit * 100.0).clamp(0.0, 100.0).round(),
        _ => 0.0,
    }
}

/// Stateful so it can dedupe: a given (provider, reason) pair fires once per
/// reset cycle, not on every poll. Keep one instance alive for the lifetime
/// of the app, not one per refresh.
#[derive(Default)]
pub struct NotificationEngine {
    thresholds: NotificationThresholds,
    fired_reasons: HashSet<String>,
}

impl NotificationEngine {
    pub fn new(thresholds: NotificationThresholds) -> Self {
        Self { thresholds, fired_reasons: HashSet::new() }
    }

    /// Call once per limit window reset so its thresholds can fire again next cycle.
    pub fn clear_for_window(&mut self, provider_id: &str, window_id: &str) {
        let prefix = format!("{provider_id}:{window_id}:");
        self.fired_reasons.retain(|reason| !reason.starts_with(&prefix));
    }

    pub fn evaluate(&mut self, status: &ProviderStatus) -> Vec<AgentNotification> {
        self.evaluate_at(status, chrono::Utc::now())
    }

    pub fn evaluate_at(&mut self, status: &ProviderStatus, now: chrono::DateTime<chrono::Utc>) -> Vec<AgentNotification> {
        let mut notifications = Vec::new();

        for window in &status.limits {
            notifications.extend(self.evaluate_window(status, window, now));
        }

        if let (Some(budget), Some(cost)) = (self.thresholds.monthly_budget_usd, &status.cost) {
            if let Some(this_month) = cost.this_month {
                if this_month > budget {
                    let reason = format!("{}:monthly_budget_exceeded", status.provider_id);
                    if !self.fired_reasons.contains(&reason) {
                        self.fired_reasons.insert(reason.clone());
                        notifications.push(make_notification(
                            status,
                            &reason,
                            NotificationSeverity::Critical,
                            "이번 달 예산을 초과했습니다.".to_string(),
                            now,
                        ));
                    }
                }
            }
        }

        notifications
    }

    fn evaluate_window(
        &mut self,
        status: &ProviderStatus,
        window: &LimitWindow,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Vec<AgentNotification> {
        let mut notifications = Vec::new();
        let used_pct = percent_used(window);
        let remaining_pct = 100.0 - used_pct;

        for &threshold in &self.thresholds.low_remaining_percents {
            let reason = format!("{}:{}:low_{}", status.provider_id, window.id, threshold);
            if remaining_pct <= threshold as f64 && !self.fired_reasons.contains(&reason) {
                self.fired_reasons.insert(reason.clone());
                let severity = if threshold <= 5 { NotificationSeverity::Critical } else { NotificationSeverity::Warning };
                notifications.push(make_notification(
                    status,
                    &reason,
                    severity,
                    format!("{} {}% 남았습니다.", window.label, threshold),
                    now,
                ));
            }
        }

        if let Some(resets_at) = &window.resets_at {
            if let Ok(resets_at) = chrono::DateTime::parse_from_rfc3339(resets_at) {
                let remaining_ms = (resets_at.with_timezone(&chrono::Utc) - now).num_milliseconds();
                let reason = format!("{}:{}:reset_soon", status.provider_id, window.id);
                if remaining_ms > 0 && remaining_ms <= self.thresholds.reset_soon_within_ms && !self.fired_reasons.contains(&reason) {
                    self.fired_reasons.insert(reason.clone());
                    notifications.push(make_notification(
                        status,
                        &reason,
                        NotificationSeverity::Info,
                        format!("{}이(가) 곧 리셋됩니다.", window.label),
                        now,
                    ));
                }
            }
        }

        notifications
    }
}

fn make_notification(
    status: &ProviderStatus,
    reason: &str,
    severity: NotificationSeverity,
    message: String,
    now: chrono::DateTime<chrono::Utc>,
) -> AgentNotification {
    AgentNotification {
        id: format!("{reason}:{}", now.timestamp_millis()),
        provider_id: status.provider_id.clone(),
        severity,
        reason: reason.to_string(),
        message,
        created_at: now.to_rfc3339(),
    }
}
