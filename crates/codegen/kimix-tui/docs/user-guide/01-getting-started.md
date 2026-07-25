# Getting Started

**Kimix** is a general-purpose agent assistant that runs in your terminal. It
helps with daily work of many kinds — answering questions, researching topics,
drafting and editing text, organizing files, running tools, automating
routines, and collaborating on projects. Software engineering is one of the
things it can do well; it is not the only thing it is for.

You interact through a TUI (Terminal User Interface), run it headlessly for
scripts and automation, or connect it to editors via the Agent Client Protocol
(ACP). Local data stays under `~/.kimix/`.

---

## Installation

If you already have a `kimix` binary on your PATH (for example under
`~/.kimix/bin/kimix`), skip to [First Launch](#first-launch).

Verify:

```bash
kimix --version
```

Update when a new build is available:

```bash
kimix update
```

---

## First Launch

Start Kimix:

```bash
kimix
```

On first use, sign in with a provider you actually use. Examples:

```bash
kimix login          # interactive login for your default subscription provider
kimix login --xai    # native xAI device-code login → ~/.kimix/auth.json
```

Credentials are stored in `~/.kimix/auth.json` and reused across sessions.
You can also put API keys in environment variables and point models at them in
`~/.kimix/config.toml` (see [Custom Models](11-custom-models.md) and
[Authentication](02-authentication.md)).

Default UI mode is **fullscreen**. If the interface ever looks unusually bare,
check `[ui] screen_mode` in `~/.kimix/config.toml`, or run:

```bash
kimix --fullscreen
```

---

## Basic Interaction

Once running, Kimix shows two main areas:

- **Scrollback** — conversation history: your messages, the assistant’s
  replies, tool calls, file changes, and progress.
- **Prompt** — input at the bottom where you type.

Type a message and press `Enter`. Kimix may read files, run commands, search
the web, or call tools as needed. Tool activity streams into the scrollback in
real time.

Press `Tab` to move focus between the prompt and the scrollback. While a turn
is running, `Ctrl+C` cancels it (or clears a non-empty draft first). Idle,
press `Esc` twice within 800ms to clear a non-empty prompt, or (with an empty
prompt and existing messages) to open rewind — see
[Keyboard Shortcuts](03-keyboard-shortcuts.md#escape).

### File References

Use `@` in your prompt to attach files or browse paths:

```
@notes/meeting.md           # Attach a file
@docs/guide.md:10-50        # Attach lines 10-50
@src/                       # Browse a directory
```

The `@` operator opens a fuzzy file picker. By default it respects
`.gitignore` and hides dotfiles. Prefix with `!` to include hidden paths:

```
@!.env                      # Attach a hidden file
```

### Permissions

By default, Kimix asks before running shell commands or editing files. You can
approve one-by-one or enable always-approve:

- Press `Ctrl+O` to toggle always-approve mode
- Launch with `kimix --yolo`
- Type `/always-approve` in the prompt

---

## Key Concepts

### Sessions

Every conversation is a **session**. Sessions are saved under
`~/.kimix/sessions/` and can be resumed later. Each session tracks history,
tool calls, edits, and task state.

- New session: `Ctrl+N` or `/new`
- Resume: `/resume` in the TUI, or `kimix --resume <ID>`
- Continue most recent: `kimix -c`

### Scrollback

The scrollback is the main display. It can show:

- **User prompts** — your messages
- **Assistant messages** — replies with markdown and syntax highlighting
- **Thinking blocks** — intermediate reasoning (collapsible), when the model provides it
- **Tool calls** — commands, file edits (with diffs), search results, and more
- **Task lists** — TODOs tracking progress

Collapse or expand the selected entry with the arrow keys (or Vim bindings).
Press `Enter` to open the fullscreen viewer.

### Tools

Built-in capabilities include reading and editing files, searching content,
running shell commands, web search and fetch, task lists, subagents, and
cross-session memory. Extend further with [MCP servers](07-mcp-servers.md).

### Slash Commands

Type `/` in the prompt for quick actions:

```
/model longcat              # Switch model
/compact                    # Compress conversation history
/always-approve             # Toggle always-approve mode
/new                        # Start a new session
```

See [Slash Commands](04-slash-commands.md) for the full list.

### Models

Kimix is multi-model. Configure providers in `~/.kimix/config.toml` and switch
with `/model` or `kimix -m <name>`. List configured models:

```bash
kimix models
```

---

## Common Launch Options

```bash
# Interactive TUI with an initial prompt
kimix "summarize the notes in ./inbox"

# Work in a git worktree
kimix --worktree=feat "draft the release outline"

# Specific directory
kimix --cwd ~/projects/my-app

# Extra standing instructions for this session
kimix --rules "Prefer concise Chinese. Ask before deleting files."

# Auto-approve tools
kimix --yolo

# Specific model
kimix -m longcat

# Resume / continue
kimix --resume <session-id>
kimix -c

# UI mode (sticky when chosen via settings or these flags)
kimix --fullscreen
kimix --minimal

# Headless one-shot
kimix -p "What should I prepare for tomorrow's meeting?"
```

---

## Headless Mode

Run non-interactively for scripting and automation:

```bash
kimix -p "Your prompt here"
```

| Format | Flag | Description |
|--------|------|-------------|
| `plain` | (default) | Human-readable text |
| `json` | `--output-format json` | Single JSON object with result fields |
| `streaming-json` | `--output-format streaming-json` | NDJSON event stream |

```bash
kimix -p "Draft a short status update" --output-format json --yolo | jq -r '.text'
```

See [Headless Mode](14-headless-mode.md).

---

## Project Rules (AGENTS.md)

Add standing instructions with an `AGENTS.md` (or compatible) file. Kimix
loads them as project context at the start of a conversation:

```
~/.kimix/AGENTS.md           # Global rules (all projects)
<repo-root>/AGENTS.md        # Repository-level rules
<cwd>/AGENTS.md              # Directory-level rules (highest priority)
```

Deeper files take precedence. See [Project Rules](12-project-rules.md).

---

## Where to Go Next

| Document | What you will learn |
|----------|---------------------|
| [Authentication](02-authentication.md) | Login, API keys, multi-provider sessions |
| [Keyboard Shortcuts](03-keyboard-shortcuts.md) | Key bindings and mouse actions |
| [Slash Commands](04-slash-commands.md) | All `/` commands |
| [Configuration](05-configuration.md) | config.toml, paths, environment variables |
| [Custom Models](11-custom-models.md) | Multi-model setup and BYOK |
