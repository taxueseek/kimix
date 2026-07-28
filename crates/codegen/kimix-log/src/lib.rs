//! Local, zero-egress observability for Kimix sessions.
//!
//! Every sink in this crate writes to the local filesystem (under the Kimix
//! home directory) and nothing else: the unified session log, the `--debug`
//! firehose, subsystem file logs (memory, hooks, sampling), and the
//! env-gated performance instrumentation. No module here opens a network
//! connection — that property is the crate's contract.
pub mod aggregator;
mod appender;
pub mod debug_log;
pub mod hooks_log;
pub mod instrumentation;
pub mod memory_log;
pub mod redact;
pub mod sampling_log;
pub mod session_ctx;
pub mod unified_log;

pub use session_ctx::with_session_ctx;
