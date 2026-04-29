//! First-run installation flow: resolve → download → verify → extract.
//!
//! One [`install_component`] call per component; `render_configs` materializes
//! nginx.conf / php.ini / my.ini once the binaries are on disk. Progress is
//! surfaced to the frontend via the `install-progress` Tauri event.

use std::path::{Path, PathBuf};

use madi_config_gen::{render_all, render_site, RenderContext, SiteSsl, DEFAULT_PHP_EXTENSIONS};
use madi_core::{Component, PortConfig};
use madi_downloader::{download_verified, extract_zip, Progress};
use madi_sources::latest;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use crate::state::AppState;

pub const INSTALL_EVENT: &str = "install-progress";

/// Phases we emit to the frontend. Mirrors [`Progress`] with two extras —
/// `resolving` (HTTP call to the source manifest, before bytes flow) and
/// `error` so the UI can style a failure consistently.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallPhase {
    Resolving,
    Downloading,
    Verifying,
    Extracting,
    Done,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallProgressEvent {
    pub slug: String,
    pub phase: InstallPhase,
    /// Bytes downloaded so far (only meaningful in `downloading`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Total bytes per Content-Length, when the server sends it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Human-readable detail: version resolved, error message, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Check whether the component's signature executable is present.
///
/// We only check the one file the supervisor relies on — it's enough to
/// distinguish "not yet downloaded" from "downloaded". Users who delete
/// random files mid-install get a clean failure from the supervisor, not
/// from us.
#[must_use]
pub fn is_installed(install_dir: &Path, component: Component) -> bool {
    signature_path(install_dir, component).is_file()
}

fn signature_path(install_dir: &Path, component: Component) -> PathBuf {
    let bin = install_dir.join("bin");
    match component {
        Component::Nginx => bin.join("nginx").join("nginx.exe"),
        Component::Php => bin.join("php").join("php-cgi.exe"),
        Component::MariaDb => bin.join("mariadb").join("bin").join("mysqld.exe"),
        // phpMyAdmin is served by nginx — no binary, check for the entrypoint
        // PHP file instead.
        Component::PhpMyAdmin => bin.join("phpmyadmin").join("index.php"),
    }
}

/// Install one component: resolve latest, download, verify, extract.
///
/// Existing `bin/<slug>/` is wiped before extraction so repeat runs are
/// deterministic. Progress events stream out on `install-progress` with a
/// matching `slug` so multiple installs can run concurrently if ever needed.
pub async fn install_component(
    app: &AppHandle,
    install_dir: &Path,
    component: Component,
) -> anyhow::Result<()> {
    let slug = component.slug().to_string();

    emit(
        app,
        InstallProgressEvent {
            slug: slug.clone(),
            phase: InstallPhase::Resolving,
            bytes: None,
            total: None,
            message: None,
        },
    );

    let client = madi_sources::build_client();
    let info = match latest(&client, component).await {
        Ok(i) => i,
        Err(e) => {
            emit_error(app, &slug, format!("resolve failed: {e}"));
            return Err(e.into());
        }
    };

    tracing::info!(
        component = %component,
        version = %info.version,
        url = %info.download_url,
        "install: resolved"
    );

    let tmp_dir = install_dir.join("tmp");
    tokio::fs::create_dir_all(&tmp_dir).await?;
    let zip_path = tmp_dir.join(&info.filename);
    let target = install_dir.join("bin").join(component.slug());

    // Bridge downloader progress → Tauri events. The channel runs in the
    // background; we await the join at the end so no event is lost.
    let (tx, rx) = mpsc::channel::<Progress>(64);
    let bridge_app = app.clone();
    let bridge_slug = slug.clone();
    let bridge = tokio::spawn(async move {
        bridge_progress(&bridge_app, &bridge_slug, rx).await;
    });

    let res = download_verified(
        &client,
        &info.download_url,
        &zip_path,
        info.sha256.as_deref(),
        Some(tx.clone()),
        None,
    )
    .await;

    if let Err(e) = res {
        drop(tx);
        let _ = bridge.await;
        emit_error(app, &slug, format!("download failed: {e}"));
        return Err(e.into());
    }

    // Closing the channel drains the bridge; Extracting phase is emitted
    // explicitly so the UI can switch state regardless of downloader timing.
    drop(tx);
    let _ = bridge.await;

    emit(
        app,
        InstallProgressEvent {
            slug: slug.clone(),
            phase: InstallPhase::Extracting,
            bytes: None,
            total: None,
            message: None,
        },
    );

    if target.exists() {
        tokio::fs::remove_dir_all(&target).await?;
    }
    if let Err(e) = extract_zip(&zip_path, &target).await {
        emit_error(app, &slug, format!("extract failed: {e}"));
        return Err(e.into());
    }

    // Best-effort: clean the downloaded archive. Failure is non-fatal.
    let _ = tokio::fs::remove_file(&zip_path).await;

    // phpMyAdmin ships without a config.inc.php — we need to drop one in
    // pointing at the right MariaDB port and with a stable blowfish secret
    // so cookie logins survive the user tweaking settings later.
    if component == Component::PhpMyAdmin {
        let ports = app
            .try_state::<AppState>()
            .map_or_else(PortConfig::default, |s| s.stored.read().ports);
        if let Err(e) = ensure_pma_config(install_dir, ports) {
            tracing::warn!(error = %e, "install: failed to render pma config");
        }
    }

    persist_install(app, install_dir, component, &info.version);

    emit(
        app,
        InstallProgressEvent {
            slug: slug.clone(),
            phase: InstallPhase::Done,
            bytes: None,
            total: None,
            message: Some(format!("v{}", info.version)),
        },
    );
    tracing::info!(component = %component, "install: done");
    Ok(())
}

fn state_file_path(install_dir: &Path) -> PathBuf {
    install_dir.join("madistack.toml")
}

/// Persist the installed version + bump the pma install counter. Lifted
/// out of `install_component` so the latter stays under the clippy
/// `too_many_lines` threshold.
fn persist_install(
    app: &AppHandle,
    install_dir: &Path,
    component: Component,
    version: &semver::Version,
) {
    let Some(app_state) = app.try_state::<AppState>() else {
        return;
    };
    let mut stored = app_state.stored.write();
    stored.installed.insert(component, version.clone());
    // Bump on every (re)install of pma so the UI re-shows the "here's
    // your initial password" banner. Counter (not bool) keeps the trigger
    // reliable across dismiss → reinstall → dismiss cycles.
    if component == Component::PhpMyAdmin {
        stored.pma_install_count = stored.pma_install_count.saturating_add(1);
    }
    if let Err(e) = madi_state_store::save(&state_file_path(install_dir), &stored) {
        tracing::warn!(error = %e, "install: failed to persist installed version");
    }
}

/// Render nginx.conf + php.ini + my.ini + site-default.conf into `config/`.
///
/// Safe to call after all three services' binaries are on disk. The
/// supervisor also re-renders on start, but doing it here means the user can
/// inspect / edit configs before ever pressing Start.
pub fn render_configs(install_dir: &Path, ports: PortConfig) -> anyhow::Result<()> {
    let config_dir = install_dir.join("config");
    std::fs::create_dir_all(&config_dir)?;
    let document_root = install_dir.join("www");
    std::fs::create_dir_all(&document_root)?;
    let ctx = RenderContext {
        install_dir,
        document_root: &document_root,
        ports,
        php_extensions: DEFAULT_PHP_EXTENSIONS,
    };
    render_all(&ctx, &config_dir)?;
    // Skip pma if it isn't extracted yet — `render_configs` runs on every
    // `save_config`, and pma may legitimately not be installed.
    if install_dir
        .join("bin")
        .join("phpmyadmin")
        .join("index.php")
        .is_file()
    {
        ensure_pma_config(install_dir, ports)?;
    }
    re_render_vhosts(install_dir, ports);
    Ok(())
}

/// Refresh every `config/sites-enabled/*.conf` against the current ports +
/// install paths. Without this, vhosts created before a port change keep the
/// old `listen`/`fastcgi_pass` values and end up routing to whatever happens
/// to be on the stale port.
///
/// SSL state is read from disk (`config/certs/<name>/cert.pem` + `key.pem`) —
/// the `.conf` itself doesn't tell us if it was rendered with the HTTPS arm,
/// and rediscovering from the filesystem is simpler than threading metadata.
///
/// Best-effort: a single failing site is logged and skipped, the rest still
/// get refreshed. Returning an error here would break `save_config` for an
/// edge case the user didn't even cause.
fn re_render_vhosts(install_dir: &Path, ports: PortConfig) {
    let sites_enabled = install_dir.join("config").join("sites-enabled");
    let Ok(entries) = std::fs::read_dir(&sites_enabled) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("conf") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_safe_vhost_name(name) {
            continue;
        }
        let site_root = install_dir.join("www").join(name);
        if !site_root.is_dir() {
            // www/<name>/ has been removed out from under us — leave the
            // .conf alone; `vhost_disable` is the right path to clean it up.
            continue;
        }

        let cert_dir = install_dir.join("config").join("certs").join(name);
        let cert_path = cert_dir.join("cert.pem");
        let key_path = cert_dir.join("key.pem");
        let ssl = if cert_path.is_file() && key_path.is_file() {
            Some(SiteSsl {
                cert_path: &cert_path,
                key_path: &key_path,
            })
        } else {
            None
        };

        let ctx = RenderContext {
            install_dir,
            document_root: &site_root,
            ports,
            php_extensions: DEFAULT_PHP_EXTENSIONS,
        };
        if let Err(e) = render_site(&ctx, name, ssl, &path) {
            tracing::warn!(site = %name, error = %e, "re_render_vhosts: render_site failed");
        }
    }
}

/// Mirror of `validate_vhost_name` in `commands.rs` — duplicated rather than
/// shared because the install module is upstream of commands and we don't
/// want a circular dep just to share three lines.
fn is_safe_vhost_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Ensure `bin/phpmyadmin/config.inc.php` exists and matches the current
/// `ports`. Generates a fresh blowfish secret the first time and persists
/// it so subsequent calls reuse the same one — otherwise every port change
/// would log out existing pma sessions.
fn ensure_pma_config(install_dir: &Path, ports: PortConfig) -> anyhow::Result<()> {
    let mut secrets = madi_services::secrets::load(install_dir)?.unwrap_or_default();
    if secrets.pma_blowfish_secret.is_empty() {
        secrets.pma_blowfish_secret = madi_services::secrets::generate_blowfish_secret();
        madi_services::secrets::save(install_dir, &secrets)?;
    }

    let pma_dir = install_dir.join("bin").join("phpmyadmin");
    let conf_out = pma_dir.join("config.inc.php");
    let tmp_dir = pma_dir.join("tmp");

    madi_config_gen::render_pma_config(ports, &secrets.pma_blowfish_secret, &tmp_dir, &conf_out)?;
    Ok(())
}

async fn bridge_progress(app: &AppHandle, slug: &str, mut rx: mpsc::Receiver<Progress>) {
    let mut total: Option<u64> = None;
    while let Some(event) = rx.recv().await {
        let payload = match event {
            Progress::Started { total_bytes } => {
                total = total_bytes;
                InstallProgressEvent {
                    slug: slug.into(),
                    phase: InstallPhase::Downloading,
                    bytes: Some(0),
                    total: total_bytes,
                    message: None,
                }
            }
            Progress::Downloaded { bytes } => InstallProgressEvent {
                slug: slug.into(),
                phase: InstallPhase::Downloading,
                bytes: Some(bytes),
                total,
                message: None,
            },
            Progress::Verifying => InstallProgressEvent {
                slug: slug.into(),
                phase: InstallPhase::Verifying,
                bytes: None,
                total: None,
                message: None,
            },
            // `Extracting` / `Done` here would be redundant: we emit them
            // ourselves around the downloader call to control sequencing.
            Progress::Extracting | Progress::Done => continue,
        };
        emit(app, payload);
    }
}

fn emit(app: &AppHandle, payload: InstallProgressEvent) {
    if let Err(e) = app.emit(INSTALL_EVENT, &payload) {
        tracing::warn!(error = %e, "failed to emit install-progress");
    }
}

fn emit_error(app: &AppHandle, slug: &str, message: String) {
    emit(
        app,
        InstallProgressEvent {
            slug: slug.into(),
            phase: InstallPhase::Error,
            bytes: None,
            total: None,
            message: Some(message),
        },
    );
}
