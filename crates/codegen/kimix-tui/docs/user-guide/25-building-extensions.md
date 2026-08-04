# Building Extensions (Mods)

Kimix is a high-performance general-purpose AI agent. You extend it with
**skills**, **hooks**, **slash commands**, **MCP tools**, and **plugins** —
the same jobs Command Code "mods" cover (tools, hooks, slash commands), using
Kimix-native packaging instead of a TypeScript `ModApi`.

If you want the agent to write an extension for you, run **`/mod-builder`**.
For product orientation, run **`/kimix-knowledge`**.

---

## Mental model

| Goal | Mechanism | Typical path |
|------|-----------|--------------|
| Teach a workflow / add `/name` | Skill (`SKILL.md`) | `~/.kimix/skills/<name>/` or `<repo>/.kimix/skills/<name>/` |
| React to session or tool events | Hook JSON + command/HTTP | `~/.kimix/hooks/*.json` or project/plugin |
| New tools the model can call | MCP server | `config.toml` `[mcp_servers.*]` or plugin `.mcp.json` |
| Ship several pieces together | **Plugin** | `~/.kimix/plugins/<name>/` or `<repo>/.kimix/plugins/<name>/` |
| Standing project instructions | AGENTS.md | repo root / nested dirs |

A **plugin** is the closest equivalent to a Command Code mod package: one
directory that can bundle skills, commands, agents, hooks, MCP, and LSP.

There is **no** in-process `cmd.addTool()` / `cmd.hooks()` TypeScript host.
Hooks are external processes (or HTTP); tools are MCP (or skill-driven use of
built-in tools).

### Map from Command Code mods

| Command Code | Kimix |
|--------------|--------|
| `export default (cmd: ModApi) => { … }` | Plugin directory + `plugin.json` |
| `cmd.addTool` | MCP tool in `.mcp.json` / config |
| `cmd.addCommand` | Skill slash or `commands/*.md` |
| `cmd.hooks({ beforeToolCall })` | `PreToolUse` hook |
| `cmd.hooks({ afterToolCall })` | `PostToolUse` / `PostToolUseFailure` |
| `transformInput` | `UserPromptSubmit` (observe/side effects; no prompt rewrite API) |
| `onSessionStart` / `onSessionEnd` | `SessionStart` / `SessionEnd` |
| `cmd.on(event)` observers | Passive hooks / logging scripts |
| `cmd mods add` / `--mod` | `kimix plugin install` / `--plugin-dir` / drop into plugins dir |
| Trust for project mods | Folder trust + plugin `--trust` |

---

## Quick starts

### 1. Skill (slash + auto-invoke)

```markdown
---
name: standup
description: >
  Summarize today's git work for standup. Use when the user runs /standup
  or asks for a standup update.
---

# Standup

1. Run `git status -sb`, `git log --since=midnight --oneline`, and
   `git diff --stat`.
2. Summarize completed work, in-progress work, and blockers in three bullets.
```

Write to `~/.kimix/skills/standup/SKILL.md`. Invoke with `/standup`. Details:
[Skills](08-skills.md). Interactive authoring: `/create-skill`.

### 2. Hook (block dangerous shell)

`~/.kimix/hooks/block-rm-rf.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|run_terminal_command",
        "hooks": [
          {
            "type": "command",
            "command": "python3 -c \"import json,sys; e=json.load(sys.stdin); c=(e.get('toolInput') or {}).get('command') or '';\nimport re\n\nif re.search(r'rm\\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r)', c):\n  print(json.dumps({'decision':'deny','reason':'rm -rf blocked by block-rm-rf hook'}))\nelse:\n  print(json.dumps({'decision':'allow'}))\n\"",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

Prefer a real script file for anything beyond a one-liner. Full contract:
[Hooks](10-hooks.md).

### 3. Plugin (bundled mod)

```
my-mod/
  plugin.json
  skills/standup/SKILL.md
  hooks/hooks.json
  bin/safety-check.sh
  .mcp.json            # optional
```

`plugin.json`:

```json
{
  "name": "my-mod",
  "version": "0.1.0",
  "description": "Standup skill plus shell safety hook"
}
```

Convention directories load without path overrides. Optional manifest fields
(`skills`, `commands`, `agents`, `hooks`, `mcpServers`, `lspServers`) can
point at custom paths or inline JSON (hooks/MCP). See [Plugins](09-plugins.md).

```bash
kimix plugin validate ./my-mod
kimix plugin install ./my-mod --trust
kimix plugin details my-mod
kimix inspect
```

User-scope plugins under `~/.kimix/plugins/` are trusted automatically.
Project plugins under `.kimix/plugins/` require trust so a cloned repo cannot
run hooks/MCP until you allow it.

CLI session-only load:

```bash
kimix agent --no-leader --plugin-dir ./my-mod stdio
```

---

## Plugin layout reference

| Path | Role |
|------|------|
| `plugin.json` | Manifest (`name` required; kebab-case) |
| `skills/**/SKILL.md` | Skills (slash + model invoke) |
| `commands/*.md` | Legacy slash command markdown |
| `agents/*` | Agent / persona definitions |
| `hooks/hooks.json` | Lifecycle hooks |
| `.mcp.json` | MCP servers for this plugin |
| `.lsp.json` | LSP servers for this plugin |

Plugin hooks receive:

| Variable | Meaning |
|----------|---------|
| `KIMIX_PLUGIN_ROOT` | Absolute install path (read-only assets) |
| `KIMIX_PLUGIN_DATA` | Writable data dir (state, logs, caches) |

(`CLAUDE_PLUGIN_*` aliases exist for compatibility.)

---

## Hooks contract (extension authors)

- **Input:** JSON on stdin (`hookEventName`, `sessionId`, `cwd`, `toolName`,
  `toolInput`, …).
- **Blocking:** only `PreToolUse` can deny (`decision: deny` or exit `2`).
- **Fail-open:** timeouts, crashes, and missing env **do not** block tools;
  they log to the hooks UI. Design safety hooks carefully.
- **Matchers:** regex on tool name; Claude aliases (`Bash` →
  `run_terminal_command`, etc.) are accepted.
- **Timeouts:** keep short (default 5s). Long work belongs in background tools,
  not hooks.
- **Trust:** global `~/.kimix/hooks` always runs; project hooks need folder
  trust (`/hooks-trust` / `--trust`).

Events include `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
`PostToolUseFailure`, `PermissionDenied`, `Stop`, `StopFailure`,
`Notification`, `SubagentStart`, `SubagentStop`, `PreCompact`, `PostCompact`,
`SessionEnd`.

---

## Tools via MCP

To add **new** model-callable tools:

1. Implement an MCP server (stdio or HTTP) exposing tools with JSON schemas.
2. Register it in `~/.kimix/config.toml` under `[mcp_servers.<id>]`, or ship
   `.mcp.json` inside a plugin.
3. Confirm with `/mcps` or `kimix inspect`.

Skills cannot register new tool schemas; they only steer how the agent uses
existing tools. For "count TODOs in the repo", either:

- a skill that runs `rg` via `run_terminal_command`, or
- an MCP tool that wraps the same logic with a stable schema.

See [MCP Servers](07-mcp-servers.md).

---

## Slash commands

Sources that appear in the `/` menu:

1. Shell / TUI builtins (`/new`, `/compact`, …) — [Slash Commands](04-slash-commands.md)
2. Skills with `user-invocable: true`
3. `commands/*.md` under skill roots (`.kimix`, `.claude`, …)
4. Plugin-provided skills and commands

Custom command markdown is the lightweight form; skills are better for multi-step
agent workflows.

---

## Recipes

| Goal | Deliver |
|------|---------|
| `/standup` git summary | Skill only (`SKILL.md` with strong description) |
| Block `rm -rf` / force-push | `PreToolUse` hook + script (plugin or `~/.kimix/hooks`) |
| Log every shell command | `PostToolUse` matcher on `Bash\|run_terminal_command` |
| New model tool (e.g. count TODOs) | MCP server + `.mcp.json`, **or** skill that runs `rg` via shell |
| Team package | Plugin bundling skill + hooks + optional MCP |

### Observe-only hook skeleton

`~/.kimix/hooks/log-shell.json` — never blocks; good for telemetry:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Bash|run_terminal_command",
        "hooks": [
          {
            "type": "command",
            "command": "python3 -c \"import json,sys; e=json.load(sys.stdin); open('/tmp/kimix-shell.log','a').write(json.dumps(e.get('toolInput') or {})+'\\n')\"",
            "timeout": 3
          }
        ]
      }
    ]
  }
}
```

Prefer a real script under a plugin `bin/` for anything non-trivial.

### MCP tool via plugin

```
my-mod/
  plugin.json
  .mcp.json
```

`.mcp.json` (shape matches Kimix MCP config; see [MCP Servers](07-mcp-servers.md)):

```json
{
  "mcpServers": {
    "todo-count": {
      "command": "node",
      "args": ["${KIMIX_PLUGIN_ROOT}/bin/todo-count-server.js"]
    }
  }
}
```

Skills cannot register new tool schemas — only MCP (or skill-driven use of
built-in tools) can.

---

## Verification loop

A broken hook or plugin can **fail silently** (hooks fail-open on crash/timeout).
Do not treat scaffolding as done until surfaces are exercised.

1. **Validate structure**
   ```bash
   kimix plugin validate <path>
   ```
   Skills: `SKILL.md` exists; frontmatter `name` + `description` parse.
   Hooks: JSON valid; script executable; matcher non-empty.
2. **Confirm discovery**
   ```bash
   kimix inspect
   kimix plugin details <name>
   ```
   Expect the skill / plugin / hook / MCP server to appear with no load errors.
3. **UI**
   `/plugins`, `/hooks`, `/skills` (or `Ctrl+L` on non–VS Code family).
4. **Exercise every registered surface**
   - **Slash:** `/` menu shows the command; running it produces the intended turn
   - **Hook (deny):** trigger the guarded tool; model sees the deny reason
   - **Hook (allow):** a non-matching / safe command still runs
   - **MCP tool:** ask the model to call it by name; `/mcps` shows the server
5. **Session-only try** (no permanent install)
   ```bash
   kimix agent --no-leader --plugin-dir ./my-mod stdio
   ```
6. **Headless / CI**
   ```bash
   kimix -p "exercise the extension" --plugin-dir ./my-mod
   ```
7. **Disable without delete**
   `kimix plugin disable <name>`, `[skills] disabled = ["…"]` in config, or
   remove/rename the hook JSON.
8. **Reload**
   Skills usually hot-reload; hooks/plugins may need the Hooks tab reload or a
   new session.

### Deny test (block-rm-rf style)

Ask the agent to run `rm -rf /tmp/scratch-dir` with the guard installed:

- Expect **deny** + reason (no tool success)
- A safe command (e.g. `ls /tmp`) must still **allow**

---

## Performance and safety

Kimix aims to stay snappy as a daily driver:

- **Narrow matchers** so hooks skip most tool calls.
- **Short timeouts**; never block the agent loop on network without need.
- Prefer **logging** and **permission modes** ([Permissions](22-permissions-and-safety.md))
  over fragile always-on scripts for hard security.
- Do not run package install lifecycle scripts from untrusted plugin sources;
  review hooks and MCP commands before `--trust`.
- Project extensions are trust-gated for a reason — treat them like code review.
- Keep hook scripts dependency-light (`bash` / `python3` / `jq`).

---

## Related

- [Skills](08-skills.md) · [Plugins](09-plugins.md) · [Hooks](10-hooks.md)
- [MCP Servers](07-mcp-servers.md) · [Permissions](22-permissions-and-safety.md)
- Bundled skills: `/create-skill`, `/mod-builder`, `/kimix-knowledge`, `/help`
