#!/bin/sh
# 로컬에서 CI와 같은 검사를 한다.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked --workspace
python3 tools/check-boundaries.py
tools/test-check-boundaries.sh
