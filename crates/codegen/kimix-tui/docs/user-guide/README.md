# Kimix User Guide

**Kimix** is a general-purpose agent assistant in the terminal. It helps with
everyday work — research, writing, organizing information, operating tools,
automating routines, and yes, also software tasks when you need them. It is
not limited to programming.

You talk to Kimix in a TUI session (or headlessly). It can read and write
files, run shell commands, search the web, call MCP tools, remember across
sessions, and spawn subagents — under your permission controls.

All local state lives under `~/.kimix/` (config, auth, sessions, memory).

---

## Tier 1: Essential User Docs

Start here. These guides cover what you need on your first day.

| # | Document | Description |
|---|----------|-------------|
| 1 | [Getting Started](01-getting-started.md) | Installation, first launch, authentication, basic interaction, and key concepts |
| 2 | [Authentication](02-authentication.md) | Login flows, API keys, multi-provider sessions, and device-code auth |
| 3 | [Keyboard Shortcuts](03-keyboard-shortcuts.md) | Reference for every key binding and mouse action in the TUI |
| 4 | [Slash Commands](04-slash-commands.md) | Every `/` command for sessions, models, memory, hooks, and plugins |
| 5 | [Configuration](05-configuration.md) | `config.toml`, `pager.toml`, environment variables, and file locations |

---

## Tier 2: Core Feature Docs

Customize and extend Kimix.

| # | Document | Description |
|---|----------|-------------|
| 6 | [Theming and Appearance](06-theming.md) | Themes, the `/theme` command, `pager.toml`, and color-support detection |
| 7 | [MCP Servers](07-mcp-servers.md) | External tool integrations through the Model Context Protocol |
| 8 | [Skills](08-skills.md) | Reusable prompt packages in the SKILL.md format |
| 9 | [Plugins](09-plugins.md) | Bundle and share skills, commands, agents, hooks, and MCP servers |
| 10 | [Hooks](10-hooks.md) | Lifecycle scripts and HTTP callbacks for pre- and post-tool-use events |
| 11 | [Custom Models](11-custom-models.md) | Bring-your-own-key, multi-model routing, and OpenAI/Anthropic-compatible endpoints |
| 12 | [Project Rules (AGENTS.md)](12-project-rules.md) | Per-directory AGENTS.md instructions and their precedence |
| 13 | [Memory](13-memory.md) | Cross-session knowledge persistence with `/flush`, `/dream`, and hybrid search |

---

## Tier 3: Advanced Usage Docs

Automate, script, and integrate Kimix with other systems.

| # | Document | Description |
|---|----------|-------------|
| 14 | [Headless Mode and Scripting](14-headless-mode.md) | `kimix -p`, output formats, CI/CD integration, and piping |
| 15 | [Agent Mode and IDE Integration](15-agent-mode.md) | ACP stdio transport, WebSocket relay, and SDK integration |
| 16 | [Subagents and Personas](16-subagents.md) | Parallel child sessions, agent types, personas, and capability modes |
| 17 | [Session Management](17-sessions.md) | Save, load, resume, rewind, compact, and the session persistence format |
| 18 | [Sandbox Mode](18-sandbox.md) | OS-level filesystem and network isolation profiles |
| 19 | [Plan Mode](19-plan-mode.md) | Structured planning, plan-file edits, and approval before acting |
| 20 | [Background Tasks and Monitoring](20-background-tasks.md) | `background: true`, `/loop`, `monitor`, and `Ctrl+G` to demote |
| 21 | [Terminal Support and Troubleshooting](21-terminal-support.md) | tmux, SSH, truecolor, clipboard, and OSC 52 |
| 22 | [Permissions and Safety Controls](22-permissions-and-safety.md) | Permission modes, auto-approved tools, and restrictive PreToolUse hooks |
