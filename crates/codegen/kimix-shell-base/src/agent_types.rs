//! Pure auth types extracted from `kimix-shell/src/agent/auth_method.rs`
//! so that downstream crates (kimix-tui, kimix-headless) can depend on these
//! types without rebuilding when shell logic changes.

use agent_client_protocol as acp;

// ── Auth method id constants ──────────────────────────────────────────────

pub const XAI_API_KEY_METHOD_ID: &str = "xai.api_key";
pub const KIMI_CODE_METHOD_ID: &str = "kimi-code";
pub const CACHED_TOKEN_AUTH_METHOD_ID: &str = "cached_token";
pub const MOONSHOT_CN_METHOD_ID: &str = "moonshot-cn";
pub const MOONSHOT_AI_METHOD_ID: &str = "moonshot-ai";
pub const GROK_SESSION_METHOD_ID: &str = "grok-session";
pub const XAI_SESSION_METHOD_ID: &str = "xai-session";

pub const AUTH_ERROR_SESSION_EXPIRED: &str =
    "Session expired. Run `kimix login` to re-authenticate.";

pub const AUTH_ERROR_API_KEY: &str = "Authentication failed. Run `kimix login`, set XAI_API_KEY, or add api_key to ~/.kimix/config.toml.";

// ── Auth method kind ──────────────────────────────────────────────────────

/// ACP session auth method. Use `is_session_based_method` for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethodKind {
    XaiApiKey,
    CachedToken,
    KimiCode,
    /// Moonshot Open Platform API-key login (moonshot.cn).
    MoonshotCn,
    /// Moonshot Open Platform API-key login (moonshot.ai).
    MoonshotAi,
    /// Native xAI OIDC session (`kimix login --xai`).
    XaiSession,
    /// Grok CLI session bridge (legacy / fallback).
    GrokSession,
    Unknown,
}

impl AuthMethodKind {
    pub fn from_id(id: &acp::AuthMethodId) -> Self {
        match id.0.as_ref() {
            XAI_API_KEY_METHOD_ID => Self::XaiApiKey,
            CACHED_TOKEN_AUTH_METHOD_ID => Self::CachedToken,
            KIMI_CODE_METHOD_ID => Self::KimiCode,
            XAI_SESSION_METHOD_ID => Self::XaiSession,
            MOONSHOT_CN_METHOD_ID => Self::MoonshotCn,
            MOONSHOT_AI_METHOD_ID => Self::MoonshotAi,
            GROK_SESSION_METHOD_ID => Self::GrokSession,
            _ => Self::Unknown,
        }
    }

    /// API key auth: no auth.json session, no refresh, no browser round-trip.
    /// The moonshot methods qualify — they validate a configured platform key
    /// and then behave exactly like an external-API-key session.
    pub fn is_api_key(self) -> bool {
        matches!(self, Self::XaiApiKey | Self::MoonshotCn | Self::MoonshotAi)
    }

    /// `true` for session-based methods (cached_token, interactive login, xAI/Grok session).
    pub fn is_session_based(self) -> bool {
        matches!(
            self,
            Self::CachedToken | Self::KimiCode | Self::XaiSession | Self::GrokSession
        )
    }

    /// Requires user interaction (device-code login in the browser).
    pub fn needs_interactive_login(self) -> bool {
        matches!(self, Self::KimiCode | Self::XaiSession)
    }

    pub fn auth_error_message(self) -> &'static str {
        if self.is_session_based() {
            AUTH_ERROR_SESSION_EXPIRED
        } else {
            AUTH_ERROR_API_KEY
        }
    }
}

/// `true` for session-based ACP methods (cached_token, interactive login).
pub fn is_session_based_method(method_id: &acp::AuthMethodId) -> bool {
    AuthMethodKind::from_id(method_id).is_session_based()
}

// ── Auth methods build inputs ─────────────────────────────────────────────

/// Pre-computed booleans for [`build_auth_methods`]. Caller computes these
/// from async side effects (token refresh) and shared mutable state
/// (`AuthManager`). The list-construction logic itself is pure.
pub struct AuthMethodsBuildInputs<'a> {
    /// True if `xai.api_key` should be advertised AT ALL. Caller computes via
    /// [`should_advertise_xai_api_key`].
    pub has_external_api_key: bool,
    /// True if a cached session token is available (either present at startup
    /// or recovered via silent refresh).
    pub has_cached_token: bool,
    /// Optional display label for the interactive login method.
    pub login_label: Option<&'a str>,
    /// True if Grok CLI is installed (so we can offer the Grok session bridge).
    pub has_grok_cli: bool,
}
