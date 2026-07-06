use agent_core::ProviderStatus;
use agent_database::{get_setting, prune_older_than, record_cost, record_usage, set_setting, Connection, CostRecord, UsageRecord};
use chrono::{Duration, Utc};

const RETENTION_DAYS: i64 = 90;
const PRUNE_CHECK_INTERVAL: Duration = Duration::hours(24);
const LAST_PRUNED_AT_KEY: &str = "historyLastPrunedAt";

/// Samples a fresh `ProviderStatus` into `usage_history`/`cost_history`.
/// Called once per successful refresh (see `scheduler.rs`) — this is what
/// will eventually power the Timeline/history view on the roadmap. Errors
/// are logged, not propagated: a failed history write shouldn't take down
/// the refresh loop or block the tray/popover from updating.
pub fn persist(db: &Connection, status: &ProviderStatus) {
    maybe_prune(db);

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

/// Bounds `usage_history`/`cost_history` growth for an app meant to be left
/// running for weeks or months — without this, every refresh adds rows
/// forever. Runs at most once per `PRUNE_CHECK_INTERVAL`, tracked via a
/// setting rather than a timer, so a fresh `persist()` call on every tick
/// (dozens of times an hour) doesn't run a DELETE that often; the check
/// itself is a cheap settings lookup either way.
fn maybe_prune(db: &Connection) {
    let now = Utc::now();
    let last_pruned_at: Option<String> = get_setting(db, LAST_PRUNED_AT_KEY, None);
    if let Some(last_pruned_at) = last_pruned_at.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()) {
        if now.signed_duration_since(last_pruned_at) < PRUNE_CHECK_INTERVAL {
            return;
        }
    }

    let cutoff = (now - Duration::days(RETENTION_DAYS)).to_rfc3339();
    if let Err(e) = prune_older_than(db, &cutoff) {
        eprintln!("failed to prune old history rows: {e}");
        return;
    }
    if let Err(e) = set_setting(db, LAST_PRUNED_AT_KEY, &now.to_rfc3339()) {
        eprintln!("failed to record history prune timestamp: {e}");
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

    fn claude_status(observed_at: &str) -> ProviderStatus {
        ProviderStatus {
            provider_id: "claude".into(),
            display_name: "Claude".into(),
            state: ConnectionState::Online,
            limits: vec![LimitWindow {
                id: "claude:session".into(),
                label: "5-hour".into(),
                period: "session".into(),
                unit: "tokens".into(),
                limit: None,
                used: 100.0,
                percent_used: None,
                resets_at: None,
                confidence: Confidence::CliLog,
            }],
            models: vec![],
            cost: None,
            observed_at: observed_at.into(),
            detail: None,
        }
    }

    #[test]
    fn maybe_prune_removes_rows_older_than_the_retention_window_once_the_gate_is_open() {
        let db = open_database(":memory:").unwrap();
        // This first persist() also runs maybe_prune() (nothing to prune
        // yet) and records historyLastPrunedAt as "now".
        persist(&db, &claude_status("2020-01-01T00:00:00Z"));
        assert_eq!(recent_usage(&db, "claude", 10).unwrap().len(), 1);

        // Force the gate open by backdating the marker past the interval,
        // instead of waiting 24 real hours in a test.
        set_setting(&db, LAST_PRUNED_AT_KEY, &(Utc::now() - PRUNE_CHECK_INTERVAL - Duration::seconds(1)).to_rfc3339()).unwrap();
        maybe_prune(&db);

        assert_eq!(recent_usage(&db, "claude", 10).unwrap().len(), 0, "the 2020 row is well outside the 90-day retention window");
    }

    #[test]
    fn does_not_prune_again_within_the_check_interval() {
        let db = open_database(":memory:").unwrap();
        // persist() already ran maybe_prune() once here, setting
        // historyLastPrunedAt to "now" — well within the interval.
        persist(&db, &claude_status("2020-01-01T00:00:00Z"));
        maybe_prune(&db);
        assert_eq!(recent_usage(&db, "claude", 10).unwrap().len(), 1, "the gate hasn't elapsed yet, so the old row survives");
    }
}
