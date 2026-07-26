//! Queue event types for `queue.jsonl`.
//!
//! Every mutation to the prompt queue is modeled as an append-only event.
//! On recovery, events are replayed in order to reconstruct the queue state.

use serde::{Deserialize, Serialize};

/// A single queue mutation event, written as one JSONL line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueEvent {
    /// A prompt was added to the queue.
    Enqueue {
        id: String,
        /// "prompt" | "bash" | "cron"
        kind: String,
        text: String,
        owner: Option<String>,
        version: u64,
        timestamp: String,
    },
    /// A prompt was removed from the queue (drained or cancelled).
    Dequeue { id: String, timestamp: String },
    /// A queued prompt's text was edited in-place.
    Edit {
        id: String,
        new_text: String,
        new_version: u64,
        editor: Option<String>,
        timestamp: String,
    },
    /// The queue order was changed by the user.
    Reorder {
        ordered_ids: Vec<String>,
        timestamp: String,
    },
    /// All queued prompts were cleared.
    Clear { timestamp: String },
}

/// Snapshot of the full queue state for fast recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub version: u64,
    pub entries: Vec<SnapshotEntry>,
    pub snapshot_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub owner: Option<String>,
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_roundtrip() {
        let event = QueueEvent::Enqueue {
            id: "p1".into(),
            kind: "prompt".into(),
            text: "fix the bug".into(),
            owner: Some("tui".into()),
            version: 0,
            timestamp: "2026-07-24T10:00:00.000Z".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "enqueue");
        assert_eq!(json["id"], "p1");
        assert_eq!(json["kind"], "prompt");

        let round: QueueEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(round, QueueEvent::Enqueue { .. }));
    }

    #[test]
    fn dequeue_roundtrip() {
        let event = QueueEvent::Dequeue {
            id: "p1".into(),
            timestamp: "2026-07-24T10:01:00.000Z".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let round: QueueEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(round, QueueEvent::Dequeue { id, .. } if id == "p1"));
    }

    #[test]
    fn edit_roundtrip() {
        let event = QueueEvent::Edit {
            id: "p1".into(),
            new_text: "fix the auth bug".into(),
            new_version: 1,
            editor: Some("vscode".into()),
            timestamp: "2026-07-24T10:02:00.000Z".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "edit");
        assert_eq!(json["new_version"], 1);

        let round: QueueEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(round, QueueEvent::Edit { new_version: 1, .. }));
    }

    #[test]
    fn reorder_roundtrip() {
        let event = QueueEvent::Reorder {
            ordered_ids: vec!["p2".into(), "p1".into()],
            timestamp: "2026-07-24T10:03:00.000Z".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "reorder");
        assert_eq!(json["ordered_ids"][0], "p2");

        let round: QueueEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(round, QueueEvent::Reorder { .. }));
    }

    #[test]
    fn clear_roundtrip() {
        let event = QueueEvent::Clear {
            timestamp: "2026-07-24T10:04:00.000Z".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let round: QueueEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(round, QueueEvent::Clear { .. }));
    }

    #[test]
    fn snapshot_roundtrip() {
        let snapshot = QueueSnapshot {
            version: 3,
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
            snapshot_at: "2026-07-24T10:05:00.000Z".into(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let round: QueueSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(round.entries.len(), 2);
        assert_eq!(round.entries[0].id, "p1");
    }

    #[test]
    fn unknown_variant_fails_cleanly() {
        let bad = r#"{"type":"unknown_evil","id":"x"}"#;
        assert!(serde_json::from_str::<QueueEvent>(bad).is_err());
    }
}
