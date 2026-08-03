# Kimix Harness 架构改进方案

> 版本：v1.1（2026-08-03）｜v1.0 基础上新增「三、内存上限架构」
> 范围：Harness（agent 决策循环）自研化 + 与上游 grok-build 差异化 + 长任务内存治理
> 前置阅读：`审计报告_运行时瓶颈_v0.1.15.md`、`evals/BASELINE.md`、`AGENTS.md`
> 目录：一、现状诊断 ｜ 二、Harness 自研化三层路线 ｜ 三、内存上限架构 ｜ 四、实施顺序 ｜ 五、总结

---

## 一、现状诊断：kimix 与上游的真正差距

### 1.1 资产盘点（自研 vs 继承）

| 层 | 自研 | 继承 grok-build |
|---|------|----------------|
| 记忆/检索 | FTS5+向量混合检索、时间衰减、MMR 重排、dream 整合 | — |
| 缓存工程 | 命中率落盘、压缩后预热、content-hash 去重 | — |
| TUI | 60fps、双语、12 主题 | 基础框架 |
| **Harness（决策循环）** | 仅有 `completionRequirement` 补丁 | **核心循环、工具协议、sandbox、sampler 全部继承** |
| **评测** | 5 题 evals + runner.py | 无 |

**结论**：kimix 的差异化资产集中在「记忆 + 缓存 + 体验」，而恰好是编程代理护城河的「Harness 编排 + 工具链生态 + 真实仓库迭代反馈」三块，全部还在上游骨架里。

### 1.2 最大瓶颈：没有「评测信号闭环」

DeepSeek 敢正面叫板，底气是「V4-Flash 用自研 Harness 跑过分」——即**用可量化的长任务评测反向驱动迭代**。kimix 当前：

- evals 只有 5 题，全是临时目录里的语法修复（`evals/cases/`）
- 没有真实仓库 diff 场景、没有长任务基准、没有跨模型矩阵
- 每次 prompt/工具改动「好不好」靠感觉（BASELINE.md 自述缺口）

没有评测信号，Harness 自研就没有方向；Harness 不动，就永远无法证明比上游强。

---

## 二、Harness 自研化的三层路线

### 第 1 层：把评测做成护城河（成本最低、见效最快）

**目标**：从 5 题扩到 20-50 题真实场景，进 CI，能证明「这版比上版强」。

具体动作：

1. **扩 evals 用例库**（对照 aider polyglot 方法论）：
   - 真实仓库 diff 场景：从开源仓库抽取真实 commit 的 bugfix 作为 fixture
   - 首轮解答 + 按测试报错修正两轮（polyglot 标准做法）
   - 覆盖现有 README 缺口的四类：大文件局部编辑、跨文件引用修改、长输出命令后判断、计划类多步任务
2. **runner.py 升级**：
   - 增加 `--model` 参数，跑跨模型矩阵（Kimi K3 / GLM 5.2 / DeepSeek / MiMo / LongCat）
   - 增加 `--repo` 模式：在真实仓库上跑（而非临时目录）
   - 输出 diff 级通过率，不只是 file_contains 正则
3. **进 CI**：`just gate` 里加 `evals`，通过率跌破 90% 或 edit_failures 趋势上升即拦截发布

### 第 2 层：Harness 决策循环解耦（架构自由度）

**现状**：agent loop 继承上游，记忆系统只是 pre-turn/post-turn 的 hook 补丁。
**目标**：把「决策循环」和「执行引擎」解耦，让记忆/规划成为一等公民。

设计（以 `kimix-agent` 现有 `Agent` 类型为基础扩展）：

```
Harness（决策循环，可插拔）
├── Planner       # 目标分解：读任务 → 产出 todo 计划
├── Executor      # 工具调用循环：选工具 → 执行 → 观察结果
├── Verifier      # 验证：跑测试 / diff 检查 → 判定完成或回退
└── MemoryHook    # 每轮 pre-turn 召回 + post-turn 写入（现有 BM25 系统）

可替换策略点：
- Planner 策略：一步直达 vs 先计划后执行（对应 plan mode）
- Executor 策略：单工具串行 vs 多工具并行
- Verifier 策略：无验证 vs 测试驱动 vs diff 驱动
```

**架构大胆改进方向**（选一个，别全做）：

- **A. 计划-执行-验证三段式循环**（对标 DeepSeek Harness）：模型先产 todo 计划，执行过程中每完成一项就跑验证，失败则回退重试。这是编程代理长任务的核心竞争力，也是当前继承循环最缺的。
- **B. 验证器插件化**：把「跑测试、diff 检查、语法检查」做成 Verifier trait，让 agent 能声明「我完成了，请验证」，Harness 自动执行验证并反馈。这是「真实仓库迭代反馈」的工程化落地。
- **C. 失败回流记忆**：eval 失败 case 自动写入记忆系统（按失败模式归类），下次同类任务先召回失败教训。形成「测→败→学→再测」自举循环。这是 DeepSeek 说的「找真实开发者补工具链短板」的自动化版本。

### 第 3 层：执行引擎性能硬化（已有审计报告，排期消化）

`审计报告_运行时瓶颈_v0.1.15.md` 的 P0 四项直接决定长任务体验：

| 优先级 | 问题 | 建议 |
|:---:|------|------|
| P0-1 | SSE 每 token 双解析 + 日志 I/O | 复用首次 Value 解析、日志降 trace |
| P0-2 | bash `read_to_end` 全量缓冲 | 迁移流式模式、read 循环中截断 |
| P0-3 | 每 100ms 全量 clone 输出缓冲 | 只传增量 / 共享引用 |
| P0-4 | git status 每轮 fork/exec | 统一走缓存路径、gix 库内实现 |

---

## 三、内存上限架构（解决 2-30G 卡死，最高优先级）

### 3.1 症状与根因

**症状**：长任务（20-30 轮）运行几分钟后内存飙到 3-4G，复杂任务冲到 20-30G 卡死。

**根因分层**（按内存贡献排序）：

| 层 | 路径 | 为什么无界 |
|---|------|-----------|
| **会话线性累积** | `persistence.rs` / `storage/jsonl/mod.rs` 每轮工具结果全部常驻内存 | 无轮数或 token 上限，20-30 轮 × 每轮几 MB 工具输出 = 数十 G |
| **JSONL 全量加载** | `load_session` 走 `read_to_string` + 全量 parse | 会话文件越大，加载时峰值内存 = 文件大小 × 2-3（原始 + parsed 对象） |
| **bash 大输出** | `read_stream` 的 `read_to_end`（P0-2）+ 每 100ms 全量 clone（P0-3） | 一条 `cat`/日志 dump 数 MB，300 次 clone = GB 级搬运 |
| **子代理叠加** | N 个子代理并行，每个 fork 父上下文 + 各自工具输出 | 4 个子代理 × 各自独立会话内存，父 + 子同时驻留 |
| **流式捕获** | `streaming_capture.rs` 已有 `STREAMING_CAPTURE_MAX_BYTES` 上限 | 已有防线，但只覆盖 extended-thinking，不覆盖上述路径 |

### 3.2 架构方案：全局内存预算（Memory Budget）三层管控

```
┌─────────────────────────────────────────────┐
│  全局内存预算（进程级，默认 2G，可配）          │
│  预算来源：会话内存 + 工具输出 + 子代理 + 缓冲  │
├─────────────────────────────────────────────┤
│  第 1 层：会话分页（Disk-backed Session）      │
│    会话按 N 轮/ M MB 分页，旧页落盘            │
│    内存只保留：最近 K 轮 + 活跃工具输出         │
│    模型上下文由 compaction 提供摘要（已有）     │
├─────────────────────────────────────────────┤
│  第 2 层：工具输出上限（Output Budget）        │
│    单条工具输出硬顶（如 10MB）→ 截断 + 落盘     │
│    运行中 bash 用流式截断，不再全量缓冲         │
│    超限输出只给模型 head/tail + 文件路径        │
├─────────────────────────────────────────────┤
│  第 3 层：子代理配额（Subagent Quota）         │
│    每个子代理分配独立内存预算（如 256MB）        │
│    超预算强制 compact 或 terminate             │
│    父代理只在子代理返回时接收摘要，不接收全文     │
└─────────────────────────────────────────────┘
```

**关键设计点**：

1. **会话分页是核心**。当前每轮工具结果全部常驻，是 20-30G 的主因。改成：
   - 会话页 `Page { id, items, byte_size, file }`，超过阈值（如 50MB）把最早页序列化到 `~/.kimix/sessions/<id>/pages/` 落盘，从内存中 drop。
   - 模型侧由 compaction 摘要承接（v0.1.15 已有压缩后预热，正好衔接）。
   - 用户回看历史时按需从磁盘页加载（lazy load），不整体读入。

2. **JSONL 加载改流式**。`load_session` 从 `read_to_string` 全量改为 `BufReader` 逐行 parse + 逐页加载，避免「文件大小 × 2-3」的峰值。

3. **工具输出预算**。在 `tool-runtime` 层统一收口（`crates/common/kimix-tool-runtime`）：
   - 所有工具输出经过 `OutputBudget::check(byte_size)`，超限截断 + 溢出部分落盘。
   - 模型只看到 head/tail + `(全文见 ~/.kimix/out/<tool_id>.txt)`。
   - bash 输出迁移到 `StreamingLocalTerminalRunner`（P0-2 修复项），运行中即截断。

4. **子代理配额**。`kimix-subagent-resolution` 的 spawn 流程加预算注入：
   - 子代理启动时声明 `memory_budget_mb`（默认 256）。
   - 每轮采样前检查，超预算强制 compact（复用 compaction）或 terminate。
   - 子代理返回只传 `summary + edited_paths`，不传完整 transcript（v0.1.16 的会话中间帧头尾摘要策略延伸到子代理结果）。

5. **内存可观测**。把 `~/.kimix/metrics/` 扩一张 `memory_usage` 表：
   - 每轮记录 `session_mb / tool_output_mb / subagent_mb / total_mb`。
   - 触发预算时记录 `budget_eviction` 事件。
   - 这样「3-4G 还是 30G」不再是玄学，每次优化有旧值→新值。

### 3.3 落地优先级（内存问题内部排序）

```
P0  会话分页 + 工具输出预算   ← 直接消灭 20-30G 的根因
P1  JSONL 流式加载           ← 消灭加载峰值
P2  子代理配额               ← 控制并行叠加
P3  内存可观测 metrics        ← 让前三个可验证
```

**验收标准**：
- 长任务（30 轮）峰值内存 ≤ 2G（当前基线：3-30G）
- 内存超预算时优雅降级（compact / 落盘），不卡死、不 OOM
- 会话回看历史功能不回退（lazy load 从磁盘页恢复）

---

## 四、建议实施顺序

```
Phase 0（先行）  内存上限架构：会话分页 + 工具输出预算 + JSONL 流式加载
Phase 1（并行）  评测护城河：evals 扩到 20 题、runner 加 --repo/--model
Phase 2（随后）  Harness 解耦：Planner/Executor/Verifier trait 落 kimix-agent
Phase 3（持续）  性能硬化：按审计报告消化 P0，每项补可复现测量
Phase 4（持续）  失败回流记忆：Phase 1 的 eval 失败自动写入记忆系统
```

**验收标准**（对应 BASELINE.md KPI 体系）：
- evals 用例 ≥ 20，通过率 ≥ 90% 可进 CI
- 同一 evals 集上多模型横向对比表
- Harness 三段式循环在真实仓库任务上可观测（计划→执行→验证 每步可见）
- P0 四项全部修复，且有旧值→新值测量记录
- 长任务峰值内存 ≤ 2G，超预算优雅降级不卡死

---

## 五、一句话总结

kimix 的记忆和缓存已经领先上游，但 Harness 决策循环还是上游的骨架。
最大瓶颈不是模型，是**没有评测信号闭环**——不知道这版比上版强在哪。
自研 Harness 的大胆方向不是重写循环，而是：内存预算先行（第 0 层，解决 2-30G 卡死）→ 评测先行（第 1 层）→ 决策循环解耦成可插拔三件套（第 2 层）→ 失败回流记忆自举（第 4 层）。
这才是 DeepSeek 用 Harness 跑 V4-Flash 时真正验证过的东西：**可量化的长任务编排能力**——而长任务编排的第一前提，是长任务不把自己跑死。
