// @generated — multi-file bundled skill trees. Regenerate when skill files change.
// Source: crates/codegen/kimix-shell/skills/

/// Files for skills/ui-design/
pub const UI_DESIGN_FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skills/ui-design/SKILL.md")),
    ("references/border.md", include_str!("../skills/ui-design/references/border.md")),
    ("references/button.md", include_str!("../skills/ui-design/references/button.md")),
    ("references/checkup.md", include_str!("../skills/ui-design/references/checkup.md")),
    ("references/color.md", include_str!("../skills/ui-design/references/color.md")),
    ("references/create.md", include_str!("../skills/ui-design/references/create.md")),
    ("references/design-html.md", include_str!("../skills/ui-design/references/design-html.md")),
    ("references/deslop.md", include_str!("../skills/ui-design/references/deslop.md")),
    ("references/finish.md", include_str!("../skills/ui-design/references/finish.md")),
    ("references/interaction.md", include_str!("../skills/ui-design/references/interaction.md")),
    ("references/layout.md", include_str!("../skills/ui-design/references/layout.md")),
    ("references/motion.md", include_str!("../skills/ui-design/references/motion.md")),
    ("references/redesign.md", include_str!("../skills/ui-design/references/redesign.md")),
    ("references/refine.md", include_str!("../skills/ui-design/references/refine.md")),
    ("references/relayout.md", include_str!("../skills/ui-design/references/relayout.md")),
    ("references/report-html.md", include_str!("../skills/ui-design/references/report-html.md")),
    ("references/responsive.md", include_str!("../skills/ui-design/references/responsive.md")),
    ("references/review.md", include_str!("../skills/ui-design/references/review.md")),
    ("references/setup.md", include_str!("../skills/ui-design/references/setup.md")),
    ("references/shadow.md", include_str!("../skills/ui-design/references/shadow.md")),
    ("references/smell.md", include_str!("../skills/ui-design/references/smell.md")),
    ("references/surface.md", include_str!("../skills/ui-design/references/surface.md")),
    ("references/taste-filter.md", include_str!("../skills/ui-design/references/taste-filter.md")),
    ("references/tokenize.md", include_str!("../skills/ui-design/references/tokenize.md")),
    ("references/typeset.md", include_str!("../skills/ui-design/references/typeset.md")),
    ("references/voice.md", include_str!("../skills/ui-design/references/voice.md")),
    ("references/writing.md", include_str!("../skills/ui-design/references/writing.md")),
];

/// Files for skills/git/
pub const GIT_FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skills/git/SKILL.md")),
    ("scripts/workspace-recovery.sh", include_str!("../skills/git/scripts/workspace-recovery.sh")),
];

/// Files for skills/implement/
pub const IMPLEMENT_FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skills/implement/SKILL.md")),
    ("scripts/memory.py", include_str!("../skills/implement/scripts/memory.py")),
];

/// Files for skills/execute-plan/
pub const EXECUTE_PLAN_FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skills/execute-plan/SKILL.md")),
    ("scripts/validate-plan.py", include_str!("../skills/execute-plan/scripts/validate-plan.py")),
];

/// Files for skills/shared/
pub const SHARED_FILES: &[(&str, &str)] = &[
    ("personas/design-doc-reviewer.md", include_str!("../skills/shared/personas/design-doc-reviewer.md")),
    ("personas/design-doc-writer.md", include_str!("../skills/shared/personas/design-doc-writer.md")),
    ("personas/implementer.md", include_str!("../skills/shared/personas/implementer.md")),
    ("personas/reviewer.md", include_str!("../skills/shared/personas/reviewer.md")),
    ("personas/security-auditor.md", include_str!("../skills/shared/personas/security-auditor.md")),
];

/// (skill_dir_name, files)
pub const ALL_SKILL_TREES: &[(&str, &[(&str, &str)])] = &[
    ("ui-design", UI_DESIGN_FILES),
    ("git", GIT_FILES),
    ("implement", IMPLEMENT_FILES),
    ("execute-plan", EXECUTE_PLAN_FILES),
    ("shared", SHARED_FILES),
];

