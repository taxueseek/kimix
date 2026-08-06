# OSS / 自定义模型接入（0.1.19+）

## 目标

在不 fork 全量 Codex、不复制插件市场的前提下，让 OpenAI 兼容的 OSS 端点用起来像「原生」：

| 能力 | 模块 | 说明 |
|------|------|------|
| 请求方言 | `kimix-sampler::dialect` | 非 Kimi host → `OpenAiCompat`，不写 thinking/schema 重写 |
| 工具参数修复 | `input_repair` | 反序列化前修字符串化 JSON、别名、标量类型 |
| 沙箱策略 | `kimix-sandbox::exec_transform` | 审批轴与 profile 轴分离；workspace-write 保护 `.git` |
| 能力图 | `kimix-models::feature_map` | tools / parallel / thinking 轻量启发 |
| 会话治愈 | `heal_conversation_pairs` | 加载 + `BuildConversationRequest` 边界；dangling + dedup + orphan + 遥测 |
| 流错误三分 | `triage_error_facts` / `triage_sampling_error` | 会话环路：Repair→heal 一次后 resubmit；Retry 留给传输层 |

## 最小配置

```toml
# ~/.kimix/config.toml 或项目配置（字段名以当前 schema 为准）
[model.my-oss]
base_url = "https://api.example.com/v1"
api_key_env = "MY_OSS_KEY"
model = "deepseek-v4-flash"
```

或 CLI / 环境变量覆盖 `base_url`（非 `*.kimi.com` / `*.moonshot.*` 即走 OpenAiCompat）。

## 沙箱（PolicySandbox）

| Profile | 写范围 | 子进程网络 |
|---------|--------|------------|
| `workspace` / workspace-write | 工作区内，**禁止** `.git` 与 `.kimix/sandbox.toml` | 默认开 |
| `read-only` | 仅 handler 拒绝写 | 限制 |
| `off` / danger-full-access | 无 handler 边界 | 不限制 |
| `devbox` | 几乎全盘可写，`/data` 写拒绝 | 默认开 |

审批（`ask` / `on-failure` / `never`）与 profile **独立**。历史 `auto_allow_bash` ≈ 沙箱激活时的 `on-failure`。

## 验收清单（OSS-native）

1. 脏工具参数（`{"path":"./a"}` 包在字符串里）→ repair 后工具成功
2. 自定义 base_url → 请求体无 Kimi-only `thinking` 重写字段泄漏
3. workspace-write → 写 `.git/config` 被拒；写普通源文件成功
4. dangling tool_call 会话加载 → heal 插入合成 result，telemetry runs ≥ 1
5. API 返回 `No tool output found for function call` → triage = `Repair`

## 明确不做

- 不用 JS shell-wrap 伪沙箱
- 不 fork 完整 Codex
- 不造 meta-orchestrator / 插件市场
