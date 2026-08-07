//! HTTP client for the Kimi search service (PRD F5).
//!
//! Wire contract ported from kimi-cli `tools/web/search.py` (`SearchWeb`)
//! and verified against the live `api.kimi.com/coding/v1` service:
//!
//! - `POST {search_url}` with JSON `{"text_query", "limit",
//!   "enable_page_crawling", "timeout_seconds": 30}`
//! - headers: `Authorization: Bearer <token>` and
//!   `X-Msh-Tool-Call-Id: <tool call id>` (search.py:82-88)
//! - 200 → `{"search_results": [{site_name, title, url, snippet,
//!   content?, date?, icon?, mime?}]}`
//!
//! Enhancements (native, no HTML SERP, no extra APIs):
//! - multi-query expansion + RRF fusion on the **same** endpoint
//! - snippet-level Selection×Absorption evidence scoring
//! - process-local positive/negative cache
//!
//! The server-side timeout is 30s but page crawling can run longer, so the
//! client allows a generous total timeout (search.py:74 uses 180s).
use super::cache::SearchCache;
use super::evidence::{render_scored, score_and_rank};
use super::rrf::{expand_queries, rrf_fuse};
use super::types::{ModelSearchEndpoint, WebSearchConfig};
use crate::attribution::{SharedAttributionCallback, ToolConsumer};
use crate::types::SharedApiKeyProvider;
use futures_util::future::join_all;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

/// Total request timeout. Tuned down from 180s: a stuck search held the whole
/// turn hostage (cancel could not interrupt the in-flight HTTP future quickly,
/// leaving the UI on "Cancelling…" for minutes). 60s bounds a hung search to a
/// single minute while still allowing page crawling when `include_content` is set.
/// Override with `KIMIX_WEB_SEARCH_TIMEOUT_SECS`.
fn search_timeout_secs() -> u64 {
    std::env::var("KIMIX_WEB_SEARCH_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
}
/// `timeout_seconds` request field — the server-side search budget
/// (search.py:93).
const SERVER_TIMEOUT_SECS: u64 = 20;
/// Consecutive failures before circuit opens (skip remote, fail fast).
const CIRCUIT_FAILURE_THRESHOLD: u32 = 3;
/// Circuit open duration in seconds.
const CIRCUIT_OPEN_SECS: u64 = 60;

fn tool_error(msg: impl Into<String>) -> kimix_tool_runtime::ToolError {
    kimix_tool_runtime::ToolError::execution(
        kimix_tool_protocol::ToolId::new("web_search").expect("valid"),
        msg.into(),
    )
}

/// Whether RRF multi-query expansion is allowed after the first hop succeeds.
/// Default ON; set `KIMIX_WEB_SEARCH_MULTI_QUERY=0` to force single-query
/// (saves subscription quota when the search service is billable per call).
fn multi_query_expand_enabled() -> bool {
    match std::env::var("KIMIX_WEB_SEARCH_MULTI_QUERY") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            !(t == "0" || t == "false" || t == "off" || t == "no")
        }
        Err(_) => true,
    }
}

/// Auth / quota denials: more subqueries cannot recover and only deepen the cost.
fn is_hard_search_stop(err: &kimix_tool_runtime::ToolError) -> bool {
    matches!(
        err.kind,
        kimix_tool_runtime::ToolErrorKind::Unauthorized
            | kimix_tool_runtime::ToolErrorKind::UsageLimitReached
            | kimix_tool_runtime::ToolErrorKind::UsagePoolExhausted
            | kimix_tool_runtime::ToolErrorKind::PermissionDenied
    )
}

/// Whether a 403 body looks like a Kimi/OpenAI-compatible search quota denial.
/// Mirrors `kimix_sampling_types::is_quota_denial` without pulling that crate
/// into `kimix-tools`.
fn is_search_quota_denial(status: u16, message: &str) -> bool {
    if status != 403 {
        return false;
    }
    let m = message.to_ascii_lowercase();
    if m.is_empty() {
        // Field observation: Kimi coding search often returns empty-body 403
        // when the search subscription cap is hit.
        return true;
    }
    [
        "access_terminated_error",
        "usage limit",
        "quota",
        "billing cycle",
        "billing",
        "insufficient_quota",
        "out of credits",
        "exceeded your current quota",
        "forbidden",
    ]
    .iter()
    .any(|needle| m.contains(needle))
}

/// Map a Kimi search HTTP status + body into a tool error.
///
/// 403 with quota/billing language is **not** "service unavailable" — it is a
/// subscription cap on `POST {coding_base}/search` (Kimi Code), which is a
/// different quota pool from chat completions and from xAI/grok search.
fn map_search_http_error(status: reqwest::StatusCode, body: &str) -> kimix_tool_runtime::ToolError {
    let code = status.as_u16();
    let body_trim = body.trim();
    let body_snip = if body_trim.len() > 240 {
        format!("{}…", &body_trim[..240])
    } else {
        body_trim.to_string()
    };

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return kimix_tool_runtime::ToolError::unauthorized(format!(
            "Search service returned 401 Unauthorized.{}",
            if body_snip.is_empty() {
                String::new()
            } else {
                format!(" Body: {body_snip}")
            }
        ));
    }

    if is_search_quota_denial(code, body_trim) {
        let detail = if body_snip.is_empty() {
            format!(
                "web_search got HTTP {code} from the active search backend \
                 (Kimi coding `/search` or a dedicated search model via \
                 `[models] web_search` / `KIMIX_WEB_SEARCH_MODEL`). This is a \
                 **search/subscription quota or policy denial**, not chat \
                 quota and not a local network fault. Switch search model, \
                 wait for reset, top up the provider, or use web_fetch / argo."
            )
        } else {
            format!(
                "web_search denied (HTTP {code}): {body_snip}. \
                 If this is quota/billing, wait for reset or switch \
                 `[models] web_search`; do not retry the same query in a tight loop."
            )
        };
        return kimix_tool_runtime::ToolError::usage_limit_reached(detail);
    }

    tool_error(format!(
        "Failed to search. Status: {status}.{}",
        if body_snip.is_empty() {
            " This may indicate that the search service is currently unavailable."
                .to_string()
        } else {
            format!(" Body: {body_snip}")
        }
    ))
}

/// One search hit (kimi-cli search.py `SearchResult`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SearchResult {
    #[serde(default)]
    pub site_name: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub date: String,
}

/// Response envelope (kimi-cli search.py `Response`).
#[derive(Debug, serde::Deserialize)]
struct SearchResponse {
    search_results: Vec<SearchResult>,
}

#[derive(Debug, Default)]
struct CircuitState {
    failures: AtomicU32,
    /// Unix-ish monotonic marker: open until this Instant (via duration since open_at).
    open_until: parking_lot::Mutex<Option<std::time::Instant>>,
}

impl CircuitState {
    fn is_open(&self) -> bool {
        let guard = self.open_until.lock();
        if let Some(until) = *guard
            && std::time::Instant::now() < until
        {
            return true;
        }
        false
    }

    fn record_success(&self) {
        self.failures.store(0, AtomicOrdering::Relaxed);
        *self.open_until.lock() = None;
    }

    fn record_failure(&self) {
        let n = self.failures.fetch_add(1, AtomicOrdering::Relaxed) + 1;
        if n >= CIRCUIT_FAILURE_THRESHOLD {
            *self.open_until.lock() =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(CIRCUIT_OPEN_SECS));
        }
    }
}

/// HTTP client for web search: optional **search-model sidecar** (Responses +
/// hosted `web_search`) and/or Kimi coding `POST …/search` (client path).
#[derive(Clone)]
pub struct WebSearchClient {
    http: reqwest::Client,
    search_url: String,
    api_key: String,
    api_key_provider: Option<SharedApiKeyProvider>,
    /// Grok-style dedicated search model (chat model A can differ).
    model_search: Option<ModelSearchEndpoint>,
    /// Optional 401-attribution hook. Callers can wire this so a 401 from
    /// the search service emits an `auth_401_attribution` event with
    /// `consumer == "WebSearch"`.
    attribution_callback: Option<SharedAttributionCallback>,
    cache: SearchCache,
    circuit: Arc<CircuitState>,
}

impl WebSearchClient {
    /// Create a new web search client from `WebSearchConfig::Enabled`.
    ///
    /// Returns `Err` if the config is not `Enabled` or if header values are invalid.
    pub fn new(
        config: &WebSearchConfig,
        api_key_provider: Option<SharedApiKeyProvider>,
    ) -> Result<Self, kimix_tool_runtime::ToolError> {
        let WebSearchConfig::Enabled {
            search_url,
            api_key,
            extra_headers,
            model_search,
        } = config
        else {
            return Err(tool_error(
                "Cannot create WebSearchClient from disabled/hosted-only config",
            ));
        };
        if model_search.is_none() && search_url.trim().is_empty() {
            return Err(tool_error(
                "WebSearchConfig::Enabled has neither model_search nor Kimi search_url",
            ));
        }
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        for (key, value) in extra_headers {
            let header_name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|e| tool_error(format!("Invalid header name '{key}': {e}")))?;
            let header_value = HeaderValue::from_str(value)
                .map_err(|e| tool_error(format!("Invalid header value for '{key}': {e}")))?;
            headers.insert(header_name, header_value);
        }
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(search_timeout_secs()))
            .build()
            .map_err(|e| tool_error(format!("Failed to build HTTP client: {e}")))?;
        Ok(Self {
            http,
            search_url: search_url.clone(),
            api_key: api_key.clone(),
            api_key_provider,
            model_search: model_search.clone(),
            attribution_callback: None,
            cache: SearchCache::new(),
            circuit: Arc::new(CircuitState::default()),
        })
    }

    /// Wire a 401-attribution callback into this client. Idempotent;
    /// safe to call before or after the first request.
    pub fn with_attribution_callback(
        mut self,
        callback: Option<SharedAttributionCallback>,
    ) -> Self {
        self.attribution_callback = callback;
        self
    }

    /// Live token from the provider (OAuth refresh) when available, else the
    /// config-time key.
    async fn current_bearer(&self) -> String {
        crate::types::api_key_provider::resolve_bearer(self.api_key_provider.as_ref())
            .await
            .unwrap_or_else(|| self.api_key.clone())
    }

    fn record_401_attribution(&self, sent_bearer: &str) {
        crate::attribution::emit_401(
            self.attribution_callback.as_ref(),
            ToolConsumer::WebSearch,
            Some(sent_bearer),
        );
    }

    /// Search with multi-backend priority (Grok-style tool decoupling):
    ///
    /// 1. **Model sidecar** (`[models] web_search` / `KIMIX_WEB_SEARCH_MODEL`) —
    ///    Responses API + server `web_search` on model B (chat may be A).
    /// 2. **Kimi client** `POST {coding_base}/search` with multi-query RRF,
    ///    evidence scoring, and cache (existing path; retained as fallback).
    ///
    /// Returns the rendered result text plus unique result URLs as citations.
    pub async fn search(
        &self,
        query: &str,
        limit: u8,
        include_content: bool,
        tool_call_id: &str,
    ) -> Result<(String, Vec<String>), kimix_tool_runtime::ToolError> {
        // Cache on the original query (post-fusion view). Shared across backends.
        if let Some(cached) = self.cache.get(query, limit, include_content) {
            match cached {
                Ok(hits) => {
                    let scored = score_and_rank(hits);
                    return Ok(render_scored(&scored));
                }
                Err(()) => {
                    return Err(tool_error(
                        "Search recently returned no results for this query (negative cache). \
                         Try a more specific query.",
                    ));
                }
            }
        }

        if self.circuit.is_open() {
            return Err(tool_error(
                "Search service circuit open after repeated failures. Retry in ~60s, \
                 or use web_fetch on a known URL if you already have one.",
            ));
        }

        // ── 1) Dedicated search model (tool-decoupled) ───────────────────
        if self.model_search.is_some() {
            match self.search_via_model(query, limit, tool_call_id).await {
                Ok(rendered) => {
                    self.circuit.record_success();
                    // Cache a synthetic single hit so identical queries skip
                    // the expensive model round-trip.
                    let (text, cites) = rendered;
                    let synthetic: Vec<SearchResult> = cites
                        .iter()
                        .enumerate()
                        .map(|(i, url)| SearchResult {
                            site_name: String::new(),
                            title: format!("result-{}", i + 1),
                            url: url.clone(),
                            snippet: text.chars().take(200).collect(),
                            content: if i == 0 { text.clone() } else { String::new() },
                            date: String::new(),
                        })
                        .collect();
                    if synthetic.is_empty() {
                        // Still cache text-only success under a placeholder.
                        self.cache.put_hits(
                            query,
                            limit,
                            include_content,
                            vec![SearchResult {
                                site_name: String::new(),
                                title: "model-search".into(),
                                url: String::new(),
                                snippet: text.chars().take(200).collect(),
                                content: text.clone(),
                                date: String::new(),
                            }],
                        );
                    } else {
                        self.cache
                            .put_hits(query, limit, include_content, synthetic);
                    }
                    return Ok((text, cites));
                }
                Err(e) => {
                    let has_kimi = !self.search_url.trim().is_empty();
                    if is_hard_search_stop(&e) && !has_kimi {
                        self.circuit.record_failure();
                        return Err(e);
                    }
                    if !has_kimi {
                        self.circuit.record_failure();
                        return Err(e);
                    }
                    tracing::warn!(
                        error = %e,
                        "web_search model sidecar failed; falling back to Kimi client path"
                    );
                }
            }
        }

        // ── 2) Kimi coding search client (RRF + evidence) ────────────────
        if self.search_url.trim().is_empty() {
            self.circuit.record_failure();
            return Err(tool_error(
                "web_search: no model_search and no Kimi coding search URL configured",
            ));
        }

        let subqueries = expand_queries(query);
        // Fetch a bit more per subquery so RRF has room to re-rank.
        let per_query_limit = (limit as usize * 2).clamp(5, 20) as u8;

        // Run the original query first. Only expand (RRF multi-query) after a
        // successful first hop — parallel fan-out used to burn 2–3× Kimi
        // search quota on every call, and a 403 would hit the endpoint thrice
        // before the model even saw the error.
        let first = self
            .search_once(&subqueries[0], per_query_limit, include_content, tool_call_id)
            .await;
        let mut lists: Vec<Vec<SearchResult>> = Vec::new();
        let mut last_err: Option<kimix_tool_runtime::ToolError> = None;
        let mut any_success = false;
        match first {
            Ok(hits) => {
                any_success = true;
                lists.push(hits);
            }
            Err(e) => {
                // Hard stop on auth/quota — further subqueries cannot help and
                // only deepen the 403.
                if is_hard_search_stop(&e) {
                    self.circuit.record_failure();
                    return Err(e);
                }
                last_err = Some(e);
            }
        }

        if any_success && subqueries.len() > 1 && multi_query_expand_enabled() {
            let futs: Vec<_> = subqueries
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, q)| {
                    let call_id = format!("{tool_call_id}-q{i}");
                    async move {
                        self.search_once(q, per_query_limit, include_content, &call_id)
                            .await
                    }
                })
                .collect();
            for outcome in join_all(futs).await {
                match outcome {
                    Ok(hits) => lists.push(hits),
                    Err(e) => {
                        // Ignore soft expand failures; first hop already worked.
                        tracing::debug!(error = %e, "web_search expand subquery failed");
                    }
                }
            }
        }

        if !any_success {
            self.circuit.record_failure();
            return Err(last_err.unwrap_or_else(|| {
                tool_error("Search request failed: no successful subquery responses")
            }));
        }
        self.circuit.record_success();

        let fused = rrf_fuse(&lists, limit as usize);
        if fused.is_empty() {
            self.cache.put_empty(query, limit, include_content);
            return Ok((
                "No search results found. Try a more specific query.\n".to_string(),
                Vec::new(),
            ));
        }

        self.cache
            .put_hits(query, limit, include_content, fused.clone());
        let scored = score_and_rank(fused);
        Ok(render_scored(&scored))
    }

    /// Tool-decoupled search via a dedicated model B (Responses + `web_search`).
    ///
    /// Mirrors upstream Grok `[models] web_search` / `GROK_WEB_SEARCH_MODEL`:
    /// the chat model only issues the tool call; this path runs the search
    /// on the configured provider (DeepSeek official, Grok, …).
    async fn search_via_model(
        &self,
        query: &str,
        limit: u8,
        tool_call_id: &str,
    ) -> Result<(String, Vec<String>), kimix_tool_runtime::ToolError> {
        let ms = self
            .model_search
            .as_ref()
            .ok_or_else(|| tool_error("model_search not configured"))?;

        let base = ms.base_url.trim_end_matches('/');
        // Accept bases with or without trailing `/v1`.
        let responses_url = if base.ends_with("/responses") {
            base.to_string()
        } else {
            format!("{base}/responses")
        };

        let prompt = format!(
            "Use web search to find up-to-date information for this query:\n\n\
             {query}\n\n\
             After searching, return up to {limit} useful sources as markdown bullets:\n\
             - **Title** — URL\n  short snippet\n\
             Prefer primary / high-credibility sources. If the search tool fails, \
             say so explicitly instead of inventing results."
        );

        let mut req = self
            .http
            .post(&responses_url)
            .header(AUTHORIZATION, format!("Bearer {}", ms.api_key))
            .header(CONTENT_TYPE, "application/json")
            .header("X-Msh-Tool-Call-Id", tool_call_id);
        for (k, v) in &ms.extra_headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                req = req.header(name, val);
            }
        }

        let body = serde_json::json!({
            "model": ms.model,
            "tools": [{ "type": "web_search" }],
            "input": prompt,
            "stream": false,
            "max_output_tokens": 2048,
        });

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tool_error(format!(
                    "web_search model sidecar request failed ({model} @ {url}): {e}",
                    model = ms.model,
                    url = responses_url
                ))
            })?;

        let status = response.status();
        let text_body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(map_search_http_error(status, &text_body));
        }

        let (content, citations) = extract_model_search_output(&text_body);
        if content.trim().is_empty() {
            return Err(tool_error(format!(
                "web_search model sidecar ({}) returned empty content. \
                 Ensure the model supports Responses API + web_search \
                 (supports_backend_search).",
                ms.model
            )));
        }

        let mut out = format!(
            "<!-- search via model sidecar: {} -->\n{}",
            ms.model, content
        );
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok((out, citations))
    }

    /// Whether a Kimi coding search URL is configured.
    #[cfg(test)]
    fn has_kimi_path(&self) -> bool {
        !self.search_url.trim().is_empty()
    }

    /// Single-query remote call (no fusion/scoring).
    async fn search_once(
        &self,
        query: &str,
        limit: u8,
        include_content: bool,
        tool_call_id: &str,
    ) -> Result<Vec<SearchResult>, kimix_tool_runtime::ToolError> {
        let bearer = self.current_bearer().await;
        let response = self
            .http
            .post(&self.search_url)
            .header(AUTHORIZATION, format!("Bearer {bearer}"))
            .header("X-Msh-Tool-Call-Id", tool_call_id)
            .json(&serde_json::json!({
                "text_query": query,
                "limit": limit,
                "enable_page_crawling": include_content,
                "timeout_seconds": SERVER_TIMEOUT_SECS,
            }))
            .send()
            .await
            .map_err(|e| {
                tool_error(format!(
                    "Search request failed: {e}. The search service may be unavailable."
                ))
            })?;
        let status = response.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::UNAUTHORIZED {
                self.record_401_attribution(&bearer);
            }
            let body = response.text().await.unwrap_or_default();
            return Err(map_search_http_error(status, &body));
        }
        let results = response
            .json::<SearchResponse>()
            .await
            .map_err(|e| tool_error(format!("Failed to parse search results: {e}")))?
            .search_results;
        Ok(results)
    }
}

/// Pull assistant text + URLs from a Responses-style JSON body.
/// Tolerant of DeepSeek / OpenAI / proxy envelope differences.
fn extract_model_search_output(body: &str) -> (String, Vec<String>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return (body.trim().to_string(), Vec::new());
    };

    let mut texts: Vec<String> = Vec::new();
    let mut urls: Vec<String> = Vec::new();

    fn walk(node: &serde_json::Value, texts: &mut Vec<String>, urls: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(map) => {
                if let Some(t) = map.get("type").and_then(|x| x.as_str()) {
                    if (t == "output_text" || t == "text")
                        && let Some(s) = map.get("text").and_then(|x| x.as_str())
                    {
                        if !s.trim().is_empty() {
                            texts.push(s.to_string());
                        }
                    }
                }
                if let Some(s) = map.get("output_text").and_then(|x| x.as_str()) {
                    if !s.trim().is_empty() {
                        texts.push(s.to_string());
                    }
                }
                // Top-level convenience fields some gateways emit.
                if let Some(s) = map.get("content").and_then(|x| x.as_str()) {
                    if !s.trim().is_empty() && map.get("type").is_none() {
                        texts.push(s.to_string());
                    }
                }
                for (k, child) in map {
                    if k == "url" || k == "uri" {
                        if let Some(u) = child.as_str() {
                            if u.starts_with("http") && !urls.iter().any(|x| x == u) {
                                urls.push(u.to_string());
                            }
                        }
                    } else {
                        walk(child, texts, urls);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for child in arr {
                    walk(child, texts, urls);
                }
            }
            serde_json::Value::String(s) => {
                // Opportunistic URL harvest from free text.
                for token in s.split_whitespace() {
                    let t = token.trim_matches(|c: char| {
                        c == '(' || c == ')' || c == '[' || c == ']' || c == ',' || c == '.'
                    });
                    if t.starts_with("http://") || t.starts_with("https://") {
                        if !urls.iter().any(|x| x == t) {
                            urls.push(t.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    walk(&v, &mut texts, &mut urls);

    // Prefer the longest text block (usually the final assistant message).
    texts.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let content = texts.first().cloned().unwrap_or_else(|| {
        // Last resort: pretty JSON if nothing extractable.
        String::new()
    });

    // Harvest URLs from chosen content as well.
    for token in content.split_whitespace() {
        let t = token.trim_matches(|c: char| {
            c == '(' || c == ')' || c == '[' || c == ']' || c == ',' || c == '.'
        });
        if (t.starts_with("http://") || t.starts_with("https://")) && !urls.iter().any(|x| x == t)
        {
            urls.push(t.to_string());
        }
    }

    (content, urls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn enabled_config(url: &str) -> WebSearchConfig {
        WebSearchConfig::Enabled {
            search_url: url.to_string(),
            api_key: "test-key".to_string(),
            extra_headers: IndexMap::new(),
            model_search: None,
        }
    }

    #[test]
    fn new_rejects_disabled_config() {
        assert!(WebSearchClient::new(&WebSearchConfig::Disabled, None).is_err());
        assert!(WebSearchClient::new(&WebSearchConfig::HostedOnly, None).is_err());
    }

    #[test]
    fn new_rejects_invalid_extra_header() {
        let mut headers = IndexMap::new();
        headers.insert("bad header name".to_string(), "v".to_string());
        let config = WebSearchConfig::Enabled {
            search_url: "https://api.kimi.com/coding/v1/search".to_string(),
            api_key: "k".to_string(),
            extra_headers: headers,
            model_search: None,
        };
        assert!(WebSearchClient::new(&config, None).is_err());
    }

    #[test]
    fn extract_model_search_output_from_responses_shape() {
        let body = serde_json::json!({
            "output": [
                {
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "- **Rust** — https://www.rust-lang.org/\n  systems language"
                    }]
                }
            ]
        })
        .to_string();
        let (text, urls) = extract_model_search_output(&body);
        assert!(text.contains("Rust"));
        assert!(urls.iter().any(|u| u.contains("rust-lang.org")));
    }

    #[tokio::test]
    async fn model_sidecar_preferred_over_kimi() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "- **Docs** — https://example.com/a\n  from model sidecar"
                    }]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        // Kimi path would 500 if called — prove we don't hit it.
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let config = WebSearchConfig::Enabled {
            search_url: format!("{}/search", server.uri()),
            api_key: "kimi-key".into(),
            extra_headers: IndexMap::new(),
            model_search: Some(ModelSearchEndpoint {
                model: "deepseek-v4-flash".into(),
                base_url: format!("{}/v1", server.uri()),
                api_key: "ds-key".into(),
                extra_headers: IndexMap::new(),
            }),
        };
        let client = WebSearchClient::new(&config, None).unwrap();
        let (content, cites) = client.search("q", 5, false, "call-1").await.unwrap();
        assert!(content.contains("model sidecar") || content.contains("Docs"));
        assert!(cites.iter().any(|u| u.contains("example.com")));
        assert!(client.has_kimi_path());
    }

    #[tokio::test]
    async fn search_sends_kimi_wire_contract_and_parses_results() {
        use wiremock::matchers::{body_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .and(header("authorization", "Bearer test-key"))
            .and(header("x-msh-tool-call-id", "call-42"))
            .and(body_json(serde_json::json!({
                "text_query": "rust ownership",
                "limit": 10,
                "enable_page_crawling": false,
                "timeout_seconds": 20,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "search_results": [{
                    "site_name": "Docs",
                    "title": "Ownership",
                    "url": "https://doc.rust-lang.org/ownership",
                    "snippet": "What is ownership?",
                    "content": "",
                    "date": "2026-05-01",
                    "icon": "",
                    "mime": ""
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client =
            WebSearchClient::new(&enabled_config(&format!("{}/search", server.uri())), None)
                .unwrap();
        let (content, citations) = client
            .search("rust ownership", 5, false, "call-42")
            .await
            .unwrap();
        assert!(content.contains("Title: Ownership"));
        assert!(content.contains("URL: https://doc.rust-lang.org/ownership"));
        assert!(content.contains("credibility="));
        assert_eq!(citations, ["https://doc.rust-lang.org/ownership"]);
    }

    #[tokio::test]
    async fn search_maps_401_to_unauthorized() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let client =
            WebSearchClient::new(&enabled_config(&format!("{}/search", server.uri())), None)
                .unwrap();
        let err = client.search("q", 5, false, "c").await.unwrap_err();
        assert!(err.to_string().contains("401"), "{err}");
        assert_eq!(err.kind, kimix_tool_runtime::ToolErrorKind::Unauthorized);
    }

    #[tokio::test]
    async fn search_maps_empty_403_to_usage_limit() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let client =
            WebSearchClient::new(&enabled_config(&format!("{}/search", server.uri())), None)
                .unwrap();
        let err = client.search("q", 5, false, "c").await.unwrap_err();
        assert_eq!(err.kind, kimix_tool_runtime::ToolErrorKind::UsageLimitReached);
        assert!(
            err.to_string().contains("search subscription quota")
                || err.to_string().contains("quota"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn search_surfaces_server_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let client =
            WebSearchClient::new(&enabled_config(&format!("{}/search", server.uri())), None)
                .unwrap();
        let err = client.search("q", 5, false, "c").await.unwrap_err();
        assert!(err.to_string().contains("503"), "{err}");
    }

    #[tokio::test]
    async fn second_identical_search_hits_cache() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "search_results": [{
                    "site_name": "Docs",
                    "title": "Cached",
                    "url": "https://example.com/cached",
                    "snippet": "from network",
                    "content": "",
                    "date": "2026-01-01"
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client =
            WebSearchClient::new(&enabled_config(&format!("{}/search", server.uri())), None)
                .unwrap();
        let (c1, _) = client.search("cache me", 5, false, "c1").await.unwrap();
        let (c2, _) = client.search("cache me", 5, false, "c2").await.unwrap();
        assert!(c1.contains("Cached"));
        assert!(c2.contains("Cached"));
        // wiremock expect(1) enforces single network call
    }
}
