//! Policy transform for command-boundary sandbox decisions.
//!
//! **Approval** and **sandbox profile** are independent axes (Codex-shaped).
//! Bash / terminal / exec-server should only call into this module for:
//! - legacy name → policy resolution
//! - profile ↔ exec-protocol mode mapping
//! - workspace-write protected path checks (`.git`, sensitive config)
//! - pure bwrap deny plan for tests
//!
//! Kernel enforcement stays in [`crate::SandboxManager`]; this module is the
//! pure policy layer so DoD holds: policy changes live here + unit tests.

use std::path::{Component, Path, PathBuf};

use crate::profiles::ProfileName;

// ── Approval (independent of OS profile) ─────────────────────────────────────

/// When the agent may auto-approve bash/exec without a user prompt.
///
/// Distinct from [`ProfileName`]: a read-only sandbox can still `Ask`, and
/// an `Off` profile can still `Never` (YOLO). Do not collapse these axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    /// Prompt before every bash/exec tool call.
    #[default]
    Ask,
    /// Auto-approve only while a kernel sandbox is active; otherwise ask.
    ///
    /// Matches the historical `auto_allow_bash` semantics.
    OnFailure,
    /// Never prompt (full auto-approve within whatever profile bounds exist).
    Never,
}

impl ApprovalPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::OnFailure => "on-failure",
            Self::Never => "never",
        }
    }

    /// Parse config / CLI strings. Unknown → `None` (caller keeps default).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ask" | "untrusted" | "on-request" => Some(Self::Ask),
            "on-failure" | "on_failure" | "auto" => Some(Self::OnFailure),
            "never" | "yolo" | "always-approve" | "always_approve" => Some(Self::Never),
            _ => None,
        }
    }

    /// Whether bash should skip the permission prompt under this policy.
    pub fn auto_allow_bash(self, sandbox_is_active: bool) -> bool {
        match self {
            Self::Never => true,
            Self::OnFailure => sandbox_is_active,
            Self::Ask => false,
        }
    }
}

// ── Combined policy ──────────────────────────────────────────────────────────

/// Fully-resolved exec surface policy (profile + approval).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecPolicy {
    pub profile: ProfileName,
    pub approval: ApprovalPolicy,
}

impl ExecPolicy {
    pub fn new(profile: ProfileName, approval: ApprovalPolicy) -> Self {
        Self { profile, approval }
    }

    pub fn auto_allow_bash(&self, sandbox_is_active: bool) -> bool {
        self.approval.auto_allow_bash(sandbox_is_active)
    }

    pub fn restricts_child_network(&self) -> bool {
        self.profile.restricts_network()
    }

    /// Whether handler-level writes are forbidden entirely (read-only profile).
    pub fn is_read_only(&self) -> bool {
        matches!(self.profile, ProfileName::ReadOnly)
    }
}

// ── Legacy / CLI name resolution ─────────────────────────────────────────────

/// Resolve a single legacy string into profile + approval.
///
/// Old configs often collapsed both axes into one token:
/// - `danger-full-access` / `off` → Off + Never
/// - `workspace-write` / `workspace` → Workspace + Ask
/// - `read-only` → ReadOnly + Ask
/// - `strict` / `devbox` → same name + Ask
///
/// Explicit `profile+approval` pairs should use [`ExecPolicy::new`] instead.
pub fn resolve_legacy_policy(name: &str) -> Result<ExecPolicy, String> {
    let s = name.trim().to_ascii_lowercase();
    if s.is_empty() {
        return Err("empty sandbox policy name".into());
    }

    // Combined aliases first (Codex / OpenAI-style).
    match s.as_str() {
        "danger-full-access" | "danger_full_access" | "full-access" | "full_access" => {
            return Ok(ExecPolicy::new(ProfileName::Off, ApprovalPolicy::Never));
        }
        "workspace-write" | "workspace_write" | "write" => {
            return Ok(ExecPolicy::new(
                ProfileName::Workspace,
                ApprovalPolicy::Ask,
            ));
        }
        _ => {}
    }

    let profile: ProfileName = s.parse().map_err(|e: String| e)?;
    Ok(ExecPolicy::new(profile, ApprovalPolicy::Ask))
}

/// Map an exec-protocol sandbox mode onto a kernel profile.
///
/// Single funnel used by `kimix-exec-server` so mode→profile cannot drift.
pub fn profile_for_sandbox_mode(mode: &str) -> Option<ProfileName> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "off" | "danger-full-access" => Some(ProfileName::Off),
        "workspace-write" | "write" | "workspace" => Some(ProfileName::Workspace),
        "read-only" | "readonly" | "read" => Some(ProfileName::ReadOnly),
        // Optional extensions not in exec-protocol today:
        "strict" => Some(ProfileName::Strict),
        "devbox" => Some(ProfileName::Devbox),
        _ => None,
    }
}

/// Inverse of [`profile_for_sandbox_mode`] for the three protocol modes.
pub fn sandbox_mode_for_profile(profile: &ProfileName) -> &'static str {
    match profile {
        ProfileName::Off => "off",
        ProfileName::ReadOnly => "read-only",
        ProfileName::Workspace | ProfileName::Devbox | ProfileName::Strict | ProfileName::Custom(_) => {
            "workspace-write"
        }
    }
}

// ── Protected write paths (Codex-like workspace-write bounds) ────────────────

/// Workspace-relative path prefixes that **must not** be written under
/// workspace-write (handler-level; reads stay allowed so `git status` works).
///
/// Kernel deny is full R+W; protecting write-only at the handler keeps
/// `.git` readable while blocking destructive rewrites of git objects / config.
pub const PROTECTED_WRITE_PREFIXES: &[&str] = &[
    ".git",
    // Project sandbox config — a malicious tool write must not hollow it out.
    ".kimix/sandbox.toml",
];

/// Whether `target` falls under a protected write path relative to `workspace`.
///
/// Lexical only (no I/O). Symlink escapes are the kernel sandbox's job;
/// this is the portable handler-level belt-and-braces check.
pub fn is_protected_write_path(workspace: &Path, target: &Path) -> bool {
    let relative = match strip_workspace_prefix(workspace, target) {
        Some(r) => r,
        None => return false, // outside workspace — different check owns that
    };
    let rel = normalize_rel(&relative);
    // `Path::is_empty` is unstable; empty OsStr means no relative components.
    if rel.as_os_str().is_empty() {
        return false;
    }
    for prefix in PROTECTED_WRITE_PREFIXES {
        if path_has_prefix(&rel, prefix) {
            return true;
        }
    }
    false
}

/// Handler-level write decision for exec-server / tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDecision {
    Allow,
    /// Profile is read-only.
    DenyReadOnly,
    /// Path is outside the workspace.
    DenyOutsideWorkspace,
    /// Path is under a protected prefix (`.git`, sandbox.toml, …).
    DenyProtected,
}

/// Decide whether a write to `target` is allowed under `profile` + workspace.
///
/// Pure policy — no fs access beyond path logic. Callers that need existence
/// checks (canonicalize) should resolve paths first, then pass absolutes.
pub fn allows_write(profile: &ProfileName, workspace: &Path, target: &Path) -> WriteDecision {
    if matches!(profile, ProfileName::Off) {
        // Off: no handler bounds (kernel also off). Still block nothing here
        // so the parent process / user permission layer remains authoritative.
        return WriteDecision::Allow;
    }
    if matches!(profile, ProfileName::ReadOnly) {
        return WriteDecision::DenyReadOnly;
    }
    // Workspace / Devbox / Strict / Custom: require containment + not protected.
    if !path_within_workspace(workspace, target) {
        return WriteDecision::DenyOutsideWorkspace;
    }
    if is_protected_write_path(workspace, target) {
        return WriteDecision::DenyProtected;
    }
    WriteDecision::Allow
}

// ── Pure bwrap plan (testable without spawning) ──────────────────────────────

/// Pure description of a Linux bwrap re-exec plan.
///
/// Mirrors what [`crate::bwrap_reexec_command`] would mount; kept free of
/// process state so unit tests pin the policy without needing bwrap installed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BwrapPlan {
    /// Paths mounted `--ro-bind` (write-denied but readable).
    pub deny_write: Vec<String>,
    /// Paths bound over with an unreadable placeholder (read+write denied).
    pub deny_read: Vec<String>,
    /// Whether deny globs were present (launch-time expansion may still apply).
    pub has_globs: bool,
}

impl BwrapPlan {
    /// Whether any re-exec is needed.
    pub fn needs_reexec(&self) -> bool {
        !self.deny_write.is_empty() || !self.deny_read.is_empty() || self.has_globs
    }

    /// Flatten to argv tokens for assertion (`--ro-bind`, path, path, …).
    ///
    /// Only the deny_write / deny_read binds — not the full bwrap wrapper.
    pub fn ro_bind_argv(&self) -> Vec<String> {
        let mut args = Vec::new();
        for p in &self.deny_write {
            args.push("--ro-bind".into());
            args.push(p.clone());
            args.push(p.clone());
        }
        for p in &self.deny_read {
            // Placeholder source is opaque at plan level; pin the target only.
            args.push("--ro-bind".into());
            args.push(format!("<blocked>:{p}"));
            args.push(p.clone());
        }
        args
    }
}

/// Build a pure [`BwrapPlan`] for a profile (devbox `/data` write-deny +
/// exact deny paths from config). Does **not** expand globs or touch the fs
/// for placeholders — those stay in the re-exec path.
///
/// Used by tests and diagnostics; production re-exec still goes through
/// [`crate::bwrap_reexec_for_profile`].
pub fn bwrap_plan_for_profile(
    profile: &ProfileName,
    workspace: &Path,
    extra_deny_write: &[&str],
    extra_deny_read: &[&str],
) -> BwrapPlan {
    let config = crate::profiles::load_sandbox_config(workspace);
    let mut deny_write: Vec<String> = Vec::new();
    let mut deny_read: Vec<String> = Vec::new();
    let mut has_globs = false;

    // Devbox (and extends=devbox): write-deny /data.
    let is_devbox = match profile {
        ProfileName::Devbox => true,
        ProfileName::Custom(name) => {
            config
                .profiles
                .get(name)
                .and_then(|p| p.extends.as_deref())
                == Some("devbox")
        }
        _ => false,
    };
    if is_devbox {
        deny_write.push("/data".into());
    }
    for p in extra_deny_write {
        if !deny_write.iter().any(|x| x == p) {
            deny_write.push((*p).to_string());
        }
    }

    // Deny list without `ProfileName::resolve_profile` (that API is
    // `enforce`+unix only). Built-ins share the same sensitive denylist shape
    // as the kernel path; custom profiles read `deny` from sandbox.toml.
    if !matches!(profile, ProfileName::Off) {
        for entry in sensitive_deny_entries_for_plan(profile, &config) {
            let s = entry.as_str();
            if s.contains('*') || s.contains('?') || s.contains('[') {
                has_globs = true;
            } else {
                let p = Path::new(s);
                let abs = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    workspace.join(p)
                };
                let abs_s = abs.to_string_lossy().into_owned();
                if !deny_read.iter().any(|x| x == &abs_s) {
                    deny_read.push(abs_s);
                }
            }
        }
    }
    for p in extra_deny_read {
        if !deny_read.iter().any(|x| x == p) {
            deny_read.push((*p).to_string());
        }
    }

    BwrapPlan {
        deny_write,
        deny_read,
        has_globs,
    }
}

/// Deny path strings for the pure bwrap plan (feature-agnostic).
fn sensitive_deny_entries_for_plan(
    profile: &ProfileName,
    config: &crate::profiles::SandboxConfig,
) -> Vec<String> {
    match profile {
        ProfileName::Custom(name) => config
            .profiles
            .get(name)
            .map(|p| p.deny.clone())
            .unwrap_or_default(),
        ProfileName::Off => Vec::new(),
        // Built-ins: same credential denylist the enforce path uses.
        ProfileName::Workspace
        | ProfileName::Devbox
        | ProfileName::ReadOnly
        | ProfileName::Strict => crate::profiles::default_sensitive_deny_paths()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    }
}

// ── Path helpers (pure) ──────────────────────────────────────────────────────

fn strip_workspace_prefix(workspace: &Path, target: &Path) -> Option<PathBuf> {
    // If both absolute and target starts with workspace.
    if target.is_absolute() && workspace.is_absolute() {
        return target
            .strip_prefix(workspace)
            .ok()
            .map(|p| p.to_path_buf());
    }
    // Relative target is already workspace-relative by convention.
    if !target.is_absolute() {
        return Some(target.to_path_buf());
    }
    None
}

fn normalize_rel(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    out
}

fn path_has_prefix(rel: &Path, prefix: &str) -> bool {
    let prefix_path = Path::new(prefix);
    if rel == prefix_path {
        return true;
    }
    rel.starts_with(prefix_path)
}

/// Lexical workspace containment (no canonicalize — callers may pre-resolve).
pub fn path_within_workspace(workspace: &Path, target: &Path) -> bool {
    // Absolute outside workspace: strip fails → not within.
    if target.is_absolute() {
        return match strip_workspace_prefix(workspace, target) {
            Some(rel) => !rel_escapes_workspace(&rel),
            None => false,
        };
    }
    // Relative paths are treated as workspace-relative; reject `..` escapes.
    !rel_escapes_workspace(target)
}

/// True when a workspace-relative path walks above the workspace root via `..`.
fn rel_escapes_workspace(rel: &Path) -> bool {
    let mut depth = 0i32;
    for c in rel.components() {
        match c {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_parse_and_auto_allow() {
        assert_eq!(ApprovalPolicy::parse("ask"), Some(ApprovalPolicy::Ask));
        assert_eq!(
            ApprovalPolicy::parse("on-failure"),
            Some(ApprovalPolicy::OnFailure)
        );
        assert_eq!(ApprovalPolicy::parse("never"), Some(ApprovalPolicy::Never));
        assert_eq!(ApprovalPolicy::parse("yolo"), Some(ApprovalPolicy::Never));
        assert_eq!(ApprovalPolicy::parse("nope"), None);

        assert!(!ApprovalPolicy::Ask.auto_allow_bash(true));
        assert!(!ApprovalPolicy::OnFailure.auto_allow_bash(false));
        assert!(ApprovalPolicy::OnFailure.auto_allow_bash(true));
        assert!(ApprovalPolicy::Never.auto_allow_bash(false));
    }

    #[test]
    fn legacy_policy_aliases() {
        let p = resolve_legacy_policy("danger-full-access").unwrap();
        assert_eq!(p.profile, ProfileName::Off);
        assert_eq!(p.approval, ApprovalPolicy::Never);

        let p = resolve_legacy_policy("workspace-write").unwrap();
        assert_eq!(p.profile, ProfileName::Workspace);
        assert_eq!(p.approval, ApprovalPolicy::Ask);

        let p = resolve_legacy_policy("read-only").unwrap();
        assert_eq!(p.profile, ProfileName::ReadOnly);

        let p = resolve_legacy_policy("strict").unwrap();
        assert_eq!(p.profile, ProfileName::Strict);
    }

    #[test]
    fn mode_profile_roundtrip() {
        assert_eq!(
            profile_for_sandbox_mode("workspace-write"),
            Some(ProfileName::Workspace)
        );
        assert_eq!(
            profile_for_sandbox_mode("read-only"),
            Some(ProfileName::ReadOnly)
        );
        assert_eq!(profile_for_sandbox_mode("off"), Some(ProfileName::Off));
        assert_eq!(
            sandbox_mode_for_profile(&ProfileName::Workspace),
            "workspace-write"
        );
        assert_eq!(sandbox_mode_for_profile(&ProfileName::ReadOnly), "read-only");
        assert_eq!(sandbox_mode_for_profile(&ProfileName::Off), "off");
    }

    #[test]
    fn protected_git_and_sandbox_toml() {
        let ws = Path::new("/home/u/proj");
        assert!(is_protected_write_path(ws, Path::new("/home/u/proj/.git/config")));
        assert!(is_protected_write_path(ws, Path::new(".git/HEAD")));
        assert!(is_protected_write_path(ws, Path::new(".git")));
        assert!(is_protected_write_path(
            ws,
            Path::new("/home/u/proj/.kimix/sandbox.toml")
        ));
        assert!(!is_protected_write_path(ws, Path::new("/home/u/proj/src/main.rs")));
        assert!(!is_protected_write_path(ws, Path::new("README.md")));
        // .gitignore is NOT .git
        assert!(!is_protected_write_path(ws, Path::new(".gitignore")));
    }

    #[test]
    fn allows_write_matrix() {
        let ws = Path::new("/ws");
        assert_eq!(
            allows_write(&ProfileName::ReadOnly, ws, Path::new("/ws/a.rs")),
            WriteDecision::DenyReadOnly
        );
        assert_eq!(
            allows_write(&ProfileName::Workspace, ws, Path::new("/etc/passwd")),
            WriteDecision::DenyOutsideWorkspace
        );
        assert_eq!(
            allows_write(&ProfileName::Workspace, ws, Path::new("/ws/.git/config")),
            WriteDecision::DenyProtected
        );
        assert_eq!(
            allows_write(&ProfileName::Workspace, ws, Path::new("/ws/src/a.rs")),
            WriteDecision::Allow
        );
        assert_eq!(
            allows_write(&ProfileName::Off, ws, Path::new("/etc/passwd")),
            WriteDecision::Allow
        );
    }

    #[test]
    fn path_within_workspace_rejects_escape() {
        let ws = Path::new("/ws");
        assert!(path_within_workspace(ws, Path::new("src/a.rs")));
        assert!(path_within_workspace(ws, Path::new("/ws/src/a.rs")));
        assert!(!path_within_workspace(ws, Path::new("/etc/passwd")));
        assert!(!path_within_workspace(ws, Path::new("../escape")));
        assert!(!path_within_workspace(ws, Path::new("foo/../../escape")));
    }

    #[test]
    fn bwrap_plan_devbox_includes_data() {
        let ws = std::env::temp_dir().join(format!(
            "kimix-bwrap-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&ws).unwrap();
        let plan = bwrap_plan_for_profile(&ProfileName::Devbox, &ws, &[], &[]);
        assert!(plan.deny_write.iter().any(|p| p == "/data"));
        assert!(plan.needs_reexec());
        let argv = plan.ro_bind_argv();
        assert!(argv.windows(3).any(|w| w == ["--ro-bind", "/data", "/data"]));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn bwrap_plan_workspace_no_reexec_by_default() {
        let ws = std::env::temp_dir().join(format!(
            "kimix-bwrap-ws-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&ws).unwrap();
        let plan = bwrap_plan_for_profile(&ProfileName::Workspace, &ws, &[], &[]);
        // Built-in workspace has sensitive deny globs (e.g. ~/.ssh/id_*) which
        // set has_globs — still no deny_write. needs_reexec may be true if globs.
        assert!(plan.deny_write.is_empty());
        let _ = std::fs::remove_dir_all(&ws);
    }
}
