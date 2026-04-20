#![forbid(unsafe_code)]
//! Streaming downloader with SHA256 verification and zip extraction.
//!
//! Designed for the first-run flow where we fetch ~100 MB of components in
//! parallel and surface progress to the UI.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("SHA256 mismatch (expected {expected}, got {actual})")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub type DownloadResult<T> = Result<T, DownloadError>;

/// Progress events emitted to the UI.
#[derive(Debug, Clone)]
pub enum Progress {
    Started { total_bytes: Option<u64> },
    Downloaded { bytes: u64 },
    Verifying,
    Extracting,
    Done,
}

/// Download `url` to `dest` and verify against `expected_sha256`.
///
/// TODO(sprint-1): implement with streaming writer + Sha256 hasher + progress
/// callback via `tokio::sync::mpsc::Sender<Progress>`.
pub async fn download_verified(
    _client: &reqwest::Client,
    _url: &str,
    _dest: &Path,
    _expected_sha256: &str,
) -> DownloadResult<()> {
    Ok(())
}

/// Extract a zip archive into `target_dir`, stripping the single top-level
/// folder that these upstream zips always ship with.
///
/// TODO(sprint-1): implement in `spawn_blocking`, since `zip` is sync.
pub async fn extract_zip(_zip_path: &Path, _target_dir: &Path) -> DownloadResult<()> {
    Ok(())
}
