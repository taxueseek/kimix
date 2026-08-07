# Changelog

本项目的所有重要变更均记录在此文件中。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.1.23] - 2026-08-07

### Added
- **工具解耦搜索模型**（对齐上游 Grok `[models] web_search` / `GROK_WEB_SEARCH_MODEL`）：
  - 配置：`[models] web_search = "deepseek-flash-responses"`（或 `grok-4-5` 等）
  - 环境变量：`KIMIX_WEB_SEARCH_MODEL`（兼容 `GROK_WEB_SEARCH_MODEL`）
  - 聊天可用模型 A；`web_search` 工具优先用模型 B 的 Responses + 官方 `web_search`
  - Kimi coding `/search` 客户端路径（RRF/证据分）**保留**作后备；模型侧路失败可回落
  - 与会话主模型无关：换日常模型不强制换搜索通道

## [0.1.22] - 2026-08-07

### Fixed
- **web_fetch SSRF 误杀公网站点（Clash fake-ip）**：
  - 本机 DNS 把公网域名解析到 `198.18.0.0/15` / `fdfe:dcba:9876::/48`（Clash fake-ip）
    时，不再当成 ULA/内网拦截；错误形态曾是
    `SSRF blocked: docs.cursor.com resolves to private/internal IP fdfe:dcba:9876::…`
  - 检测到 HTTP(S)_PROXY / ALL_PROXY / 显式 `proxy_endpoint` 时，跳过**目标主机 DNS**
    SSRF（客户端连的是代理，不是目标 IP）；URL 里写死的私网 IP 字面量仍拦截
  - `web_fetch` HTTP 客户端自动读取环境代理，无需再配 `KIMIX_WEB_FETCH_PROXY`
- **web_search 403 配额文案与消耗**：
  - Kimi `POST {coding_base}/search` 的 403 归类为 **search subscription quota**
    （与 chat 配额、与 Grok/xAI 搜索后端无关），不再伪装成「服务不可用」
  - 先单次查询，成功后再做 RRF 扩展；403/401 立即停，避免 2–3× 配额浪费
  - `KIMIX_WEB_SEARCH_MULTI_QUERY=0` 可强制单查询

## [0.1.21] - 2026-08-07

### Fixed
- **「Responding…」粘滞（对照 v0.1.16 / 0.1.19 回归）**：
  - 工具开始时对 agent 消息 `finish_running`，不再只清指针导致最后一段一直像在流式输出
  - 状态行：最后一 chunk 后静默 **800ms** 即降级为 Waiting（不再挂「回复中」数分钟）
  - 主会话默认 `max_turns=50`（`KIMIX_MAX_TURNS=0` 可关），防止单 turn 100+ 轮工具环
  - turn-continuation 的 dangling-intent：只看**末句**、要求动作动词、intent 预算 2→1，
    避免答完又被「I'll…」误触发续跑（length/empty 续传仍保留）

## [0.1.20] - 2026-08-07

### Fixed
- **退出 / 取消一劳永逸收口**（相对 0.1.19 半吊子修复，用 0.1.20 做版本区分）：
  - Leader 全客户端断开 / cancel / auto-update / relaunch：改为
    `AgentActivity::flush_for_process_exit`——先 **await** `FlushComplete`
    落盘，再 `Shutdown` 等 actor 退出；去掉「发 RPC + sleep 500ms」竞态
  - `/exit` / quit 时若仍在 `TurnCancelling`：立刻本地 force-finish
    （不等看门狗超时），再走 agent flush
  - stuck-cancel 看门狗：每帧 animation tick 都跑，不再被 `app.tick()`
    返回 false 饿死；默认超时 15s → **5s**（`KIMIX_STUCK_CANCEL_TIMEOUT_SECS`）
  - 对准生产日志里「Esc 后 1–2 秒就退出」的真实操作路径
  - 保留 0.1.19 的 in-process `AgentShutdownGuard` join 路径，并补齐
    leader / quit 两条之前漏掉的口

### Added
- npm 分发包版本与 workspace 对齐到 **0.1.20**

## [0.1.19] - 2026-08-06

### Added
- npm 分发包 `kimix`（`npm/`）：`npx kimix` 即装即用，postinstall 从 GitHub
  Releases 下载对应平台二进制并校验 SHA256SUMS；发布前 `prepublishOnly`
  门禁校验 npm 版本与 GitHub release tag 对齐
- Homebrew formula 模板 `contrib/homebrew/kimix.rb`（自建 tap 后
  `brew install kimix`）
- README 安装章节新增 npx / npm、Homebrew、cargo install 三种安装方式
- **OSS-native 韧性**：
  - Chat Completions **方言**：`ChatCompletionsDialect` 按 `base_url` 推断
    （Kimi vs OpenAiCompat），避免向 OSS 端点泄漏 thinking 重写字段
  - **input_repair**（`kimix-tool-runtime`）：工具参数反序列化前修字符串化
    JSON、别名、标量类型
  - **PolicySandbox**（`kimix-sandbox::exec_transform`）：审批轴 × profile 轴；
    workspace-write 保护 `.git` / `.kimix/sandbox.toml`；纯 `BwrapPlan`
  - **pair heal**（`heal_conversation_pairs`）：加载与 BuildConversation 边界
    治愈 dangling / orphan / dedup，带遥测
  - **流错误三分**（`stream_triage`）：Repair / Retry / Surface；会话环路
    对 tool-pair 违规 heal 后 **每 turn 最多 resubmit 一次**
  - **feature_map**（`kimix-models`）：tools / parallel / thinking 轻量启发
  - 文档：`docs/oss-models.md` 验收清单；`docs/KNOWN_ISSUES.md` 多供应商说明
- **取消卡住 / 退出丢数据 / 搜索超时（首轮，有已知缺口）**：
  - 取消兜底超时：`TurnCancelling` 超时强制本地结束（初版 15s）
  - 退出前持久化：leader 断开时发 `flush_sessions`（初版仅 sleep 500ms，
    0.1.20 改为 await 真屏障）
  - 搜索超时收紧：web_search 180s → 60s
  - in-process 退出：`AgentShutdownGuard` flush + join（对照 grok-build）

## [0.1.16] - 2026-07-31

### Added
- 缓存命中率持续指标：每次采样响应落盘 `<kimix-home>/metrics/cache_hit-<日期>.jsonl`
  （按天分文件），进程退出输出窗口摘要并追加 `process_summary`，下次启动输出
  上一进程命中率；`KIMIX_CACHE_METRICS=0` 关闭，`KIMIX_METRICS_DIR` 覆盖目录
- 压缩后前缀预热：压缩替换 conversation 后，用新前缀 + 相同工具集发 1-token
  空请求，下一轮直接命中服务端缓存（`KIMIX_COMPACTION_PREWARM=0` 关闭）
- 子代理并发上限：`[subagents] max_concurrency`（默认 4，0=不限），超限请求
  在信号量上排队，队列计数可观测
- 批量并行 explore：`task` 工具新增 `count` 参数（1-16），同一 prompt 并行
  启动 N 个只读探索 worker 并按路合并摘要
- 前缀稳定性回归测试：相同工作流两次构建断言序列化字节级一致（含 prune 占位符路径）

### Changed
- 流式中间帧不再每 tick 重建全量 `output_for_prompt`（增量模式下由完成帧构建）
- bash truncated 渲染先按源行裁剪头尾预算再 word-wrap，长输出不再每 tick 全量 wrap
- `updates.jsonl` 中间帧（InProgress）持久化瘦身为头尾 4K + 显式省略标记，
  已完成帧保持全量；rewind 重建不受影响（实测某会话 75MB 中 ~99% 是中间帧）
- scrollback 单条内存护栏：Execute 块输出超 4MB 后仅保留 256K 头 + 64K 滚动尾窗
  + 省略标记，全量输出仍在工具 output_file 落盘

## [0.1.15] - 2026-07-30

## [0.1.15] - 2026-07-30

### Added
- 视频内存硬顶：ffmpeg 提取上限约 12s / 120 帧，长视频不再无界占内存
- 空闲 TUI tick 退避：无动画时指数退避，上限 250ms，减少空转 CPU
- 上下文与工具输出可配置：`[session].max_effective_context_tokens` 与 `KIMIX_MAX_EFFECTIVE_CONTEXT_TOKENS`（0=禁用 cap），工具输出上限 `KIMIX_MAX_TOOL_OUTPUT_CHARS`
- 上下文用量 soft nudge：用量进入软区间时注入轻量效率提示（`soft_nudge_ratio` 可配，默认 ~0.55）
- 工具 ingress content-hash 去重：相同内容不重复灌入上下文（`content_hash_dedup` 可关）
- 新增 `outline` 工具（codebase-graph 单文件符号大纲，无需 LSP）
- 流式重试 metrics 脚本：`scripts/analyze-retry-metrics.py`，可选门禁 M1 peak attempt

### Fixed
- 流式重试风暴：中流传输错误（EventStream/StreamError）重试上限从 15 收紧到 3（`STREAM_TRANSPORT_RETRY_THRESHOLD`）
- 尊重响应头 `x-should-retry: false` → 立即 Fatal；`max_retries == 0` 硬关闭重试
- TUI：失败 attempt 的流式半成品丢弃，避免「叠字」和多段重复答案

### Changed
- 有效上下文默认硬顶 200K tokens（可配置）；压缩阈值基于 `min(context_window, cap)`
- headless 抽取为独立 crate `kimix-headless`（不依赖 ratatui/crossterm），便于无头编译
- shell-base 类型解耦：纯数据类型下沉，shell 变更不再牵动整棵 TUI 类型重编译
- 内存边界：prompt_texts / updates.jsonl / 粘贴图片等路径增加上限与更稳妥的加载策略
- 渲染 cache：prepare_layout resize 容差 + 折叠组 header 缓存，大会话 resize 更轻
- auto_compact 阈值与 intra-compaction 默认策略收紧
- CI/clippy 与依赖审计门禁补全；清理无用 crate / 依赖链
- Cargo.toml 版本升至 0.1.15

## [0.1.14] - 2026-07-29

### Added
- read_file 安全防护：metadata 检查（is_dir/is_file/100MB 上限），消灭 OOM + FIFO/设备文件挂死
- fuzzy walker 线程 panic 防护：`unwrap()` → `is_err()` + warn，消灭用户输入触发的进程崩溃

### Fixed
- JSONL 存储：恢复文件句柄缓存（b646deb 原始设计），每条 append 从 6 次 syscall 降回 3 次
- 消除 `once_cell` 外部依赖缺失导致的编译失败（改用 `std::sync::OnceLock`）
- bash 截断提示修正：`first/last` 改为 `last`（实际只保留尾部，消除模型系统性误判）
- 移除 `main.rs` 全 crate 级 `#![allow(unused_imports, ...)]`，恢复编译器死代码检测能力
- 移除 12 处被全局 allow 掩盖的未使用 import 和变量警告

### Changed
- clippy 零警告：修复 fuzzy.rs unused_mut、session_lock.rs unused import
- Cargo.toml 版本升至 0.1.14

## [0.1.13] - 2026-07-26

### Added
- prompt cache 命中率可观测：两条流式路径响应完成时输出 cached_tokens
  与命中率 debug 日志（此前 record_on_span 为无调用方死代码）
- 系统提示新增 task_planning 段落：长任务先建 todo 并持续重写，
  计划背诵规避 lost-in-the-middle（Manus 机制）
- evals/ 行为回归评估 harness：零依赖 runner + 确定性断言，
  通过率与编辑失败痕迹双指标，首基线 5/5
- MCP user-guide 推荐服务器清单（Context7/Playwright/Chrome DevTools/
  GitHub）与工具过载治理建议

### Changed
- task_output 截断升级为头尾保留 + 显式标注（省略数/总数/取全文路径），
  尾部错误信息不再丢失，展示预算不增加
- ratatui 0.29 → 0.30 + ansi-to-tui 8：Backend 关联错误类型适配
  （BackendResultExt 统一映射），消除双 ratatui 版本共存
- rmcp 2.1 → 2.2（覆盖 MCP 2025-11-25 规范能力）
- release-dist 启用 thin LTO + codegen-units=1（运行时提速 10-20%）

### Fixed
- i18n 分层违规修复：下沉为叶子 crate kimix-i18n，配置语言解析改为
  启动时注入（set_config_resolver），消除 kimix-shell 对 TUI 的反向依赖
- kimix-crash-handler macOS 编译错误（libc::SIG_SETBLOCK 不存在于 macOS）
- 3 个归档前失败测试：工具名断言与实现契约一致化、estimate_tokens
  新契约断言更新

### Removed
- nucleo 从 helix git fork（rev 锁定）切换到 crates.io 0.5 正式版

## [0.1.10] - 2026-07-24

### Security
- 脱敏引擎 v2：10 组 LazyLock 编译正则 + RegexSet 快速路径（零分配短路），覆盖 xAI、GitHub PAT、AWS ASIA、OAuth 字段、PEM 密钥等，覆盖率 ~35% → ~95%
- 移除 4 个核心 crate 的全局 `#![allow(...)]`

### Changed
- kimix-core 精简：18 模块 → 9 模块，移除 8 个零引用模块
- workspace lint 收紧：`needless_lifetimes` 和 `single_range_in_vec_init` 改为 `warn`

## [0.1.9] - 2026-07-22

### Added
- BM25 + 3 层记忆召回：History / Working / Recency 分层，CJK 优化
- 12 套主题 + 6 种独立动画系统
- 完整中文 i18n：506 条翻译覆盖所有视图，运行时语言切换
- 文件驱动自定义命令：`~/.kimix/commands/*.md` + YAML frontmatter
- Skill router：关键词索引（~2K tokens）替代全量 skill 体（~40K tokens），按需加载

### Changed
- 上下文预算工具结果剪枝：自动清除已消费的输出，token 成本降低 ~41.7%
- 工具输出截断：单输出上限 10K tokens
- 全工作区 clippy 零警告、fmt 零差异
- Rust edition 2024 + toolchain 1.97.0

### Security
- 协议重命名：463 处 ACP 标识符统一为 `kimix/` 前缀
- 二进制命名：自更新路径统一为 `kimix` / `kimix.exe`

## [0.1.7] and earlier

早期版本为内部迭代，未公开记录。详见历史 release assets。
