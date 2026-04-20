//! Tauri commands exposed to the Svelte frontend.
//!
//! Every command returns `Result<T, String>` because Tauri serializes errors
//! as strings on the JS side. We convert from `anyhow::Error` at the boundary.

use madi_core::Component;

/// Simple health-check so the frontend can confirm the backend is alive.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

/// List the 4 components we manage, in display order.
#[tauri::command]
pub fn list_components() -> Vec<ComponentInfo> {
    Component::all()
        .iter()
        .map(|c| ComponentInfo {
            slug: c.slug().into(),
            name: c.display_name().into(),
        })
        .collect()
}

/// Check whether a TCP port on 127.0.0.1 is available.
#[tauri::command]
pub fn port_available(port: u16) -> bool {
    madi_services::is_port_available(port)
}

#[derive(Debug, serde::Serialize)]
pub struct ComponentInfo {
    pub slug: String,
    pub name: String,
}
