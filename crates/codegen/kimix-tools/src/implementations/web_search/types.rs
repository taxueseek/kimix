use indexmap::IndexMap;

/// Configuration for the `web_search` tool (PRD F5 + hosted path).
///
/// - [`Disabled`]: kill-switch or explicit off — no client tool, no hosted WebSearch.
/// - [`HostedOnly`]: user wants web search but client HTTP credentials are
///   unavailable (e.g. API-key session without Kimi Code OAuth). Hosted
///   `WebSearch` may still be offered when the model supports backend search.
/// - [`Enabled`]: client can call the subscription search HTTP endpoint.
///
/// Availability rule: never gate hosted search solely on missing OAuth.
/// Client HTTP remains OAuth/subscription-bound because that is the only
/// channel that exposes `POST {coding_base}/search`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WebSearchConfig {
    #[default]
    Disabled,
    /// Prefer hosted/backend search; do not register the local function tool.
    HostedOnly,
    Enabled {
        /// Full POST endpoint, e.g. `https://api.kimi.com/coding/v1/search`.
        search_url: String,
        /// Initial bearer token; a live token from the api-key provider
        /// (OAuth refresh) takes precedence per request.
        api_key: String,
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        extra_headers: IndexMap<String, String>,
    },
}

impl WebSearchConfig {
    /// Client HTTP search is ready (function tool may be registered).
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    /// User wants web search capability (client and/or hosted).
    pub fn wants_search(&self) -> bool {
        matches!(self, Self::Enabled { .. } | Self::HostedOnly)
    }

    /// Return a copy safe for returning to clients: the `api_key` is
    /// replaced with `"***REDACTED***"`.
    pub fn redacted(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::HostedOnly => Self::HostedOnly,
            Self::Enabled {
                search_url,
                extra_headers,
                ..
            } => Self::Enabled {
                search_url: search_url.clone(),
                api_key: "***REDACTED***".to_string(),
                extra_headers: extra_headers.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_is_disabled() {
        let config = WebSearchConfig::default();
        assert!(!config.is_enabled());
        assert!(!config.wants_search());
    }

    #[test]
    fn hosted_only_wants_search_but_not_client() {
        let config = WebSearchConfig::HostedOnly;
        assert!(!config.is_enabled());
        assert!(config.wants_search());
    }

    #[test]
    fn test_config_redacted() {
        let mut headers = IndexMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        let config = WebSearchConfig::Enabled {
            search_url: "https://api.kimi.com/coding/v1/search".to_string(),
            api_key: "secret-key-12345".to_string(),
            extra_headers: headers,
        };
        match config.redacted() {
            WebSearchConfig::Enabled {
                search_url,
                api_key,
                extra_headers,
            } => {
                assert_eq!(api_key, "***REDACTED***");
                assert_eq!(search_url, "https://api.kimi.com/coding/v1/search");
                assert_eq!(extra_headers.get("X-Custom").unwrap(), "value");
            }
            other => panic!("expected Enabled variant, got {other:?}"),
        }
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config = WebSearchConfig::Enabled {
            search_url: "https://api.kimi.com/coding/v1/search".to_string(),
            api_key: "key".to_string(),
            extra_headers: IndexMap::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: WebSearchConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_enabled());
        assert!(parsed.wants_search());

        let hosted = WebSearchConfig::HostedOnly;
        let json = serde_json::to_string(&hosted).unwrap();
        let parsed: WebSearchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, WebSearchConfig::HostedOnly);
    }
}
