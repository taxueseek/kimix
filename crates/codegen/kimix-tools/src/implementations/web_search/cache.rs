//! Process-local search cache: L1 LRU + negative cache for empty/error.
//!
//! No disk dependency — safe for multi-session processes without shared
//! state requirements. Positive hits use a short TTL; negative hits use a
//! shorter TTL to avoid hammering a failing or empty query.

use super::client::SearchResult;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const POSITIVE_TTL: Duration = Duration::from_secs(300); // 5 min
const NEGATIVE_TTL: Duration = Duration::from_secs(45);
const MAX_ENTRIES: usize = 256;

#[derive(Clone)]
enum Entry {
    Hits {
        results: Vec<SearchResult>,
        stored_at: Instant,
    },
    Empty {
        stored_at: Instant,
    },
}

impl Entry {
    fn is_fresh(&self) -> bool {
        match self {
            Self::Hits { stored_at, .. } => stored_at.elapsed() < POSITIVE_TTL,
            Self::Empty { stored_at } => stored_at.elapsed() < NEGATIVE_TTL,
        }
    }
}

/// Shared in-process cache for web search results.
#[derive(Clone, Default)]
pub struct SearchCache {
    inner: Arc<parking_lot::Mutex<HashMap<String, Entry>>>,
}

impl SearchCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(query: &str, limit: u8, include_content: bool) -> String {
        format!("{limit}|{include_content}|{}", query.trim().to_lowercase())
    }

    /// Returns `Some(Ok(hits))` on positive hit, `Some(Err(()))` on negative
    /// (empty) hit, `None` on miss/stale.
    pub fn get(
        &self,
        query: &str,
        limit: u8,
        include_content: bool,
    ) -> Option<Result<Vec<SearchResult>, ()>> {
        let key = Self::key(query, limit, include_content);
        let mut guard = self.inner.lock();
        match guard.get(&key) {
            Some(entry) if entry.is_fresh() => match entry {
                Entry::Hits { results, .. } => Some(Ok(results.clone())),
                Entry::Empty { .. } => Some(Err(())),
            },
            Some(_) => {
                guard.remove(&key);
                None
            }
            None => None,
        }
    }

    pub fn put_hits(&self, query: &str, limit: u8, include_content: bool, results: Vec<SearchResult>) {
        let key = Self::key(query, limit, include_content);
        let mut guard = self.inner.lock();
        if guard.len() >= MAX_ENTRIES {
            // Drop roughly oldest half by clearing — simple bound, not strict LRU.
            guard.clear();
        }
        if results.is_empty() {
            guard.insert(key, Entry::Empty { stored_at: Instant::now() });
        } else {
            guard.insert(
                key,
                Entry::Hits {
                    results,
                    stored_at: Instant::now(),
                },
            );
        }
    }

    pub fn put_empty(&self, query: &str, limit: u8, include_content: bool) {
        self.put_hits(query, limit, include_content, Vec::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_and_negative_roundtrip() {
        let cache = SearchCache::new();
        let hits = vec![SearchResult {
            site_name: String::new(),
            title: "T".into(),
            url: "https://example.com".into(),
            snippet: "S".into(),
            content: String::new(),
            date: String::new(),
        }];
        cache.put_hits("rust", 5, false, hits.clone());
        let got = cache.get("rust", 5, false).unwrap().unwrap();
        assert_eq!(got[0].url, "https://example.com");

        cache.put_empty("noresults", 5, false);
        assert!(cache.get("noresults", 5, false).unwrap().is_err());
        assert!(cache.get("other", 5, false).is_none());
    }
}
