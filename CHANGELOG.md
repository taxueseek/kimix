# Changelog

本项目的所有重要变更均记录在此文件中。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- 项目重新定位为通用终端 AI 代理工具，不限于编程场景
- README 重写为全中英双语

### Removed
- 清理内部设计文档和研究材料，仅保留公开使用文档

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
