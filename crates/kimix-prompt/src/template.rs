//! Prompt template externalization.
//!
//! Templates are externalized from code so non-developers can tune prompts without
//! recompiling. Supports `{{ variable }}` placeholders (MiniJinja-compatible syntax).
//! Built-in defaults ship in code and can be overridden by files on disk.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A single prompt template.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    /// Template name (e.g. "system", "coding", "review").
    pub name: String,
    /// Applicable scenario (e.g. "general", "coding", "review").
    pub scenario: String,
    /// Language: "zh" or "en".
    pub language: String,
    /// Template content with `{{ var }}` placeholders.
    pub content: String,
}

impl PromptTemplate {
    /// Render the template by replacing `{{ var }}` placeholders with values from `ctx`.
    /// Variables not found in `ctx` are left as-is.
    pub fn render(&self, ctx: &HashMap<String, String>) -> String {
        let mut result = self.content.clone();
        for (key, value) in ctx {
            let placeholder = format!("{{{{ {} }}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }
}

/// Registry of prompt templates, loaded from code defaults and optionally from disk.
pub struct TemplateRegistry {
    templates: HashMap<String, PromptTemplate>,
    /// Directories searched for on-disk template overrides.
    search_paths: Vec<PathBuf>,
}

impl TemplateRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            search_paths: Vec::new(),
        }
    }

    /// Create a registry pre-populated with built-in default templates (Chinese-first).
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register_defaults();
        reg
    }

    /// Add a search path for on-disk template overrides.
    pub fn add_search_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    /// Load templates from a directory. Templates loaded from disk override
    /// any previously registered templates with the same key (name + language).
    ///
    /// Expected file naming: `<name>.<lang>.jinja2` (e.g. `system.zh.jinja2`).
    pub fn load_from_dir(&mut self, path: &Path) -> std::io::Result<usize> {
        let mut count = 0;
        let entries = fs::read_dir(path)?;

        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if !file_path.is_file() {
                continue;
            }

            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Parse: <name>.<lang>.jinja2
            let stem = file_name.strip_suffix(".jinja2").unwrap_or(file_name);
            let parts: Vec<&str> = stem.rsplitn(2, '.').collect();
            if parts.len() != 2 {
                continue;
            }

            let lang = parts[1];
            let name = parts[0];
            let content = fs::read_to_string(&file_path)?;

            let scenario = guess_scenario_from_name(name);

            let template = PromptTemplate {
                name: name.to_string(),
                scenario,
                language: lang.to_string(),
                content,
            };

            let key = template_key(name, lang);
            self.templates.insert(key, template);
            count += 1;
        }

        Ok(count)
    }

    /// Resolve a template by name and language.
    ///
    /// Resolution order:
    /// 1. Exact match (name + language)
    /// 2. Fallback to Chinese ("zh") if the requested language isn't available
    /// 3. Fallback to English ("en") as last resort
    pub fn resolve(&self, name: &str, lang: &str) -> Option<&PromptTemplate> {
        // 1. Exact match
        let key = template_key(name, lang);
        if let Some(t) = self.templates.get(&key) {
            return Some(t);
        }

        // 2. Chinese fallback
        if lang != "zh" {
            let zh_key = template_key(name, "zh");
            if let Some(t) = self.templates.get(&zh_key) {
                return Some(t);
            }
        }

        // 3. English fallback
        if lang != "en" {
            let en_key = template_key(name, "en");
            if let Some(t) = self.templates.get(&en_key) {
                return Some(t);
            }
        }

        None
    }

    /// Render a template with variables.
    ///
    /// Shortcut for `resolve(name, lang)` followed by `render(ctx)`.
    /// Returns `Err` with a message if the template is not found.
    pub fn render(
        &self,
        name: &str,
        lang: &str,
        ctx: &HashMap<String, String>,
    ) -> Result<String, String> {
        let template = self
            .resolve(name, lang)
            .ok_or_else(|| format!("Template not found: {}.{}", name, lang))?;
        Ok(template.render(ctx))
    }

    /// Register a template directly.
    pub fn register(&mut self, template: PromptTemplate) {
        let key = template_key(&template.name, &template.language);
        self.templates.insert(key, template);
    }

    /// Get the count of registered templates.
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Register the four built-in default templates.
    fn register_defaults(&mut self) {
        // system.zh.jinja2
        self.register(PromptTemplate {
            name: "system".to_string(),
            scenario: "general".to_string(),
            language: "zh".to_string(),
            content: r#"你是 Kimixi，一个中文优先的编程助手。
你的目标是帮助用户高效完成编程任务。

规则：
- 始终使用中文交流，除非用户明确要求使用其他语言。
- 直接输出判断、操作结果和必要说明，不寒暄、不总结、不升华。
- 修改代码前先阅读完整文件。
- 遵循项目现有的代码风格和约定。
- 优先编辑已有文件，除非绝对必要不创建新文件。

可用工具：{{ tools }}

当前工作目录：{{ cwd }}
项目语言：{{ language }}"#
                .to_string(),
        });

        // system.en.jinja2
        self.register(PromptTemplate {
            name: "system".to_string(),
            scenario: "general".to_string(),
            language: "en".to_string(),
            content: r#"You are Kimixi, a programming assistant.
Your goal is to help users complete programming tasks efficiently.

Rules:
- Communicate in English unless the user requests another language.
- Output judgments, results, and necessary explanations directly.
- Read the complete file before modifying code.
- Follow the project's existing code style and conventions.
- Edit existing files when possible; create new files only when necessary.

Available tools: {{ tools }}

Current working directory: {{ cwd }}
Project language: {{ language }}"#
                .to_string(),
        });

        // coding.zh.jinja2
        self.register(PromptTemplate {
            name: "coding".to_string(),
            scenario: "coding".to_string(),
            language: "zh".to_string(),
            content: r#"你正在进行编码任务。

编程指南：
- 遵循项目现有的代码风格、库使用习惯和设计模式。
- 用最小改动解决问题，避免无关重构。
- 无效输入时快速失败并给出清晰的错误信息。
- 修改后运行相关测试或检查。

当前上下文：
- 修改文件：{{ files }}
- 任务描述：{{ task }}
- 相关模块：{{ modules }}"#
                .to_string(),
        });

        // review.zh.jinja2
        self.register(PromptTemplate {
            name: "review".to_string(),
            scenario: "review".to_string(),
            language: "zh".to_string(),
            content: r#"你正在进行代码审查任务。

审查要点：
1. **安全性**：是否存在安全漏洞（SQL注入、XSS、密钥泄露等）。
2. **正确性**：逻辑是否正确，边界情况是否处理。
3. **性能**：是否存在明显的性能问题。
4. **可维护性**：代码是否清晰、是否遵循项目约定。
5. **错误处理**：错误路径是否得到妥善处理。

审查文件：{{ files }}
审查重点：{{ focus }}

请逐文件给出审查意见，标注严重程度（严重/一般/建议）。"#
                .to_string(),
        });
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Build the internal key: "{name}:{lang}".
fn template_key(name: &str, lang: &str) -> String {
    format!("{}:{}", name, lang)
}

/// Guess the scenario from the template name.
///
/// Simple heuristic: "coding" → "coding", "review" → "review",
/// otherwise "general".
fn guess_scenario_from_name(name: &str) -> String {
    match name {
        "coding" => "coding".to_string(),
        "review" => "review".to_string(),
        "research" => "research".to_string(),
        _ => "general".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_registry_has_four_templates() {
        let reg = TemplateRegistry::with_defaults();
        assert_eq!(reg.len(), 4);
    }

    #[test]
    fn test_resolve_system_zh() {
        let reg = TemplateRegistry::with_defaults();
        let t = reg.resolve("system", "zh");
        assert!(t.is_some());
        assert_eq!(t.unwrap().language, "zh");
    }

    #[test]
    fn test_resolve_system_en() {
        let reg = TemplateRegistry::with_defaults();
        let t = reg.resolve("system", "en");
        assert!(t.is_some());
        assert_eq!(t.unwrap().language, "en");
    }

    #[test]
    fn test_resolve_with_fallback_to_zh() {
        let reg = TemplateRegistry::with_defaults();
        // coding only has zh version, request "ja" should fallback to zh
        let t = reg.resolve("coding", "ja");
        assert!(t.is_some());
        assert_eq!(t.unwrap().language, "zh");
    }

    #[test]
    fn test_resolve_missing_template() {
        let reg = TemplateRegistry::with_defaults();
        let t = reg.resolve("nonexistent", "zh");
        assert!(t.is_none());
    }

    #[test]
    fn test_render_template() {
        let reg = TemplateRegistry::with_defaults();
        let mut ctx = HashMap::new();
        ctx.insert("tools".to_string(), "read, write, edit".to_string());
        ctx.insert("cwd".to_string(), "/home/user/project".to_string());
        ctx.insert("language".to_string(), "Rust".to_string());

        let result = reg.render("system", "zh", &ctx);
        assert!(result.is_ok());
        let rendered = result.unwrap();
        assert!(rendered.contains("read, write, edit"));
        assert!(rendered.contains("/home/user/project"));
        assert!(rendered.contains("Rust"));
        assert!(!rendered.contains("{{ tools }}"));
        assert!(!rendered.contains("{{ cwd }}"));
    }

    #[test]
    fn test_render_missing_template() {
        let reg = TemplateRegistry::with_defaults();
        let ctx = HashMap::new();
        let result = reg.render("nonexistent", "zh", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_dir() {
        let mut reg = TemplateRegistry::new();
        // Empty registry, load should fail gracefully on non-existent dir
        let non_existent = Path::new("/tmp/kimix_nonexistent_template_dir_12345");
        let result = reg.load_from_dir(non_existent);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_search_path() {
        let mut reg = TemplateRegistry::new();
        reg.add_search_path(PathBuf::from("/tmp/test_templates"));
        assert_eq!(reg.search_paths.len(), 1);
    }

    #[test]
    fn test_render_with_missing_variables() {
        let reg = TemplateRegistry::with_defaults();
        let ctx = HashMap::new(); // empty context
        let result = reg.render("system", "zh", &ctx);
        assert!(result.is_ok());
        let rendered = result.unwrap();
        // Unresolved placeholders should remain as-is
        assert!(rendered.contains("{{ tools }}"));
    }

    #[test]
    fn test_custom_template_registration() {
        let mut reg = TemplateRegistry::new();
        reg.register(PromptTemplate {
            name: "custom".to_string(),
            scenario: "general".to_string(),
            language: "zh".to_string(),
            content: "Hello {{ name }}!".to_string(),
        });

        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), "World".to_string());
        let result = reg.render("custom", "zh", &ctx);
        assert_eq!(result.unwrap(), "Hello World!");
    }

    #[test]
    fn test_empty_registry() {
        let reg = TemplateRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.resolve("system", "zh").is_none());
    }
}
