//! Per-session event log (`events.jsonl`).
pub mod log;
pub mod queue_log;
pub mod queue_types;
pub mod tracker;
pub mod types;

pub use log::EventWriter;
pub use queue_log::QueueEventWriter;
pub use queue_types::{QueueEvent, QueueSnapshot, SnapshotEntry};
pub use tracker::EventTracker;
pub use types::{
    CancellationCategory, EVENT_SCHEMA_VERSION, Event, McpConfigServer, McpErrorCategory,
    PermissionDecision, Phase, SessionRelationship, ToolOutcome, TurnOutcomeLabel,
};
