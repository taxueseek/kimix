use indexmap::IndexMap;

/// Dedicated model endpoint for tool-decoupled web search (Grok-style
/// `[models] web_search` / `KIMIX_WEB_SEARCH_MODEL`).
///
/// Chat may use model A; the `web_search` tool calls **this** model B via
/// Responses + server `web_search` tool — independent of the session model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ModelSearchEndpoint {
    /// Wire model id sent in the Responses body.
    pub model: String,
    /// API base URL (e.g. `https://api.deepseek.com` or `…/v1`).
    pub base_url: String,
    /// Bearer (or provider) key for this endpoint.
    pub api_key: String,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub extra_headers: IndexMap<String, String>,
}

/// Configuration for the `web_search` tool (PRD F5 + hosted path + model sidecar).
///
/// - [`Disabled`]: kill-switch or explicit off — no client tool, no hosted WebSearch.
/// - [`HostedOnly`]: user wants web search but no client backends are ready.
///   Hosted `WebSearch` may still attach when the **chat** model supports it.
/// - [`Enabled`]: function tool registered. Prefer [`ModelSearchEndpoint`] when
///   set (decoupled search model B); otherwise / as fallback Kimi
///   `POST {coding_base}/search` (client HTTP, RRF, evidence scoring).
///
/// Availability rule: never gate hosted search solely on missing OAuth.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WebSearchConfig {
    #[default]
    Disabled,
    /// Prefer hosted/backend search; do not register the local function tool.
    HostedOnly,
    Enabled {
        /// Full POST endpoint, e.g. `https://api.kimi.com/coding/v1/search`.
        /// Empty when only [`Self::Enabled::model_search`] is configured.
        search_url: String,
        /// Initial bearer for Kimi client path; live token from the api-key
        /// provider (OAuth refresh) takes precedence per request.
        api_key: String,
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        extra_headers: IndexMap<String, String>,
        /// Optional search-model sidecar (Responses + hosted web_search).
        /// When set, tried **before** the Kimi HTTP path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_search: Option<ModelSearchEndpoint>,
    },
}

impl WebSearchConfig {
    /// Function tool may be registered (Kimi client and/or model sidecar).
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Enabled {
                search_url,
                model_search,
                ..
            } => model_search.is_some() || !search_url.trim().is_empty(),
            _ => false,
        }
    }

    /// User wants web search capability (client and/or hosted).
    pub fn wants_search(&self) -> bool {
        matches!(self, Self::Enabled { .. } | Self::HostedOnly)
    }

    /// Return a copy safe for returning to clients: secrets redacted.
    pub fn redacted(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::HostedOnly => Self::HostedOnly,
            Self::Enabled {
                search_url,
                extra_headers,
                model_search,
                ..
            } => Self::Enabled {
                search_url: search_url.clone(),
                api_key: "***REDACTED***".to_string(),
                extra_headers: extra_headers.clone(),
                model_search: model_search.as_ref().map(|m| ModelSearchEndpoint {
                    model: m.model.clone(),
                    base_url: m.base_url.clone(),
                    api_key: "***REDACTED***".to_string(),
                    extra_headers: m.extra_headers.clone(),
                }),
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
            model_search: Some(ModelSearchEndpoint {
                model: "deepseek-v4-flash".into(),
                base_url: "https://api.deepseek.com".into(),
                api_key: "ds-secret".into(),
                extra_headers: IndexMap::new(),
            }),
        };
        match config.redacted() {
            WebSearchConfig::Enabled {
                search_url,
                api_key,
                extra_headers,
                model_search,
            } => {
                assert_eq!(api_key, "***REDACTED***");
                assert_eq!(search_url, "https://api.kimi.com/coding/v1/search");
                assert_eq!(extra_headers.get("X-Custom").unwrap(), "value");
                assert_eq!(
                    model_search.as_ref().map(|m| m.api_key.as_str()),
                    Some("***REDACTED***")
                );
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
            model_search: None,
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

    #[test]
    fn model_search_only_is_enabled() {
        let config = WebSearchConfig::Enabled {
            search_url: String::new(),
            api_key: String::new(),
            extra_headers: IndexMap::new(),
            model_search: Some(ModelSearchEndpoint {
                model: "m".into(),
                base_url: "https://example.com/v1".into(),
                api_key: "k".into(),
                extra_headers: IndexMap::new(),
            }),
        };
        assert!(config.is_enabled());
        assert!(config.wants_search());
    }
}
