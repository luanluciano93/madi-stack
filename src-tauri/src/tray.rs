//! System tray icon + right-click menu.
//!
//! The tray stays on while the app is minimized or hidden so users can
//! start/stop services and open phpMyAdmin without restoring the window.
//! Left-click toggles window visibility; the menu exposes the same actions
//! that live in the Geral tab plus shortcuts to the filesystem.

use madi_core::Component;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;

// Menu IDs — kept short because tauri passes them around as strings.
const ID_TOGGLE: &str = "toggle";
const ID_START_ALL: &str = "start_all";
const ID_STOP_ALL: &str = "stop_all";
const ID_OPEN_PMA: &str = "open_pma";
const ID_OPEN_WWW: &str = "open_www";
const ID_QUIT: &str = "quit";

pub fn init<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let handle = app.handle();

    let menu = Menu::with_items(
        handle,
        &[
            &MenuItem::with_id(handle, ID_TOGGLE, "Mostrar / ocultar", true, None::<&str>)?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(handle, ID_START_ALL, "Iniciar todos", true, None::<&str>)?,
            &MenuItem::with_id(handle, ID_STOP_ALL, "Parar todos", true, None::<&str>)?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(handle, ID_OPEN_PMA, "Abrir phpMyAdmin", true, None::<&str>)?,
            &MenuItem::with_id(handle, ID_OPEN_WWW, "Abrir pasta www", true, None::<&str>)?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(handle, ID_QUIT, "Sair", true, None::<&str>)?,
        ],
    )?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("MadiStack")
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::AssetNotFound("default window icon not bundled".into())
        })?)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, &event))
        .on_tray_icon_event(|tray, event| handle_icon(&tray.app_handle().clone(), &event))
        .build(handle)?;

    Ok(())
}

fn handle_menu<R: Runtime>(app: &AppHandle<R>, event: &MenuEvent) {
    match event.id.as_ref() {
        ID_TOGGLE => toggle_main_window(app),
        ID_START_ALL => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { start_all(&app).await });
        }
        ID_STOP_ALL => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { stop_all(&app).await });
        }
        ID_OPEN_PMA => open_url(app, "http://localhost/phpmyadmin"),
        ID_OPEN_WWW => open_www_folder(app),
        ID_QUIT => {
            // Give the supervisor a chance to stop children gracefully before
            // we tear down the Tauri runtime (Drop on Supervisor kills via
            // Job Object but skips the graceful path).
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                state.supervisor.stop_all().await;
                app.exit(0);
            });
        }
        _ => {}
    }
}

fn handle_icon<R: Runtime>(app: &AppHandle<R>, event: &TrayIconEvent) {
    // Left-click = show/hide. Right-click is reserved for the native menu,
    // which Tauri handles automatically.
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        toggle_main_window(app);
    }
}

fn toggle_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Ok(true) = window.is_visible() {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

async fn start_all<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    for c in [Component::MariaDb, Component::Php, Component::Nginx] {
        if let Err(e) = state.supervisor.start(c).await {
            tracing::warn!(component = c.slug(), error = %e, "tray start_all: failed");
        }
    }
}

async fn stop_all<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    state.supervisor.stop_all().await;
}

fn open_url<R: Runtime>(app: &AppHandle<R>, url: &str) {
    use tauri_plugin_shell::ShellExt;
    #[allow(deprecated)] // tauri-plugin-opener split isn't adopted here yet
    if let Err(e) = app.shell().open(url, None) {
        tracing::warn!(url, error = %e, "failed to open URL via shell plugin");
    }
}

fn open_www_folder<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_shell::ShellExt;
    // `www` lives next to the .exe (portable layout — see state.rs).
    let Some(www) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .map(|p| p.join("www"))
    else {
        tracing::warn!("install dir unavailable — cannot open www folder");
        return;
    };
    #[allow(deprecated)]
    if let Err(e) = app.shell().open(www.to_string_lossy(), None) {
        tracing::warn!(path = %www.display(), error = %e, "failed to open www folder");
    }
}
