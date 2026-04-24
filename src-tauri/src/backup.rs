//! MariaDB backup module — dumps a single database to a gzipped SQL file.
//!
//! Design notes:
//! - We drive the bundled `mysqldump.exe` (ships inside `bin/mariadb/bin/`)
//!   rather than implementing the dump protocol. It's the canonical tool
//!   and produces a file that any MariaDB/MySQL CLI can restore later.
//! - Password is passed via the `MYSQL_PWD` env var, not the command line,
//!   so it doesn't leak in `tasklist` output. MariaDB's client library
//!   picks it up transparently.
//! - Output is streamed from `mysqldump` stdout directly into a gzip
//!   encoder, which writes to disk. No full buffering — a 1 GB database
//!   doesn't balloon RAM usage. Progress events fire every ~250ms so the
//!   UI shows forward motion even during long dumps.
//! - We never write the dump to a temp file first: the common failure
//!   mode (mysqldump exits non-zero) is detected by the child exit status,
//!   and the partial file is removed right away.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{ChildStdout, Command};

pub const BACKUP_EVENT: &str = "backup-progress";

/// Lifecycle of a running backup job. The frontend uses it to drive the
/// progress bar and to know when to refresh the backups list.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupPhase {
    Starting,
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupProgressEvent {
    pub database: String,
    pub phase: BackupPhase,
    /// Bytes written to the gzipped file so far. `None` for phases where
    /// it doesn't apply (e.g. Starting).
    pub bytes: Option<u64>,
    /// Free-form message. Populated on `Error` with the mysqldump stderr
    /// tail so the UI can show something actionable.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub filename: String,
    pub database: String,
    /// Seconds since the Unix epoch. Frontend formats the local date.
    pub created_at_secs: u64,
    pub size_bytes: u64,
}

/// Databases shipped with MariaDB/MySQL that we hide from the backup UI.
/// Dumping `mysql` or `performance_schema` is rarely what the user wants
/// and can trip on permissions or on the DB being in use.
const SYSTEM_DATABASES: &[&str] = &[
    "mysql",
    "information_schema",
    "performance_schema",
    "sys",
];

fn mysqldump_path(install_dir: &Path) -> PathBuf {
    install_dir
        .join("bin")
        .join("mariadb")
        .join("bin")
        .join("mysqldump.exe")
}

fn mysql_path(install_dir: &Path) -> PathBuf {
    install_dir
        .join("bin")
        .join("mariadb")
        .join("bin")
        .join("mysql.exe")
}

fn backups_dir(install_dir: &Path) -> PathBuf {
    install_dir.join("data").join("backups")
}

/// Replace anything outside `[a-zA-Z0-9_-]` with `_`. MariaDB allows
/// spaces and special chars in database names but those are noise in a
/// filename — and keeping the sanitizer strict means we don't have to
/// worry about shell/path escaping when the UI shows the result.
fn sanitize_for_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Read the root password from the local secrets store. Returns an error
/// (not `Ok(None)`) when absent, because backup is meaningless without it
/// and the UI should surface the situation directly.
fn load_root_password(install_dir: &Path) -> Result<String, String> {
    let secrets = madi_services::secrets::load(install_dir)
        .map_err(|e| format!("failed to read secrets: {e}"))?
        .ok_or_else(|| "MariaDB root password not yet generated".to_string())?;
    if secrets.mariadb_root_password.is_empty() {
        return Err("MariaDB root password is empty".into());
    }
    Ok(secrets.mariadb_root_password)
}

/// List user databases via `mysql -e "SHOW DATABASES"`. System schemas
/// are filtered out — the UI exposes only what a user would reasonably
/// want to back up.
pub async fn list_databases(install_dir: &Path, port: u16) -> Result<Vec<String>, String> {
    let password = load_root_password(install_dir)?;
    let mysql = mysql_path(install_dir);
    if !mysql.is_file() {
        return Err(format!("mysql.exe not found at {}", mysql.display()));
    }

    let output = Command::new(&mysql)
        .env("MYSQL_PWD", &password)
        .arg("-u")
        .arg("root")
        .arg("-h")
        .arg("127.0.0.1")
        .arg("-P")
        .arg(port.to_string())
        .arg("--batch")
        .arg("--skip-column-names")
        .arg("-e")
        .arg("SHOW DATABASES;")
        .output()
        .await
        .map_err(|e| format!("failed to spawn mysql: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mysql failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let dbs: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| !SYSTEM_DATABASES.contains(s))
        .map(String::from)
        .collect();
    Ok(dbs)
}

/// Enumerate `.sql.gz` files under `data/backups/`. Returned newest-first
/// so the UI can render them without sorting client-side.
pub fn list_backups(install_dir: &Path) -> Result<Vec<BackupInfo>, String> {
    let dir = backups_dir(install_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read_dir failed: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !filename.ends_with(".sql.gz") {
            continue;
        }
        // Filename format: `<sanitized_db>_<unix_secs>.sql.gz` — split from
        // the right to recover both, falling back gracefully on malformed
        // names (user-dropped files).
        let stem = &filename[..filename.len() - ".sql.gz".len()];
        let (database, created_at_secs) = match stem.rsplit_once('_') {
            Some((db, ts)) => (db.to_string(), ts.parse::<u64>().unwrap_or(0)),
            None => (stem.to_string(), 0),
        };
        let size_bytes = entry.metadata().map_or(0, |m| m.len());
        out.push(BackupInfo {
            filename: filename.to_string(),
            database,
            created_at_secs,
            size_bytes,
        });
    }
    // Newest-first via descending sort key — `sort_by_key` is clippy's
    // preferred idiom over `sort_by` with a simple comparator.
    out.sort_by_key(|b| std::cmp::Reverse(b.created_at_secs));
    Ok(out)
}

/// Run `mysqldump <database>` and stream the output through gzip into a
/// new file under `data/backups/`. Emits progress events so the UI can
/// show bytes written and the final success/error state.
///
/// The produced filename is `<database>_<unix_secs>.sql.gz` — returned on
/// success so the frontend can refresh and highlight the new entry.
pub async fn backup_database(
    app: &AppHandle,
    install_dir: &Path,
    port: u16,
    database: String,
) -> Result<String, String> {
    let password = load_root_password(install_dir)?;
    let mysqldump = mysqldump_path(install_dir);
    if !mysqldump.is_file() {
        return Err(format!(
            "mysqldump.exe not found at {}",
            mysqldump.display()
        ));
    }

    let dir = backups_dir(install_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let filename = format!("{}_{}.sql.gz", sanitize_for_filename(&database), timestamp);
    let path = dir.join(&filename);

    let _ = app.emit(
        BACKUP_EVENT,
        BackupProgressEvent {
            database: database.clone(),
            phase: BackupPhase::Starting,
            bytes: None,
            message: None,
        },
    );

    // --single-transaction gives a consistent snapshot on InnoDB tables
    // without locking readers. --routines/--triggers/--events round the
    // dump out so restores don't lose stored procedures or scheduled tasks.
    let mut child = Command::new(&mysqldump)
        .env("MYSQL_PWD", &password)
        .arg("-u")
        .arg("root")
        .arg("-h")
        .arg("127.0.0.1")
        .arg("-P")
        .arg(port.to_string())
        .arg("--single-transaction")
        .arg("--routines")
        .arg("--triggers")
        .arg("--events")
        .arg("--default-character-set=utf8mb4")
        .arg(&database)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn mysqldump: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "missing stdout handle".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "missing stderr handle".to_string())?;

    // Stream the dump in a background task. Extracted as its own function
    // to keep `backup_database` within clippy's length budget and to make
    // the hot loop (read → compress → emit progress) independently testable.
    let stream = tokio::task::spawn(stream_dump_to_gzip(
        app.clone(),
        stdout,
        path.clone(),
        database.clone(),
    ));

    // Drain stderr in parallel so mysqldump doesn't block on a full pipe
    // when it prints a warning. We'll inspect the collected lines if the
    // exit code is non-zero.
    let err_task = tokio::task::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut collected = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            collected.push_str(&line);
            collected.push('\n');
        }
        collected
    });

    let stream_result = stream.await.map_err(|e| format!("stream join: {e}"))?;
    let stderr_output = err_task.await.unwrap_or_default();
    let status = child
        .wait()
        .await
        .map_err(|e| format!("wait mysqldump: {e}"))?;

    finalize_backup(
        app,
        &path,
        filename,
        database,
        status.success(),
        stream_result,
        &stderr_output,
        status,
    )
}

/// Emit the final progress event (Done or Error) and return the result
/// that `backup_database` surfaces to the Tauri caller. On error, the
/// partial `.sql.gz` is removed so the UI never lists a half-written file.
#[allow(clippy::too_many_arguments)]
fn finalize_backup(
    app: &AppHandle,
    path: &Path,
    filename: String,
    database: String,
    status_success: bool,
    stream_result: Result<u64, String>,
    stderr_output: &str,
    status: std::process::ExitStatus,
) -> Result<String, String> {
    if let (true, Ok(total)) = (status_success, &stream_result) {
        let _ = app.emit(
            BACKUP_EVENT,
            BackupProgressEvent {
                database,
                phase: BackupPhase::Done,
                bytes: Some(*total),
                message: None,
            },
        );
        return Ok(filename);
    }

    let _ = std::fs::remove_file(path);
    let msg = if status_success {
        stream_result.err().unwrap_or_else(|| "unknown error".into())
    } else {
        let trimmed = stderr_output.trim();
        if trimmed.is_empty() {
            format!("mysqldump exited with status {status}")
        } else {
            trimmed.to_string()
        }
    };
    let _ = app.emit(
        BACKUP_EVENT,
        BackupProgressEvent {
            database,
            phase: BackupPhase::Error,
            bytes: None,
            message: Some(msg.clone()),
        },
    );
    Err(msg)
}

/// Read mysqldump's stdout, pipe it through gzip, and write the result to
/// `path`. Emits a `backup-progress` Running event at most 4x/sec so the
/// UI can show bytes written without flooding the IPC bus. Returns the
/// total bytes of compressed output — useful for the final Done event.
async fn stream_dump_to_gzip(
    app: AppHandle,
    stdout: ChildStdout,
    path: PathBuf,
    database: String,
) -> Result<u64, String> {
    let mut reader = BufReader::new(stdout);
    let file = std::fs::File::create(&path)
        .map_err(|e| format!("create {} failed: {e}", path.display()))?;
    let mut encoder = GzEncoder::new(file, Compression::default());

    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| format!("read from mysqldump failed: {e}"))?;
        if n == 0 {
            break;
        }
        encoder
            .write_all(&buf[..n])
            .map_err(|e| format!("gzip write failed: {e}"))?;
        total += n as u64;
        if last_emit.elapsed() >= Duration::from_millis(250) {
            let _ = app.emit(
                BACKUP_EVENT,
                BackupProgressEvent {
                    database: database.clone(),
                    phase: BackupPhase::Running,
                    bytes: Some(total),
                    message: None,
                },
            );
            last_emit = Instant::now();
        }
    }
    encoder
        .finish()
        .map_err(|e| format!("gzip finish failed: {e}"))?;
    Ok(total)
}

/// Delete a backup file from `data/backups/`. Guarded against path
/// traversal — the frontend passes only the bare filename, so we reject
/// anything containing a path separator or parent reference before
/// touching the filesystem.
pub fn delete_backup(install_dir: &Path, filename: &str) -> Result<(), String> {
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("invalid filename".into());
    }
    if !filename.ends_with(".sql.gz") {
        return Err("refusing to delete: not a backup file".into());
    }
    let path = backups_dir(install_dir).join(filename);
    if !path.is_file() {
        return Err("backup file not found".into());
    }
    std::fs::remove_file(&path).map_err(|e| format!("remove failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_alnum_and_underscore() {
        assert_eq!(sanitize_for_filename("my_db-1"), "my_db-1");
    }

    #[test]
    fn sanitize_replaces_path_chars() {
        assert_eq!(sanitize_for_filename("a/b\\c"), "a_b_c");
    }

    #[test]
    fn sanitize_replaces_special_chars() {
        assert_eq!(sanitize_for_filename("db name!"), "db_name_");
    }

    #[test]
    fn delete_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(delete_backup(dir.path(), "../secret.sql.gz").is_err());
        assert!(delete_backup(dir.path(), "sub/file.sql.gz").is_err());
        assert!(delete_backup(dir.path(), "file.txt").is_err());
    }
}
