# Kimix 评估基准 v1（Evaluation Baseline）

> 版本基线：v0.1.16（2026-07-31 发布）
> 文档日期：2026-08-01
> 数据来源：`evals/*.json`、`~/.kimix/metrics/cache_hit-*.jsonl`、`CHANGELOG.md`、
> `docs/handoff-kimix-v0.1.15.md`、`审计报告_运行时瓶颈_v0.1.15.md`

---

## 一、为什么需要这个基准

kimix 从 v0.1.7（内部）迭代到 v0.1.16，共 10 个版本，中间积累了零散的量化数据
（两次 evals 基线、缓存命中率落盘、多份审计报告），但没有一份统一文档回答：

- 这一版到底比上一版好还是差？
- 「好/差」用什么指标衡量、阈值是多少？
- 回归（prompt / 工具 / 截断策略改动）后凭什么说没改坏东西？

本文件把历史数据归档为 **v1 基线**，并定义可持续追踪的 KPI 体系。
后续每次迭代只需重跑测量、更新对应表格。

---

## 二、历史数据归档（v0.1.13 → v0.1.16）

### 2.1 行为质量（evals 回归）

| 基线 | 用例数 | 通过率 | edit_failures | 平均耗时/题 | 备注 |
|------|:------:|:------:|:-------------:|:-----------:|------|
| 2026-07-26（v0.1.13 前后） | 5 | 100% | 0 | 5.5s | 首条基线 |
| v0.1.13 实测 | 5 | 100% | 0 | 5.5s | 同一 5 题 |

**缺口**：用例仅 5 题，覆盖面不足（README 自述目标 20–50 题）。
单次跑通 5 题不能支撑回归判断，趋势比单次值重要。

### 2.2 成本效率（缓存命中率实测）

来源：`~/.kimix/metrics/cache_hit-<日期>.jsonl`（v0.1.16 落盘功能）。

| 日期 | 请求数 | 均值 | 中位数 | 90%+ 占比 | 最低值 |
|------|:------:|:----:|:------:|:---------:|:------:|
| 2026-07-31 | 91 | 92.3% | 99.0% | 84/91 (92%) | 0%（冷启动） |
| 2026-08-01 | 415 | 97.1% | 99.5% | 396/415 (95%) | 1.6% |

**解读**：
- README 宣称「日常保持 95%+」，8-01 实测均值 97.1%，达标。
- 中位数远高于均值：命中率集中在 99% 附近，低值全部来自会话首请求（冷启动）。
  因此 **中位数比均值更适合做成本 KPI**，均值会被冷启动拉低。
- 聚合口径：均值按请求加权（token 越多的请求权重应越高）更贴近真实成本，
  v1 先按请求简单平均，v2 可升级为 token 加权。

### 2.3 性能（文档记录）

| 指标 | 旧值 | 新值 | 版本 | 来源 |
|------|------|------|:----:|------|
| headless 编译 | ~58s | ~35s | 0.1.15 | handoff |
| 大会话 resize | ~5ms | ~1ms | 0.1.15 | handoff |
| JSONL append syscall/条 | 6 | 3 | 0.1.14 | CHANGELOG |
| TUI 轮询间隔 | 100ms | 16ms（60fps） | 0.1.13 | CHANGELOG |
| release 构建 | — | thin LTO +10~20% 提速 | 0.1.13 | CHANGELOG |
| 有效上下文硬顶 | 无 | 200K（可配） | 0.1.15 | CHANGELOG |
| 流式重试上限 | 15 | 3 | 0.1.15 | CHANGELOG |
| 视频帧上限 | 无界 | ~120 帧 / 12s | 0.1.15 | CHANGELOG |

### 2.4 稳定性与正确性（文档记录）

| 类别 | 内容 | 版本 |
|------|------|:----:|
| panic 修复 | 9 处 `unwrap_or_default()` + 除零 + 非 UTF-8 文件名 | 0.1.13 |
| 内存泄漏 | 6 项（prompt_texts / updates.jsonl / 粘贴图片等） | 0.1.15 |
| 错误安全 | textwrap panic、ClassifierError 结构化、删除 2 个死 crate | 0.1.15 |
| clippy | 零警告（累计清理 65+ 处） | 0.1.13–0.1.15 |
| 敏感路径沙箱 | 默认拦截 `~/.ssh`、`~/.aws` | 0.1.14 |
| 运行时热点审计 | P0×4、P1×3、P2×2（v0.1.15 时点） | 0.1.15 |
| 会话膨胀 | 中间帧只存头尾（实测某会话 75MB 中 ~99% 为中间帧） | 0.1.16 |

### 2.5 规模基线

| 指标 | 值 | 来源 |
|------|-----|------|
| 代码量 | ~117 万行 Rust | handoff |
| crate 数 | 66（0.1.15）→ 76（分析报告口径） | handoff / output |
| 版本发布节奏 | 0.1.7 → 0.1.16 约 10 个版本 | CHANGELOG |

---

## 三、KPI 体系（v1，五个维度）

### 3.1 行为质量（最重要，一票否决）

| 指标 | 当前基线 | 目标 | 测量方式 |
|------|:--------:|:----:|----------|
| evals 通过率 | 100%（5/5） | ≥ 90% | `python3 evals/runner.py --report evals/last.json` |
| edit_failures 次数 | 0 | ≤ 2 | runner 自动统计 |
| 用例库规模 | 5 | 20–50 | 持续扩充 cases/ |

> 任何 prompt / 工具定义 / 截断策略改动后必须跑 evals；通过率跌破 90%
> 或 edit_failures 出现趋势性上升 → 判定回归，禁止发布。

### 3.2 成本效率

| 指标 | 当前基线 | 目标 | 测量方式 |
|------|:--------:|:----:|----------|
| 缓存命中率（中位数） | 99.0–99.5% | ≥ 95% | metrics/cache_hit-*.jsonl 聚合 |
| 缓存命中率（均值） | 92–97% | ≥ 90% | 同上 |
| 冷启动请求占比 | ~1%（6/506） | ≤ 5% | 同上（<10% 命中请求占比） |

### 3.3 性能

| 指标 | 当前基线 | 目标 | 测量方式 |
|------|:--------:|:----:|----------|
| 首 token 延迟 | 未归档（待测） | 待定 | 实测记录 |
| TUI 轮询 / 渲染 | 16ms / resize ~1ms | 不回退 | 实测 |
| headless 编译 | ~35s | ≤ 60s | `cargo build -p kimix-headless` 计时 |
| 大会话内存占用 | 75MB 会话不再膨胀 | 峰值可控 | 实测 |

### 3.4 稳定性

| 指标 | 当前基线 | 目标 | 测量方式 |
|------|:--------:|:----:|----------|
| 用户可触达 panic | 0（当前） | 0 | crash-handler 日志 |
| 流式重试峰值 attempt | ≤ 3 | ≤ 3 | `scripts/analyze-retry-metrics.py` |
| 内存泄漏（prompt_texts 等） | 已封顶 | 无新增 | 代码审计 |

### 3.5 代码质量

| 指标 | 当前基线 | 目标 | 测量方式 |
|------|:--------:|:----:|----------|
| clippy 警告 | 0 | 0 | `just gate`（CI） |
| 测试覆盖 | 23 个 tests/ 目录 | 不回退 | `cargo test --all-targets` |
| deny 门禁 | advisories+bans+sources+licenses | 全绿 | `just gate` |

---

## 四、测量方法与工具链

| 工具 | 位置 | 用途 |
|------|------|------|
| evals runner | `evals/runner.py` | 行为回归（退出码可进 CI） |
| 缓存 metrics | `~/.kimix/metrics/cache_hit-*.jsonl` | 成本 KPI（v0.1.16 起自动落盘） |
| 重试分析 | `scripts/analyze-retry-metrics.py` | 稳定性 KPI |
| 质量门禁 | `justfile` 的 `gate` | clippy + deny + test |

**建议的缓存指标聚合命令**（可直接复用）：

```sh
python3 - <<'EOF'
import json, glob, os, statistics
pcts = [r['cache_hit_percent']
        for f in glob.glob(os.path.expanduser('~/.kimix/metrics/cache_hit-*.jsonl'))
        for l in open(f) if l.strip() and (r := json.loads(l))['type'] == 'cache_hit']
print(f"requests={len(pcts)} median={statistics.median(pcts):.1f}% mean={statistics.mean(pcts):.1f}%")
EOF
```

---

## 五、迭代纪律（每次发版前）

1. **跑 evals**：`python3 evals/runner.py --report evals/<版本>.json`，结果并入
   `evals/`，通过率与 edit_failures 两栏必须填写。
2. **聚合并记录缓存命中率**：当日 `cache_hit-<日期>.jsonl` 的中位数与均值。
3. **更新本文件**：新基线填进第二节，KPI 表「当前基线」列改为新值。
4. **跑门禁**：`just gate` 全绿。
5. 性能/内存类改动，补一条可复现的测量记录（旧值 → 新值），
   参照 2.3 节表格格式。

---

## 六、已知缺口（后续迭代补）

- evals 用例只有 5 题，需按 README「扩库方向」扩充到 20–50 题
  （大文件局部编辑、跨文件引用、长输出后判断、计划类多步任务）。
- 缓存聚合仍是请求数简单平均，v2 建议升级为 token 加权。
- 首 token 延迟、启动时间尚无归档基线，建议在 v0.1.17 前补测。
- 多模型矩阵（Grok 4.5 / Kimi K3 / GLM 5.2 / DeepSeek / MiMo / LongCat）
  在同一 evals 集上的横向对比未做，可作为后续独立专题。
