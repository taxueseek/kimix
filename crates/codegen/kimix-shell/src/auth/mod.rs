pub(crate) mod attribution;
mod config;
pub mod credential_provider;
pub(crate) mod device;
pub mod device_code;
pub mod error;
mod flow;
pub(crate) mod grok_bridge;
pub(crate) mod kimi_oauth;
pub(crate) mod manager;
mod model;
pub(crate) mod recovery;
pub(crate) mod refresh;
mod storage;
pub(crate) mod token_type;
pub(crate) mod xai_oauth;
pub use config::{KIMI_CODE_OAUTH_SCOPE, KimiCodeConfig};
pub(crate) use flow::try_ensure_session_noninteractive;
pub use flow::{
    AuthChannels, AuthUrlInfo, AuthUrlMode, LogoutResult, ensure_authenticated,
    ensure_authenticated_or_noninteractive, perform_logout, run_auth_flow,
    run_auth_flow_with_stderr_bridge, run_cli_login, run_cli_login_xai, run_cli_logout,
    try_ensure_fresh_auth,
};
mod meta;
pub use device::device_headers;
pub use error::{AuthError, RefreshTokenError, RefreshTokenFailedReason};
pub use grok_bridge::{
    AUTH_SOURCE_GROK_SESSION, DEFAULT_CLI_CHAT_PROXY_BASE, inject_grok_session_headers,
    is_cli_chat_proxy_url, is_grok_session_auth_source, load_grok_session_token,
};
pub use manager::{AuthManager, shared_api_key_provider};
pub use meta::AuthMeta;
pub use model::{AuthMode, KimiAuth, lookup_auth};
pub(crate) use model::{TOKEN_TTL, is_expired, token_suffix};
pub use storage::{
    clear_api_key, read_api_key, read_auth_json, read_token_by_scope, store_api_key,
};
pub use xai_oauth::{
    XAI_OIDC_SCOPE, import_grok_token_into_kimix, load_xai_session_token_sync,
    run_xai_device_login, run_xai_device_login_with_channels, store_xai_auth,
};
