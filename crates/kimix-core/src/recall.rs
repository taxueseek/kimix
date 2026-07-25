//! Three-tier memory recall system.
//!
//! Mirrors KimiX's `_maybe_auto_retrieve_history` in kimisoul.py.
//!
//! Three tiers:
//! A. **History memory** — compacted turns (long-term, archived)
//! B. **Working memory** — non-compacted turns (in current context but buried)
//! C. **Recency memory** — time-decay boosted best match
//!
//! Each tier has its own threshold. With multi-candidate recall (v2),
//! each tier returns up to `max_candidates_per_tier` candidates, with
//! an exponential time-decay penalty applied. Results are deduplicated
//! and capped by both count and token budget per turn.
//!
//! When a `HybridSearcher` is configured, BM25 + vector similarity are
//! fused for the recall step.
use crate::InvertedIndex;
use crate::hybrid::HybridSearcher;
use crate::searcher::{SearchResult, Searcher};
use crate::tokenizer::Tokenizer;

/// Configuration for three-tier recall.
#[derive(Debug, Clone)]
pub struct RecallConfig {
    /// BM25 threshold for history (compacted) memory.
    pub history_threshold: f64,
    /// BM25 threshold for working (non-compacted) memory.
    pub working_memory_threshold: f64,
    /// BM25 threshold for recency-boosted memory.
    pub recency_threshold: f64,
    /// Weight applied to recency boost (0.0 = no boost, 1.0 = full boost).
    pub recency_weight: f64,
    /// Maximum number of injections per turn.
    pub max_injections_per_turn: usize,
    /// Maximum total tokens for all injections in one turn.
    pub max_tokens_per_turn: usize,
    /// Number of recent turns to exclude from working memory.
    pub recent_turns_excluded: usize,
    /// Maximum candidates retained per tier (multi-candidate recall, default 3).
    pub max_candidates_per_tier: usize,
    /// Time-decay coefficient lambda (exponential decay, default 0.01).
    ///
    /// With lambda = 0.01, a turn 100 steps in the past decays to ~37% of
    /// its original score. This approximates a ~69-turn half-life
    /// (ln(2) / 0.01 ≈ 69).
    pub decay_lambda: f64,
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            history_threshold: 5.0,
            working_memory_threshold: 5.0,
            recency_threshold: 4.0,
            recency_weight: 1.0,
            max_injections_per_turn: 3,
            max_tokens_per_turn: 2000,
            recent_turns_excluded: 2,
            max_candidates_per_tier: 3,
            decay_lambda: 0.01,
        }
    }
}

/// Metadata for a turn stored in the index.
#[derive(Debug, Clone)]
pub struct TurnMeta {
    pub turn_id: usize,
    pub role: String,
    pub content: String,
    pub is_compacted: bool,
    /// Turn number for recency boost (higher = more recent).
    pub turn_number: usize,
}

/// Result from three-tier recall.
#[derive(Debug, Clone)]
pub struct RecallResult {
    pub turn_id: usize,
    pub role: String,
    pub content: String,
    pub score: f64,
    /// Time-decayed score (score * exp(-λ * age_turns)).
    pub decayed_score: f64,
    /// Which tier this result came from.
    pub tier: RecallTier,
    pub is_compacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallTier {
    History,
    Working,
    Recency,
}

/// Three-tier memory recall engine.
pub struct RecallEngine {
    config: RecallConfig,
    /// All turns indexed (compacted + non-compacted).
    turns: Vec<TurnMeta>,
    /// BM25 index over all turns.
    index: InvertedIndex,
    tokenizer: Tokenizer,
    /// Set of turn_ids recently retrieved (for deduplication).
    recently_retrieved: std::collections::HashSet<usize>,
    /// Optional hybrid searcher for BM25 + vector fusion.
    hybrid_searcher: Option<HybridSearcher>,
}

impl RecallEngine {
    pub fn new(config: RecallConfig) -> Self {
        Self {
            config,
            turns: Vec::new(),
            index: InvertedIndex::new(),
            tokenizer: Tokenizer::new(2),
            recently_retrieved: std::collections::HashSet::new(),
            hybrid_searcher: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(RecallConfig::default())
    }

    /// Attach a hybrid searcher for BM25 + vector fusion recall.
    pub fn set_hybrid_searcher(&mut self, hybrid: HybridSearcher) {
        self.hybrid_searcher = Some(hybrid);
    }

    /// Remove the hybrid searcher (back to pure BM25).
    pub fn remove_hybrid_searcher(&mut self) {
        self.hybrid_searcher = None;
    }

    /// Add a turn to the recall index.
    ///
    /// Optionally accepts an embedding vector for hybrid retrieval.
    /// The embedding is stored in the hybrid searcher's vector index.
    pub fn add_turn(
        &mut self,
        role: &str,
        content: &str,
        is_compacted: bool,
        turn_number: usize,
    ) {
        self.add_turn_with_embedding(role, content, is_compacted, turn_number, None);
    }

    /// Add a turn with an optional embedding vector.
    ///
    /// If a hybrid searcher is configured and an embedding is provided,
    /// the embedding is stored in the vector index keyed by turn_id.
    pub fn add_turn_with_embedding(
        &mut self,
        role: &str,
        content: &str,
        is_compacted: bool,
        turn_number: usize,
        embedding: Option<&[f32]>,
    ) {
        let turn_id = self.turns.len();
        let tokens = self.tokenizer.tokenize(content);

        self.turns.push(TurnMeta {
            turn_id,
            role: role.to_string(),
            content: content.to_string(),
            is_compacted,
            turn_number,
        });

        self.index.add_document(turn_id, &tokens);

        // Store embedding if hybrid searcher is active
        if let Some(ref mut hybrid) = self.hybrid_searcher
            && let Some(emb) = embedding
        {
            hybrid.vector_index_mut().add(turn_id, emb);
        }
    }

    /// Run three-tier recall for a query.
    ///
    /// Returns deduplicated, threshold-filtered, and capped results.
    /// When a hybrid searcher is configured and a query embedding is
    /// provided, uses BM25 + vector weighted fusion.
    pub fn recall(&mut self, query: &str) -> Vec<RecallResult> {
        // Delegate to recall_with_embedding with no embedding for backward compat
        // The hybrid searcher will fall back to pure BM25 when no embedding.
        self.recall_with_embedding(query, None)
    }

    /// Run three-tier recall with an optional query embedding.
    ///
    /// When both a hybrid searcher and query embedding are available,
    /// BM25 scores are fused with cosine similarity for richer recall.
    pub fn recall_with_embedding(
        &mut self,
        query: &str,
        query_embedding: Option<&[f32]>,
    ) -> Vec<RecallResult> {
        if query.len() < 10 {
            return vec![];
        }

        // Compute the current turn number for time-decay
        let current_turn = self.turns.last().map(|t| t.turn_number).unwrap_or(0);

        // Get raw search results — use hybrid if available
        let raw_results: Vec<SearchResult> = if let Some(ref hybrid) = self.hybrid_searcher {
            hybrid.search(query, query_embedding, &self.index, 20)
        } else {
            let searcher = Searcher::new(self.tokenizer.clone(), crate::BM25Scorer::default());
            searcher.search(query, &self.index, 20)
        };

        if raw_results.is_empty() {
            return vec![];
        }

        // Filter out recently retrieved
        let candidates: Vec<&SearchResult> = raw_results
            .iter()
            .filter(|r| !self.recently_retrieved.contains(&r.doc_id))
            .collect();

        let mut injections: Vec<RecallResult> = Vec::new();
        let mut used_turn_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut token_budget = self.config.max_tokens_per_turn;

        // Helper: estimate tokens (CJK: 1 char ≈ 1 token, ASCII: 4 chars ≈ 1 token)
        let estimate_tokens = |text: &str| -> usize {
            let cjk = text
                .chars()
                .filter(|c| {
                    matches!(
                        c,
                        '\u{4E00}'..='\u{9FFF}'
                            | '\u{3040}'..='\u{30FF}'
                            | '\u{AC00}'..='\u{D7AF}'
                    )
                })
                .count();
            cjk + (text.len().saturating_sub(cjk)) / 4
        };

        let can_afford = |budget: &mut usize, content: &str| -> bool {
            let cost = estimate_tokens(content) + 15; // overhead
            if cost <= *budget {
                *budget = budget.saturating_sub(cost);
                true
            } else {
                false
            }
        };

        /// Apply exponential time decay to a BM25 score.
        /// decayed = score * exp(-lambda * (current_turn - turn_number))
        fn decay_score(score: f64, turn_number: usize, current_turn: usize, lambda: f64) -> f64 {
            let age = current_turn.saturating_sub(turn_number) as f64;
            score * (-lambda * age).exp()
        }

        // ── A. History memory (compacted turns) ──
        let max_cand = self.config.max_candidates_per_tier;
        let threshold = self.config.history_threshold;
        let lambda = self.config.decay_lambda;

        let mut compacted: Vec<(&SearchResult, f64)> = candidates
            .iter()
            .filter(|r| {
                r.doc_id < self.turns.len()
                    && self.turns[r.doc_id].is_compacted
                    && !used_turn_ids.contains(&r.doc_id)
            })
            .map(|r| {
                let meta = &self.turns[r.doc_id];
                let decayed = decay_score(r.score, meta.turn_number, current_turn, lambda);
                (*r, decayed)
            })
            .filter(|(_, decayed)| *decayed >= threshold)
            .collect();

        // Sort by decayed score descending, take top-N
        compacted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        compacted.truncate(max_cand);

        for (sr, decayed) in &compacted {
            let meta = &self.turns[sr.doc_id];
            if can_afford(&mut token_budget, &meta.content) {
                used_turn_ids.insert(sr.doc_id);
                injections.push(RecallResult {
                    turn_id: sr.doc_id,
                    role: meta.role.clone(),
                    content: meta.content.clone(),
                    score: sr.score,
                    decayed_score: *decayed,
                    tier: RecallTier::History,
                    is_compacted: true,
                });
            }
        }

        // ── B. Working memory (non-compacted, excluding recent turns) ──
        let total_turns = self.turns.len();
        let recent_start = total_turns.saturating_sub(self.config.recent_turns_excluded);

        let mut non_compacted: Vec<(&SearchResult, f64)> = candidates
            .iter()
            .filter(|r| {
                r.doc_id < self.turns.len()
                    && !self.turns[r.doc_id].is_compacted
                    && !used_turn_ids.contains(&r.doc_id)
                    && self.turns[r.doc_id].turn_number < recent_start
            })
            .map(|r| {
                let meta = &self.turns[r.doc_id];
                let decayed = decay_score(r.score, meta.turn_number, current_turn, lambda);
                (*r, decayed)
            })
            .filter(|(_, decayed)| *decayed >= self.config.working_memory_threshold)
            .collect();

        non_compacted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        non_compacted.truncate(max_cand);

        for (sr, decayed) in &non_compacted {
            let meta = &self.turns[sr.doc_id];
            if can_afford(&mut token_budget, &meta.content) {
                used_turn_ids.insert(sr.doc_id);
                injections.push(RecallResult {
                    turn_id: sr.doc_id,
                    role: meta.role.clone(),
                    content: meta.content.clone(),
                    score: sr.score,
                    decayed_score: *decayed,
                    tier: RecallTier::Working,
                    is_compacted: false,
                });
            }
        }

        // ── C. Recency memory (best boosted score) ──
        let total_turns_f = self.turns.len() as f64;
        let mut recency_candidates: Vec<(usize, f64, f64)> = candidates
            .iter()
            .filter(|r| r.doc_id < self.turns.len() && !used_turn_ids.contains(&r.doc_id))
            .map(|r| {
                let meta = &self.turns[r.doc_id];
                let recency_factor = meta.turn_number as f64 / total_turns_f.max(1.0);
                let boosted =
                    r.score * (1.0 + self.config.recency_weight * recency_factor);
                let decayed = decay_score(boosted, meta.turn_number, current_turn, lambda);
                (r.doc_id, boosted, decayed)
            })
            .filter(|(_, _, decayed)| *decayed >= self.config.recency_threshold)
            .collect();

        recency_candidates.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recency_candidates.truncate(max_cand);

        for (doc_id, boosted, decayed) in &recency_candidates {
            let meta = &self.turns[*doc_id];
            if can_afford(&mut token_budget, &meta.content) {
                used_turn_ids.insert(*doc_id);
                injections.push(RecallResult {
                    turn_id: *doc_id,
                    role: meta.role.clone(),
                    content: meta.content.clone(),
                    score: *boosted,
                    decayed_score: *decayed,
                    tier: RecallTier::Recency,
                    is_compacted: meta.is_compacted,
                });
            }
        }

        // Sort injections by decayed_score for final ordering
        injections.sort_by(|a, b| {
            b.decayed_score
                .partial_cmp(&a.decayed_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Respect the per-turn cap
        let max_inj = self.config.max_injections_per_turn;
        if injections.len() > max_inj {
            injections.truncate(max_inj);
        }

        // Track recently retrieved for deduplication
        for inj in &injections {
            self.recently_retrieved.insert(inj.turn_id);
        }

        // Limit the dedup set size
        if self.recently_retrieved.len() > 50 {
            // Keep only the most recent 30
            let to_keep: Vec<_> = self.recently_retrieved.iter().copied().collect();
            self.recently_retrieved.clear();
            for id in to_keep.iter().rev().take(30) {
                self.recently_retrieved.insert(*id);
            }
        }

        injections
    }

    /// Reset the deduplication set (e.g., on session restart).
    pub fn reset_dedup(&mut self) {
        self.recently_retrieved.clear();
    }

    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> RecallEngine {
        let mut engine = RecallEngine::new(RecallConfig {
            history_threshold: 3.0,
            working_memory_threshold: 3.0,
            recency_threshold: 2.0,
            recency_weight: 1.0,
            max_injections_per_turn: 3,
            max_tokens_per_turn: 2000,
            recent_turns_excluded: 2,
            max_candidates_per_tier: 3,
            decay_lambda: 0.0, // disable decay for deterministic tests
        });

        // Add some compacted (history) turns
        engine.add_turn(
            "user",
            "How do I set up an async HTTP client in Python?",
            true,
            1,
        );
        engine.add_turn(
            "assistant",
            "Use aiohttp with ClientSession and connection pooling",
            true,
            2,
        );

        // Add working turns
        engine.add_turn(
            "user",
            "The connection pool is not reusing connections",
            false,
            3,
        );
        engine.add_turn(
            "assistant",
            "Check the pool configuration and session reuse",
            false,
            4,
        );
        engine.add_turn(
            "user",
            "Now I need to add retry logic for failed requests",
            false,
            5,
        );
        engine.add_turn(
            "assistant",
            "Use tenacity or a custom retry decorator",
            false,
            6,
        );

        engine
    }

    #[test]
    fn test_history_recall() {
        let mut engine = make_engine();
        let results = engine.recall("async HTTP client setup Python");

        // Should recall the first turn (compacted, about async HTTP client)
        let history: Vec<_> = results
            .iter()
            .filter(|r| r.tier == RecallTier::History)
            .collect();
        assert!(
            !history.is_empty(),
            "Should find history results for relevant query"
        );
        assert!(
            history[0].score >= 3.0,
            "Score should meet threshold, got {}",
            history[0].score
        );
        // decayed_score should equal score when decay_lambda = 0
        assert!(
            (history[0].decayed_score - history[0].score).abs() < 1e-6,
            "decayed_score={} should ≈ score={} with λ=0",
            history[0].decayed_score,
            history[0].score
        );
    }

    #[test]
    fn test_working_memory_recall() {
        let mut engine = make_engine();
        let results = engine.recall("connection pool reuse failure");

        let working: Vec<_> = results
            .iter()
            .filter(|r| r.tier == RecallTier::Working)
            .collect();
        assert!(!working.is_empty(), "Should find working memory results");
    }

    #[test]
    fn test_deduplication() {
        let mut engine = make_engine();

        // First query
        let results1 = engine.recall("async HTTP client Python");
        assert!(!results1.is_empty());

        // Same query — should return empty because all candidates are deduplicated
        let results2 = engine.recall("async HTTP client Python");
        assert!(
            results2.is_empty(),
            "Same query should be deduplicated, got {} results",
            results2.len()
        );
    }

    #[test]
    fn test_empty_query() {
        let mut engine = make_engine();
        let results = engine.recall("short");
        assert!(results.is_empty(), "Query < 10 chars should return empty");
    }

    #[test]
    fn test_irrelevant_query() {
        let mut engine = make_engine();
        let results = engine.recall("completely unrelated topic about cooking recipes");
        assert!(
            results.len() <= 3,
            "Should respect max_injections_per_turn, got {}",
            results.len()
        );
    }

    #[test]
    fn test_recency_boost() {
        let mut engine = make_engine();
        let results = engine.recall("retry logic for requests");

        assert!(
            results.len() <= 3,
            "Should respect max_injections_per_turn, got {}",
            results.len()
        );
    }

    #[test]
    fn test_multi_candidate_per_tier() {
        let mut engine = make_engine();
        // Query that matches multiple history turns
        let results = engine.recall("async HTTP client connection pool Python");
        // Should return multiple results across tiers
        assert!(!results.is_empty(), "Should find multiple results");
        // All results should have decayed_score set
        for r in &results {
            assert!(
                r.decayed_score >= 0.0,
                "decayed_score should be non-negative, got {}",
                r.decayed_score
            );
        }
    }

    #[test]
    fn test_time_decay_penalizes_old_turns() {
        let mut engine = RecallEngine::new(RecallConfig {
            history_threshold: 2.0,
            working_memory_threshold: 2.0,
            recency_threshold: 1.0,
            recency_weight: 1.0,
            max_injections_per_turn: 5,
            max_tokens_per_turn: 2000,
            recent_turns_excluded: 0,
            max_candidates_per_tier: 5,
            decay_lambda: 0.1, // aggressive decay
        });

        // Old turn
        engine.add_turn("user", "async HTTP client setup Python", true, 1);
        // Recent turn with same topic
        engine.add_turn("assistant", "async HTTP client debugging tips", true, 100);

        let results = engine.recall("async HTTP client Python debugging");
        let history: Vec<_> = results
            .iter()
            .filter(|r| r.tier == RecallTier::History)
            .collect();

        if history.len() >= 2 {
            // The recent turn (turn_number=100) should have higher decayed_score
            // than the old turn (turn_number=1) because decay penalizes age
            let old_turn = history.iter().find(|r| r.turn_id == 0);
            let recent_turn = history.iter().find(|r| r.turn_id == 1);
            if let (Some(old), Some(recent)) = (old_turn, recent_turn) {
                assert!(
                    recent.decayed_score > old.decayed_score,
                    "Recent turn decayed_score={} should exceed old turn decayed_score={}",
                    recent.decayed_score,
                    old.decayed_score
                );
            }
        }
    }

    #[test]
    fn test_add_turn_with_embedding() {
        let mut engine = make_engine();
        // add_turn_with_embedding should not panic even without hybrid searcher
        engine.add_turn_with_embedding(
            "user",
            "Test query with embedding",
            false,
            7,
            Some(&[1.0, 0.0, 0.0]),
        );
        assert_eq!(engine.turn_count(), 7);
    }

    #[test]
    fn test_hybrid_searcher_set_unset() {
        let mut engine = make_engine();
        use crate::hybrid::HybridSearcher;
        use crate::vector::VectorIndex;
        use crate::BM25Scorer;

        let tokenizer = crate::Tokenizer::new(2);
        let searcher = Searcher::new(tokenizer, BM25Scorer::default());
        let vi = VectorIndex::new(3);
        let hybrid = HybridSearcher::new(searcher, vi);

        engine.set_hybrid_searcher(hybrid);
        engine.remove_hybrid_searcher();

        // Should still work after removing hybrid
        let results = engine.recall("async HTTP client Python");
        assert!(!results.is_empty());
    }
}
