# 支持 / Support

## 获取帮助 / Getting Help

- 📖 查阅 [README](README.md) 了解安装和基本使用
- 🐛 遇到 bug？在 [Issues](https://github.com/taxueseek/kimix/issues) 提交 bug 报告
- 💡 有功能建议？在 [Issues](https://github.com/taxueseek/kimix/issues) 提交功能请求
- 💬 使用问题或讨论？在 [Discussions](https://github.com/taxueseek/kimix/discussions) 发起

## 安全漏洞 / Security Vulnerabilities

请通过 [私有漏洞报告](https://github.com/taxueseek/kimix/security/advisories/new) 提交，
**不要**公开报告。详见 [SECURITY.md](SECURITY.md)。

Please report via
[private vulnerability reporting](https://github.com/taxueseek/kimix/security/advisories/new).
Do not open public issues. See [SECURITY.md](SECURITY.md).

## 常见问题 / FAQ

### 与官方 Kimi CLI 的关系？

Kimix 是一个非官方的社区构建版，与 Moonshot AI 无任何关联。
它基于开源代码独立开发，二进制名、配置目录、环境变量均独立于官方 CLI。

### Kimix 会收集我的数据吗？

不会。Kimix 是零遥测设计，唯一的出站连接是你自己配置的 API 端点
和 GitHub Releases（用于自更新）。不采集数据，不上传统计。

### 如何切换中文 / 英文？

在 TUI 的设置界面（`/settings`）中切换语言，即时生效并持久化。
也可以通过 `KIMIX_LANG=zh` 或 `KIMIX_LANG=en` 环境变量设置。
