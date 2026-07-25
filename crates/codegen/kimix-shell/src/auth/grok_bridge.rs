//! P1: read-only bridge to an existing Grok Build OIDC session.
//!
//! Kimix does **not** implement xAI login. When a model opts in with
//! `auth_source = "grok_session"`, we reuse `~/.grok/auth.json` (or
//! `KIMIX_GROK_AUTH_FILE`) so inference + hosted search ride the same
//! subscription session the user already established via `grok login`.
//!
//! Scope (deliberately small):
//! - parse Grok's auth.json shape (`auth_mode = "oidc"`, JWT + refresh)
//! - optional in-memory refresh against `{issuer}/oauth2/token`
//! - never touch marketplace / telemetry / first-party grok endpoints in
//!   `kimix-env`
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Default Grok CLI session store (sibling product, user-managed).
pub const DEFAULT_GROK_AUTH_REL: &str = ".grok/auth.json";

/// Env override for the auth.json path (tests + non-standard installs).
pub const GROK_AUTH_FILE_ENV: &str = "KIMIX_GROK_AUTH_FILE";

/// Header Grok's cli-chat-proxy expects for CLI session tokens.
pub const X_XAI_TOKEN_AUTH_HEADER: &str = "X-XAI-Token-Auth";
pub const X_XAI_TOKEN_AUTH_VALUE: &str = "xai-grok-cli";

/// cli-chat-proxy rejects requests without a client version (`426`, version
/// reported as `none`). Match Grok Build's wire headers.
pub const X_GROK_CLIENT_VERSION_HEADER: &str = "x-grok-client-version";
pub const X_GROK_CLIENT_IDENTIFIER_HEADER: &str = "x-grok-client-identifier";
pub const X_GROK_CLIENT_IDENTIFIER_VALUE: &str = "grok-shell";

/// Env override for the version string sent to cli-chat-proxy.
pub const GROK_CLIENT_VERSION_ENV: &str = "KIMIX_GROK_CLIENT_VERSION";

/// Floor known to satisfy current proxy policy (`>= 0.1.202`). Prefer the
/// user's installed `grok --version` when detectable; otherwise this default.
pub const DEFAULT_GROK_CLIENT_VERSION: &str = "0.2.111";

/// Default inference base when a grok_session model leaves base_url empty.
pub const DEFAULT_CLI_CHAT_PROXY_BASE: &str = "https://cli-chat-proxy.grok.com/v1";

/// Config value for [`crate::agent::config::ModelInfo::auth_source`].
pub const AUTH_SOURCE_GROK_SESSION: &str = "grok_session";

/// One usable access token borrowed from Grok's store.
#[derive(Debug, Clone)]
pub struct GrokSessionToken {
    pub access_token: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub oidc_issuer: Option<String>,
    pub oidc_client_id: Option<String>,
    pub refresh_token: Option<String>,
    /// True when we minted a fresh access token via refresh (caller may
    /// want to re-read the file next time; we do not write back by default).
    pub refreshed: bool,
}

#[derive(Debug, Deserialize)]
struct GrokAuthEntry {
    key: String,
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    oidc_issuer: Option<String>,
    #[serde(default)]
    oidc_client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Resolve the auth.json path: env → `~/.grok/auth.json`.
pub fn grok_auth_json_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(GROK_AUTH_FILE_ENV) {
        let pb = PathBuf::from(p);
        if !pb.as_os_str().is_empty() {
            return Some(pb);
        }
    }
    dirs::home_dir().map(|h| h.join(DEFAULT_GROK_AUTH_REL))
}

/// `true` when `auth_source` requests the Grok session bridge.
pub fn is_grok_session_auth_source(auth_source: Option<&str>) -> bool {
    auth_source
        .map(|s| s.eq_ignore_ascii_case(AUTH_SOURCE_GROK_SESSION))
        .unwrap_or(false)
}

/// Heuristic: base URL is Grok's cli-chat-proxy (session tokens apply).
pub fn is_cli_chat_proxy_url(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.contains("cli-chat-proxy.grok.com") || lower.contains("cli-chat-proxy")
}

/// Load a still-usable (or freshly refreshed) access token from Grok auth.json.
///
/// Prefers a non-expired OIDC entry with a key. If all are expired but a
/// refresh_token + client_id + issuer exist, attempts one OIDC refresh.
pub fn load_grok_session_token() -> Option<GrokSessionToken> {
    let path = grok_auth_json_path()?;
    load_grok_session_token_from_path(&path)
}

/// Path-injectable core (unit tests use a temp file).
pub fn load_grok_session_token_from_path(path: &Path) -> Option<GrokSessionToken> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "grok_bridge: auth.json unreadable"
            );
            return None;
        }
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let map: BTreeMap<String, GrokAuthEntry> = match serde_json::from_str(trimmed) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "grok_bridge: failed to parse auth.json"
            );
            return None;
        }
    };
    pick_best_entry(map).and_then(materialize_token)
}

fn pick_best_entry(map: BTreeMap<String, GrokAuthEntry>) -> Option<GrokAuthEntry> {
    let mut candidates: Vec<GrokAuthEntry> = map.into_values().collect();
    if candidates.is_empty() {
        return None;
    }
    // Prefer OIDC entries, then those with a non-empty key.
    candidates.retain(|e| !e.key.trim().is_empty());
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|e| {
        let oidc = e
            .auth_mode
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("oidc"));
        let expired = e.expires_at.is_some_and(|t| t <= Utc::now());
        // lower is better
        (!oidc as u8, expired as u8)
    });
    candidates.into_iter().next()
}

fn materialize_token(entry: GrokAuthEntry) -> Option<GrokSessionToken> {
    let still_valid = entry
        .expires_at
        .map(|t| t > Utc::now() + chrono::Duration::seconds(60))
        .unwrap_or(true);

    if still_valid {
        return Some(GrokSessionToken {
            access_token: entry.key,
            expires_at: entry.expires_at,
            oidc_issuer: entry.oidc_issuer,
            oidc_client_id: entry.oidc_client_id,
            refresh_token: entry.refresh_token,
            refreshed: false,
        });
    }

    // Expired — try refresh once (in-memory only).
    let issuer = entry.oidc_issuer.as_deref()?;
    let client_id = entry.oidc_client_id.as_deref()?;
    let refresh = entry.refresh_token.as_deref()?;
    match refresh_oidc_token(issuer, client_id, refresh) {
        Ok(tok) => {
            tracing::info!("grok_bridge: refreshed expired OIDC access token");
            Some(tok)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "grok_bridge: token expired and refresh failed; run `grok login`"
            );
            None
        }
    }
}

fn refresh_oidc_token(
    issuer: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<GrokSessionToken, String> {
    let token_url = format!("{}/oauth2/token", issuer.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&token_url)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=refresh_token&refresh_token={}&client_id={}",
            urlencoding_form(refresh_token),
            urlencoding_form(client_id),
        ))
        .send()
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", body.chars().take(200).collect::<String>()));
    }
    let parsed: OidcTokenResponse =
        serde_json::from_str(&body).map_err(|e| format!("token JSON: {e}"))?;
    let expires_at = parsed
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));
    Ok(GrokSessionToken {
        access_token: parsed.access_token,
        expires_at,
        oidc_issuer: Some(issuer.to_string()),
        oidc_client_id: Some(client_id.to_string()),
        refresh_token: parsed.refresh_token.or_else(|| Some(refresh_token.to_string())),
        refreshed: true,
    })
}

/// Minimal form-urlencoded for token refresh (no extra crate).
fn urlencoding_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Resolve a client version string acceptable to cli-chat-proxy.
///
/// Order: `KIMIX_GROK_CLIENT_VERSION` → `GROK_CLIENT_VERSION` → probe
/// `grok --version` once → [`DEFAULT_GROK_CLIENT_VERSION`].
pub fn resolve_grok_client_version() -> String {
    for env in [GROK_CLIENT_VERSION_ENV, "GROK_CLIENT_VERSION"] {
        if let Ok(v) = std::env::var(env) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    if let Some(v) = probe_local_grok_version() {
        return v;
    }
    DEFAULT_GROK_CLIENT_VERSION.to_string()
}

/// Best-effort parse of `grok --version` (e.g. `grok 0.2.111 (94172f2aa4e5)`).
fn probe_local_grok_version() -> Option<String> {
    let output = std::process::Command::new("grok")
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_grok_version_line(&text)
}

fn parse_grok_version_line(text: &str) -> Option<String> {
    // Match first semver-like token: 0.2.111 or 0.1.202
    let re = regex::Regex::new(r"\b(\d+\.\d+\.\d+)\b").ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Inject session-token headers required by cli-chat-proxy. Never overwrites
/// caller-provided values.
pub fn inject_grok_session_headers(headers: &mut indexmap::IndexMap<String, String>) {
    headers
        .entry(X_XAI_TOKEN_AUTH_HEADER.to_string())
        .or_insert_with(|| X_XAI_TOKEN_AUTH_VALUE.to_string());
    headers
        .entry(X_GROK_CLIENT_IDENTIFIER_HEADER.to_string())
        .or_insert_with(|| X_GROK_CLIENT_IDENTIFIER_VALUE.to_string());
    headers
        .entry(X_GROK_CLIENT_VERSION_HEADER.to_string())
        .or_insert_with(resolve_grok_client_version);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_auth(dir: &tempfile::TempDir, json: &str) -> PathBuf {
        let p = dir.path().join("auth.json");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        p
    }

    #[test]
    fn loads_valid_oidc_entry() {
        let dir = tempfile::tempdir().unwrap();
        let exp = (Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        let path = write_auth(
            &dir,
            &format!(
                r#"{{
                  "https://auth.x.ai::cid": {{
                    "key": "jwt-access-token",
                    "auth_mode": "oidc",
                    "expires_at": "{exp}",
                    "oidc_issuer": "https://auth.x.ai",
                    "oidc_client_id": "cid",
                    "refresh_token": "rt"
                  }}
                }}"#
            ),
        );
        let tok = load_grok_session_token_from_path(&path).expect("token");
        assert_eq!(tok.access_token, "jwt-access-token");
        assert!(!tok.refreshed);
    }

    #[test]
    fn prefers_non_expired_oidc() {
        let dir = tempfile::tempdir().unwrap();
        let good = (Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        let bad = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let path = write_auth(
            &dir,
            &format!(
                r#"{{
                  "a": {{
                    "key": "expired-token",
                    "auth_mode": "oidc",
                    "expires_at": "{bad}",
                    "oidc_issuer": "https://auth.x.ai",
                    "oidc_client_id": "c",
                    "refresh_token": "rt"
                  }},
                  "b": {{
                    "key": "fresh-token",
                    "auth_mode": "oidc",
                    "expires_at": "{good}",
                    "oidc_issuer": "https://auth.x.ai",
                    "oidc_client_id": "c"
                  }}
                }}"#
            ),
        );
        let tok = load_grok_session_token_from_path(&path).expect("token");
        assert_eq!(tok.access_token, "fresh-token");
    }

    #[test]
    fn empty_or_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_grok_session_token_from_path(&dir.path().join("nope")).is_none());
        let path = write_auth(&dir, "{}");
        assert!(load_grok_session_token_from_path(&path).is_none());
    }

    #[test]
    fn auth_source_and_proxy_helpers() {
        assert!(is_grok_session_auth_source(Some("grok_session")));
        assert!(is_grok_session_auth_source(Some("Grok_Session")));
        assert!(!is_grok_session_auth_source(Some("default")));
        assert!(!is_grok_session_auth_source(None));
        assert!(is_cli_chat_proxy_url("https://cli-chat-proxy.grok.com/v1"));
        assert!(!is_cli_chat_proxy_url("https://api.x.ai/v1"));
    }

    #[test]
    fn inject_headers_is_idempotent() {
        let mut h = indexmap::IndexMap::new();
        inject_grok_session_headers(&mut h);
        inject_grok_session_headers(&mut h);
        assert_eq!(
            h.get(X_XAI_TOKEN_AUTH_HEADER).map(String::as_str),
            Some(X_XAI_TOKEN_AUTH_VALUE)
        );
        assert_eq!(
            h.get(X_GROK_CLIENT_IDENTIFIER_HEADER).map(String::as_str),
            Some(X_GROK_CLIENT_IDENTIFIER_VALUE)
        );
        assert!(
            h.get(X_GROK_CLIENT_VERSION_HEADER)
                .is_some_and(|v| !v.is_empty() && v != "none")
        );
        h.insert(
            X_XAI_TOKEN_AUTH_HEADER.to_string(),
            "custom".to_string(),
        );
        inject_grok_session_headers(&mut h);
        assert_eq!(h.get(X_XAI_TOKEN_AUTH_HEADER).map(String::as_str), Some("custom"));
    }

    #[test]
    fn parse_grok_version_line_extracts_semver() {
        assert_eq!(
            parse_grok_version_line("grok 0.2.111 (94172f2aa4e5)").as_deref(),
            Some("0.2.111")
        );
        assert_eq!(
            parse_grok_version_line("0.1.202 [stable]").as_deref(),
            Some("0.1.202")
        );
        assert!(parse_grok_version_line("no version here").is_none());
    }
}
