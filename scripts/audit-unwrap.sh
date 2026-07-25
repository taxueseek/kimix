#!/bin/sh
# Audit production unwrap/expect/panic hotspots.
# Excludes test files and prints top-N files by count.
#
# Usage: ./scripts/audit-unwrap.sh [N]

set -eu
TOP=${1:-15}
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== Top $TOP unwrap() hotspots (production code) ==="
grep -r "\.unwrap()" "$ROOT/crates/" --include="*.rs" \
    | grep -v "/target/" \
    | grep -v "/tests/" \
    | grep -v "#\[test\]" \
    | grep -v "#\[cfg(test)\]" \
    | sed 's/:.*//' \
    | sort | uniq -c | sort -rn | head -n "$TOP"

echo ""
echo "=== Top $TOP expect() hotspots (production code) ==="
grep -r "\.expect(" "$ROOT/crates/" --include="*.rs" \
    | grep -v "/target/" \
    | grep -v "/tests/" \
    | grep -v "#\[test\]" \
    | grep -v "#\[cfg(test)\]" \
    | sed 's/:.*//' \
    | sort | uniq -c | sort -rn | head -n "$TOP"

echo ""
echo "=== Top $TOP panic! hotspots (production code) ==="
grep -r "panic!(" "$ROOT/crates/" --include="*.rs" \
    | grep -v "/target/" \
    | grep -v "/tests/" \
    | grep -v "#\[test\]" \
    | grep -v "#\[cfg(test)\]" \
    | sed 's/:.*//' \
    | sort | uniq -c | sort -rn | head -n "$TOP"

echo ""
echo "=== unsafe block count (production) ==="
grep -r "unsafe" "$ROOT/crates/" --include="*.rs" \
    | grep -v "/target/" \
    | grep -v "//.*unsafe" \
    | grep -v "/tests/" \
    | grep -v "#\[test\]" \
    | wc -l
