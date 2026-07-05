use agent_core::{AgentNotification, NotificationSeverity};

fn severity_icon(severity: NotificationSeverity) -> &'static str {
    match severity {
        NotificationSeverity::Info => "ℹ️",
        NotificationSeverity::Warning => "⚠️",
        NotificationSeverity::Critical => "🔴",
    }
}

pub struct NativeNotificationContent {
    pub title: String,
    pub body: String,
}

/// Maps a provider-agnostic AgentNotification to the title/body a native
/// notification API expects. Pure so it's testable without a display server.
pub fn to_notification_content(display_name: &str, notification: &AgentNotification) -> NativeNotificationContent {
    NativeNotificationContent {
        title: format!("{} {}", severity_icon(notification.severity), display_name),
        body: notification.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(severity: NotificationSeverity) -> AgentNotification {
        AgentNotification {
            id: "n1".into(),
            provider_id: "claude".into(),
            severity,
            reason: "claude:session:low_10".into(),
            message: "5-hour 10% left.".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn prefixes_the_title_with_a_severity_icon_and_the_provider_display_name() {
        let content = to_notification_content("Claude", &notification(NotificationSeverity::Critical));
        assert_eq!(content.title, "🔴 Claude");
        assert_eq!(content.body, "5-hour 10% left.");
    }

    #[test]
    fn uses_a_distinct_icon_per_severity() {
        let icons: std::collections::HashSet<&'static str> = [NotificationSeverity::Info, NotificationSeverity::Warning, NotificationSeverity::Critical]
            .into_iter()
            .map(severity_icon)
            .collect();
        assert_eq!(icons.len(), 3);
    }
}
