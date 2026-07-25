//! Subagent role and persona configuration types.
//!
//! These are the canonical definitions for `SubagentRole`, `SubagentPersona`,
//! and `PersonaIOField`. The shell re-exports them via
//! `kimix_shell::config::{SubagentRole, SubagentPersona, PersonaIOField}`.
//!
//! Methods that remain in `Kimix-shell` (on `SubagentsConfig`):
//! - `discover_personas()` / `discover_roles()` — filesystem discovery
//!   coupled to the shell's config resolution pipeline.
//! - `resolve()` — config layering (CLI > env > TOML > remote) is
//!   shell-specific. This crate receives already-resolved maps.
use kimix_tools::implementations::skills::discovery::extract_first_paragraph;
use std::path::PathBuf;

use serde::Deserialize;

/// A declarative subagent role definition from config.
///
/// Roles provide named presets that callers can reference via the
/// `subagent_type` field in the task tool. Each role can specify
/// a default capability mode, model override, and custom prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SubagentRole {
    /// Human-readable description of what this role does.
    pub description: String,
    /// Default capability mode for agents using this role.
    /// One of: "read-only", "read-write", "execute", "all".
    /// Can be overridden per-spawn via `capability_mode` in the task tool.
    #[serde(default)]
    pub default_capability_mode: Option<String>,
    /// Model override for this role. If set, agents using this role
    /// default to this model unless the spawn-time `model` override
    /// is provided.
    #[serde(default)]
    pub model: Option<String>,
    /// Default reasoning effort for this role (e.g. "low", "medium", "high").
    /// Can be overridden per-spawn via `reasoning_effort` in the task tool.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Path to a prompt/instruction file (relative to workspace root).
    /// Loaded at spawn time and prepended to the child's prompt as a
    /// `<role-instructions>` block.
    #[serde(default)]
    pub prompt_file: Option<String>,
    /// Inline prompt instructions. When set, used directly as the
    /// `<role-instructions>` block, avoiding file I/O. Takes precedence
    /// over `prompt_file` when both are set.
    #[serde(default)]
    pub inline_prompt: Option<String>,
    /// Default isolation mode ("none" or "worktree").
    #[serde(default)]
    pub default_isolation: Option<String>,
    /// Base directory for resolving relative `prompt_file` references.
    /// Set to the parent dir of the source `.toml` file during discovery.
    #[serde(skip)]
    pub source_dir: Option<PathBuf>,
}

/// A named persona/SOUL definition controlling tone, style, and behavior.
///
/// Personas are referenced by name via the `persona` field in the task tool.
/// Their instructions are prepended to the child's prompt as a `<persona>`
/// XML block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SubagentPersona {
    /// Inline instruction text applied as a persona layer.
    pub instructions: Option<String>,
    /// Optional short description shown in persona summaries.
    /// Falls back to first-paragraph extraction from `instructions`.
    pub description: Option<String>,
    /// Path to an instruction file (relative to workspace root).
    /// Content is loaded at spawn time and merged with `instructions`.
    /// If both are set, `instructions` is prepended before file content.
    pub instructions_file: Option<String>,
    /// Declared inputs this persona expects. The parent agent reads these
    /// to know what file paths or context to provide in the prompt.
    #[serde(default)]
    pub inputs: Vec<PersonaIOField>,
    /// Declared outputs this persona produces. The parent agent reads
    /// these to know what artifacts to expect and pass to the next agent.
    #[serde(default)]
    pub outputs: Vec<PersonaIOField>,
    /// Default isolation mode when this persona is used.
    #[serde(default)]
    pub default_isolation: Option<String>,
    /// Model override when this persona is used.
    #[serde(default)]
    pub model: Option<String>,
    /// Default reasoning effort for this persona (e.g. "low", "medium", "high").
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Base directory for resolving relative file references.
    /// Set to the parent dir of the source `.toml` file during discovery.
    /// When `None`, relative paths resolve against the workspace cwd.
    #[serde(skip)]
    pub source_dir: Option<PathBuf>,
    /// Absolute path to the source file this persona was loaded from.
    /// Populated during discovery; `None` for inline config personas.
    #[serde(skip)]
    pub source_path: Option<String>,
}

/// A declared input or output for a persona.
///
/// Enables the parent agent to discover what a persona needs (inputs)
/// and what it produces (outputs) without hardcoded knowledge of the
/// persona's protocol.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonaIOField {
    /// Short identifier (e.g. "review_file", "summary_file").
    pub name: String,
    /// What kind of artifact: "file", "text", etc.
    #[serde(default = "PersonaIOField::default_io_type")]
    pub io_type: String,
    /// Whether this input/output is required.
    #[serde(default)]
    pub required: bool,
    /// Human-readable description shown in the task tool help.
    pub description: String,
}

impl PersonaIOField {
    fn default_io_type() -> String {
        "file".to_string()
    }
}

impl SubagentPersona {
    /// Render a human-readable summary of this persona's IO contract
    /// for inclusion in the task tool description.
    pub fn render_io_summary(&self, name: &str) -> String {
        let fallback;
        let desc = if let Some(d) = self.description.as_deref().filter(|s| !s.trim().is_empty()) {
            d
        } else {
            fallback = self
                .instructions
                .as_deref()
                .and_then(extract_first_paragraph);
            fallback.as_deref().unwrap_or("Custom persona")
        };
        let scope = match self.source_path.as_deref() {
            Some(path) if path.contains("/bundled/") => "[bundled]",
            Some(_) => "[user]",
            None => "[local]",
        };
        let mut lines = vec![format!("- **{name}** {scope}: {desc}")];
        if let Some(ref path) = self.source_path {
            lines.push(format!("  Path: {path}"));
        }
        if !self.inputs.is_empty() {
            lines.push("    Expects in prompt:".to_string());
            for io in &self.inputs {
                let req = if io.required { "REQUIRED" } else { "optional" };
                lines.push(format!(
                    "      - `{}` ({}, {}): {}",
                    io.name, io.io_type, req, io.description
                ));
            }
        }
        if !self.outputs.is_empty() {
            lines.push("    Produces:".to_string());
            for io in &self.outputs {
                let req = if io.required { "REQUIRED" } else { "optional" };
                lines.push(format!(
                    "      - `{}` ({}, {}): {}",
                    io.name, io.io_type, req, io.description
                ));
            }
        }
        lines.join("\n")
    }
}

/// Return the 8 built-in default subagent roles.
///
/// These provide structured expertise presets aligned with the agent role
/// taxonomy: dev, research, write, arch, audit, agent, fast, coder.
/// Users can override any role via TOML config files in the roles directory.
pub fn default_roles() -> std::collections::HashMap<String, SubagentRole> {
    let mut roles = std::collections::HashMap::new();

    roles.insert("dev".into(), SubagentRole {
        description: "主力编码、修 bug、重构，负责日常开发任务".into(),
        default_capability_mode: Some("read-write".into()),
        model: Some("deepseek-v4-pro".into()),
        inline_prompt: Some("你是开发者（dev）角色，负责编码、调试、测试和重构。\n遵循项目现有规范，做最小改动解决完整问题。对无效输入快速失败。修改后运行相关测试。产出物：可工作的代码、通过的测试、清晰的变更说明。".into()),
        ..Default::default()
    });
    roles.insert("research".into(), SubagentRole {
        description: "快速调研、信息收集、数据分析".into(),
        default_capability_mode: Some("read-only".into()),
        model: Some("deepseek-v4-flash".into()),
        reasoning_effort: Some("medium".into()),
        inline_prompt: Some("你是研究员（research）角色，负责快速调研和信息收集。\n先广后窄，并行执行独立搜索。找不到时报告「未找到」，不扩大范围。区分事实和观点，优先技术准确性。产出物：结构化调研报告、数据摘要、信息来源列表。".into()),
        ..Default::default()
    });
    roles.insert("write".into(), SubagentRole {
        description: "中文写作、分析文档、文案优化".into(),
        default_capability_mode: Some("read-write".into()),
        model: Some("mimo-v2.5".into()),
        inline_prompt: Some("你是写作者（write）角色，负责中文写作和文档产出。\n中文使用直角引号「」和『』。输出自然流畅的口语化中文。不做不必要的概括和升华。每段末尾是具体内容而非空洞总结。产出物：文章、分析文档、文案、报告。".into()),
        ..Default::default()
    });
    roles.insert("arch".into(), SubagentRole {
        description: "系统设计、接口定义、技术方案".into(),
        default_capability_mode: Some("read-only".into()),
        model: Some("deepseek-v4-pro".into()),
        inline_prompt: Some("你是架构师（arch）角色，负责系统设计和技术方案。\n从第一性原理出发分析问题。开发做减法，验收做加法。遵循奥卡姆剃刀：如无必要，勿增实体。产出物：架构图、接口定义、技术方案文档、ADR。".into()),
        ..Default::default()
    });
    roles.insert("audit".into(), SubagentRole {
        description: "事实核查、代码审查、质量把关".into(),
        default_capability_mode: Some("read-only".into()),
        model: Some("longcat-2.0".into()),
        reasoning_effort: Some("high".into()),
        inline_prompt: Some("你是审查员（audit）角色，负责事实核查和代码审查。\n使用 CoVe 验证链：独立回答→交叉验证→基于验证结果审查。对抗性审查，假设所有路径都会出错。交叉验证信息来源。产出物：审查报告、发现清单、风险评估。".into()),
        ..Default::default()
    });
    roles.insert("agent".into(), SubagentRole {
        description: "多步编排、复杂混合任务、工具调用".into(),
        default_capability_mode: Some("execute".into()),
        model: Some("longcat-2.0".into()),
        reasoning_effort: Some("medium".into()),
        inline_prompt: Some("你是执行器（agent）角色，负责多步编排和复杂任务。\n算清交易成本再拆分：子任务拆分、上下文传递、结果验收的成本之和超过直接完成成本时，在内部执行。并行执行不依赖的独立任务。写任务给子 Agent 时自包含上下文。产出物：编排方案、执行结果、错误处理。".into()),
        ..Default::default()
    });
    roles.insert("fast".into(), SubagentRole {
        description: "简单脚本、小修小改、快速验证".into(),
        default_capability_mode: Some("read-write".into()),
        model: Some("step-3.7-flash".into()),
        inline_prompt: Some("你是快速编码（fast）角色，负责简单脚本和小修改。\n追求最小实现，不引入额外依赖。一个文件解决一个问题。不重构，不优化，只修复或实现指定功能。产出物：可运行的独立脚本、补丁、快速修复。".into()),
        ..Default::default()
    });
    roles.insert("coder".into(), SubagentRole {
        description: "复杂算法、大量代码输出、高性能实现".into(),
        default_capability_mode: Some("read-write".into()),
        model: Some("mimo-v2.5-pro".into()),
        reasoning_effort: Some("medium".into()),
        inline_prompt: Some("你是高级编码（coder）角色，负责复杂算法和高性能实现。\n先理解问题本质再编码。关注时间/空间复杂度。包含完整的错误处理和边界条件。附带测试用例验证正确性。产出物：完整实现、测试、性能分析。".into()),
        ..Default::default()
    });

    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagent_role_deserialize_defaults() {
        let role: SubagentRole = toml::from_str("").unwrap();
        assert_eq!(role.description, "");
        assert!(role.default_capability_mode.is_none());
        assert!(role.model.is_none());
        assert!(role.prompt_file.is_none());
    }

    #[test]
    fn subagent_role_deserialize_full() {
        let toml_str = r#"
description = "Research agent"
default_capability_mode = "read-only"
model = "kimix-3"
reasoning_effort = "high"
prompt_file = ".kimix/prompts/researcher.md"
default_isolation = "worktree"
"#;
        let role: SubagentRole = toml::from_str(toml_str).unwrap();
        assert_eq!(role.description, "Research agent");
        assert_eq!(role.default_capability_mode.as_deref(), Some("read-only"));
        assert_eq!(role.model.as_deref(), Some("kimix-3"));
        assert_eq!(role.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(
            role.prompt_file.as_deref(),
            Some(".kimix/prompts/researcher.md")
        );
        assert_eq!(role.default_isolation.as_deref(), Some("worktree"));
    }

    #[test]
    fn subagent_persona_deserialize_defaults() {
        let persona: SubagentPersona = toml::from_str("").unwrap();
        assert!(persona.instructions.is_none());
        assert!(persona.description.is_none());
        assert!(persona.instructions_file.is_none());
        assert!(persona.inputs.is_empty());
        assert!(persona.outputs.is_empty());
    }

    #[test]
    fn subagent_persona_deserialize_full() {
        let toml_str = r#"
instructions = "You are a concise writer."
description = "A concise writing persona."
instructions_file = ".kimix/personas/concise.md"
model = "kimix-3-fast"
reasoning_effort = "low"
default_isolation = "none"

[[inputs]]
name = "review_file"
io_type = "file"
required = true
description = "Path to the review notes file"

[[outputs]]
name = "summary_file"
io_type = "file"
required = false
description = "Path to write the summary"
"#;
        let persona: SubagentPersona = toml::from_str(toml_str).unwrap();
        assert_eq!(
            persona.instructions.as_deref(),
            Some("You are a concise writer.")
        );
        assert_eq!(
            persona.instructions_file.as_deref(),
            Some(".kimix/personas/concise.md")
        );
        assert_eq!(persona.model.as_deref(), Some("kimix-3-fast"));
        assert_eq!(persona.reasoning_effort.as_deref(), Some("low"));
        assert_eq!(persona.inputs.len(), 1);
        assert_eq!(persona.inputs[0].name, "review_file");
        assert!(persona.inputs[0].required);
        assert_eq!(persona.outputs.len(), 1);
        assert_eq!(persona.outputs[0].name, "summary_file");
        assert!(!persona.outputs[0].required);
        assert_eq!(
            persona.description.as_deref(),
            Some("A concise writing persona.")
        );
    }

    #[test]
    fn persona_io_field_default_io_type_is_file() {
        let json = r#"{"name": "test", "description": "a test field"}"#;
        let field: PersonaIOField = serde_json::from_str(json).unwrap();
        assert_eq!(field.io_type, "file");
        assert!(!field.required);
    }

    #[test]
    fn render_io_summary_uses_explicit_description() {
        let persona = SubagentPersona {
            description: Some("A focused code reviewer.".to_owned()),
            instructions: Some("Ignore this line.\nAnd this one.".to_owned()),
            ..Default::default()
        };
        let summary = persona.render_io_summary("reviewer");
        assert!(summary.contains("A focused code reviewer."));
        assert!(!summary.contains("Ignore this line"));
    }

    #[test]
    fn render_io_summary_extracts_first_paragraph_from_instructions() {
        let persona = SubagentPersona {
            instructions: Some(
                "You are a meticulous code reviewer. Review code and produce structured review\n\
                 notes in a Markdown file at the path given in the prompt.\n\n\
                 Process:\n1. Read the code."
                    .to_owned(),
            ),
            ..Default::default()
        };
        let summary = persona.render_io_summary("reviewer");
        assert!(
            summary.contains("You are a meticulous code reviewer. Review code and produce structured review notes in a Markdown file at the path given in the prompt."),
            "should join multi-line first paragraph: {summary}"
        );
        assert!(!summary.contains("Process"));
    }

    #[test]
    fn render_io_summary_falls_back_to_custom_persona() {
        let persona = SubagentPersona::default();
        let summary = persona.render_io_summary("empty");
        assert!(summary.contains("Custom persona"));
    }

    #[test]
    fn render_io_summary_extracts_lead_paragraph_before_list() {
        let persona = SubagentPersona {
            instructions: Some(
                "You are a thorough researcher. When exploring a question:\n\
                 - Exhaust all reasonable search avenues before concluding\n\
                 - Always cite specific file paths"
                    .to_owned(),
            ),
            ..Default::default()
        };
        let summary = persona.render_io_summary("researcher");
        assert!(summary.contains("You are a thorough researcher. When exploring a question:"));
        assert!(!summary.contains("Always cite specific file paths"));
    }

    #[test]
    fn render_io_summary_headings_only_instructions_falls_back() {
        let persona = SubagentPersona {
            instructions: Some("# Heading\n## Sub".to_owned()),
            ..Default::default()
        };
        let summary = persona.render_io_summary("test");
        assert!(summary.contains("Custom persona"));
    }

    #[test]
    fn render_io_summary_empty_description_falls_through_to_instructions() {
        let persona = SubagentPersona {
            description: Some("".to_owned()),
            instructions: Some("Actual description here.".to_owned()),
            ..Default::default()
        };
        let summary = persona.render_io_summary("test");
        assert!(summary.contains("Actual description here."));
    }

    #[test]
    fn render_io_summary_whitespace_description_falls_through_to_instructions() {
        let persona = SubagentPersona {
            description: Some("   ".to_owned()),
            instructions: Some("Real content.".to_owned()),
            ..Default::default()
        };
        let summary = persona.render_io_summary("test");
        assert!(summary.contains("Real content."));
    }
}
