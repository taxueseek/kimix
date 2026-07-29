default: check

check:
    cargo check --workspace --all-targets
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all --check

gate:
    cargo check --workspace --all-targets
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all --check
    cargo test --workspace --all-targets
    cargo deny check advisories bans sources licenses

deps:
    cargo deny check advisories
    cargo udeps

quick: check-shell check-tui
check-shell:
    cargo check -p kimix-shell
check-tui:
    cargo check -p kimix-tui

test-jsonl:
    cargo test -p kimix-shell --lib -- jsonl

test-read-file:
    cargo test -p kimix-tools --lib -- read_file

test-fuzzy:
    cargo test -p kimix-workspace --lib -- fuzzy

release:
    cargo build --profile release-dist -p kimix-bin
