//! Built-in files extracted to `~/.kimix/` on startup.

#[path = "bundled_skill_trees.rs"]
mod bundled_skill_trees;

const BUNDLED_FILES: &[(&str, &str)] = &[("README.md", include_str!("../README.md"))];

/// Bundled subagent personas (`~/.kimix/bundled/personas/*.toml`).
///
/// Discovered at lowest priority by `SubagentsConfig::resolve` (after project /
/// user personas). Referenced by the `task` tool `persona` parameter.
const BUNDLED_PERSONAS: &[(&str, &str)] = &[
    (
        "design-doc-reviewer.toml",
        include_str!("../bundled/personas/design-doc-reviewer.toml"),
    ),
    (
        "design-doc-writer.toml",
        include_str!("../bundled/personas/design-doc-writer.toml"),
    ),
    (
        "implementer.toml",
        include_str!("../bundled/personas/implementer.toml"),
    ),
    (
        "researcher.toml",
        include_str!("../bundled/personas/researcher.toml"),
    ),
    (
        "reviewer.toml",
        include_str!("../bundled/personas/reviewer.toml"),
    ),
    (
        "security-auditor.toml",
        include_str!("../bundled/personas/security-auditor.toml"),
    ),
    (
        "test-writer.toml",
        include_str!("../bundled/personas/test-writer.toml"),
    ),
];

const HELP_SKILL_MD: &str = include_str!("../skills/help/SKILL.md");
const CREATE_SKILL_MD: &str = include_str!("../skills/create-skill/SKILL.md");
const CODE_REVIEW_SKILL_MD: &str = include_str!("../skills/code-review/SKILL.md");
/// Compiled-in SKILL.md content for `/check-work` (available to headless mode).
pub const CHECK_SKILL_MD: &str = include_str!("../skills/check-work/SKILL.md");
/// Compiled-in SKILL.md content for headless `--best-of-n` (not extracted as
/// a bundled skill).
pub const BEST_OF_N_SKILL_MD: &str = include_str!("../skills/best-of-n/SKILL.md");
const KIMIX_KNOWLEDGE_SKILL_MD: &str = include_str!("../skills/kimix-knowledge/SKILL.md");
const MOD_BUILDER_SKILL_MD: &str = include_str!("../skills/mod-builder/SKILL.md");
const UI_DESIGN_SKILL_MD: &str = include_str!("../skills/ui-design/SKILL.md");
// Main engineering suite (product main-7).
const GIT_SKILL_MD: &str = include_str!("../skills/git/SKILL.md");
const PLAN_SKILL_MD: &str = include_str!("../skills/plan/SKILL.md");
const DESIGN_SKILL_MD: &str = include_str!("../skills/design/SKILL.md");
const IMPLEMENT_SKILL_MD: &str = include_str!("../skills/implement/SKILL.md");
const EXECUTE_PLAN_SKILL_MD: &str = include_str!("../skills/execute-plan/SKILL.md");

/// Multi-file bundled skill trees: skill dir name → (relative path, content).
/// Includes main-skill extras (scripts/references) and non-skill support trees
/// such as `shared/personas` used by design/implement/execute-plan.
const BUNDLED_SKILL_TREES: &[(&str, &[(&str, &str)])] = bundled_skill_trees::ALL_SKILL_TREES;

/// Legacy bundled skill names (renamed or removed).
///
/// These directories under `~/.kimix/skills/` will be deleted on startup
/// (during bundled file extraction). This ensures that when a bundled
/// skill is renamed (e.g. `check` → `check-work`), the old slash command
/// does not linger on users' machines after an upgrade.
///
/// Important behavior:
/// - Deletion happens **early** in `extract_bundled_files`, before we write
///   any current bundled skills.
/// - We **never** delete a name that is currently present in `BUNDLED_SKILLS`
///   (see `remove_legacy_bundled_skills`).
///
/// This means:
/// - If you later re-introduce a skill with a name that is still in this
///   legacy list (e.g. you ship a new "check" skill years later), the legacy
///   cleanup will **skip** it and the new skill will be created normally.
/// - The legacy list is a "delete old user copies of names we no longer ship",
///   not a permanent blacklist.
///
/// Lifecycle / maintenance:
/// - Add an old name here when you rename/remove a bundled skill.
/// - Once the directory is gone on a user's machine, further checks are
///   cheap no-ops.
/// - You do **not** have to remove entries immediately. It is safe to leave
///   them for many releases.
/// - After the rename has had time to propagate, you **may** clean old
///   strings out of this list for hygiene.
const LEGACY_BUNDLED_SKILL_NAMES: &[&str] =
    &["check", "best-of-n", "docx", "pptx", "xlsx", "imagine"];

/// All bundled skill SKILL.md files. Single source of truth used by both
/// the full extraction path (version bump) and the missing-file fast path
/// (same version). Adding a new skill here is all that's needed.
///
/// When renaming a bundled skill (e.g. "check" → "check-work"), also add the
/// old name to `LEGACY_BUNDLED_SKILL_NAMES` so `remove_legacy_bundled_skills`
/// will clean up the old directory on user machines on the next upgrade.
///
/// See the docs on `LEGACY_BUNDLED_SKILL_NAMES` for the full lifecycle
/// (including when it is safe/optional to remove old entries later).
const BUNDLED_SKILLS: &[(&str, &str)] = &[
    // Product main-7 (engineering collaboration loop)
    ("git", GIT_SKILL_MD),
    ("plan", PLAN_SKILL_MD),
    ("design", DESIGN_SKILL_MD),
    ("implement", IMPLEMENT_SKILL_MD),
    ("execute-plan", EXECUTE_PLAN_SKILL_MD),
    ("code-review", CODE_REVIEW_SKILL_MD),
    ("create-skill", CREATE_SKILL_MD),
    // Reserve / platform
    ("ui-design", UI_DESIGN_SKILL_MD),
    ("help", HELP_SKILL_MD),
    ("check-work", CHECK_SKILL_MD),
    ("kimix-knowledge", KIMIX_KNOWLEDGE_SKILL_MD),
    ("mod-builder", MOD_BUILDER_SKILL_MD),
];

/// True when a discovered skill is the copy `extract_bundled_files` wrote to
/// `<kimix_home>/skills/<name>/SKILL.md`. Exact-path (not prefix) so a
/// user-authored skill that reuses a bundled name — even elsewhere under
/// `<kimix_home>/skills/` — is never labeled bundled. Lives beside the
/// extraction code so the target layout and this predicate move together.
/// Used by inspect, which otherwise sees extracted copies as user skills.
pub(crate) fn is_extracted_bundled_skill(
    name: &str,
    path: &std::path::Path,
    kimix_home: &std::path::Path,
) -> bool {
    BUNDLED_SKILLS.iter().any(|&(n, _)| n == name)
        && path == kimix_home.join("skills").join(name).join("SKILL.md")
}

/// Resolve the content for a skill, applying any name-specific transforms.
fn resolve_skill_content(name: &str, raw: &str, kimix_home: &std::path::Path) -> String {
    match name {
        // Docs-router skills reference `~/.kimix/…`; expand so absolute paths
        // work when KIMIX_HOME / custom home is not the default.
        "help" | "kimix-knowledge" | "mod-builder" => {
            let kimix_home_str = format!("{}/", kimix_home.to_string_lossy());
            raw.replace("~/.kimix/", &kimix_home_str)
        }
        _ => raw.to_string(),
    }
}

/// Extract bundled files to `~/.kimix/` on startup.
///
/// Full extraction runs on every version bump. On same-version startups,
/// a lightweight check ensures all expected skill files exist on disk —
/// any missing files are extracted individually.
///
/// Legacy/renamed bundled skills (see `LEGACY_BUNDLED_SKILL_NAMES`) are
/// always cleaned up first so that old slash commands disappear after
/// a rename (e.g. the previous `/check` after the move to `/check-work`).
pub fn extract_bundled_files(kimix_home: &std::path::Path) {
    // Always remove legacy/renamed bundled skills first (e.g. the old
    // `check` directory after the rename to `check-work`). This runs on
    // every startup so users get cleaned up even without hitting a
    // version-bump marker change.
    remove_legacy_bundled_skills(kimix_home);

    // Personas are small and must exist even on the same-version fast path
    // (users upgrading mid-version still get native `task.persona` support).
    extract_bundled_personas(kimix_home, /* only_missing */ false);

    let version = kimix_version::VERSION;
    let marker = kimix_home.join(".metadata_version");

    if let Ok(existing) = std::fs::read_to_string(&marker)
        && existing.trim() == version
    {
        // Same version — only extract skill files that are missing on disk.
        // This handles skills added between version bumps.
        extract_missing_skills(kimix_home);
        return;
    }

    let _ = std::fs::create_dir_all(kimix_home);

    // Clean up changelog caches written by the removed changelog feature
    // (Kimix <= 0.1.0 cached CDN release notes in the Kimix home).
    for stale in &["CHANGELOG.json", "CHANGELOG.md"] {
        let _ = std::fs::remove_file(kimix_home.join(stale));
    }

    for &(filename, content) in BUNDLED_FILES {
        if let Err(e) = std::fs::write(kimix_home.join(filename), content) {
            tracing::debug!(error = %e, filename, "Failed to extract bundled file");
        }
    }

    // Skill SKILL.md files.
    for &(name, raw) in BUNDLED_SKILLS {
        let skill_dir = kimix_home.join("skills").join(name);
        let _ = std::fs::create_dir_all(&skill_dir);
        let content = resolve_skill_content(name, raw, kimix_home);
        if let Err(e) = std::fs::write(skill_dir.join("SKILL.md"), content) {
            tracing::debug!(error = %e, name, "Failed to write skill");
        }
    }

    // Multi-file skill trees (references/, assets/, …).
    extract_skill_trees(kimix_home, /* only_missing */ false);

    let _ = std::fs::write(&marker, version);
    tracing::debug!(version, "Extracted bundled files");
}

/// Write bundled persona TOML files under `{kimix_home}/bundled/personas/`.
///
/// Matches `bundle::bundled_root()` when `kimix_home` is the default
/// `~/.kimix`. Full extract overwrites; call with `only_missing` if needed.
fn extract_bundled_personas(kimix_home: &std::path::Path, only_missing: bool) {
    let dir = kimix_home.join("bundled").join("personas");
    let _ = std::fs::create_dir_all(&dir);
    for &(filename, content) in BUNDLED_PERSONAS {
        let path = dir.join(filename);
        if only_missing && path.exists() {
            continue;
        }
        if let Err(e) = std::fs::write(&path, content) {
            tracing::debug!(error = %e, filename, "Failed to write bundled persona");
        }
    }
}

/// Extract only missing skill SKILL.md files (same-version fast path).
/// Iterates `BUNDLED_SKILLS` so adding a new skill there is sufficient.
fn extract_missing_skills(kimix_home: &std::path::Path) {
    for &(name, raw) in BUNDLED_SKILLS {
        let skill_md = kimix_home.join("skills").join(name).join("SKILL.md");
        if skill_md.exists() {
            continue;
        }
        let _ = std::fs::create_dir_all(skill_md.parent().unwrap());
        let content = resolve_skill_content(name, raw, kimix_home);
        let _ = std::fs::write(&skill_md, content);
    }
    extract_skill_trees(kimix_home, /* only_missing */ true);
}

/// Write multi-file bundled skill contents under `skills/<name>/`.
/// When `only_missing` is true, skip files that already exist (same-version
/// fast path). Full version-bump extract passes false to overwrite.
fn extract_skill_trees(kimix_home: &std::path::Path, only_missing: bool) {
    for &(name, files) in BUNDLED_SKILL_TREES {
        let base = kimix_home.join("skills").join(name);
        for &(rel, raw) in files {
            let path = base.join(rel);
            if only_missing && path.exists() {
                continue;
            }
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let content = resolve_skill_content(name, raw, kimix_home);
            if let Err(e) = std::fs::write(&path, content) {
                tracing::debug!(error = %e, name, rel, "Failed to write skill tree file");
            }
        }
    }
}

/// Remove directories for legacy/renamed bundled skills (e.g. old `check`
/// after it was renamed to `check-work`).
///
/// Called on every startup from `extract_bundled_files`. Safe and idempotent.
///
/// Key guarantees (see `LEGACY_BUNDLED_SKILL_NAMES` docs for details):
/// - If a name is still present in `BUNDLED_SKILLS`, we deliberately skip
///   deletion. This allows safe re-use of a skill name in the future.
/// - If the target directory no longer exists, this is a trivial no-op.
fn remove_legacy_bundled_skills(kimix_home: &std::path::Path) {
    remove_legacy_skills(kimix_home, LEGACY_BUNDLED_SKILL_NAMES, BUNDLED_SKILLS);
}

/// Core implementation, extracted for testability.
fn remove_legacy_skills(
    kimix_home: &std::path::Path,
    legacy_names: &[&str],
    bundled_skills: &[(&str, &str)],
) {
    for name in legacy_names {
        // Safety: Never delete a name that we are currently shipping.
        // This protects against re-introducing a skill name that still has
        // an entry in the legacy list.
        if bundled_skills.iter().any(|(n, _)| *n == *name) {
            continue;
        }

        let dir = kimix_home.join("skills").join(name);
        if dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                tracing::debug!(error = %e, name, "Failed to remove legacy bundled skill");
            } else {
                tracing::debug!(name, "Removed legacy bundled skill directory");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_bump_re_extracts_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        extract_bundled_files(home);

        for &(filename, _) in BUNDLED_FILES {
            std::fs::write(home.join(filename), "old").unwrap();
        }
        std::fs::write(home.join("skills/help/SKILL.md"), "old").unwrap();
        for name in ["check-work", "code-review"] {
            std::fs::write(home.join(format!("skills/{name}/SKILL.md")), "old").unwrap();
        }
        std::fs::write(home.join(".metadata_version"), "0.0.0-stale").unwrap();

        // Simulate legacy skills that should be cleaned up.
        for name in ["check", "best-of-n", "docx", "pptx", "xlsx", "imagine"] {
            std::fs::create_dir_all(home.join(format!("skills/{name}"))).unwrap();
            std::fs::write(
                home.join(format!("skills/{name}/SKILL.md")),
                "old legacy skill",
            )
            .unwrap();
        }

        extract_bundled_files(home);

        for &(filename, _) in BUNDLED_FILES {
            assert_ne!(
                std::fs::read_to_string(home.join(filename)).unwrap(),
                "old",
                "{filename} was not re-extracted after version bump"
            );
        }
        assert_ne!(
            std::fs::read_to_string(home.join("skills/help/SKILL.md")).unwrap(),
            "old"
        );
        for name in ["check-work", "code-review"] {
            assert_ne!(
                std::fs::read_to_string(home.join(format!("skills/{name}/SKILL.md"))).unwrap(),
                "old",
                "{name} skill was not re-extracted after version bump"
            );
        }

        // Legacy skill directories must have been removed (the key part of
        // supporting renames like check → check-work without leaving orphans).
        for name in ["check", "best-of-n", "docx", "pptx", "xlsx", "imagine"] {
            assert!(
                !home.join(format!("skills/{name}")).exists(),
                "legacy '{name}' skill directory should have been deleted during version bump"
            );
        }
    }

    #[test]
    fn office_skills_not_bundled() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        extract_bundled_files(home);

        // Former office document skills must NOT be extracted as bundled.
        for name in ["docx", "pptx", "xlsx"] {
            assert!(
                !home.join(format!("skills/{name}")).exists(),
                "{name} should not be a bundled skill"
            );
        }
    }

    #[tokio::test]
    async fn help_skill_discovered_by_skill_pipeline() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        extract_bundled_files(home);

        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(workspace.join(".kimix").join("skills").join("help")).unwrap();
        std::fs::copy(
            home.join("skills/help/SKILL.md"),
            workspace.join(".kimix/skills/help/SKILL.md"),
        )
        .unwrap();

        let skills = kimix_agent::prompt::skills::list_skills(
            Some(workspace.to_str().unwrap()),
            &Default::default(),
            kimix_agent::prompt::skills::CompatConfig::default(),
        )
        .await;

        let help = skills.iter().find(|s| s.name == "help");
        assert!(
            help.is_some(),
            "help skill not found. skills: {:?}",
            skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        let help = help.unwrap();
        assert!(help.description.contains("configuration"));
        assert!(help.user_invocable);
    }

    // ---------------------------------------------------------------------
    // Tests for legacy bundled skill removal (the rename migration system)
    // ---------------------------------------------------------------------

    #[test]
    fn remove_legacy_deletes_old_skill_when_not_currently_shipped() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        // Simulate an old legacy "check" directory from before a rename.
        let legacy_dir = home.join("skills/check");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("SKILL.md"), "old check").unwrap();

        // "check" is in legacy list but NOT in current BUNDLED_SKILLS
        remove_legacy_skills(home, &["check"], BUNDLED_SKILLS);

        assert!(
            !legacy_dir.exists(),
            "legacy skill directory should have been deleted"
        );
    }

    #[test]
    fn remove_legacy_does_not_delete_when_name_is_reused_in_current_bundled() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        // User still has an old "check" directory.
        let legacy_dir = home.join("skills/check");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("SKILL.md"), "user had old check").unwrap();

        // Simulate the situation where we later re-ship a skill named "check".
        // In this case the legacy entry should be ignored.
        let fake_bundled: &[(&str, &str)] = &[("check", "fake content"), ("help", "help")];

        remove_legacy_skills(home, &["check"], fake_bundled);

        // The directory must still exist (we did not nuke the user's copy
        // or a skill we're about to (re)create).
        assert!(
            legacy_dir.exists(),
            "should not delete a name that is currently being shipped"
        );
    }

    #[test]
    fn remove_legacy_handles_multiple_names_some_current_some_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        std::fs::create_dir_all(home.join("skills/old-renamed")).unwrap();
        std::fs::write(home.join("skills/old-renamed/SKILL.md"), "old").unwrap();

        std::fs::create_dir_all(home.join("skills/another-legacy")).unwrap();
        std::fs::write(home.join("skills/another-legacy/SKILL.md"), "old2").unwrap();

        // Current bundled skills include one name that used to be legacy
        let current: &[(&str, &str)] = &[("another-legacy", "now shipping again")];

        // Legacy list contains both the truly removed one and the reintroduced one
        remove_legacy_skills(home, &["old-renamed", "another-legacy"], current);

        assert!(
            !home.join("skills/old-renamed").exists(),
            "truly legacy name should be removed"
        );
        assert!(
            home.join("skills/another-legacy").exists(),
            "reintroduced name must not be deleted"
        );
    }

    #[test]
    fn remove_legacy_is_noop_when_directory_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        // No directory exists for the legacy name
        remove_legacy_skills(home, &["check"], BUNDLED_SKILLS);

        // Should not panic or create anything
        assert!(!home.join("skills/check").exists());
    }

    #[test]
    fn legacy_cleanup_runs_even_on_same_version_fast_path() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        // First run: extract current state
        extract_bundled_files(home);

        // Simulate user still having an old legacy directory
        let legacy_dir = home.join("skills/check");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("SKILL.md"), "stale").unwrap();

        // Force the "same version" fast path by writing the current version marker
        let version = kimix_version::VERSION;
        std::fs::write(home.join(".metadata_version"), version).unwrap();

        // This should still run legacy cleanup even though we're in fast path
        extract_bundled_files(home);

        assert!(
            !legacy_dir.exists(),
            "legacy cleanup must run even on same-version fast path"
        );
    }

    #[test]
    fn docs_router_skills_expand_kimix_home_placeholder() {
        let home = std::path::Path::new("/custom/kimix-home");
        for name in ["help", "kimix-knowledge", "mod-builder"] {
            let out = resolve_skill_content(
                name,
                "read ~/.kimix/docs/user-guide/25-building-extensions.md and ~/.kimix/config.toml",
                home,
            );
            assert!(
                out.contains("/custom/kimix-home/docs/user-guide/25-building-extensions.md"),
                "{name}: expected expanded docs path, got {out}"
            );
            assert!(
                out.contains("/custom/kimix-home/config.toml"),
                "{name}: expected expanded config path, got {out}"
            );
            assert!(
                !out.contains("~/.kimix/"),
                "{name}: placeholder left unexpanded: {out}"
            );
        }
        // Other skills keep the literal placeholder (no rewrite).
        let plain = resolve_skill_content("create-skill", "path ~/.kimix/skills/x", home);
        assert_eq!(plain, "path ~/.kimix/skills/x");
    }

    #[test]
    fn extract_ships_kimix_knowledge_and_mod_builder() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract_bundled_files(home);

        for name in ["kimix-knowledge", "mod-builder"] {
            let path = home.join(format!("skills/{name}/SKILL.md"));
            assert!(path.exists(), "{name} must be extracted as a bundled skill");
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(
                body.contains("25-building-extensions.md") || body.contains("mod-builder"),
                "{name} skill body looks empty or wrong"
            );
            // Extracted copy must use the real home path, not the tilde form.
            let home_prefix = format!("{}/", home.to_string_lossy());
            assert!(
                body.contains(&home_prefix) || !body.contains("~/.kimix/"),
                "{name}: docs-router paths should expand at extract time"
            );
            assert!(is_extracted_bundled_skill(name, &path, home));
        }
    }

    #[test]
    fn same_version_fast_path_extracts_missing_new_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract_bundled_files(home);

        // Pretend an older binary never shipped these two.
        for name in ["kimix-knowledge", "mod-builder"] {
            std::fs::remove_dir_all(home.join(format!("skills/{name}"))).unwrap();
        }
        let version = kimix_version::VERSION;
        std::fs::write(home.join(".metadata_version"), version).unwrap();

        extract_bundled_files(home);

        for name in ["kimix-knowledge", "mod-builder"] {
            assert!(
                home.join(format!("skills/{name}/SKILL.md")).exists(),
                "same-version path must backfill missing bundled skill {name}"
            );
        }
    }

    #[test]
    fn extract_ships_ui_design_with_references() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract_bundled_files(home);

        let skill_md = home.join("skills/ui-design/SKILL.md");
        assert!(skill_md.exists(), "ui-design SKILL.md must extract");
        let body = std::fs::read_to_string(&skill_md).unwrap();
        assert!(
            body.contains("name: ui-design") || body.contains("# UI Design"),
            "ui-design body looks wrong"
        );
        assert!(
            body.contains("Name boundary") || body.contains("taste-filter"),
            "ui-design should carry rename + taste absorb markers"
        );
        assert!(
            home.join("skills/ui-design/references/taste-filter.md")
                .exists(),
            "ui-design taste-filter reference must extract"
        );
        assert!(
            home.join("skills/ui-design/references/create.md").exists(),
            "ui-design create reference must extract"
        );
        assert!(is_extracted_bundled_skill(
            "ui-design",
            &skill_md,
            home
        ));
    }

    #[test]
    fn extract_ships_main7_engineering_suite() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract_bundled_files(home);

        for name in [
            "git",
            "plan",
            "design",
            "implement",
            "execute-plan",
            "code-review",
            "create-skill",
        ] {
            let path = home.join(format!("skills/{name}/SKILL.md"));
            assert!(path.exists(), "main-7 skill {name} must extract");
            assert!(is_extracted_bundled_skill(name, &path, home));
        }

        // Multi-file extras + shared personas for orchestrator skills
        assert!(
            home.join("skills/git/scripts/workspace-recovery.sh").exists(),
            "git recovery script"
        );
        assert!(
            home.join("skills/implement/scripts/memory.py").exists(),
            "implement memory helper"
        );
        assert!(
            home.join("skills/execute-plan/scripts/validate-plan.py")
                .exists(),
            "execute-plan validator"
        );
        assert!(
            home.join("skills/shared/personas/implementer.md").exists(),
            "shared personas for design/implement/execute-plan"
        );
        assert!(
            home.join("skills/shared/personas/design-doc-writer.md")
                .exists(),
            "design-doc-writer persona"
        );

        // Runtime personas (path A): TOML under bundled/personas
        for name in [
            "implementer",
            "reviewer",
            "security-auditor",
            "design-doc-writer",
            "design-doc-reviewer",
            "researcher",
            "test-writer",
        ] {
            let path = home.join(format!("bundled/personas/{name}.toml"));
            assert!(path.exists(), "bundled persona {name}.toml must extract");
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(
                body.contains("instructions"),
                "persona {name} must have instructions"
            );
        }

        let design = std::fs::read_to_string(home.join("skills/design/SKILL.md")).unwrap();
        assert!(
            design.contains("Name boundary") || design.contains("ui-design"),
            "design skill should distinguish architecture from ui-design"
        );
        assert!(
            design.contains("task") && !design.contains("spawn_subagent"),
            "design skill should use kimix task tool, not grok spawn_subagent"
        );
        assert!(
            design.contains("task.persona") || design.contains("persona:"),
            "design skill should use native task.persona (path A)"
        );
        let implement = std::fs::read_to_string(home.join("skills/implement/SKILL.md")).unwrap();
        assert!(
            !implement.contains("Do NOT pass a `persona` parameter"),
            "implement must not forbid task.persona"
        );
        assert!(
            implement.contains("persona: \"implementer\"")
                || implement.contains("task.persona"),
            "implement must instruct passing persona by name"
        );
    }
}
