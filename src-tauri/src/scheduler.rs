use crate::commands::{render, show_agent_notification};
use crate::state::SharedState;
use agent_core::AgentNotification;
use std::time::Duration;
use tauri::AppHandle;

/// Spawns one independent polling loop per registered plugin, each on its
/// own `refresh_interval_ms` cadence. Every tick locks the shared state only
/// long enough to run that one plugin's `refresh()` and notification
/// evaluation — see `state.rs` for why this is coarser than the original
/// per-plugin-lock design.
pub async fn start(app: AppHandle, state: SharedState) {
    let ids_and_intervals: Vec<(String, u64)> = {
        let guard = state.lock().await;
        guard.registry.list().iter().map(|p| (p.id().to_string(), p.refresh_interval_ms())).collect()
    };

    for (id, interval_ms) in ids_and_intervals {
        let app = app.clone();
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tick(&app, &state, &id).await;
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
        });
    }
}

async fn tick(app: &AppHandle, state: &SharedState, provider_id: &str) {
    let mut fresh: Vec<(String, AgentNotification)> = Vec::new();
    {
        let mut guard = state.lock().await;
        let crate::state::AppState { registry, notifications, db, .. } = &mut *guard;
        if let Some(plugin) = registry.list_mut().find(|p| p.id() == provider_id) {
            plugin.refresh().await;
            let status = plugin.get_status();
            crate::history::persist(db, &status);
            let display_name = status.display_name.clone();
            fresh.extend(notifications.evaluate(&status).into_iter().map(|n| (display_name.clone(), n)));
            fresh.extend(plugin.drain_notifications().into_iter().map(|n| (display_name.clone(), n)));
        };
    }
    // Notify after releasing the state lock — showing a native notification
    // does its own IO and shouldn't hold up other plugins' refreshes.
    for (display_name, notification) in &fresh {
        let _ = show_agent_notification(app, display_name, notification);
    }
    render(app, state).await;
}
