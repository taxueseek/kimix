//! Chat Completions request dialects.
//!
//! Transport (`ApiBackend::ChatCompletions`) is not the same as provider
//! wire shape. Kimi/Moonshot need thinking-field remaps and schema fills;
//! pure OpenAI-compatible OSS endpoints (DeepSeek, Qwen, vLLM, …) must not
//! receive those rewrites.
//!
//! All request-side chat/completions adaptations go through
//! [`adapt_chat_completions_body`] — never as scattered special-cases.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire dialect for `/chat/completions` request bodies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionsDialect {
    /// Kimi / Moonshot dialect: `reasoning_effort` → `thinking`, empty
    /// assistant content strip, tool-schema `type` fill, `prompt_cache_key`.
    #[default]
    Kimi,
    /// Vanilla OpenAI-compatible: strip kimix-only message fields and stamp
    /// `prompt_cache_key` only. Does **not** rewrite thinking / schemas.
    OpenAiCompat,
}

impl ChatCompletionsDialect {
    /// Infer dialect from the inference base URL.
    ///
    /// First-party Kimi/Moonshot hosts → [`Self::Kimi`]; everything else
    /// (custom OSS gateways, DeepSeek, local vLLM, …) → [`Self::OpenAiCompat`].
    pub fn infer_from_base_url(base_url: &str) -> Self {
        if is_first_party_kimi_base(base_url) {
            Self::Kimi
        } else {
            Self::OpenAiCompat
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kimi => "kimi",
            Self::OpenAiCompat => "openai_compat",
        }
    }
}

/// Hosts that speak the Kimi/Moonshot chat-completions dialect.
fn is_first_party_kimi_base(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    // Strip scheme for host matching when possible
    let host = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(lower.as_str());
    let host = host.split('/').next().unwrap_or(host);
    // Drop port
    let host = host.split(':').next().unwrap_or(host);

    host == "api.kimi.com"
        || host.ends_with(".kimi.com")
        || host == "api.moonshot.cn"
        || host.ends_with(".moonshot.cn")
        || host == "api.moonshot.ai"
        || host.ends_with(".moonshot.ai")
        || host == "api.moonshot.com"
        || host.ends_with(".moonshot.com")
}

/// Adapt a fully-serialized chat/completions body for `dialect`.
///
/// Single adaptation point applied by [`crate::SamplingClient`] on both
/// streaming and non-streaming chat/completions paths.
pub fn adapt_chat_completions_body(
    dialect: ChatCompletionsDialect,
    body: &mut Value,
    session_id: Option<&str>,
) {
    match dialect {
        ChatCompletionsDialect::Kimi => {
            crate::kimi_compat::adapt_chat_completions_body(body, session_id);
        }
        ChatCompletionsDialect::OpenAiCompat => {
            adapt_openai_compat(body, session_id);
        }
    }
}

/// Safe universal fixes for pure OpenAI-compatible endpoints.
fn adapt_openai_compat(body: &mut Value, session_id: Option<&str>) {
    strip_model_id_from_messages(body);
    if let Some(sid) = session_id {
        let key = &sid[..sid.len().min(64)];
        body["prompt_cache_key"] = Value::String(key.to_string());
    }
}

/// Drop kimix-extension `model_id` from message objects (not in OpenAI wire).
fn strip_model_id_from_messages(body: &mut Value) {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        if let Some(obj) = message.as_object_mut() {
            obj.remove("model_id");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infer_first_party_kimi() {
        assert_eq!(
            ChatCompletionsDialect::infer_from_base_url("https://api.kimi.com/coding/v1"),
            ChatCompletionsDialect::Kimi
        );
        assert_eq!(
            ChatCompletionsDialect::infer_from_base_url("https://api.moonshot.cn/v1"),
            ChatCompletionsDialect::Kimi
        );
        assert_eq!(
            ChatCompletionsDialect::infer_from_base_url("https://api.moonshot.ai/v1"),
            ChatCompletionsDialect::Kimi
        );
    }

    #[test]
    fn infer_oss_openai_compat() {
        assert_eq!(
            ChatCompletionsDialect::infer_from_base_url("https://api.deepseek.com/v1"),
            ChatCompletionsDialect::OpenAiCompat
        );
        assert_eq!(
            ChatCompletionsDialect::infer_from_base_url("http://127.0.0.1:8000/v1"),
            ChatCompletionsDialect::OpenAiCompat
        );
        assert_eq!(
            ChatCompletionsDialect::infer_from_base_url("https://example.test"),
            ChatCompletionsDialect::OpenAiCompat
        );
    }

    #[test]
    fn openai_compat_does_not_rewrite_thinking() {
        let mut body = json!({
            "model": "deepseek-chat",
            "reasoning_effort": "high",
            "messages": [{ "role": "user", "content": "hi", "model_id": "x" }]
        });
        adapt_chat_completions_body(
            ChatCompletionsDialect::OpenAiCompat,
            &mut body,
            Some("session-abc"),
        );
        assert_eq!(body.get("reasoning_effort").and_then(|v| v.as_str()), Some("high"));
        assert!(body.get("thinking").is_none());
        assert_eq!(body["prompt_cache_key"], "session-abc");
        assert!(body["messages"][0].get("model_id").is_none());
    }

    #[test]
    fn kimi_dialect_still_rewrites_thinking() {
        let mut body = json!({
            "model": "kimi-for-coding",
            "reasoning_effort": "high",
            "messages": []
        });
        adapt_chat_completions_body(ChatCompletionsDialect::Kimi, &mut body, None);
        assert!(body.get("reasoning_effort").is_none());
        assert_eq!(body["thinking"]["type"], "enabled");
    }
}
