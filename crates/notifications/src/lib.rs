mod engine;

pub use engine::{NotificationEngine, NotificationThresholds};

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Confidence, ConnectionState, LimitWindow, ProviderStatus};
    use chrono::{Duration, Utc};

    fn status_with_usage(used: f64, resets_at: Option<String>) -> ProviderStatus {
        ProviderStatus {
            provider_id: "claude".into(),
            display_name: "Claude".into(),
            state: ConnectionState::Online,
            observed_at: Utc::now().to_rfc3339(),
            limits: vec![LimitWindow {
                id: "session".into(),
                label: "5-hour".into(),
                period: "session".into(),
                unit: "messages".into(),
                limit: Some(100.0),
                used,
                percent_used: None,
                resets_at,
                confidence: Confidence::OfficialApi,
            }],
            models: vec![],
            cost: None,
            detail: None,
        }
    }

    #[test]
    fn fires_a_low_remaining_warning_once() {
        let mut engine = NotificationEngine::new(NotificationThresholds {
            low_remaining_percents: vec![10],
            reset_soon_within_ms: 0,
            monthly_budget_usd: None,
        });
        let first = engine.evaluate(&status_with_usage(91.0, None));
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].reason, "claude:session:low_10");

        let second = engine.evaluate(&status_with_usage(95.0, None));
        assert_eq!(second.len(), 0);
    }

    #[test]
    fn re_fires_after_usage_recovers_back_past_the_threshold() {
        // No provider actually reports resets_at today (Claude and
        // OpenRouter both leave it None), so a design that only re-armed via
        // an explicit "the window reset" call from the scheduler would never
        // fire twice in the real app. A rolling-window sum going back down
        // past the threshold — which Claude's real 5-hour/weekly sums do —
        // must be enough on its own.
        let mut engine = NotificationEngine::new(NotificationThresholds {
            low_remaining_percents: vec![10],
            reset_soon_within_ms: 0,
            monthly_budget_usd: None,
        });
        engine.evaluate(&status_with_usage(91.0, None));
        engine.evaluate(&status_with_usage(50.0, None)); // usage drops back below the threshold
        let again = engine.evaluate(&status_with_usage(91.0, None));
        assert_eq!(again.len(), 1);
    }

    #[test]
    fn reset_soon_re_fires_after_a_reset_pushes_the_next_one_back_out() {
        let mut engine = NotificationEngine::new(NotificationThresholds {
            low_remaining_percents: vec![],
            reset_soon_within_ms: 60_000,
            monthly_budget_usd: None,
        });
        let soon = (Utc::now() + Duration::seconds(30)).to_rfc3339();
        let first = engine.evaluate(&status_with_usage(10.0, Some(soon)));
        assert_eq!(first.len(), 1);

        // The window actually resets; the provider now reports a resets_at
        // far in the future again.
        let next_cycle = (Utc::now() + Duration::hours(5)).to_rfc3339();
        engine.evaluate(&status_with_usage(0.0, Some(next_cycle)));

        // ...time passes and that next reset is close again.
        let soon_again = (Utc::now() + Duration::seconds(20)).to_rfc3339();
        let second = engine.evaluate(&status_with_usage(10.0, Some(soon_again)));
        assert_eq!(second.len(), 1);
    }

    #[test]
    fn fires_reset_soon_within_the_configured_window() {
        let mut engine = NotificationEngine::new(NotificationThresholds {
            low_remaining_percents: vec![],
            reset_soon_within_ms: 60_000,
            monthly_budget_usd: None,
        });
        let soon = (Utc::now() + Duration::seconds(30)).to_rfc3339();
        let result = engine.evaluate(&status_with_usage(10.0, Some(soon)));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].reason, "claude:session:reset_soon");
    }

    #[test]
    fn does_not_fire_reset_soon_outside_the_window() {
        let mut engine = NotificationEngine::new(NotificationThresholds {
            low_remaining_percents: vec![],
            reset_soon_within_ms: 60_000,
            monthly_budget_usd: None,
        });
        let later = (Utc::now() + Duration::hours(2)).to_rfc3339();
        let result = engine.evaluate(&status_with_usage(10.0, Some(later)));
        assert_eq!(result.len(), 0);
    }
}
