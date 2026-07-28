//! Native xAI OIDC device-code login (RFC 8628).
//!
//! Tokens are stored in **Kimix's** `~/.kimix/auth.json` under
//! [`XAI_OIDC_SCOPE`] — not borrowed from `~/.grok/auth.json`.
//!
//! Endpoints (discovered / fixed for `https://auth.x.ai`):
//! - `POST /oauth2/device/code` — device authorization
//! - `POST /oauth2/token` — device_code poll + refresh_token
//!
//! Client id is the public xAI CLI OIDC client used by Grok Build; Kimix
//! runs the same protocol but owns the credential store.

use chrono::{Duration, Utc};
use once_cell::sync::OnceCell;
use serde::Deserialize;

use super::model::{AuthMode, KimiAuth};

/// OAuth 流程共享的 HTTP Client（避免重复加载 TLS 根证书，每次 ~95ms）
static OAUTH_HTTP_CLIENT: OnceCell<reqwest::Client> = OnceCell::new();

fn oauth_http_client() -> &'static reqwest::Client {
    OAUTH_HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build OAuth HTTP client")
    })
}

/// Scope key in `~/.kimix/auth.json` for the native xAI OIDC session.
pub const XAI_OIDC_SCOPE: &str = "oidc/xai";

/// xAI OIDC issuer.
pub const XAI_OIDC_ISSUER: &str = "https://auth.x.ai";

/// Public CLI OIDC client id (xAI CLI / Grok Build family).
pub const XAI_OIDC_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REFRESH_GRANT: &str = "refresh_token";

/// Result of device authorization.
#[derive(Debug, Clone)]
pub struct XaiDeviceAuthorization {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    #[serde(default)]
    verification_uri: Option<String>,
    verification_uri_complete: String,
    expires_in: i64,
    #[serde(default)]
    interval: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
}

impl TokenResponse {
    fn into_auth(self) -> KimiAuth {
        let now = Utc::now();
        let expires_in = self.expires_in.unwrap_or(3600);
        KimiAuth {
            key: self.access_token,
            auth_mode: AuthMode::OAuth,
            create_time: now,
            user_id: String::new(),
            email: None,
            refresh_token: self.refresh_token,
            expires_at: Some(now + Duration::seconds(expires_in)),
            expires_in: Some(expires_in),
            scope: self.scope,
            token_type: self.token_type,
        }
    }
}

#[derive(Deserialize, Default)]
struct OAuthErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Request a device code from auth.x.ai.
pub async fn request_device_authorization() -> anyhow::Result<XaiDeviceAuthorization> {
    let url = format!("{XAI_OIDC_ISSUER}/oauth2/device/code");
    let body = format!(
        "client_id={}&scope={}",
        urlencoding_form(XAI_OIDC_CLIENT_ID),
        urlencoding_form("openid profile email offline_access")
    );
    let client = oauth_http_client();
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("xAI device authorization request failed: {e}"))?;
    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        let err: OAuthErrorBody = serde_json::from_slice(&bytes).unwrap_or_default();
        return Err(anyhow::anyhow!(
            "xAI device authorization HTTP {status}: {} {}",
            err.error.unwrap_or_else(|| "error".into()),
            err.error_description.unwrap_or_default()
        ));
    }
    let parsed: DeviceCodeResponse = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("xAI device authorization parse failed: {e}"))?;
    Ok(XaiDeviceAuthorization {
        user_code: parsed.user_code,
        device_code: parsed.device_code,
        verification_uri: parsed
            .verification_uri
            .unwrap_or_else(|| "https://accounts.x.ai/oauth2/device".into()),
        verification_uri_complete: parsed.verification_uri_complete,
        expires_in: parsed.expires_in,
        interval: parsed.interval.unwrap_or(5).max(1),
    })
}

/// Poll until the user completes device login, or error.
pub async fn poll_device_token(device_code: &str, interval_secs: i64) -> anyhow::Result<KimiAuth> {
    let url = format!("{XAI_OIDC_ISSUER}/oauth2/token");
    let client = oauth_http_client();
    let body = format!(
        "grant_type={}&device_code={}&client_id={}",
        urlencoding_form(DEVICE_GRANT),
        urlencoding_form(device_code),
        urlencoding_form(XAI_OIDC_CLIENT_ID),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1800);
    let mut interval = std::time::Duration::from_secs(interval_secs.max(1) as u64);

    loop {
        if std::time::Instant::now() > deadline {
            return Err(anyhow::anyhow!("xAI device login timed out"));
        }
        tokio::time::sleep(interval).await;

        let resp = client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("xAI token poll failed: {e}"))?;
        let status = resp.status();
        let bytes = resp.bytes().await?;

        if status.is_success() {
            let tok: TokenResponse = serde_json::from_slice(&bytes)
                .map_err(|e| anyhow::anyhow!("xAI token parse failed: {e}"))?;
            return Ok(tok.into_auth());
        }

        let err: OAuthErrorBody = serde_json::from_slice(&bytes).unwrap_or_default();
        match err.error.as_deref() {
            Some("authorization_pending") | Some("slow_down") => {
                if err.error.as_deref() == Some("slow_down") {
                    interval += std::time::Duration::from_secs(5);
                }
                continue;
            }
            Some("expired_token") | Some("access_denied") => {
                return Err(anyhow::anyhow!(
                    "xAI device login {}: {}",
                    err.error.unwrap_or_default(),
                    err.error_description.unwrap_or_default()
                ));
            }
            other => {
                return Err(anyhow::anyhow!(
                    "xAI token poll HTTP {status}: {} {}",
                    other.unwrap_or("error"),
                    err.error_description.unwrap_or_default()
                ));
            }
        }
    }
}

/// Refresh an xAI access token using the stored refresh_token.
pub async fn refresh_xai_token(refresh_token: &str) -> anyhow::Result<KimiAuth> {
    let url = format!("{XAI_OIDC_ISSUER}/oauth2/token");
    let body = format!(
        "grant_type={}&refresh_token={}&client_id={}",
        urlencoding_form(REFRESH_GRANT),
        urlencoding_form(refresh_token),
        urlencoding_form(XAI_OIDC_CLIENT_ID),
    );
    let client = oauth_http_client();
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("xAI refresh failed: {e}"))?;
    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        let err: OAuthErrorBody = serde_json::from_slice(&bytes).unwrap_or_default();
        return Err(anyhow::anyhow!(
            "xAI refresh HTTP {status}: {} {}",
            err.error.unwrap_or_else(|| "error".into()),
            err.error_description.unwrap_or_default()
        ));
    }
    let mut tok: TokenResponse = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("xAI refresh parse failed: {e}"))?;
    // Preserve refresh token if server omits rotation.
    if tok.refresh_token.is_none() {
        tok.refresh_token = Some(refresh_token.to_string());
    }
    Ok(tok.into_auth())
}

/// Full CLI device-code login: print URL, poll, return credential.
pub async fn run_xai_device_login() -> anyhow::Result<KimiAuth> {
    run_xai_device_login_with_channels(None).await
}

/// Device-code login with optional TUI channel for the verification URL.
///
/// When `url_tx` is `Some`, the verification URI is pushed to the TUI (same
/// path as Kimi Code device login) so `/login` → xAI Session shows the URL
/// in-app instead of only printing to stderr.
pub async fn run_xai_device_login_with_channels(
    url_tx: Option<tokio::sync::oneshot::Sender<super::AuthUrlInfo>>,
) -> anyhow::Result<KimiAuth> {
    let auth = request_device_authorization().await?;
    let url = auth.verification_uri_complete.clone();
    let cli_mode = url_tx.is_none();
    if let Some(tx) = url_tx {
        let _ = tx.send(super::AuthUrlInfo {
            url: url.clone(),
            mode: super::AuthUrlMode::Device,
        });
    } else {
        eprintln!("Sign in with xAI (device code)");
        eprintln!();
        eprintln!("  Open:  {url}");
        eprintln!("  Code:  {}", auth.user_code);
        eprintln!();
    }
    // Best-effort open browser (same helper as Kimi device login).
    let open_url = url.clone();
    let _ = tokio::task::spawn_blocking(move || webbrowser::open(&open_url)).await;
    if cli_mode {
        eprintln!("Waiting for authorization…");
    }
    let cred = poll_device_token(&auth.device_code, auth.interval).await?;
    if cli_mode {
        eprintln!("xAI login successful.");
    }
    Ok(cred)
}

/// Load a usable xAI access token from Kimix auth.json (refresh if needed).
///
/// Sync version for credential resolution on the request path.
pub fn load_xai_session_token_sync(auth_file: &std::path::Path) -> Option<String> {
    let store = super::storage::read_auth_json_or_empty(auth_file).ok()?;
    let entry = store.get(XAI_OIDC_SCOPE)?;
    // Still valid?
    if let Some(exp) = entry.expires_at
        && exp > Utc::now() + Duration::seconds(60)
    {
        return Some(entry.key.clone());
    }
    // Try refresh (blocking).
    let refresh = entry.refresh_token.as_deref()?;
    match refresh_xai_token_blocking(refresh) {
        Ok(new_auth) => {
            let key = new_auth.key.clone();
            if let Ok(mut store) = super::storage::read_auth_json_or_empty(auth_file) {
                store.insert(XAI_OIDC_SCOPE.to_string(), new_auth);
                let _ = super::storage::write_auth_json_public(auth_file, &store);
            }
            Some(key)
        }
        Err(e) => {
            tracing::warn!(error = %e, "xAI session refresh failed; run `kimix login --xai`");
            None
        }
    }
}

fn refresh_xai_token_blocking(refresh_token: &str) -> anyhow::Result<KimiAuth> {
    let url = format!("{XAI_OIDC_ISSUER}/oauth2/token");
    let body = format!(
        "grant_type={}&refresh_token={}&client_id={}",
        urlencoding_form(REFRESH_GRANT),
        urlencoding_form(refresh_token),
        urlencoding_form(XAI_OIDC_CLIENT_ID),
    );
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|e| anyhow::anyhow!("xAI refresh failed: {e}"))?;
    let status = resp.status();
    let bytes = resp.bytes().map_err(|e| anyhow::anyhow!("{e}"))?;
    if !status.is_success() {
        let err: OAuthErrorBody = serde_json::from_slice(&bytes).unwrap_or_default();
        return Err(anyhow::anyhow!(
            "xAI refresh HTTP {status}: {} {}",
            err.error.unwrap_or_else(|| "error".into()),
            err.error_description.unwrap_or_default()
        ));
    }
    let mut tok: TokenResponse = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("xAI refresh parse failed: {e}"))?;
    if tok.refresh_token.is_none() {
        tok.refresh_token = Some(refresh_token.to_string());
    }
    Ok(tok.into_auth())
}

/// Persist an xAI credential into `auth.json` under [`XAI_OIDC_SCOPE`].
pub fn store_xai_auth(auth_file: &std::path::Path, auth: KimiAuth) -> std::io::Result<()> {
    let mut store = super::storage::read_auth_json_or_empty(auth_file)?;
    store.insert(XAI_OIDC_SCOPE.to_string(), auth);
    super::storage::write_auth_json_public(auth_file, &store)
}

/// Import a token borrowed from `~/.grok/auth.json` into Kimix's native
/// `oidc/xai` scope so xAI-session models stop depending on the Grok CLI
/// path and 401 recovery can refresh the right authority.
pub fn import_grok_token_into_kimix(
    auth_file: &std::path::Path,
    tok: &super::grok_bridge::GrokSessionToken,
) -> std::io::Result<()> {
    use super::model::AuthMode;
    let now = Utc::now();
    let expires_in = tok
        .expires_at
        .map(|exp| (exp - now).num_seconds().max(0))
        .unwrap_or(3600);
    let auth = KimiAuth {
        key: tok.access_token.clone(),
        auth_mode: AuthMode::OAuth,
        create_time: now,
        user_id: String::new(),
        email: None,
        refresh_token: tok.refresh_token.clone(),
        expires_at: tok.expires_at,
        expires_in: Some(expires_in),
        scope: Some("imported-from-grok".into()),
        token_type: Some("Bearer".into()),
    };
    store_xai_auth(auth_file, auth)
}

fn urlencoding_form(s: &str) -> String {
    // Minimal form encoding for OAuth bodies.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_constant_stable() {
        assert_eq!(XAI_OIDC_SCOPE, "oidc/xai");
    }

    #[test]
    fn form_encode_roundtrip_basic() {
        assert_eq!(urlencoding_form("abc"), "abc");
        assert!(urlencoding_form("a b").contains('+') || urlencoding_form("a b").contains("%20"));
    }
}
