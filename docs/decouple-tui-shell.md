# kimix-tui → kimix-shell 解耦规划

> **目标**：让 headless 模式（`kimix -p "prompt"`）不需要编译 TUI 渲染代码（ratatui/crossterm/syntect 等）。
> **原则**：渐进式解耦，每步独立可交付、可回滚，不破坏现有功能。
> **日期**：2026-07-29

---

## 1. 现状概览

### 1.1 Crate 规模

| Crate | 磁盘 | 行数 | 文件数 | 定位 |
|-------|------|------|--------|------|
| `kimix-shell` | 12M | ~279K | 355 | Agent 运行时 + Session + Extensions + Auth + Config |
| `kimix-tui` | 16M | ~361K | 403 | TUI 渲染 + Headless 模式 + 所有 slash 命令 |
| `kimix-shell-base` | 104K | ~2.2K | 6 | 进程管理、CPU profile、event_id、tip 基础设施 |
| `kimix-bin` | — | — | — | 组合根二进制，依赖 tui + shell |

### 1.2 依赖关系

```
kimix-bin
  ├── kimix-tui (lib)          ← 包含 headless.rs + TUI widgets
  │     └── kimix-shell (126 个导入)
  │           └── kimix-shell-base (轻量，仅 2.2K 行)
  └── kimix-shell (直接依赖，用于 agent/leader 入口)
```

### 1.3 核心矛盾

- `headless.rs`（headless 模式入口）位于 `kimix-tui` 内部
- headless 模式不需要 ratatui/crossterm/syntect，但因同属 `kimix-tui` crate，必须全量编译
- `kimix-tui` 的 403 个 `.rs` 文件中，有 55 个文件直接 `use kimix_shell::`（13.6%）
- 55 个文件中包含 **126 个 import 语句**，分布在 30+ 个模块中

---

## 2. 依赖分析矩阵（MECE 分类）

### 2.1 分类总览

| 类别 | 唯一项 | 占比 | 解耦难度 | 优先级 |
|------|:------:|:----:|:--------:|:------:|
| 1. 类型定义（struct/enum） | ~32 | 40% | 低 | P1 |
| 2. Config 加载/持久化 | ~10 | 12% | 低-中 | P1 |
| 3. 函数调用（业务逻辑） | ~16 | 20% | 中-高 | P2 |
| 4. Agent 运行时 | ~5 | 6% | 极高 | P3 |
| 5. Session/Storage | ~8 | 10% | 中 | P2 |
| 6. Extensions 系统 | ~12 | 15% | 中 | P2 |
| 7. Auth | ~4 | 5% | 高 | P3 |
| 8. 杂项（plugin/claude_import/cli_models） | ~6 | 8% | 低-中 | P1 |

> 注：唯一项指去重后的导入路径，百分比按功能粒度计算，非精确数学统计。

### 2.2 逐项明细

#### 类别 1：类型定义（struct/enum/const）

此类可直接移动到 `kimix-shell-base` 或新建 `kimix-shell-types` crate，零运行时依赖。

| 源路径 | 类型 | 频率 | TUI 使用场景 |
|--------|------|:---:|-------------|
| `sampling::types::ReasoningEffort` | enum | 13 | slash 命令、session lifecycle、agent 配置 |
| `sampling::types::ReasoningEffortOption` | struct | 1 | effort_levels slash 命令 |
| `sampling::types::REASONING_EFFORT_META_KEY` | const | 1 | headless 模型元数据 |
| `sampling::error::RATE_LIMITED_ERROR_CODE` | const | 3 | 错误处理 |
| `agent::config::UiConfig` | struct | 5 | 设置面板、事件循环 |
| `agent::config::Config`（AgentConfig） | struct | 6 | headless、models、trace、worktree、acp |
| `agent::config::AgentSelectionConfig` | struct | 1 | agents modal |
| `agent::config::DEFAULT_AGENT_TYPE` | const | 1 | agents modal |
| `agent::config::ModelSwitchIncompatibleAgentError` | struct | 1 | effects 错误处理 |
| `agent::auth_method::AuthMethodKind` | enum | 3 | headless、app_view、acp |
| `agent::auth_method::AuthMethodsBuildInputs` | struct | 2 | acp、welcome |
| `agent::auth_method::KIMI_CODE_METHOD_ID` | const | 1 | acp |
| `agent::auth_method::XAI_API_KEY_METHOD_ID` | const | 1 | acp |
| `extensions::mcp::McpServerStatus` | enum | 5 | mcp 配置面板、acp handler |
| `extensions::mcp::McpServerStatusPayload` | struct | 1 | acp handler |
| `extensions::mcp::McpToolEntry` | struct | 1 | acp handler |
| `extensions::notification::RetryState` | enum | 4 | acp handler、session notification |
| `extensions::notification::GoalClassifierVerdict` | enum | 2 | agent、goal_detail |
| `extensions::notification::SessionUpdate` | struct | 1 | acp handler tests |
| `extensions::notification::MemoryFileInfo` | struct | 1 | memory modal |
| `extensions::notification::HookRunEntryDto` | struct | 1 | acp handler tests |
| `extensions::notification::HookRunStatusDto` | enum | 1 | acp handler tests |
| `extensions::task::KillTaskResponse` | struct | 2 | effects helpers/tests |
| `extensions::task::CancelSubagentRequest` | struct | 1 | headless |
| `extensions::task::KillTaskRequest` | struct | 1 | headless |
| `extensions::billing::UsageRow` | struct | 2 | billing dispatch tests |
| `extensions::billing::UsageResponse` | struct | 1 | effects tests |
| `extensions::prompt_meta::PromptBlockMeta` | struct | 1 | effects |
| `session::ExtMethodResult` | struct | 2 | effects、roster |
| `session::ContextInfo` | struct | 2 | scrollback 渲染 |
| `session::TokenUsageCategory` | enum | 1 | scrollback context_info |
| `session::merge::MergedSession` | struct | 1 | sessions_cmd |
| `session::acp_types::ContextInfo` | struct | 1 | effects tests |
| `session::acp_types::SessionInfoData` | struct | 1 | effects tests |
| `tools::TodoItem` | struct | 3 | todo_pane、goal_detail、playground |
| `tools::TodoStatus` | enum | 3 | 同上 |
| `tools::TodoPriority` | enum | 1 | playground |
| `sampling::ConversationItem` | struct | 1 | effects helpers |
| `sampling::AssistantItem` | struct | 1 | 同上 |
| `sampling::UserItem` | struct | 1 | 同上 |
| `leader::ConnectionStatus` | enum | 1 | acp leader_bridge |
| `leader::LeaderConnection` | struct | 1 | leader_cluster |
| `leader::LeaderReconnector` | struct | 1 | 同上 |
| `leader::ReconnectPolicy` | enum | 1 | 同上 |
| `active_sessions::ActiveSession` | struct | 1 | effects |
| `cli_models::AuthStatus` | enum | 1 | models |
| `claude_import::ImportPlan` | struct | 1 | import_claude_modal |
| `claude_import::ImportableItem` | enum | 2 | 同上 |
| `claude_import::PathKind` | enum | 2 | 同上 |
| `util::config::McpServerConfig` | struct | 3 | mcp_cmd、extensions_modal |
| `util::config::McpServerTransportConfig` | struct | 3 | 同上 |
| `util::config::WorktreeHintMode` | enum | 1 | app_view |

#### 类别 2：Config 加载/持久化函数

| 函数 | 源模块 | TUI 使用文件 |
|------|-------|-------------|
| `util::config::resolve_tips` | shell | event_loop、acp_handler/settings |
| `util::config::user_config_path` | shell | mcp_cmd |
| `util::config::load_mcp_servers` | shell | effects（3 处直接调用） |
| `util::config::worktree_type` | shell | effects |
| `util::config::persist_models_default` | shell | effects |
| `util::kimix_home::kimix_home` | shell（→ shell-base 已有） | session_startup、sessions_cmd、trace_cmd |
| `config::load_effective_config` | shell | effects |
| `config::MemoryConfig` | shell | spawn |

> **注**：`kimix_home` 已在 `kimix-shell-base::util` 中实现，但 tui 直接 import `kimix_shell::util::kimix_home::kimix_home`（shell 通过 `pub use kimix_shell_base::util` re-export）。

#### 类别 3：函数调用（业务逻辑）

| 函数 | 源模块 | TUI 使用位置 |
|------|-------|-------------|
| `sampling::error::rate_limited_user_message` | shell | headless、session_notification |
| `sampling::error::http_status_from_error` | shell | effects |
| `sampling::types::parse_canonical_effort_token` | shell（→ sampling-types） | headless |
| `sampling::types::reasoning_effort_meta_value` | shell（→ sampling-types） | headless |
| `sampling::types::supports_reasoning_effort_meta` | shell（→ sampling-types） | slash/commands/model |
| `sampling::types::get_image_content_url` | shell | effects |
| `agent::auth_method::build_auth_methods` | shell | acp、welcome |
| `agent::roster::*` | shell | roster（agent 切换逻辑） |
| `session::restore::restore_session_with_progress` | shell | effects、session_startup |
| `session::persistence::list_recent_summaries` | shell | project_picker/sources |
| `session::storage::search::execute_search` | shell | sessions_cmd |
| `session::storage::search::SessionSearchRequest` | shell | sessions_cmd |
| `session::info::Info` | shell | effects |
| `session::persistence::session_dir` | shell | effects |
| `tools::todo::todo_item_from_plan_entry` | shell | acp_handler |
| `auth::ensure_authenticated_or_noninteractive` | shell | session_startup |
| `active_sessions::register_in` | shell | effects |
| `cli_models::list_models` | shell | models |
| `claude_import::find_project_root` | shell | import_claude_modal |
| `plugin::repo_update_outcome` | shell | plugin_cmd |

#### 类别 4：Agent 运行时

这是最深的耦合点。tui 的 `acp/spawn.rs` 直接构造 `MvpAgent` 和 `AuthManager`。

| 类型/函数 | 行数 | 说明 |
|----------|:----:|------|
| `agent::MvpAgent` | ~9874（config.rs 含相关逻辑） | 核心 agent 构造函数 |
| `agent::config::Config::new_from_toml_cfg` | — | Config 构造工厂方法 |
| `agent::models::RefreshStrategy` | — | 模型刷新策略 |
| `auth::AuthManager` | ~1925（manager.rs） | 认证管理器完整实现 |
| `agent::server::ServerConfig` / `run_agent_server` | ~492 | agent server 启动 |
| `agent::session_registry_client::SessionRegistryClient` | ~640 | session 注册表客户端 |
| `leader::LeaderConnection` 等 | ~5777 | Leader 集群通信 |

#### 类别 5：Session/Storage

| 项 | 源 | 使用位置 |
|----|----|---------|
| `session::memory::storage::MemoryStorage` | shell | memory_cmd |
| `session::merge::MergedSession` | shell | sessions_cmd |
| `session::ContextInfo` | shell（re-export 自 kimix-shared） | scrollback 渲染 |
| `session::TokenUsageCategory` | shell | scrollback/blocks/context_info |

#### 类别 6：Extensions 系统

tui 直接使用 extensions 的数据类型（见类别 1）加上部分函数调用：

| 项 | 源 |
|----|----|
| `extensions::mcp::McpServerStatus` 等 | shell |
| `extensions::billing::UsageRow` / `UsageResponse` | shell |
| `extensions::notification::*` | shell |
| `extensions::task::KillTaskRequest` / `CancelSubagentRequest` | shell |

#### 类别 7：Auth

| 项 | 说明 |
|----|------|
| `auth::AuthManager` | tui 持有的 Arc<AuthManager>，共享给 ACP 消息处理 |
| `auth::ensure_authenticated_or_noninteractive` | session_startup 使用 |
| `agent::auth_method::*` | 已归类在类型/Category 1 |

#### 类别 8：杂项

| 项 | 源 | 行数 |
|----|----|:----:|
| `plugin::RepoUpdateOutcome` / `UninstallError` | shell | ~458 |
| `claude_import::*` | shell | ~83K（巨大） |
| `cli_models::list_models` | shell | ~9.5K |

---

## 3. 可行性评估表

| 类别 | 解耦难度 | 抽象需求 | 代码量变化 | 编译收益 | 风险 | 方案 |
|------|:--------:|---------|:---------:|:--------:|:----:|------|
| 1. 类型定义 | **低** | 无，纯数据移动 | +200/-0 行（re-export 保持兼容） | 中：类型 crate 可并行编译 | 低：类型移动不改变行为 | 移动到 shell-base 或新的 shell-types |
| 2. Config 函数 | **低-中** | `ConfigProvider` trait | +50 行 trait / +100 行实现 | 低-中 | 低：config 加载逻辑稳定 | trait 抽象 + shell 实现 |
| 3. 业务函数 | **中-高** | `SessionService` / `ModelService` trait | +300 行 traits / +100 行实现 | 中 | 中：函数签名可能变化 | 按服务分组抽象 |
| 4. Agent 运行时 | **极高** | `AgentFactory` trait + DI 容器 | +200 行 trait / 保持 shell 实现 | 高：最大单点瓶颈 | 高：Agent 初始化逻辑复杂 | **不建议强行解耦**，Phase 0 通过移动 headless 缓解 |
| 5. Session/Storage | **中** | `SessionStore` trait | +150 行 | 低-中 | 低 | trait 抽象 |
| 6. Extensions | **中** | 类型移动 + Extension trait | +100 行 traits | 低-中 | 低 | 类型移动 + 功能 trait |
| 7. Auth | **高** | `AuthProvider` trait | +80 行 trait | 低 | 中：认证流程涉及 HTTP、OAuth、token 刷新 | trait 包装 |
| 8. 杂项 | **低-中** | 各模块独立抽象 | +150 行 | 低 | 低-中 | claude_import 可独立为 crate |

---

## 4. 分阶段实施计划

### Phase 0：Headless 独立（1-2 天，高优先）

**目标**：让 headless 模式不再链接 TUI 渲染代码。

**现状问题**：
- `headless.rs` 位于 `kimix-tui/src/headless.rs`，是 `kimix-tui` lib 的一部分
- `headless.rs` 只导入 shell 类型和 acp-lib，**不使用任何 ratatui/crossterm 代码**
- `kimix-bin` 调用 `kimix_tui::headless::run_single_turn()`，必须链接整个 tui

**方案**：创建 `crates/codegen/kimix-headless/` 独立 crate

```
kimix-headless (新)
  ├── kimix-shell
  ├── kimix-acp-lib
  └── (无 TUI 依赖)
```

**改动清单**：

1. 新建 `crates/codegen/kimix-headless/Cargo.toml`
   - 依赖：`kimix-shell`、`kimix-acp-lib`、`agent-client-protocol`、`tokio`、`clap`
   - 不依赖：`kimix-tui`、`ratatui`、`crossterm`、`syntect`
2. 移动 `kimix-tui/src/headless.rs` → `kimix-headless/src/lib.rs`
3. 移动 `kimix-tui/src/client_identity.rs` 的 `HEADLESS_CLIENT_TYPE`、`PAGER_CLIENT_VERSION` 常量到 `kimix-headless`（或提取到 `kimix-shared`）
4. `kimix-tui` 通过 `pub use kimix_headless::*` re-export 头文件保持向后兼容
5. `kimix-bin/Cargo.toml` 添加 `kimix-headless` 依赖，main.rs 改用 `kimix_headless::run_single_turn`

**编译收益**：
- headless 编译时间从 ~60s（含 tui）降到 ~20s（仅 shell + acp-lib）
- `kimix-tui` 仍然是 TUI 模式的依赖，不受影响

**风险**：极低。headless.rs 本身是自包含模块，只依赖 shell 和 acp-lib。

---

### Phase 1：类型提取到共享 crate（3-5 天）

**目标**：将类别 1 的 ~32 个类型定义移到共享 crate，减少 tui → shell 的重编译级联。

**方案**：扩展 `kimix-shell-base`（当前仅 2.2K 行）或新建 `kimix-shell-types`

**推荐**：扩展 `kimix-shell-base`，因为：
- shell-base 已存在且被 shell 依赖
- 性质一致：都是「基础类型和工具」
- 避免引入另一个 crate 增加依赖图复杂度

**需要移动的类型模块**：

| 新位置（shell-base） | 原位置（shell） | 类型数 |
|-----|-----|:---:|
| `kimix-shell-base::sampling::types` | `kimix_shell::sampling::types`（→ `kimix_sampling_types`） | 5 |
| `kimix-shell-base::sampling::error`（const 部分） | `kimix_shell::sampling::error` | 1 |
| `kimix-shell-base::agent::config_types` | `kimix_shell::agent::config` 的纯类型部分 | 5 |
| `kimix-shell-base::agent::auth_types` | `kimix_shell::agent::auth_method` 的类型 | 4 |
| `kimix-shell-base::extensions_types` | 各 extensions 子模块的类型 | 10 |
| `kimix-shell-base::session_types` | `kimix_shell::session` 的类型 | 6 |
| `kimix-shell-base::tools_types` | `kimix_shell::tools` 的类型 | 3 |
| `kimix-shell-base::util::config_types` | `kimix_shell::util::config` 的配置结构体 | 3 |

**实施细节**：

1. 在 `kimix-shell-base` 中创建对应模块
2. 将纯类型 struct/enum + derive 宏 + serde 标注移动过去
3. `kimix-shell` 通过 `pub use kimix_shell_base::xxx` re-export，保持现有 126 个导入不破
4. `kimix-tui` 逐步迁移导入路径：`use kimix_shell::` → `use kimix_shell_base::`
5. 函数（如 `get_image_content_url`）保持在 shell，因为依赖 `agent-client-protocol`

**编译收益**：
- shell 修改不再导致 tui 的类型检查缓存失效
- 类型 crate 可被 shell 和 tui 并行编译
- 预估节省 15-20% 的增量编译时间

**风险**：
- 低。类型移动 + re-export 是标准 Rust 重构模式
- 注意 `serde` 依赖：shell-base 需添加 `serde` + `serde_json` 依赖
- 注意循环依赖：shell-base 不能依赖 shell

---

### Phase 2：服务 Trait 抽象（5-8 天）

**目标**：为类别 3（函数调用）和类别 5（session/storage）引入 trait 抽象，使 tui 不直接依赖 shell 的函数实现。

**架构图**：

```
kimix-shell-base
  ├── trait SessionService { restore, list_recent, execute_search }
  ├── trait ConfigService { resolve_tips, load_mcp_servers, user_config_path }
  ├── trait ModelService { list_models, supports_effort_meta }
  ├── trait AuthService { ensure_authenticated, build_auth_methods }
  └── trait ExtensionService { ... }

kimix-shell
  ├── impl SessionService for ShellSessionService { ... }
  ├── impl ConfigService for ShellConfigService { ... }
  └── ...

kimix-tui
  ├── fn do_something(config_svc: &dyn ConfigService) { ... }  ← 依赖 trait，不依赖 shell
  └── (在启动时注入 ShellConfigService 实例)
```

**需要抽象的 trait**：

| Trait | 方法 | 复杂度 |
|-------|------|:------:|
| `SessionService` | `restore_with_progress`, `list_recent_summaries`, `execute_search`, `get_context_info` | 中 |
| `ConfigService` | `resolve_tips`, `user_config_path`, `load_mcp_servers`, `worktree_type`, `persist_models_default` | 低 |
| `ModelService` | `list_models`, `supports_reasoning_effort_meta`, `parse_canonical_effort_token`, `reasoning_effort_meta_value` | 低 |
| `AgentLifecycle` | `create_config_from_toml`, `register_active_session` | 低 |
| `MemoryService` | `get_memory_storage` | 低 |
| `ImportService` | `find_project_root`, `create_import_plan` | 中 |
| `PluginService` | `repo_update`, `uninstall` | 低 |

**实施策略**：

1. 每个 trait 放在 `kimix-shell-base` 中定义
2. `kimix-shell` 提供默认实现
3. `kimix-tui` 通过依赖注入获取 trait object
4. 渐进式迁移：每次迁移 1 个 trait，确保 test 通过后再迁移下一个

**编译收益**：
- tui 不再直接依赖 shell 的具体实现函数
- 当 shell 的实现细节变化时，tui 不需要重新类型检查（但仍需重新链接）
- 增量编译时间减少约 10%

**风险**：
- 中。trait object 引入动态分发开销（通常可忽略，这些不是热路径）
- 异步 trait 方法需要 `async_trait` 或 `#[async_fn_in_trait]`（nightly）
- 部分函数签名复杂（如 `restore_session_with_progress` 有回调参数）

---

### Phase 3：Feature Gate + Shell 模块化（8-12 天，可选长期优化）

**目标**：通过 Cargo feature flags 让 shell 的某些模块可以条件编译，进一步减少 headless 编译体积。

**方案**：

```toml
# kimix-shell/Cargo.toml
[features]
default = ["tui-extras"]
tui-extras = []        # claude_import、plugin、mcp_doctor 等仅 TUI 使用的模块
headless-only = []     # 保留核心 agent/sampling/session，排除 TUI 专属代码
```

**可 feature-gate 的模块**：

| 模块 | 行数 | 仅 TUI 使用？ |
|------|:----:|:------------:|
| `claude_import` | 83K | 是（import_claude_modal） |
| `claude_import_state` | 11K | 是 |
| `kimi_import` | 36K | 是 |
| `plugin` | 15K | 是（plugin_cmd） |
| `mcp_doctor` | 24K | 是（mcp_cmd） |
| `cli_models` | 9.5K | 是（models） |
| `extensions::billing` | 407 行 | 是（billing dispatch） |
| `agent::session_registry_client` | 640 行 | 是（session_startup、effects） |
| `leader::*`（部分） | 5.7K | 是（leader_cluster） |

**编译收益**：
- headless 模式下 shell 编译单元减少约 30%（~180K 行 → ~125K 行）
- 预估总编译时间减少 25-30%

**风险**：
- 高。Feature gate 引入组合爆炸，CI 需测试 `default`、`headless-only`、`no-default-features` 三种组合
- 模块间可能存在未文档化的交叉依赖，feature gate 后编译失败
- `cfg(feature = "tui-extras")` 散落在代码中，降低可读性

---

## 5. 整体编译时间收益预估

| 阶段 | 改动 | headless 编译 | TUI 编译 | 增量编译 |
|------|------|:------------:|:--------:|:--------:|
| 当前 | 无 | ~60s（含全量 tui） | ~60s | 改 shell 1 行 → 重编 tui（55 个文件） |
| Phase 0 | headless 独立 | **~20s**（仅 shell + acp） | ~60s（不变） | 不变 |
| Phase 1 | 类型提取 | ~18s | ~50s（类型缓存独立） | 改 shell 实现 → tui 只需重链接，不重类型检查 |
| Phase 2 | trait 抽象 | ~15s | ~45s | 改 shell impl → tui 零编译（仅重链接） |
| Phase 3 | feature gate | **~12s** | ~50s | 改非 core 模块 → headless 零影响 |

> 以上为粗略估算，实际收益取决于硬件、并行度、模块边界。

---

## 6. 风险评估

| 风险 | 影响 | 概率 | 缓解措施 |
|------|:----:|:----:|---------|
| trait 抽象引入运行时开销 | 性能下降 | 低 | 这些非热路径，trait object 开销 < 1μs |
| 类型移动破坏 serde 序列化兼容 | 数据不兼容 | 低 | re-export 保持原路径；serde 按结构序列化，不关心模块路径 |
| feature gate 组合爆炸 | CI 时间暴涨 | 中 | 仅 gate 最顶层的独立模块；控制在 2-3 个 feature |
| MvpAgent 解耦引入新 bug | headless 功能异常 | 高（若强行解耦） | **Phase 0 已规避此风险**：移动 headless，不动 agent |
| `claude_import`（83K 行）移动复杂度 | 编译失败 | 中 | 该模块相对独立，可整体提取为独立 crate |
| 循环依赖（shell-base → shell） | 编译失败 | 极低 | shell-base 是叶子 crate，不依赖 shell |

---

## 7. 推荐执行路径

```
Phase 0（1-2 天）
  └── 立即执行，ROI 最高
      └── 交付：kimix-headless crate
      └── 验证：cargo build -p kimix-bin --no-default-features 可跳过 tui

Phase 1（3-5 天）
  └── 高 ROI，为后续打基础
      └── 交付：扩展的 kimix-shell-base
      └── 验证：tui 的 import 逐步从 shell:: 改为 shell_base::

Phase 2（5-8 天）
  └── 中等 ROI，改善长期维护性
      └── 交付：trait 定义 + shell 实现
      └── 验证：tui 通过 trait object 注入服务

Phase 3（8-12 天，可选）
  └── 低优先级，增量收益
      └── 交付：feature-gated shell
      └── 前提：Phase 0-1 完成且稳定
```

---

## 8. 附录：现有可利用的基础设施

### kimix-shell-base 已有内容

| 模块 | 文件 | 功能 |
|------|------|------|
| `cpu_profile` | `cpu_profile.rs` | CPU profiling 采样 |
| `env` | `env.rs` | 环境变量预设 |
| `util::event_id` | `util/event_id.rs` | 事件 ID 生成 |
| `util::kimix_home` | `util/kimix_home.rs` | Kimix 主目录路径 |
| `util::secure_file` | `util/secure_file.rs` | 安全文件操作 |
| `util::tips` | `util/tips.rs` | 启动提示 |
| `util::uname` | `util/uname.rs` | 系统信息 |
| `util::mod` | `util/mod.rs` | 进程管理、URL 验证、随机数 |

### 已有独立类型 crate

| Crate | 用途 | 能否承载更多类型？ |
|-------|------|:-----------------:|
| `kimix-sampling-types` | 采样相关类型 | 是，已被 `sampling::types` re-export |
| `kimix-config-types` | 配置类型 | 可能需要 |
| `kimix-workspace-types` | 工作空间类型 | 可能需要 |
| `kimix-shell-base` | **推荐扩展目标** | 是，当前仅 2.2K 行 |

### 关键依赖关系（避免循环）

```
kimix-shell-base  → (无 shell 依赖，叶子 crate)
kimix-sampling-types → (无 shell 依赖)
kimix-config-types → (无 shell 依赖)
kimix-shell  → shell-base + sampling-types + config-types
kimix-headless → kimix-shell + kimix-acp-lib
kimix-tui → kimix-shell + kimix-shell-base + kimix-headless
kimix-bin → kimix-tui + kimix-shell + kimix-headless
```

---

## 9. 具体执行检查清单

### Phase 0

- [ ] 创建 `crates/codegen/kimix-headless/` 目录和 `Cargo.toml`
- [ ] 移动 `kimix-tui/src/headless.rs` → `kimix-headless/src/lib.rs`
- [ ] 提取 `HEADLESS_CLIENT_TYPE`、`PAGER_CLIENT_VERSION` 常量到 `kimix-headless`
- [ ] `kimix-tui/Cargo.toml` 添加 `kimix-headless` 依赖
- [ ] `kimix-tui/src/lib.rs` 添加 `pub use kimix_headless;`
- [ ] `kimix-bin/Cargo.toml` 添加 `kimix-headless` 依赖
- [ ] `kimix-bin/src/main.rs` 更新 import 路径
- [ ] `cargo test -p kimix-headless` 通过
- [ ] `cargo test -p kimix-tui` 通过
- [ ] `cargo test -p kimix-bin` 通过
- [ ] 验证 `cargo build -p kimix-headless` 不链接 ratatui/crossterm

### Phase 1

- [ ] 在 `kimix-shell-base/src/` 创建 `sampling_types.rs`
- [ ] 移动 5 个 sampling 类型（通过 re-export 从 sampling-types 桥接）
- [ ] 在 `kimix-shell-base/src/` 创建 `agent_types.rs`
- [ ] 移动 `UiConfig`、`AgentSelectionConfig`、`DEFAULT_AGENT_TYPE`、`ModelSwitchIncompatibleAgentError`
- [ ] 在 `kimix-shell-base/src/` 创建 `auth_types.rs`
- [ ] 移动 `AuthMethodKind`、`AuthMethodsBuildInputs`、方法 ID 常量
- [ ] 在 `kimix-shell-base/src/` 创建 `extensions_types.rs`
- [ ] 移动 `McpServerStatus`、`McpServerStatusPayload`、`McpToolEntry` 等 10 个 extension 类型
- [ ] 在 `kimix-shell-base/src/` 创建 `session_types.rs`
- [ ] 移动 `ExtMethodResult`、`ContextInfo`、`TokenUsageCategory`、`MergedSession` 等
- [ ] 在 `kimix-shell-base/src/` 创建 `tools_types.rs`
- [ ] 移动 `TodoItem`、`TodoStatus`、`TodoPriority`
- [ ] 在 `kimix-shell-base/src/` 创建 `config_types.rs`
- [ ] 移动 `McpServerConfig`、`McpServerTransportConfig`、`WorktreeHintMode`
- [ ] `kimix-shell/src/lib.rs` 添加 re-export：`pub use kimix_shell_base::*_types;`
- [ ] 全量测试通过
