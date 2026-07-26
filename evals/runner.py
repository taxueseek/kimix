#!/usr/bin/env python3
"""kimix 行为回归评估运行器（eval harness）。

目的：改 prompt / 工具定义 / 截断策略后，用固定用例集量化行为变化，
替代「凭感觉」。指标：通过率 + 编辑失败率（search_replace 失败痕迹）。

用例为 JSON（stdlib 即可解析），结构见 cases/ 目录与 README。
运行：python3 evals/runner.py [--cases evals/cases] [--bin ~/.local/bin/kimix]
      [--filter NAME] [--report out.json]
"""
import argparse
import json
import os
import re
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
        before = a["_before"]
        after = snapshot(root)
        if before == after:
            return True, ""
        changed = [k for k in set(before) | set(after)
                   if before.get(k) != after.get(k)]
        return False, f"文件被改动: {changed}"
    if t == "command_succeeds":
        r = subprocess.run(a["cmd"], shell=True, cwd=root,
                           capture_output=True, timeout=60)
        if r.returncode == 0:
            return True, ""
        return False, f"命令失败({r.returncode}): {r.stderr.decode()[:200]}"
    return False, f"未知断言类型 {t}"


def run_case(case: dict, kimix: str, work_root: Path) -> dict:
    name = case["name"]
    case_dir = work_root / name
    case_dir.mkdir(parents=True)
    setup_case_dir(case, case_dir)

    # file_unchanged 断言需要前置快照
    for a in case.get("assert", []):
        if a["type"] == "file_unchanged":
            a["_before"] = snapshot(case_dir)

    cmd = [
        kimix, "-p", case["prompt"],
        "--cwd", str(case_dir),
        "--always-approve",
        "--max-turns", str(case.get("max_turns", DEFAULT_MAX_TURNS)),
        "--disable-web-search",
    ]
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
    elapsed = time.time() - t0

    failures = []
    if run_error:
        failures.append(f"运行失败: {run_error}")
    for a in case.get("assert", []):
        ok, why = run_assertion(a, case_dir)
        if not ok:
            failures.append(why)

    # 编辑失败率痕迹：search_replace 未命中是 coding agent 最高频失败源
    edit_fail = len(re.findall(r"was not found in the file", output))

    return {
        "name": name,
        "passed": not failures,
        "failures": failures,
        "edit_failures": edit_fail,
        "elapsed_sec": round(elapsed, 1),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cases", default=str(Path(__file__).parent / "cases"))
    ap.add_argument("--bin", default=os.environ.get("KIMIX_BIN", "kimix"))
    ap.add_argument("--filter", default=None, help="只运行名称含该子串的用例")
    ap.add_argument("--report", default=None, help="JSON 报告输出路径")
    args = ap.parse_args()

    cases_dir = Path(args.cases)
    case_files = sorted(cases_dir.glob("*.json"))
    if args.filter:
        case_files = [f for f in case_files if args.filter in f.name]
    if not case_files:
        print(f"无用例: {cases_dir}", file=sys.stderr)
        return 2

    results = []
    with tempfile.TemporaryDirectory(prefix="kimix-eval-") as tmp:
        for f in case_files:
            case = json.loads(f.read_text(encoding="utf-8"))
            r = run_case(case, args.bin, Path(tmp))
            results.append(r)
            mark = "PASS" if r["passed"] else "FAIL"
            print(f"[{mark}] {r['name']} ({r['elapsed_sec']}s, "
                  f"edit_fail={r['edit_failures']})")
            for why in r["failures"]:
                print(f"       - {why}")

    total = len(results)
    passed = sum(1 for r in results if r["passed"])
    edit_fails = sum(r["edit_failures"] for r in results)
    print(f"\n通过率: {passed}/{total} ({passed / total * 100:.0f}%), "
          f"编辑失败痕迹总计: {edit_fails}")

    if args.report:
        Path(args.report).write_text(json.dumps({
            "passed": passed, "total": total,
            "pass_rate": passed / total, "edit_failures": edit_fails,
            "results": results,
        }, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
