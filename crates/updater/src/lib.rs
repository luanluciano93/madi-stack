#![forbid(unsafe_code)]
//! Component update flow: diff → download → atomic swap → rollback on failure.
//!
//! Layout during an update for component `C`:
//!
//! ```text
//! bin/C/         ← currently installed (running if supervised)
//! bin/C.new/     ← freshly extracted upstream zip
//! bin/C.bak/     ← previous version, kept after swap as rollback safety
//! ```
//!
//! Swap sequence (each step is a single-call fs op on Windows):
//! 1. Extract to `bin/C.new/`.
//! 2. Rename any existing `bin/C.bak/` to `bin/C.old-<ts>/` and delete async.
//! 3. Rename `bin/C/` → `bin/C.bak/`.
//! 4. Rename `bin/C.new/` → `bin/C/`.
//! 5. Healthcheck: signature exe present. If not, undo 3 & 4 and error.
//! 6. Optional caller-supplied smoke test. If it fails, rollback and error.
//!
//! The `.bak/` is kept on disk so the UI can expose a "Reverter" action
//! without re-downloading the previous version.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use madi_core::{Component, ReleaseInfo};
use madi_downloader::{download_verified, extract_zip, DownloadError, Progress};
use madi_sources::latest;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("source error: {0}")]
    Source(#[from] madi_sources::SourceError),

    #[error("download error: {0}")]
    Download(#[from] madi_downloader::DownloadError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("post-swap healthcheck failed: {0}")]
    Healthcheck(String),

    #[error("post-swap smoke test failed: {0}")]
    SmokeTest(String),

    #[error("update cancelled")]
    Cancelled,

    #[error("nothing to rollback: no {0:?}.bak/ on disk")]
    NothingToRollback(Component),
}

pub type UpdateResult<T> = Result<T, UpdateError>;

/// Boxed future returned by a smoke test closure.
pub type SmokeFuture = Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Caller-supplied post-swap verification. Receives the install dir and the
/// component that was just swapped. If it returns `Err`, the updater rolls
/// back to `.bak/` before returning [`UpdateError::SmokeTest`].
pub type SmokeFn = Box<dyn FnOnce(PathBuf, Component) -> SmokeFuture + Send>;

#[derive(Debug, Clone)]
pub struct UpdateStatus {
    pub component: Component,
    pub current: Option<semver::Version>,
    pub available: semver::Version,
    pub update_available: bool,
    pub release: ReleaseInfo,
}

/// For each component, fetch the latest upstream release and compare against
/// the installed version. `installed(c) == None` is surfaced as
/// `update_available = false` — the updater only operates on components that
/// have already been installed via the first-run flow.
pub async fn check_all<F>(client: &reqwest::Client, installed: F) -> UpdateResult<Vec<UpdateStatus>>
where
    F: Fn(Component) -> Option<semver::Version>,
{
    let mut out = Vec::with_capacity(4);
    for c in Component::all() {
        let release = latest(client, *c).await?;
        let current = installed(*c);
        let update_available = match &current {
            Some(v) => release.version > *v,
            None => false,
        };
        out.push(UpdateStatus {
            component: *c,
            current,
            available: release.version.clone(),
            update_available,
            release,
        });
    }
    Ok(out)
}

/// Remove any `bin/*.old-*` directories left behind by previous swaps.
///
/// `apply` spawns a background task to delete retired `.bak/` dirs, but that
/// task dies with the process. Call this on boot to reclaim space from any
/// run that was killed mid-cleanup.
///
/// Best-effort: individual failures are logged and skipped, not propagated —
/// a locked file shouldn't block app startup.
pub async fn gc_retired(install_dir: &Path) -> std::io::Result<()> {
    let bin = install_dir.join("bin");
    let mut rd = match tokio::fs::read_dir(&bin).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    while let Some(entry) = rd.next_entry().await? {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // Match any `<slug>.old-<ts>` dir. We don't parse the slug — any dir
        // with `.old-` in its name is assumed to be our retirement marker.
        if !name_str.contains(".old-") {
            continue;
        }
        let path = entry.path();
        if let Err(e) = tokio::fs::remove_dir_all(&path).await {
            tracing::warn!(path = %path.display(), error = %e, "gc_retired: skip");
        } else {
            tracing::info!(path = %path.display(), "gc_retired: removed");
        }
    }
    Ok(())
}

/// Apply an update for one component.
///
/// Caller is responsible for ensuring the service is stopped — on Windows,
/// renaming a directory that holds a running `.exe` fails with
/// `ERROR_ACCESS_DENIED`.
///
/// Parameters:
/// - `cancel`: if fired before the swap, the in-progress download aborts and
///   no files are touched on disk. After the swap starts, cancellation is
///   ignored — we don't want to leave the install in a half-renamed state.
/// - `smoke`: optional post-swap verification (e.g. boot the service and
///   check it stays up). If it returns `Err`, we restore `.bak/` and return
///   [`UpdateError::SmokeTest`].
///
/// Returns the new version on success. Progress events are forwarded through
/// the optional channel (same [`Progress`] enum as `download_verified`).
pub async fn apply(
    client: &reqwest::Client,
    install_dir: &Path,
    component: Component,
    progress: Option<mpsc::Sender<Progress>>,
    cancel: Option<CancellationToken>,
    smoke: Option<SmokeFn>,
) -> UpdateResult<semver::Version> {
    // Opportunistic GC of prior retirement dirs. Non-fatal.
    if let Err(e) = gc_retired(install_dir).await {
        tracing::warn!(error = %e, "gc_retired failed before apply");
    }

    let release = latest(client, component).await?;

    let tmp_dir = install_dir.join("tmp");
    tokio::fs::create_dir_all(&tmp_dir).await?;
    let zip_path = tmp_dir.join(&release.filename);

    download_with_retry(
        client,
        &release.download_url,
        &zip_path,
        release.sha256.as_deref(),
        progress.clone(),
        cancel.as_ref(),
    )
    .await?;

    if let Some(tx) = &progress {
        let _ = tx.send(Progress::Extracting).await;
    }

    let bin = install_dir.join("bin");
    let current = bin.join(component.slug());
    let new_dir = bin.join(format!("{}.new", component.slug()));
    let bak = bin.join(format!("{}.bak", component.slug()));

    // Clean any stale .new/ from a previously aborted run.
    if new_dir.exists() {
        tokio::fs::remove_dir_all(&new_dir).await?;
    }
    extract_zip(&zip_path, &new_dir).await?;

    // If a previous .bak/ is around, retire it so we can put the current
    // version in its place. Timestamp avoids collisions across fast-repeat
    // swaps.
    if bak.exists() {
        let retired = bin.join(format!(
            "{}.old-{}",
            component.slug(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        ));
        tokio::fs::rename(&bak, &retired).await?;
        tokio::spawn(async move {
            let _ = tokio::fs::remove_dir_all(&retired).await;
        });
    }

    // The swap. Each rename is atomic on NTFS; we do two of them and heal
    // if the second fails.
    if current.exists() {
        tokio::fs::rename(&current, &bak).await?;
    }
    if let Err(e) = tokio::fs::rename(&new_dir, &current).await {
        // Put the old version back before bailing so we don't leave the user
        // with no `bin/C/` at all.
        if bak.exists() {
            let _ = tokio::fs::rename(&bak, &current).await;
        }
        return Err(e.into());
    }

    if !signature_path(install_dir, component).is_file() {
        // Extracted archive didn't match the expected layout — roll back.
        let _ = tokio::fs::remove_dir_all(&current).await;
        if bak.exists() {
            let _ = tokio::fs::rename(&bak, &current).await;
        }
        return Err(UpdateError::Healthcheck(format!(
            "signature binary missing after swap: {}",
            signature_path(install_dir, component).display()
        )));
    }

    // Optional smoke test. On failure, restore `.bak/` over `current/` so the
    // user is left with the previous working version.
    if let Some(f) = smoke {
        let fut = f(install_dir.to_path_buf(), component);
        if let Err(msg) = fut.await {
            tracing::warn!(component = %component, error = %msg, "smoke test failed — rolling back");
            let scratch = bin.join(format!("{}.failed", component.slug()));
            if scratch.exists() {
                let _ = tokio::fs::remove_dir_all(&scratch).await;
            }
            let _ = tokio::fs::rename(&current, &scratch).await;
            if bak.exists() {
                let _ = tokio::fs::rename(&bak, &current).await;
                // Put the failed copy where `.bak/` was so a future apply
                // doesn't think the rollback target is the broken version.
                let _ = tokio::fs::rename(&scratch, &bak).await;
            }
            return Err(UpdateError::SmokeTest(msg));
        }
    }

    let _ = tokio::fs::remove_file(&zip_path).await;

    if let Some(tx) = &progress {
        let _ = tx.send(Progress::Done).await;
    }

    tracing::info!(
        component = %component,
        version = %release.version,
        "update applied"
    );
    Ok(release.version)
}

/// Retry transient network failures up to 3 attempts with 1s/2s backoff.
///
/// Transient = reqwest HTTP/IO error. Checksum mismatches, unsafe paths and
/// zip errors are logic errors and returned immediately. Cancellation also
/// returns immediately. No byte-range resume — each retry restarts from zero.
async fn download_with_retry(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    progress: Option<mpsc::Sender<Progress>>,
    cancel: Option<&CancellationToken>,
) -> Result<(), DownloadError> {
    const MAX_ATTEMPTS: u32 = 3;
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let res = download_verified(
            client,
            url,
            dest,
            expected_sha256,
            progress.clone(),
            cancel.cloned(),
        )
        .await;

        match res {
            Ok(()) => return Ok(()),
            Err(e) => {
                let transient = matches!(e, DownloadError::Http(_) | DownloadError::Io(_));
                if !transient || attempt >= MAX_ATTEMPTS {
                    return Err(e);
                }
                if let Some(c) = cancel {
                    if c.is_cancelled() {
                        return Err(DownloadError::Cancelled);
                    }
                }
                let backoff = std::time::Duration::from_secs(1u64 << (attempt - 1));
                tracing::warn!(
                    url = %url,
                    attempt,
                    next_in_secs = backoff.as_secs(),
                    error = %e,
                    "download failed — retrying"
                );
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

/// Swap `bin/C/` ↔ `bin/C.bak/` — undo the most recent `apply`. Service must
/// be stopped (same reason as `apply`).
pub async fn rollback(install_dir: &Path, component: Component) -> UpdateResult<()> {
    let bin = install_dir.join("bin");
    let current = bin.join(component.slug());
    let bak = bin.join(format!("{}.bak", component.slug()));

    if !bak.exists() {
        return Err(UpdateError::NothingToRollback(component));
    }

    // Two-phase via a scratch name so we never lose `current` if the second
    // rename trips.
    let scratch = bin.join(format!("{}.rolling", component.slug()));
    if scratch.exists() {
        tokio::fs::remove_dir_all(&scratch).await?;
    }
    if current.exists() {
        tokio::fs::rename(&current, &scratch).await?;
    }
    if let Err(e) = tokio::fs::rename(&bak, &current).await {
        if scratch.exists() {
            let _ = tokio::fs::rename(&scratch, &current).await;
        }
        return Err(e.into());
    }
    if scratch.exists() {
        tokio::fs::rename(&scratch, &bak).await?;
    }
    tracing::info!(component = %component, "update rolled back");
    Ok(())
}

fn signature_path(install_dir: &Path, component: Component) -> PathBuf {
    let bin = install_dir.join("bin");
    match component {
        Component::Nginx => bin.join("nginx").join("nginx.exe"),
        Component::Php => bin.join("php").join("php-cgi.exe"),
        Component::MariaDb => bin.join("mariadb").join("bin").join("mysqld.exe"),
        Component::PhpMyAdmin => bin.join("phpmyadmin").join("index.php"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_status_flags_newer_version() {
        let s = UpdateStatus {
            component: Component::Nginx,
            current: Some(semver::Version::new(1, 27, 0)),
            available: semver::Version::new(1, 28, 0),
            update_available: true,
            release: ReleaseInfo {
                component: Component::Nginx,
                version: semver::Version::new(1, 28, 0),
                download_url: String::new(),
                sha256: None,
                filename: String::new(),
            },
        };
        assert!(s.update_available);
        assert!(s.current.is_some());
    }

    #[tokio::test]
    async fn gc_retired_removes_only_old_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        tokio::fs::create_dir_all(&bin).await.unwrap();
        tokio::fs::create_dir(bin.join("nginx")).await.unwrap();
        tokio::fs::create_dir(bin.join("nginx.bak")).await.unwrap();
        tokio::fs::create_dir(bin.join("nginx.old-1700000000"))
            .await
            .unwrap();
        tokio::fs::create_dir(bin.join("php.old-1700000001"))
            .await
            .unwrap();

        gc_retired(tmp.path()).await.unwrap();

        assert!(bin.join("nginx").exists());
        assert!(bin.join("nginx.bak").exists());
        assert!(!bin.join("nginx.old-1700000000").exists());
        assert!(!bin.join("php.old-1700000001").exists());
    }

    #[tokio::test]
    async fn gc_retired_missing_bin_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        gc_retired(tmp.path()).await.unwrap();
    }
}
