#![forbid(unsafe_code)]
//! Bounded in-memory log buffer for supervised processes.
//!
//! Each managed service (nginx, php-cgi, mysqld) gets one [`LogBuffer`] that
//! the supervisor's stdout/stderr reader tasks push lines into. Two consumer
//! shapes:
//!
//! * **Snapshot** — [`LogBuffer::snapshot_since`] returns every line whose
//!   sequence number is `>= since`. Used by the GUI to fetch backlog when a
//!   tab opens, and to poll for incremental updates.
//! * **Live** — [`LogBuffer::subscribe`] returns a `tokio::sync::broadcast`
//!   receiver that yields each new line as it arrives. The Tauri layer wires
//!   this into `app.emit` for live tail.
//!
//! Lines older than `capacity` are evicted on push. Sequence numbers are
//! monotonic and never reused — the GUI uses the highest seq it has seen as
//! the `since` for the next snapshot, so missed events can be filled in
//! without duplicates.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Debug, thiserror::Error)]
pub enum LogsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type LogsResult<T> = Result<T, LogsError>;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    /// Unix epoch milliseconds at the moment the line was pushed.
    pub ts_ms: u64,
    pub stream: LogStream,
    pub text: String,
}

const DEFAULT_CAPACITY: usize = 1_000;
/// Bound on the live broadcast channel. If a subscriber lags, oldest events
/// are dropped — the snapshot API can be used to refill the gap.
const BROADCAST_BUF: usize = 256;

pub struct LogBuffer {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<LogLine>,
}

struct Inner {
    lines: VecDeque<LogLine>,
    capacity: usize,
    next_seq: u64,
}

impl LogBuffer {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        let (tx, _) = broadcast::channel(BROADCAST_BUF);
        Arc::new(Self {
            inner: Mutex::new(Inner {
                lines: VecDeque::with_capacity(capacity.min(1024)),
                capacity,
                next_seq: 0,
            }),
            tx,
        })
    }

    /// Append a line. The sequence number is assigned here.
    pub fn push(&self, stream: LogStream, text: String) {
        let line = {
            let mut g = self.inner.lock();
            let seq = g.next_seq;
            g.next_seq += 1;
            let line = LogLine {
                seq,
                ts_ms: now_ms(),
                stream,
                text,
            };
            g.lines.push_back(line.clone());
            while g.lines.len() > g.capacity {
                g.lines.pop_front();
            }
            line
        };
        // send fails only when there are no subscribers — that's fine.
        let _ = self.tx.send(line);
    }

    /// All lines with `seq >= since`, in order. Use `0` to fetch everything
    /// the buffer still holds.
    #[must_use]
    pub fn snapshot_since(&self, since: u64) -> Vec<LogLine> {
        let g = self.inner.lock();
        g.lines.iter().filter(|l| l.seq >= since).cloned().collect()
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    /// Highest sequence assigned so far (0 if empty). Useful for "tail -f"
    /// callers that want to skip the backlog.
    #[must_use]
    pub fn next_seq(&self) -> u64 {
        self.inner.lock().next_seq
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushes_assign_monotonic_seq() {
        let b = LogBuffer::new();
        b.push(LogStream::Stdout, "one".into());
        b.push(LogStream::Stderr, "two".into());
        let snap = b.snapshot_since(0);
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].seq, 0);
        assert_eq!(snap[1].seq, 1);
        assert_eq!(snap[0].text, "one");
        assert_eq!(snap[1].stream, LogStream::Stderr);
    }

    #[test]
    fn evicts_oldest_when_over_capacity() {
        let b = LogBuffer::with_capacity(3);
        for i in 0..5 {
            b.push(LogStream::Stdout, format!("line-{i}"));
        }
        let snap = b.snapshot_since(0);
        // capacity=3 → only the last 3 survive, but their seqs are 2,3,4.
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].seq, 2);
        assert_eq!(snap[2].seq, 4);
        assert_eq!(b.next_seq(), 5);
    }

    #[test]
    fn snapshot_since_filters_by_seq() {
        let b = LogBuffer::new();
        for i in 0..10 {
            b.push(LogStream::Stdout, format!("l{i}"));
        }
        let snap = b.snapshot_since(7);
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].seq, 7);
    }

    #[tokio::test]
    async fn subscribers_see_pushes() {
        let b = LogBuffer::new();
        let mut rx = b.subscribe();
        b.push(LogStream::Stdout, "live".into());
        let got = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.text, "live");
    }
}
