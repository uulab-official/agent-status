mod label;

pub use label::{format_tray_label, TrayMode};

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Confidence, ConnectionState, LimitWindow, ProviderStatus};

    fn status(provider_id: &str, used: f64, limit: f64) -> ProviderStatus {
        ProviderStatus {
            provider_id: provider_id.into(),
            display_name: provider_id.into(),
            state: ConnectionState::Online,
            observed_at: chrono::Utc::now().to_rfc3339(),
            limits: vec![LimitWindow {
                id: "w".into(),
                label: "w".into(),
                period: "daily".into(),
                unit: "messages".into(),
                limit: Some(limit),
                used,
                percent_used: None,
                resets_at: None,
                confidence: Confidence::OfficialApi,
            }],
            models: vec![],
            cost: None,
            detail: None,
        }
    }

    #[test]
    fn minimal_shows_only_the_icon() {
        assert_eq!(format_tray_label(&[status("claude", 82.0, 100.0)], TrayMode::Minimal), "🤖");
    }

    #[test]
    fn compact_shows_the_single_worst_percentage() {
        let label = format_tray_label(&[status("claude", 40.0, 100.0), status("openai", 72.0, 100.0)], TrayMode::Compact);
        assert_eq!(label, "🤖 72%");
    }

    #[test]
    fn detailed_shows_per_provider_initials() {
        let label = format_tray_label(
            &[status("claude", 82.0, 100.0), status("openai", 41.0, 100.0), status("ollama", 99.0, 100.0)],
            TrayMode::Detailed,
        );
        assert_eq!(label, "🤖 C82 G41 O99");
    }

    #[test]
    fn falls_back_to_just_the_icon_with_no_reporting_providers() {
        assert_eq!(format_tray_label(&[], TrayMode::Compact), "🤖");
    }
}
