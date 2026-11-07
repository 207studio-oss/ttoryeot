# 릴리스

> 이 저장소는 GitHub Actions를 쓰지 않습니다. 검사와 묶음은 모두 로컬에서 만듭니다.

## 준비

1. `Cargo.toml`의 `[workspace.package] version`을 올립니다. 크레이트는 이 값을 함께 씁니다.
2. `CHANGELOG.md`에 바뀐 점과 검증 결과를 적습니다.
3. `tools/check.sh`(서식, clippy, 시험, 릴리스 빌드, 경계 검사)와 `tools/check-publish.sh`를 통과시킵니다.
4. 커밋하고 `main`에 올립니다.

## 받기 파일 만들기

필요한 도구는 rustup 대상 `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-gnu`,
`x86_64-unknown-linux-gnu`, mingw-w64(`brew install mingw-w64`), zig와 cargo-zigbuild, NSIS
(`brew install makensis`)입니다.

```bash
tools/release-local.sh v0.3.0 ../ttoryeot-dist
```

출력 폴더는 저장소 밖이어야 하고, 작업 트리가 깨끗해야 합니다. 스크립트는 아무것도 올리거나
서명하지 않으며 다음 파일을 만듭니다.

| 파일 | 대상 |
|---|---|
| `ttoryeot-v0.3.0-windows-x64-setup.exe` | Windows 설치 파일(NSIS, `x86_64-pc-windows-gnu`) |
| `ttoryeot-v0.3.0-windows-x64.zip` | Windows 휴대용 |
| `ttoryeot-v0.3.0-macos-apple-silicon.tar.gz` | `aarch64-apple-darwin` |
| `ttoryeot-v0.3.0-macos-intel.tar.gz` | `x86_64-apple-darwin` |
| `ttoryeot-v0.3.0-linux-x64.tar.gz` | `x86_64-unknown-linux-gnu`, cargo-zigbuild로 glibc 2.17 |
| `SHA256SUMS` | 위 다섯 파일의 SHA-256 |

압축 파일 안의 폴더 이름은 압축 파일 이름과 같습니다(`ttoryeot-v0.3.0-macos-intel/` 등). 묶음에는
실행 파일(`ttoryeot`, `ttoryeot-mcp`), README, LICENSING.md, LICENSE-AGPL-3.0,
`ttoryeot-alloc/LICENSE-MIT`·`ttoryeot-alloc/LICENSE-APACHE`(할당자), THIRD_PARTY_NOTICES, CHANGELOG, 의존 크레이트
라이선스 묶음(krilla 포함, `third-party/dependencies.md`), 빌드 정보(`BUILD.txt`)가 들어갑니다.

스크립트는 호출한 쪽의 `RUSTFLAGS` 뒤에 `--remap-path-prefix`를 붙여 홈 폴더, 저장소, cargo target
폴더 경로가 실행 파일에 남지 않게 합니다. 올리기 전에 모든 받기 파일을 풀어 개인 경로가 없는지 확인합니다.

### Windows 설치 파일

`tools/windows/ttoryeot-setup.nsi`를 makensis로 컴파일합니다. 버전은 스크립트가 넘깁니다
(`-DVERSION=0.3.0 -DVERSION_NUM=0.3.0.0`). 설치 파일은 다음과 같이 동작합니다.

- 관리자 권한 없이 사용자별로 `%LOCALAPPDATA%\Programs\ttoryeot`에 설치합니다. 화면은 한국어이고,
  Windows 표시 언어가 영어면 영어입니다. AGPL-3.0 라이선스 쪽을 보여 줍니다.
- 사용자 `Path`에 설치 폴더를 더하고, 제거할 때 그 항목만 뺍니다. NSIS 문자열은 1024자로 잘리므로
  `Path`를 NSIS 안에서 읽고 쓰지 않습니다. `tools/windows/path.ps1`을 PowerShell로 돌려
  `HKCU\Environment`의 값을 펼치지 않은 채 읽고 값 종류(`REG_EXPAND_SZ`)도 그대로 두며, 다른 항목은
  한 글자도 바꾸지 않습니다. 그 뒤 `WM_SETTINGCHANGE`를 알립니다. 설치 전에 이미 있던 항목은 제거할 때 빼지 않습니다.
- `HKCU`의 앱 목록에 `또렷 (ttoryeot) <버전>`(게시자 207studio)으로 등록하고, 시작 메뉴에는 README와
  제거 바로 가기만 만듭니다. 제거는 설치한 파일만 지웁니다.
- NSIS에 함께 들어 있는 플러그인(nsExec, System)만 씁니다. 저장소에 플러그인 DLL을 넣지 않습니다.
- Windows 코드 서명 인증서가 없어 설치 파일과 실행 파일 모두 서명하지 않습니다. 사용자는 SmartScreen
  경고를 보게 되며, README에 넘기는 방법을 적었습니다.

설치 파일은 macOS에서 교차로 만들기 때문에, 바뀐 `.nsi`나 `path.ps1`은 올리기 전에 Windows PC에서
설치·`ttoryeot --help`·제거와 `Path` 전후 값을 한 번 확인합니다.

### macOS 서명과 공증

서명과 공증은 손으로 합니다. 스크립트가 만든 두 macOS 묶음을 풀어 `ttoryeot`, `ttoryeot-mcp`를
Developer ID로 서명(`codesign --options runtime --timestamp`)하고 `notarytool`로 공증한 뒤, 같은 폴더
이름과 파일 이름으로 다시 묶고 `SHA256SUMS`를 다시 만듭니다. 서명하지 않은 macOS 묶음은 올리지 않습니다.

## 태그와 GitHub 릴리스

```bash
git tag -a v0.3.0 -m "또렷 0.3.0" && git push origin v0.3.0
gh release create v0.3.0 ../ttoryeot-dist/ttoryeot-v0.3.0-* ../ttoryeot-dist/SHA256SUMS --title "또렷 v0.3.0" --verify-tag
```

릴리스 설명의 받는 법에는 운영체제별로 받을 파일을 적습니다(README의 받는 법과 같게).

이미 받은 사람이 있으면 같은 버전을 덮지 말고 패치 버전을 올립니다.

## crates.io

`hwp-core` 이름은 다른 사람이 이미 쓰고 있어 패키지 이름은 `ttoryeot-hwp-core`입니다. 라이브러리
이름은 그대로 `hwp_core`입니다. 패치한 krilla도 `ttoryeot-krilla`로 올립니다. 의존 순서대로 올립니다.

```bash
(cd vendor/krilla && cargo publish)
cargo publish -p ttoryeot-hwp-core
cargo publish -p ttoryeot
cargo publish -p ttoryeot-alloc
cargo publish -p ttoryeot-cli
cargo publish -p ttoryeot-mcp
```

`tools/check-publish.sh`는 올리지 않고 같은 순서를 `--dry-run`으로 확인합니다. 의존 크레이트 라이선스가
바뀌었다면 먼저 `tools/licenses.sh`로 다시 만듭니다.
