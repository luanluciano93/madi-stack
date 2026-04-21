//! Integration with mkcert for local HTTPS vhosts.
//!
//! We don't bundle mkcert in the main installer — it lives in its own
//! GitHub releases and we fetch on demand into `bin/mkcert/mkcert.exe`.
//! Issuing certificates for specific hostnames is unprivileged; only the
//! one-time `mkcert -install` (adds the local root CA to Windows' trust
//! store) goes through the elevated system helper.

use std::path::{Path, PathBuf};

use serde::Deserialize;

const MKCERT_LATEST_URL: &str = "https://api.github.com/repos/FiloSottile/mkcert/releases/latest";

/// Where we drop mkcert.exe after download. Sits alongside the other
/// bundled tool binaries under the portable install tree.
#[must_use]
pub fn mkcert_exe(install_dir: &Path) -> PathBuf {
    install_dir.join("bin").join("mkcert").join("mkcert.exe")
}

/// Marker file we write after a successful `mkcert -install`. Presence
/// means "root CA is trusted on this machine" — lets us skip the UAC
/// prompt on subsequent HTTPS toggles.
#[must_use]
pub fn ca_marker_path(install_dir: &Path) -> PathBuf {
    install_dir.join("bin").join("mkcert").join(".ca-installed")
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Download mkcert.exe from the latest GitHub release if it isn't already
/// on disk. Returns the final path either way.
pub async fn ensure_downloaded(install_dir: &Path) -> anyhow::Result<PathBuf> {
    let exe = mkcert_exe(install_dir);
    if exe.is_file() {
        return Ok(exe);
    }

    let client = madi_sources::build_client();
    let release: GhRelease = client
        .get(MKCERT_LATEST_URL)
        // GitHub rejects unknown clients with 403; any non-empty UA is fine.
        .header("User-Agent", "MadiStack")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let asset = release
        .assets
        .iter()
        // Match the Windows amd64 exe — filename pattern
        // `mkcert-v1.4.4-windows-amd64.exe` as of this writing.
        .find(|a| {
            a.name.contains("windows-amd64")
                && Path::new(&a.name)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        })
        .ok_or_else(|| anyhow::anyhow!("no windows-amd64 asset in mkcert latest release"))?;

    if let Some(parent) = exe.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Stream the binary directly to disk — ~5 MB, no need to buffer in
    // memory. Skip SHA256 (mkcert releases don't publish digests; the
    // TLS connection plus github.com's pinning is what we rely on).
    madi_downloader::download_verified(
        &client,
        &asset.browser_download_url,
        &exe,
        None,
        None,
        None,
    )
    .await?;

    Ok(exe)
}

/// `true` when the local CA has already been installed into the Windows
/// trust store (marker file present).
#[must_use]
pub fn ca_installed(install_dir: &Path) -> bool {
    ca_marker_path(install_dir).is_file()
}

/// Persist the "root CA installed" marker. Call after a successful
/// `mkcert -install` via the elevated helper.
pub fn mark_ca_installed(install_dir: &Path) -> std::io::Result<()> {
    let marker = ca_marker_path(install_dir);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        marker,
        "mkcert -install ran successfully.\nDelete this file to force a re-run.\n",
    )
}

/// Issue a cert for `hostname` into `cert_dir` (creates if missing).
/// Produces `cert.pem` and `key.pem`. Unprivileged — mkcert only needs
/// admin once, during `-install`.
pub async fn issue(install_dir: &Path, cert_dir: &Path, hostname: &str) -> anyhow::Result<()> {
    let exe = mkcert_exe(install_dir);
    if !exe.is_file() {
        anyhow::bail!("mkcert.exe não está disponível em {}", exe.display());
    }
    tokio::fs::create_dir_all(cert_dir).await?;

    let cert_path = cert_dir.join("cert.pem");
    let key_path = cert_dir.join("key.pem");

    let output = tokio::process::Command::new(&exe)
        .arg("-cert-file")
        .arg(&cert_path)
        .arg("-key-file")
        .arg(&key_path)
        .arg(hostname)
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "mkcert falhou ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
