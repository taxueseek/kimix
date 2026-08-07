#!/usr/bin/env python3
"""Validate a design document's PR Plan section.

Usage:
    python3 validate-plan.py <design-doc-path>

Parses the ``## PR Plan`` section, extracts PR entries from ``### PR N:``
headings, validates the dependency DAG (unique IDs, valid references, no
cycles), computes level assignments, linearises for Graphite stack order,
and reports max parallelism.

Output (stdout): a JSON report.
Exit codes: 0 = valid, 1 = validation errors, 2 = usage / IO errors.
"""

import json
import re
import sys
from collections import defaultdict, deque

# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------


def _strip_fenced_code_blocks(content):
    """Remove fenced code blocks (backtick and tilde) so headings inside examples are ignored."""
    flags = re.MULTILINE | re.DOTALL
    content = re.sub(r"^\s*```[^\n]*\n.*?^\s*```\s*$", "", content, flags=flags)
    return re.sub(r"^\s*~~~[^\n]*\n.*?^\s*~~~\s*$", "", content, flags=flags)


def parse_pr_plan(content):
    """Parse the ``## PR Plan`` section from a design document.

    Returns ``(entries, errors)`` where *entries* is a list of dicts and
    *errors* is an empty list on success, or *entries* is ``None`` and
    *errors* is a non-empty list of error strings on failure.
    """
    # Strip fenced code blocks so example PR Plans inside ``` ``` are ignored.
    stripped = _strip_fenced_code_blocks(content)
    heading = re.search(r"^## PR Plan\s*$", stripped, re.MULTILINE)
    if heading is None:
        return None, ["No '## PR Plan' section found in the document"]

    start = heading.end()
    # Slice to the next ``## `` heading (that isn't the PR Plan itself) or EOF.
    next_section = re.search(r"^## (?!PR Plan\s*$)", stripped[start:], re.MULTILINE)
    section = stripped[start : start + next_section.start()] if next_section else stripped[start:]

    pr_re = re.compile(r"^###\s+PR\s+(\S+?):\s*(.+)$", re.MULTILINE)
    matches = list(pr_re.finditer(section))

    if not matches:
        return None, ["No PR entries found in the PR Plan section"]

    entries = []
    parse_errors = []
    for i, m in enumerate(matches):
        pr_num = m.group(1)
        title = m.group(2).strip()

        body_start = m.end()
        body_end = matches[i + 1].start() if i + 1 < len(matches) else len(section)
        body = section[body_start:body_end]

        files_raw = _extract_field(body, "Files/components affected")
        deps_raw = _extract_field(body, "Dependencies")
        description = _extract_field(body, "Description") or ""

        files = [f.strip() for f in files_raw.split(",") if f.strip()] if files_raw else []
        dependencies, dep_errors = _parse_dependencies(deps_raw, "PR {}".format(pr_num))
        parse_errors.extend(dep_errors)

        entries.append(
            {
                "id": "pr-{}".format(pr_num.lower()),
                "number": pr_num,
                "title": title,
                "files": files,
                "dependencies": dependencies,
                "description": description,
            }
        )

    if parse_errors:
        return None, parse_errors
    return entries, []


def _extract_field(body, field_name):
    """Return the value of a ``- **FieldName:** value`` bullet, or *None*.

    Handles continuation lines (indented past the bullet marker).
    """
    escaped = re.escape(field_name)
    # Handles **Field:** val, **Field**: val, and plain Field: val.
    # The tail ``(?:\n[ \t]+\S.*)*`` grabs indented continuation lines.
    pattern = re.compile(
        rf"^\s*[-*]\s+\**{escaped}:?\**:?\s*(.+(?:\n[ \t]+\S.*)*)",
        re.MULTILINE | re.IGNORECASE,
    )
    m = pattern.search(body)
    if m:
        return re.sub(r"\s*\n[ \t]+", " ", m.group(1)).strip()
    return None


def _parse_dependencies(deps_raw, pr_label):
    """Turn a dependency string like ``"PR 0a, PR 1"`` into ``(ids, errors)``.

    *pr_label* (e.g. ``"PR 1"``) is used in error messages for context.
    Returns a tuple ``(dep_ids, errors)`` where *errors* is a list of
    strings describing any comma-separated parts that could not be parsed.
    """
    if not deps_raw:
        return [], []
    stripped = deps_raw.strip()
    if stripped.lower() in ("none", "n/a", "-", ""):
        return [], []

    parts = [p.strip() for p in stripped.split(",")]
    deps = []
    errors = []
    for part in parts:
        if not part:
            continue
        m = re.match(r"PR\s+(\S+)", part, re.IGNORECASE)
        if m:
            deps.append("pr-{}".format(m.group(1).lower()))
        else:
            errors.append(
                "Unrecognized dependency format '{}' in {} (expected 'PR <id>')".format(
                    part, pr_label
                )
            )
    return deps, errors


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------


def validate_dag(entries):
    """Check unique IDs, valid dependency references, and no cycles."""
    errors = []

    # --- unique IDs ---
    seen = set()
    for entry in entries:
        if entry["id"] in seen:
            errors.append("Duplicate PR ID: '{}'".format(entry["id"]))
        seen.add(entry["id"])

    # --- valid references ---
    for entry in entries:
        for dep in entry["dependencies"]:
            if dep not in seen:
                dep_label = dep.replace("pr-", "PR ", 1)
                entry_label = entry["id"].replace("pr-", "PR ", 1)
                errors.append(
                    "Dependency '{}' in {} does not reference a valid PR ID".format(
                        dep_label, entry_label
                    )
                )

    # --- cycles (only when references are valid) ---
    if not errors:
        errors.extend(_detect_cycles(entries))

    return errors


def _detect_cycles(entries):
    """Kahn's algorithm for topological sort; returns cycle errors."""
    in_degree = {e["id"]: 0 for e in entries}
    children = defaultdict(list)
    dep_map = {e["id"]: e["dependencies"] for e in entries}

    for entry in entries:
        for dep in entry["dependencies"]:
            children[dep].append(entry["id"])
            in_degree[entry["id"]] += 1

    queue = deque(eid for eid, deg in in_degree.items() if deg == 0)
    visited = 0

    while queue:
        node = queue.popleft()
        visited += 1
        for child in children[node]:
            in_degree[child] -= 1
            if in_degree[child] == 0:
                queue.append(child)

    if visited == len(entries):
        return []

    unvisited = [e["id"] for e in entries if in_degree[e["id"]] > 0]
    cycle = _trace_cycle(dep_map, unvisited)
    if cycle:
        return ["Cycle detected: {}".format(" -> ".join(cycle))]
    return ["Cycle detected involving: {}".format(", ".join(sorted(unvisited)))]


def _trace_cycle(dep_map, unvisited_ids):
    """Walk deps among *unvisited_ids* to report one cycle path."""
    unvisited = set(unvisited_ids)
    current = unvisited_ids[0]
    path = [current]
    visited_in_path = {current}

    while True:
        next_node = None
        for dep in dep_map.get(current, []):
            if dep in unvisited:
                next_node = dep
                break
        if next_node is None:
            break
        if next_node in visited_in_path:
            idx = path.index(next_node)
            return path[idx:] + [next_node]
        path.append(next_node)
        visited_in_path.add(next_node)
        current = next_node

    return None


# ---------------------------------------------------------------------------
# DAG processing
# ---------------------------------------------------------------------------


def _pr_sort_key(pr_id):
    """Sort key: numeric PR numbers first (``int``), then lexicographic.

    Ensures ``pr-2`` sorts before ``pr-10`` while ``pr-0a`` / ``pr-0b``
    sort lexicographically among themselves.
    """
    suffix = pr_id.split("-", 1)[1] if "-" in pr_id else pr_id
    try:
        return (0, int(suffix), "")
    except ValueError:
        return (1, 0, suffix)


def compute_levels(entries):
    """Return ``{pr_id: level}``; level 0 = no deps.

    Uses iterative BFS (Kahn's order) to avoid recursion-depth limits
    on deeply-chained DAGs.
    """
    children = defaultdict(list)
    in_degree = {e["id"]: 0 for e in entries}

    for e in entries:
        for dep in e["dependencies"]:
            children[dep].append(e["id"])
            in_degree[e["id"]] += 1

    levels = {}
    queue = deque()
    for eid, deg in in_degree.items():
        if deg == 0:
            levels[eid] = 0
            queue.append(eid)

    while queue:
        node = queue.popleft()
        for child in children[node]:
            candidate = levels[node] + 1
            levels[child] = max(levels.get(child, 0), candidate)
            in_degree[child] -= 1
            if in_degree[child] == 0:
                queue.append(child)

    return levels


def linearize(entries, levels):
    """Flatten the DAG into Graphite stack order (stable within levels)."""
    by_level = defaultdict(list)
    for e in entries:
        by_level[levels[e["id"]]].append(e["id"])

    order = []
    for lv in sorted(by_level):
        order.extend(sorted(by_level[lv], key=_pr_sort_key))
    return order


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main():
    if len(sys.argv) != 2:
        print(
            json.dumps(
                {
                    "valid": False,
                    "errors": ["Usage: python3 {} <design-doc-path>".format(sys.argv[0])],
                },
                indent=2,
            )
        )
        sys.exit(2)

    path = sys.argv[1]

    try:
        with open(path, "r") as fh:
            content = fh.read()
    except FileNotFoundError:
        print(json.dumps({"valid": False, "errors": ["File not found: {}".format(path)]}, indent=2))
        sys.exit(2)
    except IOError as exc:
        print(
            json.dumps(
                {"valid": False, "errors": ["Cannot read file {}: {}".format(path, exc)]},
                indent=2,
            )
        )
        sys.exit(2)

    try:
        # -- parse --
        entries, parse_errors = parse_pr_plan(content)
        if parse_errors:
            print(json.dumps({"valid": False, "errors": parse_errors}, indent=2))
            sys.exit(1)

        # -- validate --
        errors = validate_dag(entries)
        if errors:
            print(json.dumps({"valid": False, "errors": errors}, indent=2))
            sys.exit(1)

        # -- compute execution plan --
        levels = compute_levels(entries)
        order = linearize(entries, levels)

        num_levels = max(levels.values()) + 1 if levels else 0
        counts = defaultdict(int)
        for lv in levels.values():
            counts[lv] += 1
        max_parallelism = max(counts.values()) if counts else 0

        print(
            json.dumps(
                {
                    "valid": True,
                    "pr_count": len(entries),
                    "levels": num_levels,
                    "max_parallelism": max_parallelism,
                    "linearized_order": order,
                    "level_assignments": {pid: levels[pid] for pid in order},
                    "errors": [],
                },
                indent=2,
            )
        )
        sys.exit(0)
    except Exception as exc:
        print(
            json.dumps(
                {"valid": False, "errors": ["Internal error: {}".format(exc)]},
                indent=2,
            )
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
