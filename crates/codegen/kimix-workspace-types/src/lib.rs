//! Wire types for the `Kimix-workspace` API.
//!
//! This crate is intentionally pure-data and depends on nothing more than
//! `base64`, `serde`, `serde_json`, `thiserror`, and `chrono`. There is
//! no tokio, no async-trait, no I/O. This makes it cheap to depend on
//! from anywhere -- including the eventual WASM browser SDK.
//!
//! # Module overview
//!
//! - [`events`] -- pub/sub events ([`WorkspaceEvent`]).
//! - [`rpc`] -- canonical wire types for the hub-proxied `workspace.*`
//!   RPC dispatch (trait, envelope, hub tool ids, per-method types).
//! - [`types`] -- supporting structs/enums referenced from events.
//!
//! # Wire format
//!
//! Every enum that appears on the wire is **adjacently tagged** with
//! `#[serde(tag = "type", content = "data")]`. The JSON wire shape is:
//!
//! ```json
//! {"type": "<variant_name>", "data": <payload>}
//! ```
//!
//! Adjacent (rather than internal) tagging is required because internal
//! tagging fails for newtype variants wrapping non-struct payloads --
//! e.g. `OpsChunk::GitMetadata(Option<...>)`,
//! `SessionChunk::SessionId(SessionId)` (a string),
//! `WorkspaceOpsRequest::ResolveFileRefs(Vec<String>)`. Adjacent tagging
//! works uniformly across all payload shapes (struct, newtype, unit)
//! and gives the JSON wire format an explicit discriminator. Each
//! variant maps directly to a protobuf `oneof` regardless of which
//! serde tagging form is used; the wire-format choice here only
//! affects the JSON representation.
//!
//! Wire-format struct fields use **snake_case** to match gRPC field
//! conventions (proto field names are snake_case).
//!
//! # Wire integer types
//!
//! Every `usize` field (`BeginPrompt.idx`,
//! `EndPrompt.idx`, `Rewind.target`, `MemorySearch.limit`;
//! `CodebaseIndexUpdated.files_indexed`) is replaced here with `u64`
//! (or `u32` for known-bounded sizes like `MemorySearch.limit`).
//!
//! Rationale: `usize` is host-dependent (32 vs 64 bit) and serializes
//! inconsistently across producers, which would let a 32-bit publisher
//! silently truncate a value that a 64-bit subscriber reconstructs as
//! something different. `u64` codegens cleanly to protobuf `uint64`
//! and pins the wire width regardless of host. The existing
//! `tokens: u64` field set the precedent; we extended it to every wire
//! integer.
//!
//! # TODO: proto generation
//!
//! A planned `build.rs` will walk the request / chunk /
//! event enums via reflection and emit a `.proto`. That codegen step is
//! **not** implemented yet; it will land alongside the
//! `Kimix-workspace-grpc` crate. The Rust types defined here are
//! the source of truth.
pub mod events;
pub mod rpc;
pub mod types;

/// MCP tool name delimiter: server names are qualified as `"server__tool"`.
///
/// Lives in `Kimix-workspace-types` (instead of `Kimix-workspace` or
/// `Kimix-mcp`) so both the permission-validation layer and the MCP
/// transport layer can depend on it without dragging the full workspace or
/// rmcp into each other. Re-exported by `kimix_workspace::permission`
/// for callers that historically imported it from there.
pub const MCP_TOOL_NAME_DELIMITER: &str = "__";

pub use crate::events::{WorkspaceEvent, WorkspaceTopic, WorkspaceTopicSet};
pub use crate::types::{
    FsEventKind, HookInfo, LspServerStatus, McpServerStatus, PluginInfo, SkillInfo, VcsKind,
};
