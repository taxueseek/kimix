//! Light capability map for tools / parallel tool calls / thinking.
//!
//! Complements [`crate::ModelCapability`] (media + thinking flags from the
//! wire catalog) with **runtime feature** hints used when talking to OSS
//! OpenAI-compatible endpoints that do not speak the full Kimi `/models`
//! contract. Pure heuristics — no network.

use crate::ModelCapability;

/// Coarse runtime features a model endpoint is expected to support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModelFeatureMap {
    /// Function / tool calling.
    pub tools: bool,
    /// Multiple tool calls in one assistant turn.
    pub parallel_tool_calls: bool,
    /// Reasoning / thinking channel (toggleable or always-on).
    pub thinking: bool,
    /// Always-on thinking (cannot disable).
    pub always_thinking: bool,
}

impl ModelFeatureMap {
    /// Conservative defaults for an unknown OpenAI-compatible OSS model:
    /// tools yes, parallel yes, thinking no.
    pub fn openai_compat_default() -> Self {
        Self {
            tools: true,
            parallel_tool_calls: true,
            thinking: false,
            always_thinking: false,
        }
    }

    /// Kimi / Moonshot first-party defaults (tools + parallel; thinking from caps).
    pub fn kimi_default() -> Self {
        Self {
            tools: true,
            parallel_tool_calls: true,
            thinking: true,
            always_thinking: false,
        }
    }
}

/// Derive a feature map from model id + optional pre-derived capabilities.
///
/// `capabilities` should be [`crate::derive_capabilities`] / wire-derived when
/// available; pass `&[]` for pure id heuristics.
pub fn feature_map_for_model(model_id: &str, capabilities: &[ModelCapability]) -> ModelFeatureMap {
    let id = model_id.to_ascii_lowercase();
    let mut map = if looks_like_kimi_family(&id) {
        ModelFeatureMap::kimi_default()
    } else {
        ModelFeatureMap::openai_compat_default()
    };

    // Wire / derived capabilities win for thinking flags.
    if capabilities
        .iter()
        .any(|c| matches!(c, ModelCapability::Thinking | ModelCapability::AlwaysThinking))
    {
        map.thinking = true;
    }
    if capabilities
        .iter()
        .any(|c| matches!(c, ModelCapability::AlwaysThinking))
    {
        map.always_thinking = true;
        map.thinking = true;
    }

    // Id heuristics when capabilities are empty.
    if capabilities.is_empty() {
        if id.contains("thinking") || id.contains("reasoner") || id.contains("r1") {
            map.thinking = true;
        }
        if id.contains("thinking") {
            map.always_thinking = true;
        }
        // Known weak tool-call models — still advertise tools (repair handles shape).
        if id.contains("instruct") && !id.contains("tool") {
            map.parallel_tool_calls = false;
        }
    }

    // Models that famously lack tools.
    if id.contains("embed") || id.contains("tts") || id.contains("whisper") {
        map.tools = false;
        map.parallel_tool_calls = false;
        map.thinking = false;
        map.always_thinking = false;
    }

    map
}

fn looks_like_kimi_family(id: &str) -> bool {
    id.starts_with("kimi")
        || id.starts_with("moonshot")
        || id.contains("kimi-for-coding")
        || id.starts_with("k2")
        || id.starts_with("k3")
}

/// Whether the session should enable thinking by default for this feature map.
pub fn feature_default_thinking(map: &ModelFeatureMap) -> bool {
    map.thinking || map.always_thinking
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oss_default_has_tools_no_thinking() {
        let m = feature_map_for_model("deepseek-v4-flash", &[]);
        assert!(m.tools);
        assert!(m.parallel_tool_calls);
        assert!(!m.thinking);
    }

    #[test]
    fn kimi_thinking_id() {
        let m = feature_map_for_model("kimi-k2-thinking-turbo", &[]);
        assert!(m.tools);
        assert!(m.thinking);
        assert!(m.always_thinking);
    }

    #[test]
    fn capabilities_override_id() {
        let caps = [ModelCapability::Thinking];
        let m = feature_map_for_model("custom-oss-7b", &caps);
        assert!(m.thinking);
        assert!(!m.always_thinking);
    }

    #[test]
    fn embed_has_no_tools() {
        let m = feature_map_for_model("text-embedding-3-small", &[]);
        assert!(!m.tools);
    }
}
