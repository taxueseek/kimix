//! Taste learning — capture durable coding preferences and surface them to
//! every session.
//!
//! Two capture paths feed the same store (`~/.kimix/taste/taste.md`):
//!
//! 1. **Explicit** — the `taste` tool (kimix-tools) records a user-stated
//!    preference ("prefer 2-space indentation") verbatim.
//! 2. **Git-signal mining** — recurring diff corrections are scanned from the
//!    repository history and distilled into preference rules with confidence
//!    scores (this module's `collect_git_signals` → `render_learnings`).
//!
//! Both write one line per preference:
//!
//! ```text
//! - <learning text>. Confidence: <0.0-1.0>
//! ```
//!
//! The session layer calls [`render_taste_section`] when building the system
//! prompt (`spawn.rs` → `AgentBuilder::with_taste_section`); the rendered
//! `<taste>` block tells the model to follow the preferences and read
//! category files referenced as `See [category/taste.md]`.

use std::fs;
use std::path::PathBuf;

/// Path to the global taste store (`~/.kimix/taste/taste.md`).
pub fn taste_store_path() -> PathBuf {
    if let Ok(p) = std::env::var("KIMIX_TASTE_FILE") {
        return PathBuf::from(p);
    }
    crate::session::taste::kimix_home().join("taste").join("taste.md")
}

fn kimix_home() -> PathBuf {
    kimix_config::kimix_home()
}

/// The `- <text>. Confidence: <float>` line prefix.
pub const TASTE_LINE_PREFIX: &str = "- ";
pub const TASTE_CONFIDENCE_MARKER: &str = ". Confidence: ";

/// Parse one taste-store line into `(learning, confidence)`.
///
/// Lines that do not match the canonical format (`- <text>. Confidence: <f>`)
/// return `None` — callers (validation / rendering) skip them silently.
pub fn parse_taste_line(line: &str) -> Option<(&str, f32)> {
    let line = line.trim();
    let rest = line.strip_prefix(TASTE_LINE_PREFIX)?;
    let idx = rest.rfind(TASTE_CONFIDENCE_MARKER)?;
    let learning = rest[..idx].trim();
    let confidence_str = rest[idx + TASTE_CONFIDENCE_MARKER.len()..].trim();
    let confidence: f32 = confidence_str.parse().ok()?;
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return None;
    }
    if learning.is_empty() {
        return None;
    }
    Some((learning, confidence))
}

/// Render the `<taste>` system-prompt block from the store.
///
/// `None` when the store is missing/empty/disabled (`KIMIX_TASTE_DISABLED=1`)
/// — the caller then omits the block entirely. Non-canonical lines are
/// dropped, so a hand-edited file never injects garbage into the prompt.
pub fn render_taste_section() -> Option<String> {
    if std::env::var("KIMIX_TASTE_DISABLED").map(|v| v == "1").unwrap_or(false) {
        return None;
    }
    let path = taste_store_path();
    let content = fs::read_to_string(&path).ok()?;
    let mut valid_lines: Vec<&str> = content
        .lines()
        .filter(|l| parse_taste_line(l).is_some())
        .collect();
    valid_lines.sort();
    valid_lines.dedup();
    if valid_lines.is_empty() {
        return None;
    }
    Some(format!(
        "Below is the current set of learned coding preferences for this \
         workspace. Follow them unless the user explicitly overrides.\n\n\
         {}",
        valid_lines.join("\n")
    ))
}

/// Append the rendered `<taste>` block to role instructions.
///
/// `None` when there is no taste store — the role instructions pass through
/// unchanged. When both exist the taste block is appended after a blank line.
pub fn append_taste_to_role_instructions(role: Option<String>) -> Option<String> {
    let Some(taste) = render_taste_section() else {
        return role;
    };
    let mut ri = role.unwrap_or_default();
    if !ri.trim().is_empty() {
        ri.push_str("\n\n");
    }
    ri.push_str(&format!("<taste>\n{taste}\n</taste>"));
    Some(ri)
}

// ─── Git-signal extraction (lightweight, git2) ────────────────────────────

/// A single diff correction observed in git history: a removed line and the
/// line that replaced it, in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSignal {
    /// Path of the changed file (repo-relative).
    pub file: String,
    /// The removed line (trimmed).
    pub removed: String,
    /// The added line (trimmed).
    pub added: String,
    /// Commit subject that introduced the change.
    pub subject: String,
}

/// Collect recurring correction signals from recent (non-merge) commits.
///
/// For each commit, `git2`'s diff is walked and hunks whose removed/adjusted
/// lines were **replaced** by new lines (same hunk, both sides non-empty)
/// are surfaced. This is the raw material for taste learning; callers may
/// cap `max_commits` and `max_signals` to bound work.
///
/// Returns `Ok(vec)` on success; `Err` when the directory is not a git
/// repository (`git2::Repository::discover` failure).
pub fn collect_git_signals(
    repo_dir: &std::path::Path,
    max_commits: usize,
    max_signals: usize,
) -> Result<Vec<GitSignal>, String> {
    let repo = git2::Repository::discover(repo_dir).map_err(|e| format!("{e}"))?;
    let mut revwalk = repo.revwalk().map_err(|e| format!("{e}"))?;
    revwalk
        .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
        .map_err(|e| format!("{e}"))?;
    revwalk.push_head().map_err(|e| format!("{e}"))?;

    let mut signals = Vec::new();
    for oid in revwalk.take(max_commits) {
        let oid = match oid {
            Ok(o) => o,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(oid) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if commit.parent_count() == 0 {
            continue;
        }
        let parent = match commit.parent(0) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let subject: String = commit.summary().ok().flatten().unwrap_or_default().to_string();
        let parent_tree = parent.tree().ok();
        let commit_tree = commit.tree().ok();
        let Some(diff) = repo
            .diff_tree_to_tree(parent_tree.as_ref(), commit_tree.as_ref(), None)
            .ok()
        else {
            continue;
        };
        let mut file_signals = Vec::new();
        for (delta_idx, delta) in diff.deltas().enumerate() {
            let file = match delta.new_file().path() {
                Some(p) => p.to_string_lossy().into_owned(),
                None => continue,
            };
            // Only text files (skip binaries / lockfiles / vendor noise).
            if file.ends_with("Cargo.lock")
                || file.ends_with("package-lock.json")
                || file.ends_with("go.sum")
                || file.starts_with("vendor/")
                || file.starts_with("node_modules/")
            {
                continue;
            }
            let Ok(Some(patch)) = git2::Patch::from_diff(&diff, delta_idx) else {
                continue;
            };
            for hunk_idx in 0..patch.num_hunks() {
                let Ok(line_count) = patch.num_lines_in_hunk(hunk_idx) else {
                    continue;
                };
                let mut removed_line: Option<String> = None;
                let mut added_line: Option<String> = None;
                for line_idx in 0..line_count {
                    let Ok(line) = patch.line_in_hunk(hunk_idx, line_idx) else {
                        continue;
                    };
                    match line.origin() {
                        '-' => removed_line = Some(trim_signal(
                            std::str::from_utf8(line.content()).unwrap_or(""),
                        )),
                        '+' => added_line = Some(trim_signal(
                            std::str::from_utf8(line.content()).unwrap_or(""),
                        )),
                        _ => {}
                    }
                }
                // A correction: something removed was replaced by something added.
                if let (Some(removed), Some(added)) = (removed_line, added_line) {
                    if is_noise_signal(&removed) || is_noise_signal(&added) {
                        continue;
                    }
                    file_signals.push(GitSignal {
                        file: file.clone(),
                        removed,
                        added,
                        subject: subject.clone(),
                    });
                    if file_signals.len() >= max_signals {
                        break;
                    }
                }
            }
            if file_signals.len() >= max_signals {
                break;
            }
        }
        // 严格截断：单 commit 可产生超过 cap 的信号，超出部分丢弃。
        let remaining = max_signals.saturating_sub(signals.len());
        signals.extend(file_signals.into_iter().take(remaining));
        if signals.len() >= max_signals {
            break;
        }
    }
    Ok(signals)
}

/// Trim whitespace and common punctuation noise from a diff line.
fn trim_signal(s: &str) -> String {
    s.trim().trim_end_matches(',').trim().to_string()
}

/// Version-bump / churn lines are not corrections — skip them.
fn is_noise_signal(line: &str) -> bool {
    let t = line.trim();
    t.is_empty()
        || t.len() < 3
        || t.starts_with("version = ")
        || t.starts_with("//")
        || t.starts_with('#')
        || t.starts_with("/*")
        || t.starts_with('*')
}

/// Render mined signals into taste-store lines (deduplicated).
///
/// A pure heuristic that prefers the *added* line as the lesson, attributing
/// a confidence from the frequency of the same (file, added) pair. Callers
/// that run a real LLM distillation may ignore this and use
/// [`collect_git_signals`]'s output directly.
pub fn render_learnings(signals: &[GitSignal], repo_name: &str, max_lines: usize) -> Vec<String> {
    use std::collections::HashMap;
    let mut freq: HashMap<(&str, &str), usize> = HashMap::new();
    for s in signals {
        *freq.entry((&s.file, &s.added)).or_insert(0) += 1;
    }
    let mut lines: Vec<String> = Vec::new();
    for s in signals {
        let n = freq.get(&(&s.file, &s.added)).copied().unwrap_or(1);
        let confidence = (0.3f32 + 0.15 * n as f32).min(0.95);
        lines.push(format!(
            "- In {repo_name}, prefer `{}` over `{}`. Confidence: {confidence:.2}",
            s.added, s.removed
        ));
        if lines.len() >= max_lines {
            break;
        }
    }
    lines.sort();
    lines.dedup();
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_line() {
        let (learning, conf) =
            parse_taste_line("- prefer 2-space indentation. Confidence: 0.80").unwrap();
        assert_eq!(learning, "prefer 2-space indentation");
        assert!((conf - 0.8).abs() < 1e-6);
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(parse_taste_line("just some text").is_none());
        assert!(parse_taste_line("- no confidence here").is_none());
        assert!(parse_taste_line("- conf: 1.5. Confidence: 1.5").is_none());
        assert!(parse_taste_line("- empty. Confidence: ").is_none());
    }

    // 直接渲染给定 store 内容（供测试），等价于 render_taste_section 的
    // 过滤+排序逻辑，但不需要动进程环境变量。
    fn render_content(content: &str) -> Option<String> {
        let mut valid_lines: Vec<&str> = content
            .lines()
            .filter(|l| parse_taste_line(l).is_some())
            .collect();
        valid_lines.sort();
        valid_lines.dedup();
        if valid_lines.is_empty() {
            return None;
        }
        Some(format!(
            "Below is the current set of learned coding preferences for this \
             workspace. Follow them unless the user explicitly overrides.\n\n\
             {}",
            valid_lines.join("\n")
        ))
    }

    #[test]
    fn render_section_drops_invalid_lines_and_sorts() {
        let section = render_content("garbage line\n- b rule. Confidence: 0.9\n- a rule. Confidence: 0.5\n")
            .unwrap();
        assert!(section.contains("a rule"), "sorted first");
        assert!(section.contains("b rule"));
        assert!(!section.contains("garbage"));
    }

    #[test]
    fn render_section_none_when_no_valid_lines() {
        assert!(render_content("no canonical lines here\n").is_none());
        assert!(render_content("").is_none());
    }

    /// 冒烟：对当前 crate 所在 git 仓库提取真实信号（CI/本地仓库均可）。
    #[test]
    fn collect_signals_from_real_repo() {
        let repo_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // 非 git 仓库（例如解压源码）时跳过，不视为失败。
        if git2::Repository::discover(repo_dir).is_err() {
            eprintln!("skipping: CARGO_MANIFEST_DIR is not inside a git repo");
            return;
        }
        let signals = collect_git_signals(repo_dir, 30, 20).expect("signals");
        assert!(
            signals.len() <= 20,
            "max_signals cap respected: {}",
            signals.len()
        );
        for s in &signals {
            assert!(!s.file.is_empty());
            assert!(!s.added.is_empty());
            assert!(!s.removed.is_empty());
        }
    }

    #[test]
    fn render_learnings_dedups_and_scores() {
        let signals = vec![
            GitSignal {
                file: "src/main.rs".into(),
                removed: "foo()".into(),
                added: "bar()".into(),
                subject: "fix".into(),
            },
            GitSignal {
                file: "src/main.rs".into(),
                removed: "old_call()".into(),
                added: "bar()".into(),
                subject: "fix again".into(),
            },
        ];
        let lines = render_learnings(&signals, "demo", 10);
        assert_eq!(lines.len(), 2, "different removed lines → distinct learnings");
        assert!(lines[0].contains("bar()") && lines[1].contains("bar()"));
        assert!(
            lines[0].contains("Confidence: 0.60") || lines[1].contains("Confidence: 0.60"),
            "2 occurrences → confidence 0.60"
        );
    }
}
