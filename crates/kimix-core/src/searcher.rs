//! Search pipeline: tokenize → score → rank → rerank.
//!
//! Supports BM25 ranking and optional MMR diversity reranking.
use crate::index::InvertedIndex;
use crate::scorer::BM25Scorer;
use crate::tokenizer::Tokenizer;

/// A single search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub doc_id: usize,
    pub score: f64,
}

/// MMR (Maximal Marginal Relevance) reranker.
///
/// Balances relevance (high BM25 score) with diversity (low similarity to already-selected docs).
pub struct MMRReranker {
    /// λ: relevance weight. 1.0 = pure relevance, 0.0 = pure diversity.
    lambda: f64,
}

impl MMRReranker {
    pub fn new(lambda: f64) -> Self {
        Self { lambda }
    }

    /// Rerank results using MMR.
    ///
    /// `results`: (doc_id, score) from BM25 search.
    /// `index`: inverted index for computing document similarity.
    pub fn rerank(
        &self,
        results: &[SearchResult],
        index: &InvertedIndex,
        top_k: usize,
    ) -> Vec<SearchResult> {
        if results.is_empty() {
            return vec![];
        }

        let n = top_k.min(results.len());
        let mut selected: Vec<SearchResult> = Vec::with_capacity(n);
        let mut remaining: Vec<usize> = (0..results.len()).collect();

        while selected.len() < n && !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_score = f64::NEG_INFINITY;

            for (pos, &rem_idx) in remaining.iter().enumerate() {
                let relevance = results[rem_idx].score;

                // Compute max similarity to already-selected docs
                let max_sim = selected
                    .iter()
                    .map(|s| self.doc_similarity(results[rem_idx].doc_id, s.doc_id, index))
                    .fold(0.0f64, f64::max);

                let mmr = self.lambda * relevance - (1.0 - self.lambda) * max_sim;

                if mmr > best_score {
                    best_score = mmr;
                    best_idx = pos;
                }
            }

            let rem_idx = remaining.remove(best_idx);
            selected.push(results[rem_idx].clone());
        }

        selected
    }

    /// Compute Jaccard similarity between two documents based on their token sets.
    fn doc_similarity(&self, doc_a: usize, doc_b: usize, index: &InvertedIndex) -> f64 {
        index.jaccard_similarity(doc_a, doc_b)
    }
}

/// Search engine combining tokenizer, index, scorer, and reranker.
pub struct Searcher {
    tokenizer: Tokenizer,
    scorer: BM25Scorer,
}

impl Searcher {
    pub fn new(tokenizer: Tokenizer, scorer: BM25Scorer) -> Self {
        Self { tokenizer, scorer }
    }

    /// Search the index with a text query.
    ///
    /// Returns top-k results sorted by relevance (highest first).
    /// Auto-selects WAND-pruned search for large indexes (> 1000 docs) and
    /// full-scan search (with MMR reranking) for small indexes.
    pub fn search(&self, query: &str, index: &InvertedIndex, top_k: usize) -> Vec<SearchResult> {
        if index.num_docs > 1000 {
            return self.search_wand(query, index, top_k);
        }
        self.search_full(query, index, top_k)
    }

    /// Full-scan search: scores all documents, with MMR diversity reranking.
    /// Used for small indexes where full scan is fast enough.
    fn search_full(&self, query: &str, index: &InvertedIndex, top_k: usize) -> Vec<SearchResult> {
        let query_tokens = self.tokenizer.tokenize(query);
        if query_tokens.is_empty() || index.num_docs == 0 {
            return vec![];
        }

        // Score all documents
        let mut results: Vec<SearchResult> = (0..index.num_docs)
            .map(|doc_id| {
                let score = self.scorer.score(&query_tokens, doc_id, index);
                SearchResult { doc_id, score }
            })
            .filter(|r| r.score > 0.0)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if top_k == 0 {
            return vec![];
        }

        // Truncate the candidate pool in place. MMR borrows it, so no clone is
        // needed before either reranking or returning the ranked results.
        results.truncate(top_k.saturating_mul(2));
        if results.len() > top_k {
            MMRReranker::new(0.7).rerank(&results, index, top_k)
        } else {
            results
        }
    }

    /// WAND-pruned search: only scores documents containing query terms, with
    /// upper-bound pruning for early termination. Optimized for large indexes.
    pub fn search_wand(
        &self,
        query: &str,
        index: &InvertedIndex,
        top_k: usize,
    ) -> Vec<SearchResult> {
        let query_tokens = self.tokenizer.tokenize(query);
        if query_tokens.is_empty() || index.num_docs == 0 {
            return vec![];
        }

        // Count query term frequencies
        use std::collections::HashMap;
        let mut qf_map: HashMap<&str, u32> = HashMap::new();
        for t in &query_tokens {
            *qf_map.entry(t.as_str()).or_insert(0) += 1;
        }

        // 1. Compute per-term upper bounds, sorted descending
        let mut term_ubs: Vec<(&str, f64, u32)> = qf_map
            .iter()
            .map(|(&term, &qf)| {
                let ub = self.scorer.max_term_score(term, index) * (qf as f64);
                (term, ub, qf)
            })
            .filter(|(_, ub, _)| *ub > 0.0)
            .collect();
        term_ubs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if term_ubs.is_empty() {
            return vec![];
        }

        let total_potential: f64 = term_ubs.iter().map(|(_, ub, _)| ub).sum();

        // 2. Collect candidate docs (only docs containing at least one query term)
        let candidate_docs: Vec<usize> = {
            use std::collections::HashSet;
            let mut set: HashSet<usize> = HashSet::new();
            for (term, _, _) in &term_ubs {
                if let Some(pl) = index.get_postings(term) {
                    for posting in &pl.postings {
                        set.insert(posting.doc_id);
                    }
                }
            }
            let mut docs: Vec<usize> = set.into_iter().collect();
            docs.sort_unstable();
            docs
        };

        // 3. Score candidates with upper-bound pruning (DAAT)
        let mut results: Vec<SearchResult> =
            Vec::with_capacity(candidate_docs.len().min(top_k * 2));

        for &doc_id in &candidate_docs {
            let mut score = 0.0;
            let mut scored_potential = 0.0;

            // Evaluate terms in descending upper-bound order for best pruning
            for &(term, ub, qf) in &term_ubs {
                let term_score = self.scorer.score_term(term, doc_id, qf, index);
                score += term_score;
                scored_potential += ub;

                // Pruning: if remaining potential can't reach top-K threshold, terminate
                if results.len() >= top_k {
                    // Find the current K-th best score (min-heap top)
                    let threshold = results
                        .iter()
                        .map(|r| r.score)
                        .fold(f64::INFINITY, f64::min);
                    if score + (total_potential - scored_potential) < threshold {
                        break;
                    }
                }
            }

            if score > 0.0 {
                results.push(SearchResult { doc_id, score });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::InvertedIndex;
    use crate::scorer::BM25Scorer;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_chinese_search() {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer, scorer);

        let mut idx = InvertedIndex::new();
        let tokenizer = Tokenizer::new(2);

        idx.add_document(0, &tokenizer.tokenize("帮我写一个Python的异步HTTP客户端"));
        idx.add_document(1, &tokenizer.tokenize("Rust的TUI应用怎么实现状态栏"));
        idx.add_document(2, &tokenizer.tokenize("Python异步编程的最佳实践"));

        let results = searcher.search("异步HTTP客户端", &idx, 3);
        println!("Search results: {:?}", results);
        assert!(!results.is_empty());
        // The first result should be doc 0 (most relevant)
        assert_eq!(results[0].doc_id, 0);
    }

    #[test]
    fn test_english_search() {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer, scorer);

        let mut idx = InvertedIndex::new();
        let tokenizer = Tokenizer::new(2);

        idx.add_document(
            0,
            &tokenizer.tokenize("async HTTP client with connection pool"),
        );
        idx.add_document(1, &tokenizer.tokenize("Rust TUI application status bar"));
        idx.add_document(2, &tokenizer.tokenize("Python async programming guide"));

        let results = searcher.search("async HTTP client", &idx, 3);
        println!("Search results: {:?}", results);
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, 0);
    }

    #[test]
    fn test_empty_query() {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer, scorer);

        let mut idx = InvertedIndex::new();
        idx.add_document(0, &["hello".into(), "world".into()]);

        let results = searcher.search("", &idx, 3);
        assert!(results.is_empty());
    }

    #[test]
    fn test_no_match() {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer.clone(), scorer);

        let mut idx = InvertedIndex::new();
        idx.add_document(0, &tokenizer.tokenize("hello world"));

        let results = searcher.search("xyzzy plugh", &idx, 3);
        assert!(results.is_empty());
    }
}
