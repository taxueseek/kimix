//! Mid-stream / terminal sampling-error triage.
//!
//! Classifies a [`SamplingError`] into one of three **actions** the session
//! loop should take: repair conversation state, retry the request, or surface
//! to the user. Keeps the decision pure and testable — no I/O, no retries.

use crate::error::SamplingError;

/// What the turn loop should do with a sampling failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorAction {
    /// Heal conversation pairs (dangling tool calls / orphans) then retry once.
    ///
    /// Used when the provider rejects malformed history that heal can fix.
    Repair,
    /// Transient failure — retry within the existing transport budget.
    Retry,
    /// Terminal for this turn — show the error to the user (no auto-retry).
    SurfaceToUser,
}

impl StreamErrorAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repair => "repair",
            Self::Retry => "retry",
            Self::SurfaceToUser => "surface_to_user",
        }
    }
}

/// Context hints the triage cannot recover from the error alone.
#[derive(Debug, Clone, Copy, Default)]
pub struct TriageContext {
    /// Conversation currently has unanswered tool calls (pre-heal check).
    pub has_dangling_tool_calls: bool,
    /// Provider message looks like a tool-pair / tool_use contract violation.
    pub tool_pair_violation: bool,
}

impl TriageContext {
    pub fn fresh() -> Self {
        Self::default()
    }

    pub fn with_dangling(mut self, v: bool) -> Self {
        self.has_dangling_tool_calls = v;
        self
    }

    pub fn with_tool_pair(mut self, v: bool) -> Self {
        self.tool_pair_violation = v;
        self
    }
}

/// Detect tool-pair contract language in an API error message.
///
/// Providers phrase this differently; match the common substrings without
/// claiming a full taxonomy.
pub fn looks_like_tool_pair_violation(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("tool_use")
        || m.contains("tool_call")
        || m.contains("tool call")
        || m.contains("tool output")
        || m.contains("function call")
        || m.contains("tool_result")
        || m.contains("tool result")
        || m.contains("each tool_use must have a single result")
        || m.contains("no tool output found")
}

/// Classify a sampling error into a session action.
///
/// Priority (first match wins):
/// 1. Auth / config / encrypted-content / max-tokens → Surface
/// 2. Tool-pair violation (or dangling + 4xx) → Repair
/// 3. `is_retryable()` → Retry
/// 4. Else → Surface
pub fn triage_sampling_error(err: &SamplingError, ctx: TriageContext) -> StreamErrorAction {
    // Hard terminals — never auto-retry or heal-loop.
    if err.is_auth_error() {
        return StreamErrorAction::SurfaceToUser;
    }
    if matches!(err, SamplingError::InvalidConfiguration(_)) {
        return StreamErrorAction::SurfaceToUser;
    }
    if err.is_encrypted_content_error() {
        return StreamErrorAction::SurfaceToUser;
    }
    if matches!(err, SamplingError::MaxTokensTruncation) {
        return StreamErrorAction::SurfaceToUser;
    }
    if matches!(err, SamplingError::IdleTimeout { .. }) {
        return StreamErrorAction::SurfaceToUser;
    }
    if matches!(err, SamplingError::Serialization(_)) {
        return StreamErrorAction::SurfaceToUser;
    }

    // Quota is not retryable transport — user must top up / wait.
    if err.is_quota_exceeded() {
        return StreamErrorAction::SurfaceToUser;
    }

    let pair_hint = ctx.tool_pair_violation
        || err
            .api_message()
            .map(looks_like_tool_pair_violation)
            .unwrap_or(false);

    if pair_hint || (ctx.has_dangling_tool_calls && is_client_content_4xx(err)) {
        return StreamErrorAction::Repair;
    }

    if err.is_retryable() {
        return StreamErrorAction::Retry;
    }

    StreamErrorAction::SurfaceToUser
}

/// Facts-only triage for the serializable [`crate`]-adjacent error mirror
/// (`SamplingErrorInfo` in kimix-sampler). Keeps session code free of
/// reconstructing a full [`SamplingError`].
///
/// Priority matches [`triage_sampling_error`].
pub fn triage_error_facts(
    kind: &str,
    status_code: Option<u16>,
    message: &str,
    is_retryable: bool,
    is_quota_exceeded: bool,
    ctx: TriageContext,
) -> StreamErrorAction {
    match kind {
        "auth" | "serialization" | "idle_timeout" | "max_tokens_truncation" => {
            return StreamErrorAction::SurfaceToUser;
        }
        "rate_limited" => return StreamErrorAction::Retry,
        _ => {}
    }
    if is_quota_exceeded {
        return StreamErrorAction::SurfaceToUser;
    }
    if message.to_ascii_lowercase().contains("encrypted_content") {
        return StreamErrorAction::SurfaceToUser;
    }

    let pair_hint = ctx.tool_pair_violation || looks_like_tool_pair_violation(message);
    let is_4xx = matches!(status_code, Some(400) | Some(422));
    if pair_hint || (ctx.has_dangling_tool_calls && is_4xx) {
        return StreamErrorAction::Repair;
    }

    if is_retryable {
        return StreamErrorAction::Retry;
    }

    StreamErrorAction::SurfaceToUser
}

fn is_client_content_4xx(err: &SamplingError) -> bool {
    matches!(
        err,
        SamplingError::Api { status, .. }
            if {
                let c = status.as_u16();
                c == 400 || c == 422
            }
    )
}

/// Extension: extract API message when present.
trait ApiMessage {
    fn api_message(&self) -> Option<&str>;
}

impl ApiMessage for SamplingError {
    fn api_message(&self) -> Option<&str> {
        match self {
            SamplingError::Api { message, .. } => Some(message.as_str()),
            SamplingError::StreamError { message, .. } => Some(message.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    fn api(status: u16, message: &str) -> SamplingError {
        SamplingError::Api {
            status: StatusCode::from_u16(status).unwrap(),
            message: message.into(),
            model_metadata: None,
            retry_after_secs: None,
            should_retry: None,
        }
    }

    #[test]
    fn auth_and_quota_surface() {
        let err = SamplingError::Auth("bad key".into());
        assert_eq!(
            triage_sampling_error(&err, TriageContext::fresh()),
            StreamErrorAction::SurfaceToUser
        );
        let err = api(403, "usage limit exceeded for billing cycle");
        assert_eq!(
            triage_sampling_error(&err, TriageContext::fresh()),
            StreamErrorAction::SurfaceToUser
        );
    }

    #[test]
    fn tool_pair_message_is_repair() {
        let err = api(400, "No tool output found for function call call_abc");
        assert_eq!(
            triage_sampling_error(&err, TriageContext::fresh()),
            StreamErrorAction::Repair
        );
    }

    #[test]
    fn dangling_plus_400_is_repair() {
        let err = api(400, "invalid request");
        assert_eq!(
            triage_sampling_error(&err, TriageContext::fresh().with_dangling(true)),
            StreamErrorAction::Repair
        );
    }

    #[test]
    fn rate_limit_retries() {
        let err = api(429, "slow down");
        assert_eq!(
            triage_sampling_error(&err, TriageContext::fresh()),
            StreamErrorAction::Retry
        );
    }

    #[test]
    fn empty_response_retries() {
        use crate::error::{EmptyReason, EmptyResponseContext};
        let err = SamplingError::EmptyResponse {
            context: EmptyResponseContext {
                reason: EmptyReason::NoVisibleContent,
                had_reasoning: false,
                content_len: 0,
                tool_call_count: 0,
                finish_reason: Some("stop".into()),
                completion_tokens: None,
                reasoning_tokens: None,
                prompt_tokens: None,
                model: "test".into(),
                first_choice_seen: true,
            },
        };
        assert_eq!(
            triage_sampling_error(&err, TriageContext::fresh()),
            StreamErrorAction::Retry
        );
    }

    #[test]
    fn looks_like_pair_heuristic() {
        assert!(looks_like_tool_pair_violation(
            "each tool_use must have a single result"
        ));
        assert!(!looks_like_tool_pair_violation("context length exceeded"));
    }

    #[test]
    fn triage_error_facts_matches_sampling_error() {
        assert_eq!(
            triage_error_facts(
                "api",
                Some(400),
                "No tool output found for function call x",
                false,
                false,
                TriageContext::fresh(),
            ),
            StreamErrorAction::Repair
        );
        assert_eq!(
            triage_error_facts("auth", None, "bad key", false, false, TriageContext::fresh()),
            StreamErrorAction::SurfaceToUser
        );
        assert_eq!(
            triage_error_facts(
                "http",
                Some(503),
                "upstream",
                true,
                false,
                TriageContext::fresh(),
            ),
            StreamErrorAction::Retry
        );
    }
}
