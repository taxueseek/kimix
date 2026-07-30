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
use std::path::PathBuf;
use std::sync::Mutex;

static RECALL: std::sync::OnceLock<Mutex<KimixRecallEngine>> = std::sync::OnceLock::new();
static PROMPT: std::sync::OnceLock<Mutex<KimixPromptAdapter>> = std::sync::OnceLock::new();
static SESSION_MEMORY: std::sync::OnceLock<Mutex<KimixSessionMemory>> = std::sync::OnceLock::new();
static TOOL_DEDUP: std::sync::OnceLock<Mutex<ContentHashDeduper>> = std::sync::OnceLock::new();

/// Soft efficiency band lower bound (ratio of effective context window).
const SOFT_NUDGE_RATIO: f64 = 0.55;

/// Hint for soft efficiency nudge (real chat-state token estimate).
#[derive(Debug, Clone, Copy)]
pub struct ContextUsageHint {
    pub estimated_tokens: u64,
    pub context_window: u64,
    /// `0` means no cap (use full `context_window`).
    pub max_effective_context_tokens: u32,
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
    if let Some(u) = usage {
        let effective = if u.max_effective_context_tokens > 0 {
            u.context_window
                .min(u64::from(u.max_effective_context_tokens))
        } else {
            u.context_window
        };
        if should_soft_efficiency_nudge(SOFT_NUDGE_RATIO, u.estimated_tokens, effective) {
            tracing::debug!(
                estimated_tokens = u.estimated_tokens,
                effective_window = effective,
                soft_nudge_ratio = SOFT_NUDGE_RATIO,
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

/// Cumulative tool content-hash dedup stats: `(dedup_count, tokens_saved)`.
pub fn tool_dedup_stats() -> (usize, usize) {
    tool_dedup()
        .lock()
        .map(|d| (d.dedup_count, d.tokens_saved))
        .unwrap_or((0, 0))
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
