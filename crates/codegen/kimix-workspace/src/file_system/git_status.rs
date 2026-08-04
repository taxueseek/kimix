//! Generates a compact git status for the system prompt.
//!
//! Uses the git CLI for performance — libgit2's status is 5-10x slower than
//! the native git binary on large repos due to inefficient index refresh.
//! Output is prioritized by change type and limited to ~1k characters.
//!
//! Both [`git_status`] and [`git_status_short`] share a short TTL cache so
//! concurrent prompt/user-prefix builds for the same cwd do not re-spawn
//! `git` within a few seconds. Content is byte-identical to an uncached
//! call; the prompt already documents this as a point-in-time snapshot.
use crate::file_system::FsError;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Align with `session::git::GIT_STATUS_CACHE_TTL` — prompt text is a
/// snapshot, so a few seconds of reuse does not change model semantics.
const PROMPT_GIT_STATUS_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PromptGitKind {
    /// Compact multi-line block for system / user prefix (`git_status`).
    Compact = 0,
    /// Porcelain short form (`git status --short --branch`).
    Short = 1,
}

struct PromptGitCacheEntry {
    value: String,
    cached_at: Instant,
}

impl PromptGitCacheEntry {
    fn is_fresh(&self, now: Instant) -> bool {
        now.duration_since(self.cached_at) < PROMPT_GIT_STATUS_TTL
    }
}

static PROMPT_GIT_CACHE: LazyLock<Mutex<HashMap<(PathBuf, PromptGitKind), PromptGitCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn prompt_git_cache_get(cwd: &Path, kind: PromptGitKind) -> Option<String> {
    let cache = PROMPT_GIT_CACHE.lock();
    let entry = cache.get(&(cwd.to_path_buf(), kind))?;
    if entry.is_fresh(Instant::now()) {
        Some(entry.value.clone())
    } else {
        None
    }
}

fn prompt_git_cache_put(cwd: PathBuf, kind: PromptGitKind, value: String) -> String {
    let mut cache = PROMPT_GIT_CACHE.lock();
    cache.insert(
        (cwd, kind),
        PromptGitCacheEntry {
            value: value.clone(),
            cached_at: Instant::now(),
        },
    );
    value
}

/// Test / recovery: drop all prompt git status cache entries.
#[cfg(test)]
pub(crate) fn clear_prompt_git_status_cache_for_test() {
    PROMPT_GIT_CACHE.lock().clear();
}

/// Gets a compact git status for the system prompt using the git CLI.
///
/// Output includes:
/// 1. Branch name
/// 2. Upstream ahead/behind status
/// 3. Staged files (if any)
///
/// Total output is capped at ~1k characters.
///
/// Results are cached for [`PROMPT_GIT_STATUS_TTL`] per working directory.
pub async fn git_status(working_directory: impl Into<PathBuf>) -> Result<String, FsError> {
    let working_directory = working_directory.into();
    if let Some(hit) = prompt_git_cache_get(&working_directory, PromptGitKind::Compact) {
        return Ok(hit);
    }

    let cwd = working_directory.clone();
    let result = tokio::task::spawn_blocking(move || git_status_impl(&working_directory))
        .await
        .map_err(|e| FsError::Other(format!("git status task failed: {}", e)))?;
    match result {
        Ok(s) => Ok(prompt_git_cache_put(cwd, PromptGitKind::Compact, s)),
        Err(e) => Err(e),
    }
}

/// Matches Node's default `execFile` `maxBuffer` (1 MiB). This cap is
/// load-bearing: `git status` output at or above it makes the spawn throw, so
/// the repo is dropped from `<git_status>` entirely (never truncated).
/// Oversized output is treated as an error -- the caller maps `Err` to a
/// dropped section.
const GIT_STATUS_BUFFER_LIMIT: usize = 1024 * 1024;

/// Whether `git status` stdout is large enough that the repo is dropped
/// (`>= 1 MiB`). Extracted as a pure predicate so it is unit-testable
/// without spawning git.
fn git_status_exceeds_buffer(stdout_len: usize) -> bool {
    stdout_len >= GIT_STATUS_BUFFER_LIMIT
}

/// Collapse runs of 2+ spaces to a single space.
///
/// The `<git_status>` body collapses consecutive spaces, so the
/// porcelain two-column status renders with a single separator: `A  staged.txt`
/// (index-added, clean worktree) becomes `A staged.txt`, `M  mod.txt` becomes
/// `M mod.txt`, `R  old -> new` becomes `R old -> new`. A single leading space
/// (e.g. ` M file`, worktree-modified) and the rename ` -> ` separator are
/// preserved because they are runs of length one.
/// Newlines are never touched.
fn collapse_status_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        out.push(ch);
    }
    out
}

/// Short git status for the templated user message.
///
/// Runs `git status --short --branch` and returns its output with consecutive
/// spaces collapsed via [`collapse_status_spaces`]: a leading `## <branch>`
/// line followed by the file change list (or just `## <branch>` on a clean
/// tree). This matches the body embedded in the `<git_status>` block
/// byte-for-byte.
///
/// Results are cached for [`PROMPT_GIT_STATUS_TTL`] per working directory.
pub async fn git_status_short(working_directory: impl Into<PathBuf>) -> Result<String, FsError> {
    let working_directory = working_directory.into();
    if let Some(hit) = prompt_git_cache_get(&working_directory, PromptGitKind::Short) {
        return Ok(hit);
    }

    let cwd = working_directory.clone();
    let result = tokio::task::spawn_blocking(move || {
        let output = kimix_tty_utils::git_command()
            .args(["status", "--short", "--branch"])
            .current_dir(&working_directory)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .map_err(|e| FsError::Other(format!("git status --short --branch failed: {}", e)))?;

        if !output.status.success() {
            return Err(FsError::Other(format!(
                "git status --short --branch exited with code {:?}",
                output.status.code()
            )));
        }

        // Output >= 1 MiB is dropped entirely, not truncated. Render-time
        // truncation handles the < 1 MiB case.
        if git_status_exceeds_buffer(output.stdout.len()) {
            return Err(FsError::Other(
                "git status --short --branch output exceeded 1 MiB buffer".to_string(),
            ));
        }

        // Consecutive spaces in the status body are collapsed so staged
        // entries (`A  file` -> `A file`) match the wire format.
        Ok(collapse_status_spaces(&String::from_utf8_lossy(
            &output.stdout,
        )))
    })
    .await
    .map_err(|e| FsError::Other(format!("git status --short --branch task failed: {}", e)))?;

    match result {
        Ok(s) => Ok(prompt_git_cache_put(cwd, PromptGitKind::Short, s)),
        Err(e) => Err(e),
    }
}

fn git_status_impl(working_directory: &Path) -> Result<String, FsError> {
    let _timer = /* instrumentation_timer */ () ; // dev macro; noop stub ("git_status.impl")
    let max_status_chars = 1000;
    let mut output = String::with_capacity(max_status_chars);

    // Get branch name
    let branch_name = {
        let _timer = /* instrumentation_timer */ () ; // dev macro; noop stub ("git_status.branch_info")
        run_git(working_directory, &["rev-parse", "--abbrev-ref", "HEAD"])
    };

    match &branch_name {
        Some(branch) if branch == "HEAD" => {
            // Detached HEAD — get short commit hash
            if let Some(hash) = run_git(working_directory, &["rev-parse", "--short", "HEAD"]) {
                let _ = writeln!(output, "HEAD detached at {}", hash);
            }
        }
        Some(branch) => {
            let _ = writeln!(output, "On branch {}", branch);
        }
        None => {
            return Err(FsError::Other("not a git repository".to_string()));
        }
    }

    // Get upstream ahead/behind
    {
        let _timer = (); // instrumentation_timer noop stub
        if let Some(upstream_name) = run_git(
            working_directory,
            &["rev-parse", "--abbrev-ref", "@{upstream}"],
        ) && let Some(counts) = run_git(
            working_directory,
            &["rev-list", "--count", "--left-right", "@{upstream}...HEAD"],
        ) {
            let parts: Vec<&str> = counts.split_whitespace().collect();
            if let (Some(behind_str), Some(ahead_str)) = (parts.first(), parts.get(1)) {
                let behind: usize = behind_str.parse().unwrap_or(0);
                let ahead: usize = ahead_str.parse().unwrap_or(0);

                let status_msg = match (ahead, behind) {
                    (0, 0) => {
                        format!("Your branch is up to date with '{}'.", upstream_name)
                    }
                    (a, 0) => format!(
                        "Your branch is ahead of '{}' by {} commit{}.",
                        upstream_name,
                        a,
                        if a == 1 { "" } else { "s" }
                    ),
                    (0, b) => format!(
                        "Your branch is behind '{}' by {} commit{}.",
                        upstream_name,
                        b,
                        if b == 1 { "" } else { "s" }
                    ),
                    (a, b) => format!(
                        "Your branch and '{}' have diverged ({} ahead, {} behind).",
                        upstream_name, a, b
                    ),
                };
                let _ = writeln!(output, "{}", status_msg);
            }
        }
    }

    // Get staged changes (index vs HEAD) — fast, no workdir scan
    let staged_output = {
        let _timer = /* instrumentation_timer */ () ; // dev macro; noop stub ("git_status.staged")
        run_git(
            working_directory,
            &["diff", "--cached", "--name-status", "HEAD"],
        )
    };

    let mut staged: Vec<String> = Vec::new();
    if let Some(ref diff_output) = staged_output {
        for line in diff_output.lines() {
            let mut parts = line.splitn(2, '\t');
            let status_char = parts.next().unwrap_or("");
            let path = parts.next().unwrap_or("");
            if path.is_empty() {
                continue;
            }
            let formatted = match status_char.chars().next() {
                Some('A') => format!("\tnew file: {}", path),
                Some('M') => format!("\tmodified: {}", path),
                Some('D') => format!("\tdeleted: {}", path),
                Some('R') => format!("\trenamed: {}", path),
                _ => format!("\t{}: {}", status_char, path),
            };
            staged.push(formatted);
        }
    }

    // Check if clean
    if staged.is_empty() {
        let _ = writeln!(output, "\nnothing to commit, working tree clean");
        return Ok(output);
    }

    // Reserve space for truncation message
    let reserve_for_truncation = 50;
    let char_budget = max_status_chars - reserve_for_truncation;

    // Write staged files
    if !staged.is_empty() && output.len() < char_budget {
        let _ = writeln!(output, "\nChanges to be committed:");
        for (shown, line) in staged.iter().enumerate() {
            if output.len() + line.len() + 1 > char_budget {
                let remaining = staged.len() - shown;
                if remaining > 0 {
                    let _ = writeln!(output, "\t... and {} more staged", remaining);
                }
                break;
            }
            let _ = writeln!(output, "{}", line);
        }
    }

    Ok(output)
}

/// Run a read-only git command and return its stdout, trimmed.
/// Returns None on failure.
///
/// Uses `--no-optional-locks` to avoid creating `index.lock` for stat-cache
/// refreshes.  This function is called from background tasks (system prompt
/// generation) and must never contend with foreground git operations.
fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = kimix_tty_utils::git_command()
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_buffer_cap_matches_spec() {
        assert!(!git_status_exceeds_buffer(0));
        assert!(!git_status_exceeds_buffer(GIT_STATUS_BUFFER_LIMIT - 1));
        // At or above 1 MiB -> dropped.
        assert!(git_status_exceeds_buffer(GIT_STATUS_BUFFER_LIMIT));
        assert!(git_status_exceeds_buffer(GIT_STATUS_BUFFER_LIMIT + 1));
    }

    /// Staged entries collapse the porcelain double space, while leading
    /// single spaces and ` -> ` are preserved.
    #[test]
    fn collapse_status_spaces_matches_spec() {
        let raw = "## main...origin/main\n M committed.txt\nA  staged.txt\nM  mod.txt\nR  old.txt -> new.txt\n?? untracked.txt\n";
        let want = "## main...origin/main\n M committed.txt\nA staged.txt\nM mod.txt\nR old.txt -> new.txt\n?? untracked.txt\n";
        assert_eq!(collapse_status_spaces(raw), want);
    }

    /// Newlines are never collapsed (blank lines preserved).
    #[test]
    fn collapse_status_spaces_preserves_newlines() {
        assert_eq!(collapse_status_spaces("a\n\n\nb"), "a\n\n\nb");
        assert_eq!(collapse_status_spaces(""), "");
    }

    #[test]
    fn prompt_git_cache_entry_freshness() {
        let now = Instant::now();
        let fresh = PromptGitCacheEntry {
            value: "ok".into(),
            cached_at: now,
        };
        assert!(fresh.is_fresh(now));
        assert!(fresh.is_fresh(now + Duration::from_millis(500)));
        assert!(!fresh.is_fresh(now + PROMPT_GIT_STATUS_TTL + Duration::from_millis(1)));
    }

    #[test]
    fn prompt_git_cache_put_get_roundtrip() {
        clear_prompt_git_status_cache_for_test();
        let cwd = PathBuf::from("/tmp/kimix-prompt-git-cache-test");
        let out = prompt_git_cache_put(
            cwd.clone(),
            PromptGitKind::Compact,
            "## main\nnothing to commit".into(),
        );
        assert_eq!(out, "## main\nnothing to commit");
        assert_eq!(
            prompt_git_cache_get(&cwd, PromptGitKind::Compact).as_deref(),
            Some("## main\nnothing to commit")
        );
        // Different kind must not collide.
        assert!(prompt_git_cache_get(&cwd, PromptGitKind::Short).is_none());
        clear_prompt_git_status_cache_for_test();
    }

    /// Two sequential calls for the same cwd within TTL return identical
    /// bytes (cache hit). Skips when cwd is not a git worktree.
    #[tokio::test]
    async fn git_status_short_cache_hit_is_byte_identical() {
        clear_prompt_git_status_cache_for_test();
        let cwd = std::env::current_dir().expect("cwd");
        let a = match git_status_short(&cwd).await {
            Ok(s) => s,
            Err(_) => return, // not a git repo in this environment
        };
        let b = git_status_short(&cwd)
            .await
            .expect("second call must hit cache");
        assert_eq!(a, b);
        clear_prompt_git_status_cache_for_test();
    }
}
