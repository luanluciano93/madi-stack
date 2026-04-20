#![forbid(unsafe_code)]
//! Live-tail a log file into an in-memory ring buffer and broadcast new lines
//! over a tokio channel so the Tauri frontend can stream them.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum LogsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
}

pub type LogsResult<T> = Result<T, LogsError>;

/// Start tailing `path` in a background task. Returns a broadcast receiver
/// that yields each new line.
///
/// TODO(sprint-2): implement with `notify` watcher + seek-to-end + read_to_end
/// on each event.
pub fn tail(_path: &Path) -> LogsResult<tokio::sync::broadcast::Receiver<String>> {
    let (_tx, rx) = tokio::sync::broadcast::channel::<String>(256);
    Ok(rx)
}
