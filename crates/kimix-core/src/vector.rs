//! Lightweight vector index with cosine similarity search.
//!
//! Pure Rust implementation, no ONNX or external ML dependencies.
//! Uses brute-force search suitable for small to medium datasets;
//! can be upgraded to HNSW for larger scale.
use std::collections::HashMap;

/// Default dimension for [`local_embedding`] output.
pub const LOCAL_EMBED_DIM: usize = 256;

/// Local, deterministic, dependency-free embedding for a text span.
///
/// Uses feature hashing (hashing trick) over CJK bigrams + ASCII words,
/// projected into a fixed-dimension bag-of-features vector. Not a learned
/// embedding — it captures token overlap only, but that is exactly the
/// signal BM25 already sees, and it lets the hybrid searcher run with zero
/// network/API dependency. Deterministic across calls (blake3, fixed keys),
/// so turn embeddings and query embeddings are comparable.
pub fn local_embedding(text: &str, dim: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dim];

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    while i < n {
        let c = chars[i];
        if is_cjk(c) {
            if i + 1 < n && is_cjk(chars[i + 1]) {
                // 重叠 bigram：本字符 + 下一个 CJK
                let mut s = String::with_capacity(6);
                s.push(c);
                s.push(chars[i + 1]);
                add_feature(&mut vec, dim, &s);
                i += 1;
            } else {
                // 孤立 CJK（后邻非 CJK，如「端」+ASCII 词）
                let mut s = String::with_capacity(4);
                s.push(c);
                add_feature(&mut vec, dim, &s);
                i += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            // 连续 ASCII 词（字母/数字/下划线），在 CJK 边界处自然切分
            let start = i;
            while i < n {
                let ch = chars[i];
                if ch.is_alphanumeric() || ch == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word: String = chars[start..i].iter().collect();
            if !word.is_empty() {
                add_feature(&mut vec, dim, &word);
            }
        } else {
            // 空白/标点：跳过
            i += 1;
        }
    }

    // L2 normalize for stable cosine similarity
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}

/// Hash one feature into the vector (count-based, deterministic).
fn add_feature(vec: &mut [f32], dim: usize, feature: &str) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(feature.as_bytes());
    let out = hasher.finalize();
    let h = u64::from_le_bytes(out.as_bytes()[..8].try_into().expect("8 bytes")) as usize;
    vec[h % dim] += 1.0;
}

/// CJK 统一表意文字、假名、谚文、全角字符范围检测（与 cache_engine 一致）。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF  // CJK Extension A
        | 0x4E00..=0x9FFF  // CJK Unified Ideographs
        | 0xF900..=0xFAFF  // CJK Compatibility Ideographs
        | 0x3040..=0x30FF  // Hiragana + Katakana
        | 0xAC00..=0xD7AF  // Hangul Syllables
        | 0xFF00..=0xFFEF  // Fullwidth Forms
        | 0x20000..=0x2A6DF // CJK Extension B
    )
}

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
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored.truncate(top_k);
        scored
    }
}

/// Compute cosine similarity between two unit-normalized vectors.
///
/// Returns a value in [0.0, 1.0] for non-negative vectors,
/// or [-1.0, 1.0] for general vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let norm_a: f64 = a
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt();
    let norm_b: f64 = b
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt();

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

    #[test]
    fn test_local_embedding_deterministic() {
        let a = local_embedding("异步HTTP客户端连接池", 64);
        let b = local_embedding("异步HTTP客户端连接池", 64);
        assert_eq!(a, b, "same text must produce identical embeddings");
    }

    #[test]
    fn test_local_embedding_dimension() {
        let e = local_embedding("Rust TUI 状态栏", LOCAL_EMBED_DIM);
        assert_eq!(e.len(), LOCAL_EMBED_DIM);
    }

    #[test]
    fn test_local_embedding_similar_texts_closer() {
        // 共享 CJK bigram 的两段文本应比完全不相关的文本更接近
        let dim = 256usize;
        let a = local_embedding("异步HTTP客户端连接池", dim);
        let b = local_embedding("Python 的异步 HTTP 客户端", dim);
        let c = local_embedding("花园里开满了鲜花", dim);
        let sim_ab = cosine_similarity(&a, &b);
        let sim_ac = cosine_similarity(&a, &c);
        assert!(
            sim_ab > sim_ac,
            "similar texts should be closer (ab={sim_ab}, ac={sim_ac})"
        );
    }

    #[test]
    fn test_local_embedding_nonzero() {
        let e = local_embedding("你好世界", 32);
        assert!(e.iter().any(|v| *v != 0.0), "embedding must be non-zero");
    }

    #[test]
    fn test_local_embedding_keeps_ascii_in_mixed_text() {
        // 审计回归：CJK+ASCII 混合无空白时，ASCII 词必须进入特征（原实现会丢失）
        // 对比：仅含 CJK bigram 的 embedding vs 含 HTTP 的 embedding，特征必须不同
        let dim = 256usize;
        let no_ascii = local_embedding("异步客户端", dim);
        let with_http = local_embedding("异步HTTP客户端", dim);
        assert_ne!(
            no_ascii, with_http,
            "ASCII 词 'HTTP' 必须被捕获为独立特征"
        );
        // HTTP 出现在两端文本共享的 CJK 之外，cosine 相似度应 < 1（有差异特征）
        assert!(
            cosine_similarity(&no_ascii, &with_http) < 1.0,
            "混合 ASCII 词后向量必须与纯 CJK 版本不同"
        );
    }
}
