# 已知问题 / Known Issues

## Linux 二进制体积较大

Linux 平台的二进制（尤其是 aarch64-unknown-linux-gnu）体积较大（~210 MB），
原因是 jemalloc 静态链接且当前 release-dist 未启用 LTO。

**解决方案**：在 `Cargo.toml` 的 `[profile.release-dist]` 中添加 `lto = "thin"`
可显著减小二进制体积（预计降至 ~80 MB），但会增加编译时间。

## 多供应商与自定义端点（已支持，文档曾滞后）

内置平台仍为 **Kimi Code + Moonshot（cn/ai）** 三家；自 0.1.19 起：

- 配置 `[model.*]` / 自定义 `base_url` 可接 OpenAI 兼容的 OSS 网关（DeepSeek、Qwen、vLLM 等）
- Chat Completions **方言**按 base_url 推断：`Kimi` 与 `OpenAiCompat` 分流，避免向 OSS 端点泄漏 Kimi 专用字段
- 工具参数在反序列化前走 **input repair**（字符串化 JSON、别名等），降低 OSS 工具调用失败率
- 会话 **pair heal**（dangling / orphan / dedup）与 mid-stream **错误三分**（Repair / Retry / Surface）
- 沙箱 **PolicySandbox**：审批轴与 profile 轴分离；workspace-write 保护 `.git`

接入清单与验收项见 [`docs/oss-models.md`](./oss-models.md)。

离线 fallback 目录仍以 Kimi 家族为主；完整第三方 catalog 随 live `/models` 同步。
旧文「仅支持 Kimi/Moonshot」已不成立。

## 全量测试未全部通过

`cargo test --workspace --all-targets` 包含大量 E2E 测试和 PTY 相关测试，
部分测试对环境敏感（locale、终端类型等），在 CI 中目前只运行 `--lib` 级别的测试。

## 上游同步

本项目是 grok-build 的 hard fork，已对 crate 名称、二进制名、配置路径做了全量重命名。
上游 grok-build 发布新版本时，需要手动逐 commit 评估并 cherry-pick，同步成本较高。

---

## Linux binaries are large

Linux platform binaries (especially aarch64-unknown-linux-gnu) are large (~210 MB)
due to statically linked jemalloc and no LTO in the current release-dist profile.

**Workaround**: Add `lto = "thin"` to `[profile.release-dist]` in `Cargo.toml`
to significantly reduce binary size (~80 MB), at the cost of longer compile times.

## Multi-provider and custom endpoints (supported; docs lagged)

Built-in platforms remain **Kimi Code + Moonshot (cn/ai)**. Since 0.1.19:

- `[model.*]` / custom `base_url` works with OpenAI-compatible OSS gateways
- Chat Completions **dialect** is inferred from base_url (`Kimi` vs `OpenAiCompat`)
- Tool args run **input repair** before deserialize for OSS shape quirks
- Conversation **pair heal** + mid-stream error triage (Repair / Retry / Surface)
- Sandbox **PolicySandbox**: approval × profile axes; workspace-write protects `.git`

See [`docs/oss-models.md`](./oss-models.md) for the OSS-native checklist.

The offline fallback catalog is still Kimi-centric; live `/models` drives the rest.
The older “Kimi/Moonshot only” claim is obsolete.

## Not all tests pass

`cargo test --workspace --all-targets` includes many E2E and PTY-dependent tests.
Some are environment-sensitive (locale, terminal type). CI currently only runs
`--lib` level tests.

## Upstream sync cost

This is a hard fork of grok-build with full crate renaming, binary renaming, and
independent config paths. Upstream grok-build releases require manual per-commit
evaluation and cherry-picking, which carries higher sync costs.
