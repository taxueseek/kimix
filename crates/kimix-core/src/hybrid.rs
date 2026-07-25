//! Hybrid retrieval combining BM25 text search with vector similarity.
//!
//! Inspired by grok-build's vector 70% + text 30% fusion strategy.
//! Falls back to pure BM25 when no query embedding is provided or the vector
//! index is empty.
use crate::index::InvertedIndex;
use crate::searcher::{SearchResult, Searcher};
use crate::vector::VectorIndex;

/// Hybrid search engine combining BM25 text retrieval with vector similarity.
///
/// # Score Fusion
///
/// Both BM25 and cosine similarity scores are min-max normalized to [0, 1],
/// then fused as a weighted sum:
///
/// ```text
/// final = text_weight * bm25_norm + vector_weight * cos_sim_norm
/// ```
///
/// Results are then diversity-reranked with MMR.
pub struct HybridSearcher {
    searcher: Searcher,
    vector_index: VectorIndex,
    /// Vector retrieval weight (grok-build default: 0.7).
    vector_weight: f64,
    /// Text retrieval weight (grok-build default: 0.3).
    text_weight: f64,
    /// MMR lambda parameter — relevance vs diversity trade-off.
    mmr_lambda: f64,
}

impl HybridSearcher {
    /// Create a new hybrid searcher.
    ///
    /// Defaults to grok-build weights: 70% vector, 30% text.
    pub fn new(searcher: Searcher, vector_index: VectorIndex) -> Self {
        Self {
            searcher,
            vector_index,
            vector_weight: 0.7,
            text_weight: 0.3,
            mmr_lambda: 0.7,
        }
    }

    /// Create with custom weights and MMR lambda.
    pub fn with_weights(
        searcher: Searcher,
        vector_index: VectorIndex,
        vector_weight: f64,
        text_weight: f64,
        mmr_lambda: f64,
    ) -> Self {
        Self {
            searcher,
            vector_index,
            vector_weight,
            text_weight,
            mmr_lambda,
        }
    }

    /// Access the underlying vector index.
    pub fn vector_index(&self) -> &VectorIndex {
        &self.vector_index
    }

    /// Mutable access to the underlying vector index.
    pub fn vector_index_mut(&mut self) -> &mut VectorIndex {
        &mut self.vector_index
    }

    /// Access the underlying text searcher.
    pub fn searcher(&self) -> &Searcher {
        &self.searcher
    }

    /// Hybrid search: BM25 + vector weighted fusion with MMR diversity rerank.
    ///
    /// If `query_embedding` is `None` or the vector index is empty, falls back
    /// to pure BM25 with MMR reranking.
    ///
    /// Returns top-k results sorted by fused score (descending).
    pub fn search(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        index: &InvertedIndex,
        top_k: usize,
    ) -> Vec<SearchResult> {
        // Fetch a pool larger than top_k so MMR has candidates to select from
        let pool_size = (top_k * 3).max(10);

        // BM25 text search
        let bm25_results = self.searcher.search(query, index, pool_size);

        // If no BM25 results, return empty
        if bm25_results.is_empty() {
            return vec![];
        }

        // Try vector search
        let has_vector = query_embedding.is_some() && !self.vector_index.is_empty();
        let vector_results: Vec<(usize, f64)> = if let Some(q_emb) = query_embedding {
            self.vector_index.search(q_emb, pool_size)
        } else {
            vec![]
        };

        // Build doc_id → score maps for fusion
        let mut fused: Vec<SearchResult> = if has_vector && !vector_results.is_empty() {
            fuse_scores(
                &bm25_results,
                &vector_results,
                self.text_weight,
                self.vector_weight,
            )
        } else {
            // Pure BM25 fallback
            bm25_results
        };

        // Sort by score descending
        fused.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if fused.len() <= top_k {
            return fused;
        }

        // Apply MMR diversity rerank
        mmr_rerank(&fused, index, top_k, self.mmr_lambda)
    }
}

/// Fuse BM25 and vector scores using min-max normalization + weighted sum.
fn fuse_scores(
    bm25: &[SearchResult],
    vectors: &[(usize, f64)],
    text_weight: f64,
    vector_weight: f64,
) -> Vec<SearchResult> {
    // Collect all unique doc_ids from both sources
    let mut all_docs: std::collections::HashMap<usize, (f64, f64)> =
        std::collections::HashMap::new();

    for r in bm25 {
        all_docs.entry(r.doc_id).or_insert_with(|| (0.0, 0.0)).0 = r.score;
    }
    for &(doc_id, cos_sim) in vectors {
        all_docs.entry(doc_id).or_insert_with(|| (0.0, 0.0)).1 = cos_sim;
    }

    if all_docs.is_empty() {
        return vec![];
    }

    // Min-max normalize BM25 scores
    let bm25_values: Vec<f64> = all_docs.values().map(|(s, _)| *s).collect();
    let (bm25_min, bm25_max) = min_max(&bm25_values);

    // Cosine similarity is already in [-1, 1], normalize to [0, 1]
    let cos_values: Vec<f64> = all_docs.values().map(|(_, c)| *c).collect();
    let (cos_min, cos_max) = min_max(&cos_values);

    let norm_bm25 = |s: f64| -> f64 {
        if (bm25_max - bm25_min).abs() < f64::EPSILON {
            if bm25_max > 0.0 { 1.0 } else { 0.0 }
        } else {
            (s - bm25_min) / (bm25_max - bm25_min)
        }
    };

    let norm_cos = |c: f64| -> f64 {
        if (cos_max - cos_min).abs() < f64::EPSILON {
            if cos_max > 0.0 { 1.0 } else { 0.0 }
        } else {
            (c - cos_min) / (cos_max - cos_min)
        }
    };

    // Weighted fusion
    let mut results: Vec<SearchResult> = all_docs
        .into_iter()
        .map(|(doc_id, (bm25_s, cos_s))| {
            let score = text_weight * norm_bm25(bm25_s) + vector_weight * norm_cos(cos_s);
            SearchResult { doc_id, score }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

fn min_max(values: &[f64]) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for &v in values {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if min == f64::MAX {
        (0.0, 0.0)
    } else {
        (min, max)
    }
}

/// MMR (Maximal Marginal Relevance) reranking for diversity.
///
/// Selects top_k results balancing relevance (score) with diversity
/// (dissimilarity to already-selected documents, measured by Jaccard
/// distance on token sets).
fn mmr_rerank(
    results: &[SearchResult],
    index: &InvertedIndex,
    top_k: usize,
    lambda: f64,
) -> Vec<SearchResult> {
    if results.is_empty() || top_k == 0 {
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

            let max_sim = selected
                .iter()
                .map(|s| jaccard_similarity(results[rem_idx].doc_id, s.doc_id, index))
                .fold(0.0f64, f64::max);

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;

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

/// Jaccard similarity between two documents based on token sets.
fn jaccard_similarity(doc_a: usize, doc_b: usize, index: &InvertedIndex) -> f64 {
    index.jaccard_similarity(doc_a, doc_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::InvertedIndex;
    use crate::scorer::BM25Scorer;
    use crate::tokenizer::Tokenizer;
    use crate::vector::VectorIndex;

    fn make_hybrid() -> (HybridSearcher, InvertedIndex) {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer.clone(), scorer);
        let vector_index = VectorIndex::new(3);

        let hybrid = HybridSearcher::new(searcher, vector_index);

        let mut idx = InvertedIndex::new();
        idx.add_document(0, &tokenizer.tokenize("异步HTTP客户端连接池"));
        idx.add_document(1, &tokenizer.tokenize("Rust TUI应用状态栏实现"));
        idx.add_document(2, &tokenizer.tokenize("Python异步编程最佳实践"));

        (hybrid, idx)
    }

    #[test]
    fn test_pure_bm25_fallback() {
        let (hybrid, idx) = make_hybrid();
        // No query embedding → pure BM25
        let results = hybrid.search("异步HTTP客户端", None, &idx, 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, 0);
    }

    #[test]
    fn test_empty_vector_index_fallback() {
        let (hybrid, idx) = make_hybrid();
        // Vector index is empty → pure BM25 even with embedding
        let results = hybrid.search("异步HTTP客户端", Some(&[1.0, 0.0, 0.0]), &idx, 3);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_hybrid_fusion() {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer.clone(), scorer);

        let mut vector_index = VectorIndex::new(2);
        // doc 0 semantically near query, doc 1 orthogonal, doc 2 somewhat near
        vector_index.add(0, &[0.9, 0.1]);
        vector_index.add(1, &[-0.9, -0.1]);
        vector_index.add(2, &[0.5, 0.5]);

        let hybrid = HybridSearcher::new(searcher, vector_index);

        let mut idx = InvertedIndex::new();
        idx.add_document(0, &tokenizer.tokenize("async HTTP client pooling"));
        idx.add_document(1, &tokenizer.tokenize("Rust TUI status bar"));
        idx.add_document(2, &tokenizer.tokenize("Python async best practices"));

        // Query embedding close to doc 0
        let results = hybrid.search("async", Some(&[1.0, 0.0]), &idx, 3);
        assert!(!results.is_empty());

        // Doc 0 should rank high due to both BM25 ("async") + vector (close)
        let doc0_score = results.iter().find(|r| r.doc_id == 0).map(|r| r.score);
        assert!(doc0_score.is_some());
    }

    #[test]
    fn test_mmr_diversity() {
        let tokenizer = Tokenizer::new(2);
        let scorer = BM25Scorer::default();
        let searcher = Searcher::new(tokenizer.clone(), scorer);
        let vector_index = VectorIndex::new(2);

        let hybrid = HybridSearcher::with_weights(searcher, vector_index, 0.5, 0.5, 0.5);

        let mut idx = InvertedIndex::new();
        idx.add_document(0, &tokenizer.tokenize("Python async HTTP client"));
        idx.add_document(1, &tokenizer.tokenize("Python async HTTP server"));
        idx.add_document(2, &tokenizer.tokenize("Rust async runtime tokio"));

        let results = hybrid.search("async Python HTTP", None, &idx, 2);
        assert_eq!(results.len(), 2);
        // With MMR, results should be diverse (not both Python HTTP)
    }
}
