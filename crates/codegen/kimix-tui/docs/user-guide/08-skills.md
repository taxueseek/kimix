# Skills

Skills are reusable prompt packages that extend Kimix with task-specific instructions. They let you capture a repeatable procedure once, instead of re-explaining it each session.

---

## What Are Skills?

A skill is a directory that contains a `SKILL.md` file. Its markdown body tells Kimix how to handle a specific type of task: step-by-step instructions, conventions, and tool-usage patterns.

Use a skill for a repeatable procedure that's too specific for AGENTS.md but too long to retype. Kimix activates a skill only when it applies to your current task.

---

## Skill Locations

Kimix discovers skills from these directories, in priority order:

| Location | Scope | Priority | Notes |
|----------|-------|----------|-------|
| `./.kimix/skills/`, `./.kimix/commands/` | Local (CWD) | Highest | Current directory skills / legacy command markdown |
| `<repo_root>/.kimix/skills/`, `…/commands/` | Repo | Medium | Shared across the repo |
| `~/.kimix/skills/`, `~/.kimix/commands/` | User | Lowest | Personal skills for all projects |
| `~/.claude/skills/`, `~/.claude/commands/` | User | Lowest | Claude Code compatibility (configurable) |
| `./.claude/skills/`, `./.claude/commands/` | Local / Repo | High | Project Claude skills and legacy custom slash commands |
| `~/.cursor/skills/` | User | Lowest | Cursor compatibility (configurable) |
| `./.cursor/skills/` | Local / Repo | High | Project Cursor skills (when cursor compat skills are enabled) |

Kimix deduplicates skills by name -- a higher-priority location overrides a lower one. Kimix also scans `.agents/skills/` (and `commands/`) at each tier (alongside `.kimix/`) and walks every directory between your working directory and the repo root.

Flat `*.md` files under a `commands/` directory become user-invocable slash commands (filename stem = command name), matching Claude Code's legacy custom-command layout.

Skill and command discovery does **not** use `.gitignore`. Paths under known skill roots (`.kimix/`, `.agents/`, `.claude/`, `.cursor/`) always load when present on disk — teams often ignore `.claude/**` as local-only config while still expecting `/frontend`-style project commands to work. To hide a skill, use `[skills] ignore` in config (not repo ignore rules).

Kimix scans the Claude and Cursor skill directories by default. To stop scanning a vendor, set its `skills` cell to `false` under `[compat.cursor]` or `[compat.claude]` in `~/.kimix/config.toml`, or set the `KIMIX_CURSOR_SKILLS_ENABLED` or `KIMIX_CLAUDE_SKILLS_ENABLED` environment variable to `false`. See [Configuration](05-configuration.md#harness-compatibility) for details. Kimix always filters out known vendor-shipped default skills (such as Cursor's `shell`, `canvas`, and `statusline`), regardless of these settings.

### Additional Skill Directories

Add directories, exclude paths, or disable individual skills via `[skills]` in `~/.kimix/config.toml`:

```toml
[skills]
paths = ["~/my-team-skills"]          # Additional directories to scan
ignore = ["~/my-team-skills/wip"]     # Paths to exclude (hidden entirely)
disabled = ["wip-skill"]              # Skill names to keep listed but inactive
```

Each entry in `paths` is a `SKILL.md` file or a directory that Kimix walks recursively. `ignore` hides a skill completely; `disabled` keeps it in the list but excludes it from the system prompt and from invocation. `paths` and `ignore` take filesystem paths and support `~` expansion; `disabled` takes skill names.

---

## Creating a Skill

### Directory Structure

Each skill lives in its own directory with a `SKILL.md` file:

```
~/.kimix/skills/
  commit/
    SKILL.md
  review-pr/
    SKILL.md
  deploy/
    SKILL.md
```

### SKILL.md Format

A skill file has YAML frontmatter followed by markdown instructions:

```markdown
---
name: commit
description: Create well-formatted git commits following conventional commit standards. Use when the user wants to commit changes or asks for /commit.
---

# Git Commit Skill

Review staged changes and create a commit with a clear, conventional message.

## Steps

1. Run `git diff --staged` to see changes
2. Summarize what changed and why
3. Create commit message following conventional commits format
4. Run `git commit -m "..."` with the message
```

### Core Frontmatter Fields

| Field | Description |
|-------|-------------|
| `name` | Skill identifier. Use lowercase letters, digits, and hyphens, up to 64 characters. Kimix normalizes spaces and underscores to hyphens. If you omit `name`, Kimix uses the skill's directory name. |
| `description` | What the skill does and when to use it. Kimix reads this to decide whether to invoke the skill. If you omit it, Kimix uses the first paragraph of the body. |

Write a specific `description`. It determines when Kimix invokes the skill automatically. Name the trigger phrases and use cases.

### Optional Frontmatter Fields

Multi-word frontmatter keys use kebab-case (single-word keys like `model` are written as-is).

| Field | Description |
|-------|-------------|
| `when-to-use` | Trigger phrases for automatic invocation, kept separate from `description`. |
| `allowed-tools` | Tools the skill uses, as a YAML list or a comma- or space-separated string. |
| `argument-hint` | Hint text shown in the slash-command autocomplete (for example, `commit message`). |
| `user-invocable` | Whether you can run the skill as a slash command. Defaults to `true`; set `false` to hide it from slash commands. (To stop the model from invoking a skill, set `disable-model-invocation` instead.) |
| `disable-model-invocation` | When `true`, only your slash command runs the skill -- the model cannot invoke it automatically. Defaults to `false`. |
| `model` | Model override for running the skill. |
| `effort` | Reasoning-effort override. |
| `license` | License identifier (for example, `Apache-2.0`). |
| `compatibility` | Environment requirements (for example, `Requires git, docker, jq`). |
| `metadata` | Arbitrary string key-value pairs. Kimix promotes `metadata.author` and `metadata.short-description` for display. |

---

## Creating Skills with /create-skill

The `/create-skill` command walks you through building a new skill interactively. Kimix asks what you want, drafts the files, and writes them to disk.

### How It Works

When you run `/create-skill`, Kimix:

1. **Gathers requirements.** Kimix asks for the skill name, the scope to save it under, and a description of the workflow you want to capture. Use a name with lowercase letters, digits, and hyphens (2–64 characters, starting and ending with a letter or digit).

2. **Drafts the description.** Kimix writes a `description` that states what the skill does, the phrases that trigger it, and the slash command name. You approve or edit the draft before continuing.

3. **Creates the skill directory.** Kimix creates the `<scope>/.kimix/skills/<name>/` directory, plus `scripts/` or `references/` subdirectories when the skill needs them.

4. **Writes SKILL.md.** Kimix writes the frontmatter (`name` and `description`) and a markdown body of instructions, along with any supporting files.

5. **Verifies and confirms.** Kimix reads the file back, confirms it wrote correctly, and tells you how to run the skill.

### Choosing a Scope

Kimix asks where to save the skill:

- **Project** (`<repo_root>/.kimix/skills/<name>/`) -- available only in this repository and shareable with teammates through version control. Kimix recommends this scope inside a git repository.
- **User** (`~/.kimix/skills/<name>/`) -- available across all your projects.

The new skill appears in the slash menu within a few seconds, because Kimix reloads skills when files change on disk.

---

## Using Skills

### Run a Skill by Name

Each skill is a slash command named after the skill. Run one by typing its name:

```
/commit              # Runs the "commit" skill
/review-pr           # Runs the "review-pr" skill
```

Running a skill loads its instructions into the conversation and directs the model to follow them. To pass arguments, type them after the name:

```
/commit fix the build
```

To browse your skills, type `/` to open the slash-command menu. Kimix lists every built-in command and skill and filters them as you type. To list skills from the command line instead, run `kimix inspect` (see [Viewing Skill Details](#viewing-skill-details)).

### Qualified Names

When a skill's name collides with another skill or a built-in command, Kimix advertises a qualified name prefixed by the skill's scope -- `local:`, `repo:`, `user:`, or the plugin name. Use the qualified form to choose a specific skill:

```
/local:commit        # The "commit" skill from ./.kimix/skills/
/user:commit         # The "commit" skill from ~/.kimix/skills/
```

### Automatic Invocation

Kimix can invoke a skill on its own when it recognizes a relevant task. Kimix matches your prompt against the skill's `description` and `when-to-use` fields, so write both to describe the triggering situation.

For example, if a skill's description says "Use when the user wants to commit changes," then saying "commit my changes" can trigger that skill automatically. To require an explicit slash command and prevent automatic invocation, set `disable-model-invocation: true` in the frontmatter.

---

## Viewing Skill Details

Run `kimix inspect` to see every skill Kimix discovers, along with the rest of your configuration:

```bash
kimix inspect          # Human-readable summary
kimix inspect --json   # Machine-readable report
```

In the human-readable output, the Skills section lists each skill's name and its source -- `project`, `user`, `bundled`, `config` (a `[skills].paths` entry), `server` (skills synced from the skill store in managed workspaces), or `plugin: <name>`. Kimix tags any skill disabled via `[skills].disabled` or from a disabled vendor surface with `[disabled]`.

The report honors your `[skills]` config the same way a live session does: skills from `paths` are listed, skills under an `ignore` prefix are hidden, and skills named in `disabled` stay listed but tagged `[disabled]`.

The `--json` report includes the full detail for each skill: its `name`, `description`, `source` (with the path to the SKILL.md file), and `userInvocable` flag.

---

## Bundled and Plugin Skills

Kimix ships with built-in skills and extracts them to `~/.kimix/skills/` on startup -- among them `/create-skill`, `/help`, `/check-work`, `/kimix-knowledge` (product map), and `/mod-builder` (author plugins, hooks, slash commands, MCP). Bundled skills behave like user skills, and a same-named skill in a higher-priority location (local or repo) overrides the bundled copy; `kimix inspect` labels the extracted copies `bundled` so they stay distinguishable from skills you authored yourself. (A plugin skill of the same name does not override it; it stays available under its qualified `plugin:name` form.)

Skills can also come from plugins. When you install a plugin that includes skills, they appear alongside your user and project skills. `kimix inspect` labels each plugin-provided skill with its source as `plugin: <name>`.

See the [Plugins guide](09-plugins.md) for more on installing plugins that provide skills. For end-to-end extension authoring (skills + hooks + MCP as one package), see [Building Extensions (Mods)](25-building-extensions.md).

---

## Best Practices

1. **Write specific descriptions.** The description drives automatic invocation. "Create git commits" is too vague; "Create well-formatted git commits following conventional commit standards. Use when the user wants to commit changes or asks for /commit." works better.

2. **Include concrete steps.** Skills work best when they give Kimix a clear, ordered procedure to follow.

3. **Reference tools by name.** When a skill relies on specific tools (such as `run_terminal_command` or `search_replace`), name them so the model knows what to use.

4. **Keep skills focused.** Write one skill per workflow. A "deploy" skill and a "rollback" skill work better than a single "deploy-and-rollback" skill.

5. **Version-control project skills.** Commit `.kimix/skills/` to your repository so the whole team benefits. User skills in `~/.kimix/skills/` stay personal and unshared.

6. **Test by running it.** Invoke `/name` and confirm the skill works before you rely on automatic invocation.
