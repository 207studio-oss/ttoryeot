# Windows

## 빌드

Windows x86_64 실행 파일은 macOS·Linux에서 mingw-w64로 교차 빌드하고, 같은 스크립트가 NSIS로 설치
파일도 만듭니다(`tools/release-local.sh`).
실행 파일은 Windows 10 이상에 기본으로 있는 DLL만 씁니다. Windows에서 바로 빌드하려면 아래 명령을 씁니다.

## 이 PC에서 확인한 것

2026-10-27에 실제 Windows PC에서 확인했습니다. WSL이 아니라 Windows에서 바로 돌렸습니다.

| 항목 | 값 |
|---|---|
| 운영체제 | Windows 11 Pro 10.0.26200, x86_64 |
| CPU | AMD Ryzen 7 9800X3D |
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) |
| cargo | 1.99.1 (5f94df478 2026-08-27) |

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p ttoryeot-cli
```

| 검사 | 결과 |
|---|---|
| 서식 | 통과 |
| 정적 검사 | 통과, 경고 없음 |
| 시험 | 320개 통과, 0개 실패 |
| 릴리스 빌드 | 통과 (`ttoryeot.exe`) |

시험 하나가 아니라 명령줄 도구도 직접 돌렸습니다.

```powershell
cargo run -q -p ttoryeot-cli -- render hwp-core\tests\fixtures\synthetic.hwpx `
  -o "$env:TEMP\ttoryeot-win-check.pdf" --font assets\fonts --json
```

결과는 `11쪽, 4,592바이트, 경고 0개`였고 파일은 `%PDF-1.7`로 시작했습니다.

기본 글꼴 경로도 확인했습니다. 이 PC에서는 `%WINDIR%\Fonts`만 있고
`%LOCALAPPDATA%\Microsoft\Windows\Fonts`는 없었습니다.

```powershell
cargo run -q -p ttoryeot-cli -- fonts --json
# paths: C:\WINDOWS\Fonts, faces: 248
# 함초롬바탕·함초롬돋움·바탕·돋움·굴림·맑은 고딕 있음, 함초롬돋움 LVT 없음
```

## 확인하지 않은 것

- 실제 말뭉치 문서의 화면 일치(쪽 나눔·글자 위치). README의 Windows 수치는 다른 실행
  환경에서 잰 값이며, 이 PC에서 다시 측정하지 않았습니다.
- 한/글 2024와 같은 글꼴 조합에서의 인쇄 결과와 문서 내장 글꼴.
- Windows 코드 서명. 설치 파일(`windows-x64-setup.exe`)과 휴대용 zip은 서명하지 않아 SmartScreen이
  경고합니다. 설치 파일은 macOS에서 NSIS로 교차로 만들며, 설치·`Path`·제거 동작은 [릴리스 문서](releasing.md)에 적었습니다.

빌드와 시험이 통과해도 한/글과 같은 모양을 보장하지는 않습니다. 글꼴이 다르면 쪽 나눔이
달라질 수 있으므로, 화면 일치가 중요한 문서는 [글꼴 문서](fonts.md)를 보고 같은 글꼴을
넣은 뒤 직접 비교하세요.

## Windows에서 직접 돌리기

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p ttoryeot-cli
.\target\release\ttoryeot.exe fonts
.\target\release\ttoryeot.exe render 문서.hwp --font C:\Windows\Fonts --json
```

- 저장소의 텍스트 파일은 `.gitattributes`가 LF로 맞춥니다. 글꼴·HWPX·PDF는 이진 파일로
  다룹니다.
- 경로 구분자는 Rust의 `std::path`가 처리하므로 `--font`에 `C:\Windows\Fonts`를 그대로
  넘겨도 됩니다.
- `cargo`의 대상 폴더를 바꿨다면 실행 파일 경로도 그 설정을 따릅니다.
