#!/bin/sh
#
# Build kimix release binary + fix macOS code-signing.
#
# macOS taskgated rejects adhoc linker signatures on binaries in certain
# paths (e.g. ~/Documents). This wraps `cargo build --release` and
# re-signs the binary so it passes gatekeeper validation.
#
# Usage:
#   ./scripts/build-release.sh           # default release build
#   ./scripts/build-release.sh --dist    # hardened release-dist build

set -eu

PROFILE="${1:-release}"
case "$PROFILE" in
    --dist) PROFILE="release-dist" ;;
    release|release-dist) ;;
    *) echo "usage: $0 [release|--dist]" >&2; exit 1 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "Building kimix ($PROFILE profile)..."
cargo build --profile "$PROFILE" --bin kimix

BIN="$ROOT/target/$PROFILE/kimix"
if [ ! -f "$BIN" ]; then
    echo "error: binary not found at $BIN" >&2
    exit 1
fi

# macOS: re-sign adhoc to pass taskgated validation.
# On Linux this is a no-op (codesign doesn't exist).
if [ "$(uname -s)" = "Darwin" ]; then
    if command -v codesign >/dev/null 2>&1; then
        codesign --force --sign - "$BIN"
        echo "codesign: re-signed $BIN"
    fi
fi

echo "done: $BIN"
