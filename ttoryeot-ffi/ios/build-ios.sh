#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
output=${1:-"$root/dist/Ttoryeot.xcframework"}
mkdir -p "$(dirname "$output")"
output=$(CDPATH= cd -- "$(dirname "$output")" && pwd)/$(basename "$output")
if [ -e "$output" ]; then
    printf '출력 경로가 이미 있습니다: %s\n' "$output" >&2
    exit 1
fi

CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/target"}
export CARGO_TARGET_DIR
IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET:-15.0}
export IPHONEOS_DEPLOYMENT_TARGET
# Xcode 도구와 LLVM 버전이 달라 프로젝트 크레이트의 LTO 비트코드는 빼고 만든다.
CARGO_PROFILE_RELEASE_LTO=false
export CARGO_PROFILE_RELEASE_LTO
jobs=${CARGO_BUILD_JOBS:-2}
headers=$(mktemp -d "${TMPDIR:-/tmp}/ttoryeot-headers.XXXXXX")
trap 'rm -rf "$headers"' EXIT HUP INT TERM
cp "$root/ttoryeot-ffi/include/ttoryeot.h" "$headers/"
cp "$root/ttoryeot-ffi/include/module.modulemap" "$headers/"
cp "$root/ttoryeot-ffi/include/hwp_core.h" "$headers/"
cd "$root"
cargo build --release --locked -p ttoryeot-ffi --target aarch64-apple-ios -j "$jobs"
cargo build --release --locked -p ttoryeot-ffi --target aarch64-apple-ios-sim -j "$jobs"

xcodebuild -create-xcframework \
    -library "$CARGO_TARGET_DIR/aarch64-apple-ios/release/libttoryeot_ffi.a" \
    -headers "$headers" \
    -library "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/release/libttoryeot_ffi.a" \
    -headers "$headers" \
    -output "$output"
