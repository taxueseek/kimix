//! `web_search` tool — new architecture (`Tool` trait).
//!
//! Calls the Kimi search service (PRD F5; kimi-cli `tools/web/search.py`
//! parity). Reads the pre-constructed `WebSearchClient` from Resources
//! (inserted by `with_backend()` when the config is `Enabled`, i.e. only
//! when client HTTP credentials exist). Hosted-only sessions rely on
//! backend `HostedTool::WebSearch` instead.
use crate::implementations::web_search::client::WebSearchClient;
use crate::types::output::WebSearchOutput;
use crate::types::requirements::{Expr, ToolRequirement};
use crate::types::tool::{ToolKind, ToolNamespace};

// ───────────────────────────────────────────────────────────────────────────
// Input
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct WebSearchInput {
    #[schemars(description = "The query text to search for.")]
    pub query: String,
    #[schemars(
        description = "The number of results to return (1-20). Typically you do \
                              not need to set this value. When the results do not contain \
                              what you need, you probably want to give a more concrete \
                              query."
    )]
    pub limit: Option<u8>,
    #[schemars(
        description = "Whether to include the content of the web pages in the \
                              results. It can consume a large amount of tokens when set. \
                              Avoid enabling this together with a large limit."
    )]
    pub include_content: Option<bool>,
}

/// kimi-cli search.py `Params.limit` default / bounds (default=5, ge=1, le=20).
const DEFAULT_LIMIT: u8 = 5;
const MAX_LIMIT: u8 = 20;

/// Tool description: search capability + agent discipline (Argo essence).
const WEB_SEARCH_DESCRIPTION: &str = "\
Search the web for up-to-date information (coding, research, facts).

Results include Evidence fields: selection (domain authority), absorption \
(snippet evidence density), freshness, credibility. Prefer high-credibility \
hits; treat SERP/jump URLs and social posts as narrative, not sole fact sources.

Discipline:
1. High-stakes claims (holdings, safety, whether something is true): search → \
read Evidence → web_fetch top sources → then conclude.
2. Numbers: keep units/口径; if sources conflict, list them side by side — never merge blindly.
3. Do not treat search-engine result pages as primary sources.
4. Social posts = narrative/sentiment only, not ground truth.
5. For fact checks, use specific queries (entity + metric + time); one vague query is not enough.
";

// ───────────────────────────────────────────────────────────────────────────
// Tool implementation
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct WebSearchTool;

impl crate::types::tool_metadata::ToolMetadata for WebSearchTool {
    fn kind(&self) -> ToolKind {
        ToolKind::WebSearch
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::Kimix
    }

    fn description_template(&self) -> &str {
        WEB_SEARCH_DESCRIPTION
    }

    fn requires_expr(&self) -> Expr<ToolRequirement> {
        Expr::True
    }
}

impl kimix_tool_runtime::Tool for WebSearchTool {
    type Args = WebSearchInput;
    type Output = WebSearchOutput;

    fn id(&self) -> kimix_tool_protocol::ToolId {
        kimix_tool_protocol::ToolId::new("web_search").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::kimix_tool_runtime::ListToolsContext,
    ) -> kimix_tool_types::ToolDescription {
        kimix_tool_types::ToolDescription::new(
            "web_search",
            crate::types::tool_metadata::ToolMetadata::description_template(self),
        )
    }

    fn capabilities(&self) -> kimix_tool_protocol::ToolCapabilities {
        kimix_tool_protocol::ToolCapabilities {
            is_read_only: true,
            tool_scope: Some(kimix_tool_protocol::ToolScope::Read),
            ..Default::default()
        }
    }

    #[tracing::instrument(name = "tool.web_search", skip_all)]
    async fn run(
        &self,
        ctx: kimix_tool_runtime::ToolCallContext,
        input: WebSearchInput,
    ) -> Result<WebSearchOutput, kimix_tool_runtime::ToolError> {
        use crate::types::tool_metadata::shared_resources;
        let resources = shared_resources(&ctx)?;

        let client;
        {
            let res = resources.lock().await;
            client = res.require::<WebSearchClient>()?.clone();
        }

        let limit = input.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let (content, citations) = client
            .search(
                &input.query,
                limit,
                input.include_content.unwrap_or(false),
                ctx.call_id.as_str(),
            )
            .await?;

        Ok(WebSearchOutput {
            query: input.query.clone(),
            content,
            citations,
            allowed_domains: None,
            pre_formatted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::resources::Resources;
    use crate::types::tool_metadata::test_ctx_with_call_id;

    #[test]
    fn tool_name_and_description() {
        let tool = WebSearchTool;
        assert_eq!(kimix_tool_runtime::Tool::id(&tool).as_str(), "web_search");
        let desc = crate::types::tool_metadata::ToolMetadata::description_template(&tool);
        assert!(desc.contains("Search the web"));
        assert!(desc.contains("Evidence"));
        assert!(
            desc.contains("High-stakes")
                || desc.contains("high-stakes")
                || desc.contains("Discipline")
        );
    }

    #[tokio::test]
    async fn errors_when_client_not_in_resources() {
        let resources = Resources::new();
        let tool = WebSearchTool;
        let result = kimix_tool_runtime::Tool::run(
            &tool,
            test_ctx_with_call_id(resources.into_shared(), "test-call"),
            WebSearchInput {
                query: "test".into(),
                limit: None,
                include_content: None,
            },
        )
        .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("missing required resource"),
            "Expected 'missing required resource' error, got: {err_msg}"
        );
    }
}
