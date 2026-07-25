# Smoke checklist (PRD F9)

F9 carries the grok-build harness capabilities forward unchanged. Each row
names the capability, the automated case that covers it (run with
`CARGO_INCREMENTAL=0`), and the manual probe when automation cannot reach it.
A release is smoke-clean when every row has at least one passing case.

Legend: `auto` = covered by the named test target in CI; `manual` = run the
listed command and check the listed observable.

| # | Capability | Case | How |
|---|------------|------|-----|
| 1 | Fullscreen TUI (mouse, themes, shortcuts, slash commands) | auto | `cargo test -p kimix-tui --lib` (welcome/menu/mouse/theme suites, 6.6k tests); manual spot: `kimix` → welcome moon renders, `ctrl+q` quits |
| 2 | Headless / script mode | auto | `cargo test -p kimix-tui --lib headless`; manual: `kimix -p "Reply OK"` prints the reply and exits 0 |
| 3 | ACP server | manual | `printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}}}\n' \| kimix acp` → one JSON-RPC result line with `"protocolVersion":1` |
| 4 | MCP client (stdio/http/oauth) | auto | `cargo test -p kimix-mcp --lib`, `cargo test -p kimix-shell --lib util::config::mcp`; manual: `kimix mcp add t -- npx -y @modelcontextprotocol/server-everything` then `kimix mcp doctor t` |
| 5 | `--mcp-config-file` injection | auto | `cargo test -p kimix-shell --lib util::config::mcp::tests::cli_mcp`; manual: flag + `x.ai/mcp/servers_updated` notification lists the injected server |
| 6 | Skills | auto | `cargo test -p kimix-agent --lib skills` |
| 7 | Plugins (local load) | auto | `cargo test -p kimix-agent --lib plugins`; manual: `kimix plugin list` |
| 8 | Hooks | auto | `cargo test -p kimix-hooks --lib` and `cargo test -p kimix-shell --lib hooks` |
| 9 | Sandbox | auto | `cargo test -p kimix-sandbox --lib` |
| 10 | Checkpoint / session persistence | auto | `cargo test -p kimix-shell --lib session::` (persistence/rewind suites); manual: `kimix sessions` lists the last session, `kimix -c` resumes it |
| 11 | Worktrees | auto | `cargo test -p kimix-fast-worktree --lib`; manual: `kimix worktree list` |
| 12 | Mermaid rendering | auto | `cargo test -p kimix-mermaid --lib` |
| 13 | Crash handling | auto | `cargo test -p kimix-crash-handler --lib` |
| 14 | Kimi auth (login picker: device flow + Moonshot API key) | auto | `cargo test -p kimix-shell --lib auth::` (138 cases incl. live-shape fixtures) + `cargo test -p kimix-shell --lib auth_method` (moonshot validation); manual: `kimix login` completes in a browser, token lands in keyring service `kimix`; welcome picker also accepts a Moonshot API key into `[platforms.*]` in config.toml |
| 15 | Inference (ChatCompletions Kimi dialect) | auto | `cargo test -p kimix-sampler` (incl. `test_kimi_wire`); e2e: `scratchpad` mock flow (write → run → answer), see AGENTS.md |
| 16 | Model catalog sync (`/models`) | auto | `cargo test -p kimix-shell --lib models_fetch` + `cargo test -p kimix-models --lib` |
| 17 | Search/fetch tools (OAuth-gated) | auto | `cargo test -p kimix-tools --lib web_search` and `--lib web_fetch` (Kimi wire contracts) |
| 18 | Performance budgets | auto | `scripts/bench.sh` — `kimix --version` p95 ≤ 50 ms, TUI first frame ≤ 300 ms (CI `perf` job) |
| 19 | Coexistence with official kimi-cli | manual | both logged in on one machine; kimix never reads/writes `~/.kimi` or keyring service `kimi-code` (F7 import is read-only; §9 requires a 24 h parallel-use pass) |
