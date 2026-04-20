// Prevent a console window from showing up on Windows in release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod state;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

fn main() {
    // Structured logging — env var `MADISTACK_LOG=debug` bumps verbosity.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("MADISTACK_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .manage(state::AppState::default())
        .setup(|app| {
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "MadiStack starting");
            let _ = app.handle();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::list_components,
            commands::port_available,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
