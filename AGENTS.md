# Kimix — agent/developer notes

Single source of truth for how this repository is organized and the
constraints every change must respect. Update this file whenever the tech
stack or product direction changes.

## What this is

Kimix 是一个通用终端 AI 代理工具，基于 grok-build（Apache-2.0, Rust）的 hard fork，
重新锚定 Kimi Code 订阅 API 和 Moonshot 开放平台，
内置 CJK 感知的 BM25 检索和高缓存命中设计的提示工程。
适用场景包括编程、研究、数据分析和自动化工作流。
It coexists with the official `kimi` CLI: binary `kimix`, config dir `~/.kimix`
(`KIMIX_SHARE_DIR` override), keyring service `kimix`, env prefix `KIMIX_*`.
Never read or write `~/.kimi` (except the explicit one-time read-only
import) or any `KIMI_*` env var.

## Hard constraints

- **Zero egress**: outbound connections are limited to
  `auth.kimi.com`, `api.kimi.com`, `api.moonshot.cn`, `api.moonshot.ai`,
  GitHub Releases domains, and user-configured MCP servers. No telemetry,
  no analytics, ever. `crates/codegen/kimix-env` is the single home of
  first-party endpoints.
- **Toolchain**: Rust 1.97.0 (rust-toolchain.toml), edition 2024.
- **Gates** (all must stay green):
  `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets` (zero warnings),
  `cargo fmt --all --check`, `cargo deny check advisories`.
- **Observability is local**: `kimix-log` (unified session log, `--debug`
  firehose, subsystem file logs, opt-in instrumentation) writes under
  `~/.kimix` only. Its zero-network property is a contract.
- The root `Cargo.toml` is hand-maintained (upstream's generator is not in
  this repo). Members sorted; versions inherited from
  `workspace.package.version` (0.1.0).

## Layout

- `crates/codegen/` — the bulk of the application (kimix-* crates).
  Key ones: `kimix-bin` (binary `kimix`), `kimix-tui` (full-screen TUI +
  headless `-p` mode + `acp`/`mcp` commands), `kimix-shell` (agent runtime,
  leader-follower IPC, sessions), `kimix-sampler` (inference client;
  ChatCompletions/Responses/Messages backends), `kimix-auth` (credentials),
  `kimix-config` (config layering, `~/.kimix` paths), `kimix-env` (endpoints),
  `kimix-tools` (tool implementations incl. codex/opencode ports — see its
  THIRD_PARTY_NOTICES.md), `kimix-workspace` (FS/VCS/exec/permissions,
  checkpoint/worktree), `kimix-log` (local observability).
- `crates/common/`, `crates/build/`, `prod/mc/` — shared libs, proto build,
  proxy wire types (the latter to be redefined against Kimi in M1).
- `third_party/` — vendored Mermaid rendering stack (untouched policy).
- `bin/protoc` — dotslash launcher used by proto codegen.

## Test seams

Cross-crate test hooks are behind the `test-support` cargo feature
(kimix-workspace, kimix-pager-render, kimix-config, kimix-tui), enabled via
dependents' `[dev-dependencies]`. Don't expose new test seams as plain
`#[cfg(test)]` items across crate boundaries.

## Milestones (PRD §8.3)

- M0 (done): rename, deletions (voice/telemetry/announcements/marketplace/
  relay-gateway), toolchain, gates.
- M1: Kimi device-flow auth (F1), Moonshot API-key channel (F2), inference
  via ChatCompletions (F3), dynamic model sync (F4). The auth stack in
  `kimix-shell/src/auth` + `kimix-auth` gets rewritten here; transitional
  grok.com references live only there and in `kimix-sampler`/proxy types.
- M2: server-side search/fetch (F5), command parity with kimi-cli 1.49.0
  (F6), one-time `~/.kimi/config.toml` import (F7), F9 smoke list, perf CI.
- M3: GitHub Releases distribution, install scripts, self-update (F8).
