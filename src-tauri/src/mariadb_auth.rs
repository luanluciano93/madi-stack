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

// --- Skip-grant-tables recovery -------------------------------------------
//
// "I lost the password entirely" escape hatch: the user can't type the
// current root password (forgot it, never knew it), so [`save_password`]
// is unreachable. The canonical recovery is the documented MariaDB dance:
// stop the running mysqld, start a private one with `--skip-grant-tables`
// (which disables the privilege system), connect as root with no password,
// FLUSH PRIVILEGES + ALTER USER to set a new value, shut the recovery
// server down, then bring the supervised mysqld back up.
//
// [`reset_via_skip_grant`] handles the inner two phases (skip-grant +
// ALTER); the surrounding stop/start/secrets-write is orchestrated by the
// `mariadb_password_reset` Tauri command so it can reuse the supervisor's
// stop/start methods directly.

#[cfg(windows)]
const SKIP_GRANT_BOOT_TIMEOUT: Duration = Duration::from_secs(15);

/// Spawn a private mysqld with `--skip-grant-tables --bind-address=127.0.0.1`,
/// set root's password to `new_password`, then graceful-shutdown the
/// recovery server. Caller MUST have stopped the supervised mysqld first
/// (otherwise this fails with "port busy" before doing anything risky).
///
/// Returns one of the same string-tagged errors the rest of the module
/// uses (`"empty_password"`, `"invalid_password"`, `"binary_missing"`,
/// `"skip_grant_boot_timeout"`, `"alter_failed:<stderr>"`,
/// `"spawn:<io-error>"`).
pub async fn reset_via_skip_grant(
    install_dir: &Path,
    port: u16,
    new_password: &str,
) -> Result<(), String> {
    if new_password.is_empty() {
        return Err("empty_password".into());
    }
    // Reject anything that would either break the secrets TOML round-trip
    // or require us to escape the value when interpolating into the
    // ALTER USER statement below.
    if new_password.len() > 256
        || new_password
            .chars()
            .any(|c| c == '\'' || c == '\\' || c.is_control())
    {
        return Err("invalid_password".into());
    }

    #[cfg(not(windows))]
    {
        let _ = (install_dir, port);
        return Err("binary_missing".into());
    }

    #[cfg(windows)]
    {
        let mysqld = install_dir
            .join("bin")
            .join("mariadb")
            .join("bin")
            .join("mysqld.exe");
        let mysql = install_dir
            .join("bin")
            .join("mariadb")
            .join("bin")
            .join("mysql.exe");
        let config = install_dir.join("config").join("my.ini");
        if !mysqld.is_file() || !mysql.is_file() {
            return Err("binary_missing".into());
        }

        tracing::info!(port, "mariadb_auth: launching skip-grant-tables mysqld");

        // Bind explicitly to 127.0.0.1 even when my.ini has 0.0.0.0 — we
        // don't want a 5-second window where the password-less recovery
        // server is reachable from the LAN.
        let mut child = Command::new(&mysqld)
            .arg(format!("--defaults-file={}", config.display()))
            .arg(format!("--port={port}"))
            .arg("--bind-address=127.0.0.1")
            .arg("--skip-grant-tables")
            .arg("--console")
            .arg("--standalone")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("spawn:{e}"))?;

        // Poll for readiness — skip-grant accepts any password, so a
        // successful mysql connection is also our "mysqld is up" signal.
        if !wait_until_skip_grant_ready(&mysql, port, SKIP_GRANT_BOOT_TIMEOUT).await {
            tracing::warn!("mariadb_auth: skip-grant mysqld did not become ready");
            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(5), child.wait()).await;
            return Err("skip_grant_boot_timeout".into());
        }

        // FLUSH PRIVILEGES first so MariaDB reloads the grant tables and
        // accepts ALTER USER. Then update both the localhost row (always
        // present) and the 127.0.0.1 row (only on some installs). The
        // `IF EXISTS` clause makes the second statement portable across
        // MariaDB versions where the row may or may not exist.
        let pw = new_password;
        let sql = format!(
            "FLUSH PRIVILEGES; \
             ALTER USER 'root'@'localhost' IDENTIFIED BY '{pw}'; \
             ALTER USER IF EXISTS 'root'@'127.0.0.1' IDENTIFIED BY '{pw}'; \
             FLUSH PRIVILEGES;"
        );

        let alter = Command::new(&mysql)
            .arg("-u")
            .arg("root")
            .arg("-h")
            .arg("127.0.0.1")
            .arg("-P")
            .arg(port.to_string())
            .arg("--protocol=tcp")
            .arg("-e")
            .arg(&sql)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .await
            .map_err(|e| format!("spawn:{e}"))?;

        teardown_recovery_mysqld(
            install_dir,
            port,
            alter.status.success().then_some(new_password),
            &mut child,
        )
        .await;

        if !alter.status.success() {
            let stderr = String::from_utf8_lossy(&alter.stderr).trim().to_string();
            return Err(format!("alter_failed:{stderr}"));
        }
        tracing::info!("mariadb_auth: root password reset via skip-grant-tables");
        Ok(())
    }
}

/// Poll `mysql -u root` (no password) every 500ms until it succeeds or
/// `deadline` elapses. Skip-grant mode accepts any credentials, so a
/// successful connection means mysqld is fully up.
#[cfg(windows)]
async fn wait_until_skip_grant_ready(mysql: &Path, port: u16, deadline: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        let res = Command::new(mysql)
            .arg("-u")
            .arg("root")
            .arg("-h")
            .arg("127.0.0.1")
            .arg("-P")
            .arg(port.to_string())
            .arg("--protocol=tcp")
            .arg("--connect-timeout=1")
            .arg("-N")
            .arg("-e")
            .arg("SELECT 1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .await;
        if res.is_ok_and(|s| s.success()) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// Stop the recovery mysqld. Prefer a graceful `mysqladmin shutdown` with
/// the freshly-set password — `TerminateProcess` works but leaves InnoDB
/// doing crash recovery on the next boot, costing a few seconds users can
/// feel. Falls back to `start_kill` when the password isn't usable yet
/// (ALTER failed) or mysqladmin is missing.
#[cfg(windows)]
async fn teardown_recovery_mysqld(
    install_dir: &Path,
    port: u16,
    password: Option<&str>,
    child: &mut tokio::process::Child,
) {
    let mysqladmin = install_dir
        .join("bin")
        .join("mariadb")
        .join("bin")
        .join("mysqladmin.exe");
    let mut graceful = false;
    if let Some(pw) = password {
        if mysqladmin.is_file() {
            let res = Command::new(&mysqladmin)
                .env("MYSQL_PWD", pw)
                .arg("-u")
                .arg("root")
                .arg("-h")
                .arg("127.0.0.1")
                .arg("-P")
                .arg(port.to_string())
                .arg("--protocol=tcp")
                .arg("--connect-timeout=2")
                .arg("shutdown")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .await;
            graceful = res.is_ok_and(|s| s.success());
        }
    }
    if !graceful {
        let _ = child.start_kill();
    }
    let _ = timeout(Duration::from_secs(10), child.wait()).await;
}

/// Persist `new_password` to `madistack-secrets.toml` without verifying
/// against the live mysqld. Used right after [`reset_via_skip_grant`]
/// succeeds — at that point we know the value matches the DB because we
/// just set it, so re-running [`try_login`] would be wasted work.
pub fn save_secret_unverified(install_dir: &Path, new_password: &str) -> Result<(), String> {
    let mut s = secrets::load(install_dir)
        .map_err(|e| format!("read_secrets:{e}"))?
        .unwrap_or_default();
    s.mariadb_root_password = new_password.to_string();
    secrets::save(install_dir, &s).map_err(|e| format!("write_secrets:{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reset_rejects_empty_password() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            reset_via_skip_grant(dir.path(), 3306, "").await.unwrap_err(),
            "empty_password"
        );
    }

    #[tokio::test]
    async fn reset_rejects_unsafe_chars() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["pw'with-quote", "pw\\with-back", "pw\nwith-ctrl"] {
            let err = reset_via_skip_grant(dir.path(), 3306, bad)
                .await
                .unwrap_err();
            assert_eq!(err, "invalid_password", "expected {bad:?} to fail");
        }
    }

    #[tokio::test]
    async fn reset_returns_binary_missing_when_mysqld_absent() {
        let dir = tempfile::tempdir().unwrap();
        // No bin/mariadb/ at all → binary_missing, not panic.
        assert_eq!(
            reset_via_skip_grant(dir.path(), 3306, "abcDEF12345678901234567")
                .await
                .unwrap_err(),
            "binary_missing"
        );
    }

    #[test]
    fn save_secret_unverified_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        save_secret_unverified(dir.path(), "newSecret123").unwrap();
        let loaded = secrets::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.mariadb_root_password, "newSecret123");
    }
}
