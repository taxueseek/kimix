# 开模型能力吸纳 — 实施记录与基线方法学（2026-08-07）

## 目标重述（第一性原理）
一个 harness 若假设模型同质,就必然在异质模型上流失能力。Claude/GPT 与
DeepSeek/Kimi/Qwen/本地模型的失败模式不同(工具语法泄漏成纯文本、不主动
CoT、todo 多标并发、工具调用前习惯性冒号)。Muse 实验证明:同模型同任务,
**只改 prompting** 就能 token -2.7x / 耗时 -2x / 成本 -2.4x。本次让 kimix
按模型类别自适应地切换 prompt 策略与请求塑形,把弱模型已知失败模式逐条堵掉,
且对强模型零回归。

## MECE 层级与状态
| 层 | 内容 | 状态 |
|---|---|---|
| 0 门控基础 | `ModelCategory` 检测(Premium/OpenSource)+ env 覆盖 | ✅ |
| A Prompt | 全模型 Muse 加深 + 开模型补偿块(`is_open_model` 门控) | ✅ |
| B 请求塑形 | `tool_choice:"none"`(无工具轮+开模型+openai 兼容)+ 子调用 temp:0 | ✅ |
| C 技能 | per-skill effort 下接 + skill 动态 shell | ⏸ 延后(见下) |
| D Bug | task/mod.rs 缩进;余为已文档化设计债 | ✅ |
| E 性能 | 评估:prompt 渲染已缓存,真瓶颈已处理 | ✅(无改动) |
| F 测试 | 2641 lib 测试绿 + 12 新测试 | ✅ |

## 已落地改动(按文件)

### A — Prompt(`crates/codegen/kimix-agent/templates/prompt.md`)
- 新增 `<tool_batching>`(全模型):独立工具调用并行批处理。
- 新增 `<evidence_grounding>`(全模型,Muse 加深):ground claims、evidence
  before synthesis、independent oracle、test discipline(绝不缩窄失败跑)、
  verification gate discovery(读 Makefile/CI/包元数据再跑 gate)。
- `<task_planning>` 追加顺序 todo 纪律(一次一个 in_progress,做完即下一个)。
- 新增 `<open_model_discipline>`(`${%- if is_open_model %}` 门控):CoT 前言、
  工具调用前禁冒号、第一人称探究——仅开模型渲染,Premium 零回归。
- `template.rs` render-assertion 扩 4 测试(tool_batching / evidence_grounding
  / sequential_todo / open_model 门控);XOR 载荷经 `scripts/encrypt_templates.py`
  重生成,`test_encrypted_templates_not_stale` 通过。
- `PromptContext` 加 `is_open_model: bool`(serde default,后向兼容)+ 进
  `placeholders()`;`AgentBuilder::with_is_open_model`;`Agent::render_prompt_for_open_model`
  /`set_open_model`(分离 async 渲染与同步缓存,RefMut 不跨 await)。

### 0 — 门控(`crates/codegen/kimix-sampler/src/model_category.rs`)
- `enum ModelCategory { Premium, OpenSource }` + `classify(model, base_url)`:
  env `KIMIX_MODEL_CATEGORY` 覆盖 → 显式 premium 签名 → 显式 open 签名 → 默认
  Premium(未知不改 premium 自然风格)。纯逻辑 `classify_inner` 可测,无需
  改真实 env(Rust 1.97 `set_var` unsafe,规避并行测试竞态)。8 测试。
- 接线:`handle_set_session_model`(model_switch.rs)切换时分类→重渲→
  `set_open_model` 缓存→重写对话 system message;`AgentRebuildSpec.is_open_model`
  字段(构造处 spawn 从 `sampling_config` 算),`build_agent_inner` 经
  `.with_is_open_model()` 透传——覆盖 initial build + 模型切换 rebuild 两条路径。

### B — 请求塑形
- **B1 tool_choice:"none"**(`crates/codegen/kimix-sampler/src/client.rs`
  `apply_defaults`):无工具轮 + `OpenAiCompat` 方言 + 开模型 → 显式
  `tool_choice="none"`,堵 DeepSeek/Qwen/GLM 类把原生工具调用信封当普通文本
  回吐(吸纳 maka-agent `toolChoice:'none'` 修复)。hosted_tools 一并判定
  (stream 路径在 apply_defaults 之后才注入 hosted,避免误抑制 web_search)。
  Premium 与 Kimi 方言不动(避 OpenAI 无工具拒 tool_choice / Kimi 自有处理)。
  4 测试。
- **B2 子调用 temperature:0**(`crates/codegen/kimix-shell/src/session/image_describe.rs`):
  图像描述子调用 temp 0.2→0.0(确定性提取,吸纳 command-code 分层采样)。
  28 测试无回退。

### D — Bug
- `crates/codegen/kimix-tools/src/implementations/kimix/task/mod.rs`:用户 0.1.20
  改动引入的 7 处 `task_id` 缩进错配(12/16 空格混用),rustfmt 修齐,该文件
  fmt-clean。
- `session_lifecycle.rs` TODO(PR-4)、`drain_old_session_thread`、stdio bridge
  静默丢消息、4 处 stale TODO:均为**已文档化设计债/边缘场景**(有 warn!/error!
  操作员信号或保守构造),非被忽略 bug,强改越界且高风险,留原状。

## 量化基线方法学(待真实任务建档复测)
受限于本环境无模型 API key,无法实跑;对照 Muse 实验方法论,复测应:
1. 选同一开模型(如 DeepSeek-V3 / Kimi-K2)与同一组冒烟任务:一个 bug-repro、
   一个多文件编辑、一个纯问答。
2. 在 `evals/` 下分两配置跑:吸纳前(回退 `is_open_model=false` + 移除
   `<open_model_discipline>`/`<evidence_grounding>`)vs 吸纳后(默认)。
3. 记录每任务:**总 token、轮次、墙钟、成本**;对标 Muse 的 -2.7x/-2x/-2.4x。
4. 重点观测开模型是否仍泄漏工具调用语法成纯文本(B1 的直接收益)。
5. 结果回填本文件「实测」一节。

## 延后项(精确 hook)
### C1 — per-skill `effort` 下接到 ReasoningEffort
`discovery.rs:558` **已解析** `effort`,存于 `SkillInfo`,但下游无消费(仅测试
断言)。完整下接需跨 crate:skill 工具执行(kimix-tools)→ 通知 session
(kimix-shell)对该轮设 `models.rs:522 set_current_reasoning_effort`。属会话级
跨切面改动,本次不碰;hook 已定位。`effort` 取值映射 `low/medium/high/xhigh/max
→ ReasoningEffort`(`inherit`=不改),并暴露 `${KIMIX_EFFORT}` 到 skill body。

### C2 — skill 动态 shell 注入(`!`cmd`` / ```! 块)
command-code 在 skill body 渲染前跑 shell 内联回填。对 kimix 这会在 prompt 构造期
执行 shell,**绕过 bash 工具的 permission_mode/sandbox** —— 对 kimix 较严权限模型
是安全回归。需先设计「经 bash 工具权限闸」的变体再吸纳。注入点
`crates/codegen/kimix-tools/src/implementations/skills/skill.rs:494
load_skill_with_body`。

### P2(整体延后,中高风险)
每供应商 reasoning-replay 传输 shim、synthesis cache、context-budget 旋钮族、
economy vs heavy 双策略 + self-check sandbox 门、Taste 连续偏好学习。

## 测试与质量状态
- `cargo test -p kimix-sampler -p kimix-agent -p kimix-tools -p kimix-sampling-types --lib`:
  **2641 passed; 0 failed**。
- 新增 12 测试:prompt 4(template render-assertion)+ model_category 8 +
  apply_defaults 4 = 16(其中 prompt/model_category 独立计数)。
- `cargo clippy -p kimix-sampler --lib`:我新代码无 lint(`map_or`→`is_none_or`
  已修)。其余 clippy 警告(kimix-tools 的 collapsed-if 等)为预存,非本次改动。
- `cargo fmt --check -p kimix-sampler -p kimix-agent -p kimix-shell`:我新代码
  fmt-clean;`template.rs:249` 与 `client.rs:208/215` 为**预存已提交** fmt
  违规(非本次、非用户 0.1.20 改动),未越界清理。

## config.toml 覆盖(原计划延后)
原计划 `[model.*] category="opensource"` 覆盖未做(需 SamplerConfig 全量字面
改动,管线较重)。首版用 env `KIMIX_MODEL_CATEGORY` 作手动逃生阀,检测规则
表覆盖常见开模型。config.toml 覆盖留作后续。
