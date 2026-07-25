# Hooks & Plugins Guide

Kimix supports **hooks** (event-driven shell commands) and **plugins** (bundles of skills, agents, hooks, and MCP servers). Both are managed through a unified modal interface.

## Opening the Modal

| Method | Opens on tab |
|--------|-------------|
| `Ctrl+L` | Plugins (any pane; **non–VS Code family** — on VS Code / Cursor / Windsurf / Zed use `/plugins`) |
| `/plugins` | Plugins (any terminal) |
| `/hooks` | Hooks |

## Tabs

The modal has four tabs: **Hooks**, **Plugins**, **Skills**, and **MCP Servers**. Switch between them with `Tab` / `→` (forward) or `Shift+Tab` / `←` (backward).

---

## Hooks Tab

Hooks are shell commands (or HTTP calls) that run automatically on events like `session_start`, `post_tool_use`, `notification`, etc. See [Creating Custom Hooks](custom-hooks.md) for how to write your own.

Hooks are grouped by source:
- **Global hooks** — from `~/.kimix/hooks/`
- **Project hooks** — from `.kimix/hooks/` in your repo
- **Plugin hooks** — bundled with installed plugins
- **Custom hooks** — added manually via a path

Each hook shows:
- **Event** it triggers on (e.g., `session_start`, `post_tool_use`)
- **Command** or **URL** that runs
- **Timeout** duration
- **Status** — enabled or `[disabled]`

### Shortcuts (Hooks tab)

| Key | Action |
|-----|--------|
| `l` | Reload all hooks |
| `a` | Add hook from path |
| `r` | Remove selected hook |
| `e` | Enable / disable selected hook |
| `Space` | Expand / collapse group |

---

## Plugins Tab

Plugins are directories containing any combination of skills, agents, hooks, and MCP server configs.

Each plugin shows (when expanded):
- **Name** and **version**
- **Scope** — `user`, `project`, or `cli`
- **Skills** — names or count
- **Agents** — names or count
- **Hooks** — count
- **MCP servers** — count (or "blocked" if not trusted)
- **Description**
- **Conflicts** — ⚠ warning if any

Plugin hooks automatically receive `KIMIX_PLUGIN_ROOT` and `KIMIX_PLUGIN_DATA` environment variables (see the [Plugins guide](../user-guide/09-plugins.md#environment-variables-in-plugin-hooks)).

### Shortcuts (Plugins tab)

| Key | Action |
|-----|--------|
| `r` | Reload all plugins |
| `i` | Install plugin from path |
| `e` | Enable / disable selected plugin |
| `Space` | Expand / collapse plugin details |
| `/` | Search plugins by name |

---

## General Keyboard Shortcuts

These work across all tabs:

| Key | Action |
|-----|--------|
| `Tab` / `→` | Next tab |
| `Shift+Tab` / `←` | Previous tab |
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Space` | Toggle expand / collapse |
| `/` | Start search (Plugins) |
| `Backspace` | Delete search char, or re-enter search |
| `Esc` | Clear search, or close modal |
| `q` | Close modal |

## Confirmation & Errors

Some actions (like uninstalling a plugin) may ask for confirmation:
- Press `y` to confirm
- Press `Esc` or any other key to cancel

Errors are shown as a message overlay — press any key to dismiss.

While an action is in progress, the modal shows "Processing..." and blocks input until the operation completes.

## See Also

- [Creating Custom Hooks](custom-hooks.md) — step-by-step guide to writing your own hooks and scripts
- [Hooks user guide](user-guide/10-hooks.md) — events, matchers, trust model
- [Hook Examples](../../../kimix-hooks/examples/README.md) — ready-to-use sample hooks
- [Plugins user guide](user-guide/09-plugins.md) — install and trust
