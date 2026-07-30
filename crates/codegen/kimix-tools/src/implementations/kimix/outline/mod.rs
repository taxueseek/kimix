//! `outline` tool — single-file symbol outline via kimix-codebase-graph.
//!
//! Prefer over full-file reads when only structure (defs / line numbers) is needed.
//! Does not require an LSP server (tree-sitter queries only).
use crate::types::output::ToolOutput;
use crate::types::resources::{
    Cwd, DisplayCwd, display_cwd_or_cwd, resolve_model_path,
};
use crate::types::tool::{ToolKind, ToolNamespace};
use kimix_codebase_graph::{
    format_outline, language_label_for_path, outline_file,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct OutlineInput {
    #[schemars(
        description = "Path to a source file (relative to workspace root or absolute). Supported: .rs, .go, .py, .ts/.tsx, .js/.jsx."
    )]
    pub file_path: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct OutlineToolOutput(pub String);

impl kimix_tool_runtime::ToolOutput for OutlineToolOutput {}

impl From<OutlineToolOutput> for ToolOutput {
    fn from(o: OutlineToolOutput) -> Self {
        ToolOutput::Text(o.0.into())
    }
}

const OUTLINE_DESCRIPTION: &str = r#"List definitions (functions, types, methods, modules, …) in a source file with 1-based line numbers — without reading the whole file into context.
Uses tree-sitter (no language server). Prefer over ${{ tools.by_kind.read }} when you only need structure / jump targets; then ${{ tools.by_kind.read }} with offset+limit for the specific region.
Supported extensions: .rs, .go, .py, .ts/.tsx, .js/.jsx.
For cross-file go-to-definition / references when an LSP is configured, prefer ${{ tools.by_kind.lsp }}."#;

#[derive(Debug, Default)]
pub struct OutlineTool;

impl crate::types::tool_metadata::ToolMetadata for OutlineTool {
    fn kind(&self) -> ToolKind {
        // Dedicated tool id `outline`; avoid sharing ToolKind::Lsp so
        // `${{ tools.by_kind.lsp }}` still resolves to the LSP tool.
        ToolKind::Other
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::Kimix
    }

    fn description_template(&self) -> &str {
        OUTLINE_DESCRIPTION
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl kimix_tool_runtime::Tool for OutlineTool {
    type Args = OutlineInput;
    type Output = OutlineToolOutput;

    fn id(&self) -> kimix_tool_protocol::ToolId {
        kimix_tool_protocol::ToolId::new("outline").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::kimix_tool_runtime::ListToolsContext,
    ) -> kimix_tool_types::ToolDescription {
        kimix_tool_types::ToolDescription::new(
            "outline",
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

    #[tracing::instrument(name = "tool.outline", skip_all, fields(file_path = %input.file_path))]
    async fn run(
        &self,
        ctx: kimix_tool_runtime::ToolCallContext,
        input: OutlineInput,
    ) -> Result<OutlineToolOutput, kimix_tool_runtime::ToolError> {
        use crate::types::tool_metadata::shared_resources;
        let resources = shared_resources(&ctx)?;

        let (cwd, display_cwd) = {
            let res = resources.lock().await;
            let cwd = res
                .get::<Cwd>()
                .map(|c| c.0.clone())
                .ok_or_else(|| {
                    kimix_tool_runtime::ToolError::custom("outline", "workspace cwd unavailable")
                })?;
            let display_cwd = res.get::<DisplayCwd>().map(|d| d.0.clone());
            (cwd, display_cwd)
        };

        let path = resolve_model_path(&cwd, display_cwd.as_deref(), &input.file_path);
        if path.is_dir() {
            return Err(kimix_tool_runtime::ToolError::custom(
                "outline",
                format!(
                    "{} is a directory; pass a source file path",
                    input.file_path
                ),
            ));
        }
        if !path.exists() {
            return Err(kimix_tool_runtime::ToolError::custom(
                "outline",
                format!("File not found: {}", input.file_path),
            ));
        }

        let entries = outline_file(&path).map_err(|e| {
            kimix_tool_runtime::ToolError::custom("outline", e.to_string())
        })?;

        let display_base = display_cwd_or_cwd(&cwd, display_cwd.as_deref());
        let display_path = path
            .strip_prefix(&cwd)
            .ok()
            .map(|rel| display_base.join(rel))
            .unwrap_or_else(|| path.clone());
        let display = display_path.display().to_string();
        let lang = language_label_for_path(&path);
        let text = format_outline(&display, &lang, &entries);
        Ok(OutlineToolOutput(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kimix_codebase_graph::outline_source;
    use std::path::Path;

    #[test]
    fn outline_source_roundtrip_via_tool_format() {
        let src = b"fn alpha() {}\nfn beta() {}\n";
        let entries = outline_source(Path::new("t.rs"), src).unwrap();
        let text = format_outline("t.rs", "rust", &entries);
        assert!(text.contains("alpha") || text.contains("beta") || text.contains("definition"));
    }

    #[test]
    fn tool_id_is_outline() {
        use kimix_tool_runtime::Tool;
        let t = OutlineTool;
        assert_eq!(t.id().as_str(), "outline");
        assert!(crate::types::tool_metadata::ToolMetadata::is_read_only(&t));
    }
}
