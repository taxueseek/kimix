//! Pure-delta bash progress helpers (P1-c).
//!
//! Extracted from `mod.rs` so streaming math stays testable without pulling
//! the full bash tool surface. Producers emit pure-delta
//! [`BashOutputChunk`] payloads; this module turns them into
//! [`kimix_tool_runtime::ToolProgress`] frames capped at
//! [`MAX_PROGRESS_DELTA_BYTES`].

use std::sync::LazyLock;

use crate::notification::types::{BashNotificationBase, BashOutputChunk, ToolNotification};

/// Maximum size, in bytes, of a single emitted progress `delta`. Guards
/// against a pathological single-tick burst (a large accumulation flushed in
/// one ~100 ms tick) flooding the harness in one frame. A delta larger than
/// this is cut on a UTF-8 char boundary and the remainder is held back for the
/// next tick (append is lossless); `total_bytes` still reflects the true
/// monotonic count.
pub(super) const MAX_PROGRESS_DELTA_BYTES: usize = 16 * 1024;

/// Bash's capabilities incl. its streaming spec (single source of truth):
/// raw stdout is the terminal projection, so `RawTerminal` / `Append`,
/// capped per frame at [`MAX_PROGRESS_DELTA_BYTES`].
pub(super) static BASH_CAPABILITIES: LazyLock<kimix_tool_protocol::ToolCapabilities> =
    LazyLock::new(|| kimix_tool_protocol::ToolCapabilities {
        is_read_only: false,
        tool_scope: Some(kimix_tool_protocol::ToolScope::Write),
        streaming: Some(kimix_tool_protocol::StreamingSpec {
            subkind: "bash_output_chunk".to_owned(),
            max_delta_bytes: Some(MAX_PROGRESS_DELTA_BYTES as u32),
        }),
        ..Default::default()
    });

/// One `ToolProgress` delta from a `BashOutputChunk`; `None` when no new bytes.
///
/// `chunk.base.output` is a **pure delta**. Unconsumed bytes (e.g. held back by
/// the 16 KiB per-frame cap) stay in `pending` so the next call can re-slice
/// them without re-fetching a full buffer snapshot from the producer.
pub(super) fn bash_output_chunk_progress(
    spec: &kimix_tool_protocol::StreamingSpec,
    chunk: &BashOutputChunk,
    last_total: &mut usize,
    pending: &mut Vec<u8>,
) -> Option<kimix_tool_runtime::ToolProgress> {
    pending.extend_from_slice(&chunk.base.output);
    // `stream_chunk` counts in `u64`; convert at the boundary.
    let mut cursor = *last_total as u64;
    let total = chunk.base.total_bytes as u64;
    let progress = kimix_tool_runtime::stream_chunk(
        spec,
        pending,
        total,
        &mut cursor,
        // Cumulative truncation maps to `truncated`; per-tick overflow is `gap`.
        chunk.base.truncated,
    )?;
    *last_total = cursor as usize;
    // Keep only bytes not yet surfaced (suffix of pure-delta accumulation).
    let keep = total.saturating_sub(cursor) as usize;
    if pending.len() > keep {
        let drain = pending.len() - keep;
        pending.drain(..drain);
    }
    Some(progress)
}

/// Convert a terminal [`BashNotificationBase`] (full snapshot) into a pure-delta
/// synthetic chunk relative to `last_total`, for the final drain fold.
pub(super) fn pure_delta_from_snapshot(
    base: &BashNotificationBase,
    last_total: usize,
) -> BashOutputChunk {
    let total = base.total_bytes;
    let new = total.saturating_sub(last_total);
    let delta = if new == 0 {
        Vec::new()
    } else if new <= base.output.len() {
        base.output[base.output.len() - new..].to_vec()
    } else {
        base.output.clone()
    };
    BashOutputChunk {
        base: BashNotificationBase {
            tool_call_id: base.tool_call_id.clone(),
            command: base.command.clone(),
            output: delta,
            total_bytes: total,
            truncated: base.truncated,
            cwd: base.cwd.clone(),
        },
    }
}

/// Extract the final [`BashNotificationBase`] carried by a terminal bash
/// notification (`Complete` / `Timeout` / `Backgrounded`), or `None` for any
/// other notification variant.
///
/// `BashTool::run` always sends one of these as its last notification, and
/// `LocalTerminalActor::drain_remaining_output` can append bytes *after* the
/// final periodic `BashOutputChunk` has been emitted. The streaming loop folds
/// this final base into a synthetic chunk so the in-band `bash_output_chunk`
/// deltas reach the terminal `total_bytes` without losing the tail.
pub(super) fn terminal_notification_base(
    notif: &ToolNotification,
) -> Option<&BashNotificationBase> {
    match notif {
        ToolNotification::BashExecutionComplete(c) => Some(&c.base),
        ToolNotification::BashExecutionTimeout(t) => Some(&t.base),
        ToolNotification::BashExecutionBackgrounded(b) => Some(&b.base),
        _ => None,
    }
}
