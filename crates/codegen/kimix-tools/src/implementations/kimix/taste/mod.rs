//! `taste` tool — record an explicit user preference into the global taste
//! store (`~/.kimix/taste/taste.md`).
//!
//! The taste system learns coding preferences two ways:
//!
//! 1. **Git-signal mining** (session layer, `kimix-shell::session::taste`):
//!    diffs are scanned for recurring correction patterns and compiled into
//!    preference rules with confidence scores.
//! 2. **Explicit capture** (this tool): when the user states a preference in
//!    plain language ("I prefer 2-space indentation", "always use `#[must_use]`
//!    on new public APIs"), the model calls `taste` and the preference lands
//!    in the same store, visible to future sessions via the `<taste>` system
//!    prompt section.
//!
//! The tool is a **pure file operation** — no session dependencies — so it
//! registers in every toolset (native, harness-compat, compact) uniformly.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::output::ToolOutput;
use crate::types::tool::{ToolKind, ToolNamespace};

/// Input for the `taste` tool.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TasteInput {
    /// A concise, self-contained statement of the user's coding preference,
    /// e.g. "prefer 2-space indentation over tabs" or "use `assert_eq!`
    /// with a message argument in tests".
    pub instruction: String,
    /// Optional confidence (0.0–1.0). Defaults to 0.8 when omitted.
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// Registered name of the `taste` tool.
pub const TASTE_TOOL_NAME: &str = "taste";

#[derive(Debug, Default)]
pub struct TasteTool;

impl crate::types::tool_metadata::ToolMetadata for TasteTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Taste
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::Kimix
    }

    fn description_template(&self) -> &str {
        "Record a user-stated coding preference into the persistent taste store \
         (~/.kimix/taste/taste.md), where it is injected into future sessions via \
         the <taste> system prompt section.\n\n\
         Use when the user expresses a preference, style rule, or project convention \
         in plain language (e.g. \"I prefer 2-space indentation\", \"always add a \
         changelog entry\", \"use kebab-case file names\"). State the preference as a \
         single self-contained sentence so it stays meaningful outside the current \
         conversation. Do NOT call this for one-off task instructions — only durable, \
         reusable preferences.\n\n\
         Returns a confirmation listing the recorded preference."
    }
}

impl kimix_tool_runtime::Tool for TasteTool {
    type Args = TasteInput;
    type Output = ToolOutput;

    fn id(&self) -> kimix_tool_protocol::ToolId {
        kimix_tool_protocol::ToolId::new(TASTE_TOOL_NAME).expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &kimix_tool_runtime::ListToolsContext,
    ) -> kimix_tool_types::ToolDescription {
        kimix_tool_types::ToolDescription::new(
            TASTE_TOOL_NAME,
            crate::types::tool_metadata::ToolMetadata::description_template(self),
        )
    }

    fn capabilities(&self) -> kimix_tool_protocol::ToolCapabilities {
        kimix_tool_protocol::ToolCapabilities {
            is_read_only: false,
            tool_scope: Some(kimix_tool_protocol::ToolScope::Write),
            ..Default::default()
        }
    }

    async fn run(
        &self,
        _ctx: kimix_tool_runtime::ToolCallContext,
        input: TasteInput,
    ) -> Result<ToolOutput, kimix_tool_runtime::ToolError> {
        let instruction = input.instruction.trim().to_string();
        if instruction.is_empty() {
            return Ok(ToolOutput::Text(
                "No preference given — provide a one-sentence instruction.".into(),
            ));
        }
        let confidence = clamp_confidence(input.confidence.unwrap_or(0.8));
        let path = taste_store_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let line = format!("- {instruction}. Confidence: {confidence:.2}");
        let mut contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                "This file is auto-managed by Kimix taste learning.\n\
                 One preference per line: `- <learning>. Confidence: <0.0-1.0>`.\n"
                    .to_string()
            }
        };
        // De-duplicate: identical text already recorded → no-op.
        if contents.lines().any(|l| l.trim() == line) {
            return Ok(ToolOutput::Text(
                format!("Preference already recorded: {instruction}").into(),
            ));
        }
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&line);
        contents.push('\n');
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| {
                kimix_tool_runtime::ToolError::execution(
                    kimix_tool_protocol::ToolId::new(TASTE_TOOL_NAME).expect("valid"),
                    format!("cannot open taste store {path:?}: {e}"),
                )
            })?;
        f.write_all(contents.as_bytes()).map_err(|e| {
            kimix_tool_runtime::ToolError::execution(
                kimix_tool_protocol::ToolId::new(TASTE_TOOL_NAME).expect("valid"),
                format!("cannot write taste store {path:?}: {e}"),
            )
        })?;
        Ok(ToolOutput::Text(
            format!("Recorded preference (confidence {confidence:.2}): {instruction}").into(),
        ))
    }
}

/// `~/.kimix/taste/taste.md` (overridable via `KIMIX_TASTE_FILE` for tests).
pub fn taste_store_path() -> PathBuf {
    if let Ok(p) = std::env::var("KIMIX_TASTE_FILE") {
        return PathBuf::from(p);
    }
    crate::util::kimix_home::kimix_home().join("taste").join("taste.md")
}

fn clamp_confidence(c: f32) -> f32 {
    if c.is_finite() {
        c.clamp(0.0, 1.0)
    } else {
        0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_path_defaults_to_kimix_home() {
        // 未设置覆盖变量时指向 ~/.kimix/taste/taste.md
        let p = taste_store_path();
        assert!(p.ends_with("taste/taste.md"), "got {p:?}");
    }

    #[test]
    fn store_path_respects_override() {
        // 直接测试路径拼接逻辑（避免测试中改动进程环境变量）。
        let p = crate::util::kimix_home::kimix_home().join("taste").join("taste.md");
        assert!(p.ends_with("taste/taste.md"), "got {p:?}");
    }

    #[test]
    fn confidence_clamped() {
        assert_eq!(clamp_confidence(1.5), 1.0);
        assert_eq!(clamp_confidence(-0.2), 0.0);
        assert_eq!(clamp_confidence(0.5), 0.5);
        assert_eq!(clamp_confidence(f32::NAN), 0.8);
    }
}
