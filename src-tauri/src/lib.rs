mod builtins;
mod commands;
mod history;
mod notification_bridge;
mod scheduler;
mod state;
mod view_model;

use agent_database::{get_setting, open_database};
use agent_tray::TrayMode;
use state::{AppState, SharedState};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tokio::sync::Mutex;

const POPOVER_WIDTH: f64 = 320.0;
const POPOVER_HEIGHT: f64 = 500.0;
const POPOVER_LABEL: &str = "popover";

fn toggle_popover(app: &AppHandle, tray_rect: tauri::Rect) {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    let scale = window.scale_factor().unwrap_or(1.0);
    let tray_pos = tray_rect.position.to_logical::<f64>(scale);
    let tray_size = tray_rect.size.to_logical::<f64>(scale);

    let mut x = tray_pos.x + tray_size.width / 2.0 - POPOVER_WIDTH / 2.0;
    let y = tray_pos.y + tray_size.height;

    if let Ok(Some(monitor)) = window.current_monitor() {
        let monitor_pos = monitor.position().to_logical::<f64>(scale);
        let monitor_size = monitor.size().to_logical::<f64>(scale);
        let min_x = monitor_pos.x;
        let max_x = monitor_pos.x + monitor_size.width - POPOVER_WIDTH;
        x = x.max(min_x).min(max_x);
    }

    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
    let _ = window.show();
    let _ = window.set_focus();

    // The popover is created once (hidden) at startup and, until now, only
    // ever updated by the pushed "status-update" event. That's a
    // fire-and-forget emit — `listen()`'s subscription is itself an async
    // round-trip (see docs/plugin-development.md's note on this exact
    // race), and a window built `visible: false` and left hidden for a
    // while may not process events delivered while off-screen at all.
    // Verified live: a process actively refreshing Claude in the background
    // still showed "No providers detected" on open, confirming the push
    // never reached the DOM. Re-running `render()` here doesn't fix that on
    // its own — it still only *emits*. What actually fixes it is directly
    // invoking the frontend's own `refresh()` (a real invoke() request/
    // response, exposed on `window` in popover.js for exactly this call)
    // right after showing, so the popover is correct via a guaranteed
    // round-trip instead of a maybe-delivered push.
    if let Some(state) = app.try_state::<SharedState>() {
        let state = state.inner().clone();
        let app = app.clone();
        let window = window.clone();
        tauri::async_runtime::spawn(async move {
            commands::render(&app, &state).await;
            let _ = window.eval("window.refresh && window.refresh()");
        });
    }
}

async fn init(app: AppHandle) {
    let data_dir = app.path().app_data_dir().expect("no app data dir available");
    std::fs::create_dir_all(&data_dir).expect("failed to create app data dir");
    let db = open_database(data_dir.join("agent-status.sqlite").to_str().unwrap()).expect("failed to open database");

    let tray_mode_str: String = get_setting(&db, "trayMode", "compact".to_string());
    let tray_mode = TrayMode::parse(&tray_mode_str).unwrap_or(TrayMode::Compact);
    let launch_at_login: bool = get_setting(&db, "launchAtLogin", false);

    let registry = builtins::create_default_registry().await;
    let notifications = agent_notifications::NotificationEngine::default();

    let icon = Image::from_bytes(include_bytes!("../icons/trayTemplate.png")).expect("invalid tray icon asset");
    let quit_item = MenuItem::with_id(&app, "quit", "Quit Agent Status", true, None::<&str>).expect("menu item");
    let menu = Menu::with_items(&app, &[&quit_item]).expect("menu");

    let app_for_tray = app.clone();
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Agent Status")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
            }
        })
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click { rect, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_popover(&app_for_tray, rect);
            }
        })
        .build(&app)
        .expect("failed to build tray icon");

    let popover = WebviewWindowBuilder::new(&app, POPOVER_LABEL, WebviewUrl::App("index.html".into()))
        .title("Agent Status")
        .inner_size(POPOVER_WIDTH, POPOVER_HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .expect("failed to build popover window");
    // Menu-bar popovers close on losing focus, like every native menu extra.
    let popover_for_blur = popover.clone();
    popover.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let _ = popover_for_blur.hide();
        }
    });

    let shared_state: SharedState = Arc::new(Mutex::new(AppState {
        registry,
        notifications,
        db,
        tray_mode,
        launch_at_login,
        latest_view_model: None,
        tray: Some(tray),
    }));
    app.manage(shared_state.clone());

    commands::render(&app, &shared_state).await;
    scheduler::start(app.clone(), shared_state).await;
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            commands::get_view_model,
            commands::set_tray_mode,
            commands::set_launch_at_login,
            commands::send_test_notification,
            commands::get_usage_history,
            commands::quit_app,
        ])
        .setup(|app| {
            // A menu-bar-only utility shouldn't have a Dock icon or behave
            // like a regular foreground app — without this, the popover
            // window's focus/blur handling is unreliable (macOS is
            // inconsistent about granting a borderless utility window real
            // key-window status under the default "Regular" policy).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                init(handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
