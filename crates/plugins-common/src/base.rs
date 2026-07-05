use agent_core::{AgentNotification, ConnectionState, ProviderStatus};
use std::fmt::Display;

/// Shared scaffolding embedded by every provider struct. Rust has no class
/// inheritance, so this isn't a base *class* the way `BasePlugin` was in the
/// TypeScript version — it's a small state holder each provider's `struct`
/// composes, pairing with a couple of helper methods so the "catch errors,
/// degrade to Unknown, cache the last good reading" behavior isn't
/// reimplemented per provider. See docs/plugin-development.md.
pub struct BasePluginState {
    status: ProviderStatus,
    pending_notifications: Vec<AgentNotification>,
}

impl BasePluginState {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            status: ProviderStatus::unknown(id, display_name),
            pending_notifications: Vec::new(),
        }
    }

    pub fn status(&self) -> ProviderStatus {
        self.status.clone()
    }

    pub fn set_status(&mut self, status: ProviderStatus) {
        self.status = status;
    }

    /// Call from `refresh()`'s error branch: degrades to `Unknown` with the
    /// error recorded in `detail`, without discarding the provider's id/name.
    pub fn set_error(&mut self, error: impl Display) {
        self.status.state = ConnectionState::Unknown;
        self.status.observed_at = chrono::Utc::now().to_rfc3339();
        self.status.detail = Some(error.to_string());
    }

    pub fn queue_notification(&mut self, notification: AgentNotification) {
        self.pending_notifications.push(notification);
    }

    pub fn drain_notifications(&mut self) -> Vec<AgentNotification> {
        std::mem::take(&mut self.pending_notifications)
    }
}
