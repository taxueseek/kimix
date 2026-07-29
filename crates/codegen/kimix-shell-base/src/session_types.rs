//! Pure wire types extracted from `kimix-shell/src/session/` so that
//! downstream crates (kimix-tui, kimix-headless) can depend on these types
//! without rebuilding when shell logic changes.

use agent_client_protocol as acp;
use serde::{Deserialize, Serialize};
use serde_json::value::to_raw_value;
use std::sync::Arc;

// ── From session/result.rs ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtMethodError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ExtMethodError {
    pub fn with_data<D: Serialize>(
        code: impl Into<String>,
        message: impl Into<String>,
        data: D,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: serde_json::to_value(data).ok(),
        }
    }
}

/// Extension method result: `{ result: T | null, error?: string | ExtMethodError }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtMethodResult<T: Serialize> {
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

impl<T: Serialize> ExtMethodResult<T> {
    pub fn success(result: T) -> Self {
        Self {
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(error: impl std::fmt::Display) -> Self {
        Self {
            result: None,
            error: Some(serde_json::Value::String(error.to_string())),
        }
    }

    pub fn partial(result: T, error: impl std::fmt::Display) -> Self {
        Self {
            result: Some(result),
            error: Some(serde_json::Value::String(error.to_string())),
        }
    }

    pub fn from_result<E: std::fmt::Display>(result: Result<T, E>) -> Self {
        match result {
            Ok(value) => Self::success(value),
            Err(e) => Self::failure(e),
        }
    }

    pub fn to_ext_response(&self) -> anyhow::Result<acp::ExtResponse> {
        serde_json::to_value(self)
            .and_then(|v| to_raw_value(&v))
            .map(|raw| acp::ExtResponse::new(Arc::from(raw)))
            .map_err(|e| anyhow::anyhow!(e))
    }
}

// ── From session/acp_types.rs ─────────────────────────────────────────────

/// Formats a count with a naively pluralized noun: `"1 skill"`, `"21 skills"`.
pub fn count_detail(count: u64, noun: &str) -> String {
    let suffix = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{suffix}")
}

/// A single row in the context-usage breakdown, e.g. Skills or MCP server tokens
/// injected. These rows overlap [`ContextInfo::message_tokens`]; a fresh
/// session can show rows before the reminders are injected. Neither
/// estimate counts the `<system-reminder>` wrapper added on injection.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TokenUsageCategory {
    /// Display label, e.g. `"Skills"` or `"MCP servers"`.
    pub label: String,
    /// Estimated tokens this category costs in context.
    pub tokens: u64,
    /// Short supporting detail. By convention a count followed by a
    /// noun, e.g. `"21 skills"`; the pager right-aligns the leading count
    /// across rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl TokenUsageCategory {
    /// Row for the skills listing. `text` is the canonical render from
    /// `SkillManager::listing_snapshot`.
    pub fn skills_listing(text: &str, skill_count: usize) -> Self {
        Self {
            label: "Skills".to_string(),
            tokens: kimix_token_estimation::estimate_tokens(text),
            detail: Some(count_detail(skill_count as u64, "skill")),
        }
    }

    /// Row for the MCP server announcement. `text` is the full reminder
    /// body for the current server set.
    pub fn mcp_servers(text: &str, server_count: usize) -> Self {
        Self {
            label: "MCP servers".to_string(),
            tokens: kimix_token_estimation::estimate_tokens(text),
            detail: Some(count_detail(server_count as u64, "server")),
        }
    }
}

fn default_auto_compact_threshold() -> u8 {
    75
}

/// Context usage breakdown for session info.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextInfo {
    pub used: u64,
    pub total: u64,
    pub system_prompt_tokens: u64,
    pub tool_definitions_count: u64,
    pub tool_definitions_tokens: u64,
    pub compaction_count: u64,
    pub turn_count: u64,
    pub tool_call_count: u64,
    /// Total conversation items (system + user + assistant + tool responses).
    pub message_count: u64,
    /// Bytes/4 estimate of all non-system conversation items.
    pub message_tokens: u64,
    pub free_tokens: u64,
    pub usage_pct: u8,
    /// The resolved auto-compact threshold percent (0-100) for the active model
    /// at the time this snapshot was captured. Comes from the 6-tier resolution
    /// (env > user per-model > user global > GB per-model > GB global > 75).
    /// Used by the TUI `/context` view so the displayed "Auto-compact at X%"
    /// always matches the actual trigger (e.g. 65 for Kimix in remote settings).
    #[serde(default = "default_auto_compact_threshold")]
    pub auto_compact_threshold_percent: u8,
    /// Itemized usage rows (skills listing, MCP server listing). Empty on
    /// partial snapshots.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub usage_categories: Vec<TokenUsageCategory>,
    /// Cache hit rate for the current turn (0.0-1.0).
    /// Derived from `turn_cached_input_tokens / turn_input_tokens`.
    /// Used by the TUI context bar to display cache efficiency.
    #[serde(default)]
    pub cache_hit_rate: f64,
}

impl ContextInfo {
    /// Partial snapshot from a notification carrying only used + total.
    /// Breakdown fields default to zero until the next full ContextInfo update.
    pub fn from_notification(used: u64, total: u64) -> Self {
        Self {
            used,
            total,
            usage_pct: kimix_token_estimation::usage_percentage_u8(used, total),
            free_tokens: kimix_token_estimation::free_tokens(total, used),
            auto_compact_threshold_percent: 75,
            ..Self::default()
        }
    }
}

// ── From session/mcp_dispatcher.rs ────────────────────────────────────────

/// JSON payload pushed over ACP. Fields written in camelCase per ACP
/// convention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatusPayload {
    /// Owning session id.
    pub session_id: String,
    /// MCP server name (`github`, ...).
    pub name: String,
    /// Always `local` (user `.Kimix/config.toml` and friends).
    pub source: crate::extensions_types::McpServerSource,
    /// Current status — see [`McpServerStatus`].
    pub status: McpServerStatus,
    /// What drove the status change. See [`McpServerStatusReason`].
    pub reason: McpServerStatusReason,
    /// Optional human-readable detail. Surfaces the full handshake /
    /// transport error reason to the UI verbatim — no sanitization or
    /// truncation — so failures are easy to debug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Reserved for future use; always `null` today; may fill
    /// this with the post-restart tool list so the client can
    /// re-render without a follow-up `mcp/list` round-trip.
    pub tools: Option<serde_json::Value>,
}

/// Status enum surfaced to the wire. Lowercase serialization to
/// match the existing pager `McpSessionStatus` family.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatus {
    /// Client is in [`kimix_mcp::servers::ClientStateKind::Ready`]
    /// and the transport is healthy.
    Ready,
    /// Per-server handshake is in flight, or a restart is being
    /// debounced.
    Initializing,
    /// Transport closed, handshake failed, or the server is
    /// disabled/unconfigured.
    Unavailable,
    /// OAuth required but not yet acquired.
    NeedsAuth,
}

/// Reason a status delta was emitted. Lowercase + snake_case
/// serialization to keep the wire schema stable.
///
/// `RestartSucceeded` / `RestartFailed` are reserved for the
/// auto-restart path. `Initialized` is emitted for the first-time
/// `Ready` transition out of `ensure_initialized` — distinguishing
/// a brand-new handshake from a successful re-handshake.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatusReason {
    TransportClosed,
    HandshakeFailed,
    ConfigAdded,
    ConfigRemoved,
    ConfigChanged,
    Disabled,
    AuthExpired,
    /// First-time successful handshake (a new server transitioned
    /// from `Initializing` → `Ready`).
    Initialized,
    /// Restart succeeded within the auto-restart loop.
    RestartSucceeded,
    /// Restart failed (e.g. after reaching the retry cap).
    RestartFailed,
}
