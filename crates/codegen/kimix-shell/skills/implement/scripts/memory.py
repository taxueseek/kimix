#!/usr/bin/env python3
"""Implement skill memory file manager.

The /implement skill records common review issue patterns to a workspace-scoped
memory file so future runs can warn the implementer/reviewers up front. This
helper centralises:

  * **Workspace-scoped path resolution.** The file lives under
    ``$HOME/.kimix/implement-memory/<workspace-id>.md``. The workspace id is
    derived from a canonicalised git remote URL (or, as fallbacks, the absolute
    path of the main ``.git`` directory or the absolute cwd). Canonicalisation
    collapses SSH/HTTPS, with/without ``.git`` suffix, and trailing-slash
    variants of the same upstream onto a single id, so all worktrees and
    clones of one logical repo share a single memory file.

  * **Concurrency-safe writes.** Updates take an exclusive ``fcntl.flock``
    lock (Python stdlib; no ``flock(1)`` shell binary required) on a sibling
    ``.lock`` file and are committed via temp-file + rename, so two /implement
    runs in different worktrees can update the file at the same time without
    corrupting it. Readers see a consistent snapshot.

  * **Deterministic dedup + compaction.** The orchestrator passes this run's
    patterns as JSON; the helper normalises descriptions (case, whitespace,
    trailing punctuation) to merge duplicates into existing entries with
    incremented counters, caps each category at ``MAX_PATTERNS_PER_CATEGORY``
    (lowest-count entries dropped first), and caps the Recent Runs log at
    ``MAX_RECENT_RUNS`` entries.

Subcommands
-----------
``path``      Print the resolved memory file path.
``read``      Print the memory file contents (empty output if file is missing).
``snapshot``  Print the parsed file as JSON, so the orchestrator never has to
              re-parse markdown.
``update``    Read a JSON merge spec from stdin, merge into the memory file
              under an exclusive lock, and print summary stats as pretty JSON.

Update spec (stdin)
-------------------
::

    {
      "patterns": [
        {"category": "Error Handling",
         "description": "Missing null/undefined checks on function inputs"},
        ...
      ],
      "run": {
        "date": "2026-04-23",
        "description": "Add retry logic to blackbox client",
        "rounds": 2,
        "issues_by_severity": {"bug": 1, "suggestion": 1, "nit": 5},
        "key_patterns": ["...", "..."],
        "specializations": ["general", "security"]
      }
    }

Either field may be omitted: send only ``patterns`` to merge issue patterns
without logging a run; send only ``run`` to log a run without new patterns.
Malformed input fails fast with a clear message and exit code 4 — the helper
never silently drops malformed entries.

File permissions
----------------
Both the memory file and the lock file are written and then ``chmod``ded to
mode ``0o600`` (owner read/write only) on a best-effort basis. The memory
file may contain security-review patterns and ``key_patterns`` drawn from
non-public source review, so it must not be world-readable on shared hosts:
the workspace id is a deterministic SHA-256 of the canonical remote URL,
so an unprivileged account on the same host that knows or can guess the
public-repo URL could otherwise enumerate and ``cat`` the file directly.
If a cross-user race causes the ``chmod`` to fail the file's umask-default
mode is preserved (the ``flock`` semantics that matter for correctness do
not depend on the mode); in the typical single-user case both files end
up at ``0o600`` regardless of the process umask.

Platform support
----------------
POSIX only (Linux, macOS, BSD). Imports ``fcntl`` and uses ``os.O_CLOEXEC``
unconditionally; both are POSIX-specific. Windows is not supported.

Python version: 3.9+ recommended (the helper uses PEP 585/604 syntax that is
lazy via ``from __future__ import annotations`` on 3.7+, but stdlib type
introspection of those hints requires 3.9+).

Exit codes
----------
0  success
1  unexpected I/O error not otherwise classified
2  workspace id could not be determined (cwd unreadable, ``$HOME`` unset)
3  lock could not be acquired within ``LOCK_TIMEOUT_SECONDS``
4  invalid JSON on stdin / malformed spec
"""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from collections import OrderedDict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MEMORY_DIR_NAME = "implement-memory"
MAX_PATTERNS_PER_CATEGORY = 25
MAX_RECENT_RUNS = 20
LOCK_TIMEOUT_SECONDS = 30
LOCK_POLL_INTERVAL = 0.2
GIT_TIMEOUT_SECONDS = 5
# Owner-only read/write. The memory file may contain security-review
# patterns and key_patterns drawn from non-public source review; on shared
# hosts where the workspace id (sha256 of the canonical remote URL) is
# guessable, world-readable mode would let unprivileged accounts on the
# same host enumerate and read the file directly. 0o600 prevents that.
FILE_MODE = 0o600

# NOTE: DEFAULT_HEADER is mirrored verbatim in
# .kimix/skills/implement/SKILL.md (Memory File Format section, marked with
# `<!-- mirror-of: scripts/memory.py DEFAULT_HEADER -->`). If you edit one,
# edit the other. The drift-check unit test
# tests.test_memory.TestDocsConsistency.test_skill_md_default_header_matches
# fails the build if they go out of sync.
DEFAULT_HEADER = [
    "# Implementation Review Patterns",
    "",
    "> This file is maintained by the /implement skill.",
    "> It records common issues found during implementation reviews to help avoid them in future runs.",
    "> Shared across all working directories that resolve to the same workspace id.",
]

SEVERITY_ORDER = ("bug", "suggestion", "nit")


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class MemoryHelperError(Exception):
    """Base exception type for this module."""

    exit_code = 1


class WorkspaceIdError(MemoryHelperError):
    """Raised when no usable workspace id can be derived."""

    exit_code = 2


class LockTimeoutError(MemoryHelperError):
    """Raised when an exclusive lock can't be acquired within the deadline."""

    exit_code = 3


class SpecError(MemoryHelperError):
    """Raised when stdin JSON is missing, malformed, or fails validation."""

    exit_code = 4


# ---------------------------------------------------------------------------
# Workspace identification
# ---------------------------------------------------------------------------


def _git(*args: str) -> str | None:
    try:
        result = subprocess.run(
            ["git", *args],
            capture_output=True,
            text=True,
            check=False,
            timeout=GIT_TIMEOUT_SECONDS,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0:
        return None
    out = result.stdout.strip()
    return out or None


# SSH form: optionally allows ssh://user@host:port/path or git@host:path.
_SSH_REMOTE_RE = re.compile(r"^[\w.-]+@([\w.-]+):(.+)$")
# Bare SSH shorthand: "host:path" without any "user@" prefix. Common when
# ~/.ssh/config has e.g. `Host github.com\n  User git`, which lets users
# write "github.com:owner/repo" directly (git resolves the user via the SSH
# config). The host is required to contain at least one dot so we don't
# accidentally swallow relative paths like "subdir:file" or Windows-style
# drive prefixes; the path must not start with "/" so absolute file paths
# (and `https://`-style URLs already handled below) don't match here.
_SSH_BARE_REMOTE_RE = re.compile(r"^([\w-]+(?:\.[\w-]+)+):([^/].*)$")
_URL_REMOTE_RE = re.compile(
    r"^[a-z][a-z0-9+.-]*://(?:[^@]+@)?([\w.-]+)(?::\d+)?(/.+)$",
    re.IGNORECASE,
)

# Forges where the path component (owner/repo) is treated case-insensitively.
# For these hosts the canonical key lowercases both host and path so e.g.
# github.com/Owner/Repo and github.com/owner/repo collapse to one id.
_CASE_INSENSITIVE_HOSTS = frozenset({"github.com", "gitlab.com", "bitbucket.org"})


def canonicalize_remote(url: str) -> str:
    """Normalise a git remote URL to a stable key.

    Collapses these variants of the same upstream onto a single string:
      * ``git@github.com:owner/repo.git``
      * ``github.com:owner/repo.git`` (bare SSH shorthand, ``user@`` omitted
        because it's supplied by ``~/.ssh/config``)
      * ``ssh://git@github.com/owner/repo.git``
      * ``ssh://git@github.com:22/owner/repo.git`` (explicit port)
      * ``https://github.com/owner/repo.git``
      * ``https://github.com:443/owner/repo`` (explicit port)
      * ``https://user@github.com/owner/repo``
      * ``HTTPS://GitHub.com/owner/repo.git/``
      * ``https://github.com/owner/repo.git.git`` (doubled suffix)

    The host is always lowercased. For hosts in ``_CASE_INSENSITIVE_HOSTS``
    (the major public forges) the path is also lowercased so that
    ``github.com/Owner/Repo`` and ``github.com/owner/repo`` collapse onto one
    id. For other (case-sensitive) forges the path is preserved as-is. All
    trailing slashes and any number of trailing ``.git`` suffixes are stripped.

    Limitations:
      * The ``_CASE_INSENSITIVE_HOSTS`` set is exact-match only. Subdomains
        (e.g. ``gist.github.com``) and self-hosted Enterprise instances
        (e.g. ``github.example.com``) are NOT included — their
        case-sensitivity is forge-dependent. Add such hosts to the set
        explicitly if you know they are case-insensitive.
      * The bare SSH shorthand ``git@host:path`` cannot represent an
        explicit port; in this form the ``:`` is always the host/path
        separator. ``git@github.com:22/owner/repo`` is parsed as
        ``host=github.com, path=22/owner/repo`` (i.e. ``22`` becomes the
        leading path segment, not a port). Use the URL form
        ``ssh://git@github.com:22/owner/repo`` if you need explicit ports.
    """
    url = url.strip().rstrip("/")
    while url.endswith(".git"):
        url = url[:-4]
    m = _SSH_REMOTE_RE.match(url)
    if m:
        host = m.group(1).lower()
        path = m.group(2)
        if host in _CASE_INSENSITIVE_HOSTS:
            path = path.lower()
        return f"{host}/{path}"
    m = _SSH_BARE_REMOTE_RE.match(url)
    if m:
        host = m.group(1).lower()
        path = m.group(2)
        if host in _CASE_INSENSITIVE_HOSTS:
            path = path.lower()
        return f"{host}/{path}"
    m = _URL_REMOTE_RE.match(url)
    if m:
        host = m.group(1).lower()
        path = m.group(2)
        if host in _CASE_INSENSITIVE_HOSTS:
            path = path.lower()
        return f"{host}{path}"
    return url


def get_workspace_id() -> str:
    """Return ``"<readable-name>-<hash12>"`` identifying this workspace.

    Order of preference for the source string:

    1. Canonicalised ``git config remote.origin.url`` — stable across all
       clones/worktrees of the same upstream repo, regardless of SSH vs
       HTTPS or ``.git`` suffix.
    2. Absolute path of the main ``.git`` directory (``--git-common-dir``) —
       stable across worktrees of a single clone, even with no remote.
    3. Absolute path of the current working directory — last-ditch fallback
       for non-git workspaces.

    Raises ``WorkspaceIdError`` if all three sources are unusable (very rare:
    e.g., the cwd has been deleted out from under the process).
    """
    raw_remote = _git("config", "--get", "remote.origin.url")
    id_source = canonicalize_remote(raw_remote) if raw_remote else ""

    if not id_source:
        common_dir = _git("rev-parse", "--git-common-dir")
        if common_dir:
            try:
                id_source = str(Path(common_dir).resolve(strict=False))
            except OSError:
                id_source = common_dir

    if not id_source:
        try:
            id_source = str(Path.cwd().resolve())
        except (OSError, FileNotFoundError):
            raise WorkspaceIdError(
                "could not determine workspace id (no git repo, no remote, no usable cwd)"
            )

    digest = hashlib.sha256(id_source.encode("utf-8")).hexdigest()[:12]

    raw_name = id_source.rstrip("/").rsplit("/", 1)[-1]
    if raw_name.endswith(".git"):
        raw_name = raw_name[:-4]
    safe_name = re.sub(r"[^a-zA-Z0-9_.-]+", "_", raw_name).strip("_.-")
    safe_name = safe_name[:40].rstrip("_.-") or "workspace"

    return f"{safe_name}-{digest}"


def memory_paths(*, create_dir: bool = False) -> dict[str, Path]:
    """Return ``{"dir": ..., "file": ..., "lock": ...}`` for this workspace.

    Pure path computation by default. Pass ``create_dir=True`` (used only by
    the ``update`` write path) to ensure the parent directory exists.

    Raises ``WorkspaceIdError`` on unrecoverable HOME / workspace-id failures.
    """
    # Path.home() consults $HOME first on POSIX (and the registry on Windows,
    # which we don't support anyway), so a single call covers both branches.
    try:
        home = Path.home()
    except RuntimeError:
        raise WorkspaceIdError(
            "could not determine the user's home directory ($HOME unset and pwd lookup failed)"
        )
    base = home / ".grok" / MEMORY_DIR_NAME
    if create_dir:
        try:
            base.mkdir(parents=True, exist_ok=True)
        except OSError as exc:
            raise WorkspaceIdError(f"could not create memory directory {base}: {exc}")
    workspace_id = get_workspace_id()
    return {
        "dir": base,
        "file": base / f"{workspace_id}.md",
        "lock": base / f"{workspace_id}.lock",
    }


# ---------------------------------------------------------------------------
# Input sanitisation
# ---------------------------------------------------------------------------


def sanitize_one_line(text: str) -> str:
    """Collapse line-breaking whitespace to single spaces; strip outer whitespace.

    Markdown bullets must live on a single line, so embedded ``\\n``, ``\\r``
    and ``\\t`` are collapsed to single spaces (they would silently break
    round-trip parsing otherwise). Internal multi-space runs are
    **preserved** — e.g., ``"Use `   ` for indentation"`` keeps its three
    spaces. Dedup at the comparison layer (``normalize()``) collapses runs of
    whitespace anyway, so this preserves the user's wording without
    sacrificing dedup.
    """
    return re.sub(r"[\r\n\t]+", " ", text).strip()


# ---------------------------------------------------------------------------
# Type validation helpers
# ---------------------------------------------------------------------------


def _require_str(value: Any, field: str) -> str:
    if not isinstance(value, str):
        raise SpecError(f'"{field}" must be a string, got {type(value).__name__}')
    return value


def _require_optional_str(value: Any, field: str) -> str:
    if value is None:
        return ""
    return _require_str(value, field)


def _require_list(value: Any, field: str) -> list[Any]:
    if not isinstance(value, list):
        raise SpecError(f'"{field}" must be a list, got {type(value).__name__}')
    return value


def _require_dict(value: Any, field: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SpecError(f'"{field}" must be an object, got {type(value).__name__}')
    return value


def _require_int(value: Any, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        # bool is an int subclass in Python; reject it explicitly so that
        # `"rounds": true` doesn't silently render as `1`.
        raise SpecError(f'"{field}" must be an integer, got {type(value).__name__}')
    return value


# ---------------------------------------------------------------------------
# Markdown parsing & rendering
# ---------------------------------------------------------------------------

_SECTION_COMMON_ISSUES_RE = re.compile(r"^##\s+Common Issues\s*$")
_SECTION_RECENT_RUNS_RE = re.compile(r"^##\s+Recent Runs\s*$")
_CATEGORY_RE = re.compile(r"^###\s+(.+?)\s*$")
# Accept singular `time` (count=1) and plural `times` (count!=1).
_ISSUE_RE = re.compile(r"^-\s+(.+?)\s+\(seen\s+(\d+)\s+times?\)\s*$")
# Em-dash, en-dash, and ASCII hyphen are all accepted as separators.
_RUN_RE = re.compile(r"^###\s+(\d{4}-\d{2}-\d{2})\s*[\u2014\u2013-]\s*(.+?)\s*$")


def parse_memory_file(content: str) -> dict[str, Any]:
    """Parse the memory file into a structured dict.

    Returns::

        {
          "header": [str, ...],            # lines before "## Common Issues"
          "common_issues": OrderedDict[
              str,                          # category name
              list[{"description": str, "count": int}]
          ],
          "recent_runs": [
              {"date": str, "description": str, "body_lines": [str, ...]},
              ...
          ],
        }

    Header lines and run body lines are preserved verbatim. Inside
    ``## Common Issues`` only ``### Category`` headers and ``- desc (seen N
    time(s))`` bullets are recognised; any other content there is dropped on
    re-render — SKILL.md warns users not to add freeform notes inside
    Common Issues, only inside the header.
    """
    state: dict[str, Any] = {
        "header": [],
        "common_issues": OrderedDict(),
        "recent_runs": [],
    }

    # Treat blank/whitespace-only files as fresh.
    if not content.strip():
        return state

    section = "header"
    current_category: str | None = None
    current_run: dict[str, Any] | None = None

    for line in content.splitlines():
        if _SECTION_COMMON_ISSUES_RE.match(line):
            if current_run is not None:
                state["recent_runs"].append(current_run)
                current_run = None
            section = "common_issues"
            current_category = None
            continue
        if _SECTION_RECENT_RUNS_RE.match(line):
            if current_run is not None:
                state["recent_runs"].append(current_run)
                current_run = None
            section = "recent_runs"
            current_category = None
            continue

        if section == "header":
            state["header"].append(line)
            continue

        if section == "common_issues":
            cat_match = _CATEGORY_RE.match(line)
            if cat_match:
                current_category = cat_match.group(1).strip()
                state["common_issues"].setdefault(current_category, [])
                continue
            issue_match = _ISSUE_RE.match(line)
            if issue_match and current_category is not None:
                state["common_issues"][current_category].append(
                    {
                        "description": issue_match.group(1).strip(),
                        "count": int(issue_match.group(2)),
                    }
                )
            continue

        if section == "recent_runs":
            run_match = _RUN_RE.match(line)
            if run_match:
                if current_run is not None:
                    state["recent_runs"].append(current_run)
                current_run = {
                    "date": run_match.group(1),
                    "description": run_match.group(2).strip(),
                    "body_lines": [],
                }
                continue
            if current_run is not None:
                current_run["body_lines"].append(line)

    if current_run is not None:
        state["recent_runs"].append(current_run)

    state["header"] = _drop_trailing_blank(state["header"])
    for run in state["recent_runs"]:
        run["body_lines"] = _drop_trailing_blank(run["body_lines"])
    return state


def _drop_trailing_blank(lines: list[str]) -> list[str]:
    """Return a new list with trailing blank/whitespace-only lines removed."""
    end = len(lines)
    while end > 0 and not lines[end - 1].strip():
        end -= 1
    return list(lines[:end])


def render_memory_file(state: dict[str, Any]) -> str:
    out: list[str] = []

    header = state.get("header") or list(DEFAULT_HEADER)
    out.extend(_drop_trailing_blank(header))
    out.append("")

    out.append("## Common Issues")
    out.append("")
    has_any = any(entries for entries in state["common_issues"].values())
    if not has_any:
        out.append("_No patterns recorded yet._")
        out.append("")
    else:
        for category, entries in state["common_issues"].items():
            if not entries:
                continue
            out.append(f"### {category}")
            for e in entries:
                times = "time" if e["count"] == 1 else "times"
                out.append(f"- {e['description']} (seen {e['count']} {times})")
            out.append("")

    out.append("## Recent Runs")
    out.append("")
    for run in state["recent_runs"]:
        out.append(f"### {run['date']} \u2014 {run['description']}")
        out.extend(_drop_trailing_blank(run.get("body_lines") or []))
        out.append("")

    return "\n".join(out).rstrip() + "\n"


# ---------------------------------------------------------------------------
# Merge logic
# ---------------------------------------------------------------------------


_PUNCT_TAIL_RE = re.compile(r"[.;:,!?\s]+$")
_WHITESPACE_RE = re.compile(r"\s+")


def normalize(text: str) -> str:
    """Collapse a description to a canonical form for dedup matching.

    Lowercase, strip trailing punctuation/whitespace, collapse internal
    whitespace runs to single spaces. The orchestrator is expected to
    harmonise phrasing against existing entries before calling ``update``;
    this pass is the safety net for trivial mismatches (case, trailing
    periods, double spaces).
    """
    text = text.lower().strip()
    text = _PUNCT_TAIL_RE.sub("", text)
    text = _WHITESPACE_RE.sub(" ", text)
    return text


def _format_severity_summary(ibs: dict[str, int]) -> tuple[str, int]:
    """Render ``"3 bugs, 4 suggestions"`` and return the total it represents.

    Zero-count entries are skipped (so ``{"bug": 0, "suggestion": 1}`` renders
    as ``"1 suggestion"`` with total 1, not ``"0 bugs, 1 suggestion"``).
    Canonical severities (``bug``/``suggestion``/``nit``) come first in fixed
    order; any extras are appended in sorted-key order for determinism.
    """
    parts: list[str] = []
    rendered_total = 0
    seen: set[str] = set()
    for sev in SEVERITY_ORDER:
        if sev in ibs:
            count = ibs[sev]
            seen.add(sev)
            if count > 0:
                label = sev if count == 1 else f"{sev}s"
                parts.append(f"{count} {label}")
                rendered_total += count
    for sev in sorted(ibs):
        if sev in seen:
            continue
        count = ibs[sev]
        if count > 0:
            label = sev if count == 1 else f"{sev}s"
            parts.append(f"{count} {label}")
            rendered_total += count
    return ", ".join(parts), rendered_total


def _bump_or_append(
    entries: list[dict[str, Any]],
    lookup: dict[str, dict[str, Any]],
    description: str,
) -> bool:
    """Increment matching entry's count, or append a new one.

    ``lookup`` is a per-category index ``{normalize(description): entry_dict}``
    maintained by the caller across all patterns in this update. Caller is
    responsible for building it once per category (giving overall O(n+k)
    work for ``n`` existing entries and ``k`` new patterns) and for keeping
    it in sync — this function updates the lookup whenever it appends.

    Returns ``True`` on a match (existing entry's count was incremented),
    ``False`` on a new append.
    """
    norm_new = normalize(description)
    match = lookup.get(norm_new)
    if match is not None:
        match["count"] += 1
        return True
    new_entry = {"description": description, "count": 1}
    entries.append(new_entry)
    lookup[norm_new] = new_entry
    return False


def merge_run(state: dict[str, Any], spec: dict[str, Any]) -> dict[str, Any]:
    """Merge a run spec into ``state`` in place. Return summary stats.

    Type-validates every field strictly: malformed input raises ``SpecError``
    instead of silently dropping entries or rendering garbage. Newlines and
    other whitespace within description/category/key_pattern strings are
    collapsed to single spaces so they fit on a single markdown line.
    """
    stats: dict[str, Any] = {
        "new_patterns": 0,
        "merged_patterns": 0,
        "categories_touched": [],
        "categories_capped": {},
        "recent_runs_dropped": 0,
    }
    touched: set[str] = set()
    # Per-category lookup dicts {normalize(description): entry}, built lazily
    # the first time we touch a category and reused for every subsequent
    # pattern in the same category. Gives true O(n+k) merge cost.
    category_lookups: dict[str, dict[str, dict[str, Any]]] = {}

    patterns = spec.get("patterns")
    if patterns is not None:
        patterns = _require_list(patterns, "patterns")
        for idx, raw in enumerate(patterns):
            field = f"patterns[{idx}]"
            raw = _require_dict(raw, field)

            category_raw = raw.get("category")
            if category_raw is None or category_raw == "":
                category = "Other"
            else:
                category = sanitize_one_line(_require_str(category_raw, f"{field}.category"))
                if not category:
                    category = "Other"

            # Treat null and omitted description identically (both => skip).
            description = sanitize_one_line(
                _require_optional_str(raw.get("description"), f"{field}.description")
            )
            if not description:
                continue

            entries = state["common_issues"].setdefault(category, [])
            lookup = category_lookups.get(category)
            if lookup is None:
                lookup = {normalize(e["description"]): e for e in entries}
                category_lookups[category] = lookup
            if _bump_or_append(entries, lookup, description):
                stats["merged_patterns"] += 1
            else:
                stats["new_patterns"] += 1
            touched.add(category)

    # Sort each category by count desc, then description asc (case-insensitive);
    # cap length, recording how many entries were dropped per capped category.
    for category, entries in state["common_issues"].items():
        entries.sort(key=lambda e: (-e["count"], e["description"].lower()))
        if len(entries) > MAX_PATTERNS_PER_CATEGORY:
            dropped = len(entries) - MAX_PATTERNS_PER_CATEGORY
            del entries[MAX_PATTERNS_PER_CATEGORY:]
            stats["categories_capped"][category] = dropped

    run_raw = spec.get("run")
    if run_raw is not None:
        run = _require_dict(run_raw, "run")
        if run:
            _merge_recent_run(state, run, stats)

    stats["categories_touched"] = sorted(touched)
    return stats


def _merge_recent_run(state: dict[str, Any], run: dict[str, Any], stats: dict[str, Any]) -> None:
    """Validate and prepend a Recent Runs entry. Updates ``stats`` in place."""
    date_raw = run.get("date")
    if date_raw is None or (isinstance(date_raw, str) and not date_raw.strip()):
        date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    else:
        date_str = _require_str(date_raw, "run.date").strip()
        try:
            datetime.strptime(date_str, "%Y-%m-%d")
        except ValueError:
            raise SpecError(
                f'"run.date" must be a calendar-valid YYYY-MM-DD date, got {date_str!r}'
            )
        date = date_str

    desc_raw = run.get("description")
    description = (
        sanitize_one_line(_require_optional_str(desc_raw, "run.description")) or "(no description)"
    )
    # Strip ALL double-quote characters so the run header has exactly one
    # outer pair and no broken-looking inner quotes. Internal quotes in the
    # user's commit-message-style label would otherwise render as e.g.
    # `### YYYY-MM-DD — "Add retry "logic" to client"` — syntactically
    # parseable but visually broken in any markdown viewer.
    description = description.replace('"', "").strip() or "(no description)"
    description = f'"{description}"'

    body_lines: list[str] = []

    rounds_raw = run.get("rounds")
    if rounds_raw is not None:
        rounds = _require_int(rounds_raw, "run.rounds")
        body_lines.append(f"- **Rounds**: {rounds}")

    ibs_raw = run.get("issues_by_severity")
    if ibs_raw is not None:
        ibs = _require_dict(ibs_raw, "run.issues_by_severity")
        normalized_ibs: dict[str, int] = {}
        for sev, count in ibs.items():
            if not isinstance(sev, str):
                raise SpecError(
                    f'"run.issues_by_severity" keys must be strings, got {type(sev).__name__}'
                )
            normalized_ibs[sev] = _require_int(count, f'run.issues_by_severity["{sev}"]')
        if normalized_ibs:
            summary, rendered_total = _format_severity_summary(normalized_ibs)
            if summary:
                body_lines.append(f"- **Issues**: {rendered_total} total ({summary})")

    key_patterns_raw = run.get("key_patterns")
    if key_patterns_raw is not None:
        key_patterns = _require_list(key_patterns_raw, "run.key_patterns")
        cleaned_kp = [
            sanitize_one_line(_require_str(p, f"run.key_patterns[{i}]"))
            for i, p in enumerate(key_patterns)
        ]
        cleaned_kp = [p for p in cleaned_kp if p]
        if cleaned_kp:
            body_lines.append(f"- **Key patterns**: {', '.join(cleaned_kp)}")

    specs_raw = run.get("specializations")
    if specs_raw is not None:
        specs = _require_list(specs_raw, "run.specializations")
        cleaned_specs = [
            sanitize_one_line(_require_str(s, f"run.specializations[{i}]"))
            for i, s in enumerate(specs)
        ]
        cleaned_specs = [s for s in cleaned_specs if s]
        if cleaned_specs:
            body_lines.append(f"- **Specializations used**: {', '.join(cleaned_specs)}")

    state["recent_runs"].insert(
        0,
        {"date": date, "description": description, "body_lines": body_lines},
    )

    if len(state["recent_runs"]) > MAX_RECENT_RUNS:
        stats["recent_runs_dropped"] = len(state["recent_runs"]) - MAX_RECENT_RUNS
        del state["recent_runs"][MAX_RECENT_RUNS:]


# ---------------------------------------------------------------------------
# File I/O with locking
# ---------------------------------------------------------------------------


def _atomic_write(path: Path, content: str) -> None:
    """Write ``content`` to ``path`` atomically, with mode ``FILE_MODE``."""
    fd, tmp_name = tempfile.mkstemp(
        dir=str(path.parent),
        prefix=f".{path.name}.",
        suffix=".tmp",
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(content)
            f.flush()
            os.fsync(f.fileno())
        # mkstemp creates the file with mode 0o600 already (matching
        # FILE_MODE), but call os.chmod explicitly so this code keeps the
        # documented invariant if FILE_MODE is ever widened — and so the
        # on-disk file mode is independent of any umask weirdness.
        os.chmod(tmp_name, FILE_MODE)
        os.replace(tmp_name, str(path))
    except Exception:
        try:
            os.unlink(tmp_name)
        except OSError:
            pass
        raise


def _acquire_lock(lock_path: Path) -> int:
    """Acquire an exclusive lock on ``lock_path``. Returns the fd to close.

    Uses ``O_RDONLY`` (least privilege — ``flock(2)`` doesn't need write
    access) and ``O_CLOEXEC`` (defence-in-depth: prevents the lock fd from
    leaking into any future child process). After opening, force the file
    mode to ``FILE_MODE`` via ``os.chmod`` so the lock file mode is
    independent of the process umask (matching the memory file's behaviour).
    Polls every ``LOCK_POLL_INTERVAL`` seconds up to ``LOCK_TIMEOUT_SECONDS``.

    Raises ``LockTimeoutError`` on timeout. Raises ``MemoryHelperError`` if
    the lock file itself can't be opened (e.g., parent dir permission
    denied). Closes the fd before raising on any other error so a failed
    acquire never leaks a descriptor.
    """
    try:
        fd = os.open(
            str(lock_path),
            os.O_CREAT | os.O_RDONLY | os.O_CLOEXEC,
            FILE_MODE,
        )
    except OSError as exc:
        raise MemoryHelperError(f"could not open lock file {lock_path}: {exc}")
    try:
        # Force mode regardless of umask. Best-effort (e.g., a non-owner
        # second user touching the lock file shouldn't crash us).
        try:
            os.chmod(str(lock_path), FILE_MODE)
        except OSError:
            pass
        deadline = time.monotonic() + LOCK_TIMEOUT_SECONDS
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return fd
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise LockTimeoutError(
                        f"could not acquire lock on {lock_path} within {LOCK_TIMEOUT_SECONDS}s"
                    )
                time.sleep(LOCK_POLL_INTERVAL)
    except BaseException:
        # Any failure (timeout, OSError, KeyboardInterrupt) must not leak fd.
        try:
            os.close(fd)
        except OSError:
            pass
        raise


def _release_lock(fd: int) -> None:
    try:
        fcntl.flock(fd, fcntl.LOCK_UN)
    except OSError:
        pass
    try:
        os.close(fd)
    except OSError:
        pass


# ---------------------------------------------------------------------------
# Subcommands
# ---------------------------------------------------------------------------


def _build_snapshot(state: dict[str, Any], *, exists: bool) -> dict[str, Any]:
    return {
        "common_issues": [
            {
                "category": cat,
                "description": e["description"],
                "count": e["count"],
            }
            for cat, entries in state["common_issues"].items()
            for e in entries
        ],
        "recent_runs": [
            {
                "date": r["date"],
                "description": r["description"],
                "body_lines": r["body_lines"],
            }
            for r in state["recent_runs"]
        ],
        "exists": exists,
    }


def cmd_path(_args: argparse.Namespace) -> int:
    print(memory_paths()["file"])
    return 0


def cmd_read(_args: argparse.Namespace) -> int:
    f = memory_paths()["file"]
    if f.exists():
        sys.stdout.write(f.read_text(encoding="utf-8"))
    return 0


def cmd_snapshot(_args: argparse.Namespace) -> int:
    f = memory_paths()["file"]
    if f.exists():
        state = parse_memory_file(f.read_text(encoding="utf-8"))
        payload = _build_snapshot(state, exists=True)
    else:
        payload = _build_snapshot(
            {"header": [], "common_issues": OrderedDict(), "recent_runs": []},
            exists=False,
        )
    json.dump(payload, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


def cmd_update(_args: argparse.Namespace) -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        raise SpecError("update requires JSON spec on stdin")
    try:
        spec = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise SpecError(f"invalid JSON on stdin: {exc}")
    if not isinstance(spec, dict):
        raise SpecError("update spec must be a JSON object")

    paths = memory_paths(create_dir=True)
    file_path = paths["file"]
    lock_path = paths["lock"]

    existed_before = file_path.exists()
    fd = _acquire_lock(lock_path)
    try:
        existing = file_path.read_text(encoding="utf-8") if file_path.exists() else ""
        state = parse_memory_file(existing)
        stats = merge_run(state, spec)
        new_content = render_memory_file(state)
        _atomic_write(file_path, new_content)

        json.dump(
            {
                "file": str(file_path),
                "existed_before": existed_before,
                "stats": stats,
                "total_categories": len([c for c, e in state["common_issues"].items() if e]),
                "total_patterns": sum(len(e) for e in state["common_issues"].values()),
                "total_recent_runs": len(state["recent_runs"]),
            },
            sys.stdout,
            indent=2,
        )
        sys.stdout.write("\n")
        return 0
    finally:
        _release_lock(fd)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="memory.py",
        description="Implement skill memory file manager (workspace-scoped, lock-safe).",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("path", help="Print the memory file path for this workspace.")
    sub.add_parser("read", help="Print the memory file contents (empty if missing).")
    sub.add_parser(
        "snapshot",
        help="Print the parsed memory file as JSON (no markdown re-parsing required).",
    )
    sub.add_parser(
        "update",
        help="Merge a JSON spec from stdin into the memory file under an exclusive lock.",
    )

    args = parser.parse_args(argv)
    handlers = {
        "path": cmd_path,
        "read": cmd_read,
        "snapshot": cmd_snapshot,
        "update": cmd_update,
    }
    try:
        return handlers[args.cmd](args)
    except MemoryHelperError as exc:
        print(f"memory.py: {exc}", file=sys.stderr)
        return exc.exit_code
    except OSError as exc:
        # Last-ditch catch for unclassified I/O errors (disk full mid-write,
        # permission flips during execution, FS races). Surfaces a clean
        # message instead of a Python traceback.
        print(f"memory.py: I/O error: {exc}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        # Re-raise so the shell sees the standard 130 exit; never swallow
        # interactive cancellation as a generic error.
        raise
    except Exception as exc:
        # Final safety net. Anything not caught above (e.g.,
        # UnicodeDecodeError raised by Path.read_text(encoding="utf-8") on a
        # corrupted memory file, ValueError from a malformed JSON line that
        # slipped past json.loads, or any other unexpected runtime error)
        # would otherwise dump a Python traceback that pollutes the
        # orchestrator logs and breaks past_issues_briefing/memory flush.
        # Surface the exception class + message in the same `memory.py: ...`
        # format used by MemoryHelperError so consumers can grep for it.
        print(f"memory.py: unexpected error: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
