# Kimix 用户指南

## 目录

1. [安装](#安装)
2. [登录与认证](#登录与认证)
3. [基本使用](#基本使用)
4. [模型切换](#模型切换)
5. [供应商配置](#供应商配置)
6. [自定义命令 / Skills](#自定义命令--skills)
7. [MCP 服务器](#mcp-服务器)
8. [无头模式](#无头模式)
9. [ACP 协议](#acp-协议)

---

## 安装

### 一键安装

```sh
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/taxueseek/kimix/main/install.sh | bash
```

```powershell
# Windows PowerShell
irm https://raw.githubusercontent.com/taxueseek/kimix/main/install.ps1 | iex
```

安装到 `~/.kimix/bin/kimix`，安装器会自动添加 PATH。

### 从源码构建

```sh
rustup toolchain install 1.97.0
cargo build --profile release-dist -p kimix-bin
```

### 更新

```sh
kimix update    # 内置自更新
```

---

## 登录与认证

### Kimi Code 订阅

```sh
kimix login
```

浏览器会自动打开 Kimi Code 登录页面，完成后 token 保存在系统密钥环（服务名 `kimix`）。

### Moonshot API Key

```sh
export KIMIX_MOONSHOT_API_KEY=sk-...
```

或写入 `~/.kimix/config.toml`：

```toml
[platforms.moonshot-cn]
api_key = "sk-..."
```

---

## 基本使用

启动 TUI：

```sh
kimix
```

常用快捷键：

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+Q` | 退出 |
| `Ctrl+M` | 切换模型 |
| `Ctrl+L` | 清屏 |
| `Esc+Esc` | 撤销 / 打开回退面板 |
| `/` | 输入斜杠命令 |

### 斜杠命令

在输入框中输入 `/` 触发命令面板：

| 命令 | 功能 |
|------|------|
| `/model` | 切换模型 |
| `/settings` | 打开设置 |
| `/login` | 登录 |
| `/sessions` | 会话列表 |
| `/help` | 帮助 |
| `/context` | 查看上下文用量 |

---

## 模型切换

登录后，Kimix 自动从各平台同步可用模型列表。

模型选择器（`Ctrl+M` 或 `/model`）中按 `{平台}/{模型}` 格式显示，例如：
- `kimi-code/k3`
- `moonshot-cn/moonshot-v1-8k`

也可以通过 CLI 指定：

```sh
kimix --model kimi-code/k3
```

---

## 供应商配置

当前支持三平台：

| 平台 | 认证 | 说明 |
|------|------|------|
| Kimi Code | OAuth 设备码 | 订阅用户，支持搜索 |
| Moonshot CN | API Key | 国内开放平台 |
| Moonshot AI | API Key | 海外开放平台 |

---

## 自定义命令 / Skills

创建 `~/.kimix/commands/` 目录，放入 `.md` 文件：

```markdown
---
name: 代码审查
description: 审查当前改动的代码
trigger: review
---

请审查以下代码改动，重点关注安全和性能。
```

保存后在输入框输入 `review` 即可触发。

---

## MCP 服务器

添加 MCP 服务器：

```sh
kimix mcp add my-server -- npx -y @modelcontextprotocol/server-filesystem /path/to/dir
```

查看状态：

```sh
kimix mcp doctor my-server
```

---

## 无头模式

在脚本中使用：

```sh
kimix -p "解释这段代码的作用" < main.rs
```

或管道模式：

```sh
echo "列出所有 TODO 注释" | kimix -p
```

---

## ACP 协议

通过 ACP 协议嵌入编辑器：

```sh
kimix acp
```

支持的编辑器通过 ACP 插件连接后，Kimix 作为后台代理运行。
