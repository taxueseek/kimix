# kimix 行为回归评估（evals）

改 prompt、工具定义、截断/压缩策略后的量化回归手段，替代「凭感觉」。

## 运行

```bash
# 全量
python3 evals/runner.py

# 指定二进制 / 过滤 / 输出 JSON 报告
python3 evals/runner.py --bin target/release/kimix --filter typo --report evals/last.json

# 指定模型（透传 headless --model）与 repo 用例的真实仓库
python3 evals/runner.py --model kimi-k2.5 --repo evals/fixtures/repo-version-bump
```

退出码：全部通过为 0，任一失败为 1（可直接进 CI）。

## 指标

- **通过率**：所有断言通过的用例占比
- **编辑失败痕迹**：输出中 `was not found in the file`（search_replace 未命中）
  出现次数——coding agent 最高频失败源，趋势比单次值重要

## 用例格式（`cases/*.json`，stdlib 可解析）

```json
{
  "name": "fix-typo",
  "files": { "hello.py": "def greet():\n    return 'helo wrold'\n" },
  "prompt": "Fix the two typos ...",
  "assert": [
    { "type": "file_contains", "path": "hello.py", "pattern": "hello world" },
    { "type": "file_not_contains", "path": "hello.py", "pattern": "helo" },
    { "type": "file_exists", "path": "out.txt" },
    { "type": "file_unchanged" },
    { "type": "command_succeeds", "cmd": "python3 main.py" }
  ],
  "max_turns": 6,
  "timeout_sec": 180,
  "retry_rounds": 1
}
```

- `files`：写入临时目录的 fixture
- `file_unchanged`：阴性对照（问答案型用例，断言任何文件都未被改动）
- `command_succeeds`：在用例目录内执行，60s 超时
- `retry_rounds`：首轮断言失败后，把失败原因拼进新 prompt 再让模型修正的
  轮数（默认 1，即最多跑两轮）。这是验证器插件化的最简形态
- `"repo": true`：真实仓库模式，不写 `files`，runner 把 `--repo` 指定的
  仓库复制到临时目录再执行（忽略 .git/target/node_modules/evals）

## 扩库方向

参照 aider polyglot 的方法论：真实仓库 diff 场景、首轮解答 + 按测试报错
修正两轮。新增用例优先覆盖：大文件局部编辑、跨文件引用修改、
长输出命令后的判断（截断路径）、计划类多步任务。目标规模 20~50 题，
每次 prompt/工具变更跑一遍并记录两个指标。

## 验证器回流

`retry_rounds` 把断言失败反馈喂回模型（拼接失败原因 + 「只针对失败修正」），
模拟用户把错误丢给 agent 后让它自己修完再验。当前是 runner 侧的最简实现，
不引入独立 Verifier trait；多轮能力已用假 binary 做过正/负路径验证
（首轮失败二轮修正 → PASS；两轮都失败 → FAIL 保留首轮结果）。

## 注意

- 用例会真实调用模型 API，逐用例串行执行，注意速率与成本
- runner 强制 `--always-approve` 与 `--disable-web-search`，与网络隔离
- 临时目录随运行结束销毁；调试时用 `--report` 保存结果
