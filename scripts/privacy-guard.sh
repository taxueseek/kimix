#!/bin/sh
# privacy-guard — local risk prompt vs upload hard-block
#
# Modes (set by hook name or PRIVACY_GUARD_MODE):
#   local  — pre-commit: allow commit, print RISK warnings for secret-like
#            fixtures / env docs so local testing works. Still hard-blocks
#            private-key PEM blocks and machine-like author emails.
#   upload — pre-push: hard-block secrets + (on public remotes) home paths,
#            username, Documents/GPT, hostname. Never override for real keys.
#
# Env:
#   PRIVACY_HOOK_ALLOW_HOME_PATHS=1  — upload mode only: skip identity/path checks
#                                     (secrets still blocked)
#   PRIVACY_GUARD_MODE=local|upload — override auto mode

set -eu

MODE="${PRIVACY_GUARD_MODE:-}"
if [ -z "$MODE" ]; then
  case "${0##*/}" in
    pre-push) MODE=upload ;;
    *)        MODE=local  ;;
  esac
fi

die() {
  echo "git ${MODE} [privacy-guard]: $*" >&2
  exit 1
}

warn() {
  echo "git ${MODE} [privacy-guard] RISK: $*" >&2
}

# --- author identity (always hard-block; never commit as machine email) ---
ident=$(git var GIT_AUTHOR_IDENT 2>/dev/null || true)
email=$(printf '%s' "$ident" | sed -n 's/.*<\(.*\)>.*/\1/p')
if [ -n "$email" ]; then
  case "$email" in
    *MacBook*|*".local"|*"@localhost"|*"@."*|*"@local")
      die "author email looks like machine identity: $email
  Fix: git config --global user.email 'you@users.noreply.github.com'"
      ;;
  esac
fi

# --- collect text to scan ---
# local: staged adds
# upload: ranges from pre-push stdin (<local_ref> <local_sha> <remote_ref> <remote_sha>)
DIFF=""
if [ "$MODE" = "upload" ]; then
  # Read all stdin lines first (pre-push protocol).
  PUSH_LINES=$(cat || true)
  if [ -n "$PUSH_LINES" ]; then
    DIFF=$(
      printf '%s\n' "$PUSH_LINES" | while read -r local_ref local_sha remote_ref remote_sha; do
        [ -z "${local_sha:-}" ] && continue
        if [ "$local_sha" = "0000000000000000000000000000000000000000" ]; then
          continue # delete ref
        fi
        if [ -z "${remote_sha:-}" ] || [ "$remote_sha" = "0000000000000000000000000000000000000000" ]; then
          # new branch: diff against empty tree
          EMPTY=$(git hash-object -t tree /dev/null 2>/dev/null || echo "4b825dc642cb6eb9a060e54bf8d69288fbee4904")
          git diff -U0 --diff-filter=ACMR "${EMPTY}" "${local_sha}" 2>/dev/null || true
        else
          git diff -U0 --diff-filter=ACMR "${remote_sha}..${local_sha}" 2>/dev/null || true
        fi
      done
    )
  fi
  if [ -z "$DIFF" ]; then
    # Fallback: unpushed commits vs upstream
    UPSTREAM=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || true)
    if [ -n "$UPSTREAM" ]; then
      DIFF=$(git diff -U0 --diff-filter=ACMR "${UPSTREAM}..HEAD" 2>/dev/null || true)
    else
      DIFF=$(git diff -U0 --diff-filter=ACMR "origin/main..HEAD" 2>/dev/null || \
             git diff -U0 --diff-filter=ACMR "origin/master..HEAD" 2>/dev/null || true)
    fi
  fi
else
  DIFF=$(git diff --cached -U0 --diff-filter=ACMR 2>/dev/null || true)
fi

[ -z "$DIFF" ] && exit 0
ADDED=$(printf '%s\n' "$DIFF" | grep '^+' | grep -v '^+++' || true)
[ -z "$ADDED" ] && exit 0

# --- secret patterns ---
has_private_key=0
has_aws_key=0
has_gh_token=0
has_openai_key=0
has_slack=0
has_key_assign=0

printf '%s\n' "$ADDED" | grep -E '-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----' >/dev/null 2>&1 && has_private_key=1
# Strip well-known documentation placeholders before secret matching
# (AWS docs use AKIAIOSFODNN7EXAMPLE / ASIAIOSFODNN7EXAMPLE).
SCAN=$(printf '%s\n' "$ADDED" \
  | sed -e 's/AKIAIOSFODNN7EXAMPLE//g' -e 's/ASIAIOSFODNN7EXAMPLE//g' \
        -e 's/wJalrXUtnFEMI\/K7MDENG\/bPxRfiCYEXAMPLEKEY//g')

printf '%s\n' "$SCAN" | grep -E 'AKIA[0-9A-Z]{16}' >/dev/null 2>&1 && has_aws_key=1
printf '%s\n' "$SCAN" | grep -E 'ghp_[A-Za-z0-9]{20,}|gho_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}' >/dev/null 2>&1 && has_gh_token=1
printf '%s\n' "$SCAN" | grep -E 'sk-[A-Za-z0-9]{20,}|sk-proj-[A-Za-z0-9_\-]{20,}' >/dev/null 2>&1 && has_openai_key=1
printf '%s\n' "$SCAN" | grep -E 'xox[baprs]-[A-Za-z0-9-]{10,}' >/dev/null 2>&1 && has_slack=1
# Key-assignment: skip pure env *name* documentation; require a value-looking tail.
printf '%s\n' "$SCAN" | grep -Ei 'AWS_SECRET_ACCESS_KEY[[:space:]]*=[[:space:]]*["'"'"']?[A-Za-z0-9/+=]{16,}' >/dev/null 2>&1 && has_key_assign=1
printf '%s\n' "$SCAN" | grep -Ei '(OPENAI|ANTHROPIC|XAI)[_-]?API[_-]?KEY[[:space:]]*=[[:space:]]*["'"'"']?[A-Za-z0-9_\-]{16,}' >/dev/null 2>&1 && has_key_assign=1
printf '%s\n' "$SCAN" | grep -Ei 'api[_-]?key[[:space:]]*=[[:space:]]*["'"'"'][A-Za-z0-9_\-]{20,}' >/dev/null 2>&1 && has_key_assign=1

# Private keys: never allow in git history (local or upload).
if [ "$has_private_key" = "1" ]; then
  die "content contains PRIVATE KEY block — remove before commit/push"
fi

report_secret() {
  msg="$1"
  if [ "$MODE" = "upload" ]; then
    die "$msg — upload blocked. Remove secrets before push."
  else
    warn "$msg
  Local commit is allowed for fixture/config testing, but DO NOT push until cleaned.
  Override is not available for real secrets on push."
  fi
}

if [ "$has_aws_key" = "1" ]; then
  report_secret "looks like AWS access key id"
fi
if [ "$has_gh_token" = "1" ]; then
  report_secret "looks like GitHub token"
fi
if [ "$has_openai_key" = "1" ]; then
  report_secret "looks like OpenAI-style API key"
fi
if [ "$has_slack" = "1" ]; then
  report_secret "looks like Slack token"
fi
if [ "$has_key_assign" = "1" ]; then
  report_secret "looks like API key assignment / env secret name"
fi

# --- identity / home paths: warn local, hard-block upload on public remotes ---
REMOTE=$(git remote get-url origin 2>/dev/null || true)
case "$REMOTE" in
  *github.com*|*gitlab.com*|*bitbucket.org*|*gitcode.com*|*gitee.com*) PUBLIC_REMOTE=1 ;;
  *) PUBLIC_REMOTE=0 ;;
esac

if [ "$PUBLIC_REMOTE" = "1" ] && [ "${PRIVACY_HOOK_ALLOW_HOME_PATHS:-}" != "1" ]; then
  USERNAME=$(id -un 2>/dev/null || whoami)
  HOST=$(hostname -s 2>/dev/null || hostname 2>/dev/null || true)
  # Build sensitive path markers at runtime so this script source is not self-flagging.
  DOCS_MARKER="Documents/""GPT/"
  identity_hit=0

  # Ignore hits that only appear inside this guard script (self-description).
  SCAN_ID=$(printf '%s\n' "$ADDED" | grep -v 'privacy-guard' || true)

  if [ -n "$USERNAME" ] && printf '%s\n' "$SCAN_ID" | grep -E "/Users/${USERNAME}/|/home/${USERNAME}/" >/dev/null 2>&1; then
    identity_hit=1
    printf '%s\n' "$SCAN_ID" | grep -nE "/Users/${USERNAME}/|/home/${USERNAME}/" | head -6 >&2 || true
  fi
  if [ -n "$USERNAME" ] && [ "${#USERNAME}" -ge 3 ] && printf '%s\n' "$SCAN_ID" | grep -F "$USERNAME" >/dev/null 2>&1; then
    identity_hit=1
    printf '%s\n' "$SCAN_ID" | grep -nF "$USERNAME" | head -6 >&2 || true
  fi
  if printf '%s\n' "$SCAN_ID" | grep -F "$DOCS_MARKER" >/dev/null 2>&1; then
    identity_hit=1
    printf '%s\n' "$SCAN_ID" | grep -nF "$DOCS_MARKER" | head -6 >&2 || true
  fi
  if [ -n "$HOST" ] && [ "${#HOST}" -ge 4 ] && printf '%s\n' "$SCAN_ID" | grep -F "$HOST" >/dev/null 2>&1; then
    identity_hit=1
    printf '%s\n' "$SCAN_ID" | grep -nF "$HOST" | head -6 >&2 || true
  fi

  if [ "$identity_hit" = "1" ]; then
    if [ "$MODE" = "upload" ]; then
      die "public remote: personal path/username/hostname in content
  Clean before push. (override identity only: PRIVACY_HOOK_ALLOW_HOME_PATHS=1)"
    else
      warn "personal path/username/hostname in staged content
  OK for local testing; push will be blocked until cleaned.
  (upload override for identity only: PRIVACY_HOOK_ALLOW_HOME_PATHS=1)"
    fi
  fi
fi

exit 0
