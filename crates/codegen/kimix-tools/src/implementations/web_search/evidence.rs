//! Snippet-level evidence scoring for web search hits.
//!
//! Ported essence from Argo evidence (Selection × Absorption), without any
//! HTML SERP scraping. Operates only on `{title, url, snippet, date}` fields
//! returned by an existing search API.

use super::client::SearchResult;

/// Scored hit ready for agent consumption.
#[derive(Debug, Clone)]
pub struct ScoredHit {
    pub result: SearchResult,
    pub selection: f32,
    pub absorption: f32,
    pub freshness: f32,
    pub credibility_fast: f32,
    pub is_serp_or_jump: bool,
    pub evidence_flags: Vec<&'static str>,
}

/// Score and re-rank a list of raw search results.
pub fn score_and_rank(results: Vec<SearchResult>) -> Vec<ScoredHit> {
    let mut scored: Vec<ScoredHit> = results.into_iter().map(score_one).collect();
    scored.sort_by(|a, b| {
        b.credibility_fast
            .partial_cmp(&a.credibility_fast)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored
}

fn score_one(result: SearchResult) -> ScoredHit {
    let is_serp = is_serp_or_jump_url(&result.url);
    let mut flags: Vec<&'static str> = Vec::new();

    let mut selection = authority_score(&result.url);
    if is_serp {
        selection = selection.min(0.15);
        flags.push("serp_or_jump");
    }

    let (absorption, abs_flags) = absorption_score(&result.title, &result.snippet, &result.content);
    flags.extend(abs_flags);

    let freshness = freshness_score(&result.date, &result.url);
    if freshness >= 0.7 {
        flags.push("fresh");
    }

    // final ≈ 0.40·selection + 0.35·absorption + 0.15·freshness + 0.10·engine
    // engine term omitted (single-API path) → renormalize weights slightly.
    let credibility_fast = 0.45 * selection + 0.40 * absorption + 0.15 * freshness;

    ScoredHit {
        result,
        selection,
        absorption,
        freshness,
        credibility_fast,
        is_serp_or_jump: is_serp,
        evidence_flags: flags,
    }
}

/// Detect search-engine result pages and common jump/redirect shells.
pub fn is_serp_or_jump_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "google.com/search",
        "google.com/url?",
        "bing.com/search",
        "bing.com/ck/a",
        "baidu.com/s?",
        "baidu.com/link?",
        "sogou.com/web",
        "sogou.com/link",
        "duckduckgo.com/?",
        "duckduckgo.com/l/?",
        "search.yahoo.com",
        "so.com/s",
        "yandex.com/search",
        "yandex.ru/search",
        "search.brave.com",
        "startpage.com/sp/search",
    ];
    PATTERNS.iter().any(|p| lower.contains(p))
}

fn authority_score(url: &str) -> f32 {
    let host = host_of(url);
    if host.is_empty() {
        return 0.35;
    }
    // High-trust reference / official docs
    const HIGH: &[&str] = &[
        "wikipedia.org",
        "github.com",
        "arxiv.org",
        "doi.org",
        "nature.com",
        "science.org",
        "nih.gov",
        "gov.cn",
        "edu.cn",
        "ac.cn",
        "rust-lang.org",
        "python.org",
        "docs.rs",
        "developer.mozilla.org",
        "ietf.org",
        "w3.org",
        "openai.com",
        "anthropic.com",
        "x.ai",
        "deepseek.com",
        "moonshot.cn",
        "reuters.com",
        "bloomberg.com",
        "ft.com",
        "wsj.com",
        "nytimes.com",
        "bbc.com",
        "bbc.co.uk",
        "theguardian.com",
        "apnews.com",
        "sec.gov",
        "who.int",
        "un.org",
    ];
    // Chinese quality sources
    const HIGH_CN: &[&str] = &[
        "zhihu.com",
        "sspai.com",
        "36kr.com",
        "caixin.com",
        "cls.cn",
        "eastmoney.com",
        "xueqiu.com",
        "people.com.cn",
        "xinhuanet.com",
        "cctv.com",
        "gov.cn",
    ];
    // Narrative / social — lower selection, still usable as narrative
    const NARRATIVE: &[&str] = &[
        "twitter.com",
        "x.com",
        "reddit.com",
        "weibo.com",
        "xiaohongshu.com",
        "bilibili.com",
        "tiktok.com",
        "youtube.com",
        "medium.com",
        "substack.com",
        "zhihu.com/question", // Q&A threads often low absorption
    ];
    const LOW: &[&str] = &[
        "pinterest.com",
        "quora.com",
        "answers.com",
        "baike.baidu.com", // often thin / SEO
    ];

    if HIGH.iter().any(|d| host.ends_with(d)) {
        return 0.90;
    }
    if HIGH_CN.iter().any(|d| host.ends_with(d)) {
        return 0.82;
    }
    if NARRATIVE
        .iter()
        .any(|d| host.contains(d) || url.contains(d))
    {
        return 0.40;
    }
    if LOW.iter().any(|d| host.ends_with(d)) {
        return 0.30;
    }
    // Default mid authority
    0.55
}

fn absorption_score(title: &str, snippet: &str, content: &str) -> (f32, Vec<&'static str>) {
    let text = format!("{title} {snippet} {content}");
    let mut score: f32 = 0.25;
    let mut flags = Vec::new();

    if has_numbers(&text) {
        score += 0.18;
        flags.push("has_numbers");
    }
    if looks_definition(&text) {
        score += 0.15;
        flags.push("definition");
    }
    if looks_comparison(&text) {
        score += 0.12;
        flags.push("comparison");
    }
    if looks_howto(&text) {
        score += 0.10;
        flags.push("howto");
    }
    if looks_disclosure(&text) {
        score += 0.08;
        flags.push("disclose");
    }
    if looks_qa_thin(&text) {
        score -= 0.12;
        flags.push("is_qa");
    }
    // Length proxy for density
    let len = snippet.chars().count() + content.chars().count().min(2000);
    if len > 200 {
        score += 0.08;
    }
    if len > 800 {
        score += 0.06;
    }

    (score.clamp(0.0, 1.0), flags)
}

fn freshness_score(date: &str, url: &str) -> f32 {
    if let Some(year) = extract_year(date).or_else(|| extract_year(url)) {
        let current = 2026i32; // keep in sync with product year; tests pin this
        let age = (current - year).max(0);
        return match age {
            0 => 1.0,
            1 => 0.85,
            2 => 0.70,
            3 => 0.55,
            4 => 0.40,
            _ => 0.25,
        };
    }
    0.50 // unknown
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn has_numbers(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
        || text.contains('%')
        || text.contains("亿")
        || text.contains("万")
}

fn looks_definition(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        " is ",
        " are ",
        "指的是",
        "是指",
        "定义为",
        " definition",
        " means ",
        " refers to ",
    ];
    let lower = text.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|m| lower.contains(&m.to_ascii_lowercase()))
}

fn looks_comparison(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        " vs ",
        " versus ",
        " compared to ",
        "对比",
        "比较",
        "差异",
        "区别",
        "高于",
        "低于",
        "同比",
        "环比",
    ];
    let lower = text.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|m| lower.contains(&m.to_ascii_lowercase()))
}

fn looks_howto(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        "how to", "step ", "tutorial", "如何", "怎么", "步骤", "教程", "guide",
    ];
    let lower = text.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|m| lower.contains(&m.to_ascii_lowercase()))
}

fn looks_disclosure(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        "according to",
        "source:",
        "披露",
        "公告",
        "filing",
        "sec ",
        "10-k",
        "10-q",
        "财报",
        "年报",
    ];
    let lower = text.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|m| lower.contains(&m.to_ascii_lowercase()))
}

fn looks_qa_thin(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("someone asked") || lower.contains("网友问") || lower.contains("best answer"))
        && text.chars().count() < 180
}

fn extract_year(s: &str) -> Option<i32> {
    // Prefer full ISO-like dates first
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let y: i32 = std::str::from_utf8(&bytes[i..i + 4]).ok()?.parse().ok()?;
            if (1990..=2035).contains(&y) {
                return Some(y);
            }
        }
        i += 1;
    }
    None
}

/// Render scored hits into agent-facing text (Title/Date/URL/Summary + evidence).
pub fn render_scored(hits: &[ScoredHit]) -> (String, Vec<String>) {
    let mut content = String::new();
    let mut citations: Vec<String> = Vec::new();
    for (i, hit) in hits.iter().enumerate() {
        if i > 0 {
            content.push_str("---\n\n");
        }
        let r = &hit.result;
        content.push_str(&format!(
            "Title: {}\nDate: {}\nURL: {}\nSummary: {}\n",
            r.title, r.date, r.url, r.snippet
        ));
        content.push_str(&format!(
            "Evidence: selection={:.2} absorption={:.2} freshness={:.2} credibility={:.2}",
            hit.selection, hit.absorption, hit.freshness, hit.credibility_fast
        ));
        if !hit.evidence_flags.is_empty() {
            content.push_str(&format!(" flags=[{}]", hit.evidence_flags.join(",")));
        }
        content.push_str("\n\n");
        if !r.content.is_empty() {
            content.push_str(&format!("{}\n\n", r.content));
        }
        if !citations.contains(&r.url) {
            citations.push(r.url.clone());
        }
    }
    (content, citations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serp_urls_detected() {
        assert!(is_serp_or_jump_url("https://www.google.com/search?q=rust"));
        assert!(is_serp_or_jump_url("https://www.baidu.com/s?wd=test"));
        assert!(!is_serp_or_jump_url("https://doc.rust-lang.org/book/"));
    }

    #[test]
    fn serp_demoted_below_content() {
        let results = vec![
            SearchResult {
                site_name: "Google".into(),
                title: "Search".into(),
                url: "https://www.google.com/search?q=x".into(),
                snippet: "results".into(),
                content: String::new(),
                date: "2026-01-01".into(),
            },
            SearchResult {
                site_name: "Docs".into(),
                title: "Rust is a language".into(),
                url: "https://doc.rust-lang.org/book/".into(),
                snippet:
                    "Rust is a systems programming language. Step by step guide with 10 chapters."
                        .into(),
                content: String::new(),
                date: "2026-06-01".into(),
            },
        ];
        let ranked = score_and_rank(results);
        assert_eq!(ranked[0].result.url, "https://doc.rust-lang.org/book/");
        assert!(ranked[0].credibility_fast > ranked[1].credibility_fast);
        assert!(ranked[1].is_serp_or_jump);
    }

    #[test]
    fn render_includes_evidence_fields() {
        let hits = score_and_rank(vec![SearchResult {
            site_name: "X".into(),
            title: "T".into(),
            url: "https://example.com/a".into(),
            snippet: "has 42 numbers".into(),
            content: String::new(),
            date: "2026-01-01".into(),
        }]);
        let (text, cites) = render_scored(&hits);
        assert!(text.contains("credibility="));
        assert!(text.contains("selection="));
        assert_eq!(cites, ["https://example.com/a"]);
    }
}
