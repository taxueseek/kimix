//! Model-category classification — the gating foundation for open-weight
//! optimizations.
//!
//! A harness tuned for one model class under-performs on another.
//! [`ModelCategory::classify`] splits models into [`ModelCategory::Premium`]
//! (Claude/GPT/Grok flagships) and [`ModelCategory::OpenSource`]
//! (DeepSeek/Kimi-Moonshot/Qwen/GLM/Llama/Mistral/ollama/local/openai-compatible
//! non-flagship). The result gates prompt sections (`<open_model_discipline>`)
//! and request-shaping (`tool_choice: "none"` on tool-free turns) so
//! open-weight failure modes are compensated only where they apply, leaving
//! premium models on their original path.
//!
//! ## Override
//! Set `KIMIX_MODEL_CATEGORY=premium|opensource` to force a category
//! regardless of detection (manual escape hatch). A per-model
//! `[model.*] category` config key is the intended long-form override and is
//! deferred — detection + env cover the common cases today.

/// Coarse model class. See the module docs for the gating rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModelCategory {
    /// Claude / GPT-4/5 / o-series / Grok flagships — leave optimizations off.
    Premium,
    /// DeepSeek / Kimi-Moonshot / Qwen / GLM / Llama / Mistral / ollama /
    /// local / openai-compatible non-flagship — apply open-weight compensations.
    OpenSource,
}

impl ModelCategory {
    /// `true` for [`ModelCategory::OpenSource`].
    pub fn is_open_source(self) -> bool {
        matches!(self, ModelCategory::OpenSource)
    }

    /// Classify a model from its model id and API base URL.
    ///
    /// Precedence: `KIMIX_MODEL_CATEGORY` env override → explicit premium
    /// signatures → explicit open-source signatures → default [`Premium`]
    /// (don't change a premium model's natural concise style for an unknown).
    pub fn classify(model: &str, base_url: &str) -> ModelCategory {
        Self::classify_inner(model, base_url, env_override())
    }

    /// Pure classification core: same precedence as [`classify`], but the
    /// forced override is passed in rather than read from the process env.
    /// Lets tests exercise the rule table without mutating (or racing on)
    /// the real environment — `std::env::set_var` is `unsafe` as of Rust
    /// 1.97 precisely because env mutation is not thread-safe across the
    /// parallel test runner, so we avoid it entirely.
    fn classify_inner(
        model: &str,
        base_url: &str,
        forced: Option<ModelCategory>,
    ) -> ModelCategory {
        if let Some(c) = forced {
            return c;
        }
        let m = model.to_ascii_lowercase();
        let b = base_url.to_ascii_lowercase();
        let hay = format!("{m} {b}");

        // Explicit premium flagships. Matched first so an openai-compatible
        // endpoint that happens to serve `gpt-4o` stays premium.
        const PREMITS: &[&str] = &[
            "claude", "gpt-4", "gpt-5", "o1-", "o3-", "o4-", "grok", "openai.com",
            "anthropic.com", "x.ai",
        ];
        if PREMITS.iter().any(|sig| hay.contains(sig)) {
            return ModelCategory::Premium;
        }

        // Explicit open-weight / local-host signatures.
        const OPENSIGS: &[&str] = &[
            "deepseek", "kimi", "moonshot", "qwen", "glm", "llama", "mistral",
            "yi-", "gemma", "ollama", "lmstudio", "lm-studio", "vllm", "baseten",
            "localhost", "127.0.0.1", "0.0.0.0", "cai",
        ];
        if OPENSIGS.iter().any(|sig| hay.contains(sig)) {
            return ModelCategory::OpenSource;
        }

        ModelCategory::Premium
    }
}

/// Read the `KIMIX_MODEL_CATEGORY` env override. Returns `None` for unset or
/// unrecognized values (falls through to the rule table).
fn env_override() -> Option<ModelCategory> {
    match std::env::var("KIMIX_MODEL_CATEGORY")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "opensource" | "open" | "oss" => Some(ModelCategory::OpenSource),
            "premium" => Some(ModelCategory::Premium),
            _ => None,
        },
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premium_flagships() {
        for (m, b) in [
            ("claude-sonnet-4-5", "https://api.anthropic.com"),
            ("gpt-4o", "https://api.openai.com"),
            ("gpt-5", "https://api.openai.com"),
            ("o3-mini", "https://api.openai.com"),
            ("grok-4", "https://api.x.ai"),
        ] {
            assert_eq!(
                ModelCategory::classify_inner(m, b, None),
                ModelCategory::Premium,
                "{m}/{b}"
            );
        }
    }

    #[test]
    fn open_source_signatures() {
        for (m, b) in [
            ("deepseek-chat", "https://api.deepseek.com"),
            ("kimi-k2", "https://api.moonshot.cn"),
            ("qwen-max", "https://dashscope.aliyuncs.com"),
            ("glm-4.6", "https://open.bigmodel.cn"),
            ("llama-3.1-70b", "http://localhost:11434"),
            ("mistral-large", "https://api.baseten.co"),
        ] {
            assert_eq!(
                ModelCategory::classify_inner(m, b, None),
                ModelCategory::OpenSource,
                "{m}/{b}"
            );
        }
    }

    #[test]
    fn local_host_is_open() {
        assert_eq!(
            ModelCategory::classify_inner("anything", "http://localhost:8080", None),
            ModelCategory::OpenSource
        );
        assert_eq!(
            ModelCategory::classify_inner("anything", "http://127.0.0.1:1234", None),
            ModelCategory::OpenSource
        );
    }

    #[test]
    fn unknown_defaults_premium() {
        // Unknown model on a generic endpoint → don't add open-model preamble.
        assert_eq!(
            ModelCategory::classify_inner("my-custom-model", "https://inference.example.com", None),
            ModelCategory::Premium
        );
    }

    #[test]
    fn openai_compatible_serving_open_model_is_open() {
        // An openai-compatible endpoint serving an open model → open.
        assert_eq!(
            ModelCategory::classify_inner("deepseek-v3", "https://gateway.example.com/v1", None),
            ModelCategory::OpenSource,
        );
    }

    #[test]
    fn forced_override_wins_over_rules() {
        // A forced override short-circuits the rule table, both directions.
        assert_eq!(
            ModelCategory::classify_inner("gpt-4o", "https://api.openai.com", Some(ModelCategory::OpenSource)),
            ModelCategory::OpenSource,
        );
        assert_eq!(
            ModelCategory::classify_inner("deepseek-chat", "https://api.deepseek.com", Some(ModelCategory::Premium)),
            ModelCategory::Premium,
        );
    }

    #[test]
    fn env_override_parses_aliases() {
        // The env parser accepts premium / opensource / open / oss and rejects junk.
        assert_eq!(parse_override_str("opensource"), Some(ModelCategory::OpenSource));
        assert_eq!(parse_override_str("open"), Some(ModelCategory::OpenSource));
        assert_eq!(parse_override_str("oss"), Some(ModelCategory::OpenSource));
        assert_eq!(parse_override_str("premium"), Some(ModelCategory::Premium));
        assert_eq!(parse_override_str("  Premium  "), Some(ModelCategory::Premium));
        assert_eq!(parse_override_str("nope"), None);
        assert_eq!(parse_override_str(""), None);
    }

    /// Mirror of the `env_override` match arm, factored out so the parser can
    /// be tested without touching the real environment.
    fn parse_override_str(v: &str) -> Option<ModelCategory> {
        match v.trim().to_ascii_lowercase().as_str() {
            "opensource" | "open" | "oss" => Some(ModelCategory::OpenSource),
            "premium" => Some(ModelCategory::Premium),
            _ => None,
        }
    }

    #[test]
    fn is_open_source_helper() {
        assert!(!ModelCategory::Premium.is_open_source());
        assert!(ModelCategory::OpenSource.is_open_source());
    }
}
