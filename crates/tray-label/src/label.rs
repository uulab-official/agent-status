use agent_core::ProviderStatus;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMode {
    Minimal,
    Compact,
    Detailed,
}

impl TrayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrayMode::Minimal => "minimal",
            TrayMode::Compact => "compact",
            TrayMode::Detailed => "detailed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "minimal" => Some(TrayMode::Minimal),
            "compact" => Some(TrayMode::Compact),
            "detailed" => Some(TrayMode::Detailed),
            _ => None,
        }
    }
}

fn known_initials() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("claude", "C"),
        ("openai", "G"), // GPT
        ("codex", "X"),
        ("gemini", "M"), // avoid colliding with GPT's "G"
        ("cursor", "U"),
        ("copilot", "P"),
        ("ollama", "O"),
        ("openrouter", "R"),
    ])
}

fn initial_for(provider_id: &str) -> String {
    if let Some(initial) = known_initials().get(provider_id) {
        return initial.to_string();
    }
    provider_id.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
}

/// A window with neither `percent_used` nor `limit` (e.g. Claude's token
/// counts, where the plan's numeric cap isn't observable) has no percentage
/// to report — treating it as 0% would render as if there were no usage at
/// all, which is actively misleading. Such windows are excluded from the
/// tray label entirely rather than shown as a false zero.
fn has_known_limit(window: &agent_core::LimitWindow) -> bool {
    window.percent_used.is_some() || window.limit.is_some()
}

fn percent_used(window: &agent_core::LimitWindow) -> i64 {
    if let Some(p) = window.percent_used {
        return p.round() as i64;
    }
    match window.limit {
        Some(limit) if limit > 0.0 => (window.used / limit * 100.0).clamp(0.0, 100.0).round() as i64,
        _ => 0,
    }
}

/// The single worst (highest-usage) limit window across all reported providers, or `None` if none report limits.
fn worst_percent(statuses: &[ProviderStatus]) -> Option<i64> {
    statuses.iter().flat_map(|s| s.limits.iter()).filter(|w| has_known_limit(w)).map(percent_used).max()
}

/// Renders the menu-bar text per the user's chosen mode:
/// - minimal: just the icon, no text
/// - compact: icon + the single worst usage percentage across all providers
/// - detailed: icon + per-provider initial and its top usage percentage
pub fn format_tray_label(statuses: &[ProviderStatus], mode: TrayMode) -> String {
    const ICON: &str = "🤖";
    match mode {
        TrayMode::Minimal => ICON.to_string(),
        TrayMode::Compact => match worst_percent(statuses) {
            Some(worst) => format!("{ICON} {worst}%"),
            None => ICON.to_string(),
        },
        TrayMode::Detailed => {
            let parts: Vec<String> = statuses
                .iter()
                .filter_map(|s| {
                    let top = s.limits.iter().filter(|w| has_known_limit(w)).map(percent_used).max()?;
                    Some(format!("{}{}", initial_for(&s.provider_id), top))
                })
                .collect();
            if parts.is_empty() {
                ICON.to_string()
            } else {
                format!("{ICON} {}", parts.join(" "))
            }
        }
    }
}
