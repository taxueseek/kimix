//! BM25 relevance scorer.
//!
//! Standard BM25 with configurable k1 and b parameters.
//! IDF uses the Lucene variant: ln(1 + (N - df + 0.5) / (df + 0.5)).
use crate::index::InvertedIndex;

/// BM25 scorer over an InvertedIndex.
pub struct BM25Scorer {
    k1: f64,
    b: f64,
}

impl BM25Scorer {
    pub fn new(k1: f64, b: f64) -> Self {
        Self { k1, b }
    }
}

impl Default for BM25Scorer {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

impl BM25Scorer {
    /// Compute the maximum possible score a term can contribute (WAND upper bound).
    ///
    /// Uses the theoretical upper bound of the TF component (k1 + 1) multiplied by
    /// the term's IDF. This is a conservative estimate suitable for Block-Max WAND pruning.
    pub fn max_term_score(&self, term: &str, index: &InvertedIndex) -> f64 {
        let df = index.doc_freq(term);
        if df == 0 {
            return 0.0;
        }
        let n = index.num_docs;
        let idf = Self::idf(df, n);
        idf * (self.k1 + 1.0)
    }

    /// Score a single term for a single document.
    ///
    /// Returns the partial BM25 contribution of `term` with frequency `query_tf` in the query
    /// against `doc_id`. Used by WAND for incremental term-by-term evaluation with pruning.
    pub fn score_term(
        &self,
        term: &str,
        doc_id: usize,
        query_tf: u32,
        index: &InvertedIndex,
    ) -> f64 {
        let df = index.doc_freq(term);
        if df == 0 || doc_id >= index.doc_lengths.len() {
            return 0.0;
        }
        let idf = Self::idf(df, index.num_docs);

        let doc_len = index.doc_lengths[doc_id] as f64;
        let avgdl = index.avgdl();
        if avgdl == 0.0 {
            return 0.0;
        }

        let tf = index
            .get_postings(term)
            .and_then(|pl| {
                pl.postings
                    .binary_search_by_key(&doc_id, |p| p.doc_id)
                    .ok()
                    .map(|pos| pl.postings[pos].term_freq)
            })
            .unwrap_or(0) as f64;

        if tf == 0.0 {
            return 0.0;
        }

        let tf_component =
            (tf * (self.k1 + 1.0)) / (tf + self.k1 * (1.0 - self.b + self.b * doc_len / avgdl));
        idf * tf_component * (query_tf as f64)
    }

    /// Compute IDF for a term.
    /// Lucene BM25 variant: ln(1 + (N - df + 0.5) / (df + 0.5)).
    pub fn idf(df: usize, n: usize) -> f64 {
        if df == 0 || n == 0 {
            return 0.0;
        }
        let df = df as f64;
        let n = n as f64;
        ((1.0 + (n - df + 0.5) / (df + 0.5)).ln()).max(0.0)
    }

    /// Score a single document against a query.
    ///
    /// `query_tfs`: term → frequency in query.
    /// `doc_id`: the document to score.
    /// `index`: the inverted index.
    pub fn score(&self, query_terms: &[String], doc_id: usize, index: &InvertedIndex) -> f64 {
        if index.num_docs == 0 || doc_id >= index.doc_lengths.len() {
            return 0.0;
        }

        let doc_len = index.doc_lengths[doc_id] as f64;
        let avgdl = index.avgdl();
        if avgdl == 0.0 {
            return 0.0;
        }

        let n = index.num_docs;
        let mut score = 0.0;

        // Count query term frequencies
        use std::collections::HashMap;
        let mut qf_map: HashMap<&str, u32> = HashMap::new();
        for t in query_terms {
            *qf_map.entry(t.as_str()).or_insert(0) += 1;
        }

        for (term, qf) in qf_map {
            let df = index.doc_freq(term);
            if df == 0 {
                continue;
            }

            let idf = Self::idf(df, n);

            // Get tf for this doc
            let tf = index
                .get_postings(term)
                .and_then(|pl| {
                    pl.postings
                        .binary_search_by_key(&doc_id, |p| p.doc_id)
                        .ok()
                        .map(|idx| pl.postings[idx].term_freq)
                })
                .unwrap_or(0) as f64;

            if tf == 0.0 {
                continue;
            }

            let tf_component =
                (tf * (self.k1 + 1.0)) / (tf + self.k1 * (1.0 - self.b + self.b * doc_len / avgdl));

            score += idf * tf_component * (qf as f64);
        }

        // Length normalization bonus for longer queries matching well
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::InvertedIndex;

    #[test]
    fn test_idf() {
        let idf = BM25Scorer::idf(1, 10);
        assert!(idf > 0.0);

        let idf_all = BM25Scorer::idf(10, 10);
        // Term in all docs should have lower IDF
        assert!(idf_all < idf);
    }

    #[test]
    fn test_basic_scoring() {
        let mut idx = InvertedIndex::new();
        idx.add_document(0, &["hello".into(), "world".into()]);
        idx.add_document(1, &["hello".into(), "rust".into(), "hello".into()]);
        idx.add_document(2, &["goodbye".into(), "world".into()]);

        let scorer = BM25Scorer::default();
        let query = &["hello".to_string()];

        let s0 = scorer.score(query, 0, &idx);
        let s1 = scorer.score(query, 1, &idx);
        let s2 = scorer.score(query, 2, &idx);

        // doc 1 has tf=2 for "hello", should score highest
        assert!(s1 > s0);
        // doc 2 has no "hello"
        assert_eq!(s2, 0.0);
    }
}
