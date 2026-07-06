use crate::notification_bridge::to_notification_content;
use crate::state::SharedState;
use crate::view_model::{build_popover_view_model, PopoverViewModel, SettingsViewModel};
use agent_database::{recent_cost, recent_usage, set_setting, CostHistoryRow, UsageHistoryRow};
use agent_tray::{format_tray_label, TrayMode};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

/// The number of most-recent readings returned per call — enough for a
/// simple sparkline/table once a Timeline view exists (v1.5 on the
/// roadmap), without letting one provider's history response grow unbounded.
const HISTORY_ROW_LIMIT: u32 = 200;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistoryResponse {
    pub usage: Vec<UsageHistoryRow>,
    pub cost: Vec<CostHistoryRow>,
}

/// Recomputes the view model from the registry's current statuses, updates
/// the tray title, and pushes the fresh snapshot to the popover window (if
/// it's listening). Called after every scheduled refresh and after any
/// setting change.
pub async fn render(app: &AppHandle, state: &SharedState) {
    let mut guard = state.lock().await;
    let statuses: Vec<_> = guard.registry.list().iter().map(|p| p.get_status()).collect();
    let tray_mode = guard.tray_mode;
    let settings = SettingsViewModel::new(tray_mode, guard.launch_at_login);

    if let Some(tray) = &guard.tray {
        let _ = tray.set_title(Some(format_tray_label(&statuses, tray_mode)));
    }

    let view_model = build_popover_view_model(statuses, settings);
    guard.latest_view_model = Some(view_model.clone());
    drop(guard);

    let _ = app.emit("status-update", view_model);
}

#[tauri::command]
pub async fn get_view_model(state: State<'_, SharedState>) -> Result<PopoverViewModel, String> {
    let guard = state.lock().await;
    guard.latest_view_model.clone().ok_or_else(|| "not ready yet".to_string())
}

#[tauri::command]
pub async fn set_tray_mode(mode: String, app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    let tray_mode = TrayMode::parse(&mode).ok_or_else(|| format!("unknown tray mode: {mode}"))?;
    {
        let mut guard = state.lock().await;
        guard.tray_mode = tray_mode;
        set_setting(&guard.db, "trayMode", &mode).map_err(|e| e.to_string())?;
    }
    render(&app, state.inner()).await;
    Ok(())
}

#[tauri::command]
pub async fn set_launch_at_login(enabled: bool, app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    {
        let mut guard = state.lock().await;
        guard.launch_at_login = enabled;
        set_setting(&guard.db, "launchAtLogin", &enabled).map_err(|e| e.to_string())?;
    }
    let autolaunch = app.autolaunch();
    let result = if enabled { autolaunch.enable() } else { autolaunch.disable() };
    result.map_err(|e| e.to_string())?;
    render(&app, state.inner()).await;
    Ok(())
}

/// Shows a native OS notification for an `AgentNotification`. Used both by
/// the scheduler (real threshold-crossing notifications from
/// `NotificationEngine::evaluate` and each plugin's own
/// `drain_notifications()`) and by the manual "Send Test Notification"
/// button in the popover — same code path, so testing the button is a
/// faithful test of the real thing.
pub fn show_agent_notification(app: &AppHandle, display_name: &str, notification: &agent_core::AgentNotification) -> Result<(), String> {
    let content = to_notification_content(display_name, notification);
    app.notification().builder().title(content.title).body(content.body).show().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn send_test_notification(app: AppHandle) -> Result<(), String> {
    show_agent_notification(
        &app,
        "Agent Status",
        &agent_core::AgentNotification {
            id: "test".into(),
            provider_id: "system".into(),
            severity: agent_core::NotificationSeverity::Info,
            reason: "test_notification".into(),
            message: "This is a test notification from Agent Status.".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    )
}

/// Reads back what `history::persist()` has written for one provider, newest
/// first. Called by `ui/popover.js`'s `loadRelativeUsageBars()` for the
/// no-known-limit rows' "% of recent peak" bar; also ready for a future
/// Timeline view (v1.5) to call for a longer history chart.
#[tauri::command]
pub async fn get_usage_history(provider_id: String, state: State<'_, SharedState>) -> Result<UsageHistoryResponse, String> {
    let guard = state.lock().await;
    let usage = recent_usage(&guard.db, &provider_id, HISTORY_ROW_LIMIT).map_err(|e| e.to_string())?;
    let cost = recent_cost(&guard.db, &provider_id, HISTORY_ROW_LIMIT).map_err(|e| e.to_string())?;
    Ok(UsageHistoryResponse { usage, cost })
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
