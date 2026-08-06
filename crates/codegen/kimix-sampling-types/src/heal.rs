//! Conversation heal: pair repair, orphan strip, and process-local telemetry.
//!
//! Builds on [`crate::repair_dangling_tool_calls`] / [`crate::dedup_duplicate_tool_results`]
//! without growing `conversation.rs`. Call sites that already heal on load
//! (e.g. `ChatState::new`) should prefer [`heal_conversation_pairs`] so
//! telemetry stays centralized.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::conversation::{
    ConversationItem, DanglingToolCallReason, dedup_duplicate_tool_results,
    repair_dangling_tool_calls,
};

// ── Telemetry (process-local atomics; zero I/O) ──────────────────────────────

static HEAL_RUNS: AtomicU64 = AtomicU64::new(0);
static DANGLING_REPAIRED: AtomicU64 = AtomicU64::new(0);
static DUP_RESULTS_REMOVED: AtomicU64 = AtomicU64::new(0);
static ORPHAN_RESULTS_REMOVED: AtomicU64 = AtomicU64::new(0);

/// Snapshot of heal counters since process start (or last [`reset_heal_telemetry`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealTelemetry {
    pub runs: u64,
    pub dangling_repaired: u64,
    pub dup_results_removed: u64,
    pub orphan_results_removed: u64,
}

/// Read process-local heal counters.
pub fn heal_telemetry() -> HealTelemetry {
    HealTelemetry {
        runs: HEAL_RUNS.load(Ordering::Relaxed),
        dangling_repaired: DANGLING_REPAIRED.load(Ordering::Relaxed),
        dup_results_removed: DUP_RESULTS_REMOVED.load(Ordering::Relaxed),
        orphan_results_removed: ORPHAN_RESULTS_REMOVED.load(Ordering::Relaxed),
    }
}

/// Reset counters (tests only).
pub fn reset_heal_telemetry() {
    HEAL_RUNS.store(0, Ordering::Relaxed);
    DANGLING_REPAIRED.store(0, Ordering::Relaxed);
    DUP_RESULTS_REMOVED.store(0, Ordering::Relaxed);
    ORPHAN_RESULTS_REMOVED.store(0, Ordering::Relaxed);
}

fn record(dangling: usize, deduped: usize, orphans: usize) {
    HEAL_RUNS.fetch_add(1, Ordering::Relaxed);
    if dangling > 0 {
        DANGLING_REPAIRED.fetch_add(dangling as u64, Ordering::Relaxed);
    }
    if deduped > 0 {
        DUP_RESULTS_REMOVED.fetch_add(deduped as u64, Ordering::Relaxed);
    }
    if orphans > 0 {
        ORPHAN_RESULTS_REMOVED.fetch_add(orphans as u64, Ordering::Relaxed);
    }
}

// ── Report ───────────────────────────────────────────────────────────────────

/// Counts from one heal pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealReport {
    pub dangling_repaired: usize,
    pub dup_results_removed: usize,
    pub orphan_results_removed: usize,
}

impl HealReport {
    pub fn total_mutations(self) -> usize {
        self.dangling_repaired + self.dup_results_removed + self.orphan_results_removed
    }

    pub fn is_clean(self) -> bool {
        self.total_mutations() == 0
    }
}

// ── Heal pass ────────────────────────────────────────────────────────────────

/// Full pair-heal: dedup → repair dangling → strip orphan results.
///
/// Order matters:
/// 1. Dedup keeps the **last** result per id (real over synthetic).
/// 2. Repair inserts synthetics for unanswered tool calls.
/// 3. Orphan strip drops results whose id never appears in any assistant
///    tool_calls (stale rows from partial history rewrites).
pub fn heal_conversation_pairs(
    conversation: &mut Vec<ConversationItem>,
    reason: DanglingToolCallReason,
) -> HealReport {
    let dup_results_removed = dedup_duplicate_tool_results(conversation);
    let dangling_repaired = repair_dangling_tool_calls(conversation, reason);
    let orphan_results_removed = strip_orphan_tool_results(conversation);
    record(dangling_repaired, dup_results_removed, orphan_results_removed);
    HealReport {
        dangling_repaired,
        dup_results_removed,
        orphan_results_removed,
    }
}

/// Remove `ToolResult` items whose `tool_call_id` is not referenced by any
/// assistant `tool_calls` in the whole conversation.
///
/// Complements dangling repair (missing result) with the inverse defect
/// (result without a call) that some providers also reject.
///
/// Returns the number of results removed.
pub fn strip_orphan_tool_results(conversation: &mut Vec<ConversationItem>) -> usize {
    let mut known_ids: HashSet<String> = HashSet::new();
    for item in conversation.iter() {
        if let ConversationItem::Assistant(a) = item {
            for tc in &a.tool_calls {
                known_ids.insert(tc.id.as_ref().to_owned());
            }
        }
    }

    let before = conversation.len();
    conversation.retain(|item| match item {
        ConversationItem::ToolResult(tr) => known_ids.contains(&tr.tool_call_id),
        _ => true,
    });
    before - conversation.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{AssistantItem, ToolCall};

    fn assistant_with_calls(ids: &[&str]) -> ConversationItem {
        ConversationItem::Assistant(AssistantItem {
            content: String::new().into(),
            tool_calls: ids
                .iter()
                .map(|id| ToolCall {
                    id: (*id).into(),
                    name: "bash".into(),
                    arguments: "{}".into(),
                })
                .collect(),
            model_id: None,
            model_fingerprint: None,
            reasoning_effort: None,
        })
    }

    #[test]
    fn strip_orphan_removes_unreferenced_results() {
        let mut conv = vec![
            assistant_with_calls(&["c1"]),
            ConversationItem::tool_result("c1", "ok"),
            ConversationItem::tool_result("ghost", "stale"),
        ];
        assert_eq!(strip_orphan_tool_results(&mut conv), 1);
        assert_eq!(conv.len(), 2);
        match &conv[1] {
            ConversationItem::ToolResult(tr) => assert_eq!(tr.tool_call_id, "c1"),
            _ => panic!("expected tool result"),
        }
    }

    #[test]
    fn heal_pass_records_telemetry() {
        reset_heal_telemetry();
        let mut conv = vec![
            assistant_with_calls(&["a", "b"]),
            ConversationItem::tool_result("a", "first"),
            ConversationItem::tool_result("a", "second"), // dup
            ConversationItem::tool_result("orphan", "x"),
            // b is dangling
        ];
        let report = heal_conversation_pairs(&mut conv, DanglingToolCallReason::UserCancelled);
        assert!(report.dup_results_removed >= 1);
        assert_eq!(report.dangling_repaired, 1); // b
        assert_eq!(report.orphan_results_removed, 1);
        let tel = heal_telemetry();
        assert_eq!(tel.runs, 1);
        assert!(tel.dangling_repaired >= 1);
        assert!(tel.dup_results_removed >= 1);
        assert_eq!(tel.orphan_results_removed, 1);
    }
}
