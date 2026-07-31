# kimix v0.1.15 内存占用瓶颈审计报告

**审计日期**：2026-07-31
**审计范围**：TUI scrollback、视图状态、渲染管线、缓存层、jemalloc 配置、会话历史、视频帧、工具结果
**已修复项（不在本报告范围）**：prompt_texts 512 上限、updates.jsonl 两遍扫描、agent_edited_paths 1024 上限、粘贴图片 20MB/张、ChatStateActor 有界 channel 512、jemalloc 定期 purge

---

## P0 高危

### P0-1：对话 history 每轮 clone 全量到 API 请求

**位置**：[`crates/codegen/kimix-chat-state/src/actor/request_builder.rs:78`](crates/codegen/kimix-chat-state/src/actor/request_builder.rs#L78) 和 [`:107`](crates/codegen/kimix-chat-state/src/actor/request_builder.rs#L107)

**问题**：`build_conversation_request` 每次构建 API 请求都执行 `self.state.conversation.clone()`，即便不需要 prune / image compact 时也是整份 clone（line 107 hot path），需要 prune 时 line 78 先 clone 再 mut 操作。

**证据**：
```rust
// line 77-108
let items = if needs_mutation {
    let mut items = self.state.conversation.clone();  // line 78: 全量 clone + mut
    // ...
} else {
    self.state.conversation.clone()  // line 107: hot path，毫无理由的全量 clone
};
```

**量级估算**：
- `ConversationItem` 的字符串字段大多为 `Arc<str>`（system content、assistant content、tool result content、ContentPart::Text、ContentPart::Image url），clone 时只 bump refcount，不复制字节
- 但 Vec 本身的堆分配和枚举 discriminant 会被完整复制
- 200 条消息的会话，每条消息约 200 字节的枚举 + Vec 头开销 → clone 一次约 40KB（纯结构开销，字符串不复制）
- 看起来不大，但此 clone 每轮 API 调用执行 1 次，加上 mutation 路径的实际替换（`replace_conversation`）可能导致实际字符串在 compaction 期间被复制
- 真正的问题是：**这是不必要的 copy**，hot path 可以直接传 `&[ConversationItem]` 的 slice 引用，只在需要修改时才 clone

**修复建议**：hot path 直接使用 `Arc<[ConversationItem]>` 或 `&self.state.conversation` 引用，只在 mutation 路径 clone。由于 `ConversationItem` 内部已是 `Arc<str>`，可以让 `ConversationRequest.items` 改为 `Cow<'_, [ConversationItem]>` 或 `Arc<[ConversationItem]>`。

---

### P0-2：Scrollback 全量保留 + 每 entry 缓存渲染副本

**位置**：
- [`crates/codegen/kimix-tui/src/scrollback/state/mod.rs:45`](crates/codegen/kimix-tui/src/scrollback/state/mod.rs#L45) — `entries: IndexMap<EntryId, ScrollbackEntry>`，从不淘汰
- [`crates/codegen/kimix-tui/src/scrollback/entry.rs:12-20`](crates/codegen/kimix-tui/src/scrollback/entry.rs#L12-L20) — `CachedOutput` 保存完整 `RenderedBlockOutput`
- [`crates/codegen/kimix-tui/src/scrollback/types.rs:391-394`](crates/codegen/kimix-tui/src/scrollback/types.rs#L391-L394) — `BlockOutput { lines: Vec<BlockLine> }`
- [`crates/codegen/kimix-tui/src/scrollback/types.rs:140-171`](crates/codegen/kimix-tui/src/scrollback/types.rs#L140-L171) — `BlockLine` 含 `Line<'static>` 富文本 spans

**问题**：
1. `ScrollbackState.entries` 是 `IndexMap`，从不淘汰条目。每个 `ScrollbackEntry` 持有 `RenderBlock`（包含原始文本 String）和 `cached_output`（包含渲染后的 `BlockOutput` 即 `Vec<BlockLine>`，每个 BlockLine 有 `Line<'static>` 富文本 spans）
2. 这意味着每条消息有**两份内存副本**：源文本（在 RenderBlock 变体中）+ 渲染后文本（在 BlockOutput 的 BlockLine spans 中）
3. `EVICT_KEEP_MARGIN_ENTRIES = 128` 仅控制渲染缓存的 eviction，不控制条目本身
4. 200 轮会话，每轮约 10 个 block（tool calls + thinking + agent message）= 2000 entries

**量级估算**：
- 假设每轮 agent 回复 5000 字符 + 4 个 tool call 各 2000 字符 = 13000 字符
- 200 轮 × 13000 字符 = 2.6MB 源文本
- 渲染后 BlockLine spans 对每个 span 有额外 allocation（每个 styled segment 是独立的 `String`），通常放大 1.5-2 倍
- **总计**：源文本 2.6MB + 渲染缓存 3.9MB = **~6.5MB**（典型 200 轮会话）
- 大文件工具结果（如 50KB 的 read/edit 输出）会使此数字快速膨胀

**触发条件**：长会话（200+ 轮）、大量工具调用、大文件读取

**修复建议**：对 off-screen 的 entry 清除 `cached_output`（当前 `EVICT_KEEP_MARGIN_ENTRIES` 已做，但只清除渲染缓存且 entry 本身不清）。可以考虑对很旧的 entry（非最近 N 轮）清除 `block` 中的文本内容或用 `Arc<str>` 替换 `String` 以减少独立分配。

---

### P0-3：视频帧全量预提取 + 可能双份驻留

**位置**：
- [`crates/codegen/kimix-pager-render/src/prompt_images.rs:249-267`](crates/codegen/kimix-pager-render/src/prompt_images.rs#L249-L267) — `VideoViewerState { frames: Vec<Vec<u8>> }`
- [`crates/codegen/kimix-pager-render/src/prompt_images.rs:240`](crates/codegen/kimix-pager-render/src/prompt_images.rs#L240) — `MAX_VIDEO_EXTRACT_FRAMES = 120`
- [`crates/codegen/kimix-tui/src/app/agent_view/mod.rs:105-118`](crates/codegen/kimix-tui/src/app/agent_view/mod.rs#L105-L118) — `InlineVideoState { frames: Vec<Vec<u8>> }`（独立结构）

**问题**：
1. `VideoViewerState` 在 `open_from_path` 时 ffmpeg 预提取最多 120 帧到内存（12 秒 × 10fps），每帧是 PNG 或 JPEG 编码的完整字节
2. `InlineVideoState`（agent_view/mod.rs:105）是一个**独立的结构**，也有 `frames: Vec<Vec<u8>>`。如果视频既在 inline 显示又在 viewer 打开，帧数据存了两份
3. `MAX_VIDEO_EXTRACT_FRAMES = 120` 已从之前版本降低（handoff 文档说原来是 300 帧 60MB），但仍可观

**量级估算**：
- 120 帧 × 每帧缩放到 320px 宽（`VIDEO_MAX_WIDTH`），PNG 压缩后约 200KB/帧
- **VideoViewerState**: 120 × 200KB = **~24MB**
- **InlineVideoState**: 同上，**~24MB**
- 同时存在时：**~48MB**（双份）
- 如果 `VIDEO_MAX_WIDTH` 更大（如 640px），每帧可到 800KB+，120 帧 = **~96MB**

**触发条件**：用户在 scrollback 中有 inline 视频且同时点击打开 viewer

**修复建议**：让 `InlineVideoState` 和 `VideoViewerState` 共享 `Arc<Vec<Vec<u8>>>` 帧数据，避免双份。同时考虑 lazy loading：第一遍只加载前 10 帧，播放时按需提取后续帧（需要更精细的 ffmpeg 集成）。

---

## P1 中危

### P1-1：DashboardState 巨型结构（10,219 行）

**位置**：[`crates/codegen/kimix-tui/src/views/dashboard/state.rs`](crates/codegen/kimix-tui/src/views/dashboard/state.rs)

**问题**：`DashboardState` 包含大量 `BTreeSet`、`HashMap`、`PromptWidget` 等容器字段。`pinned: BTreeSet<DashboardRowId>`、`collapsed_sections: HashSet<SectionKey>`、`peek: Option<PeekPanelState>` 等在长会话中会增长。10,219 行 state 定义意味着每个 DashboardState 实例的静态大小就很大。

**量级估算**：
- 静态结构体大小（不含堆分配）：约 500-1000 字节（数个 BTreeSet/HashSet/Box）
- 动态堆分配：取决于会话中的 subagent 数量、计划步骤数等
- 在多子代理会话中，`pinned` BTreeSet 可能达到数百个条目
- **不估算具体数值**，但 DashboardState 是 AgentView 的核心状态，其大小直接影响整体内存

**触发条件**：复杂任务（多个 subagent、大量文件搜索）、大型仪表盘

**修复建议**：考虑将 dashboard 数据延迟加载或分页。在离开 dashboard 视图时释放部分缓存数据。

---

### P1-2：图片解码双缓冲

**位置**：[`crates/codegen/kimix-pager-render/src/prompt_images.rs:31-45`](crates/codegen/kimix-pager-render/src/prompt_images.rs#L31-L45)

**问题**：`ImageViewerState` 同时持有 `image_bytes: Vec<u8>`（原始编码字节）和 `display_bytes: Vec<u8>`（终端格式字节，如 PNG→PNG for Kitty 或 JPEG→PNG for iTerm2）。两者通常是同一份数据的不同格式，内存翻倍。

**量级估算**：
- 用户粘贴的图片限制 20MB/张（v0.1.15 已修复上限）
- 20MB 原始 + 20MB 显示格式 = **40MB**（一张最大图片）
- 典型场景（5MB 截图）：5MB + 5MB = **10MB**

**触发条件**：打开图片 viewer

**修复建议**：显示后立即释放 `image_bytes`（仅保留 `display_bytes`），或使用 lazy 转换（不保留两份）。

---

### P1-3：工具结果字符串在 scrollback block 中独立存储

**位置**：
- [`crates/codegen/kimix-tui/src/scrollback/blocks/tool/read.rs`](crates/codegen/kimix-tui/src/scrollback/blocks/tool/read.rs) — ReadToolCallBlock 含 `content: String`
- [`crates/codegen/kimix-tui/src/scrollback/blocks/tool/execute.rs`](crates/codegen/kimix-tui/src/scrollback/blocks/tool/execute.rs) — ExecuteToolCallBlock 含 stdout/stderr String
- chat-state 中的 `ToolResultItem.content: Arc<str>` 是同一份数据的共享引用

**问题**：工具结果同时在 chat-state（`ConversationItem::ToolResult` 的 `Arc<str>`）和 scrollback block（RenderBlock 变体中的 `String`）中各存一份。虽然 chat-state 的 clone 是 Arc bump，但 scrollback block 的 `String` 是独立分配。

**量级估算**：
- 100 次工具调用，每次平均 30KB 输出
- chat-state: 30KB × 100 = 3MB（但 Arc 共享，实际只一份底层数据）
- scrollback: **独立的** 30KB × 100 = **3MB**（额外副本）
- 大文件 read（500KB+）会使此额外地翻倍

**触发条件**：大量工具调用、大文件读取

**修复建议**：让 scrollback block 也使用 `Arc<str>` 引用 chat-state 中的同份数据，而不是 `String` clone。

---

## P2 低危

### P2-1：normalize_cache moka 缓存（已受控）

**位置**：[`crates/codegen/kimix-shell/src/session/normalize_cache.rs`](crates/codegen/kimix-shell/src/session/normalize_cache.rs)

**状态**：64MB 上限 + TTL（1h）+ TTI（15min），weigher 按 byte 加权，默认不启用（`enabled = false`）。受控，无需修复。

---

### P2-2：syntect 语法高亮静态缓存（已受控）

**位置**：[`crates/codegen/kimix-pager-render/src/syntax.rs`](crates/codegen/kimix-pager-render/src/syntax.rs)

**状态**：使用 `OnceLock<Syntect>` 懒加载 3 套主题，`include_bytes!` 嵌入编译期。静态数据量约 50-100KB/套，总计 < 300KB。受控。

---

### P2-3：TimelineEntry 每次构建新 Vec

**位置**：[`crates/codegen/kimix-tui/src/scrollback/state/timeline.rs:46-63`](crates/codegen/kimix-tui/src/scrollback/state/timeline.rs#L46-L63)

**问题**：`timeline_entries()` 每次调用都创建新的 `Vec<TimelineEntry>`，但每个 preview 限制 120 字符。200 轮 × 120 字符 = 24KB，且是短暂临时的。低危。

---

### P2-4：jemalloc 配置正确

**位置**：
- [`.cargo/config.toml:102`](.cargo/config.toml#L102) — Apple Silicon 16KB 页大小
- [`crates/codegen/kimix-bin/src/main.rs:1099-1118`](crates/codegen/kimix-bin/src/main.rs#L1099-L1118) — `arena.4096.purge`
- [`crates/codegen/kimix-tui/src/memory_release.rs`](crates/codegen/kimix-tui/src/memory_release.rs) — 覆盖全部 release 点

**状态**：jemalloc 全局分配器配置正确，release hook 覆盖了 session-load、reconnect-reload、subagent transcript replay、agent tab close、video teardown、image viewer close、rewind truncation 七大 cliffs。内存追踪（`memory_trace.rs`）可生成 JSONL 证据。受控，无需修复。

---

## 内存副本总数分析：一条消息从产生到发送给 LLM 的 clone 次数

以用户发送一条纯文本消息为例：

| 步骤 | 操作 | 数据形式 | 内存副本 |
|------|------|---------|:------:|
| 1 | 用户在 textarea 输入 | `String`（textarea buffer） | 原始 |
| 2 | `push_user_message` 存入 chat-state | `ConversationItem::User(ContentPart::Text { text: Arc<str> })` | Arc bump（共享底层 String） |
| 3 | scrollback `push_block(UserPromptBlock { text: String })` | `String`（独立分配） | **额外副本 1** |
| 4 | scrollback entry 渲染 → `cached_output` | `RenderedBlockOutput { output: BlockOutput { lines: Vec<BlockLine> } }` | **额外副本 2**（渲染为 ratatui `Line<'static>` spans） |
| 5 | `build_conversation_request` clone | 整个 `Vec<ConversationItem>` clone，内容为 `Arc<str>` (bump) | 副本 3（结构复制，字符串仅 bump refcount） |
| 6 | 序列化为 JSON 发送 | JSON 字符串（临时） | 副本 4（临时，发送后释放） |

**同样，tool result 也有 3 份副本**：
1. chat-state `ToolResultItem.content: Arc<str>`（共享，来自 persistence 反序列化）
2. scrollback block 变体中的 `String`（独立分配，额外副本 1）
3. scrollback entry 渲染缓存 `BlockOutput`（额外副本 2）

**总结**：每条消息在内存中存在 **3-4 份副本**（不含临时序列化）。其中：
- chat-state Arc<str> 是"真源"
- scrollback block String 是"展示源"（可改为 Arc 引用真源）
- rendered BlockOutput 是"渲染缓存"（权衡：保留以加速重绘 vs 清除以省内存）

---

## 修复优先级

1. **P0-1**：消除 request builder 的无意义全量 clone（最小改动，hot path 直接传引用）
2. **P0-3**：视频帧数据共享 `Arc`（最小改动，两行代码）
3. **P0-2**：scrollback 渲染缓存可对滚出视口的 entry 延迟驱逐（已有机制 `EVICT_KEEP_MARGIN_ENTRIES`，但 threshold 保守 = 128 entries ≈ 3-4 屏）
4. **P1-3**：让 scrollback block 也使用 `Arc<str>` 而非 `String`
5. **P1-2**：图片 viewer 释放原始字节
6. **P1-1**：Dashboard 大数据按需加载

---

## 最关键的 3 个瓶颈（<300 字）

**1. 对话历史每轮 API 调用全量 clone**（request_builder.rs:107）：即使 hot path 不需要修改数据，`self.state.conversation.clone()` 仍然每次复制整个 Vec。虽然字符串字段用 `Arc<str>` 不会复制底层字节，但每次 API 调用都产生不必要的分配。修复：hot path 传 `&[ConversationItem]` 引用。

**2. Scrollback entries 永久保留渲染缓存副本**（state/mod.rs:45）：每一条消息的源文本（在 RenderBlock 的 String 中）和渲染后的富文本（在 BlockOutput 的 BlockLine spans 中）各自独立存储。200 轮会话 = 2000+ entries，源文本 + 渲染缓存合计约 6.5MB+。且即使滚出屏幕，entry 本身及其 block 从不释放，只清除渲染缓存（且 keep margin 128 entries）。

**3. 视频帧双份驻留**（prompt_images.rs:251 + agent_view/mod.rs:109）：VideoViewerState 和 InlineVideoState 各自持有独立的 `frames: Vec<Vec<u8>>`，都是 120 帧预提取。同时打开时内存翻倍（~24MB × 2 = 48MB）。修复：用 `Arc<Vec<Vec<u8>>>` 共享帧数据。
