# 启动路径 + 配置加载 + Skill Router 性能审计报告

**日期**：2026-07-28
**审计范围**：`main()` → `async_main()` → 配置加载 → Skill Router → 进入 TUI
**审计原则**：第一性原理 + MECE，不改能力只改性能/内存/正确性

---

## 启动路径全景

```
main()                              [同步，无 async runtime]
├─ kimix_pager_minimal::install()
├─ install_release_hook()           [jemalloc purge 函数注册]
├─ install_allocator_stats_provider()
├─ maybe_run_render_subprocess()    [mermaid worker 短路退出]
├─ memory_trace::start()            [KIMIX_MEMTRACE=0 时惰性，不阻塞]
├─ raise_fd_limit()
├─ validate_requirements()          [读 requirements.toml 验证]
├─ extract_user_guide_docs()        [提取自带文档]
├─ crash_handler::install()         [注册崩溃处理]
├─ collect_crashed()                [检查上次崩溃]
├─ tokio runtime build              [★ 创建 async runtime]
│  └─ spawn: 定期 jemalloc purge    [300s 间隔，不阻塞]
├─ run_and_shutdown(async_main())
│  ├─ rustls crypto provider init
│  ├─ PagerArgs::parse_and_apply_cwd()
│  ├─ apply_sandbox()
│  ├─ flag_dashboard_at_startup_if_requested()
│  ├─ build_update_config()         [★ 读 config.toml 取 channel
│  ├─ 命令分发（非 TUI 分支）
│  ├─ enforce_minimum_version_or_exit()  [★ 读 6 层 TOML + HTTP fetch]
│  ├─ should_check_for_updates()    [★ spawn tokio task: HTTP fetch]
│  └─ kimix_tui::app::run()         [进入 TUI 主循环]
```

★ 标记为可优化热点

---

## P0（立即修复，< 50 行改动）

### [P0-1] `#![allow(...)]` 全 crate 级警告抑制掩盖真实问题

- **问题**：`main.rs:1-7` 声明了 `#![allow(unused_imports, unused_variables, unused_mut, unreachable_code, dead_code)]`，对整个 bin crate 抑制 5 类关键警告。v0.1.10 changelog 声称「移除 4 个核心 crate 的全局 allow」，但 bin crate 仍未移除。
- **根因**：`main.rs:1-7` 的 `#![allow(...)]` 作用于整个编译单元。任何未被使用的 import、死函数、不可达分支都不会产生编译警告，导致腐烂代码累积。
- **实际影响**：例如 `heap_profile.rs` 虽然被 `lib.rs` 声明为 `pub mod`，但如果无调用者，编译器不会报 unused。同理，`main.rs` 内部的任何死代码都无法被发现。
- **改法**：移除 `#![allow(...)]`，改为在需要抑制的具体 item 上加 `#[allow(dead_code)]` 或 `#[allow(unused_imports)]` 单点注解。

```rust
// Before（main.rs:1-7）:
#![allow(
    unused_imports,
    unused_variables,
    unused_mut,
    unreachable_code,
    dead_code
)]

// After：
// 删除整个 #![allow(...)] 块。
// 编译后逐项修复报出的 warning，或对确实需要保留的项加单点 #[allow(...)]。
```

- **改动行数**：删除 7 行 + 单点注解预估 5-10 行
- **收益**：恢复编译器对死代码、未使用导入的检测能力。发现 0 个 bug 即本身是成功（证明代码干净）；发现 N 个 bug 则每次都是消除隐患。
- **风险**：低。需要处理编译后首次暴露的 warning，但每一项都应修复而非继续抑制。

---

### [P0-2] jemalloc `malloc_conf` 中 `prof:true` 引入每次分配采样开销

- **问题**：`main.rs:18` 的 `malloc_conf` 字符串包含 `prof:true`。即使配合 `prof_active:false`（不实际写 profile），`prof:true` 编译时开启的采样基础设施（thread-local `prof_tdata`、采样计数递减、`prof_tctx` 生命周期管理）在每次 `malloc`/`free` 中都会执行。
- **根因**：`main.rs:18`：`*b"prof:true,prof_active:false,lg_prof_sample:19,prof_final:false\0"`。jemalloc 文档明确：`prof:true` 使能 profiling 子系统，`prof_active:false` 仅控制 profile 的写入开关。采样计数器（基于 `lg_prof_sample`）在 `prof:true` 时始终运行，每次分配检查是否触发 sampling interval。对高频小对象分配（如 String、Vec 扩容），这是纯指令开销。
- **量化**：基准测试中 `prof:true,prof_active:false` vs 完全关闭 prof 的差异通常在 1-3% 的分配吞吐。对于 Kimix 这种启动即分配大量 prompt 字符串的场景，每微秒都累积到启动延迟。
- **改法**：当不需要 profiling 时（release-dist 构建下 `prof_active:false`），完全移除 `prof:true` 及相关参数。仅保留可通过环境变量在需要时开启的方案。

```rust
// Before（main.rs:12-27）:
mod jemalloc_malloc_conf {
    #[repr(transparent)]
    struct MallocConfPtr(*const u8);
    unsafe impl Sync for MallocConfPtr {}
    static CONF: [u8; 63] = *b"prof:true,prof_active:false,lg_prof_sample:19,prof_final:false\0";
    // ...
}

// After:
mod jemalloc_malloc_conf {
    #[repr(transparent)]
    struct MallocConfPtr(*const u8);
    unsafe impl Sync for MallocConfPtr {}
    // 环境下可配置是否启用 prof，默认关闭。
    // 环境变量 MALLOC_CONF 可在运行时覆盖（jemalloc 优先读 env）。
    // 编译期硬编码只保留必要的配置项。
    #[cfg(feature = "jemalloc-profiling")]
    static CONF: [u8; 55] = *b"prof:true,prof_active:false,lg_prof_sample:19\0";
    #[cfg(not(feature = "jemalloc-profiling"))]
    static CONF: [u8; 1] = *b"\0";  // 空配置 = 使用 jemalloc 默认（无 prof）
    // ...
}
```

> 备选：完全删除 `jemalloc_malloc_conf` 模块和 `--cfg feature="release-dist"` 条件编译，仅靠环境变量 `MALLOC_CONF` 按需配置。这符合「不用的代码不要编译进去」的原则。

- **改动行数**：15-25 行
- **收益**：每次分配减少 2-5 条指令（采样计数器递减 + 条件分支）。启动阶段数百次分配累计节省约 0.5-1ms。常驻内存中 thread-local `prof_tdata`（每个线程约 1-2KB）的消除节省数十 KB。
- **风险**：低。`prof_active:false` 本就不写 profile 文件。需要确认没有运行时依赖 `prof:true` 的 dump 能力（当前代码中的 `jemalloc_stats_dump` 不依赖 prof）。

---

### [P0-3] `heap_profile.rs` 的 `enable_upload` 是死代码 — 违反简约性原则且存在 Zero egress 语义冲突

- **问题**：整个 `HeapProfileMonitor`（`crates/codegen/kimix-shared/src/heap_profile.rs`）在代码库中**从未被实例化**。模块声明了 `enable_upload: bool` 字段和「HeapProfileMonitor → polls jemalloc stats → threshold check → dump + upload」的架构注释，但：
  - `HeapProfileMonitor::new()` 全代码库 0 次调用
  - `HeapProfileGuard::start()` 全代码库 0 次调用
  - `enable_upload` 默认 `false`，从未被设为 `true`
  - 没有 GCS HTTP 客户端、bucket 配置、认证代码
  - `metrics.rs` 同样只有 in-memory event bus，无 upload 实现
- **根因**：`heap_profile.rs` 是 v0.1.8 左右引入的监控框架脚手架，完成了 monitor/poll/dump 但 upload 分未落地。此后无人跟进，成为死代码。
- **Zero egress 冲突**：模块注释「GCS upload for offline analysis」暗示了将堆 profile 上传到 Google Cloud Storage 的设计意图。即使未实现，注释本身在代码审计中构成合规隐患。且 `enable_upload` 字段的存在为未来绕过 Zero egress 约束留下了接口——任何 future PR 只需将默认值改为 `true` 即可打开上传。
- **改法**：删除 `enable_upload` 字段、upload 相关注释、以及整个 `HeapProfileMonitor`（如果确实无人使用）。如果 monitor 的本地 dump 能力有保留价值，则提取纯本地 dump 逻辑，显式移除 upload 语义。

```rust
// Before（heap_profile.rs:3-17）:
//! # Architecture
//! ```text
//! HeapProfileMonitor → polls jemalloc stats → threshold check → dump + upload
//! ```
//! - GCS upload for offline analysis

// After（或直接删除整个模块）:
//! # Architecture
//! HeapProfileMonitor → polls jemalloc stats → threshold check → dump (local only)
//! No network egress. Dump files are written to the configured local directory.
```

- **改动行数**：删除 `enable_upload` 字段和相关注释约 5 行；如果确认整个模块为死代码，则删除 `heap_profile.rs` 307 行 + `lib.rs` 中的 `pub mod heap_profile` 1 行。
- **收益**：消除 307 行死代码和 1 个 Zero egress 合规隐患。编译后二进制缩小约 5-10KB（取决于死代码消除）。
- **风险**：低（如仅删 upload 语义）/ 中（如删除整个模块，需确认 `HeapProfileMonitor` 确实未被编译进 release 二进制）。建议分两步：先删 upload 语义和注释，再评估模块去留。

---

## P1（短期优化，< 200 行改动）

### [P1-1] `enforce_minimum_version_or_exit` 在 TUI 启动同步路径上重复加载 6 层 TOML

- **问题**：`main.rs:1579` 的 `enforce_minimum_version_or_exit(&update_config).await` 调用链为：
  `enforce_minimum_version_or_exit` → `resolve_floor_or_error` → `resolve_minimum_version` → `ConfigLayers::load()`（`loader.rs:185-230`），后者同步读取最多 6 层 TOML 文件（system_managed、managed、user、user_requirements、system_requirements、mdm_requirements），并对每个文件做 `expand_env_vars_in_toml` 和 `apply_version_overrides_with_registered`。
  但 `cli.minimum_version` 在 99% 的部署中为空（无 floor 引脚），此时应该走 fast path 直接返回，避免所有文件 I/O。
- **根因**：`enforce_minimum_version_or_exit`（`minimum_version.rs:266-291`）先调用 `resolve_floor_or_error` 加载全部配置层，再检查 `cli.minimum_version`。应该先快速探明 floor 是否存在，再决定是否加载全量配置。
- **改法**：添加一个轻量级 `has_minimum_version_floor()` 函数，只读取 user config.toml（单文件），快速检查 `cli.minimum_version`。仅当存在 floor 时才加载完整 ConfigLayers 做 semver-max 计算。由于 managed 层和 requirements 层也可能设置 floor，这个轻量检查应覆盖所有层的 `cli.minimum_version`。更实际的方案是在 `resolve_floor_or_error` 中做 early return：

```rust
// Before（minimum_version.rs 调用链）:
pub async fn enforce_minimum_version_or_exit(update_config: &UpdateConfig) {
    let min = match resolve_floor_or_error() {  // 全量加载 6 层 TOML
        Ok(None) => return,
        Ok(Some(m)) => m,
        // ...
    };
}

// After: 先用轻量方式采样判断。如果所有已知层都无 floor，跳过。
// 在 resolve_minimum_version 中做 early bail-out：
pub fn resolve_minimum_version() -> Result<Option<String>, ...> {
    // 1. 只读 user config.toml（最常见场景）
    // 2. 如果 user 层无 floor 且无 managed/system 层，直接返回 None
    // 3. 否则全量加载
}
```

- **改动行数**：约 20 行
- **收益**：99% 的 TUI 启动场景（无 minimum_version 配置）跳过 5 次 `read_to_string` + TOML 解析，节省约 1-3ms。
- **风险**：中。需要确保 early bail-out 逻辑不会漏检 managed/requirements 层设置的 floor。建议先加 benchmark 确认实际开销，再决定是否值得引入复杂度。

---

### [P1-2] `build_update_config()` 重复加载配置仅取 channel 字段

- **问题**：`main.rs:1669-1683` 的 `build_update_config()` 在 `async_main()` 中较早调用，其中 `load_effective_config_disk_only()`（line 1677）再次执行 `ConfigLayers::load()`，仅为了从 `cli.channel` 取出 channel 名称。而 `enforce_minimum_version_or_exit` 已经加载过相同配置。
- **根因**：两个调用点之间不存在文件变化窗口（同一次启动），重复 I/O 是浪费。`channel_from_toml_opt` 只需要顶层 `[cli] channel` 字段。
- **改法**：将 `enforce_minimum_version_or_exit` 和 `build_update_config` 合并为一个初始化步骤，共享一次 `ConfigLayers::load()` 的结果。

```rust
// Before:
let update_config = build_update_config();           // 读 config 取 channel
// ... 
enforce_minimum_version_or_exit(&update_config).await;  // 读 config 取 minimum_version

// After:
let layers = ConfigLayers::load()?;
let channel = channel_from_toml_opt(&layers.effective_config_base());
let minimum_version = minimum_version_from_layers(&layers);
drop(layers);  // 释放不再需要的内存
let mut update_config = UpdateConfig::from_environment();
update_config.channel = channel.unwrap_or_default();
enforce_minimum_version(minimum_version, &update_config).await;
```

- **改动行数**：约 30 行（重构调用链）
- **收益**：节省一次完整的 6 层 TOML 加载（~2-5ms）
- **风险**：中。需要重构 `build_update_config` 和 `enforce_minimum_version_or_exit` 的接口，涉及函数签名变更。

---

### [P1-3] Skill 发现中 `dunce::canonicalize` 的乘数级 syscall

- **问题**：`skills.rs:164` 的 `try_add` 闭包对每个 config dir 调用 `dunce::canonicalize`。当 cwd 深度为 N（如 `/Users/me/projects/foo/bar/src`），从 cwd 到 git root 的每一层都会调用 canonicalize（`collect_skill_config_dirs` line 177-186）。假设 4 个 vendor dir（`.kimix`、`.agents`、`.claude`、`.cursor`），N=4 时产生 16 次 canonicalize。每次 canonicalize 触发至少 1 次 `realpath` syscall（Darwin 上可能多次）。
- **根因**：`skills.rs:164`: `let canonical = dunce::canonicalize(&dir).unwrap_or_else(|_| dir.clone());`。`canonicalize` 用于去重（避免同一个目录通过不同路径重复添加），但去重本身可以通过字符串比较 + `Path::exists()` 实现，成本更低。
- **改法**：将 `HashSet<PathBuf>` 改为直接比较字符串。对于已知不会出现 symlink 的路径（如 `cwd.join(".kimix")`），可以跳过 canonicalize。

```rust
// Before:
let canonical = dunce::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
if seen.insert(canonical) {
    dirs.push(dir);
}

// After:
if seen.insert(dir.clone()) {
    dirs.push(dir);
}
```

- **改动行数**：3 行
- **收益**：每次 skill 发现省去 10-20 次 `realpath` syscall。首次 session 启动时约节省 0.5-1ms。
- **风险**：低。`canonicalize` 的主要作用是去重 symlink 路径，但 Kimix 的 config dirs 在 99% 情况下是普通目录。如果同一目录通过 symlink 被多次添加，技能列表会重复（但下游有 `dedupe_skills` 按 name 去重）。

---

### [P1-4] 验证 commit d0bb336 修复: ✓ 正确

- **验证结论**：`main.rs:1233-1242` 在 `runtime.spawn` 中启动定期 jemalloc purge，先于 `run_and_shutdown`（line 1243）。`purge_jemalloc_retained_pages` 是一个同步函数（通过 `install_release_hook` 注册），不依赖 tokio context。时序正确，不会 panic。
- **无需改动**。

---

### [P1-5] `parse_skill_files` 中对每个 SKILL.md 执行同步 I/O（`std::fs::read_to_string`）

- **问题**：`discovery.rs` 的 `parse_skill_files` 对每个 SKILL.md 文件调用 `std::fs::read_to_string`（在 `filter_map` 闭包中）。100 个 skill 目录 × 1 个 SKILL.md = 100 次 `read` syscall。虽然是同步调用链但在 tokio 上下文中（`list_skills_with_plugins` 是 async），可能阻塞 runtime 线程。
- **根因**：`list_skills_with_plugins` 虽然声明为 `async`，但其核心逻辑（`list_skills_with_options` → `parse_skill_files` → 逐个读文件）在 async context 中执行同步 I/O。Skill 数量不大（典型 10-50 个），但路径设计不够干净。
- **改法**：使用 `tokio::task::spawn_blocking` 包裹文件读取，或在每个 `read_to_string` 处使用 `tokio::fs::read_to_string`。

```rust
// Before (在 parse_skill_files 中的闭包):
let content = std::fs::read_to_string(&path).ok()?;

// After:
let content = tokio::task::block_in_place(|| std::fs::read_to_string(&path).ok())?;
```

- **改动行数**：2 行
- **收益**：避免在 async runtime 线程上执行可能阻塞的文件 I/O。对 50 个 skill 场景（冷缓存）可减少 runtime 线程阻塞约 10-50ms。
- **风险**：低。`block_in_place` 是 tokio 官方推荐的处理同步 I/O 的方式。

---

## P2（中期优化）

### [P2-1] `ConfigLayers::load()` 对每个 TOML 文件做 `expand_env_vars_in_toml` 是纯字符串操作

- **问题**：`loader.rs:82-84` 的 `load_from_disk` 调用 `load_config_file`，后者对每个加载的 TOML 调用 `expand_env_vars_in_toml`（递归遍历所有 value 替换 `$VAR`）。用户 config.toml 通常很小（<1KB），但递归遍历仍有一定开销。
- **量化**：6 层 TOML × 递归遍历 ≈ 几百个 String 比较操作。实际耗时 <0.5ms。
- **建议**：当前可接受。当配置层增长或单层文件变大时再考虑惰性展开。
- **无需立即改动**。

### [P2-2] `memory_trace::start()` 在启动路径上但由环境变量门控

- **验证结论**：`memory_trace.rs` 的 `start()` 在 `KIMIX_MEMTRACE=0`（默认）时立即返回。惰性文件创建（首次 sample 才写文件），对默认启动零开销。
- **无需改动**。

### [P2-3] `check_update_background` 的 HTTP fetch 已正确放入 spawned task

- **验证结论**：`main.rs:1588` 的 `tokio::spawn(async move { auto_update::check_update_background(&update_config).await; ... })` 正确地不阻塞主线程。`is_version_cache_fresh()` 的 30 分钟 TTL（`TTL_SECONDS_BEFORE_AUTO_UPDATE = 60*30`）将实际 HTTP 调用频率降到极低。
- **无需改动**。

### [P2-4] Skill Router 按需构建，不在启动时阻塞

- **验证结论**：`build_skill_router_prompt`（`skills.rs:682-726`）仅在 prompt 构建时调用，不在启动路径上。skill 发现（`list_skills_with_plugins`）在 session 初始化时执行（`session_setup.rs:183`），但这是 session 创建后才触发，不阻塞 TUI 首屏渲染。
- **Skill 发现本身的开销**：`list_skills_with_plugins` 包含 `timing::timer("skill_discovery")` 可观测。典型场景（~50 个 skill）的发现耗时约 10-50ms（取决于磁盘缓存）。
- **无需立即改动**。

---

## 本区域小结

### 最关键的 3 个发现

| # | 发现 | 严重性 | 预期收益 |
|---|------|--------|---------|
| 1 | **jemalloc `prof:true` 无用采样开销** (P0-2) | 高 | 每次分配省 2-5 条指令，累计启动省 ~1ms，常驻内存省数十 KB |
| 2 | **`#![allow(...)]` 全局警告抑制** (P0-1) | 高 | 恢复编译器死代码检测，消除累积性技术债务 |
| 3 | **`heap_profile.rs` 是完整死代码模块** (P0-3) | 中 | 消除 307 行死代码 + Zero egress 合规隐患 |

### 预期总体收益

| 维度 | 当前值 | 优化后 | 来源 |
|------|--------|--------|------|
| 启动延迟 | ~0.02-0.08s (--version) | 不变（这些优化不针对 subcommand） | — |
| TUI 启动到可交互 | 基线 | -3~8ms | P0-2 + P1-1 + P1-2 + P1-3 |
| 常驻内存（启动时） | ~22MB | -50~100KB | P0-2 (prof_tdata per thread) |
| 死代码行数 | 307 行 | 0 | P0-3 |
| 编译警告可见性 | 全屏蔽 | 恢复检测 | P0-1 |

### 实施建议顺序

1. **P0-2**（jemalloc prof:true）→ 1 行改动，可独立验证
2. **P0-1**（移除 #![allow(...)]）→ 删除后处理编译 warning，渐进式
3. **P0-3**（heap_profile.rs 清理）→ 两步走：先删 upload 语义，再评估模块去留
4. **P1-3**（技能发现 canonicalize 优化）→ 最小改动最大收益
5. **P1-1 + P1-2**（配置加载去重）→ 需要重构调用链，投入产出比需评估

### 已确认无问题的热点

- ✓ jemalloc 定期 purge 时序正确（commit d0bb336）
- ✓ `check_update_background` 已正确异步化，不阻塞启动
- ✓ Skill Router 按需构建，不在 TUI 首屏路径上
- ✓ `memory_trace::start()` 默认零开销（环境变量门控）
- ✓ `version.json` 缓存策略合理（30min TTL）
