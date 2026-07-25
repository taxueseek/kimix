//! Lightweight vector index with cosine similarity search.
//!
//! Pure Rust implementation, no ONNX or external ML dependencies.
//! Uses brute-force search suitable for small to medium datasets;
//! can be upgraded to HNSW for larger scale.
use std::collections::HashMap;

/// A brute-force vector index with cosine similarity.
pub struct VectorIndex {
    /// doc_id → embedding vector
    vectors: HashMap<usize, Vec<f32>>,
    /// Expected embedding dimension (validated on add).
    dim: usize,
}

impl VectorIndex {
    /// Create a new vector index with the expected embedding dimension.
    pub fn new(dim: usize) -> Self {
        Self {
            vectors: HashMap::new(),
            dim,
        }
    }

    /// Number of vectors in the index.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Add or update a vector for a document.
    ///
    /// Panics if the embedding dimension doesn't match the index dimension.
    pub fn add(&mut self, doc_id: usize, embedding: &[f32]) {
        assert_eq!(
            embedding.len(),
            self.dim,
            "Embedding dimension {} != index dimension {}",
            embedding.len(),
            self.dim
        );
        self.vectors.insert(doc_id, embedding.to_vec());
    }

    /// Remove a document from the index.
    pub fn remove(&mut self, doc_id: usize) {
        self.vectors.remove(&doc_id);
    }

    /// Search for the top-k most similar documents by cosine similarity.
    ///
    /// Returns `Vec<(doc_id, similarity)>` sorted by similarity descending.
    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<(usize, f64)> {
        assert_eq!(
            query_embedding.len(),
            self.dim,
            "Query dimension {} != index dimension {}",
            query_embedding.len(),
            self.dim
        );

        if self.vectors.is_empty() || top_k == 0 {
            return vec![];
        }

        // Compute cosine similarity for all vectors
        let mut scored: Vec<(usize, f64)> = self
            .vectors
            .iter()
            .map(|(&doc_id, vec)| {
                let sim = cosine_similarity(query_embedding, vec);
                (doc_id, sim)
            })
            .collect();

        // Sort by similarity descending
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scored.truncate(top_k);
        scored
    }
}

/// Compute cosine similarity between two unit-normalized vectors.
///
/// Returns a value in [0.0, 1.0] for non-negative vectors,
/// or [-1.0, 1.0] for general vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| f64::from(*x) * f64::from(*y)).sum();
    let norm_a: f64 = a.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_search() {
        let mut idx = VectorIndex::new(3);
        idx.add(0, &[1.0, 0.0, 0.0]);
        idx.add(1, &[0.0, 1.0, 0.0]);
        idx.add(2, &[0.5, 0.5, 0.0]);

        let results = idx.search(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0); // doc 0 most similar
    }

    #[test]
    fn test_cosine_orthogonal() {
        let mut idx = VectorIndex::new(2);
        idx.add(0, &[1.0, 0.0]);
        idx.add(1, &[0.0, 1.0]);

        let results = idx.search(&[1.0, 0.0], 2);
        assert!((results[0].1 - 1.0).abs() < 1e-6);
        assert!(results[1].1.abs() < 1e-6); // orthogonal → 0
    }

    #[test]
    fn test_empty_index() {
        let idx = VectorIndex::new(3);
        let results = idx.search(&[1.0, 0.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    #[should_panic(expected = "dimension")]
    fn test_dimension_mismatch_add() {
        let mut idx = VectorIndex::new(3);
        idx.add(0, &[1.0, 0.0]); // 2 ≠ 3
    }

    #[test]
    #[should_panic(expected = "dimension")]
    fn test_dimension_mismatch_search() {
        let idx = VectorIndex::new(3);
        idx.search(&[1.0, 0.0], 5); // 2 ≠ 3
    }
}
