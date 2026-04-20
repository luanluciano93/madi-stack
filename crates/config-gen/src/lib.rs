#![forbid(unsafe_code)]
//! Renders nginx.conf / php.ini / my.ini from Tera templates.
//!
//! The templates live in `/templates/*.tera` at the repo root and are
//! embedded into the binary at compile time via `include_str!`.

use std::path::Path;

use madi_core::PortConfig;

#[derive(Debug, thiserror::Error)]
pub enum ConfigGenError {
    #[error("template error: {0}")]
    Tera(#[from] tera::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ConfigGenResult<T> = Result<T, ConfigGenError>;

/// Context passed to every template.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenderContext<'a> {
    pub install_dir: &'a Path,
    pub ports: PortConfig,
    pub document_root: &'a Path,
}

/// Render the three main config files into `config_dir`.
///
/// TODO(sprint-1): embed templates with `include_str!` + run `tera::Tera::one_off`.
pub fn render_all(_ctx: &RenderContext<'_>, _config_dir: &Path) -> ConfigGenResult<()> {
    Ok(())
}
