#!/usr/bin/env bash
# 추적되는 파일에 개인 경로, 비공개 이름, 비공개 문서와 글꼴이 들어오지 않게 막는다.
# 공개 저장소에서 다시 새지 않게 하는 검사이고, 히스토리는 보지 않는다.
# 비공개 이름 목록은 그 자체가 비공개라 저장소 밖 파일에서 읽는다(한 줄에 확장 정규식 하나, # 줄과 빈 줄은 무시).
# 경로는 TTORYEOT_PRIVATE_PATTERNS로 바꿀 수 있고, 기본값은 ~/.config/ttoryeot/private-patterns.txt다.
set -u
cd "$(git rev-parse --show-toplevel)"
status=0
excludes=(':!Cargo.lock' ':!third-party/dependencies.md' ':!vendor')

scan() {
  git grep -niIE "$1" -- . "${excludes[@]}"
  case $? in
    0) echo "error: $2" >&2; status=1 ;;
    1) ;;
    *) echo "error: git grep이 패턴을 읽지 못했습니다(잘못된 정규식일 수 있음)" >&2; status=1 ;;
  esac
}

scan '/Users/[A-Za-z0-9_]|/home/[a-z][a-z0-9_-]+/|C:\\Users\\' "개인 경로로 보이는 문자열이 있습니다"

names_file=${TTORYEOT_PRIVATE_PATTERNS:-${HOME:-}/.config/ttoryeot/private-patterns.txt}
names_checked=0
if [ -f "$names_file" ]; then
  names=$(grep -vE '^[[:space:]]*(#|$)' "$names_file" | tr -d '\r' | paste -sd'|' -)
  if [ -z "$names" ]; then
    echo "error: $names_file 에 패턴이 없습니다" >&2
    status=1
  else
    scan "$names" "비공개 이름으로 보이는 문자열이 있습니다"
    names_checked=1
  fi
elif [ -n "${TTORYEOT_PRIVATE_PATTERNS:-}" ]; then
  echo "error: TTORYEOT_PRIVATE_PATTERNS가 가리키는 $names_file 파일이 없습니다" >&2
  status=1
else
  echo "check-private: 비공개 패턴 파일($names_file)이 없어 비공개 이름 검사를 건너뜁니다. 개인 경로와 파일 검사는 계속합니다." >&2
fi

allowed='^(assets/fonts/LiberationSans-Regular\.ttf|hwp-core/tests/fixtures/synthetic[^/]*)$'
binaries=$(git ls-files | grep -iE '\.(hwp|hwpx|hwt|hwtx|pdf|ttf|otf|ttc|docx|xlsx|pptx)$' | grep -vE "$allowed" || true)
if [ -n "$binaries" ]; then
  printf '%s\n' "$binaries"
  echo "error: 허용되지 않은 문서나 글꼴 파일이 있습니다" >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  if [ "$names_checked" -eq 1 ]; then echo "check-private: ok"; else echo "check-private: ok (비공개 이름 검사 건너뜀)"; fi
fi
exit "$status"
