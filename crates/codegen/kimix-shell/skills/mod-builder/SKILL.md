---
name: mod-builder
description: >
  Build a Kimix extension ("mod") end to end — plugin, skill, hook, slash
  command, or MCP tool package. Use when the user wants to create, scaffold,
  port a Command Code mod, or add custom tools/hooks/slash commands to Kimix.
  Also /mod-builder.
---

# Mod Builder (Kimix)

Ship Kimix extensions the way Command Code ships mods: small, loadable units
that add **tools**, **hooks**, and **slash commands**. On Kimix the packaging
unit is a **plugin** (or a lone skill/hook). There is **no** TypeScript
`ModApi` — do not write `export default (cmd) => …` factories.

**Always read first:** `~/.kimix/docs/user-guide/25-building-extensions.md`  
Also as needed: `08-skills.md`, `09-plugins.md`, `10-hooks.md`, `07-mcp-servers.md`,
`22-permissions-and-safety.md`. Product orientation only → `/kimix-knowledge`.

## 1. Classify the request

| User wants | Deliver |
|------------|---------|
| One reusable procedure / `/name` prompt | Skill only (`SKILL.md`) |
| Block / log / notify on lifecycle events | Hook JSON (+ script) |
| New model-callable tool | MCP server (stdio/HTTP) ± plugin wrap |
| Several of the above as one package | **Plugin** (preferred "mod") |
| Standing project rules | AGENTS.md (not a mod) |
| Port a Command Code TS mod | Map ModApi verbs → table below, then implement |

Default scope: **user** (`~/.kimix/…`) for personal automation; **project**
(`.kimix/…`) when the team should share it. Ask once if unclear.

### Command Code → Kimix map

| Command Code | Kimix |
|--------------|--------|
| `cmd.addCommand` | Skill slash or `commands/*.md` |
| `cmd.addTool` | MCP tool in `.mcp.json` / `[mcp_servers.*]` |
| `cmd.hooks({ beforeToolCall })` | `PreToolUse` hook |
| `cmd.hooks({ afterToolCall })` | `PostToolUse` / `PostToolUseFailure` |
| `transformInput` | `UserPromptSubmit` (side effects; no prompt-rewrite API) |
| `onSessionStart` / `onSessionEnd` | `SessionStart` / `SessionEnd` |
| `cmd.on(event)` observers | Passive hooks / logging scripts |
| `cmd mods add` / `--mod` | `kimix plugin install` / `--plugin-dir` / plugins dir |

## 2. Scaffold

### A. Single skill

```
~/.kimix/skills/<name>/SKILL.md          # user
# or
<repo>/.kimix/skills/<name>/SKILL.md     # project
```

Frontmatter: `name` + strong `description` (triggers + `/name`). Body = agent
instructions. Prefer `/create-skill` for interactive authoring of skill-only work.

### B. Hooks only

```
~/.kimix/hooks/<id>.json
# or <repo>/.kimix/hooks/<id>.json  (needs folder trust)
```

### C. Plugin (preferred "mod")

```
~/.kimix/plugins/<name>/                 # or .kimix/plugins/<name>/
  plugin.json
  skills/<skill-name>/SKILL.md           # optional
  commands/<cmd>.md                      # optional slash markdown
  agents/<agent>.md                      # optional
  hooks/hooks.json                       # optional
  .mcp.json                              # optional
  bin/…                                  # hook scripts
```

Minimal `plugin.json`:

```json
{
  "name": "review-guard",
  "version": "0.1.0",
  "description": "Block risky shell and add /standup"
}
```

Name: kebab-case. User plugins under `~/.kimix/plugins/` are auto-trusted;
project plugins need trust.

```bash
kimix plugin validate ~/.kimix/plugins/<name>
kimix plugin install ~/.kimix/plugins/<name> --trust   # if needed
kimix plugin details <name>
kimix inspect
# session-only try:
kimix agent --no-leader --plugin-dir ./my-mod stdio
```

## 3. Implement surfaces

### Slash command (skill or `commands/`)

- **Skill slash:** `SKILL.md` with `user-invocable: true` (default) → `/name`
- **Legacy:** `commands/standup.md` → `/standup`
- Prefer SKILL.md for multi-step agent workflows

Example skill body pattern: tell the agent to run `git status -sb`,
`git log --since=midnight --oneline`, `git diff --stat`, then three bullets
(done / in-progress / blockers).

### Hook (mutating lifecycle)

Blocking only on **`PreToolUse`**: stdout `{"decision":"deny","reason":"…"}` or
exit `2`. Failures **fail-open** (timeouts/crashes do not block tools).

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|run_terminal_command",
        "hooks": [
          {
            "type": "command",
            "command": "bin/block-rm-rf.sh",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

Script: JSON on stdin; keep timeouts short; use `$KIMIX_PLUGIN_ROOT` /
`$KIMIX_PLUGIN_DATA` inside plugins. Matchers accept Claude aliases
(`Bash`, `Read`, `Edit`).

### Tool (MCP)

Kimix does not register arbitrary in-process tools from plugins. Ship tools as
an MCP server + `.mcp.json` / config. Confirm with `/mcps` or `kimix inspect`.
Skills steer existing tools; they do not add tool schemas.

### Observe-only

Logging / metrics → non-blocking hooks (`PostToolUse`, `SessionStart`, …) that
always allow. Do not use observe hooks when you need to block — use `PreToolUse`.

## 4. Verify what you built (do not skip)

A broken hook or plugin can **fail silently** (fail-open). Do not declare done
until every surface is exercised.

1. **Validate structure**
   ```bash
   kimix plugin validate <path>    # plugins
   # skills: SKILL.md exists, frontmatter name/description parse
   # hooks: JSON valid; script executable; matcher non-empty
   ```
2. **Confirm discovery**
   ```bash
   kimix inspect
   kimix plugin details <name>
   ```
   Skill/plugin/hook/MCP must appear; note load warnings.
3. **Exercise every surface**
   - **Slash:** type `/` — command appears with description; run it
   - **Hook:** trigger the guarded action; deny path shows reason; allow path runs
   - **MCP tool:** ask the model to call it by name
   - **UI:** `/plugins`, `/hooks`, `/skills` (or `Ctrl+L` on non–VS Code family)
4. **Iterate**
   - Skills usually hot-reload
   - Hooks/plugins may need Hooks tab reload or a new session
   - Session-only: `kimix agent --no-leader --plugin-dir ./my-mod stdio`
5. **Headless / CI when it matters**
   ```bash
   kimix -p "exercise the extension" --plugin-dir ./my-mod
   ```
   Design fail-open hooks and non-interactive defaults for CI.

Concrete deny test (block-rm-rf style): ask the agent to `rm -rf /tmp/scratch-dir`
and expect deny + reason; a different safe command must still run.

## 5. Recipes (minimal)

| Recipe | Pieces |
|--------|--------|
| `/standup` summary | Skill only |
| Block `rm -rf` / force-push | `PreToolUse` + script (plugin or global hook) |
| Log every shell command | `PostToolUse` matcher on `Bash\|run_terminal_command` |
| Custom tool (e.g. count TODOs) | MCP server + `.mcp.json` **or** skill that runs `rg` |
| Team package | Plugin: skill + hooks + optional MCP |

Full scaffolds and JSON examples: `25-building-extensions.md`.

## 6. Boundaries (state up front)

- **No sandbox for hooks/MCP** — treat project plugins like code review; use `--trust` deliberately
- **Fail-open** on crash/timeout — pair critical blocks with permission modes / sandbox (`22-…`)
- **No secrets** in committed project plugins; use env vars
- Prefer **one plugin** over scattering many global hooks for a team feature
- Keep scripts dependency-light (`bash` / `python3` / `jq`)
- Performance: narrow matchers, short timeouts, no N² work per tool call

## If the user only wants knowledge

Use `/kimix-knowledge` and the user-guide; **do not scaffold files** until they
ask to build something.
