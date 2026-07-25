# 贡献指南 / Contributing

Kimix 是一个基于 [xai-org/grok-build](https://github.com/xai-org/grok-build) 的 hard fork
（上游不接受外部贡献），Kimix 自身欢迎 issue 和 pull request。

Kimix is a hard fork of [xai-org/grok-build](https://github.com/xai-org/grok-build)
(which does not accept external contributions). Kimix itself welcomes issues
and pull requests.

---

## 基本规则 / Ground Rules

- 工具链由 `rust-toolchain.toml` 锁定（Rust 1.97.0，edition 2024）
- 提交 PR 前，确保 CI 门禁在本地全部通过：

  ```sh
  cargo check --workspace --all-targets
  cargo clippy --workspace --all-targets
  cargo fmt --all --check
  cargo deny check advisories
  ```

- 禁止添加遥测、分析或新的出站端点。第一方端点的封闭集合定义在 `crates/codegen/kimix-env`
- 上游同步策略：逐 commit 评估、手动 cherry-pick，参见 AGENTS.md 的 milestone 说明

- Toolchain is pinned by `rust-toolchain.toml` (Rust 1.97.0, edition 2024)
- Before opening a PR, make sure the CI gates pass locally:

  ```sh
  cargo check --workspace --all-targets
  cargo clippy --workspace --all-targets
  cargo fmt --all --check
  cargo deny check advisories
  ```

- No telemetry, analytics, or new outbound endpoints. The closed set of
  first-party endpoints lives in `crates/codegen/kimix-env`
- Upstream syncs are evaluated commit-by-commit and cherry-picked manually

---

## 开发环境搭建 / Development Setup

### 前置条件 / Prerequisites

```sh
# Rust 工具链
rustup toolchain install 1.97.0
rustup component add rustfmt clippy

# protoc 启动器（跨平台）
cargo install dotslash --locked
# 或 macOS: brew install dotslash
```

### 构建 / Build

```sh
# 开发构建（快速迭代）
cargo build -p kimix-bin

# 发布构建（带优化）
cargo build --profile release-dist -p kimix-bin
```

### 运行测试 / Running Tests

```sh
# 全部测试
cargo test --workspace --lib

# 单个 crate
cargo test -p kimix-tools --lib
cargo test -p kimix-shell --lib

# 性能基准
cargo bench -p kimix-tools
```

### 代码风格 / Code Style

- 遵循 `rustfmt.toml` 和 `clippy.toml` 中的项目级配置
- 跨 crate 测试钩子统一通过 `test-support` cargo feature 暴露（不直接使用 `#[cfg(test)]` 跨 crate 边界）
- 使用 `dunce::canonicalize` 而非 `std::fs::canonicalize`（Windows verbatim 路径兼容）

---

## 项目结构 / Project Layout

```
crates/
├── codegen/       # 应用主体（kimix-bin / kimix-tui / kimix-shell 等）
├── common/        # 共享库（协议类型、工具运行时、测试工具）
├── build/         # 构建支持（proto 代码生成）
├── kimix-core/    # BM25 检索引擎核心
├── kimix-prompt/  # 提示工程核心
└── kimix-bridge/  # 层间桥接类型
third_party/       # vendored 第三方库（mermaid 渲染栈）
bin/protoc         # dotslash 启动器
```

---

## 提交规范 / Commit Guidelines

- 使用中文或英文均可，保持简洁
- 提交消息格式：`<模块>: <简述>`
- 示例：`kimix-tui: 修复主题切换时的渲染闪烁`

- Chinese or English both accepted, keep it concise
- Commit message format: `<module>: <brief description>`
- Example: `kimix-tui: fix render flicker on theme switch`

---

## 安全 / Security

发现安全漏洞请通过 [GitHub 私有漏洞报告](https://github.com/taxueseek/kimix/security/advisories/new)
提交，**不要**公开提 issue。

Report vulnerabilities via
[GitHub private vulnerability reporting](https://github.com/taxueseek/kimix/security/advisories/new).
Please do not open public issues for security reports.
