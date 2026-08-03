#!/usr/bin/env python3
"""kimix 行为回归评估运行器（eval harness）。

目的：改 prompt / 工具定义 / 截断策略后，用固定用例集量化行为变化，
替代「凭感觉」。指标：通过率 + 编辑失败率（search_replace 失败痕迹）。

验证器回流（最简验证器插件化）：断言失败时把失败原因拼进下一轮 prompt，
让模型修正后再评断言（case 可用 "retry_rounds" 覆盖，默认 1 次修正），
模拟「用户把错误喂回给 agent」的迭代闭环，且不引入独立 Verifier trait。

用例为 JSON（stdlib 即可解析），结构见 cases/ 目录与 README。
运行：python3 evals/runner.py [--cases evals/cases] [--bin ~/.local/bin/kimix]
      [--filter NAME] [--report out.json] [--model M] [--repo PATH]
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# 每个用例的默认上限（可用例覆盖）
DEFAULT_TIMEOUT_SEC = 240
DEFAULT_MAX_TURNS = 8


def setup_case_dir(case: dict, root: Path) -> None:
    """把 fixture 文件写入临时目录。"""
    for rel, content in case.get("files", {}).items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")


def snapshot(root: Path) -> dict:
    """目录内容快照，用于 file_unchanged 断言。"""
    snap = {}
    for p in sorted(root.rglob("*")):
        if p.is_file():
            snap[str(p.relative_to(root))] = p.read_bytes()
    return snap


def run_assertion(a: dict, root: Path) -> tuple[bool, str]:
    """单条断言，返回 (是否通过, 失败原因)。"""
    t = a["type"]
    path = root / a.get("path", "")
    if t == "file_contains":
        if not path.is_file():
            return False, f"{a['path']} 不存在"
        text = path.read_text(encoding="utf-8", errors="replace")
        if re.search(a["pattern"], text):
            return True, ""
        return False, f"{a['path']} 未命中 /{a['pattern']}/"
    if t == "file_not_contains":
        if not path.is_file():
            return True, ""
        text = path.read_text(encoding="utf-8", errors="replace")
        if re.search(a["pattern"], text):
            return False, f"{a['path']} 不应命中 /{a['pattern']}/"
        return True, ""
    if t == "file_exists":
        return (path.is_file(), "" if path.is_file() else f"{a['path']} 不存在")
    if t == "file_unchanged":
        # 只对比运行前已存在的文件，允许运行中新增文件（如 answer.txt）。
        # 任何已有文件被改动（内容变化或删除）即失败。
        before = a["_before"]
        after = snapshot(root)
        changed = [k for k in before
                   if k not in after or before[k] != after[k]]
        if not changed:
            return True, ""
        return False, f"已有文件被改动/删除: {changed}"
    if t == "command_succeeds":
        r = subprocess.run(a["cmd"], shell=True, cwd=root,
                           capture_output=True, timeout=60)
        if r.returncode == 0:
            return True, ""
        return False, f"命令失败({r.returncode}): {r.stderr.decode()[:200]}"
    return False, f"未知断言类型 {t}"


def retry_suffix(failures: list[str], round_no: int) -> str:
    """修正回流 prompt 后缀：把上一轮失败原因反馈给模型。

    验证器插件化的最简形态：不引入独立 Verifier trait，而是把断言失败
    信息直接拼进下一轮 prompt，模拟「用户把错误喂回给 agent」的迭代闭环。
    """
    items = "\n".join(f"- {f}" for f in failures[:5])
    return (
        f"\n\n## 验证反馈（第 {round_no} 次修正）\n"
        f"你上一轮的结果未通过验证，失败原因如下：\n{items}\n"
        "请只针对上述失败原因修正，不要做无关改动；修正完成后自行复查。\n"
    )


def run_case(case: dict, kimix: str, work_root: Path,
             model: str | None = None, repo: Path | None = None) -> dict:
    name = case["name"]
    is_repo = bool(case.get("repo", False))
    if is_repo and repo is None:
        return {
            "name": name,
            "passed": False,
            "failures": ["repo 用例需要 --repo 参数（真实仓库模式）"],
            "edit_failures": 0,
            "elapsed_sec": 0.0,
            "skipped": True,
        }
    if is_repo:
        # 真实仓库场景：使用复制到临时目录的仓库，不写 files fixture。
        case_dir = work_root / "repo"
        case_dir.mkdir(parents=True, exist_ok=True)
        shutil.copytree(repo, case_dir, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns(".git", "target",
                                                      "node_modules", "evals"))
    else:
        case_dir = work_root / name
        case_dir.mkdir(parents=True)
        setup_case_dir(case, case_dir)

    # file_unchanged 断言需要前置快照（所有轮次都对比这个运行前快照）
    for a in case.get("assert", []):
        if a["type"] == "file_unchanged":
            a["_before"] = snapshot(case_dir)

    rounds = int(case.get("retry_rounds", 1)) + 1  # 首轮 + 至多 N 次修正
    all_output = ""
    failures: list[str] = []
    elapsed_total = 0.0
    run_error = None
    for rnd in range(rounds):
        if rnd > 0 and not failures:
            break
        prompt = case["prompt"]
        if rnd > 0:
            prompt += retry_suffix(failures, rnd)
        cmd = [
            kimix, "-p", prompt,
            "--cwd", str(case_dir),
            "--always-approve",
            "--max-turns", str(case.get("max_turns", DEFAULT_MAX_TURNS)),
            "--disable-web-search",
        ]
        if model:
            cmd += ["--model", model]
        t0 = time.time()
        try:
            proc = subprocess.run(
                cmd, capture_output=True, text=True,
                timeout=case.get("timeout_sec", DEFAULT_TIMEOUT_SEC),
            )
            output = proc.stdout + proc.stderr
            run_error = None if proc.returncode == 0 else f"exit={proc.returncode}"
        except subprocess.TimeoutExpired:
            output = ""
            run_error = "timeout"
        elapsed_total += time.time() - t0
        all_output += output

        # 重新评估全部断言（file_unchanged 始终对比运行前快照）
        failures = []
        if run_error:
            failures.append(f"运行失败: {run_error}")
        for a in case.get("assert", []):
            ok, why = run_assertion(a, case_dir)
            if not ok:
                failures.append(why)
        if not failures:
            break

    # 编辑失败率痕迹：search_replace 未命中是 coding agent 最高频失败源
    edit_fail = len(re.findall(r"was not found in the file", all_output))

    return {
        "name": name,
        "passed": not failures,
        "failures": failures,
        "edit_failures": edit_fail,
        "elapsed_sec": round(elapsed_total, 1),
        "rounds": rnd + 1,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cases", default=str(Path(__file__).parent / "cases"))
    ap.add_argument("--bin", default=os.environ.get("KIMIX_BIN", "kimix"))
    ap.add_argument("--filter", default=None, help="只运行名称含该子串的用例")
    ap.add_argument("--model", default=None, help="headless 使用的模型名（透传 --model）")
    ap.add_argument("--repo", default=None, help="真实仓库路径：repo 用例在此仓库副本上运行")
    ap.add_argument("--report", default=None, help="JSON 报告输出路径")
    args = ap.parse_args()

    cases_dir = Path(args.cases)
    case_files = sorted(cases_dir.glob("*.json"))
    if args.filter:
        case_files = [f for f in case_files if args.filter in f.name]
    if not case_files:
        print(f"无用例: {cases_dir}", file=sys.stderr)
        return 2

    repo = Path(args.repo).resolve() if args.repo else None
    if repo is not None and not repo.is_dir():
        print(f"--repo 不是目录: {repo}", file=sys.stderr)
        return 2

    results = []
    with tempfile.TemporaryDirectory(prefix="kimix-eval-") as tmp:
        for f in case_files:
            case = json.loads(f.read_text(encoding="utf-8"))
            r = run_case(case, args.bin, Path(tmp), model=args.model, repo=repo)
            results.append(r)
            if r.get("skipped"):
                print(f"[SKIP] {r['name']} (需要 --repo)")
                continue
            mark = "PASS" if r["passed"] else "FAIL"
            print(f"[{mark}] {r['name']} ({r['elapsed_sec']}s, "
                  f"rounds={r.get('rounds', 1)}, edit_fail={r['edit_failures']})")
            for why in r["failures"]:
                print(f"       - {why}")

    total = len(results)
    passed = sum(1 for r in results if r["passed"])
    skipped = sum(1 for r in results if r.get("skipped"))
    edit_fails = sum(r["edit_failures"] for r in results)
    if total - skipped > 0:
        print(f"\n通过率: {passed}/{total - skipped} 运行（{skipped} 跳过），"
              f"编辑失败痕迹总计: {edit_fails}")
    else:
        print(f"\n无可运行用例（{skipped} 跳过，需要 --repo）")

    if args.report:
        report_path = Path(args.report)
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps({
            "passed": passed, "total": total, "skipped": skipped,
            "pass_rate": passed / (total - skipped) if total > skipped else 0,
            "edit_failures": edit_fails,
            "results": results,
        }, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if passed + skipped == total else 1


if __name__ == "__main__":
    sys.exit(main())
