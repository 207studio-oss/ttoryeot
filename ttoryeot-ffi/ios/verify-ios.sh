#!/bin/sh
set -eu

if [ "$#" -ne 2 ] && [ "$#" -ne 4 ]; then
    printf '사용법: %s <XCFramework 경로> <켜진 iPad 시뮬레이터 ID> [공개 plain.hwp 한글글꼴]\n' "$0" >&2
    exit 2
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
framework=$(CDPATH= cd -- "$1" && pwd)
simulator=$2
work=$(mktemp -d "${TMPDIR:-/tmp}/ttoryeot-verify.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
sim_sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)
device_sdk=$(xcrun --sdk iphoneos --show-sdk-path)
sim_headers="$framework/ios-arm64-simulator/Headers"
device_headers="$framework/ios-arm64/Headers"

swiftc -swift-version 6 -strict-concurrency=complete -warnings-as-errors \
    -emit-module -module-name TtoryeotSwift -module-cache-path "$work/module-cache" \
    -I "$sim_headers" "$root/ttoryeot-ffi/swift/Ttoryeot.swift" \
    -emit-module-path "$work/TtoryeotSwift.swiftmodule"
swiftc -swift-version 6 -strict-concurrency=complete -warnings-as-errors -typecheck \
    -module-cache-path "$work/module-cache" -I "$work" -I "$sim_headers" \
    "$root/ttoryeot-ffi/ios/concurrency-smoke.swift"

SDKROOT="$sim_sdk" swiftc -target arm64-apple-ios15.0-simulator -sdk "$sim_sdk" \
    -module-cache-path "$work/sim-cache" -I "$sim_headers" \
    "$root/ttoryeot-ffi/swift/Ttoryeot.swift" "$root/ttoryeot-ffi/ios/renderer-smoke.swift" \
    "$framework/ios-arm64-simulator/libttoryeot_ffi.a" \
    -framework PDFKit -framework Security -framework CoreFoundation -o "$work/smoke-sim"
SDKROOT="$device_sdk" swiftc -target arm64-apple-ios15.0 -sdk "$device_sdk" \
    -module-cache-path "$work/device-cache" -I "$device_headers" \
    "$root/ttoryeot-ffi/swift/Ttoryeot.swift" "$root/ttoryeot-ffi/ios/renderer-smoke.swift" \
    "$framework/ios-arm64/libttoryeot_ffi.a" \
    -framework PDFKit -framework Security -framework CoreFoundation -o "$work/smoke-device"

xcrun simctl spawn "$simulator" "$work/smoke-sim" fixtures \
    "$root/hwp-core/tests/fixtures/synthetic.hwpx" \
    "$root/hwp-core/tests/fixtures/synthetic-password-v0.hwp" \
    "$root/hwp-core/tests/fixtures/synthetic-unsupported-encryption-v3.hwp" \
    "$root/assets/fonts/LiberationSans-Regular.ttf" "$work/output"

if [ "$#" -eq 4 ]; then
    xcrun simctl spawn "$simulator" "$work/smoke-sim" render \
        "$3" "$4" "$work/public.pdf" '' '안내문' 24
fi
