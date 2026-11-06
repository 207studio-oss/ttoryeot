#!/bin/sh
# 공개 크레이트를 실제로 올리지 않고 crates.io에 올릴 수 있는지 본다.
# 아직 올라가지 않은 ttoryeot-krilla는 path로 잇는다.
set -eu
repo=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

(cd "$repo/vendor/krilla" && cargo publish --dry-run --allow-dirty)
cd "$repo"
cargo publish --dry-run --allow-dirty \
    --config 'patch.crates-io.ttoryeot-krilla.path="vendor/krilla"' \
    -p ttoryeot-hwp-core -p ttoryeot -p ttoryeot-alloc -p ttoryeot-cli -p ttoryeot-mcp
