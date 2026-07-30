## v0.1.15 — 重试不再刷屏，长视频不撑爆，上下文更有数

> 流式重试风暴、视频内存、上下文经济：三个用户痛点一起端。

---

### 「重试中」不再刷屏

一条请求断在中途，按理说重试几回就够了。以前默认重试 15 次，而且每次重试的半成品都可能叠在屏幕上——「叠字」「多段答案混在一起」。现在中流传输错误的重试上限缩到 3 次；如果服务端明确说「别重试了」（`x-should-retry: false`），直接停；`max_retries=0` 则完全不重试。TUI 里失败 attempt 的输出会丢弃，不再把半成品推到界面上。

附带离线分析脚本 `scripts/analyze-retry-metrics.py`（可选 `--gate` 检查 peak attempt）。

### 长视频不撑爆内存

拖长视频进去时，ffmpeg 提取硬性限制在约 12 秒 / 120 帧，避免无界堆帧把内存打爆。

### 上下文预算更清楚

有效上下文默认硬顶 200K tokens（可配置；设 0 禁用），压缩不必等到超大窗口才触发。用量进入约 55% 软区间时注入轻量效率提示；工具 ingress 按 content-hash 去重——同样内容不重复灌进上下文。

配置示例（`~/.kimix/config.toml`）：

```toml
[session]
max_effective_context_tokens = 200000  # 0 = 关闭硬顶
soft_nudge_ratio = 0.55                # 0 = 关闭 soft nudge
content_hash_dedup = true
```

环境变量：`KIMIX_MAX_EFFECTIVE_CONTEXT_TOKENS`、`KIMIX_MAX_TOOL_OUTPUT_CHARS` 等。

### 结构与工具

- 新增 `outline`：单文件符号大纲（tree-sitter，无需 LSP），适合先看结构再定点阅读
- headless 独立 crate、shell / TUI 类型解耦；内存边界与渲染缓存收紧

### Install

```sh
curl -fsSL https://raw.githubusercontent.com/taxueseek/kimix/main/install.sh | sh
~/.kimix/bin/kimix --version   # expect 0.1.15
```

已安装用户：`kimix update`（由 `KIMIX_AUTO_UPDATE` 控制）。

---

**Full Changelog**: https://github.com/taxueseek/kimix/compare/v0.1.14...v0.1.15
