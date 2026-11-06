#!/bin/zsh
# GitHub Actions 없이 이 Mac에서 CLI·MCP 릴리스 묶음을 만든다.
# 올리지 않는다. usage: tools/release-local.sh <version 예: v0.3.0> <out-dir>
# 필요: rustup target aarch64/x86_64-apple-darwin, x86_64-pc-windows-gnu, x86_64-unknown-linux-gnu,
#       mingw-w64(x86_64-w64-mingw32-gcc), zig + cargo-zigbuild(리눅스 glibc 2.17)
set -eu
root=${0:A:h:h}; version=$1; out=${2:A}
cd $root
[[ -z $(git status --porcelain) ]] || { echo "작업 트리가 깨끗하지 않습니다" >&2; exit 1; }
sha=$(git rev-parse HEAD)
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}
target_dir=${CARGO_TARGET_DIR:-$root/target}
mkdir -p $out; rm -f $out/*.tar.gz(N) $out/*.zip(N) $out/SHA256SUMS
for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-gnu x86_64-unknown-linux-gnu; do
  case $target in
    *linux*) cargo zigbuild --release --locked --target $target.2.17 -p ttoryeot-cli -p ttoryeot-mcp ;;
    *) cargo build --release --locked --target $target -p ttoryeot-cli -p ttoryeot-mcp ;;
  esac
  name=ttoryeot-$version-$target; suffix=""; [[ $target == *windows* ]] && suffix=.exe
  bin=$target_dir/$target/release
  if [[ $target == $(rustc -vV | awk '/host/ {print $2}') ]]; then
    $bin/ttoryeot$suffix --version; $bin/ttoryeot-mcp$suffix --version
  fi
  stage=$out/$name; rm -rf $stage; mkdir -p $stage/third-party
  cp $bin/ttoryeot$suffix $bin/ttoryeot-mcp$suffix $stage/
  cp README.md LICENSING.md LICENSE-AGPL-3.0 ttoryeot-alloc/LICENSE-MIT ttoryeot-alloc/LICENSE-APACHE THIRD_PARTY_NOTICES.md CHANGELOG.md $stage/
  cp third-party/dependencies.md $stage/third-party/
  printf 'commit %s\nbuilt %s\nhost %s (local, unsigned)\n' $sha "$(date -u +%FT%TZ)" "$(uname -sm)" > $stage/BUILD.txt
  case $target in
    *windows*) (cd $out && zip -qr $name.zip $name) ;;
    *) tar -C $out -czf $out/$name.tar.gz $name ;;
  esac
  rm -rf $stage
done
(cd $out && shasum -a 256 *.tar.gz *.zip > SHA256SUMS && cat SHA256SUMS)
