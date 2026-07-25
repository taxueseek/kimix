//! Tool infrastructure for Kimix-shell.
//!

//! All tool execution goes through `Kimix-tools` via the `ToolBridge`.
//! Types (ToolOutput, ToolInput, TodoState, etc.) come from `Kimix-tools` directly.
pub mod bridge;
pub mod config;
pub mod notification_bridge;
pub mod retry;
pub mod todo;
pub mod tool_context;

pub use self::{
    config::{BashToolConfig, FileToolset, ShellToolsetConfig},
    retry::{RetryConfig, execute_with_retry},
    tool_context::ToolContext,
};

// Re-export key types from Kimix-tools for convenience
pub use self::todo::{TodoId, TodoItem, TodoPriority, TodoStatus};
pub use kimix_tools::types::output::ToolOutput;
pub use kimix_tools::types::{MCPToolInput, ToolInput};
