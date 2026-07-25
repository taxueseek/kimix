//! Inverted index with postings lists.
//!
//! Term → list of (doc_id, term_frequency) pairs, stored as sorted vectors
//! for efficient intersection during search.
use std::collections::HashMap;

/// A single posting: which document contains the term and how many times.
#[derive(Debug, Clone, Copy)]
pub struct Posting {
    pub doc_id: usize,
    pub term_freq: u32,
}

/// Postings list for a single term (sorted by doc_id).
#[derive(Debug, Clone, Default)]
pub struct PostingsList {
    pub postings: Vec<Posting>,
}

impl PostingsList {
    pub fn len(&self) -> usize {
        self.postings.len()
    }

    pub fn doc_freq(&self) -> usize {
        self.postings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }

    /// Add or update a posting. Keeps list sorted by doc_id.
    pub fn upsert(&mut self, doc_id: usize, tf: u32) {
        match self.postings.binary_search_by_key(&doc_id, |p| p.doc_id) {
            Ok(idx) => {
                self.postings[idx].term_freq = tf;
            }
            Err(idx) => {
                self.postings.insert(
                    idx,
                    Posting {
                        doc_id,
                        term_freq: tf,
                    },
                );
            }
        }
    }
}

/// Inverted index: term → PostingsList.
#[derive(Debug, Default)]
pub struct InvertedIndex {
    /// Term → postings list.
    pub terms: HashMap<String, PostingsList>,
    /// doc_id → document length (in tokens).
    pub doc_lengths: Vec<usize>,
    /// doc_id → set of terms in this document (for update support).
    doc_terms: Vec<Vec<String>>,
    /// Total number of documents.
    pub num_docs: usize,
    /// Sum of all document lengths.
    pub total_tokens: usize,
    /// Average document length.
    avgdl: f64,
}

impl InvertedIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a document's tokens to the index.
    /// doc_id must be the next sequential id (0, 1, 2, ...).
    /// Re-adding the same doc_id updates its entry (old terms are removed).
    pub fn add_document(&mut self, doc_id: usize, tokens: &[String]) {
        // Count term frequencies
        let mut tf_map: HashMap<&str, u32> = HashMap::new();
        for token in tokens {
            *tf_map.entry(token.as_str()).or_insert(0) += 1;
        }

        let doc_len = tokens.len();

        // Update or create doc_lengths and doc_terms entries
        if doc_id >= self.doc_lengths.len() {
            self.doc_lengths.resize(doc_id + 1, 0);
            self.doc_terms.resize(doc_id + 1, Vec::new());
            self.num_docs = doc_id + 1;
        }

        // Remove old postings for this document
        for old_term in &self.doc_terms[doc_id] {
            if let Some(pl) = self.terms.get_mut(old_term.as_str()) {
                if let Ok(idx) = pl.postings.binary_search_by_key(&doc_id, |p| p.doc_id) {
                    pl.postings.remove(idx);
                }
                // Remove empty postings lists
                if pl.postings.is_empty() {
                    // Will be cleaned below
                }
            }
        }
        // Clean up empty postings lists
        self.terms.retain(|_, pl| !pl.postings.is_empty());

        // Update doc metadata
        let old_len = self.doc_lengths[doc_id];
        self.total_tokens = self.total_tokens.saturating_sub(old_len);
        self.total_tokens += doc_len;
        self.doc_lengths[doc_id] = doc_len;
        self.doc_terms[doc_id] = tokens.to_vec();
        // dedup terms
        self.doc_terms[doc_id].sort();
        self.doc_terms[doc_id].dedup();

        self.avgdl = if self.num_docs > 0 {
            self.total_tokens as f64 / self.num_docs as f64
        } else {
            0.0
        };

        // Add new postings
        for (term, tf) in tf_map {
            self.terms
                .entry(term.to_string())
                .or_default()
                .upsert(doc_id, tf);
        }
    }

    /// Average document length.
    pub fn avgdl(&self) -> f64 {
        self.avgdl
    }

    /// Get postings for a term.
    pub fn get_postings(&self, term: &str) -> Option<&PostingsList> {
        self.terms.get(term)
    }

    /// Check if a term exists in the index.
    pub fn has_term(&self, term: &str) -> bool {
        self.terms.contains_key(term)
    }

    /// Number of documents containing a term.
    pub fn doc_freq(&self, term: &str) -> usize {
        self.terms.get(term).map(|p| p.doc_freq()).unwrap_or(0)
    }

    /// Number of unique terms.
    pub fn num_terms(&self) -> usize {
        self.terms.len()
    }

    /// Unique sorted terms for a document, if it exists.
    pub fn document_terms(&self, doc_id: usize) -> Option<&[String]> {
        self.doc_terms.get(doc_id).map(Vec::as_slice)
    }

    /// Jaccard similarity between two documents' unique token sets.
    pub fn jaccard_similarity(&self, doc_a: usize, doc_b: usize) -> f64 {
        if doc_a == doc_b {
            return 1.0;
        }
        let (Some(terms_a), Some(terms_b)) =
            (self.document_terms(doc_a), self.document_terms(doc_b))
        else {
            return 0.0;
        };
        let mut a = 0;
        let mut b = 0;
        let mut intersection = 0;
        while a < terms_a.len() && b < terms_b.len() {
            match terms_a[a].cmp(&terms_b[b]) {
                std::cmp::Ordering::Less => a += 1,
                std::cmp::Ordering::Greater => b += 1,
                std::cmp::Ordering::Equal => {
                    intersection += 1;
                    a += 1;
                    b += 1;
                }
            }
        }
        let union = terms_a.len() + terms_b.len() - intersection;
        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Clear all data.
    pub fn clear(&mut self) {
        self.terms.clear();
        self.doc_lengths.clear();
        self.doc_terms.clear();
        self.num_docs = 0;
        self.total_tokens = 0;
        self.avgdl = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_retrieve() {
        let mut idx = InvertedIndex::new();
        idx.add_document(0, &["hello".into(), "world".into(), "hello".into()]);
        idx.add_document(1, &["hello".into(), "rust".into()]);

        assert_eq!(idx.num_docs, 2);
        assert_eq!(idx.doc_freq("hello"), 2);
        assert_eq!(idx.doc_freq("world"), 1);
        assert_eq!(idx.doc_freq("rust"), 1);
        assert_eq!(idx.doc_freq("nonexistent"), 0);

        // hello appears twice in doc 0
        let postings = idx.get_postings("hello").unwrap();
        assert_eq!(postings.len(), 2);
        let p0 = postings.postings.iter().find(|p| p.doc_id == 0).unwrap();
        assert_eq!(p0.term_freq, 2);
    }

    #[test]
    fn test_update_existing_doc() {
        let mut idx = InvertedIndex::new();
        idx.add_document(0, &["a".into(), "b".into()]);
        idx.add_document(0, &["c".into(), "d".into(), "e".into()]);

        assert_eq!(idx.num_docs, 1);
        assert_eq!(idx.doc_lengths[0], 3);
        assert!(!idx.has_term("a")); // replaced
        assert!(idx.has_term("c"));
    }
}
