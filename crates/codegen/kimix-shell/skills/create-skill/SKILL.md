---
name: create-skill
description: >
  Create or improve Kimix agent skills (SKILL.md + optional scripts/references).
  Use when the user wants to create a skill, scaffold a skill, improve an existing
  skill, optimize a skill description for triggering, or runs /create-skill.
metadata:
  short-description: "Create a new Kimix skill"
---

# Create Skill

Guide for authoring effective Kimix skills. Skills are directories with a `SKILL.md`
that teach the agent how to perform specialized tasks.

## Before You Begin: Gather Requirements

Collect (from conversation first; ask only what is still missing):

1. **Purpose and scope** — what specific workflow does this skill own?
2. **Target location** — project or user (see Storage Locations)
3. **Trigger scenarios** — when should the agent load it?
4. **Domain knowledge** — what does the agent not already know?
5. **Output format** — templates, checklists, or artifacts required?
6. **Existing patterns** — prior prompts, scripts, or examples to lock in?

### Verbatim text from the user

If the user includes exact wording for the skill, use it **verbatim** in `SKILL.md`
(same words, same order). Do not paraphrase or expand unrequested copy.

### Inferring from Context

If the conversation already shows a repeated workflow, extract steps, tools, and
corrections from history, then confirm gaps before drafting.

### Gathering Additional Information

Ask one question at a time as regular conversation (not structured option prompts
for free-text). For binary choices (scope, scripts yes/no), short options are fine.

---

## Skill File Structure

### Directory Layout

```
skill-name/
├── SKILL.md              # Required
├── references/           # Optional — load on demand
├── scripts/              # Optional — executable helpers
└── assets/               # Optional — templates/fixtures for output
```

### Storage Locations

| Type | Path | Scope |
|------|------|-------|
| Project (recommended in a git repo) | `<repo-root>/.kimix/skills/<name>/` | This repo; shareable |
| User | `~/.kimix/skills/<name>/` | All projects for this user |

Also valid for cross-harness sharing (import-friendly, not the default write target):

- `<repo-root>/.agents/skills/<name>/`
- `~/.agents/skills/<name>/`

**Do not write into** other products' managed roots (`~/.claude/skills`,
`~/.codex/skills`, `~/.cursor/skills-cursor`, `~/.grok/bundled/skills`) unless the
user explicitly asked for that product.

Resolve kimix home: `$KIMIX_HOME` when set, else `~/.kimix`.

### SKILL.md Structure

```markdown
---
name: your-skill-name
description: What it does and when to use it (WHAT + WHEN, third person)
---

# Your Skill Name

## Instructions
Step-by-step guidance for the agent.

## Examples
Concrete examples when quality depends on seeing them.
```

### Required Metadata Fields

| Field | Requirements | Purpose |
|-------|--------------|---------|
| `name` | 1–64 chars; lowercase `a-z`, digits, hyphens; start/end alnum; **must match directory name** | Identifier / slash command |
| `description` | Max 1024 chars; non-empty | Discovery and auto-invocation |

Optional: `metadata.short-description`, `license`, `compatibility`, `allowed-tools`.

---

## Writing Effective Descriptions

The `description` is the primary trigger. Models tend to under-trigger — be specific.

1. **Third person** — "Processes Excel files…" not "I can help…"
2. **WHAT + WHEN** — capabilities and trigger phrases the user would say
3. **Include the slash** — e.g. "Use when the user runs /deploy-k8s"
4. **Push slightly** — name synonyms and adjacent intents, not only the ideal phrasing

Examples:

```yaml
description: Extract text and tables from PDF files, fill forms, merge documents. Use when working with PDF files or when the user mentions PDFs, forms, or document extraction.

description: Generate descriptive commit messages by analyzing git diffs. Use when the user asks for help writing commit messages or reviewing staged changes.
```

Show the drafted description to the user and let them approve or edit before writing files.

---

## Core Authoring Principles

### 1. Concise is Key

The context window is shared. Default assumption: the agent is already smart.
Only add procedural knowledge it would not already have.

### 2. Keep SKILL.md Under 500 Lines

Put detail in `references/` and link it with clear "when to read" cues.

### 3. Progressive Disclosure

1. Metadata (name + description) — always available
2. SKILL.md body — on trigger
3. Bundled files — only when the body says to read/run them

Keep references **one level deep** from SKILL.md.

### 4. Degrees of Freedom

| Freedom | When | Example |
|---------|------|---------|
| High (text) | Multiple valid approaches | Code review guidelines |
| Medium (templates) | Preferred pattern | Report structure |
| Low (scripts) | Fragile, must be consistent | Migrations, packaging |

### 5. One capability per skill

If the user describes two loosely related jobs, propose splitting them.

---

## Common Patterns

### Template Pattern

Provide the exact output skeleton the agent should fill.

### Examples Pattern

Show input → output pairs when quality depends on seeing format.

### Workflow Pattern

Numbered steps + a copyable checklist for multi-step work.

### Conditional Workflow Pattern

Branch on create vs edit, or on artifact type, with explicit paths.

### Feedback Loop Pattern

Edit → validate script/command → fix → only proceed when validation passes.

---

## Anti-Patterns to Avoid

1. **Windows-style paths** — use `scripts/helper.py`, not `scripts\helper.py`
2. **Too many options without a default** — pick one default + escape hatch
3. **Time-sensitive instructions** as permanent rules — put legacy under "Old patterns"
4. **Inconsistent terminology** — one term per concept throughout
5. **Vague names** — `changelog-writer` not `helper` / `utils`
6. **README/INSTALL/CHANGELOG clutter** inside the skill — agent instructions only
7. **Writing foreign harness roots** without explicit user request

---

## Creation Workflow

### Phase 1: Discovery

Confirm purpose, scope path, triggers, constraints, and any verbatim copy.

### Phase 2: Design

1. Draft `name` (kebab-case, matches directory)
2. Draft third-person `description` (WHAT + WHEN + slash)
3. Outline body sections; decide scripts/references

### Phase 3: Implementation

1. Create the skill directory (`mkdir -p`)
2. Write `SKILL.md` (frontmatter + body)
3. Add scripts/references only when needed
4. If scripts exist, **run them once** to prove they work

### Phase 4: Verification

Before declaring done:

- [ ] `name` matches directory; description has WHAT + WHEN
- [ ] Third person; under 500 lines in SKILL.md
- [ ] References one level deep; no foreign-root writes
- [ ] Scripts tested when present
- [ ] Tell the user how to invoke: `/<name>`, skills menu, or auto-trigger

### Validation (lightweight)

If a project validator exists, run it. Otherwise at least:

```bash
test -f <SKILL_DIR>/SKILL.md && head -20 <SKILL_DIR>/SKILL.md
```

Re-read the written file and fix frontmatter/name mismatches before finishing.

### Forward-test (when non-trivial)

For non-trivial skills, optionally run a fresh subagent on 1–2 realistic prompts
**without** leaking expected answers. Prefer raw artifacts over your diagnosis.
Skip if the user wants a quick scaffold only.

---

## Complete Example

```
code-review/
├── SKILL.md
└── references/standards.md
```

```markdown
---
name: code-review
description: Review code for quality, security, and maintainability following team standards. Use when reviewing pull requests, examining code changes, or when the user asks for a code review or runs /code-review.
---

# Code Review

## Quick Start

1. Check correctness and edge cases
2. Verify security basics
3. Assess readability and tests

## Checklist

- [ ] Logic handles edge cases
- [ ] No obvious injection / XSS / secret leaks
- [ ] Style matches the project
- [ ] Tests cover the change

## Additional Resources

- Standards: [references/standards.md](references/standards.md)
```

---

## Summary Checklist

### Core Quality

- [ ] Description specific; WHAT + WHEN; third person
- [ ] SKILL.md under 500 lines; consistent terms
- [ ] Concrete examples where needed

### Structure

- [ ] Progressive disclosure; one-level references
- [ ] Clear workflow steps; no stale dated rules

### Scripts (if any)

- [ ] Solve a real problem; documented invoke line
- [ ] Explicit errors; no Windows-only paths
- [ ] Actually executed once successfully
