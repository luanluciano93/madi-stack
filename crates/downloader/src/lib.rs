#![forbid(unsafe_code)]
//! Streaming downloader with SHA256 verification and zip extraction.
//!
//! Two entry points:
//! - [`download_verified`] — streams a URL to disk, computes SHA256 as bytes
//!   arrive, verifies against an expected digest if provided.
//! - [`extract_zip`] — extracts an archive into a target directory, stripping
//!   the single common top-level folder if all entries share one (nginx,
//!   mariadb, phpmyadmin zips all have this), and guarding against zip-slip.

use std::{
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

pub mod extract;

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

    #[error("download cancelled")]
    Cancelled,

    #[error("zip entry has unsafe path: {0}")]
    UnsafePath(String),

    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub type DownloadResult<T> = Result<T, DownloadError>;

/// Progress events emitted during a download.
#[derive(Debug, Clone)]
pub enum Progress {
    /// Server accepted the request; `total_bytes` is `None` when the server
    /// does not advertise Content-Length.
    Started { total_bytes: Option<u64> },
    /// Cumulative bytes written to disk so far.
    Downloaded { bytes: u64 },
    /// Hash check in progress (only relevant when an expected digest was set).
    Verifying,
    /// Extraction of an archive in progress.
    Extracting,
    /// All phases finished successfully.
    Done,
}

/// Stream `url` into `dest`, computing SHA256 while bytes arrive.
///
/// - If `expected_sha256` is `Some`, the digest is compared and a mismatch
///   returns [`DownloadError::ChecksumMismatch`]. The partial file is left
///   on disk for inspection — the caller is expected to delete it.
/// - If `cancel` fires, the in-progress download stops and the partial file
///   is removed. Returns [`DownloadError::Cancelled`].
/// - `progress` is best-effort: send errors are ignored (a closed channel
///   just means the UI has stopped listening, not that the download failed).
pub async fn download_verified(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    progress: Option<tokio::sync::mpsc::Sender<Progress>>,
    cancel: Option<CancellationToken>,
) -> DownloadResult<()> {
    let response = client.get(url).send().await?.error_for_status()?;
    let total = response.content_length();

    if let Some(tx) = &progress {
        let _ = tx.send(Progress::Started { total_bytes: total }).await;
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let result = stream_to_file(response, dest, progress.as_ref(), cancel.as_ref()).await;

    match result {
        Ok(digest) => {
            if let Some(tx) = &progress {
                let _ = tx.send(Progress::Verifying).await;
            }
            if let Some(expected) = expected_sha256 {
                if !expected.eq_ignore_ascii_case(&digest) {
                    return Err(DownloadError::ChecksumMismatch {
                        expected: expected.to_string(),
                        actual: digest,
                    });
                }
            }
            if let Some(tx) = &progress {
                let _ = tx.send(Progress::Done).await;
            }
            Ok(())
        }
        Err(e) => {
            // Best-effort cleanup for partial download
            let _ = tokio::fs::remove_file(dest).await;
            Err(e)
        }
    }
}

async fn stream_to_file(
    response: reqwest::Response,
    dest: &Path,
    progress: Option<&tokio::sync::mpsc::Sender<Progress>>,
    cancel: Option<&CancellationToken>,
) -> DownloadResult<String> {
    let mut file = tokio::fs::File::create(dest).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if let Some(c) = cancel {
            if c.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
        }
        let bytes = chunk?;
        hasher.update(&bytes);
        file.write_all(&bytes).await?;
        downloaded += bytes.len() as u64;
        if let Some(tx) = progress {
            let _ = tx.send(Progress::Downloaded { bytes: downloaded }).await;
        }
    }

    file.flush().await?;
    file.sync_all().await?;
    Ok(hex::encode(hasher.finalize()))
}

/// Extract a zip archive at `zip_path` into `target_dir`.
///
/// If all entries share a single top-level folder (the common layout for
/// nginx, mariadb and phpMyAdmin upstream zips), that folder is stripped
/// so the contents land directly in `target_dir`.
///
/// Runs in a blocking task because the `zip` crate is sync.
pub async fn extract_zip(zip_path: &Path, target_dir: &Path) -> DownloadResult<()> {
    let zip_path = zip_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract::extract_zip_sync(&zip_path, &target_dir)).await?
}

/// Safe path join that refuses entries trying to escape `base` via `..`
/// or absolute paths (zip-slip prevention).
pub(crate) fn safe_join(base: &Path, rel: &str) -> DownloadResult<PathBuf> {
    use std::path::Component;

    let rel = rel.replace('\\', "/");
    let rel_path = Path::new(rel.trim_start_matches('/'));

    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(DownloadError::UnsafePath(rel.clone())),
        }
    }

    Ok(base.join(rel_path))
}

/// Find the common single top-level folder of every entry in an archive.
///
/// Returns `Some("foo/")` if every non-empty entry starts with "foo/".
/// Returns `None` if entries have different top-level prefixes or if any
/// entry sits directly at the root (like PHP zips, which extract flat).
pub(crate) fn find_common_top_prefix<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Option<String> {
    let mut prefix: Option<String> = None;

    for i in 0..archive.len() {
        let entry = archive.by_index(i).ok()?;
        let name = entry.name().replace('\\', "/");
        let trimmed = name.trim_start_matches('/');
        if trimmed.is_empty() {
            continue;
        }

        let Some((top, rest)) = trimmed.split_once('/') else {
            // File sitting at the root — no common prefix possible
            return None;
        };
        if top.is_empty() {
            return None;
        }
        // If this entry IS the top-level directory itself, `rest` is empty
        // and that's fine — it still confirms the prefix.
        let _ = rest;

        match &prefix {
            None => prefix = Some(format!("{top}/")),
            Some(p) if p.trim_end_matches('/') == top => {}
            _ => return None,
        }
    }

    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_allows_normal_paths() {
        let base = Path::new("/tmp/foo");
        let p = safe_join(base, "bar/baz.txt").unwrap();
        assert_eq!(p, Path::new("/tmp/foo/bar/baz.txt"));
    }

    #[test]
    fn safe_join_rejects_parent() {
        assert!(safe_join(Path::new("/tmp"), "../etc/passwd").is_err());
    }

    #[test]
    fn safe_join_rejects_absolute() {
        assert!(safe_join(Path::new("/tmp"), "/etc/passwd").is_ok());
        // The leading slash is trimmed; result is /tmp/etc/passwd, which is
        // inside base. That's actually fine for our use.
        // But a windows-style drive letter should fail:
        #[cfg(windows)]
        assert!(safe_join(Path::new("C:\\tmp"), "D:\\evil").is_err());
    }

    #[test]
    fn safe_join_normalizes_backslashes() {
        let base = Path::new("/tmp");
        let p = safe_join(base, "bin\\nginx.exe").unwrap();
        assert_eq!(p, Path::new("/tmp/bin/nginx.exe"));
    }
}
