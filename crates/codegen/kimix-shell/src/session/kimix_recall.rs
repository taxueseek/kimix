//! Kimix recall + prune + cross-session search for session turns.
//!
//! Integrated capabilities:
//! 1. **BM25 recall**: CJK bigram 3-tier memory, injected as system-reminder
//! 2. **Context-budget prune**: Tracks tool-result tokens, reports savings
//! 3. **Cross-session search**: BM25 search across all sessions
//! 4. **Soft efficiency nudge**: one-turn system-reminder on the current user
//!    message only (never rewrites history — prompt-cache safe)
//! 5. **Tool content-hash dedup**: ingress-only stub for duplicate large payloads
use kimix_bridge::{
    ContentHashDeduper, KimixPromptAdapter, KimixRecallEngine, KimixSessionMemory,
    SOFT_EFFICIENCY_NUDGE, should_soft_efficiency_nudge,
};
use kimix_sampling_types::ConversationItem;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static RECALL: std::sync::OnceLock<Mutex<KimixRecallEngine>> = std::sync::OnceLock::new();
static PROMPT: std::sync::OnceLock<Mutex<KimixPromptAdapter>> = std::sync::OnceLock::new();
static SESSION_MEMORY: std::sync::OnceLock<Mutex<KimixSessionMemory>> = std::sync::OnceLock::new();
static TOOL_DEDUP: std::sync::OnceLock<Mutex<ContentHashDeduper>> = std::sync::OnceLock::new();

/// Default soft efficiency band lower bound (ratio of effective context window).
pub const DEFAULT_SOFT_NUDGE_RATIO: f64 = 0.55;

/// Soft-nudge ratio as `f64` bits; `0` disables. Initialized to default.
static SOFT_NUDGE_RATIO_BITS: AtomicU64 = AtomicU64::new(DEFAULT_SOFT_NUDGE_RATIO.to_bits());
/// When false, `admit_tool_payload` is a no-op pass-through.
static CONTENT_HASH_DEDUP_ENABLED: AtomicBool = AtomicBool::new(true);
/// How many times a soft efficiency reminder was injected this process/session.
static SOFT_NUDGE_INJECTIONS: AtomicU64 = AtomicU64::new(0);

/// Hint for soft efficiency nudge (real chat-state token estimate).
#[derive(Debug, Clone, Copy)]
pub struct ContextUsageHint {
    pub estimated_tokens: u64,
    pub context_window: u64,
    /// `0` means no cap (use full `context_window`).
    pub max_effective_context_tokens: u32,
}

/// Snapshot of context-economy counters for status / tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextEconomyStats {
    pub prune_tokens_saved: usize,
    pub prune_passes: usize,
    pub dedup_count: usize,
    pub dedup_tokens_saved: usize,
    pub soft_nudge_injections: u64,
    pub content_hash_dedup_enabled: bool,
}

fn tool_dedup() -> &'static Mutex<ContentHashDeduper> {
    TOOL_DEDUP.get_or_init(|| Mutex::new(ContentHashDeduper::new()))
}

fn recall() -> &'static Mutex<KimixRecallEngine> {
    RECALL.get_or_init(|| Mutex::new(KimixRecallEngine::new()))
}
fn prompt_adapter() -> &'static Mutex<KimixPromptAdapter> {
    PROMPT.get_or_init(|| {
        Mutex::new(KimixPromptAdapter::new(
            "You are a persistent autonomous coding agent with context-budget management.\n\
         Tools available. Tool results are cached briefly; summarize key findings.\n\
         Memory: recall is auto-injected from past conversation turns.",
        ))
    })
}
fn session_memory() -> &'static Mutex<KimixSessionMemory> {
    SESSION_MEMORY.get_or_init(|| {
        let dir = std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".kimix"))
            .unwrap_or_else(|| PathBuf::from(".kimix"));
        Mutex::new(KimixSessionMemory::new(dir))
    })
}

/// Current soft-nudge ratio (`0.0` disables).
pub fn soft_nudge_ratio() -> f64 {
    f64::from_bits(SOFT_NUDGE_RATIO_BITS.load(Ordering::Relaxed))
}

/// Whether ingress content-hash dedup is enabled.
pub fn content_hash_dedup_enabled() -> bool {
    CONTENT_HASH_DEDUP_ENABLED.load(Ordering::Relaxed)
}

/// Apply resolved economy settings (call at session spawn).
///
/// `soft_nudge_ratio`: `0.0` disables; values outside `(0, 1]` are clamped.
/// `content_hash_dedup`: when false, tool payloads pass through unchanged.
pub fn configure_context_economy(soft_nudge_ratio: f64, content_hash_dedup: bool) {
    let ratio = if soft_nudge_ratio <= 0.0 {
        0.0
    } else if soft_nudge_ratio > 1.0 {
        1.0
    } else {
        soft_nudge_ratio
    };
    SOFT_NUDGE_RATIO_BITS.store(ratio.to_bits(), Ordering::Relaxed);
    CONTENT_HASH_DEDUP_ENABLED.store(content_hash_dedup, Ordering::Relaxed);
    tracing::info!(
        soft_nudge_ratio = ratio,
        content_hash_dedup,
        summary = %context_economy_summary(),
        "context_economy: configured"
    );
}

/// Inject recall context + prune stats into user message before sending to model.
pub fn inject_recall_context(user_message: &mut String) {
    inject_recall_context_with_usage(user_message, None);
}

/// Like [`inject_recall_context`], optionally injecting a soft efficiency nudge
/// based on real chat-state token usage. Nudge text is only prepended to the
/// **current** user message — historical messages are never rewritten.
pub fn inject_recall_context_with_usage(
    user_message: &mut String,
    usage: Option<ContextUsageHint>,
) {
    if user_message.len() < 10 {
        return;
    }
    let mut engine = recall().lock().expect("lock poisoned");
    let mut adapter = prompt_adapter().lock().expect("lock poisoned");
    let recall_text = engine.recall_and_format(user_message, 1200);
    let _ = adapter.begin_turn(user_message, &[]);
    let saved = adapter.tokens_saved();
    let prunes = adapter.prune_count();
    let tn = engine.turn_count();
    engine.add_turn("user", user_message, false, tn);
    let mut prefix = String::new();
    if !recall_text.is_empty() {
        prefix.push_str(&format!(
            "<system-reminder>\n{}\n</system-reminder>\n",
            recall_text
        ));
    }
    if prunes > 0 && saved > 100 {
        prefix.push_str(&format!(
            "<system-reminder>\n[Kimix prune: {} passes, ~{}K tokens saved. Summarize tool outputs.]\n</system-reminder>\n",
            prunes, saved / 1000
        ));
    }
    let (dedup_count, dedup_saved) = tool_dedup_stats();
    if dedup_count > 0 && dedup_saved > 50 {
        prefix.push_str(&format!(
            "<system-reminder>\n[Kimix content-dedup: {dedup_count} duplicate tool payload(s) stubbed, ~{} tokens omitted this session. Prefer not re-reading identical large outputs.]\n</system-reminder>\n",
            dedup_saved
        ));
    }
    if let Some(u) = usage {
        let effective = if u.max_effective_context_tokens > 0 {
            u.context_window
                .min(u64::from(u.max_effective_context_tokens))
        } else {
            u.context_window
        };
        let soft = soft_nudge_ratio();
        if should_soft_efficiency_nudge(soft, u.estimated_tokens, effective) {
            SOFT_NUDGE_INJECTIONS.fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                estimated_tokens = u.estimated_tokens,
                effective_window = effective,
                soft_nudge_ratio = soft,
                soft_nudge_injections = SOFT_NUDGE_INJECTIONS.load(Ordering::Relaxed),
                "soft_nudge: injecting efficiency reminder on current user message"
            );
            prefix.push_str(&format!(
                "<system-reminder>\n{SOFT_EFFICIENCY_NUDGE}\n</system-reminder>\n"
            ));
        }
    }
    if !prefix.is_empty() {
        *user_message = format!("{}\n{}", prefix, user_message);
    }
}

/// Admit a tool payload with session content-hash dedup.
///
/// Duplicate large payloads are replaced with a short stub **at insert time
/// only** — prior history is never mutated (prompt-cache safe).
pub fn admit_tool_payload(content: String) -> String {
    if !content_hash_dedup_enabled() {
        return content;
    }
    match tool_dedup().lock() {
        Ok(mut d) => {
            let before = d.tokens_saved;
            let out = d.admit(content);
            let gained = d.tokens_saved.saturating_sub(before);
            if gained > 0 {
                tracing::debug!(
                    omitted_tokens = gained,
                    dedup_count = d.dedup_count,
                    "tool content_dedup: suppressed duplicate payload"
                );
            }
            out
        }
        Err(_) => content,
    }
}

/// Run content-hash dedup on a [`ConversationItem`] tool result (identity for
/// other variants). Use this immediately before every `push_tool_result`.
pub fn admit_tool_result_item(item: ConversationItem) -> ConversationItem {
    match item {
        ConversationItem::ToolResult(mut tr) => {
            let admitted = admit_tool_payload(tr.content.to_string());
            tr.content = std::sync::Arc::<str>::from(admitted);
            ConversationItem::ToolResult(tr)
        }
        other => other,
    }
}

/// Admit then push a tool result. Prefer this over raw `push_tool_result` on
/// all production paths so content-hash dedup cannot be skipped.
pub fn push_admitted_tool_result(
    handle: &kimix_chat_state::ChatStateHandle,
    item: ConversationItem,
) {
    handle.push_tool_result(admit_tool_result_item(item));
}

/// Cumulative tool content-hash dedup stats: `(dedup_count, tokens_saved)`.
pub fn tool_dedup_stats() -> (usize, usize) {
    tool_dedup()
        .lock()
        .map(|d| (d.dedup_count, d.tokens_saved))
        .unwrap_or((0, 0))
}

/// Aggregate economy stats for status surfaces / tests.
pub fn context_economy_stats() -> ContextEconomyStats {
    let (prune_tokens_saved, prune_passes) = prune_stats();
    let (dedup_count, dedup_tokens_saved) = tool_dedup_stats();
    ContextEconomyStats {
        prune_tokens_saved,
        prune_passes,
        dedup_count,
        dedup_tokens_saved,
        soft_nudge_injections: SOFT_NUDGE_INJECTIONS.load(Ordering::Relaxed),
        content_hash_dedup_enabled: content_hash_dedup_enabled(),
    }
}

/// One-line summary for logs / slash status.
pub fn context_economy_summary() -> String {
    let s = context_economy_stats();
    format!(
        "context-economy: soft_nudge_ratio={:.2} soft_nudges={} dedup={} ({} hits, ~{} tok saved) prune={} passes/~{} tok",
        soft_nudge_ratio(),
        s.soft_nudge_injections,
        if s.content_hash_dedup_enabled {
            "on"
        } else {
            "off"
        },
        s.dedup_count,
        s.dedup_tokens_saved,
        s.prune_passes,
        s.prune_tokens_saved,
    )
}

/// Record assistant response for prune tracking + cross-session indexing.
pub fn record_assistant_response(response: &str) {
    if response.len() < 5 {
        return;
    }
    let mut adapter = prompt_adapter().lock().expect("lock poisoned");
    adapter.record_response(response);
    let mem = session_memory();
    if let Ok(m) = mem.lock() {
        m.add_turn("current", "assistant", response);
    }
}

/// Index user message for cross-session search.
pub fn index_user_message(msg: &str) {
    if msg.len() < 10 {
        return;
    }
    let mem = session_memory();
    if let Ok(m) = mem.lock() {
        m.add_turn("current", "user", msg);
    }
}

/// Cross-session BM25 search.
pub fn cross_session_search(query: &str, top_k: usize) -> String {
    if query.len() < 3 {
        return "Query too short.".into();
    }
    let mem = session_memory();
    let results = match mem.lock() {
        Ok(m) => m.search(query, top_k),
        Err(_) => return "Memory lock failed.".into(),
    };
    if results.is_empty() {
        return "No results.".into();
    }
    let mut lines = vec![format!("Kimix search: \"{}\"", query)];
    for (i, (sid, _ti, score)) in results.iter().enumerate() {
        lines.push(format!("  {}. [{:.3}] session: {}", i + 1, score, sid));
    }
    lines.join("\n")
}

pub fn prune_stats() -> (usize, usize) {
    if let Ok(a) = prompt_adapter().lock() {
        (a.tokens_saved(), a.prune_count())
    } else {
        (0, 0)
    }
}
pub fn recall_stats() -> usize {
    if let Ok(e) = recall().lock() {
        e.turn_count()
    } else {
        0
    }
}

/// Set the context window on the global prompt adapter for auto-compact
/// observability (80% usage logging in `AgentPrompt::begin_turn`).
pub fn set_context_window(window: u64) {
    if let Ok(mut adapter) = prompt_adapter().lock() {
        adapter.set_context_window(window as usize);
    }
}

/// Set the effective context cap for the 80% observability ratio.
/// `0` clears the cap (use full context window).
pub fn set_max_effective_context_tokens(cap: u32) {
    if let Ok(mut adapter) = prompt_adapter().lock() {
        adapter.set_max_effective_context_tokens(cap);
    }
}

pub fn reset_all() {
    if let Some(r) = RECALL.get() {
        let _ = r.lock().map(|mut e| *e = KimixRecallEngine::new());
    }
    if let Some(p) = PROMPT.get() {
        let _ = p.lock().map(|mut a| *a = KimixPromptAdapter::new(""));
    }
    if let Some(d) = TOOL_DEDUP.get() {
        let _ = d.lock().map(|mut x| *x = ContentHashDeduper::new());
    }
    SOFT_NUDGE_INJECTIONS.store(0, Ordering::Relaxed);
    configure_context_economy(DEFAULT_SOFT_NUDGE_RATIO, true);
}
pub fn index_turn(user_msg: &str, assistant_msg: &str, is_compacted: bool) {
    if user_msg.len() < 10 {
        return;
    }
    let mut engine = recall().lock().expect("lock poisoned");
    let tn = engine.turn_count();
    engine.add_turn("user", user_msg, is_compacted, tn);
    engine.add_turn("assistant", assistant_msg, is_compacted, tn + 1);
    if tn.is_multiple_of(5) {
        let mem = session_memory();
        if let Ok(m) = mem.lock() {
            m.save_all();
        }
    }
}
pub fn reset_recall() {
    if let Some(engine) = RECALL.get() {
        let _ = engine.lock().map(|mut e| *e = KimixRecallEngine::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kimix_sampling_types::ConversationItem;
    use std::sync::Mutex;

    /// Process-global economy statics are shared across tests; serialize them.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_economy_lock<F: FnOnce()>(f: F) {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_all();
        f();
        reset_all();
    }

    #[test]
    fn admit_tool_result_item_dedups_second_large_payload() {
        with_economy_lock(|| {
            configure_context_economy(DEFAULT_SOFT_NUDGE_RATIO, true);
            let payload = "Z".repeat(400);
            let first =
                admit_tool_result_item(ConversationItem::tool_result("c1", payload.clone()));
            let second = admit_tool_result_item(ConversationItem::tool_result("c2", payload));
            match (&first, &second) {
                (ConversationItem::ToolResult(a), ConversationItem::ToolResult(b)) => {
                    assert!(a.content.starts_with('Z'));
                    assert!(
                        b.content.contains("duplicate content hash="),
                        "second tool result must be stubbed: {}",
                        b.content
                    );
                }
                _ => panic!("expected ToolResult items"),
            }
            let stats = context_economy_stats();
            assert_eq!(stats.dedup_count, 1);
            assert!(stats.dedup_tokens_saved > 0);
            assert!(stats.content_hash_dedup_enabled);
        });
    }

    #[test]
    fn admit_tool_payload_disabled_is_passthrough() {
        with_economy_lock(|| {
            configure_context_economy(DEFAULT_SOFT_NUDGE_RATIO, false);
            let payload = "Y".repeat(400);
            let a = admit_tool_payload(payload.clone());
            let b = admit_tool_payload(payload.clone());
            assert_eq!(a, payload);
            assert_eq!(b, payload);
            assert_eq!(tool_dedup_stats(), (0, 0));
            assert!(!content_hash_dedup_enabled());
        });
    }

    #[test]
    fn soft_nudge_injects_on_current_user_message_only() {
        with_economy_lock(|| {
            configure_context_economy(0.55, true);
            let mut msg = "please continue the long task with more details".to_string();
            inject_recall_context_with_usage(
                &mut msg,
                Some(ContextUsageHint {
                    estimated_tokens: 120_000,
                    context_window: 1_000_000,
                    max_effective_context_tokens: 200_000, // 60% of effective
                }),
            );
            assert!(
                msg.contains("context efficiency"),
                "soft nudge should appear: {msg}"
            );
            assert!(msg.contains("please continue the long task"));
            assert_eq!(context_economy_stats().soft_nudge_injections, 1);
        });
    }

    #[test]
    fn soft_nudge_zero_ratio_disables() {
        with_economy_lock(|| {
            configure_context_economy(0.0, true);
            let mut msg = "please continue the long task with more details".to_string();
            inject_recall_context_with_usage(
                &mut msg,
                Some(ContextUsageHint {
                    estimated_tokens: 120_000,
                    context_window: 200_000,
                    max_effective_context_tokens: 200_000,
                }),
            );
            assert!(
                !msg.contains("context efficiency"),
                "disabled soft nudge must not inject: {msg}"
            );
            assert_eq!(context_economy_stats().soft_nudge_injections, 0);
        });
    }

    #[test]
    fn economy_summary_non_empty() {
        with_economy_lock(|| {
            let s = context_economy_summary();
            assert!(s.contains("context-economy"));
            assert!(s.contains("dedup="));
        });
    }

    #[test]
    fn push_admitted_tool_result_dedups_via_handle_path() {
        with_economy_lock(|| {
            configure_context_economy(DEFAULT_SOFT_NUDGE_RATIO, true);
            let payload = "W".repeat(400);
            let a = admit_tool_result_item(ConversationItem::tool_result("a", payload.clone()));
            let b = admit_tool_result_item(ConversationItem::tool_result("b", payload));
            match b {
                ConversationItem::ToolResult(tr) => {
                    assert!(
                        tr.content.contains("duplicate content hash="),
                        "handle-path admit must stub: {}",
                        tr.content
                    );
                }
                _ => panic!("expected ToolResult"),
            }
            match a {
                ConversationItem::ToolResult(tr) => assert!(tr.content.starts_with('W')),
                _ => panic!("expected ToolResult"),
            }
            assert_eq!(context_economy_stats().dedup_count, 1);
        });
    }
}
