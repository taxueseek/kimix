//! Append-only writer for `queue.jsonl`.
//!
//! Mirrors the pattern from `log.rs` (EventWriter) but for queue events.
//! Each `QueueEvent` is serialized as a single JSONL line.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::queue_types::QueueEvent;

const QUEUE_FILE: &str = "queue.jsonl";

/// Shared event writer for `queue.jsonl`. `Clone + Send + Sync`.
#[derive(Clone)]
pub struct QueueEventWriter {
    inner: Arc<QueueEventWriterInner>,
}

struct QueueEventWriterInner {
    file: Mutex<Option<File>>,
    error_logged: AtomicBool,
}

impl QueueEventWriter {
    /// Open the queue event log in the given session directory.
    /// Creates the file if it doesn't exist, appends if it does.
    pub fn open(session_dir: &Path) -> Self {
        let path = session_dir.join(QUEUE_FILE);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| {
                tracing::warn!(path = %path.display(), error = %e, "failed to open {QUEUE_FILE}");
                e
            })
            .ok();
        Self {
            inner: Arc::new(QueueEventWriterInner {
                file: Mutex::new(file),
                error_logged: AtomicBool::new(false),
            }),
        }
    }

    /// No-op writer that discards all events.
    /// Used when queue persistence is disabled via config.
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(QueueEventWriterInner {
                file: Mutex::new(None),
                error_logged: AtomicBool::new(true),
            }),
        }
    }

    /// Returns `true` if this writer actually persists events.
    pub fn is_active(&self) -> bool {
        matches!(self.inner.file.lock(), Ok(guard) if guard.is_some())
    }

    /// Append a queue event to the log.
    /// Serialization failures are silently dropped (no panic on hot path).
    pub fn emit(&self, event: QueueEvent) {
        let Ok(mut line) = serde_json::to_vec(&event) else {
            return;
        };
        line.push(b'\n');

        let Ok(mut guard) = self.inner.file.lock() else {
            return;
        };
        if let Some(ref mut f) = *guard {
            if let Err(e) = f.write_all(&line)
                && !self.inner.error_logged.swap(true, Ordering::Relaxed)
            {
                tracing::warn!(error = %e, "{QUEUE_FILE} write failed");
            }
            // Best-effort flush — durability over performance for queue events.
            // Queue events are low-frequency (user input), so this is acceptable.
            let _ = f.flush();
        }
    }
}

impl std::fmt::Debug for QueueEventWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueEventWriter").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::queue_types::QueueEvent;

    fn _assert_send_sync_clone()
    where
        QueueEventWriter: Send + Sync + Clone,
    {
    }

    #[test]
    fn emit_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let writer = QueueEventWriter::open(dir.path());

        writer.emit(QueueEvent::Enqueue {
            id: "p1".into(),
            kind: "prompt".into(),
            text: "hello".into(),
            owner: Some("tui".into()),
            version: 0,
            timestamp: "2026-07-24T10:00:00.000Z".into(),
        });
        writer.emit(QueueEvent::Dequeue {
            id: "p1".into(),
            timestamp: "2026-07-24T10:01:00.000Z".into(),
        });

        let text = std::fs::read_to_string(dir.path().join("queue.jsonl")).unwrap();
        let lines: Vec<&str> = text.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["type"], "enqueue");
        assert_eq!(first["id"], "p1");

        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["type"], "dequeue");
    }

    #[test]
    fn noop_writer_discards() {
        let writer = QueueEventWriter::noop();
        assert!(!writer.is_active());

        writer.emit(QueueEvent::Clear {
            timestamp: "2026-07-24T10:00:00.000Z".into(),
        });
        // No file created, no panic — mission accomplished.
    }

    #[test]
    fn cloned_writer_shares_file() {
        let dir = tempfile::tempdir().unwrap();
        let w1 = QueueEventWriter::open(dir.path());
        let w2 = w1.clone();

        w1.emit(QueueEvent::Enqueue {
            id: "a".into(),
            kind: "prompt".into(),
            text: "first".into(),
            owner: None,
            version: 0,
            timestamp: "2026-07-24T10:00:00.000Z".into(),
        });
        w2.emit(QueueEvent::Enqueue {
            id: "b".into(),
            kind: "prompt".into(),
            text: "second".into(),
            owner: None,
            version: 0,
            timestamp: "2026-07-24T10:00:01.000Z".into(),
        });

        let text = std::fs::read_to_string(dir.path().join("queue.jsonl")).unwrap();
        let lines: Vec<&str> = text.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn append_mode_preserves_existing() {
        let dir = tempfile::tempdir().unwrap();

        // First writer writes one event
        let w1 = QueueEventWriter::open(dir.path());
        w1.emit(QueueEvent::Enqueue {
            id: "old".into(),
            kind: "prompt".into(),
            text: "existing".into(),
            owner: None,
            version: 0,
            timestamp: "2026-07-24T09:00:00.000Z".into(),
        });

        // Second writer appends
        let w2 = QueueEventWriter::open(dir.path());
        w2.emit(QueueEvent::Enqueue {
            id: "new".into(),
            kind: "prompt".into(),
            text: "appended".into(),
            owner: None,
            version: 0,
            timestamp: "2026-07-24T10:00:00.000Z".into(),
        });

        let text = std::fs::read_to_string(dir.path().join("queue.jsonl")).unwrap();
        let lines: Vec<&str> = text.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["id"], "old");
        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["id"], "new");
    }
}
