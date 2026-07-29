# Changelog

本项目的所有重要变更均记录在此文件中。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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

## [Unreleased]

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
