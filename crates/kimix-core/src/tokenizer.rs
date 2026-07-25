//! CJK-aware n-gram tokenizer.
//!
//! Strategy:
//! - CJK characters (Unicode ranges): overlapping bigrams
//! - Latin/ASCII words: whitespace-split, lowercased
//! - Mixed sequences: bigram across script boundaries to capture cross-script context
use lru::LruCache;
use unicode_normalization::UnicodeNormalization;

/// Returns true if the character is a CJK unified ideograph, hiragana, katakana, or hangul.
#[inline]
fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // Extension A
        | '\u{20000}'..='\u{2EBEF}' // Extensions B-F
        | '\u{F900}'..='\u{FAFF}' // Compatibility Ideographs
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
    )
}

/// A single token produced by the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    /// The token text (lowercased for ASCII).
    pub text: String,
    /// Byte offset in the original text.
    pub offset: usize,
}

/// CJK bigram + whitespace tokenizer.
#[derive(Debug)]
pub struct Tokenizer {
    /// N-gram size for CJK characters.
    n: usize,
    /// LRU cache for tokenization results (interior mutability for &self access).
    cache: std::cell::RefCell<LruCache<String, Vec<String>>>,
}

impl Clone for Tokenizer {
    fn clone(&self) -> Self {
        Self {
            n: self.n,
            cache: std::cell::RefCell::new(LruCache::new(Self::CACHE_CAP)),
        }
    }
}

impl Tokenizer {
    /// Tokenization cache capacity.
    const CACHE_CAP: std::num::NonZeroUsize = std::num::NonZeroUsize::new(512).unwrap();

    pub fn new(n: usize) -> Self {
        Self { n, cache: std::cell::RefCell::new(LruCache::new(Self::CACHE_CAP)) }
    }

    /// Normalize text: NFKC + lowercase.
    pub fn normalize(text: &str) -> String {
        text.nfkc().collect::<String>().to_lowercase()
    }

    /// Tokenize text into a list of tokens, with caching (interior mutability).
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        {
            let mut cache = self.cache.borrow_mut();
            if let Some(cached) = cache.get(text) {
                return cached.clone();
            }
        }
        let result = self.tokenize_impl(text);
        self.cache.borrow_mut().put(text.to_string(), result.clone());
        result
    }

    /// Tokenize without caching (used internally).
    fn tokenize_impl(&self, text: &str) -> Vec<String> {
        let normalized = Self::normalize(text);
        let chars: Vec<char> = normalized.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            // Skip whitespace
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }

            if is_cjk(chars[i]) {
                // CJK: collect consecutive CJK chars + following ASCII word for cross-boundary bigrams
                let start = i;
                while i < chars.len() && (is_cjk(chars[i]) || !chars[i].is_whitespace()) {
                    i += 1;
                }
                let segment: String = chars[start..i].iter().collect();

                // Generate overlapping n-grams from this segment
                let seg_chars: Vec<char> = segment.chars().collect();
                if seg_chars.len() < self.n {
                    tokens.push(segment);
                } else {
                    for j in 0..=seg_chars.len() - self.n {
                        let gram: String = seg_chars[j..j + self.n].iter().collect();
                        tokens.push(gram);
                    }
                }
            } else {
                // ASCII word: collect until whitespace or CJK
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() && !is_cjk(chars[i]) {
                    i += 1;
                }
                // Also include immediately following CJK for cross-boundary bigrams
                let mut end = i;
                while end < chars.len() && (is_cjk(chars[end]) || !chars[end].is_whitespace()) {
                    end += 1;
                }
                let segment: String = chars[start..end].iter().collect();
                let seg_chars: Vec<char> = segment.chars().collect();

                if seg_chars.len() < self.n {
                    tokens.push(segment);
                } else {
                    for j in 0..=seg_chars.len() - self.n {
                        let gram: String = seg_chars[j..j + self.n].iter().collect();
                        tokens.push(gram);
                    }
                }
            }
        }

        // Deduplicate adjacent identical tokens (common in bigram)
        tokens.dedup();
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cjk() {
        assert!(is_cjk('中'));
        assert!(is_cjk('本'));
        assert!(is_cjk('あ')); // hiragana
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
    }

    #[test]
    fn test_pure_chinese_bigram() {
        let t = Tokenizer::new(2);
        let tokens = t.tokenize("异步HTTP客户端");
        println!("tokens: {:?}", tokens);
        // Should contain bigrams like "异步", "步h", "ht", "tt", "tp", etc.
        assert!(tokens.contains(&"异步".to_string()));
    }

    #[test]
    fn test_pure_english() {
        let t = Tokenizer::new(2);
        let tokens = t.tokenize("hello world");
        println!("tokens: {:?}", tokens);
        assert!(tokens.contains(&"he".to_string()));
    }

    #[test]
    fn test_mixed_chinese_english() {
        let t = Tokenizer::new(2);
        let tokens = t.tokenize("Python的异步HTTP客户端");
        println!("tokens: {:?}", tokens);
        // Should handle cross-boundary tokens
        assert!(!tokens.is_empty());
    }
}
