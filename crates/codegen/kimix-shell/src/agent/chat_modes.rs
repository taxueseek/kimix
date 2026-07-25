//! Legacy `--chat` gateway gate.
//!
//! The kimi.com chat-product model picker (`/rest/modes`, `ChatModesManager`)
//! was removed with the xAI proxy: those "modes" came from a Kimix backend with
//! no Kimi counterpart. Only the process-mode gate survives so the `--chat`
//! frontend path stays a compile-time-off no-op across crates without a
//! cross-crate churn to delete every reference.

/// Process-wide flag set by the pager when started with `--chat`.
pub const KIMIX_CHAT_MODE_ENV: &str = "KIMIX_CHAT_MODE";

/// True when the process is a gateway light-frontend (`--chat`) agent.
/// Hard-off: the Kimix chat-modes backend is gone, so this is always `false`.
pub fn process_chat_mode_enabled() -> bool {
    false
}
