//! Kimix tools library.
pub use kimix_version::VERSION;

/// Default maximum output size (in bytes) for tool results sent to the model.
/// 40 KB ≈ 10 000 tokens
pub const DEFAULT_TOOL_OUTPUT_BYTES: usize = 40_000;

/// Default maximum output size (in characters) for bash/terminal tool results.
/// 20 000 chars ≈ 5 000 tokens. Matches the common `SHELL_CHAR_HARD_LIMIT`.
/// Override at runtime with env `KIMIX_MAX_TOOL_OUTPUT_CHARS` (see
/// [`tool_output_chars_limit`]).
pub const DEFAULT_TOOL_OUTPUT_CHARS: usize = 20_000;

/// Env override for bash/terminal tool output character budget.
pub const ENV_MAX_TOOL_OUTPUT_CHARS: &str = "KIMIX_MAX_TOOL_OUTPUT_CHARS";

/// Effective bash/terminal tool output character budget.
///
/// Precedence: env `KIMIX_MAX_TOOL_OUTPUT_CHARS` (positive integer) >
/// [`DEFAULT_TOOL_OUTPUT_CHARS`]. Use this at call sites that previously
/// hard-coded the default so operators can tighten memory/token pressure
/// without a rebuild (`0` is ignored → fall through to default).
pub fn tool_output_chars_limit() -> usize {
    std::env::var(ENV_MAX_TOOL_OUTPUT_CHARS)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_TOOL_OUTPUT_CHARS)
}

/// MCP inline tool-result cap (`MCP_MAX_OUTPUT_BYTES` and host/env helpers).
pub use util::mcp_truncate::{
    ENV_KIMIX_MAX_MCP_OUTPUT_BYTES, ENV_MAX_MCP_OUTPUT_BYTES, MCP_MAX_OUTPUT_BYTES,
    mcp_max_output_bytes, mcp_max_output_bytes_from_env, set_mcp_max_output_bytes,
};

pub mod attribution;

pub mod bridge;
pub mod computer;
pub mod gitignore;
pub mod implementations;
pub mod normalization;
pub mod notification;
pub mod persistence;
pub mod registry;
pub mod reminders;
pub mod retry;
pub mod tool_taxonomy;
pub mod types;
pub mod url_scheme;
pub mod util;
pub mod versions;

pub use attribution::{
    Auth401AttributionCallback, SENT_BEARER_PREFIX_LEN, SharedAttributionCallback, ToolConsumer,
};
