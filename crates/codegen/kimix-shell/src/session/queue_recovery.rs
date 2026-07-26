//! Queue recovery: load queue state from snapshot + replay events.
//!
//! On session start, if queue persistence is enabled, this module reconstructs
//! the in-memory queue from persisted data. The process:
//!
//! 1. Load `queue_snapshot.json` if it exists (fast path for most sessions).
//! 2. Replay any events in `queue.jsonl` that are newer than the snapshot.
//! 3. Return the reconstructed entries to be loaded into `pending_inputs`.

use std::fs;
use std::path::Path;

use kimix_file_utils::events::queue_types::{QueueEvent, QueueSnapshot, SnapshotEntry};

/// Result of queue recovery.
#[derive(Debug)]
pub struct RecoveryResult {
    pub entries: Vec<SnapshotEntry>,
    pub recovered: bool,
}

/// Recover the queue state from disk.
///
/// Returns entries in queue order (front first). If no persisted data exists,
/// returns an empty list with `recovered: false`.
pub fn recover_queue(session_dir: &Path) -> RecoveryResult {
    let snapshot_path = session_dir.join("queue_snapshot.json");
    let events_path = session_dir.join("queue.jsonl");

    // Step 1: Try to load snapshot as base state
    let (mut entries, snapshot_ts) = match load_snapshot(&snapshot_path) {
        Some((entries, ts)) => (entries, Some(ts)),
        None => (Vec::new(), None),
    };

    // Step 2: Replay incremental events from queue.jsonl
    let replayed = match replay_events(&events_path, &mut entries, snapshot_ts.clone()) {
        Some(ts) => ts,
        None => {
            return RecoveryResult {
                entries,
                recovered: snapshot_ts.is_some(),
            };
        }
    };

    // Step 3: If we replayed events, rewrite snapshot to include them
    // (amortizes cost of future recoveries)
    if replayed {
        persist_snapshot_internal(session_dir, &entries);
    }

    RecoveryResult {
        entries,
        recovered: true,
    }
}

/// Load the snapshot file. Returns (entries, snapshot_timestamp) on success.
fn load_snapshot(path: &Path) -> Option<(Vec<SnapshotEntry>, String)> {
    let content = fs::read_to_string(path).ok()?;
    let snapshot: QueueSnapshot = serde_json::from_str(&content).ok()?;
    Some((snapshot.entries, snapshot.snapshot_at))
}

/// Replay events from queue.jsonl, applying them to the entries in-place.
/// Only events newer than `after_ts` are applied.
fn replay_events(
    path: &Path,
    entries: &mut Vec<SnapshotEntry>,
    after_ts: Option<String>,
) -> Option<bool> {
    let content = fs::read_to_string(path).ok()?;
    let mut replayed_any = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let event: QueueEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue, // Skip corrupted lines
        };

        // Skip events older than the snapshot (they're already incorporated)
        if let Some(ref ts) = after_ts
            && event_timestamp(&event) <= *ts
        {
            continue;
        }

        apply_event(entries, &event);
        replayed_any = true;
    }

    Some(replayed_any)
}

/// Extract the timestamp from a queue event.
fn event_timestamp(event: &QueueEvent) -> String {
    match event {
        QueueEvent::Enqueue { timestamp, .. } => timestamp.clone(),
        QueueEvent::Dequeue { timestamp, .. } => timestamp.clone(),
        QueueEvent::Edit { timestamp, .. } => timestamp.clone(),
        QueueEvent::Reorder { timestamp, .. } => timestamp.clone(),
        QueueEvent::Clear { timestamp, .. } => timestamp.clone(),
    }
}

/// Apply a single queue event to the in-memory state.
fn apply_event(entries: &mut Vec<SnapshotEntry>, event: &QueueEvent) {
    match event {
        QueueEvent::Enqueue {
            id,
            kind,
            text,
            owner,
            version,
            ..
        } => {
            // Remove existing entry with same id (idempotent), then push
            entries.retain(|e| &e.id != id);
            entries.push(SnapshotEntry {
                id: id.clone(),
                kind: kind.clone(),
                text: text.clone(),
                owner: owner.clone(),
                version: *version,
            });
        }
        QueueEvent::Dequeue { id, .. } => {
            entries.retain(|e| &e.id != id);
        }
        QueueEvent::Edit {
            id,
            new_text,
            new_version,
            ..
        } => {
            if let Some(entry) = entries.iter_mut().find(|e| &e.id == id) {
                entry.text = new_text.clone();
                entry.version = *new_version;
            }
        }
        QueueEvent::Reorder { ordered_ids, .. } => {
            // Reorder entries to match the requested order
            let mut reordered: Vec<SnapshotEntry> = Vec::new();
            for id in ordered_ids {
                if let Some(pos) = entries.iter().enumerate().find(|(_, e)| &e.id == id) {
                    reordered.push(entries.remove(pos.0));
                }
            }
            // Append any remaining entries not in the reorder list
            reordered.append(entries);
            *entries = reordered;
        }
        QueueEvent::Clear { .. } => {
            entries.clear();
        }
    }
}

/// Persist the current queue state as a snapshot.
pub fn persist_snapshot(session_dir: &Path, entries: &[SnapshotEntry]) {
    persist_snapshot_internal(session_dir, entries);
}

fn persist_snapshot_internal(session_dir: &Path, entries: &[SnapshotEntry]) {
    if let Err(e) = std::fs::create_dir_all(session_dir) {
        tracing::warn!(error = %e, "failed creating session dir for queue snapshot");
        return;
    }

    let snapshot = QueueSnapshot {
        version: entries.iter().map(|e| e.version).max().unwrap_or(0),
        entries: entries.to_vec(),
        snapshot_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    };

    let json = match serde_json::to_string_pretty(&snapshot) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed serializing queue snapshot");
            return;
        }
    };

    let path = session_dir.join("queue_snapshot.json");
    let tmp_path = session_dir.join("queue_snapshot.json.tmp");

    if let Err(e) = fs::write(&tmp_path, json) {
        tracing::warn!(error = %e, "failed writing queue snapshot temp file");
        return;
    }

    if let Err(e) = fs::rename(&tmp_path, &path) {
        tracing::warn!(error = %e, "failed renaming queue snapshot");
        let _ = fs::remove_file(&tmp_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kimix_file_utils::events::queue_types::QueueEvent;

    #[test]
    fn recover_from_snapshot_only() {
        let dir = tempfile::tempdir().unwrap();

        // Write a snapshot directly
        let snapshot = QueueSnapshot {
            version: 2,
            entries: vec![
                SnapshotEntry {
                    id: "p1".into(),
                    kind: "prompt".into(),
                    text: "first".into(),
                    owner: Some("tui".into()),
                    version: 0,
                },
                SnapshotEntry {
                    id: "p2".into(),
                    kind: "bash".into(),
                    text: "cargo test".into(),
                    owner: Some("vscode".into()),
                    version: 1,
                },
            ],
            snapshot_at: "2026-07-24T10:00:00.000Z".into(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(dir.path().join("queue_snapshot.json"), json).unwrap();

        let result = recover_queue(dir.path());
        assert!(result.recovered);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].id, "p1");
        assert_eq!(result.entries[1].id, "p2");
    }

    #[test]
    fn replay_incremental_events() {
        let dir = tempfile::tempdir().unwrap();

        // Snapshot with one entry
        let snapshot = QueueSnapshot {
            version: 0,
            entries: vec![SnapshotEntry {
                id: "p1".into(),
                kind: "prompt".into(),
                text: "first".into(),
                owner: Some("tui".into()),
                version: 0,
            }],
            snapshot_at: "2026-07-24T10:00:00.000Z".into(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(dir.path().join("queue_snapshot.json"), json).unwrap();

        // Write events after snapshot
        let events = vec![
            QueueEvent::Enqueue {
                id: "p2".into(),
                kind: "prompt".into(),
                text: "second".into(),
                owner: Some("vscode".into()),
                version: 0,
                timestamp: "2026-07-24T10:01:00.000Z".into(),
            },
            QueueEvent::Dequeue {
                id: "p1".into(),
                timestamp: "2026-07-24T10:02:00.000Z".into(),
            },
        ];
        let mut content = String::new();
        for e in &events {
            content.push_str(&serde_json::to_string(e).unwrap());
            content.push('\n');
        }
        std::fs::write(dir.path().join("queue.jsonl"), content).unwrap();

        let result = recover_queue(dir.path());
        assert!(result.recovered);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, "p2");
    }

    #[test]
    fn recover_with_edit() {
        let dir = tempfile::tempdir().unwrap();

        let snapshot = QueueSnapshot {
            version: 0,
            entries: vec![SnapshotEntry {
                id: "p1".into(),
                kind: "prompt".into(),
                text: "original".into(),
                owner: Some("tui".into()),
                version: 0,
            }],
            snapshot_at: "2026-07-24T10:00:00.000Z".into(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(dir.path().join("queue_snapshot.json"), json).unwrap();

        let event = QueueEvent::Edit {
            id: "p1".into(),
            new_text: "edited".into(),
            new_version: 1,
            editor: Some("vscode".into()),
            timestamp: "2026-07-24T10:01:00.000Z".into(),
        };
        std::fs::write(
            dir.path().join("queue.jsonl"),
            serde_json::to_string(&event).unwrap() + "\n",
        )
        .unwrap();

        let result = recover_queue(dir.path());
        assert!(result.recovered);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].text, "edited");
        assert_eq!(result.entries[0].version, 1);
    }

    #[test]
    fn recover_with_reorder() {
        let dir = tempfile::tempdir().unwrap();

        let snapshot = QueueSnapshot {
            version: 0,
            entries: vec![
                SnapshotEntry {
                    id: "p1".into(),
                    kind: "prompt".into(),
                    text: "first".into(),
                    owner: None,
                    version: 0,
                },
                SnapshotEntry {
                    id: "p2".into(),
                    kind: "prompt".into(),
                    text: "second".into(),
                    owner: None,
                    version: 0,
                },
            ],
            snapshot_at: "2026-07-24T10:00:00.000Z".into(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(dir.path().join("queue_snapshot.json"), json).unwrap();

        let event = QueueEvent::Reorder {
            ordered_ids: vec!["p2".into(), "p1".into()],
            timestamp: "2026-07-24T10:01:00.000Z".into(),
        };
        std::fs::write(
            dir.path().join("queue.jsonl"),
            serde_json::to_string(&event).unwrap() + "\n",
        )
        .unwrap();

        let result = recover_queue(dir.path());
        assert!(result.recovered);
        assert_eq!(result.entries[0].id, "p2");
        assert_eq!(result.entries[1].id, "p1");
    }

    #[test]
    fn recover_with_clear() {
        let dir = tempfile::tempdir().unwrap();

        let snapshot = QueueSnapshot {
            version: 0,
            entries: vec![SnapshotEntry {
                id: "p1".into(),
                kind: "prompt".into(),
                text: "first".into(),
                owner: None,
                version: 0,
            }],
            snapshot_at: "2026-07-24T10:00:00.000Z".into(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(dir.path().join("queue_snapshot.json"), json).unwrap();

        let event = QueueEvent::Clear {
            timestamp: "2026-07-24T10:01:00.000Z".into(),
        };
        std::fs::write(
            dir.path().join("queue.jsonl"),
            serde_json::to_string(&event).unwrap() + "\n",
        )
        .unwrap();

        let result = recover_queue(dir.path());
        assert!(result.recovered);
        assert!(result.entries.is_empty());
    }

    #[test]
    fn no_persisted_data() {
        let dir = tempfile::tempdir().unwrap();
        let result = recover_queue(dir.path());
        assert!(!result.recovered);
        assert!(result.entries.is_empty());
    }

    #[test]
    fn persist_and_recover_roundtrip() {
        let dir = tempfile::tempdir().unwrap();

        let entries = vec![
            SnapshotEntry {
                id: "p1".into(),
                kind: "prompt".into(),
                text: "hello".into(),
                owner: Some("tui".into()),
                version: 0,
            },
            SnapshotEntry {
                id: "p2".into(),
                kind: "bash".into(),
                text: "cargo build".into(),
                owner: Some("vscode".into()),
                version: 1,
            },
        ];

        persist_snapshot(dir.path(), &entries);
        let result = recover_queue(dir.path());

        assert!(result.recovered);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].id, "p1");
        assert_eq!(result.entries[1].text, "cargo build");
    }
}
