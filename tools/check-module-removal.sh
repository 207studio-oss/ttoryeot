#!/bin/sh
# 플랫폼 모듈을 지워도 엔진과 나머지가 빌드되는지 복사본에서 본다.
set -eu
repo=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/ttoryeot-removal.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$work/target"}
export CARGO_TARGET_DIR

check_without() {
    rm -rf "$work/tree"
    mkdir "$work/tree"
    (cd "$repo" && tar -cf - --exclude ./target --exclude ./dist --exclude ./.git --exclude node_modules .) | tar -xf - -C "$work/tree"
    for module in "$@"; do
        rm -rf "${work:?}/tree/$module"
    done
    printf '== %s 없이\n' "$*"
    (cd "$work/tree" && cargo check --workspace --all-targets)
    "$work/tree/tools/check-boundaries.py"
    "$work/tree/tools/test-check-boundaries.sh" >/dev/null
}

check_without ttoryeot-cli
check_without ttoryeot-ffi
check_without ttoryeot-mcp
check_without ttoryeot-score
check_without ttoryeot-cli ttoryeot-ffi ttoryeot-mcp ttoryeot-alloc
check_without ttoryeot-cli ttoryeot-ffi ttoryeot-mcp ttoryeot-score ttoryeot-alloc
