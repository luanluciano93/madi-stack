//! Detect and recover from MariaDB root password drift.
//!
//! The password lives in two places that have to stay aligned:
//! 1. The `mysql.user` row inside MariaDB itself (set at bootstrap time
//!    and modifiable by anyone with root, e.g. via phpMyAdmin's "change
//!    password" form).
//! 2. The `mariadb_root_password` field of `madistack-secrets.toml`,
//!    consumed by the supervisor (`mysqladmin shutdown`), the backup
//!    module (`mysqldump --password=...`), and the Configurações
//!    "Revelar senha" button.
//!
//! When the user changes the password through phpMyAdmin we have no way
//! to learn the new value — passwords are stored hashed in MariaDB and
//! we don't proxy SQL traffic. Symptom: graceful shutdowns silently
//! fall back to TerminateProcess, backups fail mid-spawn, and the UI
//! shows a stale value.
//!
//! This module probes the running mysqld with the secrets-file password
//! and reports drift to the frontend, which can then prompt the user
//! for the real password and call back into [`save_password`] to
//! re-sync the secrets file. We never push the secrets value into
//! MariaDB automatically — that would silently re-establish a known
//! password on a multi-user install where someone may have rotated it
//! deliberately.

use std::path::Path;
use std::time::Duration;

use madi_services::secrets;
use tokio::process::Command;
use tokio::time::timeout;

// `tokio::process::Command::creation_flags` is an inherent method on
// Windows — no need to import `std::os::windows::process::CommandExt`.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Outcome of a single probe.
///
/// Serialised with `tag = "status"` so the TypeScript side gets a
/// discriminated union (`{ status: 'drift' }` etc.) without leaking
/// Rust-specific shapes.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PasswordStatus {
    /// Stored password authenticates — nothing to do.
    InSync,
    /// `mysql -e "SELECT 1"` returned `Access denied`. The secrets file
    /// is stale; UI should prompt the user to type the real one.
    Drift,
    /// MariaDB isn't reachable on the configured port. Either the
    /// service is stopped or `mysqld` is mid-startup. UI hides the
    /// drift banner — there's nothing actionable yet.
    Unreachable,
    /// `madistack-secrets.toml` is missing the password (pre-bootstrap
    /// install). UI hides the banner.
    NoSecret,
    /// Couldn't run the probe at all (mysql.exe missing on a corrupt
    /// install, spawn failure, …). Best-effort: UI hides the banner
    /// and we log details on the backend side.
    ProbeError,
}

/// Probe the running mysqld with the password currently in the secrets
/// file. The result is intentionally a best-effort summary — the caller
/// is expected to react informationally, not to abort flows.
///
/// `port` should come from the live supervisor [`PortConfig`], not from
/// `madistack.toml` directly, so an in-flight port change applies.
pub async fn check_password(install_dir: &Path, port: u16) -> PasswordStatus {
    let stored = match secrets::load(install_dir) {
        Ok(Some(s)) if !s.mariadb_root_password.is_empty() => s.mariadb_root_password,
        Ok(_) => return PasswordStatus::NoSecret,
        Err(e) => {
            tracing::warn!(error = %e, "mariadb_auth: failed to load secrets");
            return PasswordStatus::ProbeError;
        }
    };

    match try_login(install_dir, port, &stored).await {
        Ok(LoginOutcome::Ok) => PasswordStatus::InSync,
        Ok(LoginOutcome::AccessDenied) => PasswordStatus::Drift,
        Ok(LoginOutcome::Unreachable) => PasswordStatus::Unreachable,
        Err(e) => {
            tracing::warn!(error = %e, "mariadb_auth: probe spawn failed");
            PasswordStatus::ProbeError
        }
    }
}

/// Update `madistack-secrets.toml` with `password` after confirming it
/// authenticates against the running mysqld. Returns the same error key
/// the UI expects (`"access_denied"`, `"unreachable"`, …) so the Svelte
/// side can localise without parsing strings.
///
/// Intentionally does *not* run any SQL on its own — the caller already
/// has the working password from the user. Touching the DB here would
/// turn this into a "change password" flow with broader semantics.
pub async fn save_password(install_dir: &Path, port: u16, password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err("empty_password".into());
    }
    match try_login(install_dir, port, password).await {
        Ok(LoginOutcome::Ok) => {}
        Ok(LoginOutcome::AccessDenied) => return Err("access_denied".into()),
        Ok(LoginOutcome::Unreachable) => return Err("unreachable".into()),
        Err(e) => {
            tracing::warn!(error = %e, "mariadb_auth: probe spawn failed during save");
            return Err("probe_error".into());
        }
    }

    let mut s = secrets::load(install_dir)
        .map_err(|e| format!("read_secrets:{e}"))?
        .unwrap_or_default();
    s.mariadb_root_password = password.to_string();
    secrets::save(install_dir, &s).map_err(|e| format!("write_secrets:{e}"))?;
    Ok(())
}

#[derive(Debug)]
enum LoginOutcome {
    Ok,
    /// MariaDB returned 1045 — user/password mismatch.
    AccessDenied,
    /// TCP refused, `mysqld` not listening yet, port mismatch, etc.
    Unreachable,
}

/// Spawn `mysql.exe -u root -h 127.0.0.1 -P {port} -e "SELECT 1"` with
/// the password supplied via `MYSQL_PWD` (matches the convention in
/// `backup.rs` so the password never appears in `tasklist`).
async fn try_login(
    install_dir: &Path,
    port: u16,
    password: &str,
) -> Result<LoginOutcome, std::io::Error> {
    let mysql = install_dir
        .join("bin")
        .join("mariadb")
        .join("bin")
        .join("mysql.exe");
    if !mysql.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "mysql.exe not found",
        ));
    }

    let mut cmd = Command::new(&mysql);
    cmd.env("MYSQL_PWD", password)
        .arg("-u")
        .arg("root")
        .arg("-h")
        .arg("127.0.0.1")
        .arg("-P")
        .arg(port.to_string())
        .arg("--connect-timeout=2")
        .arg("--batch")
        .arg("--skip-column-names")
        .arg("-e")
        .arg("SELECT 1;");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    // 5s ceiling — `--connect-timeout=2` already bounds the network
    // wait, but a wedged mysqld can still hang on auth. We don't want
    // the UI banner to feel laggy.
    let output = timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "mysql probe timed out"))??;

    if output.status.success() {
        return Ok(LoginOutcome::Ok);
    }

    // mysql exits 1 for both auth and connect failures — disambiguate
    // by inspecting stderr. Tags are stable across MariaDB versions.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Access denied") {
        Ok(LoginOutcome::AccessDenied)
    } else if stderr.contains("Can't connect")
        || stderr.contains("Connection refused")
        || stderr.contains("Lost connection")
    {
        Ok(LoginOutcome::Unreachable)
    } else {
        // Unknown failure — most often "Got timeout reading communication
        // packets" during a port change. Treat as Unreachable so the
        // UI doesn't show a misleading drift banner.
        tracing::debug!(stderr = %stderr.trim(), "mariadb_auth: unclassified mysql failure");
        Ok(LoginOutcome::Unreachable)
    }
}
