# opencode-acp → Kimix 对照分析

> 日期：2026-07-30  
> 源码：`Documents/GPT/third_party_research/opencode-acp`（`https://github.com/ranxianglei/opencode-acp`）  
> 许可：**AGPL-3.0** — 只吸收思路，不复制代码 / 不合并依赖  
> Kimix 许可：Apache-2.0  
> 分支：`opt/runtime-l-m-q-2026-07-29`  
> L/M/Q 归档：`archive/opt-runtime-l-m-q-2026-07-29`

---

## 1. 目标项目是什么

**Active Context Pruning（ACP）** 是 OpenCode 的 TypeScript 插件，核心主张：

- 把上下文压缩的**时机与范围**交给模型（`compress` / `decompress` 工具），而不是硬截断历史
- 三级 LSM 压缩：T1 捕获 → T2 蒸馏 → T3 浓缩
- 软提示（nudge）驱动模型主动压缩，100% 时 GC 兜底
- 实测：上下文 p90 多在 150K–190K，聚合 prompt-cache 命中约 91%

**不是** LeanToken / 其他项目。

---

## 2. 与 Kimix 的架构对照

| 维度 | opencode-acp | Kimix（本批前） |
|------|--------------|-----------------|
| 语言 / 形态 | TS 插件（OpenCode） | Rust 原生 CLI / agent |
| 压缩决策 | 模型调 `compress` | `auto_compact` + compaction pipeline |
| 动态提示 | tail / 当前消息 suffix nudge | system-reminder 注入当前 user 消息 |
| 历史改写 | **刻意避免** mid-history prune（会破 cache） | stable prefix + 不改写已发送消息 |
| 有效窗口 | maxContextLimit ~55% 起 nudge | `max_effective_context_tokens` 默认 200K（L/M/Q 的 Q） |
| 工具输出 | 模型压缩 + 保护工具列表 | 字符预算 + ingress 截断 |
| 许可 | AGPL-3.0 | Apache-2.0 |

### ACP 对 prompt-cache 的关键教训

1. **不要改写已发送历史**（中途改 tool output / 插入中间消息 → cache miss）
2. **动态内容只放当前 turn 的 tail / user 前缀**
3. **上下文长期压在窗口的 10–20% 比「堆到 80% 再压一次」更省**（未缓存部分每次全价）
4. **软信号先于硬压缩**（~55% efficiency band → 再 hard compact）

Kimix 已有：4-layer stable prompt、`context_budget_prune`、200K effective cap、auto_compact 75%。

---

## 3. 可行性判定（可原生吸纳的机制）

| 机制 | ROI | 风险 | 本批 | 说明 |
|------|-----|------|------|------|
| 软效率 nudge（55% band） | 高 | 低 | **已做** | 仅挂当前 user 消息；不碰历史 |
| 工具 payload content-hash 去重（ingress-only） | 高 | 低 | **已做** | 二次相同大输出 → stub；先前消息不动 |
| 有效 200K cap（Q） | 高 | 低 | 已有 | L/M/Q 首批 |
| 工具输出预算（Q） | 高 | 低 | 已有 | `KIMIX_MAX_TOOL_OUTPUT_CHARS` |
| 模型驱动 compress 工具 + T1/T2/T3 LSM | 中高 | 高 | **不做** | 体量大、AGPL 边界、与现有 compaction 重叠 |
| decompress / search_context 工具 | 中 | 中 | 不做 | 依赖 compress 块存储 |
| GC 100% 截断摘要 | 中 | 中 | 已有类似 | auto_compact / overflow 路径 |
| 质量门 ROUGE | 低 | 中 | 不做 | 需压缩块语料 |
| 改写历史做 prune | **负** | 高 | **禁止** | ACP 自己也修过 cache 破坏 |

---

## 4. 本批原生实现（思路重写，非移植）

### 4.1 Soft efficiency nudge

- **库**：`kimix_prompt::should_soft_efficiency_nudge` + `SOFT_EFFICIENCY_NUDGE`
- **生产路径**：`kimix_recall::inject_recall_context_with_usage`，由 `turn.rs` 传入 `get_estimated_total_tokens` + effective window
- **条件**：`soft_ratio < usage ≤ 0.8`（默认 soft=0.55）；超过 0.8 留给 hard auto_compact
- **cache**：只 prepend 到**当前** user 消息的 `<system-reminder>`

### 4.2 Content-hash dedup（ingress-only）

- **库**：`ContentHashDeduper`（FNV-1a 64）
- **AgentPrompt**：`record_tool_result` 走 dedup
- **生产路径**：`tool_calls.rs` 在 `push_tool_result` 前 `admit_tool_payload`
- **规则**：≥256 字符才参与；命中则短 stub；**不修改**已入历史的消息

### 4.3 明确不做

- 不引入 AGPL 代码或 `context-compress-algorithms`
- 不实现 compress/decompress 工具链
- 不中途改写 chat history 做「后置 prune」

---

## 5. 与 L/M/Q 的关系

| 项 | 状态 |
|----|------|
| M 视频帧硬顶 | 已归档 tag |
| L 空闲 tick 退避 | 已归档 tag |
| Q effective context + 工具输出预算 | 已归档 tag |
| 本批 soft nudge + content dedup | L/M/Q 之上的 cache/token 续作 |

回档 L/M/Q：`git checkout archive/opt-runtime-l-m-q-2026-07-29`  
回档基线：`v0.1.15-baseline`

---

## 6. 后续可选（未做）

1. 把 soft_nudge_ratio / content_hash_dedup 暴露到 `config.toml`
2. 工具链全路径 dedup（error / cancel 路径）
3. 模型侧 `compact` 建议工具（自研，非 ACP 移植）— 需单独设计与 compaction 协同
4. 会话级 cache hit 遥测，验证 soft band 是否抬高命中率

---

## 7. 结论

**可以运用到 Kimix，但只能吸收机制，不能合 AGPL 源码。**

最高 ROI 且不伤 Kimix 优势（Rust 原生、stable prefix、现有 compaction）的两条已落地：

1. 软效率信号（提前节流，非 panic）
2. 入口去重（重复大工具输出不再二次入账）

三级 LSM + 模型 compress 留给远期；当前 auto_compact + 200K cap + 本批两项已覆盖「长会话低 token + 高 cache 稳定性」主路径。
