# Kimix v0.1.15 架构优化交接任务说明

> 日期：2026-07-29
> 目标：以 Rust 规范审视 Kimix 架构与代码，进一步减少代码量、提升可维护性、解耦耦合点、消灭 bug 类、发现工程巧思
> 方法论：第一性原理 + MECE + 量化思维，子代理团队并行实施
> 基线：commit `36faff5`（v0.1.14 + Phase 1-2 全部改动）

---

## 一、项目现状

### 基本信息

- **项目路径**：`/Users/taxuexunxian/Documents/GPT/kimix`
- **GitHub**：`https://github.com/taxueseek/kimix`
- **语言**：Rust 1.97.0 / edition 2024
- **规模**：117.5 万行自研代码 + 2.6 万行 third_party，64 个 crate
- **定位**：grok-build（Apache-2.0）hard fork，通用终端 AI 代理工具
- **二进制**：`target/release/kimix`（133MB），终端通过 symlink 安装

### 已完成的优化（v0.1.13 → v0.1.14 + Phase 1-2）

| 版本 | 改动 | 文件数 | 行数 |
|------|------|--------|------|
| v0.1.13 | LTO + jemalloc + prompt cache 可观测 + panic 修复 + i18n 下沉 | 10 | +120/-27 |
| v0.1.14 | file_cache 恢复 + OnceLock + read_file 防护 + bash 截断修正 + fuzzy panic 防护 + 移除 #![allow] + 12 处 warning 修复 | 18 | +176/-76 |
| Phase 1 | renovate.json5 + justfile + 敏感路径 deny + glob 超时 + rg 原子化 + xAI env var | 8 | +84/-25 |
| Phase 2 | interjection-core 移植 + tracing-macros 移植 + scrollback 硬上限 + 退出会话释放 + auto_compact 可观测 | 10 | +620/-3 |

### 硬约束（AGENTS.md）

- **Zero egress**：出站仅限 auth.kimi.com / api.kimi.com / api.moonshot.* / GitHub Releases / 用户 MCP
- **零遥测**：无数据采集、无分析、无追踪
- **Gates**（全须绿）：`cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets`（零警告）、`cargo fmt --all --check`、`cargo deny check advisories`
- **Toolchain**：Rust 1.97.0，edition 2024
- **根 Cargo.toml**：手维护（上游生成器不在本仓库）

### crate 结构

64 个 crate，最大 5 个：
1. `kimix-tui`（36.1 万行）— 全屏 TUI + headless + ACP/MCP
2. `kimix-shell`（27.2 万行）— agent runtime + leader-follower IPC + sessions
3. `kimix-tools`（10.9 万行）— 工具实现（含 codex/opencode 移植）
4. `kimix-workspace`（5.7 万行）— FS/VCS/exec/permissions/checkpoint
5. `kimix-pager-render`（3.6 万行）— 渲染引擎

### 已知 pre-existing 问题

1. `kimix-tui` 有 1 个 `unused_imports` 警告（非本轮引入）
2. `kimix-crash-handler` 有 7 个 warning（pre-existing，非本轮引入）
3. `kimix-workspace` 有 18 个预存测试失败（`permission::` 和 `session_lock` 模块）
4. macOS Gatekeeper 会杀 `cp` 到 `~/.kimix/downloads/` 的二进制（exit 137），必须用 symlink 安装
5. `hello.py` 在项目根目录（测试残留，应清理）

---

## 二、本轮分析维度（MECE）

### 维度 A：代码量削减

**第一性原理**：每一行代码都是维护成本。117.5 万行中，有多少是「非自研不可」的？

| 方向 | 具体问题 | 分析方法 |
|------|---------|---------|
| A1 重复代码 | grep/glob 工具在 `kimix-tools` 和 `kimix-workspace` 中各有 ripgrep 解包代码（已原子化但仍重复） | 搜索重复函数/模式 |
| A2 死 crate | 64 个 crate 中是否有零引用或仅被测试引用的 crate | `cargo-udeps` + `cargo-shear` 扫描 |
| A3 过度抽象 | trait 只有 1 个实现者的「过早抽象」 | grep `trait.*{` + 统计 impl 数量 |
| A4 代码重复 | `kimix-tools` 中 codex/opencode/kimix 三套工具实现的重复 | diff 对比三套实现的共性 |
| A5 第三方 vendored | `third_party/`（2.6 万行 mermaid/dagre）是否有 crates.io 替代 | 搜索 crates.io 替代 |

### 维度 B：解耦

| 方向 | 具体问题 | 分析方法 |
|------|---------|---------|
| B1 crate 依赖链 | 改 `kimix-env`（181 行）触发 23 个 crate 重编（950K 行，5250x 放大） | `cargo tree` + 编译拓扑分析 |
| B2 循环依赖 | 是否存在 crate 间的循环依赖或反向依赖 | `cargo tree --duplicates` |
| B3 上帝 crate | `kimix-shell`（27 万行）和 `kimix-tui`（36 万行）是否过大 | 分析模块内聚度，是否可拆分 |
| B4 特性耦合 | `kimix-tui` 是否可以不依赖 `kimix-shell`（headless 模式不需要 TUI） | 检查依赖方向 |
| B5 配置耦合 | 配置加载是否有不必要的跨 crate 依赖 | 追踪 ConfigLayers 的依赖链 |

### 维度 C：一行消灭一类 bug

| 方向 | 具体问题 | 分析方法 |
|------|---------|---------|
| C1 unwrap/expect | 非测试代码中的 `unwrap()`/`expect()` 是否有用户输入可触发的路径 | grep `\.unwrap()\|\.expect(` 排除非静态常量 |
| C2 panic 路径 | `panic!`/`unreachable!`/`todo!`/`unimplemented!` 是否有用户输入可达的 | grep + 调用链分析 |
| C3 unsafe 块 | `unsafe` 块是否有充分的安全论证 | grep `unsafe` 审查 |
| C4 资源泄漏 | 文件句柄/锁/子进程是否有 drop 保证 | 审查 RAII 模式 |
| C5 整数溢出 | `as usize`/`as u64` 等类型转换是否有溢出风险 | grep `as usize\|as u64` |
| C6 并发安全 | `Mutex`/`RwLock` 是否有死锁风险 | 审查锁获取顺序 |

### 维度 D：工程巧思

| 方向 | 具体问题 | 分析方法 |
|------|---------|---------|
| D1 编译优化 | `cargo-hakari` 是否已初始化（Phase 1 已安装但未 init） | 检查 workspace-hack crate |
| D2 CI/CD | `.github/workflows` 是否有自动化 gate | 检查 CI 配置 |
| D3 错误处理 | 是否统一用 `thiserror`/`anyhow` 还是混用 | grep error 类型 |
| D4 测试覆盖 | 关键路径（JSONL 存储、auth、tool 执行）是否有测试 | 统计测试覆盖率 |
| D5 文档 | 公共 API 是否有 doc comment | `cargo doc` 检查 |

### 维度 E：用户体验

| 方向 | 具体问题 | 分析方法 |
|------|---------|---------|
| E1 启动速度 | `--version` 0.02-0.08s，TUI 启动到可交互的延迟 | 实测计时 |
| E2 流式体感 | 60fps 帧合并是否在所有模型速度下都流畅 | 不同模型实测 |
| E3 错误消息 | 用户可见错误是否友好（不含内部 jargon） | 检查 error 消息 |
| E4 子代理 | `--agent`/`--agents` 是否在所有场景正常工作 | 实测各子代理类型 |
| E5 环境兼容 | 不同终端（Terminal.app/iTerm2/Alacritty/tmux）是否兼容 | 实测 |

---

## 三、子代理团队分工建议

### Agent 1：架构审计（DeepSeek-V4-Pro）

**区域**：crate 依赖链 + 死代码 + 过度抽象

任务：
1. 运行 `cargo tree --duplicates` 分析重复依赖
2. 运行 `cargo-udeps`（需 nightly）扫描死依赖
3. 对 64 个 crate 逐一检查：是否有零引用 crate
4. 搜索只有 1 个实现者的 trait（过早抽象）
5. 分析 `kimix-shell` 和 `kimix-tui` 的模块内聚度，是否可拆分
6. 检查 `kimix-tools` 中 codex/opencode/kimix 三套实现的重复代码

产出：crate 合并/拆分建议表 + 代码量削减估算

### Agent 2：Bug 猎手（Kimi-K3）

**区域**：unwrap/panic/unsafe/资源泄漏/并发安全

任务：
1. grep 非测试代码中的 `\.unwrap()`、`\.expect(`、`panic!`、`unreachable!`、`todo!`
2. 逐一判断每个是否用户输入可达
3. grep `unsafe` 块，审查安全论证
4. 检查文件句柄/锁/子进程的 RAII 保证
5. 检查 `as usize`/`as u64` 的溢出风险
6. 检查 `Mutex`/`RwLock` 的锁获取顺序

产出：一行消灭一类 bug 清单（按风险排序）

### Agent 3：工程实践（DeepSeek-V4-Pro）

**区域**：编译优化 + CI/CD + 错误处理 + 测试覆盖 + 文档

任务：
1. 检查 `cargo-hakari` 是否已初始化（Phase 1 安装了但可能没 init）
2. 检查 `.github/workflows` CI 配置
3. grep error 类型（thiserror vs anyhow vs 裸 String）
4. 统计关键 crate 的测试覆盖率
5. 检查公共 API 的 doc comment 覆盖
6. 检查 `justfile` 的 gate 命令是否完整

产出：工程改进清单 + 实施优先级

### Agent 4：实测验证（DeepSeek-V4-Flash）

**区域**：不同环境/不同对话/不同子代理功能测试

任务：
1. `kimix --version` / `--help` / 各子命令
2. `kimix -p "你好"` headless 模式（无 API 会 401，确认不崩溃）
3. `kimix mcp list` / `kimix acp --help` / `kimix import-kimi --help`
4. 检查 `~/.kimix/config.toml` 中各模型配置是否正确解析
5. 检查 `~/.kimix/agents/` 目录是否有 agent 定义文件
6. 检查 `~/.kimix/SKILL-ROUTER.md` 是否存在且格式正确
7. 检查不同终端类型（TERM 环境变量）下的兼容性

产出：功能测试报告 + 兼容性问题清单

### Agent 5：对抗性审查（GLM-5.2）

**区域**：交叉验证 Agent 1-4 的关键结论

任务：
1. 验证 Agent 1 的 crate 合并建议不会破坏编译隔离
2. 验证 Agent 2 的 bug 修复不会引入新问题
3. 验证 Agent 4 的功能测试是否覆盖关键路径

产出：SAFE/NEEDS_REVISION 裁定

---

## 四、关键文件索引

### 入口与核心
- `crates/codegen/kimix-bin/src/main.rs` — 二进制入口（jemalloc、runtime、命令分发）
- `crates/codegen/kimix-shell/src/agent/` — agent runtime 主循环
- `crates/codegen/kimix-tui/src/app/` — TUI 主循环

### 存储与持久化
- `crates/codegen/kimix-shell/src/session/storage/jsonl/mod.rs` — JSONL 存储（file_cache 已恢复）
- `crates/codegen/kimix-agent-memory/src/` — BM25 + 三级召回 + markdown 持久化
- `crates/common/kimix-compaction/` — 上下文压缩
- `crates/kimix-prompt/src/lib.rs` — prompt 构建 + context_budget_prune + auto_compact

### 工具系统
- `crates/codegen/kimix-tools/src/implementations/kimix/` — kimix 原生工具
- `crates/codegen/kimix-tools/src/implementations/opencode/` — opencode 移植
- `crates/codegen/kimix-tools/src/implementations/codex/` — codex 移植（如有）
- `crates/codegen/kimix-sandbox/src/profiles.rs` — 沙箱 profile（已加 deny）
- `crates/codegen/kimix-workspace/src/` — FS/VCS/permissions

### 认证
- `crates/codegen/kimix-shell/src/auth/` — OAuth + device flow + token refresh
- `crates/codegen/kimix-auth/src/` — 凭证管理

### 子代理
- `crates/codegen/kimix-subagent-resolution/src/` — 子代理定义
- `crates/codegen/kimix-shell/src/agent/mvp_agent/subagent_coordinator.rs` — 子代理 spawn
- `crates/codegen/kimix-agent/src/config.rs` — BuiltinAgentName 枚举
- `crates/codegen/kimix-agent/src/prompt/subagent_prompts.rs` — 子代理提示词

### 新增 crate（Phase 2）
- `crates/common/kimix-interjection-core/` — 用户中途插话
- `crates/codegen/kimix-tracing-macros/` — timed! 宏

### 配置
- `~/.kimix/config.toml` — 模型配置（`[model.xxx]` 格式）
- `~/.kimix/auth.json` — OAuth 凭证
- `~/.kimix/agents/` — 自定义 agent 定义
- `~/.kimix/SKILL-ROUTER.md` — Skill 路由索引

### 审计报告
- `docs/optimization-plan-2026-07-28.md` — v0.1.14 优化方案
- `docs/v0.2-improvement-plan.md` — v0.2 改进方案（三源整合）
- `docs/audit/startup-perf-audit-2026-07-28.md` — 启动区域审计
- `docs/audit-agent-runtime-performance.md` — Runtime 区域审计

### 工具链
- `justfile` — 一键开发循环（check/deps/test-jsonl/test-read-file/test-fuzzy/release）
- `renovate.json5` — 依赖自动更新
- `clippy.toml` — clippy 规则
- `deny.toml` — cargo deny 规则
- `rust-toolchain.toml` — 工具链锁定（1.97.0）

---

## 五、已知改进待实施（v0.2 方案 Phase 3-5）

### Phase 3：token 经济（1 周）
- 哈希去重（`sent_hashes: HashSet<u64>`，~80 行）
- outline 工具暴露 codebase-graph（~100 行）
- token 预算推广到所有工具（~100 行）
- short_prompt 模式（~200 行）

### Phase 4：权限 + 记忆（2 周）
- 权限三态 strict/auto/yolo（~200 行）
- session grants 前缀规则自动生成（~300 行）
- 决策轨迹记忆（~500 行）
- 文件写入原子化（~12 行）

### Phase 5：工程清理（持续）
- 配置加载去重（~30 行）
- 空闲帧退避（~25 行）
- search_replace 行号反馈（~5 行）
- markdown-core 移植（~1070 行）
- 合并 15 个叶子微 crate
- `cargo-hakari` 初始化

---

## 六、方法论提醒

1. **开发做减法，验收做加法** — 开发时奥卡姆剃刀（如无必要勿增实体），验收时墨菲定律（凡是可能出错的路径都按会出错来测）
2. **先回到本质，再接受挑刺** — 从第一性原理出发，不类比已有实现；完成后启动多 Agent 对抗性审查
3. **确定性收集直接做，代理只做判断** — 磁盘统计、文件计数、配置读取直接用 Bash；把结构化数据喂给代理做分析判断
4. **算清交易成本再拆分** — 任务拆解、上下文传递、结果验收都是交易成本，超过自己做的成本时留在内部执行
5. **一行消灭一类 bug 优先** — 最高杠杆的改动是 1 行修复一类问题
