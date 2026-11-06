#!/bin/sh
# 경계 검사가 위반을 실제로 잡는지 본다. 위반마다 트리를 새로 복사해 하나만 넣는다.
set -eu
repo=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/ttoryeot-boundaries.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
failed=0

fresh() {
    rm -rf "$work/tree"
    mkdir "$work/tree"
    (cd "$repo" && tar -cf - --exclude ./target --exclude ./dist --exclude ./.git .) | tar -xf - -C "$work/tree"
}

fake_module() {
    rm -rf "${work:?}/tree/$1"
    mkdir -p "$work/tree/$1/src"
    printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n%s\n' "$1" "${2:-}" > "$work/tree/$1/Cargo.toml"
    : > "$work/tree/$1/src/lib.rs"
}

expect_clean() {
    fresh
    if ! python3 "$repo/tools/check-boundaries.py" "$work/tree" >/dev/null 2>&1; then
        printf '실패: 손대지 않은 트리를 거부했습니다\n' >&2
        failed=1
    fi
}

expect_violation() {
    label=$1
    pattern=$2
    if output=$(python3 "$repo/tools/check-boundaries.py" "$work/tree" 2>&1); then
        printf '실패: 잡지 못했습니다. %s\n' "$label" >&2
        failed=1
    elif ! printf '%s\n' "$output" | grep -q "경계 위반.*$pattern"; then
        printf '실패: 다른 이유로 거부했습니다. %s\n%s\n' "$label" "$output" >&2
        failed=1
    else
        printf '잡음: %s\n' "$label"
    fi
}

toml_after_dependencies() {
    python3 - "$1" "$2" <<'P'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
assert "[dependencies]\n" in text
path.write_text(text.replace("[dependencies]\n", "[dependencies]\n" + sys.argv[2] + "\n", 1))
P
}

expect_clean

fresh
fake_module ttoryeot-fake-a
toml_after_dependencies "$work/tree/hwp-core/Cargo.toml" 'ttoryeot-fake-a = { path = "../ttoryeot-fake-a" }'
expect_violation '엔진이 플랫폼 모듈에 의존' '플랫폼 모듈에 의존'

fresh
fake_module ttoryeot-alloc
toml_after_dependencies "$work/tree/ttoryeot/Cargo.toml" 'ttoryeot-alloc = { path = "../ttoryeot-alloc" }'
expect_violation '엔진이 할당자에 의존' '플랫폼 모듈에 의존'

fresh
toml_after_dependencies "$work/tree/ttoryeot/Cargo.toml" 'libc = "0.2"'
expect_violation '엔진이 libc에 의존' 'libc'

fresh
printf '\n[target.'"'"'cfg(windows)'"'"'.dependencies]\nserde = "1"\n' >> "$work/tree/ttoryeot/Cargo.toml"
expect_violation '엔진의 대상별 의존' '대상별 의존'

fresh
printf '\n[target.'"'"'cfg(unix)'"'"'.dev-dependencies]\nserde = "1"\n' >> "$work/tree/hwp-core/Cargo.toml"
expect_violation '엔진의 대상별 개발 의존' '대상별 의존'

fresh
printf '\n#[cfg(target_os = "macos")]\npub fn only_mac() {}\n' >> "$work/tree/ttoryeot/src/lib.rs"
expect_violation '엔진의 target_os cfg' 'cfg'

fresh
printf '\npub fn is_windows() -> bool { cfg!(windows) }\n' >> "$work/tree/hwp-core/src/lib.rs"
expect_violation '엔진의 cfg! 매크로' 'cfg'

fresh
printf '\n#[cfg_attr(unix, derive(Debug))]\npub struct OnlyUnix;\n' >> "$work/tree/ttoryeot/src/lib.rs"
expect_violation '엔진의 cfg_attr' 'cfg'

fresh
printf '\n#[cfg(all(test, windows))]\nmod windows_test {}\n' >> "$work/tree/ttoryeot/src/lib.rs"
expect_violation '엔진의 cfg(all(test, windows))' 'cfg'

fresh
printf '\n#[unsafe(no_mangle)]\npub extern "C" fn exported() {}\n' >> "$work/tree/hwp-core/src/lib.rs"
expect_violation '엔진의 내보내는 심볼' '심볼\|C ABI'

fresh
printf '\npub fn read() -> Option<String> { std::env::var("X").ok() }\n' >> "$work/tree/ttoryeot/src/lib.rs"
expect_violation '엔진의 환경 변수' '환경 변수'

fresh
printf '\npub const FONTS: &str = "/System/Library/Fonts";\n' >> "$work/tree/ttoryeot/src/fonts.rs"
expect_violation '엔진의 운영체제 경로' '운영체제 경로'

fresh
printf '\n[[bin]]\nname = "engine-tool"\npath = "src/lib.rs"\n' >> "$work/tree/ttoryeot/Cargo.toml"
expect_violation '엔진의 실행 파일' '실행 파일'

fresh
python3 - "$work/tree/hwp-core/Cargo.toml" <<'P'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
assert "[lib]\n" in text
path.write_text(text.replace("[lib]\n", '[lib]\ncrate-type = ["staticlib", "rlib"]\n', 1))
P
expect_violation '엔진의 staticlib' '네이티브 라이브러리'

fresh
sed -i.bak 's/#!\[forbid(unsafe_code)\]//' "$work/tree/hwp-core/src/lib.rs"
expect_violation '엔진의 unsafe 금지 제거' 'forbid'

fresh
fake_module ttoryeot-fake-a
fake_module ttoryeot-fake-b 'ttoryeot-fake-a = { path = "../ttoryeot-fake-a" }'
expect_violation '새 모듈도 플랫폼 모듈끼리 의존하지 못함' '플랫폼 모듈끼리'

fresh
fake_module ttoryeot-alloc 'ttoryeot = { path = "../ttoryeot" }'
expect_violation '공용 크레이트가 엔진에 의존' '공용 크레이트'

exit "$failed"
