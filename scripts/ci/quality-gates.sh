#!/usr/bin/env bash
set -u -o pipefail

# Kimix verification contract: run every independent gate, collect evidence,
# and fail only after all gates have had a chance to report their result.
# This keeps a single failure from hiding unrelated regressions.

summary_file="${GITHUB_STEP_SUMMARY:-}"
failed=0

if [[ -n "$summary_file" ]]; then
  {
    echo "## Kimix verification gates"
    echo
    echo "| Gate | Result | Duration |"
    echo "|---|---|---:|"
  } >> "$summary_file"
fi

run_gate() {
  local name="$1"
  shift
  local start end elapsed rc
  start=$(date +%s)
  echo "::group::${name}"
  echo "+ $*"
  "$@"
  rc=$?
  echo "::endgroup::"
  end=$(date +%s)
  elapsed=$((end - start))

  if [[ $rc -eq 0 ]]; then
    echo "PASS ${name} (${elapsed}s)"
    if [[ -n "$summary_file" ]]; then
      echo "| ${name} | PASS | ${elapsed}s |" >> "$summary_file"
    fi
  else
    echo "FAIL ${name} (${elapsed}s)"
    if [[ -n "$summary_file" ]]; then
      echo "| ${name} | FAIL | ${elapsed}s |" >> "$summary_file"
    fi
    failed=1
  fi
}

run_gate "cargo check" cargo check --workspace --all-targets
run_gate "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
run_gate "cargo fmt" cargo fmt --all --check
run_gate "cargo test" cargo test --workspace --all-targets

if command -v cargo-deny >/dev/null 2>&1; then
  run_gate "cargo deny" cargo deny check advisories bans sources licenses
else
  echo "cargo-deny not found; installing locked version"
  cargo install cargo-deny --locked
  if [[ $? -eq 0 ]]; then
    run_gate "cargo deny" cargo deny check advisories bans sources licenses
  else
    echo "FAIL cargo deny installation"
    if [[ -n "$summary_file" ]]; then
      echo "| cargo deny | FAIL | install failed |" >> "$summary_file"
    fi
    failed=1
  fi
fi

if [[ $failed -ne 0 ]]; then
  echo "Verification failed. See every gate above; no later gate was skipped."
  exit 1
fi

echo "All verification gates passed."
