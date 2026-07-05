use agent_core::{ConnectionState, ProviderStatus};
use agent_tray::TrayMode;
use serde::Serialize;

fn format_duration_ms(ms: i64) -> String {
    if ms <= 0 {
        return "now".to_string();
    }
    let total_minutes = ms / 60_000;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours == 0 && minutes == 0 {
        "<1m".to_string()
    } else if hours == 0 {
        format!("{minutes}m")
    } else {
        format!("{hours}h {minutes}m")
    }
}

fn percent_used(window: &agent_core::LimitWindow) -> f64 {
    if let Some(p) = window.percent_used {
        return p;
    }
    match window.limit {
        Some(limit) if limit > 0.0 => (window.used / limit * 100.0).clamp(0.0, 100.0).round(),
        _ => 0.0,
    }
}

fn has_known_limit(window: &agent_core::LimitWindow) -> bool {
    window.percent_used.is_some() || window.limit.is_some()
}

/// Abbreviates large counts (Claude's token totals routinely run into the
/// millions) so the popover doesn't wrap a 10-digit number onto two lines.
fn format_count(value: f64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        format!("{:.1}B", value / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}

/// What to print next to the label. Windows with no known cap (e.g. Claude's
/// token counts, where Anthropic doesn't expose the plan's numeric ceiling)
/// used to render as a permanently-empty 0% bar with no indication that any
/// usage was even observed — show the raw count instead of hiding it.
fn format_value_text(window: &agent_core::LimitWindow) -> String {
    match window.limit {
        Some(limit) => format!("{} / {} {}", format_count(window.used), format_count(limit), window.unit),
        None => format!("{} {}", format_count(window.used), window.unit),
    }
}

fn state_indicator(state: ConnectionState) -> &'static str {
    match state {
        ConnectionState::Online => "🟢",
        ConnectionState::Busy => "🟡",
        ConnectionState::RateLimited => "🔴",
        ConnectionState::Offline => "⚫",
        ConnectionState::Waiting => "🔵",
        ConnectionState::Updating => "🟣",
        ConnectionState::ResetSoon => "🟠",
        ConnectionState::Unknown => "🟤",
    }
}

fn progress_tone(percent: f64) -> &'static str {
    if percent >= 90.0 {
        "critical"
    } else if percent >= 70.0 {
        "warning"
    } else {
        "ok"
    }
}

fn state_priority(state: ConnectionState) -> u8 {
    match state {
        ConnectionState::RateLimited => 0,
        ConnectionState::ResetSoon => 1,
        ConnectionState::Busy => 2,
        ConnectionState::Waiting | ConnectionState::Updating => 3,
        ConnectionState::Online => 4,
        ConnectionState::Unknown => 5,
        ConnectionState::Offline => 6,
    }
}

fn worst_usage(status: &ProviderStatus) -> f64 {
    status.limits.iter().map(percent_used).fold(0.0, f64::max)
}

/// Rate-limited/reset-soon providers surface first (they need attention),
/// then by descending worst-window usage, then alphabetically for a stable
/// tiebreak.
fn sort_providers_by_attention(mut statuses: Vec<ProviderStatus>) -> Vec<ProviderStatus> {
    statuses.sort_by(|a, b| {
        state_priority(a.state)
            .cmp(&state_priority(b.state))
            .then_with(|| worst_usage(b).partial_cmp(&worst_usage(a)).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
    statuses
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitRowViewModel {
    pub id: String,
    pub label: String,
    pub has_limit: bool,
    pub percent: i64,
    pub tone: &'static str,
    pub value_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_text: Option<String>,
    pub confidence_stars: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRowViewModel {
    pub id: String,
    pub display_name: String,
    pub indicator: &'static str,
    pub state: String,
    pub limits: Vec<LimitRowViewModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsViewModel {
    pub tray_mode: &'static str,
    pub launch_at_login: bool,
}

impl SettingsViewModel {
    pub fn new(tray_mode: TrayMode, launch_at_login: bool) -> Self {
        Self { tray_mode: tray_mode.as_str(), launch_at_login }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverViewModel {
    pub providers: Vec<ProviderRowViewModel>,
    pub settings: SettingsViewModel,
    pub generated_at: String,
}

fn format_cost_text(cost: &agent_core::CostSnapshot) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(today) = cost.today {
        parts.push(format!("Today ${today:.2}"));
    }
    if let Some(this_month) = cost.this_month {
        parts.push(format!("This month ${this_month:.2}"));
    }
    if let Some(credits) = cost.credits_remaining {
        parts.push(format!("${credits:.2} credits left"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Everything the popover needs, pre-formatted. The frontend has no access to
/// Rust presentation logic — it just renders whatever JSON this produces.
pub fn build_popover_view_model(statuses: Vec<ProviderStatus>, settings: SettingsViewModel) -> PopoverViewModel {
    let sorted = sort_providers_by_attention(statuses);

    let providers = sorted
        .into_iter()
        // A provider stuck at `Unknown` has nothing to show (no limits, no
        // cost, often just a "not implemented yet" detail string) — that's
        // an internal debugging state, not something a user should see
        // filling up the popover. It's still detected and persisted to
        // history as normal; only the display is filtered.
        .filter(|status| status.state != ConnectionState::Unknown)
        .map(|status| {
            let limits = status
                .limits
                .iter()
                .map(|window| {
                    let percent = percent_used(window);
                    LimitRowViewModel {
                        id: window.id.clone(),
                        label: window.label.clone(),
                        has_limit: has_known_limit(window),
                        percent: percent.round() as i64,
                        tone: progress_tone(percent),
                        value_text: format_value_text(window),
                        reset_text: window.resets_at.as_ref().and_then(|resets_at| {
                            chrono::DateTime::parse_from_rfc3339(resets_at).ok().map(|resets_at| {
                                let remaining = (resets_at.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_milliseconds();
                                format!("Resets in {}", format_duration_ms(remaining))
                            })
                        }),
                        confidence_stars: window.confidence as u8,
                    }
                })
                .collect();

            ProviderRowViewModel {
                id: status.provider_id.clone(),
                display_name: status.display_name.clone(),
                indicator: state_indicator(status.state),
                state: format!("{:?}", status.state).to_lowercase(),
                limits,
                cost_text: status.cost.as_ref().and_then(format_cost_text),
                detail: status.detail.clone(),
            }
        })
        .collect();

    PopoverViewModel { providers, settings, generated_at: chrono::Utc::now().to_rfc3339() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Confidence, LimitWindow};

    fn status(overrides: impl FnOnce(&mut ProviderStatus)) -> ProviderStatus {
        let mut status = ProviderStatus::unknown("claude", "Claude");
        status.state = ConnectionState::Online;
        overrides(&mut status);
        status
    }

    fn settings() -> SettingsViewModel {
        SettingsViewModel::new(TrayMode::Compact, false)
    }

    #[test]
    fn formats_limit_windows_with_percent_tone_and_reset_text() {
        let resets_at = (chrono::Utc::now() + chrono::Duration::minutes(90)).to_rfc3339();
        let vm = build_popover_view_model(
            vec![status(|s| {
                s.limits = vec![LimitWindow {
                    id: "session".into(),
                    label: "5-hour".into(),
                    period: "session".into(),
                    unit: "messages".into(),
                    limit: Some(100.0),
                    used: 92.0,
                    percent_used: None,
                    resets_at: Some(resets_at),
                    confidence: Confidence::OfficialApi,
                }];
            })],
            settings(),
        );
        assert_eq!(vm.providers.len(), 1);
        let row = &vm.providers[0];
        assert_eq!(row.limits[0].id, "session");
        assert_eq!(row.limits[0].label, "5-hour");
        assert!(row.limits[0].has_limit);
        assert_eq!(row.limits[0].percent, 92);
        assert_eq!(row.limits[0].tone, "critical");
        assert_eq!(row.limits[0].value_text, "92 / 100 messages");
        assert!(row.limits[0].reset_text.as_ref().unwrap().starts_with("Resets in 1h"));
    }

    #[test]
    fn windows_with_no_known_limit_show_a_raw_count_instead_of_a_fake_zero_percent() {
        let vm = build_popover_view_model(
            vec![status(|s| {
                s.limits = vec![LimitWindow {
                    id: "claude:session".into(),
                    label: "5-hour".into(),
                    period: "session".into(),
                    unit: "tokens".into(),
                    limit: None,
                    used: 7_148_012.0,
                    percent_used: None,
                    resets_at: None,
                    confidence: Confidence::CliLog,
                }];
            })],
            settings(),
        );
        let row = &vm.providers[0].limits[0];
        assert!(!row.has_limit);
        assert_eq!(row.value_text, "7.1M tokens");
    }

    #[test]
    fn providers_stuck_at_unknown_are_hidden_from_the_popover() {
        let vm = build_popover_view_model(
            vec![
                status(|s| {
                    s.provider_id = "copilot".into();
                    s.state = ConnectionState::Unknown;
                    s.detail = Some("fetch_status() not yet implemented".into());
                }),
                status(|s| {
                    s.provider_id = "claude".into();
                    s.state = ConnectionState::Online;
                }),
            ],
            settings(),
        );
        assert_eq!(vm.providers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["claude"]);
    }

    #[test]
    fn sorts_rate_limited_providers_to_the_top() {
        let vm = build_popover_view_model(
            vec![
                status(|s| {
                    s.provider_id = "a".into();
                    s.display_name = "A".into();
                    s.state = ConnectionState::Online;
                }),
                status(|s| {
                    s.provider_id = "b".into();
                    s.display_name = "B".into();
                    s.state = ConnectionState::RateLimited;
                }),
            ],
            settings(),
        );
        assert_eq!(vm.providers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["b", "a"]);
    }

    #[test]
    fn formats_cost_into_a_single_summary_string() {
        let vm = build_popover_view_model(
            vec![status(|s| {
                s.cost = Some(agent_core::CostSnapshot {
                    currency: "usd".into(),
                    today: None,
                    this_week: None,
                    this_month: Some(12.42),
                    credits_remaining: None,
                    confidence: Confidence::OfficialApi,
                });
            })],
            settings(),
        );
        assert_eq!(vm.providers[0].cost_text.as_deref(), Some("This month $12.42"));
    }

    #[test]
    fn passes_settings_through_unchanged() {
        let vm = build_popover_view_model(vec![status(|_| {})], SettingsViewModel::new(TrayMode::Detailed, true));
        assert_eq!(vm.settings.tray_mode, "detailed");
        assert!(vm.settings.launch_at_login);
    }
}
