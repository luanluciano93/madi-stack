//! Tauri commands exposed to the Svelte frontend.
//!
//! Every command returns `Result<T, String>` because Tauri serializes errors
//! as strings on the JS side. We convert from `anyhow::Error` at the boundary.

use madi_core::{Component, PortConfig, ServiceStatus};
use madi_logs::LogLine;
use madi_state_store::{save, Prefs};
use tauri::{AppHandle, Emitter};

use crate::state::AppState;

pub const STATUS_EVENT: &str = "service-status";
/// Per-line event emitted live by the log forwarder spawned in `service_start`.
/// Payload is a single [`LogLine`] with a `slug` field added.
pub const LOG_EVENT: &str = "service-log";

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceStatusEvent {
    pub slug: String,
    pub status: ServiceStatus,
}

/// Simple health-check so the frontend can confirm the backend is alive.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

/// List the 4 components we manage, in display order.
#[tauri::command]
pub fn list_components() -> Vec<ComponentInfo> {
    Component::all()
        .iter()
        .map(|c| ComponentInfo {
            slug: c.slug().into(),
            name: c.display_name().into(),
        })
        .collect()
}

/// Check whether a TCP port on 127.0.0.1 is available.
#[tauri::command]
pub fn port_available(port: u16) -> bool {
    madi_services::is_port_available(port)
}

/// Return `{ free, occupier?, is_self }`. `is_self` is true when the occupier
/// is a MadiStack-managed binary (exe path sits inside `install_dir/bin`) —
/// the UI uses it to render a calm "em uso pelo MadiStack" instead of the
/// red conflict warning.
#[tauri::command]
pub fn port_inspect(port: u16, state: tauri::State<'_, AppState>) -> PortInspectionDto {
    let free = madi_services::is_port_available(port);
    let occupier = if free {
        None
    } else {
        madi_services::port_occupier(port)
    };
    let is_self = occupier
        .as_ref()
        .and_then(|o| o.exe_path.as_deref())
        .is_some_and(|p| p.starts_with(state.supervisor.install_dir().join("bin")));
    PortInspectionDto {
        free,
        occupier,
        is_self,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PortInspectionDto {
    pub free: bool,
    pub occupier: Option<madi_services::PortOccupier>,
    pub is_self: bool,
}

#[tauri::command]
pub async fn service_start(
    component: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<ServiceHandleDto, String> {
    let c = parse_component(&component)?;
    let sup = state.supervisor.clone();
    let h = sup.start(c).await.map_err(|e| e.to_string())?;
    emit_status(&app, c, sup.status(c));

    // Spawn one forwarder per start: subscribe AFTER the service is up so we
    // catch every line, then bridge each broadcast event into a Tauri event.
    // The task self-terminates when the broadcast sender closes (stop_all on
    // shutdown drops the LogBuffer) or on Lagged (subscriber too slow — we
    // log and continue, GUI can re-snapshot via `service_logs`).
    let buf = sup.logs(c);
    let app_for_task = app.clone();
    let slug = c.slug().to_string();
    tokio::spawn(async move {
        let mut rx = buf.subscribe();
        loop {
            match rx.recv().await {
                Ok(line) => {
                    let _ = app_for_task.emit(
                        LOG_EVENT,
                        LogLineEvent {
                            slug: slug.clone(),
                            line,
                        },
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(slug, dropped = n, "log forwarder lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    Ok(ServiceHandleDto {
        slug: c.slug().into(),
        pid: h.pid,
    })
}

/// Snapshot of the per-component log ring buffer with `seq >= since`.
/// Use `since: 0` to fetch all lines the buffer still holds.
#[tauri::command]
pub fn service_logs(
    component: String,
    since: u64,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LogLine>, String> {
    let c = parse_component(&component)?;
    Ok(state.supervisor.logs(c).snapshot_since(since))
}

#[derive(Debug, Clone, serde::Serialize)]
struct LogLineEvent {
    slug: String,
    line: LogLine,
}

#[tauri::command]
pub async fn service_stop(
    component: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let c = parse_component(&component)?;
    let sup = state.supervisor.clone();
    let result = sup.stop(c).await.map_err(|e| e.to_string());
    emit_status(&app, c, sup.status(c));
    result
}

fn emit_status(app: &AppHandle, c: Component, status: ServiceStatus) {
    let _ = app.emit(
        STATUS_EVENT,
        ServiceStatusEvent {
            slug: c.slug().into(),
            status,
        },
    );
}

#[tauri::command]
pub fn service_status(
    component: String,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceStatus, String> {
    let c = parse_component(&component)?;
    Ok(state.supervisor.status(c))
}

fn parse_component(slug: &str) -> Result<Component, String> {
    Component::all()
        .iter()
        .copied()
        .find(|c| c.slug() == slug)
        .ok_or_else(|| format!("unknown component: {slug}"))
}

#[derive(Debug, serde::Serialize)]
pub struct ComponentInfo {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ServiceHandleDto {
    pub slug: String,
    pub pid: u32,
}

// --- Config (ports + prefs) ------------------------------------------------
//
// Reads from and writes to the in-memory `stored` state, and persists to
// `madistack.toml`. Port changes do **not** hot-reload into the supervisor —
// the new values apply on the next stop+start of each service. The frontend
// surfaces that caveat.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfigDto {
    pub ports: PortConfig,
    pub prefs: Prefs,
}

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> AppConfigDto {
    let s = state.stored.read();
    AppConfigDto {
        ports: s.ports,
        prefs: s.prefs.clone(),
    }
}

#[tauri::command]
pub async fn save_config(
    config: AppConfigDto,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    validate_ports(&config.ports)?;
    let install_dir = state.supervisor.install_dir().to_path_buf();
    {
        let mut s = state.stored.write();
        s.ports = config.ports;
        s.prefs = config.prefs;
        save(&state_file_path(), &s).map_err(|e| e.to_string())?;
    }
    // Push the new ports into the supervisor so `preflight_port` and the
    // mysqld/php-cgi launch args see them on the next start. Then re-render
    // nginx.conf / php.ini / my.ini so the on-disk config reflects the
    // saved state — nginx reads its file, not our struct. Config rendering
    // is best-effort: if bin/ is still empty on a fresh install the user
    // hasn't hit the "Baixar tudo" flow yet, which is fine.
    state.supervisor.set_ports(config.ports);
    if let Err(e) = crate::install::render_configs(&install_dir, config.ports) {
        tracing::warn!(error = %e, "save_config: re-render skipped");
    }
    // If nginx is already running, apply the new config live. `-s reload`
    // is a no-op when nginx isn't up, and we log-and-continue on failure —
    // the next start will read the rendered file anyway.
    if let Err(e) = reload_nginx(&install_dir).await {
        tracing::warn!(error = %e, "save_config: nginx reload failed");
    }
    Ok(())
}

fn validate_ports(p: &PortConfig) -> Result<(), String> {
    if p.http == 0 || p.mariadb == 0 || p.php_fcgi == 0 {
        return Err("ports must be non-zero".into());
    }
    if p.http == p.mariadb || p.http == p.php_fcgi || p.mariadb == p.php_fcgi {
        return Err("ports must be distinct".into());
    }
    Ok(())
}

// --- Install / first-run ---------------------------------------------------
//
// Progress streams via the `install-progress` event (see `install.rs`). The
// frontend subscribes once on page load and routes by `slug`.

#[tauri::command]
pub fn component_installed(
    component: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let c = parse_component(&component)?;
    Ok(crate::install::is_installed(
        state.supervisor.install_dir(),
        c,
    ))
}

#[tauri::command]
pub async fn component_install(
    component: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let c = parse_component(&component)?;
    let install_dir = state.supervisor.install_dir().to_path_buf();
    crate::install::install_component(&app, &install_dir, c)
        .await
        .map_err(|e| e.to_string())
}

/// Install all three services sequentially, then phpMyAdmin, then render
/// default configs. Serial on purpose — parallel saves wall time but makes
/// progress UX noisy and hits the same CDNs harder.
#[tauri::command]
pub async fn install_all(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let install_dir = state.supervisor.install_dir().to_path_buf();
    for c in Component::all() {
        crate::install::install_component(&app, &install_dir, *c)
            .await
            .map_err(|e| e.to_string())?;
    }
    let ports = state.stored.read().ports;
    crate::install::render_configs(&install_dir, ports).map_err(|e| e.to_string())?;
    Ok(())
}

// --- Updater ---------------------------------------------------------------
//
// `updater_check` is a one-shot fetch; `updater_apply` streams progress
// through the `update-progress` event (same shape as `install-progress`).

pub const UPDATE_EVENT: &str = "update-progress";

/// After a swap, try to boot the service and confirm it stays up for a few
/// seconds. Catches corrupted/incompatible builds that slipped past SHA256.
/// phpMyAdmin has no process — the signature check (`index.php` present) is
/// enough, so we skip it here.
fn build_smoke_test(
    component: Component,
    supervisor: std::sync::Arc<madi_services::Supervisor>,
) -> Option<madi_updater::SmokeFn> {
    if matches!(component, Component::PhpMyAdmin) {
        return None;
    }
    Some(Box::new(move |_dir, component| {
        Box::pin(async move {
            supervisor
                .start(component)
                .await
                .map_err(|e| format!("start failed: {e}"))?;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let status = supervisor.status(component);
            if status != ServiceStatus::Running {
                // Stop best-effort so the rollback rename isn't blocked by a
                // half-alive process holding the exe.
                let _ = supervisor.stop(component).await;
                return Err(format!("service not running after boot ({status:?})"));
            }
            // Leave it running — the user expected a working update.
            Ok(())
        })
    }))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateStatusDto {
    pub slug: String,
    pub current: Option<String>,
    pub available: String,
    pub update_available: bool,
    /// True when the signature binary is present on disk. Lets the UI say
    /// "instalado (versão desconhecida)" when the install predates the
    /// version-tracking code, instead of the misleading "não instalado".
    pub installed_on_disk: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePhase {
    Downloading,
    Verifying,
    Extracting,
    Done,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateProgressEvent {
    pub slug: String,
    pub phase: UpdatePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[tauri::command]
pub async fn updater_check(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<UpdateStatusDto>, String> {
    let mut installed = state.stored.read().installed.clone();
    let install_dir = state.supervisor.install_dir().to_path_buf();

    // Backfill any missing installed versions by probing the binaries on
    // disk. Catches installs that predate the version-tracking code and
    // lets auto-update resume without a full reinstall. Persists so the
    // probe only runs once per component.
    let mut persisted = false;
    for c in Component::all() {
        if installed.contains_key(c) {
            continue;
        }
        if !crate::install::is_installed(&install_dir, *c) {
            continue;
        }
        let probed: Option<semver::Version> = crate::version_probe::probe(&install_dir, *c).await;
        if let Some(v) = probed {
            installed.insert(*c, v.clone());
            let mut s = state.stored.write();
            s.installed.insert(*c, v);
            persisted = true;
        }
    }
    if persisted {
        let s = state.stored.read();
        if let Err(e) = madi_state_store::save(&state_file_path(), &s) {
            tracing::warn!(error = %e, "updater_check: failed to persist probed versions");
        }
    }

    let client = madi_sources::build_client();
    let statuses = madi_updater::check_all(&client, |c| installed.get(&c).cloned())
        .await
        .map_err(|e| e.to_string())?;
    Ok(statuses
        .into_iter()
        .map(|s| UpdateStatusDto {
            slug: s.component.slug().into(),
            current: s.current.map(|v| v.to_string()),
            available: s.available.to_string(),
            update_available: s.update_available,
            installed_on_disk: crate::install::is_installed(&install_dir, s.component),
        })
        .collect())
}

#[tauri::command]
pub async fn updater_apply(
    component: String,
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let c = parse_component(&component)?;
    let install_dir = state.supervisor.install_dir().to_path_buf();

    // Stop the service before renaming its directory — Windows locks
    // running exes. Best-effort: if it wasn't running, `stop` errors and we
    // just move on.
    let _ = state.supervisor.stop(c).await;

    // Bridge downloader progress into `update-progress` events.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<madi_downloader::Progress>(64);
    let slug_for_task = c.slug().to_string();
    let app_for_task = app.clone();
    let bridge = tokio::spawn(async move {
        let mut total: Option<u64> = None;
        while let Some(ev) = rx.recv().await {
            let payload = match ev {
                madi_downloader::Progress::Started { total_bytes } => {
                    total = total_bytes;
                    UpdateProgressEvent {
                        slug: slug_for_task.clone(),
                        phase: UpdatePhase::Downloading,
                        bytes: Some(0),
                        total: total_bytes,
                        message: None,
                    }
                }
                madi_downloader::Progress::Downloaded { bytes } => UpdateProgressEvent {
                    slug: slug_for_task.clone(),
                    phase: UpdatePhase::Downloading,
                    bytes: Some(bytes),
                    total,
                    message: None,
                },
                madi_downloader::Progress::Verifying => UpdateProgressEvent {
                    slug: slug_for_task.clone(),
                    phase: UpdatePhase::Verifying,
                    bytes: None,
                    total: None,
                    message: None,
                },
                madi_downloader::Progress::Extracting => UpdateProgressEvent {
                    slug: slug_for_task.clone(),
                    phase: UpdatePhase::Extracting,
                    bytes: None,
                    total: None,
                    message: None,
                },
                madi_downloader::Progress::Done => continue,
            };
            let _ = app_for_task.emit(UPDATE_EVENT, &payload);
        }
    });

    let client = madi_sources::build_client();
    let smoke = build_smoke_test(c, state.supervisor.clone());
    let apply_res =
        madi_updater::apply(&client, &install_dir, c, Some(tx.clone()), None, smoke).await;
    drop(tx);
    let _ = bridge.await;

    match apply_res {
        Ok(new_version) => {
            // Persist the new version so a later `updater_check` shows it as
            // current. Errors here are non-fatal: the binary is already swapped.
            {
                let mut s = state.stored.write();
                s.installed.insert(c, new_version.clone());
                if let Err(e) = madi_state_store::save(&state_file_path(), &s) {
                    tracing::warn!(error = %e, "updater: failed to persist version");
                }
            }
            let _ = app.emit(
                UPDATE_EVENT,
                UpdateProgressEvent {
                    slug: c.slug().into(),
                    phase: UpdatePhase::Done,
                    bytes: None,
                    total: None,
                    message: Some(format!("v{new_version}")),
                },
            );
            Ok(new_version.to_string())
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = app.emit(
                UPDATE_EVENT,
                UpdateProgressEvent {
                    slug: c.slug().into(),
                    phase: UpdatePhase::Error,
                    bytes: None,
                    total: None,
                    message: Some(msg.clone()),
                },
            );
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn updater_rollback(
    component: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let c = parse_component(&component)?;
    let install_dir = state.supervisor.install_dir().to_path_buf();
    let _ = state.supervisor.stop(c).await;
    madi_updater::rollback(&install_dir, c)
        .await
        .map_err(|e| e.to_string())
}

// --- Firewall --------------------------------------------------------------
//
// All three rules (nginx, mariadb, php-cgi) are pushed in a single COM
// session so UAC (if the process isn't already elevated) prompts at most
// once. The frontend surfaces a "Recriar regras de firewall" button in
// Configurações that hits `firewall_ensure_rules`; `firewall_rules_status`
// feeds the badge.

#[derive(Debug, Clone, serde::Serialize)]
pub struct FirewallRulesStatus {
    pub nginx: bool,
    pub mariadb: bool,
    pub php_fcgi: bool,
}

const FIREWALL_RULE_NGINX: &str = "MadiStack — Nginx";
const FIREWALL_RULE_MARIADB: &str = "MadiStack — MariaDB";
const FIREWALL_RULE_PHP: &str = "MadiStack — PHP FastCGI";

#[tauri::command]
pub fn firewall_ensure_rules(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let install_dir = state.supervisor.install_dir().to_path_buf();
    let rules = madi_firewall::madistack_rules(&install_dir);

    // Route through the elevated helper so the user sees one UAC prompt
    // instead of a raw `0x80070005 Access Denied`. The main app stays
    // un-elevated — only the helper inherits admin rights, and only for the
    // lifetime of this single call.
    let helper = system_helper_path().ok_or_else(|| {
        "helper binary madistack-system-helper.exe not found next to the main \
         executable — reinstall MadiStack"
            .to_string()
    })?;
    madi_firewall::run_elevated_ensure(&helper, &rules).map_err(|e| match e {
        madi_firewall::ElevatedError::UserCancelled => {
            "A permissão foi negada no prompt do Windows. Tente novamente e \
             clique em \"Sim\" para autorizar a criação das regras."
                .into()
        }
        other => other.to_string(),
    })
}

/// Resolve the helper binary sitting next to the main executable. Tauri's
/// `externalBin` bundler may either strip the target triple suffix or keep
/// it depending on version, so we try both layouts before giving up.
fn system_helper_path() -> Option<std::path::PathBuf> {
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;

    let plain = dir.join("madistack-system-helper.exe");
    if plain.is_file() {
        return Some(plain);
    }

    // Bundled variant: `madistack-system-helper-<triple>.exe`. Glob the dir
    // for anything matching the prefix so we don't have to hardcode the
    // target triple at runtime.
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with("madistack-system-helper-")
            && std::path::Path::new(name_str)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        {
            return Some(entry.path());
        }
    }
    None
}

#[tauri::command]
pub fn firewall_remove_rules() -> Result<(), String> {
    for name in [
        FIREWALL_RULE_NGINX,
        FIREWALL_RULE_MARIADB,
        FIREWALL_RULE_PHP,
    ] {
        madi_firewall::remove_rule(name).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn firewall_rules_status() -> FirewallRulesStatus {
    // A COM failure on one probe shouldn't blank the whole view — report each
    // rule independently, treating probe errors as "absent".
    FirewallRulesStatus {
        nginx: madi_firewall::rule_exists(FIREWALL_RULE_NGINX).unwrap_or(false),
        mariadb: madi_firewall::rule_exists(FIREWALL_RULE_MARIADB).unwrap_or(false),
        php_fcgi: madi_firewall::rule_exists(FIREWALL_RULE_PHP).unwrap_or(false),
    }
}

/// Mirror of the logic in [`crate::state::AppState::new`] — keeps the
/// portable rule that the state file lives next to the `.exe`. Duplicated
/// rather than exposed so the Tauri command doesn't pull `state.rs` into
/// its dependency graph.
fn state_file_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("madistack.toml")
}

// --- Virtual hosts --------------------------------------------------------
//
// A "site" is a subfolder under `www/`. Enabling one renders an nginx vhost
// into `config/sites-enabled/<name>.conf`, adds `127.0.0.1 <name>.test` to
// the system hosts file (via the elevated helper), and reloads nginx.

#[derive(Debug, Clone, serde::Serialize)]
pub struct VhostDto {
    pub name: String,
    pub hostname: String,
    pub enabled: bool,
    /// Cert + key present under `config/certs/<name>/`. Independent from
    /// `enabled`: a site can be disabled but keep its cert so re-enabling
    /// with HTTPS is instant.
    pub ssl: bool,
}

const VHOST_TAG_PREFIX: &str = "vhost:";

/// Accept only ASCII alphanumerics, hyphen and underscore so the name is
/// safe as both a filename segment and a DNS label.
fn validate_vhost_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 63 {
        return Err("nome do site deve ter entre 1 e 63 caracteres".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("nome do site só pode conter letras, números, hífen e sublinhado".into());
    }
    Ok(())
}

#[tauri::command]
pub fn vhost_list(state: tauri::State<'_, AppState>) -> Vec<VhostDto> {
    let install_dir = state.supervisor.install_dir();
    let www = install_dir.join("www");
    let sites_enabled = install_dir.join("config").join("sites-enabled");

    // No www/ yet → no sites. Don't error; the UI will just show an empty
    // state with a hint to create a folder.
    let Ok(entries) = std::fs::read_dir(&www) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        };
        // Skip dotfiles and anything with characters we wouldn't accept at
        // enable time — keeps the list aligned with what `vhost_enable`
        // can actually process.
        if name.starts_with('.') || validate_vhost_name(name).is_err() {
            continue;
        }
        let enabled = sites_enabled.join(format!("{name}.conf")).is_file();
        let ssl = install_dir
            .join("config")
            .join("certs")
            .join(name)
            .join("cert.pem")
            .is_file();
        out.push(VhostDto {
            name: name.to_string(),
            hostname: format!("{name}.test"),
            enabled,
            ssl,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[tauri::command]
pub async fn vhost_enable(
    name: String,
    https: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    validate_vhost_name(&name)?;

    let install_dir = state.supervisor.install_dir().to_path_buf();
    let site_root = install_dir.join("www").join(&name);
    if !site_root.is_dir() {
        return Err(format!("pasta www/{name}/ não existe"));
    }

    // When the user ticks HTTPS, make sure mkcert is available and a cert
    // exists for this hostname before writing the nginx conf — otherwise
    // nginx fails to start with "cannot load certificate" and the user is
    // left with a broken site.
    let cert_dir = install_dir.join("config").join("certs").join(&name);
    let ssl = if https {
        ensure_mkcert_ready(&install_dir).await?;
        if !cert_dir.join("cert.pem").is_file() {
            crate::mkcert::issue(&install_dir, &cert_dir, &format!("{name}.test"))
                .await
                .map_err(|e| e.to_string())?;
        }
        Some(madi_config_gen::SiteSsl {
            cert_path: &cert_dir.join("cert.pem"),
            key_path: &cert_dir.join("key.pem"),
        })
    } else {
        None
    };

    // Render `config/sites-enabled/<name>.conf` pointing at the per-site
    // document root. Reuses the same port/php config as the main server —
    // different ports per vhost is a v2 feature.
    let ports = state.stored.read().ports;
    let php_exts = madi_config_gen::DEFAULT_PHP_EXTENSIONS;
    let ctx = madi_config_gen::RenderContext {
        install_dir: &install_dir,
        ports,
        document_root: &site_root,
        php_extensions: php_exts,
    };
    let conf_path = install_dir
        .join("config")
        .join("sites-enabled")
        .join(format!("{name}.conf"));
    madi_config_gen::render_site(&ctx, &name, ssl, &conf_path).map_err(|e| e.to_string())?;

    // Add the hosts-file entry via the elevated helper. Tag uniquely so we
    // can later remove just this one without disturbing other vhosts.
    let helper = system_helper_path()
        .ok_or_else(|| "helper binário não encontrado — reinstale o MadiStack".to_string())?;
    let entry = madi_firewall::HostEntry::new(
        "127.0.0.1",
        format!("{name}.test"),
        format!("{VHOST_TAG_PREFIX}{name}"),
    );
    madi_firewall::run_elevated_hosts_edit(&helper, &[entry], &[]).map_err(|e| match e {
        madi_firewall::ElevatedError::UserCancelled => {
            // Roll back the generated .conf so the UI doesn't show the site
            // as enabled when DNS won't resolve it.
            let _ = std::fs::remove_file(&conf_path);
            "A permissão foi negada. O site não foi ativado.".to_string()
        }
        other => {
            let _ = std::fs::remove_file(&conf_path);
            other.to_string()
        }
    })?;

    // Ask nginx to pick up the new vhost file. Errors here are surfaced but
    // non-fatal from the user's perspective — the next service start will
    // load the config anyway.
    if let Err(e) = reload_nginx(&install_dir).await {
        tracing::warn!(site = %name, error = %e, "vhost_enable: nginx reload failed");
    }
    Ok(())
}

#[tauri::command]
pub async fn vhost_disable(name: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    validate_vhost_name(&name)?;

    let install_dir = state.supervisor.install_dir().to_path_buf();
    let conf_path = install_dir
        .join("config")
        .join("sites-enabled")
        .join(format!("{name}.conf"));

    // Remove the hosts entry first — if the user cancels UAC here we don't
    // want to have already broken the site (the .conf is still there and
    // nginx still knows how to serve it).
    let helper = system_helper_path()
        .ok_or_else(|| "helper binário não encontrado — reinstale o MadiStack".to_string())?;
    let tag = format!("{VHOST_TAG_PREFIX}{name}");
    madi_firewall::run_elevated_hosts_edit(&helper, &[], std::slice::from_ref(&tag)).map_err(
        |e| match e {
            madi_firewall::ElevatedError::UserCancelled => {
                "A permissão foi negada. O site não foi desativado.".to_string()
            }
            other => other.to_string(),
        },
    )?;

    // Best-effort removal of the conf file — if the user deleted it manually
    // we don't want to error out after the hosts edit already succeeded.
    let _ = std::fs::remove_file(&conf_path);

    if let Err(e) = reload_nginx(&install_dir).await {
        tracing::warn!(site = %name, error = %e, "vhost_disable: nginx reload failed");
    }
    Ok(())
}

/// Re-render the on-disk configs from the embedded Tera templates. Called
/// on boot so template changes ship as bug fixes without requiring the user
/// to click Salvar.
pub fn render_configs_at_boot(
    install_dir: &std::path::Path,
    ports: PortConfig,
) -> anyhow::Result<()> {
    crate::install::render_configs(install_dir, ports)
}

/// Download mkcert if missing and ensure the local root CA is trusted.
/// Idempotent — subsequent HTTPS toggles skip both steps.
async fn ensure_mkcert_ready(install_dir: &std::path::Path) -> Result<(), String> {
    crate::mkcert::ensure_downloaded(install_dir)
        .await
        .map_err(|e| format!("falha ao baixar mkcert: {e}"))?;

    if !crate::mkcert::ca_installed(install_dir) {
        let helper =
            system_helper_path().ok_or_else(|| "helper binário não encontrado".to_string())?;
        let exe = crate::mkcert::mkcert_exe(install_dir);
        madi_firewall::run_elevated_mkcert_install(&helper, &exe).map_err(|e| match e {
            madi_firewall::ElevatedError::UserCancelled => {
                "A permissão foi negada. O HTTPS não pôde ser configurado.".to_string()
            }
            other => other.to_string(),
        })?;
        if let Err(e) = crate::mkcert::mark_ca_installed(install_dir) {
            tracing::warn!(error = %e, "failed to persist mkcert CA marker");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MkcertStatusDto {
    /// True when `bin/mkcert/mkcert.exe` is on disk.
    pub binary_present: bool,
    /// True when we've already run `mkcert -install` successfully.
    pub ca_installed: bool,
}

#[tauri::command]
pub fn mkcert_status(state: tauri::State<'_, AppState>) -> MkcertStatusDto {
    let dir = state.supervisor.install_dir();
    MkcertStatusDto {
        binary_present: crate::mkcert::mkcert_exe(dir).is_file(),
        ca_installed: crate::mkcert::ca_installed(dir),
    }
}

/// Run `nginx.exe -s reload` against the running supervised nginx. If nginx
/// is not running, this is a no-op — the next `start` will read the updated
/// config.
async fn reload_nginx(install_dir: &std::path::Path) -> Result<(), String> {
    let nginx_dir = install_dir.join("bin").join("nginx");
    let exe = nginx_dir.join("nginx.exe");
    if !exe.is_file() {
        return Ok(());
    }
    let conf = install_dir.join("config").join("nginx.conf");
    let output = tokio::process::Command::new(&exe)
        .arg("-p")
        .arg(&nginx_dir)
        .arg("-c")
        .arg(&conf)
        .arg("-s")
        .arg("reload")
        .output()
        .await
        .map_err(|e| format!("spawn nginx -s reload: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "nginx -s reload falhou ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
