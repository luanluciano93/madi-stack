#![forbid(unsafe_code)]
//! Load and persist `madistack.toml`.

use std::{collections::BTreeMap, path::Path};

use madi_core::{Component, PortConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    De(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    Ser(#[from] toml::ser::Error),
}

pub type StateResult<T> = Result<T, StateError>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    #[serde(default)]
    pub installed: BTreeMap<Component, semver::Version>,
    #[serde(default)]
    pub ports: PortConfig,
    #[serde(default)]
    pub prefs: Prefs,
    /// Incremented on every `install_component(PhpMyAdmin)`. The frontend
    /// compares this against its own `acked_count` in localStorage to
    /// decide whether to keep showing the initial-password banner.
    #[serde(default)]
    pub pma_install_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Prefs {
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub open_browser_on_start: bool,
    #[serde(default)]
    pub minimize_to_tray_on_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    PtBr,
    En,
    Es,
    Nl,
    De,
    It,
    Pl,
    Ru,
    ZhCn,
    Tr,
    Hu,
    Lv,
    Ro,
}

/// Load the state file, or return a default one if it doesn't exist.
pub fn load_or_default(path: &Path) -> StateResult<AppState> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(toml::from_str(&raw)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppState::default()),
        Err(e) => Err(e.into()),
    }
}

/// Persist the state file atomically (write to temp → rename).
pub fn save(path: &Path, state: &AppState) -> StateResult<()> {
    let raw = toml::to_string_pretty(state)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, raw)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
