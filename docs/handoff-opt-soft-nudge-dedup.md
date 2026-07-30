# Kimix soft-nudge + content-dedup 交付

> 日期：2026-07-30  
> 分支：`opt/runtime-l-m-q-2026-07-29`  
> 前置：L/M/Q 首批（`a75d26d`）+ 归档 tag `archive/opt-runtime-l-m-q-2026-07-29`

---

## 状态梳理

| 阶段 | 状态 |
|------|------|
| L/M/Q 落地并归档 | 完成 |
| soft efficiency nudge + ingress content-hash 去重 | 完成 |
| `config.toml` / env 暴露 ratio 与 dedup 开关 | 完成（本分支后续 commit 区） |
| admit 全路径 + economy 统计 | 完成 |
| `cargo test -p kimix-prompt` / shell recall | **PASS** |

---

## 本批改动

### Soft efficiency nudge（~55% band）

| 项 | 值 |
|----|-----|
| 库 | `kimix_prompt::should_soft_efficiency_nudge` / `SOFT_EFFICIENCY_NUDGE` |
| 生产 | `kimix_recall::inject_recall_context_with_usage` ← `turn.rs` 传入真实 token 估算 |
| 条件 | usage ∈ (soft_ratio, 0.80] of effective window；>0.8 留给 auto_compact |
| 默认 soft_ratio | 0.55；`0` 关闭 |
| cache | 仅 prepend 到**当前** user 消息；历史不动 |

### Content-hash dedup（ingress-only）

| 项 | 值 |
|----|-----|
| 库 | `ContentHashDeduper`（FNV-1a 64，≥256 chars） |
| AgentPrompt | `record_tool_result` |
| 生产 | `push_admitted_tool_result` / `admit_tool_result_item`（全路径） |
| cache | 只影响**新写入** payload；已入账消息不改写 |

### 配置

```toml
[session]
soft_nudge_ratio = 0.55   # 0 = 关闭
content_hash_dedup = true
```

（env 以 shell resolve 为准，见 `resolve_soft_nudge_ratio` / `resolve_content_hash_dedup`。）

---

## 回档

```bash
# 回到 L/M/Q 首批 commit（不含 soft-nudge/dedup）
git checkout a75d26d

# 或回到 L/M/Q 归档 tag
git checkout archive/opt-runtime-l-m-q-2026-07-29

# 整批回 v0.1.15 基线
git reset --hard v0.1.15-baseline
```

行为级关闭：

- soft nudge：`soft_nudge_ratio = 0` 或对应 env
- dedup：`content_hash_dedup = false`

---

## 验证

| 检查 | 结果 |
|------|------|
| `cargo test -p kimix-prompt --lib` | PASS |
| `kimix_recall` economy / admit / soft_nudge | PASS |
| soft_nudge / content_dedup 单元测试 | PASS |

---

## 明确不做

- 中途改写历史消息做 prune（破 prompt-cache）
- 与现有 auto_compact 并行的第二套模型驱动 compress 工具链

---

## 后续可选

1. 会话 cache-hit 遥测（验证 soft band 是否抬高命中率）
2. 自研模型侧 compact 建议（与现有 compaction 协同）
3. TUI `status_line` 扩展
