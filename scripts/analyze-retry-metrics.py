#!/usr/bin/env python3
"""M1–M3 acceptance metrics from a kimix session `updates.jsonl`.

Redefines the retry-storm problem as measurable signals (not vibes):

  M1  max_peak_attempt     — highest `retry_state.attempt` in any transport chain
  M2  total_retrying       — count of `retry_state` type=retrying events
  M3  storm_wall_seconds   — sum of wall time across transport retry chains
                              (first retrying → last retrying of each chain)

Transport reasons (capped by STREAM_TRANSPORT_RETRY_THRESHOLD = 3):
  - EventStreamError / "error decoding response body"
  - Http send failures / "error sending request"

Acceptance after P0 stream cap (threshold=3):
  - M1 ≤ 2   (attempt 1,2 may emit Retrying; 3rd failure is Fatal)
  - For a pure transport-storm session, M2 and M3 drop sharply vs baseline

This script is offline analysis only — it does not need a new binary.
Historical sessions still reflect pre-fix behavior (use as baseline).
Post-fix sessions should pass the gates when the new code is installed.

Usage:
  python3 scripts/analyze-retry-metrics.py PATH/to/updates.jsonl
  python3 scripts/analyze-retry-metrics.py PATH/to/session_dir
  python3 scripts/analyze-retry-metrics.py PATH --gate   # exit 1 if M1>2
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


TRANSPORT_NEEDLES = (
    "error decoding response body",
    "event stream",
    "error sending request",
    "connection reset",
    "connection closed",
    "broken pipe",
    "error reading a body",
    "timed out",
    "dns error",
)


def is_transport_reason(reason: str) -> bool:
    r = (reason or "").lower()
    return any(n in r for n in TRANSPORT_NEEDLES)


@dataclass
class RetryEvent:
    line: int
    ts: int | None
    typ: str  # retrying | exhausted | failed
    attempt: int | None
    max_retries: int | None
    reason: str


@dataclass
class Chain:
    events: list[RetryEvent] = field(default_factory=list)

    @property
    def peak_attempt(self) -> int:
        return max((e.attempt or 0) for e in self.events) if self.events else 0

    @property
    def wall_seconds(self) -> int:
        # Only span retrying events — a late failed/exhausted must not
        # inflate M3 with idle time after the storm already ended.
        ts = [e.ts for e in self.events if e.typ == "retrying" and e.ts is not None]
        if len(ts) < 2:
            return 0
        return max(0, ts[-1] - ts[0])

    @property
    def is_transport(self) -> bool:
        return any(is_transport_reason(e.reason) for e in self.events if e.typ == "retrying")

    @property
    def retrying_count(self) -> int:
        return sum(1 for e in self.events if e.typ == "retrying")


def load_retry_events(path: Path) -> list[RetryEvent]:
    events: list[RetryEvent] = []
    with path.open(encoding="utf-8") as f:
        for i, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                o = json.loads(line)
            except json.JSONDecodeError:
                continue
            upd = (o.get("params") or {}).get("update") or {}
            if upd.get("sessionUpdate") != "retry_state":
                continue
            events.append(
                RetryEvent(
                    line=i,
                    ts=o.get("timestamp"),
                    typ=str(upd.get("type") or ""),
                    attempt=upd.get("attempt"),
                    max_retries=upd.get("max_retries"),
                    reason=str(upd.get("reason") or upd.get("message") or ""),
                )
            )
    return events


def group_chains(events: list[RetryEvent]) -> list[Chain]:
    """Group retrying events into per-request chains.

    A new chain starts when:
      - attempt is 1 (or None) after a previous non-empty chain, or
      - attempt decreases relative to previous retrying attempt, or
      - a non-retrying terminal (failed/exhausted) closes the chain.
    """
    chains: list[Chain] = []
    cur: Chain | None = None
    last_attempt = 0

    def close() -> None:
        nonlocal cur, last_attempt
        if cur and cur.events:
            chains.append(cur)
        cur = None
        last_attempt = 0

    for e in events:
        if e.typ == "retrying":
            att = e.attempt or 0
            if cur is None:
                cur = Chain()
            elif att <= 1 and last_attempt >= 1:
                close()
                cur = Chain()
            elif att > 0 and last_attempt > 0 and att < last_attempt:
                close()
                cur = Chain()
            assert cur is not None
            cur.events.append(e)
            last_attempt = att
        elif e.typ in ("failed", "exhausted"):
            if cur is None:
                cur = Chain()
            cur.events.append(e)
            close()
        else:
            # unknown type — close current
            if cur is not None:
                cur.events.append(e)
                close()
    close()
    return chains


def resolve_path(raw: str) -> Path:
    p = Path(raw).expanduser()
    if p.is_dir():
        cand = p / "updates.jsonl"
        if cand.is_file():
            return cand
        raise SystemExit(f"no updates.jsonl in directory: {p}")
    if not p.is_file():
        raise SystemExit(f"file not found: {p}")
    return p


def analyze(path: Path) -> dict[str, Any]:
    events = load_retry_events(path)
    chains = group_chains(events)
    transport_chains = [c for c in chains if c.is_transport]
    all_retrying = [e for e in events if e.typ == "retrying"]
    transport_retrying = [e for e in all_retrying if is_transport_reason(e.reason)]

    reasons = Counter((e.reason[:100] or "(empty)") for e in all_retrying)
    types = Counter(e.typ for e in events)

    m1 = max((c.peak_attempt for c in transport_chains), default=0)
    m2 = len(transport_retrying)
    m3 = sum(c.wall_seconds for c in transport_chains)

    # STREAM_TRANSPORT_RETRY_THRESHOLD = 3 → max emitted attempt is 2
    stream_threshold = 3
    gate_m1_max = stream_threshold - 1  # 2
    m1_pass = m1 <= gate_m1_max

    return {
        "path": str(path),
        "retry_state_events": len(events),
        "types": dict(types),
        "reasons_top": reasons.most_common(10),
        "chains_total": len(chains),
        "chains_transport": len(transport_chains),
        "M1_max_peak_attempt_transport": m1,
        "M2_total_transport_retrying": m2,
        "M3_transport_storm_wall_seconds": m3,
        "gate": {
            "STREAM_TRANSPORT_RETRY_THRESHOLD": stream_threshold,
            "M1_max_allowed": gate_m1_max,
            "M1_pass": m1_pass,
            "note": (
                "Historical pre-fix sessions are expected to FAIL the gate. "
                "Pass only after a new binary records a session under the cap."
            ),
        },
        "worst_chains": [
            {
                "peak": c.peak_attempt,
                "retrying": c.retrying_count,
                "wall_s": c.wall_seconds,
                "lines": f"{c.events[0].line}-{c.events[-1].line}",
                "reason": (c.events[0].reason or "")[:80],
            }
            for c in sorted(transport_chains, key=lambda c: (c.peak_attempt, c.wall_seconds), reverse=True)[:8]
        ],
    }


def print_report(report: dict[str, Any]) -> None:
    print(f"file: {report['path']}")
    print(f"retry_state events: {report['retry_state_events']}  types={report['types']}")
    print("top reasons:")
    for reason, n in report["reasons_top"]:
        print(f"  {n:4d}  {reason}")
    print(
        f"chains: total={report['chains_total']}  transport={report['chains_transport']}"
    )
    print()
    print("=== Metrics ===")
    print(f"  M1 max peak attempt (transport) : {report['M1_max_peak_attempt_transport']}")
    print(f"  M2 total transport retrying     : {report['M2_total_transport_retrying']}")
    print(f"  M3 transport storm wall seconds : {report['M3_transport_storm_wall_seconds']}")
    g = report["gate"]
    status = "PASS" if g["M1_pass"] else "FAIL"
    print()
    print(
        f"=== Gate (STREAM_TRANSPORT={g['STREAM_TRANSPORT_RETRY_THRESHOLD']}, "
        f"M1≤{g['M1_max_allowed']}) : {status} ==="
    )
    print(f"  {g['note']}")
    if report["worst_chains"]:
        print()
        print("worst transport chains:")
        for c in report["worst_chains"]:
            print(
                f"  peak={c['peak']} retrying={c['retrying']} wall_s={c['wall_s']} "
                f"L{c['lines']}  {c['reason']}"
            )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("path", help="updates.jsonl or session directory")
    ap.add_argument(
        "--gate",
        action="store_true",
        help="exit 1 if M1 exceeds STREAM_TRANSPORT-1 (for post-fix sessions)",
    )
    ap.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    args = ap.parse_args()

    path = resolve_path(args.path)
    report = analyze(path)
    if args.json:
        json.dump(report, sys.stdout, ensure_ascii=False, indent=2)
        print()
    else:
        print_report(report)

    if args.gate and not report["gate"]["M1_pass"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
