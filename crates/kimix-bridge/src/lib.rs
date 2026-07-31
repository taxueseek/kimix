//! Kimix-kimix-bridge — integrates kimix crates into Kimix-CLI agent runtime.
//!

//! Provides:
//! - **KimixRecallEngine**: CJK-aware BM25 3-tier recall, drop-in for Kimix-memory search
//! - **KimixPromptAdapter**: 4-layer cache-friendly prompt with context-budget prune
//! - **KimixSessionMemory**: Session-level memory with persistence
//!

//! Architecture:
//! ```text
//! Kimix TUI + Agent Runtime
//!         │
//!         ├── Kimix-kimix-bridge (this crate)
//!         │       ├── kimix-prompt  (4-layer prompt + prune)
//!         │       ├── kimix-memory  (session persistence)
//!         │       └── kimix-core    (BM25 + 3-tier recall)
//!         │
//!         └── Kimix-tools / Kimix-shell (unchanged)
//! ```
use kimix_core::{
    BM25Scorer, HybridSearcher, LOCAL_EMBED_DIM, RecallConfig, RecallEngine, RecallTier, Searcher,
    Tokenizer, VectorIndex, local_embedding,
};
#[cfg(test)]
mod tests;
use kimix_agent_memory::MemoryManager;
use kimix_prompt::{AgentPrompt, PromptConfig, RecallInjection};
use std::path::PathBuf;
use std::sync::Mutex;

// Re-exports for shell/runtime without a direct kimix-prompt dep.
pub use kimix_prompt::{
    ContentHashDeduper, SOFT_EFFICIENCY_NUDGE, should_soft_efficiency_nudge,
};

// ============================================================================
// KimixRecallEngine — drop-in for Kimix-memory's search
// ============================================================================

/// CJK-aware BM25 recall engine with 3-tier memory (history/working/recency).
///
/// More accurate than Kimix's default n-gram search for CJK text because
/// kimix-core uses overlapping bigrams specifically optimized for Chinese.
pub struct KimixRecallEngine {
    engine: RecallEngine,
}

impl KimixRecallEngine {
    pub fn new() -> Self {
        let mut engine = RecallEngine::new(RecallConfig {
            history_threshold: 5.0,
            working_memory_threshold: 5.0,
            recency_threshold: 4.0,
            recency_weight: 1.0,
            max_injections_per_turn: 3,
            max_tokens_per_turn: 2000,
            recent_turns_excluded: 2,
            max_candidates_per_tier: 3,
            decay_lambda: 0.01,
        });
        // 接入本地哈希 embedding 的 hybrid 检索（BM25 + 向量融合，无外部 API 依赖）。
        // embedding 不可用时 HybridSearcher 自动回退纯 BM25。
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer.clone(), scorer);
        let vector_index = VectorIndex::new(LOCAL_EMBED_DIM);
        engine.set_hybrid_searcher(HybridSearcher::new(searcher, vector_index));
        Self { engine }
    }

    /// Add a conversation turn to the recall index (with local embedding for hybrid retrieval).
    pub fn add_turn(&mut self, role: &str, content: &str, is_compacted: bool, turn_number: usize) {
        let embedding = local_embedding(content, LOCAL_EMBED_DIM);
        self.engine
            .add_turn_with_embedding(role, content, is_compacted, turn_number, Some(&embedding));
    }

    /// Run 3-tier recall for a query. Returns formatted text for prompt injection.
    pub fn recall_and_format(&mut self, query: &str, max_chars: usize) -> String {
        let query_embedding = local_embedding(query, LOCAL_EMBED_DIM);
        let results = self.engine.recall_with_embedding(query, Some(&query_embedding));
        if results.is_empty() {
            return String::new();
        }

        let mut lines = vec!["[Kimix auto-recall]".to_string()];
        let mut total = lines[0].len();

        for r in &results {
            let snippet = format!(
                "[{}] (score: {:.2}): {}",
                r.role,
                r.score,
                truncate_str(&r.content, 100)
            );
            if total + snippet.len() > max_chars {
                break;
            }
            total += snippet.len() + 1;
            lines.push(snippet);
        }

        lines.join("\n")
    }

    pub fn turn_count(&self) -> usize {
        self.engine.turn_count()
    }
}

impl Default for KimixRecallEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// KimixPromptAdapter — 4-layer cache-friendly prompt + context-budget prune
// ============================================================================

/// Adapter that wraps kimix-prompt's AgentPrompt for use with Kimix's agent.
///
/// Key features over Kimix's prompt system:
/// 1. **Stable system prompt** — never modified mid-session (max KV cache reuse)
/// 2. **Independent injection pipeline** — recall results as separate messages
/// 3. **Stale injection stripping** — removes old system-reminders before each turn
/// 4. **Context-budget prune** — removes consumed tool results (-41.7% token, +2.48pp)
pub struct KimixPromptAdapter {
    prompt: AgentPrompt,
}

impl KimixPromptAdapter {
    pub fn new(system_prompt: &str) -> Self {
        let config = PromptConfig {
            context_budget_prune: true,
            ..PromptConfig::default()
        };
        let mut prompt = AgentPrompt::new(config);
        prompt.set_system_prompt(system_prompt.to_string());
        Self { prompt }
    }

    pub fn with_prune_disabled(system_prompt: &str) -> Self {
        let config = PromptConfig {
            context_budget_prune: false,
            ..PromptConfig::default()
        };
        let mut prompt = AgentPrompt::new(config);
        prompt.set_system_prompt(system_prompt.to_string());
        Self { prompt }
    }

    /// Begin a new turn with recall injections.
    /// Returns the messages to send to the API.
    pub fn begin_turn(
        &mut self,
        user_input: &str,
        recall_results: &[kimix_core::RecallResult],
    ) -> Vec<kimix_prompt::Message> {
        let injections: Vec<RecallInjection> = recall_results
            .iter()
            .map(|r| RecallInjection {
                tier: match r.tier {
                    RecallTier::History => "history".into(),
                    RecallTier::Working => "working".into(),
                    RecallTier::Recency => "recency".into(),
                },
                role: r.role.clone(),
                content: r.content.clone(),
                score: r.score,
                is_compacted: r.is_compacted,
            })
            .collect();

        self.prompt.begin_turn(user_input, &injections).to_vec()
    }

    /// Record the final assistant response.
    pub fn record_response(&mut self, response: &str) {
        self.prompt.record_response(response);
    }

    /// Tokens saved by context-budget pruning (cumulative).
    pub fn tokens_saved(&self) -> usize {
        self.prompt.tokens_saved
    }

    /// Number of prune passes executed.
    pub fn prune_count(&self) -> usize {
        self.prompt.prune_count
    }

    /// Current message count.
    pub fn message_count(&self) -> usize {
        self.prompt.message_count()
    }

    /// Current turn count.
    pub fn turn_count(&self) -> usize {
        self.prompt.turn_count()
    }

    /// Whether prune is enabled.
    pub fn is_prune_enabled(&self) -> bool {
        self.prompt.is_prune_enabled()
    }

    /// Set the context window for auto-compact observability (80% usage logging).
    pub fn set_context_window(&mut self, window: usize) {
        self.prompt.set_context_window(window);
    }

    /// Set the effective context cap for the 80% observability ratio.
    /// `0` clears the cap.
    pub fn set_max_effective_context_tokens(&mut self, cap: u32) {
        self.prompt
            .set_max_effective_context_tokens(cap as usize);
    }
}

// ============================================================================
// KimixSessionMemory — session-level persistence
// ============================================================================

/// Session memory backed by kimix-memory.
pub struct KimixSessionMemory {
    manager: Mutex<MemoryManager>,
}

impl KimixSessionMemory {
    pub fn new(storage_dir: PathBuf) -> Self {
        let manager = MemoryManager::new(storage_dir, true);
        Self {
            manager: Mutex::new(manager),
        }
    }

    /// Add a turn to the current session.
    pub fn add_turn(&self, session_id: &str, role: &str, content: &str) {
        let mut mgr = self.manager.lock().expect("lock poisoned");
        let session = mgr.get_session(session_id);
        session.add_turn(role, content);
    }

    /// Search across all sessions.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, usize, f64)> {
        let mut mgr = self.manager.lock().expect("lock poisoned");
        mgr.load_all_sessions();
        mgr.cross_session_search(query, top_k)
    }

    /// Save all sessions to disk.
    pub fn save_all(&self) {
        if let Ok(mgr) = self.manager.lock() {
            mgr.save_all().ok();
        }
    }

    /// Get the number of turns in a session.
    pub fn turn_count(&self, session_id: &str) -> usize {
        let mut mgr = self.manager.lock().expect("lock poisoned");
        mgr.get_session(session_id).len()
    }
}

// ============================================================================
// Utilities
// ============================================================================

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}
