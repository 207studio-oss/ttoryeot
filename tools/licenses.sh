#!/bin/sh
# 의존 크레이트의 라이선스 묶음을 다시 만든다.
#   cargo install cargo-about --locked --features cli
set -eu
cd "$(dirname "$0")/.."
mkdir -p third-party
cargo about generate --config about.toml --output-file third-party/dependencies.md about.hbs
echo "third-party/dependencies.md 를 다시 만들었습니다."
