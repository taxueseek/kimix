# Kimix runtime L/M/Q 优化交付（opt/runtime-l-m-q）

> 日期：2026-07-29  
> 基线 tag：`v0.1.15-baseline`（`8aa5f43`）  
> 分支：`opt/runtime-l-m-q-2026-07-29`  
> 说明：子代理因 API 配额失败，主代理按 Option F 直实施

---

## 回档

```bash
# 整批回退到 v0.1.15 基线
git checkout main
git reset --hard v0.1.15-baseline

# 或仅 revert 本批 commit（保留基线）
git revert <opt-commit-sha>

# 行为级回档（不改代码）
export KIMIX_MAX_EFFECTIVE_CONTEXT_TOKENS=0   # 禁用 200K cap
export KIMIX_MAX_TOOL_OUTPUT_CHARS=20000      # 恢复默认 bash 输出预算
```

---

## 本批改动

### M — 视频内存硬顶

| 项 | 值 |
|----|-----|
| 文件 | `crates/codegen/kimix-pager-render/src/prompt_images.rs` |
| 策略 | ffmpeg `-t 12s` + `-frames:v 120` + `load_frames` 二次 truncate |
| 常量 | `MAX_VIDEO_EXTRACT_FRAMES = 120`（约 10fps × 12s） |
| 收益 | 长视频不再无界堆帧；上界约 120 帧而非 300+ |
| 测试 | `video_frame_cap_bounds_memory` |

### L — 空闲 tick 指数退避

| 项 | 值 |
|----|-----|
| 文件 | `crates/codegen/kimix-tui/src/app/event_loop.rs` |
| 策略 | 连续 no-op animation tick → 间隔 ×2，上限 250ms |
| 函数 | `idle_tick_backoff` / `schedule_tick_with_backoff` |
| 收益 | sticky `needs_animation` 时避免空转满帧 |
| 测试 | `idle_tick_backoff_doubles_up_to_cap` |

### Q — effective context cap 可配置 + 工具输出预算

| 项 | 值 |
|----|-----|
| 配置 | `[session].max_effective_context_tokens` |
| Env | `KIMIX_MAX_EFFECTIVE_CONTEXT_TOKENS`（优先；`0`=禁用 cap） |
| 默认 | 200_000（与 v0.1.15 一致） |
| 贯通 | resolve → spawn → CompactionPolicy + PromptAdapter 80% 日志 |
| 工具 | `tool_output_chars_limit()` + `KIMIX_MAX_TOOL_OUTPUT_CHARS` |

示例 `~/.kimix/config.toml`：

```toml
[session]
max_effective_context_tokens = 150000  # 更早压缩
# max_effective_context_tokens = 0     # 用满模型窗口
```

---

## 验证

| 检查 | 结果 |
|------|------|
| `cargo check` shell/tui/tools/prompt/bridge/pager-render | PASS |
| `video_frame_cap_bounds_memory` | PASS |
| `max_effective_context_tokens` resolve tests | PASS |
| `idle_tick_backoff_doubles_up_to_cap` | PASS |
| `kimix-tools` tool_output 相关 | PASS |
| `kimix-prompt` `test_truncate_tool_output` | **pre-existing FAIL**（ASCII token 估算，非本批引入） |
| `app_view` esc_idle / stale_idle 过滤名命中 | **pre-existing FAIL**（`app_view.rs` 未改；pending quit 断言，与 event_loop 无关） |

---

## 未做（下一批）

- 视频真正「按当前帧 + 预取 10」流式解码（本批为硬顶 MVP）
- ~~哈希去重 / outline 工具~~ → **完成**：content-hash 工具 ingress 去重 + soft efficiency nudge + `outline`（见 `docs/handoff-opt-soft-nudge-dedup.md`）
- 多代理单 call 批派发
- Phase 3 feature gate / hakari
- 全 workspace `just gate`

---

## 子代理状态

| 角色 | 结果 |
|------|------|
| developer ×3 | 启动即 403 usage limit，未产出 |
| 主代理 | 完成 M/L/Q 首批落地 |
