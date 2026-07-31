#!/usr/bin/env bash
# Context economy + prompt-cache terminal bench (DeepSeek / LongCat).
#
# Measures (per run):
#   - wall time
#   - headless JSON usage: input/output/cache_read, numTurns
#   - debug log: kimix_sampler::prompt_cache hit % per model call
#   - debug log: soft_nudge / content-hash dedup (when present)
#
# Modes:
#   on  — defaults (soft_nudge ~0.55, content_hash_dedup on)
#   off — KIMIX_SOFT_NUDGE_RATIO=0 KIMIX_CONTENT_HASH_DEDUP=0
#
# Usage:
#   scripts/bench-context-economy.sh [kimix-bin]
#   MODELS=on,off MODELS=deepseek-flash,longcat MAX_TURNS=10 \
#     scripts/bench-context-economy.sh target/release/kimix
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KIMIX_BIN="${1:-$ROOT/target/release/kimix}"
# Resolve after optional relative arg so `cd $FIX` cannot break PATH lookup.
if [[ "$KIMIX_BIN" != /* ]]; then
  KIMIX_BIN="$(cd "$(dirname "$KIMIX_BIN")" && pwd)/$(basename "$KIMIX_BIN")"
fi
MODELS="${MODELS:-deepseek-flash,longcat}"
MODES="${MODES:-on,off}"
MAX_TURNS="${MAX_TURNS:-12}"
FIX="${FIX:-/tmp/kimix-bench-ctx-2026-07-30}"
OUT_ROOT="${OUT_ROOT:-$ROOT/docs/bench-results}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$OUT_ROOT/context-economy-$STAMP"
mkdir -p "$OUT_DIR"

if [[ ! -x "$KIMIX_BIN" ]]; then
  echo "error: kimix binary not found: $KIMIX_BIN" >&2
  echo "build with: cargo build --release -p kimix-bin" >&2
  exit 2
fi

if [[ ! -d "$FIX/src" ]]; then
  echo "error: fixture missing at $FIX (expected src/big_module.rs)" >&2
  exit 2
fi

# Multi-turn tool task: force reads + re-read same large file (dedup pressure),
# then answer from structure (outline/grep path if available).
PROMPT="$(cat <<'EOF'
You are in a small Rust fixture. Complete ALL steps with tools, then answer briefly.

1) list_dir on .
2) read_file src/big_module.rs (full or large chunk)
3) grep for "compute_10" under src
4) read_file src/big_module.rs AGAIN (same file — intentional re-read)
5) read_file src/main.rs
6) Reply with ONLY:
   - count of `pub fn compute_` in big_module (integer)
   - whether marker-alpha exists in main.rs (yes/no)
   - first line of big_module.rs

Rules: no git write, no network, no edits. Prefer tools over guessing. Keep final answer under 8 lines.
EOF
)"

echo "bin:    $KIMIX_BIN ($("$KIMIX_BIN" --version 2>/dev/null | head -1))"
echo "fix:    $FIX"
echo "out:    $OUT_DIR"
echo "models: $MODELS"
echo "modes:  $MODES"
echo "turns:  $MAX_TURNS"
echo

summary_tsv="$OUT_DIR/summary.tsv"
printf 'model\tmode\texit\twall_s\tnum_turns\tuncached_in\tcache_read\tout\ttotal\tavg_hit_pct\tmax_hit_pct\tn_cache_logs\tnudge_logs\tdedup_logs\tresult_json\n' \
  >"$summary_tsv"

run_one() {
  local model="$1" mode="$2"
  local tag="${model}__${mode}"
  local log="$OUT_DIR/${tag}.debug.log"
  local json="$OUT_DIR/${tag}.result.json"
  local stdout="$OUT_DIR/${tag}.stdout.txt"
  local meta="$OUT_DIR/${tag}.meta.txt"
  local env_note=""

  export RUST_LOG="kimix_sampler::prompt_cache=debug,kimix_shell::session::kimix_recall=debug,info"
  # Do NOT set KIMIX_SHARE_DIR: user models/keys live under ~/.kimix/config.toml.
  # Isolating share drops custom model catalog and causes auth fallback failures.

  unset KIMIX_SOFT_NUDGE_RATIO KIMIX_CONTENT_HASH_DEDUP || true
  if [[ "$mode" == "off" ]]; then
    export KIMIX_SOFT_NUDGE_RATIO=0
    export KIMIX_CONTENT_HASH_DEDUP=0
    env_note="soft_nudge=0 content_hash_dedup=0"
  else
    env_note="soft_nudge=default content_hash_dedup=default"
  fi

  # Tighten tool output so large re-reads still land under budget paths.
  export KIMIX_MAX_TOOL_OUTPUT_BYTES="${KIMIX_MAX_TOOL_OUTPUT_BYTES:-40000}"
  export KIMIX_MAX_TOOL_OUTPUT_CHARS="${KIMIX_MAX_TOOL_OUTPUT_CHARS:-20000}"

  echo "── $tag ($env_note) ──"
  local start end wall rc
  start=$(date +%s)
  set +e
  (
    cd "$FIX"
    # shellcheck disable=SC2086
    "$KIMIX_BIN" \
      -m "$model" \
      -p "$PROMPT" \
      --max-turns "$MAX_TURNS" \
      --always-approve \
      --output-format json \
      --no-memory \
      --no-subagents \
      --disable-web-search \
      --debug-file "$log" \
      2>"$OUT_DIR/${tag}.stderr.txt"
  ) >"$stdout"
  rc=$?
  set -e
  end=$(date +%s)
  wall=$((end - start))

  # Prefer last JSON object on stdout
  if command -v python3 >/dev/null; then
    python3 - "$stdout" "$json" <<'PY' || true
import json, sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, encoding="utf-8", errors="replace").read().strip()
obj = None
# whole file
try:
    obj = json.loads(text)
except Exception:
    # last {...} block
    i = text.rfind("{")
    if i >= 0:
        try:
            obj = json.loads(text[i:])
        except Exception:
            obj = None
if obj is not None:
    json.dump(obj, open(dst, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
    print("wrote", dst)
else:
    open(dst, "w").write(text[:2000])
    print("no json; wrote raw snippet", file=sys.stderr)
PY
  else
    cp "$stdout" "$json"
  fi

  # Parse metrics
  local metrics
  metrics=$(python3 - "$json" "$log" <<'PY'
import json, re, sys
from pathlib import Path
jpath, lpath = Path(sys.argv[1]), Path(sys.argv[2])
num_turns = uncached = cache = out = total = ""
try:
    data = json.loads(jpath.read_text(encoding="utf-8"))
except Exception:
    data = {}
usage = data.get("usage") or {}
# headless projects uncached as input_tokens
uncached = usage.get("input_tokens", usage.get("inputTokens", ""))
cache = usage.get("cache_read_input_tokens", usage.get("cacheReadInputTokens", usage.get("cache_read_tokens", "")))
out = usage.get("output_tokens", usage.get("outputTokens", ""))
total = usage.get("total_tokens", usage.get("totalTokens", ""))
num_turns = usage.get("numTurns", usage.get("num_turns", data.get("numTurns", "")))
# modelUsage sum fallback
if not cache and isinstance(usage.get("modelUsage"), dict):
    c = i = o = 0
    for m in usage["modelUsage"].values():
        i += int(m.get("input_tokens") or m.get("inputTokens") or 0)
        o += int(m.get("output_tokens") or m.get("outputTokens") or 0)
        c += int(m.get("cached_read_tokens") or m.get("cache_read_input_tokens") or 0)
    if i or c or o:
        uncached, cache, out = i, c, o
        total = (int(uncached or 0) + int(cache or 0) + int(out or 0)) if total == "" else total

log = lpath.read_text(encoding="utf-8", errors="replace") if lpath.exists() else ""
hits = []
for m in re.finditer(r"cache_hit_percent[=:]?\s*([0-9.]+)", log):
    hits.append(float(m.group(1)))
# also structured tracing fields style
for m in re.finditer(r"cache_hit_percent:\s*([0-9.]+)", log):
    hits.append(float(m.group(1)))
avg = max_h = nlog = 0
if hits:
    avg = sum(hits) / len(hits)
    max_h = max(hits)
    nlog = len(hits)
nudge = len(re.findall(r"soft_nudge", log, flags=re.I))
dedup = len(re.findall(r"content.?hash|dedup|admit_tool", log, flags=re.I))
print(f"{num_turns}\t{uncached}\t{cache}\t{out}\t{total}\t{avg:.2f}\t{max_h:.2f}\t{nlog}\t{nudge}\t{dedup}")
PY
  )

  IFS=$'\t' read -r num_turns uncached cache out total avg_hit max_hit n_cache nudge dedup <<<"$metrics"
  {
    echo "model=$model mode=$mode rc=$rc wall_s=$wall"
    echo "env=$env_note"
    echo "usage: turns=$num_turns uncached_in=$uncached cache_read=$cache out=$out total=$total"
    echo "log: avg_hit%=$avg_hit max_hit%=$max_hit n_cache_logs=$n_cache soft_nudge_hits=$nudge dedup_hits=$dedup"
  } | tee "$meta"

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$model" "$mode" "$rc" "$wall" "$num_turns" "$uncached" "$cache" "$out" "$total" \
    "$avg_hit" "$max_hit" "$n_cache" "$nudge" "$dedup" "$json" \
    >>"$summary_tsv"
  echo
}

IFS=',' read -ra MODEL_ARR <<<"$MODELS"
IFS=',' read -ra MODE_ARR <<<"$MODES"
for model in "${MODEL_ARR[@]}"; do
  for mode in "${MODE_ARR[@]}"; do
    run_one "$model" "$mode"
  done
done

# Markdown report
python3 - "$summary_tsv" "$OUT_DIR/REPORT.md" "$KIMIX_BIN" <<'PY'
import sys, subprocess
from pathlib import Path
tsv, md, binp = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
rows = []
for i, line in enumerate(tsv.read_text().splitlines()):
    if not line.strip():
        continue
    parts = line.split("\t")
    if i == 0:
        header = parts
        continue
    rows.append(dict(zip(header, parts)))

ver = ""
try:
    ver = subprocess.check_output([binp, "--version"], text=True).strip().splitlines()[0]
except Exception:
    pass

lines = [
    "# Context economy terminal bench",
    "",
    f"- binary: `{binp}`",
    f"- version: `{ver}`",
    f"- fixture: `/tmp/kimix-bench-ctx-2026-07-30`",
    "",
    "## Results",
    "",
    "| model | mode | exit | wall_s | turns | uncached_in | cache_read | out | avg_hit% | max_hit% | nudge_logs | dedup_logs |",
    "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
]
for r in rows:
    lines.append(
        f"| {r.get('model','')} | {r.get('mode','')} | {r.get('exit','')} | {r.get('wall_s','')} | "
        f"{r.get('num_turns','')} | {r.get('uncached_in','')} | {r.get('cache_read','')} | {r.get('out','')} | "
        f"{r.get('avg_hit_pct','')} | {r.get('max_hit_pct','')} | {r.get('nudge_logs','')} | {r.get('dedup_logs','')} |"
    )
lines += [
    "",
    "## Notes",
    "",
    "- `mode=on`: default soft_nudge + content_hash_dedup",
    "- `mode=off`: `KIMIX_SOFT_NUDGE_RATIO=0` and `KIMIX_CONTENT_HASH_DEDUP=0`",
    "- `uncached_in` is headless-projected (full − cache_read); higher cache_read ⇒ more prompt-cache reuse",
    "- soft_nudge only fires when estimated usage is past the soft band of effective window; short benches may show 0 nudge_logs",
    "- provider must report cache_read tokens; if 0 for both modes, API may not expose cache metrics for that model/route",
    "",
]
md.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(md.read_text())
PY

echo "summary: $summary_tsv"
echo "report:  $OUT_DIR/REPORT.md"
