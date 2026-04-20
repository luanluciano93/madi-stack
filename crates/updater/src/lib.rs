#![forbid(unsafe_code)]
//! Component update flow: diff → download → atomic swap → rollback on failure.

use madi_core::Component;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("source error: {0}")]
    Source(#[from] madi_sources::SourceError),

    #[error("download error: {0}")]
    Download(#[from] madi_downloader::DownloadError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type UpdateResult<T> = Result<T, UpdateError>;

#[derive(Debug, Clone)]
pub struct UpdateStatus {
    pub component: Component,
    pub current: Option<semver::Version>,
    pub available: semver::Version,
    pub update_available: bool,
}

/// Compare installed vs. upstream for each component.
///
/// TODO(sprint-3): implement by zipping state_store::installed with sources::latest.
pub async fn check_all(_client: &reqwest::Client) -> UpdateResult<Vec<UpdateStatus>> {
    Ok(Vec::new())
}

/// Apply an update: download new zip into `bin/<c>.new/`, rename old to
/// `bin/<c>.bak/`, rename new to `bin/<c>/`, healthcheck, rollback on failure.
pub async fn apply(_client: &reqwest::Client, _component: Component) -> UpdateResult<()> {
    Ok(())
}
