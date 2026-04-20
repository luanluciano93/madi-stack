use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use madi_services::Supervisor;
use madi_state_store::{load_or_default, AppState as StoredState};

/// Shared application state, held inside a `tauri::State` wrapper.
///
/// Writes are rare (config changes), reads are frequent (status polling).
/// `parking_lot::RwLock` is preferred over `std::sync::RwLock` for speed.
pub struct AppState {
    pub stored: RwLock<StoredState>,
    pub supervisor: Arc<Supervisor>,
}

impl AppState {
    /// Build the app state anchored at the running binary's directory.
    ///
    /// Portable: `install_dir` is resolved from `current_exe()`, never from
    /// `%APPDATA%` or the registry.
    pub fn new() -> Self {
        let install_dir = current_exe_dir().unwrap_or_else(|| PathBuf::from("."));
        let state_path = install_dir.join("madistack.toml");
        let stored = load_or_default(&state_path).unwrap_or_else(|e| {
            tracing::warn!(error = %e, path = %state_path.display(), "failed to load state file — using defaults");
            StoredState::default()
        });
        // Document root must exist before nginx spawns — it doesn't mkdir it.
        let www = install_dir.join("www");
        if let Err(e) = std::fs::create_dir_all(&www) {
            tracing::warn!(error = %e, path = %www.display(), "failed to create www dir");
        }
        let supervisor = Arc::new(Supervisor::new(install_dir, stored.ports));
        Self {
            stored: RwLock::new(stored),
            supervisor,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(std::path::Path::to_path_buf)
}
