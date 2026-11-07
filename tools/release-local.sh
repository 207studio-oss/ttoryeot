#!/bin/zsh
# GitHub Actions 없이 이 Mac에서 CLI·MCP 릴리스 받기 파일을 운영체제별로 만든다.
# 올리지 않고, 서명·공증하지 않는다. usage: tools/release-local.sh <version 예: v0.3.0> <out-dir>
# 필요: rustup target aarch64/x86_64-apple-darwin, x86_64-pc-windows-gnu, x86_64-unknown-linux-gnu,
#       mingw-w64(x86_64-w64-mingw32-gcc), zig + cargo-zigbuild(리눅스 glibc 2.17), makensis(Windows 설치 파일)
#
# 만드는 것(<ver>는 넘긴 버전 그대로):
#   ttoryeot-<ver>-windows-x64-setup.exe        Windows 설치 파일(NSIS, 사용자별, 서명 안 함)
#   ttoryeot-<ver>-windows-x64.zip              Windows 휴대용 압축 파일
#   ttoryeot-<ver>-macos-apple-silicon.tar.gz   aarch64-apple-darwin
#   ttoryeot-<ver>-macos-intel.tar.gz           x86_64-apple-darwin
#   ttoryeot-<ver>-linux-x64.tar.gz             x86_64-unknown-linux-gnu, glibc 2.17 이상
#   SHA256SUMS
set -eu
(( $# == 2 )) || { echo "usage: $0 <version 예: v0.3.0> <out-dir>" >&2; exit 2; }
root=${0:A:h:h}; version=$1; out=${2:A}
cd $root
[[ -z $(git status --porcelain) ]] || { echo "작업 트리가 깨끗하지 않습니다" >&2; exit 1; }
[[ $out != $root && $out != $root/* ]] || { echo "out-dir은 저장소 밖에 두세요" >&2; exit 1; }
command -v makensis >/dev/null || { echo "makensis가 없습니다(brew install makensis)" >&2; exit 1; }
# 설치 파일 버전 정보: v0.3.0-rc.1 -> 표시 0.3.0-rc.1, 숫자 0.3.0.0
display_version=${version#v}
numeric=${display_version%%[-+]*}
[[ $numeric =~ '^[0-9]+\.[0-9]+\.[0-9]+$' ]] || { echo "버전은 v<주>.<부>.<수>[-꼬리] 꼴이어야 합니다: $version" >&2; exit 1; }
sha=$(git rev-parse HEAD)
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}
# CARGO_TARGET_DIR와 cargo 설정의 target-dir을 모두 따르도록 cargo에게 묻는다.
target_dir=$(cargo metadata --format-version 1 --no-deps --locked | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
# 실행 파일에 이 Mac의 경로가 남지 않게 한다. 뒤에 적은 것이 앞선다(저장소·target이 $HOME 안이라 $HOME을 맨 앞에).
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$HOME=~ --remap-path-prefix=$root=/ttoryeot --remap-path-prefix=$target_dir=/ttoryeot/target"
host=$(rustc -vV | awk '/host/ {print $2}')
mkdir -p $out
rm -f $out/ttoryeot-*.tar.gz(N) $out/ttoryeot-*.zip(N) $out/ttoryeot-*-setup.exe(N) $out/SHA256SUMS

for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-gnu x86_64-unknown-linux-gnu; do
  case $target in
    aarch64-apple-darwin) os=macos-apple-silicon ;;
    x86_64-apple-darwin) os=macos-intel ;;
    x86_64-pc-windows-gnu) os=windows-x64 ;;
    x86_64-unknown-linux-gnu) os=linux-x64 ;;
  esac
  case $target in
    *linux*) cargo zigbuild --release --locked --target $target.2.17 -p ttoryeot-cli -p ttoryeot-mcp ;;
    *) cargo build --release --locked --target $target -p ttoryeot-cli -p ttoryeot-mcp ;;
  esac
  name=ttoryeot-$version-$os; suffix=""; [[ $target == *windows* ]] && suffix=.exe
  bin=$target_dir/$target/release
  if [[ $target == $host ]]; then
    $bin/ttoryeot$suffix --version; $bin/ttoryeot-mcp$suffix --version
  fi
  stage=$out/$name; rm -rf $stage; mkdir -p $stage/third-party
  cp $bin/ttoryeot$suffix $bin/ttoryeot-mcp$suffix $stage/
  cp README.md LICENSING.md LICENSE-AGPL-3.0 ttoryeot-alloc/LICENSE-MIT ttoryeot-alloc/LICENSE-APACHE THIRD_PARTY_NOTICES.md CHANGELOG.md $stage/
  cp third-party/dependencies.md $stage/third-party/
  printf 'version %s\ntarget %s\ncommit %s\nbuilt %s\nhost %s (local, unsigned)\n' \
    $version $target $sha "$(date -u +%FT%TZ)" "$(uname -sm)" > $stage/BUILD.txt
  case $target in
    *windows*)
      (cd $out && zip -qrX $name.zip $name)
      LC_ALL=en_US.UTF-8 makensis -INPUTCHARSET UTF8 -WX -V2 \
        -DVERSION=$display_version -DVERSION_NUM=$numeric.0 \
        -DSTAGE=$stage -DOUTFILE=$out/$name-setup.exe tools/windows/ttoryeot-setup.nsi
      ;;
    *) tar -C $out -czf $out/$name.tar.gz $name ;;
  esac
  rm -rf $stage
done
(cd $out && shasum -a 256 ttoryeot-$version-*.exe ttoryeot-$version-*.zip ttoryeot-$version-*.tar.gz > SHA256SUMS && cat SHA256SUMS)
