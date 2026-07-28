# Kimix 系统性优化方案

> 日期：2026-07-28
> 方法：4 路 MECE 子代理并行审计（DeepSeek-V4-Pro x3 + Kimi-K3 x1）+ GLM-5.2 对抗性交叉验证 + 实测验证
> 基线：v0.1.13-post-all-opt，117.5 万行自研 Rust，60+ crate

---

## 〇、重新定义问题

Kimix 的核心价值不在「功能更多」，而在三个原子指标：

1. **单位 token 的任务成功率** — 上下文效率（对抗 context rot）
2. **单位时间的用户体感** — 流式延迟、内存占用、启动速度
3. **单位维护成本的代码寿命** — 死代码、警告抑制、安全面

本方案按「收益/改动行数」排序，只做 Top N。

---

## 一、已实施修改（本轮完成，已验证）

以下 4 处修改已实施并通过编译 + 104 个 JSONL 单元测试 + 功能验证。

| # | 文件 | 改动 | 改动行数 | 收益 | 风险 |
|---|------|------|---------|------|------|
| 1 | `kimix-shell/src/session/storage/jsonl/mod.rs` | 恢复 b646deb 的 file_cache 句柄缓存，修复 dev-runtime 的 metadata 二次确认 bug（双重 metadata 导致 torn check 被跳过） | +9/-2 | 每条 JSONL append 从 6 次 syscall 降到 3 次（open 缓存命中时 2 次），100 轮会话省 ~400 次 syscall | 低（104 测试全通过） |
| 2 | `kimix-shell/src/auth/xai_oauth.rs` | `once_cell::OnceCell` → `std::sync::OnceLock`，消除缺失依赖编译错误 | +2/-2 | 修复 P0 编译错误（`cargo check -p kimix-shell` 原本失败），消除外部依赖 | 零 |
| 3 | `kimix-workspace/src/file_system/fuzzy.rs` | `let mut next_scored` → `let next_scored`（移除不需要的 mut） | 1 行 | 消除 clippy 警告，满足「零警告」硬约束 | 零 |
| 4 | `kimix-workspace/src/session_lock.rs` | 移除 unused import `OpenOptions` | 1 行 | 消除 clippy 警告，满足「零警告」硬约束 | 零 |

### 修改 1 的关键决策过程

dev-runtime 子代理恢复了 file_cache 并增加了 metadata 二次确认。GLM-5.2 审查裁定 SAFE。但实测发现 1 个测试失败（`append_update_terminates_torn_trailing_line`）：`get_or_open_cached` 做了 metadata 检查返回 `current_len` 作为 `last_len`，但 `append_jsonl_line` 又做一次 metadata 得到相同值，导致 `len == last_len` 恒真，torn check 被跳过。

修复方案：回到 b646deb 原始设计——`get_or_open_cached` 不做 metadata，直接返回 `cached_len`。`append_jsonl_line` 的 `len != last_len` 检查已能正确检测外部修改（截断或追加都会改变 metadata.len()）。正常路径：缓存命中时 write + flush（2 次 syscall），首次打开时 open + metadata + torn check + write + flush。

---

## 二、优先级矩阵（待实施）

按「收益量级 / 改动行数」排序。P0 = <50 行改动且收益显著。

### P0：立即实施

| # | 区域 | 问题 | 改法 | 行数 | 收益 | 风险 | 审查裁定 |
|---|------|------|------|------|------|------|---------|
| P0-1 | 工具安全 | `read_file` 无界读取：模型可触发 OOM（读 10GB 文件）或永久挂死（读 FIFO/`/dev/zero`） | 在 `read_file/mod.rs:346` 前插入 metadata 检查：`is_dir()` 优先返回 IsADirectory，`!is_file()` 拒绝非常规文件，`len > 100MB` 拒绝 | ~12 行 | 消灭 OOM 攻击面 + FIFO 挂死，agent 不会被诱导读取 `/dev/zero` | 低 | NEEDS_REVISION（补 is_dir 优先） |
| P0-2 | 启动 | `main.rs:1-7` 的 `#![allow(unused_imports, unused_variables, unused_mut, unreachable_code, dead_code)]` 全 crate 级抑制 5 类警告 | 移除 `#![allow(...)]`，逐项修复暴露的 warning | ~7 行删除 + 修复 | 恢复编译器死代码检测能力，消除累积性技术债务 | 低 | SAFE |
| P0-3 | 启动 | `main.rs:18` jemalloc `prof:true` 即使 `prof_active:false` 仍在每次 malloc/free 执行采样计数 | 移除 `prof:true`，仅保留 `prof_active:false` 或完全删除 malloc_conf | ~5 行 | 每次分配省 2-5 条指令，常驻内存省数十 KB（thread-local prof_tdata） | 低 | SAFE |
| P0-4 | 启动 | `heap_profile.rs`（307 行）是完整死代码，`enable_upload` 字段 + GCS upload 注释与 Zero egress 硬约束冲突 | 删除整个模块或移除 upload 语义 | ~5 行（删 upload）或 308 行（删模块） | 消除 Zero egress 合规隐患 + 死代码 | 低 | SAFE |
| P0-5 | 工具正确性 | `bash/mod.rs:394` 截断提示语声称 `first/last` 但实际只保留尾部，模型基于错误前提推理 | `first/last` → `last` | 1 行 | 消除模型对截断输出的系统性误判 | 零 | SAFE |

### P1：短期实施（<200 行）

| # | 区域 | 问题 | 改法 | 行数 | 收益 | 风险 | 审查裁定 |
|---|------|------|------|------|------|------|---------|
| P1-1 | TUI 内存 | `ScrollbackState::entries` 无硬上限，8h 会话内存无限增长（可达 4-8GB） | 增加 `MAX_SCROLLBACK_ENTRIES`（建议 8192），驱逐**非 running** 的最旧条目 + 视觉提示 | ~35 行 | 8h 会话内存从无上限 → ~500MB hard cap | 低（须排除 running 条目） | NEEDS_REVISION |
| P1-2 | TUI 内存 | `/home` 退出会话不释放内存，旧 agent 渲染缓存悬空 | `dispatch_exit_session` 中调用 `release_retained_memory_with("exit-session")` | ~8 行 | 退出会话后内存释放 ~50-70% | 极低 | SAFE |
| P1-3 | 工具安全 | 沙箱默认 `deny: vec![]`，agent 可读 `~/.ssh/id_rsa`、`~/.aws/credentials` | 用 `dirs::home_dir()` 构造绝对路径 deny 列表（`~` 不展开！），deny `id_*`/`config` 但 allow `known_hosts`，不 deny `~/.cargo`/`~/.gitconfig` | ~15 行 | 凭据泄露面从「整个家目录可读」收敛到「内核级拒绝」 | 中（须正确构造路径） | NEEDS_REVISION |
| P1-4 | 工具稳定 | `glob` 工具无超时（grep 有），`rg --files` 在 NFS/巨型目录上挂起即工具永久悬挂 | 复用 grep 的 `grep_timeout()` 包裹 stdout 读取循环 | ~6 行 | 消灭 glob 工具永久挂起 | 低 | SAFE |
| P1-5 | 工具稳定 | bundled ripgrep 解包非原子，崩溃/并发解包留半个二进制，后续永久失败 | `fs::write` → `write tmp + rename`（POSIX 原子） | 3 行 x2 处 | 消除 bundle 用户的低频永久损坏 | 低 | SAFE |
| P1-6 | 工具正确 | `XAI_VALIDATE_TYPE_TIMEOUT_MS` 环境变量残留 xAI 命名，用户按 `KIMIX_*` 设置不生效 | 新名 `KIMIX_VALIDATE_TYPE_TIMEOUT_MS` + 旧名兜底 | 2 行 | 消除命名空间残留，零破坏 | 零 | SAFE |
| P1-7 | Runtime 可观测 | `context_budget_prune` 无 per-turn tracing，无法判断 prune 是否退化 | 增加 `tracing::debug!` 记录 removed/saved_tokens/turn | 5 行 | 无运行时代价的可观测性增强 | 零 | SAFE |
| P1-8 | 工具稳定 | `fuzzy.rs:94` `walk_handle.join().unwrap()`，walker 线程 panic 传播崩溃整个 agent | `unwrap()` → `is_err()` + warn + 重启 | 1 行 | 消灭用户输入触发的进程崩溃 | 低 | SAFE |

### P2：中期改进

| # | 区域 | 问题 | 改法 | 行数 | 收益 |
|---|------|------|------|------|------|
| P2-1 | 工具正确 | 文件写入非原子（裸 `fs::write`），崩溃即用户文件损坏 | 同目录 tmp + rename（POSIX 原子） | ~12 行 | 消除崩溃损坏用户源代码的尾部风险 |
| P2-2 | 工具正确 | `search_replace` 多重匹配只告诉「找到多处」不给行号 | `positions` 已在手，format 进消息 | ~5 行 | 模型一次即可自我纠正 |
| P2-3 | 启动性能 | `enforce_minimum_version_or_exit` + `build_update_config` 重复加载 6 层 TOML | 合并为一次 `ConfigLayers::load()` | ~30 行 | 启动省 ~3-5ms |
| P2-4 | 启动性能 | Skill 发现中 `dunce::canonicalize` 对每个 config dir 调用，N 层 cwd x 4 vendor dir = 16 次 realpath | 改用字符串比较去重 | 3 行 | 首次 session 启动省 ~1ms |
| P2-5 | TUI 性能 | 空闲时 16ms 绘制间隔产生空帧，消耗 CPU | 连续 N 帧无变化时间隔指数退避 | ~25 行 | 空闲 CPU 降 50-70% |
| P2-6 | 工具正确 | bash 截断策略改为头尾各保留一半 + 中间 elision 标记 | 重写 `truncate_buffer` | ~15 行 | 模型同时看到开头和结尾 |

---

## 三、对抗性审查关键结论

GLM-5.2 对 5 个关键决策的交叉验证：

| 决策 | 裁定 | 核心修正 |
|------|------|---------|
| JSONL file_cache 恢复 | **SAFE** → 实施后发现 bug → **已修复** | metadata 二次确认导致 torn check 被跳过；回到 b646deb 原始设计 |
| once_cell → OnceLock | **SAFE** | Rust 1.97.0 远超 1.70 稳定版本，标准用法 |
| scrollback 硬上限 | **NEEDS_REVISION** | 必须排除 running 条目，否则静默丢弃 agent 流式输出 |
| 敏感路径 deny | **NEEDS_REVISION** | `~` 不被展开导致 deny 完全无效；必须用 `dirs::home_dir()`；不 deny `~/.cargo`/`~/.gitconfig` |
| read_file 无界读取 | **NEEDS_REVISION** | 须补 `is_dir()` 优先返回 IsADirectory，再检查 `is_file()` 和 size |

---

## 四、实测验证结果

| 验证项 | 结果 |
|--------|------|
| `cargo check -p kimix-shell` | 通过（修复 once_cell 后） |
| `cargo check -p kimix-workspace` | 零警告（修复 unused_mut + unused import 后） |
| JSONL 单元测试（104 个） | 全通过（修复 file_cache torn check bug 后） |
| `kimix --version` | 正常（0.02-0.08s，RSS 22MB） |
| `kimix --help` / 子命令（acp/mcp/import-kimi/update/login） | 全部正常 |
| `kimix -p` headless 模式（无 API） | 正确返回 401 错误，不崩溃不挂死 |
| `--agent` / `--agents` 子代理定义参数 | 正常解析 |

---

## 五、设计亮点确认（无需修改）

以下设计经子代理验证确认成熟，无需改动：

| 设计 | 验证结论 |
|------|---------|
| 流式帧合并（`min_draw_interval` 16ms + ACP batch 32 条/帧） | 正确实现，token 到达快于渲染时合并丢弃中间帧 |
| 增量 Markdown 解析（checkpoint 冻结 + 尾部分离） | O(N²) → 近似 O(N)，任意长度解析 <1ms |
| 未闭合块保守渲染 | checkpoint 只在 depth=0 创建，嵌套块天然重渲染无闪烁 |
| mermaid out-of-process 渲染 | 单线程 worker + panic=abort 隔离 + 3s 超时 + 200MB 磁盘缓存 |
| memory_release edge-triggered 机制 | 12 个内存悬崖全覆盖 + 帧延迟 purge + attribution tracing |
| memory_trace 零开销 | `is_active()` 单次 RwLock::read，关闭时不采样不记录 |
| 脱屏渲染缓存驱逐 | 每 5s sweep，保留 viewport ±128 条目 |
| prompt cache 前缀稳定性 | 系统提示渲染一次后缓存整个 session 不变 |
| compaction 原子性保护 | temp-file + rename 策略 + corruption tolerance + quarantine |
| bash 超时 | 默认 FG 120s + max_timeout 5min + 超时转后台（已实现，报告「无超时」有误） |
| edit 失败反馈 | `build_nearest_match_hint` + Unicode confusable 诊断（已实现） |
| grep/glob 性能 | rg 子进程 + head limit + 超时 + 早杀（已实现） |

---

## 六、实施路线图

### 第一批（已完成）：修复编译 + 恢复性能优化

- [x] file_cache 恢复 + torn check 修复
- [x] once_cell → OnceLock
- [x] clippy 零警告修复

### 第二批（P0，建议立即实施）：一行消灭一类 bug

1. `read_file` 无界读取防护（~12 行）
2. 移除 `#![allow(...)]`（~7 行）
3. 移除 jemalloc `prof:true`（~5 行）
4. 删除/精简 `heap_profile.rs` upload 语义（~5 行）
5. bash 截断提示语修正（1 行）

### 第三批（P1，两周内）：内存 + 稳定性

6. scrollback 硬上限（排除 running 条目）（~35 行）
7. 退出会话释放内存（~8 行）
8. 敏感路径默认 deny（用 `dirs::home_dir()`）（~15 行）
9. glob 超时（~6 行）
10. rg 解包原子化（6 行）
11. xAI env var 残留清理（2 行）
12. context_budget_prune tracing（5 行）
13. fuzzy join panic 防护（1 行）

### 第四批（P2，月度）：正确性 + 微优化

14. 文件写入原子化（~12 行）
15. search_replace 多重匹配行号反馈（~5 行）
16. 配置加载去重（~30 行）
17. Skill 发现 canonicalize 优化（3 行）
18. 空闲帧退避（~25 行）

---

## 七、预期总体收益

| 维度 | 当前 | 优化后 | 来源 |
|------|------|--------|------|
| JSONL append syscall | 6 次/条 | 2-3 次/条 | file_cache 恢复 |
| 编译状态 | `cargo check -p kimix-shell` 失败 | 通过 | OnceLock 修复 |
| clippy 警告 | 2 个（workspace） | 零 | unused_mut + unused import 修复 |
| 8h 会话峰值内存 | 无上限（4-8GB） | ~500MB hard cap | scrollback 硬上限 |
| 退出会话后内存 | 不释放 | 释放 ~50-70% | exit-session release |
| read_file OOM 风险 | 无防护 | 100MB + 非常规文件拦截 | metadata 检查 |
| 凭据泄露面 | 整个家目录可读 | 内核级拒绝 | 默认 deny 规则 |
| glob 工具挂死 | 无超时 | 复用 grep 超时 | timeout 包裹 |
| 死代码 | 307 行（heap_profile） | 0 | 模块清理 |
| 启动分配开销 | prof:true 采样 | 无采样 | malloc_conf 精简 |
| 模型截断误判 | 提示语撒谎 | 提示语准确 | 1 行修正 |

---

## 八、方法论说明

### 子代理团队配置

| 代理 | 模型 | 区域 | 角色 |
|------|------|------|------|
| dev-startup | DeepSeek-V4-Pro | 启动/配置/SkillRouter | 性能审计 |
| dev-runtime | DeepSeek-V4-Pro | AgentRuntime/会话存储/上下文 | 性能审计 + 实施 |
| dev-tui | DeepSeek-V4-Pro | TUI渲染/流式/内存 | 性能审计 |
| audit-tools | Kimi-K3 | 工具系统/工作区/安全 | Bug 审查 |
| audit-glm | GLM-5.2 | 5 个关键决策 | 对抗性交叉验证 |

### 验证原则

- 开发做减法（奥卡姆剃刀），验收做加法（墨菲定律）
- 生产方和审查方来自不同模型家族（DeepSeek 写，GLM/Kimi 审）
- 每个修改经「子代理分析 → 对抗审查 → 实测验证」三重确认
- 一行消灭一类 bug 优先于多行优化
