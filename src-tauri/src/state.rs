use crate::view_model::PopoverViewModel;
use agent_core::PluginRegistry;
use agent_database::Connection;
use agent_notifications::NotificationEngine;
use agent_tray::TrayMode;
use std::sync::Arc;
use tauri::tray::TrayIcon;
use tokio::sync::Mutex;

/// Single shared state guarded by one coarse async mutex. This is a
/// deliberate v1 simplification vs. the original design's "one independent
/// timer per plugin" — refreshes now serialize through this lock rather than
/// running fully concurrently. Acceptable for a handful of lightweight local
/// checks and a couple of short-timeout HTTP calls; revisit with a per-plugin
/// lock (`Vec<Mutex<Box<dyn ProviderPlugin>>>`) if that ever becomes a real
/// latency problem. See docs/architecture.md.
pub struct AppState {
    pub registry: PluginRegistry,
    pub notifications: NotificationEngine,
    pub db: Connection,
    pub tray_mode: TrayMode,
    pub launch_at_login: bool,
    pub latest_view_model: Option<PopoverViewModel>,
    pub tray: Option<TrayIcon>,
}

pub type SharedState = Arc<Mutex<AppState>>;
