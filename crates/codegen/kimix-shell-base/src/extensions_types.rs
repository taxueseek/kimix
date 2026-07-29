//! Pure wire types extracted from `kimix-shell/src/extensions/` so that
//! downstream crates (kimix-tui, kimix-headless) can depend on these types
//! without rebuilding when shell logic changes.

use serde::{Deserialize, Serialize};

// ── From extensions/task.rs ──────────────────────────────────────────────

/// Wire DTO for the `kimix/task/kill` ext request.
///
/// `pub` (with both serde directions) so ACP clients (kimix-tui) build
/// the request from the same type the agent parses — keeping the wire
/// contract typed end-to-end instead of duplicated `json!` literals.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillTaskRequest {
    pub session_id: String,
    pub task_id: String,
}

/// Wire DTO for the `kimix/task/kill` ext response payload (nested under
/// `result` in the `ExtMethodResult` envelope).
///
/// `pub` (with both serde directions) so ACP clients deserialize the typed
/// outcome instead of probing raw JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillTaskResponse {
    pub task_id: String,
    pub outcome: KillOutcome,
}

/// Outcome of a `kimix/task/kill` ext request.
///
/// Serialized over the wire in the `Kimix/task/kill` ext response
/// (`Kimix-shell::extensions::task::KillTaskResponse`) and deserialized
/// by clients (Kimix-tui), so it derives both serde directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillOutcome {
    Killed,
    AlreadyExited,
    NotFound,
}

/// Wire DTO for the `kimix/subagent/cancel` ext request.
///
/// `pub` (with both serde directions) so ACP clients (kimix-tui) build
/// the request from the same type the agent parses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSubagentRequest {
    pub subagent_id: String,
}

// ── From extensions/notification.rs ───────────────────────────────────────

/// Metadata for a single memory file, sent to the pager for the memory modal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryFileInfo {
    pub path: String,
    /// `"global"`, `"workspace"`, or `"session"`.
    pub source: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_epoch_secs: Option<u64>,
}

/// State of a retry operation or error for visual feedback in the TUI
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RetryState {
    /// A retry is in progress
    Retrying {
        /// Current retry attempt number (1-indexed)
        attempt: u32,
        /// Maximum number of retries allowed
        max_retries: u32,
        /// Human-readable reason for the retry
        reason: String,
    },
    /// All retries have been exhausted
    Exhausted {
        /// Total number of attempts made
        attempts: u32,
        /// Human-readable reason for the failure
        reason: String,
        /// True when the exhaustion was caused by an HTTP 429 rate limit.
        /// Clients use this to show a user-friendly upgrade message instead
        /// of the raw `reason` string.
        #[serde(default)]
        is_rate_limited: bool,
    },
    /// A non-retryable error occurred (e.g., auth error, invalid params)
    Failed {
        /// Category of the error (e.g., "auth", "invalid_params", "server")
        error_type: String,
        /// Human-readable error message
        message: String,
    },
}

// ── From extensions/mcp.rs ────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpServerSource {
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
