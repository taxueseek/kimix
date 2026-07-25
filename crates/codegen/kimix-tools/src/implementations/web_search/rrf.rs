//! Reciprocal Rank Fusion for multi-query search on the same API.
//!
//! Does not introduce new engines — only fuses ranked lists from multiple
//! queries against one hosted/subscription search endpoint.

use super::client::SearchResult;
use std::collections::HashMap;

/// Classic RRF constant (Cormack et al.).
const RRF_K: f32 = 60.0;

/// Fuse multiple ranked result lists via RRF, returning unique hits ordered
/// by descending fusion score. When URLs collide, the hit with the richer
/// snippet/content is kept.
pub fn rrf_fuse(lists: &[Vec<SearchResult>], limit: usize) -> Vec<SearchResult> {
    if lists.is_empty() {
        return Vec::new();
    }
    if lists.len() == 1 {
        let mut only = lists[0].clone();
        only.truncate(limit);
        return only;
    }

    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut best: HashMap<String, SearchResult> = HashMap::new();

    for list in lists {
        for (rank, hit) in list.iter().enumerate() {
            let url = hit.url.clone();
            let contrib = 1.0 / (RRF_K + rank as f32 + 1.0);
            *scores.entry(url.clone()).or_insert(0.0) += contrib;
            best.entry(url)
                .and_modify(|existing| {
                    if hit_richness(hit) > hit_richness(existing) {
                        *existing = hit.clone();
                    }
                })
                .or_insert_with(|| hit.clone());
        }
    }

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    ranked
        .into_iter()
        .take(limit)
        .filter_map(|(url, _)| best.remove(&url))
        .collect()
}

fn hit_richness(h: &SearchResult) -> usize {
    h.snippet.len() + h.content.len() + h.date.len()
}

/// Expand a user query into 1–3 sub-queries for the same API.
///
/// Conservative: always includes the original; adds a source/data oriented
/// variant for fact-heavy questions; adds an English/Chinese counterpart
/// when mixed-script cues appear.
pub fn expand_queries(query: &str) -> Vec<String> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let mut out = vec![q.to_string()];

    if is_fact_heavy(q) {
        if contains_cjk(q) {
            push_unique(&mut out, format!("{q} 来源 数据"));
        } else {
            push_unique(&mut out, format!("{q} official source data"));
        }
    }

    if contains_cjk(q) && contains_latin_word(q) {
        // Mixed: keep original; add a simplified CJK-only slice if possible.
        let cjk_only: String = q
            .chars()
            .filter(|c| !c.is_ascii_alphabetic())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let cjk_only = cjk_only.trim().to_string();
        if cjk_only.chars().count() >= 2 {
            push_unique(&mut out, cjk_only);
        }
    }

    // Cap at 3 to bound latency/cost on the same API.
    out.truncate(3);
    out
}

fn push_unique(out: &mut Vec<String>, q: String) {
    if !out.iter().any(|e| e == &q) {
        out.push(q);
    }
}

fn is_fact_heavy(q: &str) -> bool {
    const MARKERS: &[&str] = &[
        "是否",
        "多少",
        "几成",
        "占比",
        "股价",
        "市值",
        "财报",
        "营收",
        "利润",
        "持仓",
        "安全吗",
        "真的吗",
        "属实",
        "how many",
        "how much",
        "is it true",
        "market cap",
        "revenue",
        "earnings",
        "price of",
        "when did",
        "who is",
        "%",
    ];
    let lower = q.to_ascii_lowercase();
    MARKERS.iter().any(|m| lower.contains(&m.to_ascii_lowercase()))
        || q.chars().any(|c| c.is_ascii_digit())
}

fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        ('\u{4e00}'..='\u{9fff}').contains(&c)
            || ('\u{3400}'..='\u{4dbf}').contains(&c)
            || ('\u{f900}'..='\u{faff}').contains(&c)
    })
}

fn contains_latin_word(s: &str) -> bool {
    let mut run = 0;
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(url: &str, title: &str) -> SearchResult {
        SearchResult {
            site_name: String::new(),
            title: title.into(),
            url: url.into(),
            snippet: title.into(),
            content: String::new(),
            date: String::new(),
        }
    }

    #[test]
    fn rrf_promotes_consensus() {
        let a = vec![hit("https://a.example/1", "A1"), hit("https://b.example/2", "B2")];
        let b = vec![hit("https://b.example/2", "B2"), hit("https://c.example/3", "C3")];
        let fused = rrf_fuse(&[a, b], 5);
        assert_eq!(fused[0].url, "https://b.example/2");
    }

    #[test]
    fn expand_fact_query_adds_source_variant() {
        let qs = expand_queries("英伟达市值多少");
        assert!(qs.len() >= 2);
        assert_eq!(qs[0], "英伟达市值多少");
        assert!(qs.iter().any(|q| q.contains("来源")));
    }

    #[test]
    fn expand_simple_query_stays_single() {
        let qs = expand_queries("rust ownership");
        assert_eq!(qs, vec!["rust ownership".to_string()]);
    }
}
