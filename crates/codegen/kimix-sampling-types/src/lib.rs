//! Pure data types for the xAI sampling / chat-completion API layer.
//!
//! This crate contains the API-agnostic conversation types, chat completion
//! request/response types, streaming types, and error types used across the
//! xAI agent stack.  It intentionally contains **no I/O** (no HTTP clients,
//! no file system access) so it can be depended on by downstream crates
//! (e.g., `Kimix-chat-state`) without pulling in the full `Kimix-shell`.
pub mod conversation;
pub mod doom_loop;
pub mod error;
pub mod heal;
pub mod messages;
pub mod serde_helpers;
pub mod stream_triage;
pub mod types;

pub use self::conversation::*;
pub use self::doom_loop::{
    DOOM_LOOP_CHECK_EVENT_TYPE, DOOM_LOOP_CHECK_HEADER, DoomLoopPeek, DoomLoopRecoveryPolicy,
    DoomLoopSignal, DoomLoopSignalKind, is_check_event, peek_doom_loop,
};
pub use self::error::{
    EmptyReason, EmptyResponseContext, ResponseModelMetadata, Result, SamplingError,
    is_context_length_error, is_quota_denial,
};
pub use self::heal::{
    HealReport, HealTelemetry, heal_conversation_pairs, heal_telemetry, reset_heal_telemetry,
    strip_orphan_tool_results,
};
pub use self::stream_triage::{
    StreamErrorAction, TriageContext, looks_like_tool_pair_violation, triage_error_facts,
    triage_sampling_error,
};
pub use self::types::*;

// Re-export async-openai crate Responses API types under `rs` namespace
pub use async_openai::types::responses as rs;
