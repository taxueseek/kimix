//! Production tool-notification surface (P1-b).
//!
//! **Canonical wire + runtime handle for Kimix shell / TUI.**
//! - Payload shapes and tags live here (tokio `mpsc`, schemars, base64
//!   `output` on the wire).
//! - Variant *tags* are cross-checked against
//!   [`kimix_tool_protocol::KNOWN_NOTIFICATION_KINDS`] (protocol SSOT for
//!   kind names).
//! - [`kimix_tool_runtime::notification`] is the trait-crate twin
//!   (futures `mpsc`, no schemars) kept in lockstep via the same kind list;
//!   do not invent a third enum.
pub mod types;

pub use types::ALL_NOTIFICATION_TAGS;
pub use types::BashExecutionBackgrounded;
pub use types::BashExecutionComplete;
pub use types::BashExecutionFailed;
pub use types::BashExecutionTimeout;
pub use types::BashNotificationBase;
pub use types::BashOutputChunk;
pub use types::FileRead;
pub use types::FileWritten;
pub use types::LspServerCrashed;
pub use types::LspServerFailed;
pub use types::LspServerReady;
pub use types::LspServerRetrying;
pub use types::LspServerStarting;
pub use types::MonitorEvent;
pub use types::PerCallNotificationSink;
pub use types::PlanModeEntered;
pub use types::PlanModeExited;
pub use types::ScheduledTaskCreated;
pub use types::ScheduledTaskFired;
pub use types::ScheduledTaskRemoved;
pub use types::ToolNotification;
pub use types::ToolNotificationHandle;
pub use types::UserQuestionAsked;
pub use types::notification_schema_catalog;
