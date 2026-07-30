# Kimix soft-nudge + content-dedup 交付（ACP 思路原生重写）

> 日期：2026-07-30  
> 分支：`opt/runtime-l-m-q-2026-07-29`  
> 前置：L/M/Q 首批（`a75d26d`）+ 归档 tag `archive/opt-runtime-l-m-q-2026-07-29`  
> 调研：`docs/analysis-opencode-acp-for-kimix.md`  
> 源项目：`https://github.com/ranxianglei/opencode-acp`（AGPL — **只吸收思路**）

---

## 状态梳理

| 阶段 | 状态 |
|------|------|
| L/M/Q 落地并归档 | 完成 |
| 目标仓库纠正为 opencode-acp（非 LeanToken） | 完成 |
| 源码下载至 `third_party_research/opencode-acp` | 完成 |
| 对照 + 可行性清单 | 完成 |
| 高 ROI 原生实现 | 完成 |
| `cargo test -p kimix-prompt` / `cargo check -p kimix-bridge -p kimix-shell` | **PASS** |

---

## 本批改动

### Soft efficiency nudge（~55% band）

| 项 | 值 |
|----|-----|
| 库 | `kimix_prompt::should_soft_efficiency_nudge` / `SOFT_EFFICIENCY_NUDGE` |
| 生产 | `kimix_recall::inject_recall_context_with_usage` ← `turn.rs` 传入真实 token 估算 |
| 条件 | usage ∈ (0.55, 0.80] of effective window；>0.8 留给 auto_compact |
| cache | 仅 prepend 到**当前** user 消息；历史不动 |

### Content-hash dedup（ingress-only）

| 项 | 值 |
|----|-----|
| 库 | `ContentHashDeduper`（FNV-1a 64，≥256 chars） |
| AgentPrompt | `record_tool_result` |
| 生产 | `tool_calls.rs` → `admit_tool_payload` 再 `push_tool_result` |
| cache | 只影响**新写入** payload；已入账消息不改写 |

### 文档

- `docs/analysis-opencode-acp-for-kimix.md`
- 本 handoff

---

## 回档

```bash
# 回到 L/M/Q 首批 commit（不含本批）
git checkout a75d26d

# 或回到 L/M/Q 归档 tag
git checkout archive/opt-runtime-l-m-q-2026-07-29

# 整批回 v0.1.15 基线
git reset --hard v0.1.15-baseline
```

行为级关闭（本批无独立 env；需要时再加 config）：

- soft nudge：改 `SOFT_NUDGE_RATIO` / 未来 config（当前固定 0.55）
- dedup：`PromptConfig.content_hash_dedup = false`（库路径）；生产路径暂始终开启

---

## 验证

| 检查 | 结果 |
|------|------|
| `cargo test -p kimix-prompt --lib` | 52 passed |
| `cargo check -p kimix-bridge -p kimix-shell` | PASS |
| soft_nudge / content_dedup 单元测试 | PASS |
| `test_truncate_tool_output` | 已修（ASCII token 预算） |

---

## 明确不做

- AGPL 代码合并 / compress·decompress 工具 / T1–T3 LSM 全栈
- 中途改写历史消息做 prune（破 prompt-cache）

---

## 下一批可选

1. `config.toml` 暴露 `soft_nudge_ratio` / `content_hash_dedup`
2. cancel/error 工具路径也走 `admit_tool_payload`
3. 会话 cache-hit 遥测
4. 自研模型侧 compact 建议（与现有 compaction 协同，非 ACP 移植）
