use agent_core::ProviderStatus;
use agent_database::{record_cost, record_usage, Connection, CostRecord, UsageRecord};

/// Samples a fresh `ProviderStatus` into `usage_history`/`cost_history`.
/// Called once per successful refresh (see `scheduler.rs`) — this is what
/// will eventually power the Timeline/history view on the roadmap. Errors
/// are logged, not propagated: a failed history write shouldn't take down
/// the refresh loop or block the tray/popover from updating.
pub fn persist(db: &Connection, status: &ProviderStatus) {
    for window in &status.limits {
        let record = UsageRecord {
            provider_id: &status.provider_id,
            window_id: &window.id,
            period: &window.period,
            unit: &window.unit,
            used: window.used,
            limit_value: window.limit,
            confidence: window.confidence as u8,
            observed_at: &status.observed_at,
        };
        if let Err(e) = record_usage(db, &record) {
            eprintln!("failed to record usage history for {}: {e}", status.provider_id);
        }
    }

    if let Some(cost) = &status.cost {
        let entries = [("today", cost.today), ("week", cost.this_week), ("month", cost.this_month)];
        for (period, amount) in entries {
            let Some(amount) = amount else { continue };
            let record = CostRecord {
                provider_id: &status.provider_id,
                currency: &cost.currency,
                amount,
                period,
                confidence: cost.confidence as u8,
                observed_at: &status.observed_at,
            };
            if let Err(e) = record_cost(db, &record) {
                eprintln!("failed to record cost history for {}: {e}", status.provider_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Confidence, ConnectionState, CostSnapshot, LimitWindow};
    use agent_database::{open_database, recent_cost, recent_usage};

    #[test]
    fn persists_limit_windows_and_cost_components() {
        let db = open_database(":memory:").unwrap();
        let status = ProviderStatus {
            provider_id: "openrouter".into(),
            display_name: "OpenRouter".into(),
            state: ConnectionState::Online,
            limits: vec![LimitWindow {
                id: "credit".into(),
                label: "Credit limit".into(),
                period: "fixed".into(),
                unit: "usd".into(),
                limit: Some(20.0),
                used: 4.2,
                percent_used: None,
                resets_at: None,
                confidence: Confidence::OfficialApi,
            }],
            models: vec![],
            cost: Some(CostSnapshot {
                currency: "usd".into(),
                today: None,
                this_week: None,
                this_month: Some(4.2),
                credits_remaining: Some(15.8),
                confidence: Confidence::OfficialApi,
            }),
            observed_at: "2026-01-01T00:00:00Z".into(),
            detail: None,
        };

        persist(&db, &status);

        let usage = recent_usage(&db, "openrouter", 10).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].used, 4.2);

        let cost = recent_cost(&db, "openrouter", 10).unwrap();
        assert_eq!(cost.len(), 1, "only this_month was Some, so exactly one row");
        assert_eq!(cost[0].period, "month");
        assert_eq!(cost[0].amount, 4.2);
    }

    #[test]
    fn skips_cost_entirely_when_none() {
        let db = open_database(":memory:").unwrap();
        let status = ProviderStatus::unknown("ollama", "Ollama");
        persist(&db, &status);
        assert_eq!(recent_usage(&db, "ollama", 10).unwrap().len(), 0);
        assert_eq!(recent_cost(&db, "ollama", 10).unwrap().len(), 0);
    }
}
