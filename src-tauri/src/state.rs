use parking_lot::RwLock;

use madi_state_store::AppState as StoredState;

/// Shared application state, held inside a `tauri::State` wrapper.
///
/// Writes are rare (config changes), reads are frequent (status polling).
/// `parking_lot::RwLock` is preferred over `std::sync::RwLock` for speed.
#[derive(Debug, Default)]
pub struct AppState {
    pub stored: RwLock<StoredState>,
}
