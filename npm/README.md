# kimix — npm 分发包

让 kimix 可以通过 npx / npm 安装：`npx kimix` 即装即用，无需 curl 管道。

## 安装

```sh
# 临时使用（不落盘）
npx kimix --version

# 全局安装
npm install -g kimix
kimix login
kimix
```

安装时 `postinstall` 会从 GitHub Releases 下载当前平台的二进制
（与 `install.sh` 同一套资产、同一份 `SHA256SUMS` 校验），因此包本身只有几个
KB，真正的二进制约几十 MB。

## 支持平台

| 平台 | triple | 说明 |
|------|--------|------|
| macOS arm64 / x64 | `aarch64-apple-darwin` / `x86_64-apple-darwin` | ✓ |
| Linux arm64 / x64 | `aarch64-unknown-linux-gnu` / `x86_64-unknown-linux-gnu` | ✓ |
| Windows x64 | `x86_64-pc-windows-msvc` | ✓（需 Windows 10+ 自带 tar） |

## 版本同步

npm 包版本必须与 GitHub release tag 对齐（`v0.1.16` ↔ `0.1.16`）。

发布流程：

```sh
# 1. 确认 GitHub 上已发布 v<version> 的 release（含对应平台资产 + SHA256SUMS）
# 2. 把 package.json 的 version 改为同一版本
cd npm
npm publish

# 发布前会自动运行 prepublishOnly 门禁（scripts/check-version.js）：
# 版本格式必须是 X.Y.Z，且 GitHub release 必须已存在。
```

## 环境变量（调试用）

- `KIMIX_NPM_VERSION`：覆盖下载版本（如 `KIMIX_NPM_VERSION=0.1.15 npm install`）
- `KIMIX_DOWNLOAD_BASE`：覆盖下载基址
- `KIMIX_NPM_SKIP=1`：跳过二进制下载

## 已知限制

- 若平台暂未发布对应资产，postinstall 会打印回退指引（官方 install.sh / install.ps1），
  不会破坏 npm 安装本身。
- 二进制下载失败不会回滚 npm 包，`kimix` 命令会提示改用官方脚本。
