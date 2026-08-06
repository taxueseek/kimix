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
use super::types::WebSearchConfig;
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

/// A minimal, purpose-built HTTP client for the Kimi search service.
#[derive(Clone)]
pub struct WebSearchClient {
    http: reqwest::Client,
    search_url: String,
    api_key: String,
    api_key_provider: Option<SharedApiKeyProvider>,
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
        } = config
        else {
            return Err(tool_error(
                "Cannot create WebSearchClient from disabled/hosted-only config",
            ));
        };
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

    /// Search the Kimi service with multi-query RRF, evidence scoring, and cache.
    ///
    /// Returns the rendered result text plus the unique result URLs as citations.
    pub async fn search(
        &self,
        query: &str,
        limit: u8,
        include_content: bool,
        tool_call_id: &str,
    ) -> Result<(String, Vec<String>), kimix_tool_runtime::ToolError> {
        // Cache on the original query (post-fusion view).
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

        let subqueries = expand_queries(query);
        // Fetch a bit more per subquery so RRF has room to re-rank.
        let per_query_limit = (limit as usize * 2).clamp(5, 20) as u8;

        let futs: Vec<_> = subqueries
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let call_id = if i == 0 {
                    tool_call_id.to_string()
                } else {
                    format!("{tool_call_id}-q{i}")
                };
                async move {
                    self.search_once(q, per_query_limit, include_content, &call_id)
                        .await
                }
            })
            .collect();

        let outcomes = join_all(futs).await;
        let mut lists: Vec<Vec<SearchResult>> = Vec::new();
        let mut last_err: Option<kimix_tool_runtime::ToolError> = None;
        let mut any_success = false;
        for outcome in outcomes {
            match outcome {
                Ok(hits) => {
                    any_success = true;
                    lists.push(hits);
                }
                Err(e) => last_err = Some(e),
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
        if status == reqwest::StatusCode::UNAUTHORIZED {
            self.record_401_attribution(&bearer);
            return Err(kimix_tool_runtime::ToolError::unauthorized(
                "Search service returned 401 Unauthorized".to_string(),
            ));
        }
        if !status.is_success() {
            return Err(tool_error(format!(
                "Failed to search. Status: {status}. This may indicate that the \
                 search service is currently unavailable."
            )));
        }
        let results = response
            .json::<SearchResponse>()
            .await
            .map_err(|e| tool_error(format!("Failed to parse search results: {e}")))?
            .search_results;
        Ok(results)
    }
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
        };
        assert!(WebSearchClient::new(&config, None).is_err());
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
                "timeout_seconds": 30,
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
