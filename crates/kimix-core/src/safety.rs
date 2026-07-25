//! File-operation safety: deny-first path guards and response sanitization.
//!
//! Provides a deny-first security layer for file operations, blocking access
//! to sensitive paths and sanitizing API keys from generated text.
//!
//! Redaction engine uses `LazyLock`-compiled regex patterns with a `RegexSet`
//! fast-path: input with no potential secrets returns `Cow::Borrowed` (zero allocation).

use std::borrow::Cow;
use std::path::{Path, PathBuf};

// ── PathGuard (unchanged API, preserved for backward compatibility) ──────────

/// Path-based security guard with deny-first pattern matching.
///
/// Evaluates file paths against a deny list and an optional safe-mode workspace
/// boundary before any file I/O is performed.
pub struct PathGuard {
    /// Deny-list path patterns (glob-style, case-sensitive).
    deny_patterns: Vec<String>,
    /// Pre-compiled glob set for patterns containing `*`.
    wildcard_set: globset::GlobSet,
    /// Maps `wildcard_set` match indices back to `deny_patterns` indices.
    wildcard_index: Vec<usize>,
    /// Workspace root for safe-mode enforcement.
    workspace_root: PathBuf,
    /// When true, only paths within `workspace_root` are permitted.
    safe_mode: bool,
}

impl PathGuard {
    /// Default deny patterns covering common sensitive paths.
    pub const DEFAULT_DENY_PATTERNS: &'static [&'static str] = &[
        ".ssh/",
        ".aws/",
        "credentials",
        "*.pem",
        "*.key",
        ".env",
    ];

    /// Build a guard with default deny patterns.
    pub fn new(workspace_root: PathBuf) -> Self {
        Self::with_patterns(
            workspace_root,
            Self::DEFAULT_DENY_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )
    }

    /// Build a guard with custom deny patterns.
    pub fn with_patterns(workspace_root: PathBuf, deny_patterns: Vec<String>) -> Self {
        let (wildcard_set, wildcard_index) = Self::compile_wildcards(&deny_patterns);
        Self {
            deny_patterns,
            wildcard_set,
            wildcard_index,
            workspace_root,
            safe_mode: false,
        }
    }

    /// Pre-compile wildcard patterns into a `GlobSet`.
    ///
    /// `literal_separator(false)` preserves the legacy semantics where `*`
    /// crosses `/` (e.g. `*.pem` matches `/etc/ssl/cert.pem`).
    fn compile_wildcards(patterns: &[String]) -> (globset::GlobSet, Vec<usize>) {
        let mut builder = globset::GlobSetBuilder::new();
        let mut index = Vec::new();
        for (i, pattern) in patterns.iter().enumerate() {
            if !pattern.contains('*') {
                continue;
            }
            match globset::GlobBuilder::new(pattern)
                .literal_separator(false)
                .build()
            {
                Ok(glob) => {
                    builder.add(glob);
                    index.push(i);
                }
                // Invalid globs are dropped; they simply never match.
                Err(_) => {}
            }
        }
        let set = builder.build().unwrap_or_else(|_| globset::GlobSet::empty());
        (set, index)
    }

    /// Enable safe mode: reject any path outside `workspace_root`.
    pub fn set_safe_mode(&mut self, enabled: bool) {
        self.safe_mode = enabled;
    }

    /// Update the workspace root (e.g., after the user changes directories).
    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = root;
    }

    /// Check whether `path` is permitted.
    ///
    /// Returns `Ok(())` if the path passes all checks, or `Err(reason)` if denied.
    pub fn check(&self, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy();

        // 1. Deny-list check (deny-first — evaluated before any allow rules)
        // 1a. Plain patterns: substring match.
        for pattern in &self.deny_patterns {
            if !pattern.contains('*') && path_str.contains(pattern) {
                return Err(format!(
                    "Path denied by safety guard: '{}' matches deny pattern '{}'",
                    path_str, pattern
                ));
            }
        }
        // 1b. Wildcard patterns: pre-compiled glob set.
        if let Some(&pattern_idx) = self
            .wildcard_set
            .matches(path_str.as_ref())
            .first()
            .and_then(|&m| self.wildcard_index.get(m))
        {
            return Err(format!(
                "Path denied by safety guard: '{}' matches deny pattern '{}'",
                path_str, self.deny_patterns[pattern_idx]
            ));
        }

        // 2. Safe-mode workspace boundary check
        if self.safe_mode {
            // dunce avoids Windows \\?\ verbatim paths that break starts_with.
            let canonical_root = dunce::canonicalize(&self.workspace_root)
                .unwrap_or_else(|_| self.workspace_root.clone());
            let canonical_path =
                dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

            if !canonical_path.starts_with(&canonical_root) {
                return Err(format!(
                    "Safe-mode: path '{}' is outside workspace root '{}'",
                    path_str,
                    canonical_root.display()
                ));
            }
        }

        Ok(())
    }
}

// ── Redaction engine (upgraded with LazyLock + RegexSet fast path) ──────────

const REDACTED: &str = "***REDACTED***";

/// Compile a regex with panic-on-error (valid at compile time via LazyLock).
fn compile(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("invalid regex pattern")
}

/// Vendor API keys with `sk-`/`sk_` prefixes and xAI (`xai-`) keys.
/// `\b`-anchored so `task-`/`disk-`/`risk-` don't false-match.
static API_KEY_PREFIX_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile(r"\b(?:sk[-_]|xai-)[A-Za-z0-9_-]{20,}"));

/// AWS long-term (`AKIA`) and temporary (`ASIA`) access-key IDs.
static AWS_ACCESS_KEY_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"));

/// GitHub PATs: classic (`ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`) + fine-grained (`github_pat_`).
static GITHUB_TOKEN_REGEX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    compile(r"\b(?:gh[opusr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})")
});

/// GitLab (`glpat-`) and Slack (`xoxa-`/`xoxb-`/`xoxp-`/`xapp-`) tokens.
static VENDOR_TOKEN_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile(r"\b(?:glpat-|xox[abp]-|xapp-)[A-Za-z0-9-]{10,}"));

/// Google API keys (`AIza` + 35 chars).
static GOOGLE_API_KEY_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile(r"\bAIza[0-9A-Za-z_-]{35}"));

/// PEM private-key block (any key type). `(?s)` makes `.` span newlines.
static PEM_PRIVATE_KEY_REGEX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    compile(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----")
});

/// Bearer tokens in `Authorization: Bearer ...` headers.
static BEARER_TOKEN_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile(r"(?i)\bBearer\s+[A-Za-z0-9._\-]{16,}\b"));

/// Bare JWT (`eyJ...header.payload.signature`) — deployment keys / OIDC tokens.
static JWT_REGEX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    compile(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b")
});

/// Secret assignment patterns: `api_key=xxx`, `access_token: yyy`, etc.
static SECRET_ASSIGNMENT_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| {
        compile(
            r#"(?ix)
            \b(
                api[_-]?key
              | (?:access|refresh|id)[_-]token
              | token
              | secret
              | client[_-]secret
              | password
            )\b
            (\s*[:=]\s*)
            (["']?)
            [^\s"',&]{8,}
            "#,
        )
    });

/// URL pattern for detecting URLs in free text.
static URL_REGEX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile(r#"https?://[^\s"'<>(){}\[\],;`]+"#));

/// Fast-path check: returns true if any pattern might match.
static MATCH_ANY: std::sync::LazyLock<regex::RegexSet> = std::sync::LazyLock::new(|| {
    regex::RegexSet::new([
        API_KEY_PREFIX_REGEX.as_str(),
        AWS_ACCESS_KEY_REGEX.as_str(),
        GITHUB_TOKEN_REGEX.as_str(),
        VENDOR_TOKEN_REGEX.as_str(),
        GOOGLE_API_KEY_REGEX.as_str(),
        PEM_PRIVATE_KEY_REGEX.as_str(),
        BEARER_TOKEN_REGEX.as_str(),
        JWT_REGEX.as_str(),
        URL_REGEX.as_str(),
        SECRET_ASSIGNMENT_REGEX.as_str(),
    ])
    .expect("redact_secrets RegexSet")
});

/// Redact secrets from input text.
///
/// Uses a `RegexSet` fast-path: if no pattern matches, returns `Cow::Borrowed`
/// (zero allocation). Otherwise returns `Cow::Owned` with all secrets replaced.
pub fn redact_secrets(input: &str) -> Cow<'_, str> {
    if !MATCH_ANY.is_match(input) {
        return Cow::Borrowed(input);
    }
    let s = PEM_PRIVATE_KEY_REGEX.replace_all(input, REDACTED);
    let s = API_KEY_PREFIX_REGEX.replace_all(&s, REDACTED);
    let s = AWS_ACCESS_KEY_REGEX.replace_all(&s, REDACTED);
    let s = GITHUB_TOKEN_REGEX.replace_all(&s, REDACTED);
    let s = VENDOR_TOKEN_REGEX.replace_all(&s, REDACTED);
    let s = GOOGLE_API_KEY_REGEX.replace_all(&s, REDACTED);
    let s = BEARER_TOKEN_REGEX.replace_all(&s, format!("Bearer {REDACTED}"));
    let s = JWT_REGEX.replace_all(&s, REDACTED);
    let s = redact_urls_in(&s);
    let s = SECRET_ASSIGNMENT_REGEX
        .replace_all(&s, format!("$1$2$3{REDACTED}"))
        .into_owned();
    Cow::Owned(s)
}

/// Walk all string values in a JSON value, applying `f` to each.
pub fn walk_json_strings(value: &mut serde_json::Value, f: &mut impl FnMut(&mut String)) {
    match value {
        serde_json::Value::String(s) => f(s),
        serde_json::Value::Array(arr) => arr.iter_mut().for_each(|v| walk_json_strings(v, f)),
        serde_json::Value::Object(map) => {
            map.values_mut().for_each(|v| walk_json_strings(v, f))
        }
        _ => {}
    }
}

/// Redact secrets from all string values in a JSON value (in-place).
pub fn redact_json_string_values(value: &mut serde_json::Value) {
    walk_json_strings(value, &mut |s| {
        if let Cow::Owned(replaced) = redact_secrets(s) {
            *s = replaced;
        }
    });
}

/// Redact sensitive query parameters in URLs found within free text.
fn redact_urls_in(text: &str) -> String {
    URL_REGEX
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let raw = &caps[0];
            url::Url::parse(raw).map_or_else(|_| raw.to_owned(), |mut url| {
                redact_url(&mut url);
                url.to_string()
            })
        })
        .into_owned()
}

/// Sensitive URL query parameter names whose values should be redacted.
const SENSITIVE_QUERY_PARAMS: &[&str] = &[
    "access_token",
    "api_key",
    "assertion",
    "auth",
    "client_secret",
    "code",
    "code_verifier",
    "id_token",
    "key",
    "password",
    "refresh_token",
    "requested_token",
    "session_id",
    "state",
    "subject_token",
    "token",
];

/// Redact sensitive parameters in a URL's query string.
pub fn redact_url(url: &mut url::Url) {
    let sensitive: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| {
            SENSITIVE_QUERY_PARAMS
                .iter()
                .any(|p| k.eq_ignore_ascii_case(p))
        })
        .map(|(k, _)| k.to_string())
        .collect();
    if sensitive.is_empty() {
        return;
    }
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            if sensitive.iter().any(|s| s.eq_ignore_ascii_case(&k)) {
                (k.to_string(), "redacted".to_string())
            } else {
                (k.to_string(), v.to_string())
            }
        })
        .collect();
    // Also strip user:password from authority
    if !url.username().is_empty() {
        pairs.push(("_user_redacted".to_string(), "true".to_string()));
    }
    url.set_query(None);
    for (k, v) in &pairs {
        if k == "_user_redacted" {
            url.set_username("redacted").ok();
        } else {
            url.query_pairs_mut().append_pair(k, v);
        }
    }
}

/// Redact user home directory paths: `$HOME` → `~`, username → `<user>`.
pub fn redact_user_paths(input: &str) -> Cow<'_, str> {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok());
    let username = std::env::var("USERNAME")
        .ok()
        .or_else(|| std::env::var("USER").ok());

    let mut result = input.to_string();

    // Replace $HOME with ~
    if let Some(home_dir) = &home
        && !home_dir.is_empty()
    {
        result = result.replace(home_dir, "~");
    }

    // Replace username with <user> (only whole path segments)
    if let Some(name) = &username
        && name.len() >= 3
    {
        let pattern = format!(r"(^|[/\\]){}([/\\]|$)", regex::escape(name));
        if let Ok(re) = regex::Regex::new(&pattern) {
            result = re.replace_all(&result, "${1}<user>${2}").to_string();
        }
    }

    if result == input {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(result)
    }
}

// ── Backward-compatible API ─────────────────────────────────────────────────

/// Sanitize a text response by redacting API keys, tokens, and secrets.
///
/// This is the original kimix API. It now delegates to the upgraded
/// `redact_secrets` engine while preserving the additional `api_keys`
/// exact-match redaction.
pub fn sanitize_response(text: &str, api_keys: &[&str]) -> String {
    let mut result = match redact_secrets(text) {
        Cow::Borrowed(_) => text.to_string(),
        Cow::Owned(s) => s,
    };

    // Redact exact-match API keys (original behavior)
    for key in api_keys {
        if !key.is_empty() {
            result = result.replace(*key, REDACTED);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_guard(workspace: &Path) -> PathGuard {
        PathGuard::new(workspace.to_path_buf())
    }

    // ── PathGuard deny tests (preserved) ────────────────────────────

    #[test]
    fn test_deny_credentials_substring() {
        let guard = make_guard(Path::new("/tmp"));
        assert!(guard.check(Path::new("/tmp/credentials")).is_err());
        assert!(guard.check(Path::new("/home/user/.aws/credentials")).is_err());
    }

    #[test]
    fn test_deny_dot_env() {
        let guard = make_guard(Path::new("/tmp"));
        assert!(guard.check(Path::new("/project/.env")).is_err());
        assert!(guard.check(Path::new("/project/.env.production")).is_err());
    }

    #[test]
    fn test_deny_pem_key_files() {
        let guard = make_guard(Path::new("/tmp"));
        assert!(guard.check(Path::new("id_rsa.pem")).is_err());
        assert!(guard.check(Path::new("/keys/private.key")).is_err());
        assert!(guard.check(Path::new("/etc/ssl/cert.pem")).is_err());
    }

    #[test]
    fn test_allow_normal_files() {
        let guard = make_guard(Path::new("/tmp"));
        assert!(guard.check(Path::new("/tmp/readme.md")).is_ok());
        assert!(guard.check(Path::new("/tmp/src/main.rs")).is_ok());
        assert!(guard.check(Path::new("/tmp/config.toml")).is_ok());
    }

    #[test]
    fn test_deny_ssh_and_aws_paths() {
        let guard = make_guard(Path::new("/tmp"));
        assert!(guard.check(Path::new("/home/user/.ssh/id_rsa")).is_err());
        assert!(guard.check(Path::new("/home/user/.aws/config")).is_err());
    }

    // ── PathGuard safe_mode tests (preserved) ───────────────────────

    #[test]
    fn test_safe_mode_allows_workspace_path() -> std::io::Result<()> {
        let dir = TempDir::new()?;
        let file_path = dir.path().join("allowed.txt");
        fs::write(&file_path, "ok")?;

        let mut guard = make_guard(dir.path());
        guard.set_safe_mode(true);
        assert!(guard.check(&file_path).is_ok());
        Ok(())
    }

    #[test]
    fn test_safe_mode_denies_outside_path() -> std::io::Result<()> {
        let dir = TempDir::new()?;
        let outside = Path::new("/etc/passwd");

        let mut guard = make_guard(dir.path());
        guard.set_safe_mode(true);
        assert!(guard.check(outside).is_err());
        Ok(())
    }

    #[test]
    fn test_safe_mode_off_allows_outside() {
        let guard = make_guard(Path::new("/tmp/workspace"));
        assert!(guard.check(Path::new("/etc/hosts")).is_ok());
    }

    // ── sanitize_response tests (preserved) ────────────────────────

    #[test]
    fn test_sanitize_api_key_assignment() {
        let key = format!("sk-{}", "x".repeat(40));
        let input = format!("api_key = \"{key}\"");
        let sanitized = sanitize_response(&input, &[]);
        assert!(!sanitized.contains(&key));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_bearer_token() {
        // Synthetic JWT-shaped stand-in (not a real token).
        let jwt = format!("{}.{}.{}", "eyJ0ZXN0", "eyJwYXlsb2Fk", "c2ln");
        let input = format!("Authorization: Bearer {jwt}");
        let sanitized = sanitize_response(&input, &[]);
        assert!(!sanitized.contains(&jwt));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_jwt() {
        let jwt = format!(
            "{}.{}.{}",
            "eyJ0ZXN0",
            "eyJwYXlsb2Fk",
            "c2lnbmF0dXJlMTIz"
        );
        let input = format!("token: {jwt}");
        let sanitized = sanitize_response(&input, &[]);
        assert!(!sanitized.contains(&jwt));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_aws_key() {
        // Runtime-built so source/history scanners do not see a full key id.
        let id = format!("AKIA{}", "0".repeat(16));
        let input = format!("AWS_ACCESS_KEY_ID={id}");
        let sanitized = sanitize_response(&input, &[]);
        assert!(!sanitized.contains("AKIA0"));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_private_key() {
        let input = "key: -----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
        let sanitized = sanitize_response(input, &[]);
        assert!(!sanitized.contains("BEGIN RSA PRIVATE KEY"));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_password_assignment() {
        let input = "password = \"mysecret123\"";
        let sanitized = sanitize_response(input, &[]);
        assert!(!sanitized.contains("mysecret123"));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_exact_key_redaction() {
        let key = format!("sk-test-{}", "z".repeat(20));
        let input = format!("Using key {key} for authentication.");
        let sanitized = sanitize_response(&input, &[key.as_str()]);
        assert!(!sanitized.contains(&key));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn test_sanitize_preserves_normal_text() {
        let input = "The file was successfully read from /home/user/document.txt.";
        let sanitized = sanitize_response(input, &[]);
        assert_eq!(sanitized, input);
    }

    // ── New: redact_secrets tests ──────────────────────────────────

    #[test]
    fn test_redact_xai_api_key() {
        // Construct at runtime so privacy-guard does not see a literal key assignment.
        let key = format!("xai-{}", "a".repeat(32));
        let input = format!("export XAI_API_KEY={key}");
        let result = redact_secrets(&input);
        assert!(!result.contains(&key));
        assert!(result.contains(REDACTED));
    }

    #[test]
    fn test_redact_github_pat() {
        // Build fixture at runtime so the source never contains a literal PAT.
        let pat = format!("ghp_{}", "0".repeat(36));
        let input = format!("token: {pat}");
        let result = redact_secrets(&input);
        assert!(!result.contains(&pat));
        assert!(result.contains(REDACTED));
    }

    #[test]
    fn test_redact_aws_temp_key() {
        let id = format!("ASIA{}", "0".repeat(16));
        let result = redact_secrets(&id);
        assert!(!result.contains("ASIA0"));
        assert!(result.contains(REDACTED));
    }

    #[test]
    fn test_redact_access_token_assignment() {
        // Synthetic three-segment JWT-shaped string (not a real token).
        let jwt = format!(
            "{}.{}.{}",
            "eyJ0ZXN0".to_string(), // base64url-ish header stand-in
            "eyJwYXlsb2Fk".to_string(),
            "c2lnbmF0dXJl".to_string()
        );
        let input = format!(r#"{{"access_token": "{jwt}"}}"#);
        let result = redact_secrets(&input);
        assert!(result.contains("access_token"));
        assert!(result.contains(REDACTED));
    }

    #[test]
    fn test_redact_preserves_clean_text() {
        let input = "The quick brown fox jumps over the lazy dog.";
        let result = redact_secrets(input);
        assert_eq!(result, Cow::Borrowed(input));
    }

    // ── New: redact_json_string_values tests ───────────────────────

    #[test]
    fn test_redact_json_with_secrets() {
        let sk = format!("sk-{}", "a".repeat(32));
        let pat = format!("ghp_{}", "0".repeat(36));
        let mut value = serde_json::json!({
            "name": "test",
            "api_key": sk,
            "nested": {
                "token": pat
            }
        });
        redact_json_string_values(&mut value);
        let json_str = value.to_string();
        assert!(!json_str.contains(&sk));
        assert!(!json_str.contains(&pat));
        assert!(json_str.contains(REDACTED));
    }

    // ── New: redact_url tests ──────────────────────────────────────

    #[test]
    fn test_redact_url_sensitive_params() {
        let mut url = url::Url::parse("https://api.example.com/callback?code=secret123&state=xyz&page=1").unwrap();
        redact_url(&mut url);
        let url_str = url.to_string();
        assert!(url_str.contains("code=redacted"));
        assert!(url_str.contains("state=redacted"));
        assert!(url_str.contains("page=1"));
    }

    // ── New: redact_user_paths tests ───────────────────────────────

    #[test]
    fn test_redact_user_paths_home() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/testuser".to_string());
        let input = format!("{}/Documents/project/file.rs", home);
        let result = redact_user_paths(&input);
        assert!(result.contains("~/Documents/project/file.rs"));
        assert!(!result.contains(&home));
    }
}
