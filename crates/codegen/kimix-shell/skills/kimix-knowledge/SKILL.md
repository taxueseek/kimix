---
name: kimix-knowledge
description: >
  Answer questions about Kimix itself — features, config keys, slash commands,
  skills, plugins, hooks, MCP, permissions, sessions, headless/ACP, plan mode,
  memory, dashboard, and how to extend it. Prefer this skill over guessing
  product behavior. Also /kimix-knowledge.
---

# Kimix Knowledge

Kimix is a **high-performance general-purpose AI agent** (TUI + headless + ACP).
Local state lives under `~/.kimix/`.

The **user-guide next to the product** is authoritative — it is extracted to
`~/.kimix/docs/user-guide/` on startup from the same sources the TUI docs
picker uses. **Never invent** slash commands, config keys, tool names, or CLI
flags. Open the matching doc file and answer from it.

## How to use this skill

1. Pick the reference file(s) below that cover the question and **read them**.
2. Answer from what they say; quote exact ids, flags, paths, and settings keys
   verbatim.
3. For **building** a plugin / hook / slash command / skill / MCP package
   (Kimix "mod"), switch to `/mod-builder` — do not improvise packaging here.
4. Prefer a minimal working example over abstract theory when the doc has one.

## Reference index

Path root: `~/.kimix/docs/user-guide/`

| Topic | File |
|-------|------|
| Install / first run | `01-getting-started.md` |
| Auth | `02-authentication.md` |
| Keyboard shortcuts | `03-keyboard-shortcuts.md` |
| Slash commands | `04-slash-commands.md` |
| Configuration (`config.toml`, env) | `05-configuration.md` |
| Theming / appearance | `06-theming.md` |
| MCP servers | `07-mcp-servers.md` |
| Skills | `08-skills.md` |
| Plugins | `09-plugins.md` |
| Hooks | `10-hooks.md` |
| Custom models | `11-custom-models.md` |
| Project rules (AGENTS.md) | `12-project-rules.md` |
| Memory | `13-memory.md` |
| Headless / CI / scripting | `14-headless-mode.md` |
| ACP / IDE / agent mode | `15-agent-mode.md` |
| Subagents / personas | `16-subagents.md` |
| Sessions | `17-sessions.md` |
| Sandbox | `18-sandbox.md` |
| Plan mode | `19-plan-mode.md` |
| Background tasks / monitoring | `20-background-tasks.md` |
| Terminal support / troubleshooting | `21-terminal-support.md` |
| Permissions and safety | `22-permissions-and-safety.md` |
| Agent dashboard | `23-dashboard.md` |
| Usage / OpenTelemetry | `24-monitoring-usage.md` |
| **Building extensions (mods)** | `25-building-extensions.md` |

Also useful:

- `~/.kimix/config.toml`, `~/.kimix/pager.toml`, `~/.kimix/README.md`
- In-app: `/help` docs picker, `kimix inspect`, `kimix plugin …`

## Extension surface (Kimix "mods")

Kimix does **not** load TypeScript `ModApi` factories (Command Code style).
Practical layers, smallest → largest:

| Want | Use |
|------|-----|
| Reusable procedure / `/name` prompt | **Skill** (`SKILL.md`) or legacy `commands/*.md` |
| Lifecycle script (block tool, log, notify) | **Hook** (`hooks/*.json` + command/HTTP) |
| Extra model-callable tools | **MCP server** (`[mcp_servers.*]` or plugin `.mcp.json`) |
| Bundle skills + commands + agents + hooks + MCP + LSP | **Plugin** (`plugin.json` + convention dirs) |
| Standing project instructions | **AGENTS.md** |
| Cross-session facts | **Memory** |

Command Code "mod" ≈ Kimix **plugin** (plus hooks/MCP), not a single TS file.

| Command Code | Kimix |
|--------------|--------|
| `cmd.addTool` | MCP tool (or skill driving built-ins) |
| `cmd.addCommand` | Skill slash / `commands/*.md` / plugin `commands/` |
| `hooks.beforeToolCall` | `PreToolUse` hook |
| `hooks.afterToolCall` | `PostToolUse` / `PostToolUseFailure` |
| `transformInput` / session hooks | `UserPromptSubmit` / `SessionStart` / `SessionEnd` |
| package install | `kimix plugin install` / drop into plugins dir |

Discovery (high level — details in the linked docs):

- **Skills:** CWD / repo `.kimix/skills` → `~/.kimix/skills` (+ compat roots)
- **Hooks:** `~/.kimix/hooks` (always trusted) + project hooks (folder trust) + plugin hooks
- **Plugins:** session/cli dirs → project `.kimix/plugins` (trust) → `~/.kimix/plugins`

## Built-in tools (model-facing)

Typical surface (hook matchers also accept Claude aliases):

- Files: `read_file`, `search_replace` / `write`, `list_dir`, `grep`
- Shell: `run_terminal_command` (alias `Bash` in matchers)
- Web: `web_search`, `web_fetch` / page tools
- Agents: `spawn_subagent`, background task controls
- Planning: todos, plan mode entry/exit

## Performance posture

Kimix is tuned as a daily-driver agent (idle tick backoff, spinner frame-gating,
mouse motion without full repaint). When advising on extensions:

- Narrow hook **matchers**; short **timeouts**; fail-open on crash/timeout
- Prefer permission modes / sandbox over fragile always-on scripts for hard security
- Put expensive work off the PreToolUse hot path

## Verify your answer

- Paths, flags, and config keys must appear in a user-guide file or `kimix --help` /
  `kimix inspect` output — if you cannot find it, say so rather than inventing.
- When the user should run something, prefer the exact command the doc shows
  (e.g. `kimix plugin validate ./my-mod`, `kimix inspect`).
- Building? Hand off to `/mod-builder` and `25-building-extensions.md`.
