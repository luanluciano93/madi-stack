// Prevent a console window from showing up on Windows in release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod install;
mod mkcert;
mod state;
mod tray;
mod version_probe;

use std::time::Duration;

use tauri::{Emitter, Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

use madi_core::{Component, ServiceStatus};

use crate::commands::{ServiceStatusEvent, STATUS_EVENT};

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
        .manage(state::AppState::new())
        .setup(|app| {
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "MadiStack starting");
            tray::init(app)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(status_watcher(handle));

            let app_state = app.state::<state::AppState>();
            let install_dir = app_state.supervisor.install_dir().to_path_buf();
            let ports = app_state.stored.read().ports;

            // Sweep `.old-<ts>/` dirs left by swaps that were killed mid-cleanup.
            let install_dir_gc = install_dir.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = madi_updater::gc_retired(&install_dir_gc).await {
                    tracing::warn!(error = %e, "boot gc_retired failed");
                }
            });

            // Re-render on-disk configs from the embedded templates. Keeps
            // nginx.conf / php.ini / my.ini / pma config in sync whenever
            // we ship a template change — user has no canonical state to
            // preserve, templates are source of truth.
            if let Err(e) = commands::render_configs_at_boot(&install_dir, ports) {
                tracing::warn!(error = %e, "boot render_configs failed");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close button → minimize to tray instead of quitting. Explicit
            // quit still available via tray menu or `app.exit(0)`. This is
            // the standard USBWebserver-era behavior users expect.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::list_components,
            commands::port_available,
            commands::port_inspect,
            commands::service_start,
            commands::service_stop,
            commands::service_status,
            commands::service_logs,
            commands::get_config,
            commands::save_config,
            commands::firewall_ensure_rules,
            commands::firewall_remove_rules,
            commands::firewall_rules_status,
            commands::component_installed,
            commands::component_install,
            commands::install_all,
            commands::updater_check,
            commands::updater_apply,
            commands::updater_rollback,
            commands::vhost_list,
            commands::vhost_enable,
            commands::vhost_disable,
            commands::mkcert_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Poll each supervised service every 2s and emit a `service-status` event
/// whenever its status changes. Frontend subscribes once and drops its own
/// timer. Still catches crashes because we re-ask the supervisor.
async fn status_watcher(app: tauri::AppHandle) {
    let mut last = [
        (Component::Nginx, ServiceStatus::Stopped),
        (Component::Php, ServiceStatus::Stopped),
        (Component::MariaDb, ServiceStatus::Stopped),
    ];
    loop {
        {
            let state = app.state::<state::AppState>();
            for (component, prev) in &mut last {
                let now = state.supervisor.status(*component);
                if now != *prev {
                    *prev = now;
                    let _ = app.emit(
                        STATUS_EVENT,
                        ServiceStatusEvent {
                            slug: component.slug().into(),
                            status: now,
                        },
                    );
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
