//! Auth-related helpers extracted from `acp/mod.rs`.
use agent_client_protocol as acp;
use kimix_shell::agent::auth_method::AuthMethodKind;

/// Construct a `METHOD_NOT_FOUND` error for `WaitForTerminalExit`.
///
/// Both the interactive pager and headless mode reject this ACP method
/// (the adapter falls back to polling). Centralised here so the error
/// code and message format stay in sync.
pub fn wait_for_exit_not_supported(context: &str) -> acp::Error {
    acp::Error::new(
        acp::ErrorCode::MethodNotFound.into(),
        format!("{context} does not handle WaitForTerminalExit"),
    )
}

/// Pick the method id for eager authenticate.
///
/// 1. Agent's `defaultAuthMethodId` when present in the advertised list
/// 2. Legacy: `cached_token` if advertised, else first method
pub fn select_eager_auth_method(
    auth_methods: &[acp::AuthMethod],
    default_auth_method_id: Option<&acp::AuthMethodId>,
) -> Option<acp::AuthMethodId> {
    if let Some(default_id) = default_auth_method_id
        && auth_methods.iter().any(|m| m.id() == default_id)
    {
        return Some(default_id.clone());
    }
    let cached_token_method = auth_methods
        .iter()
        .find(|m| AuthMethodKind::from_id(m.id()) == AuthMethodKind::CachedToken);
    cached_token_method
        .or_else(|| auth_methods.first())
        .map(|m| m.id().clone())
}

/// Parse `defaultAuthMethodId` from agent init meta.
pub fn parse_default_auth_method_id(meta: Option<&acp::Meta>) -> Option<acp::AuthMethodId> {
    meta.and_then(|m| m.get("defaultAuthMethodId"))
        .and_then(|v| v.as_str())
        .map(acp::AuthMethodId::new)
}
