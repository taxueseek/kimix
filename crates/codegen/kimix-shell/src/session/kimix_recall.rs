//! Kimix recall + prune + cross-session search for session turns.
//!
//! Three integrated capabilities:
//! 1. **BM25 recall**: CJK bigram 3-tier memory, injected as system-reminder
//! 2. **Context-budget prune**: Tracks tool-result tokens, reports savings
//! 3. **Cross-session search**: BM25 search across all sessions
use kimix_bridge::{KimixPromptAdapter, KimixRecallEngine, KimixSessionMemory};
use std::path::PathBuf;
use std::sync::Mutex;

static RECALL: std::sync::OnceLock<Mutex<KimixRecallEngine>> = std::sync::OnceLock::new();
static PROMPT: std::sync::OnceLock<Mutex<KimixPromptAdapter>> = std::sync::OnceLock::new();
static SESSION_MEMORY: std::sync::OnceLock<Mutex<KimixSessionMemory>> = std::sync::OnceLock::new();

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
    if !prefix.is_empty() {
        *user_message = format!("{}\n{}", prefix, user_message);
    }
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
pub fn reset_all() {
    if let Some(r) = RECALL.get() {
        let _ = r.lock().map(|mut e| *e = KimixRecallEngine::new());
    }
    if let Some(p) = PROMPT.get() {
        let _ = p.lock().map(|mut a| *a = KimixPromptAdapter::new(""));
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
