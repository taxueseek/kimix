#!/bin/sh
# Install privacy-guard as pre-commit (local warn) + pre-push (upload block).
set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
HOOKS="$ROOT/.git/hooks"
GUARD="$ROOT/scripts/privacy-guard.sh"

if [ ! -d "$ROOT/.git" ]; then
  echo "not a git repo: $ROOT" >&2
  exit 1
fi
if [ ! -f "$GUARD" ]; then
  echo "missing $GUARD" >&2
  exit 1
fi

chmod +x "$GUARD"
mkdir -p "$HOOKS"

# pre-commit → local mode (warn secrets, allow commit)
cat > "$HOOKS/pre-commit" <<EOF
#!/bin/sh
export PRIVACY_GUARD_MODE=local
exec "$GUARD" "\$@"
EOF
chmod +x "$HOOKS/pre-commit"

# pre-push → upload mode (hard block)
cat > "$HOOKS/pre-push" <<EOF
#!/bin/sh
export PRIVACY_GUARD_MODE=upload
exec "$GUARD" "\$@"
EOF
chmod +x "$HOOKS/pre-push"

echo "privacy-guard installed:"
echo "  pre-commit  → local  (warn risk, allow config/fixture tests)"
echo "  pre-push    → upload (hard block secrets + public identity leaks)"
echo "  script: $GUARD"
