# 또렷 (Ttoryeot)

HWP/HWPX 문서를 한/글에서 연 것과 같은 모양으로 PDF로 그리는 Rust 엔진입니다.
문서를 읽는 파서, 쪽에 배치하는 렌더러, 한/글 기준 PDF와 비교하는 채점기로 이루어져 있습니다.
명령줄 도구와 MCP 서버로 PDF, PNG, 텍스트를 뽑고, HWPX의 글은 서식을 건드리지 않고 바꿉니다.
MCP 서버는 [또렷 데스크톱 앱](https://github.com/207studio-oss/ttoryeot-desktop/releases/latest)(무료)과 이어져,
사람이 앱에서 남긴 주석을 AI 에이전트가 읽고 문서를 고칩니다. 한/글 문서를 다루는 에이전트 스킬도 [skills](skills/)에 있습니다.

**Ttoryeot** is a Rust engine for Korean HWP/HWPX documents. It renders PDF and PNG
with the same layout as Hancom Office (한/글) and extracts plain text or markdown, through a
command-line tool (`ttoryeot`) and a stdio MCP server (`ttoryeot-mcp`). It replaces text in HWPX
files with all formatting untouched. The MCP server works with the free Ttoryeot desktop app: an
agent opens a document there, reads the comments a person leaves, edits the document and marks
them resolved. Agent skills for Hangul documents are in [skills](skills/).

## 무엇을 하나요

- HWP 5.x와 HWPX를 하나의 문서 모델로 읽습니다.
- 글·표·그림·도형·수식·머리말·꼬리말·각주·단을 쪽에 배치합니다.
- 쪽 크기·여백·테두리·쪽 번호·머리말 높이 등 한/글이 저장한 배치 값을 그대로 씁니다.
- 암호를 아는 HWP5(EncryptVersion 0·4)·HWPX 문서와 배포용 문서를 엽니다.
- 문서를 PDF, 쪽 PNG, 평문, markdown으로 내보냅니다.
- AI 도구가 문서를 읽도록 MCP 서버를 함께 제공합니다.
- 쪽마다 문단·표·칸·개체·머리말 같은 구성 요소의 이름과 자리를 물을 수 있습니다.
- HWPX의 글을 서식을 그대로 두고 바꿉니다. 줄 바꿈을 넣고 지우며, 빈 문단에 글을 넣습니다.
- 사람이 또렷 데스크톱 앱에서 남긴 주석을 AI 에이전트가 읽고, 고친 뒤 해결로 표시합니다.
- 한/글이 내보낸 기준 PDF와 화소·글자·잉크를 자동으로 비교해 수치를 냅니다.

원본 파일은 고치지 않습니다. 글 바꾸기는 새 파일에 쓰고, 사람과 함께 고칠 때만(`in_place`) 고치기 전 사본을 남기고 그 문서를 고칩니다.

## 받는 법

또렷은 창이 뜨는 앱이 아닙니다. 터미널에서 쓰는 명령줄 도구 `ttoryeot`와, Claude Desktop·Claude Code
같은 AI 도구가 부르는 MCP 서버 `ttoryeot-mcp` 두 실행 파일입니다. 설치한 뒤에는 명령 프롬프트,
PowerShell, 터미널에서 `ttoryeot --help`로 시작합니다.

미리 빌드한 파일은 [Releases](https://github.com/207studio-oss/ttoryeot/releases)에서 운영체제에 맞는
것 하나를 받습니다. 모든 파일에 두 실행 파일이 함께 들어 있고, `SHA256SUMS`로 내려받은 파일을 확인할 수 있습니다.

| 운영체제 | 받을 파일 |
|---|---|
| Windows 10·11 (64비트) | `ttoryeot-<버전>-windows-x64-setup.exe` (설치 파일, 권장) 또는 `ttoryeot-<버전>-windows-x64.zip` (압축만 푸는 휴대용) |
| macOS, Apple 실리콘(M1 이후) | `ttoryeot-<버전>-macos-apple-silicon.tar.gz` |
| macOS, Intel | `ttoryeot-<버전>-macos-intel.tar.gz` |
| Linux x86_64 (glibc 2.17 이상) | `ttoryeot-<버전>-linux-x64.tar.gz` |

### Windows

**설치 파일(권장).** `windows-x64-setup.exe`를 실행합니다. 관리자 권한 없이 지금 사용자에게만
`%LOCALAPPDATA%\Programs\ttoryeot`에 설치하고, 그 폴더를 사용자 `Path`에 더합니다. 설치가 끝나면
**새로** 연 명령 프롬프트나 PowerShell에서 `ttoryeot --help`가 됩니다. 시작 메뉴의 `또렷 (ttoryeot)`
폴더에는 README와 제거 바로 가기만 있습니다. 지울 때는 설정 › 앱 › 설치된 앱에서 `또렷 (ttoryeot)`을
제거합니다. 제거하면 설치가 더한 `Path` 항목 하나만 빠지고 다른 항목은 그대로 둡니다.

**휴대용 zip.** 설치하지 않고 쓰려면 `windows-x64.zip`을 원하는 폴더에 풀고 그 폴더에서 `.\ttoryeot.exe`를
실행합니다. 어디서나 부르려면 그 폴더를 직접 `Path`에 넣으세요.

설치 파일과 실행 파일은 Windows 코드 서명을 하지 않았습니다. 그래서 처음 실행할 때 Microsoft Defender
SmartScreen이 "Windows의 PC 보호" 창을 띄울 수 있습니다. 받은 파일의 `SHA256SUMS` 값을 확인했다면
**추가 정보**를 누른 뒤 **실행**을 누릅니다. PowerShell에서는 다음과 같이 해시를 봅니다.

```powershell
Get-FileHash .\ttoryeot-v0.3.1-windows-x64-setup.exe -Algorithm SHA256
```

### macOS

Mac의 칩에 맞는 파일을 받습니다. Apple 메뉴 › **이 Mac에 관하여**에서 "칩"이 Apple M1·M2 같은
이름이면 Apple 실리콘(`macos-apple-silicon`), "프로세서"가 Intel이면 `macos-intel`입니다. 터미널에서
`uname -m`이 `arm64`이면 Apple 실리콘, `x86_64`이면 Intel입니다.

macOS 실행 파일은 Developer ID로 서명하고 Apple 공증을 받았습니다. 풀어서 `PATH`에 있는 폴더로 옮깁니다.

```bash
tar -xzf ttoryeot-v0.3.1-macos-apple-silicon.tar.gz
sudo install -m 755 ttoryeot-v0.3.1-macos-apple-silicon/ttoryeot* /usr/local/bin/
ttoryeot --help
```

### Linux

x86_64용 하나이고 glibc 2.17 이상(대부분의 배포판)에서 돕니다. Arm Linux용은 아직 없으니
아래처럼 소스에서 설치하세요.

```bash
tar -xzf ttoryeot-v0.3.1-linux-x64.tar.gz
sudo install -m 755 ttoryeot-v0.3.1-linux-x64/ttoryeot* /usr/local/bin/
```

### 소스에서 설치

`ttoryeot`(명령줄)와 `ttoryeot-mcp`(MCP 서버)는 별개 실행 파일입니다. 필요한 것만 설치하세요.

```bash
cargo install --git https://github.com/207studio-oss/ttoryeot --tag v0.3.1 --locked ttoryeot-cli
cargo install --git https://github.com/207studio-oss/ttoryeot --tag v0.3.1 --locked ttoryeot-mcp
```

Rust 1.99 이상이 필요합니다. 저장소에서 직접 빌드하려면 다음과 같이 합니다.

```bash
cargo build --release -p ttoryeot-cli -p ttoryeot-mcp
# target/release/ttoryeot, target/release/ttoryeot-mcp
```

## 명령줄 사용법

| 명령 | 하는 일 |
|---|---|
| `ttoryeot render` | 문서를 PDF로 그립니다 |
| `ttoryeot png` | 쪽을 PNG로 그립니다. `--pages`, `--dpi` |
| `ttoryeot text` | 글자를 평문이나 markdown으로 꺼냅니다. `--format`, `--pages` |
| `ttoryeot info` | 쪽 수, 문서 속성, 사용 글꼴, 암호화 여부를 보여 줍니다 |
| `ttoryeot fonts` | 찾은 글꼴과 한/글 글꼴 설치 상태를 확인합니다 |

```bash
ttoryeot render 문서.hwp                    # 문서.pdf
ttoryeot render 문서.hwpx -o 결과/문서.pdf
ttoryeot png 문서.hwp --pages 1-3,5 --dpi 200 -o 쪽그림/
ttoryeot text 문서.hwpx --format markdown --pages 2- -o 문서.md
ttoryeot info 문서.hwp
```

쪽 범위는 `1-3,5,7-`처럼 씁니다. 모든 명령은 `--json`을 받아 결과를 JSON 한 줄로 출력합니다.

```bash
ttoryeot render 문서.hwp --json
# {"input":"문서.hwp","output":"문서.pdf","pages":26,"bytes":184320,"warnings":0}
```

`text`의 평문은 쪽에 그려진 줄 그대로이고, markdown은 문서 구조(제목, 목록, 표)를 따릅니다.

### 암호 문서

암호는 환경 변수로만 받습니다. 인자로 받으면 셸 기록과 프로세스 목록에 남기 때문입니다.

```bash
read -rs TTORYEOT_PASSWORD          # 화면에 남기지 않고 입력
export TTORYEOT_PASSWORD
ttoryeot render 암호문서.hwp --password-env TTORYEOT_PASSWORD
```

### 글꼴

한/글 문서는 보통 함초롬체를 가리킵니다. 그 글꼴이 없으면 비슷한 글꼴로 대신 그리므로
쪽 나눔과 글자 위치가 달라질 수 있습니다. 또렷은 한컴 글꼴을 함께 배포하지 않습니다.
먼저 이 컴퓨터에 어떤 글꼴이 있는지 확인하세요.

```bash
ttoryeot fonts
```

한/글과 같은 모양이 필요하면 정식으로 받은 글꼴을 설치하거나 `--font`로 폴더나 파일을 지정합니다.

```bash
ttoryeot render 문서.hwp --font ~/Library/Fonts/함초롬체
```

자세한 내용은 [글꼴 문서](https://github.com/207studio-oss/ttoryeot/blob/main/docs/fonts.md)를 보세요.

## MCP 서버

`ttoryeot-mcp`는 표준 입출력으로 통신하는 [MCP](https://modelcontextprotocol.io) 서버입니다.
Claude Desktop, Claude Code 같은 MCP 클라이언트가 문서를 읽을 수 있게 합니다.

| 도구 | 인자 | 돌려주는 것 |
|---|---|---|
| `read_document_info` | `path` | 형식, 쪽 수, 메타데이터, 사용 글꼴, 암호화 여부 |
| `extract_text` | `path`, `format`(`plain`, `markdown`), `pages` | 글자 |
| `render_pdf` | `path`, `output`, `pages` | 새로 만든 PDF의 경로와 크기 |
| `replace_text` | `path`, `output` 또는 `in_place`, `search`, `replacement`, `occurrence`(선택) | 쓴 HWPX의 경로와 바꾼 번째, 바로 고쳤으면 백업 경로 |
| `render_page_png` | `path`, `page`, `dpi` | PNG 이미지 |
| `list_fonts` | `document`(선택) | 찾은 글꼴과 문서가 쓰는 글꼴의 상태 |
| `open_in_viewer` | `path`, `page`(선택) | 또렷 데스크톱에 문서를 띄움(꺼져 있으면 켜고, 켜져 있으면 새 탭). 주석 파일 경로 |
| `list_annotations` | `path`, `include_resolved`, `images` | 또렷 데스크톱에서 남긴 주석: 번호, 쪽, 고른 자리, 대상 요소의 경로와 글, 댓글, 바꿀 글, 해결 여부, 문서가 바뀌었는지, 고른 부분 PNG |
| `resolve_annotation` | `path`, `id`, `resolution`, `resolved` | 주석을 해결로 표시(또는 해제)하고 남은 주석 수 |
| `find_components` | `path`, `page`, `contains`, `x`, `y` | 쪽의 문단·표·칸·그림 등의 종류, 이름, 경로, 글, 자리(pt) |

클라이언트 설정 파일에 다음을 넣습니다. Claude Desktop은 `claude_desktop_config.json`입니다.

```json
{
  "mcpServers": {
    "ttoryeot": {
      "command": "ttoryeot-mcp",
      "args": ["--font", "/path/to/fonts"],
      "env": { "TTORYEOT_PASSWORD": "암호 문서를 열 때만 넣습니다" }
    }
  }
}
```

Claude Code에서는 다음과 같이 등록합니다.

```bash
claude mcp add ttoryeot -- ttoryeot-mcp --font /path/to/fonts
```

- `--font`와 `TTORYEOT_PASSWORD`는 선택입니다. 도구 인자로는 암호를 받지 않습니다.
- `render_pdf`는 확장자가 `.pdf`인 새 파일만 만들고, 있는 파일을 덮어쓰지 않습니다.
- `replace_text`는 HWPX만 받습니다. 보통은 `output`에 확장자가 `.hwpx`인 새 파일만 만들고, 있는 파일을
  덮어쓰지 않습니다. `in_place`를 `true`로 주면 그 문서를 바로 고치고, 고치기 전 사본을 주석 폴더의
  `backups`에 문서마다 최근 20개까지 남깁니다. 글자 모양, 문단 모양, 줄 간격은 그대로 두고 글만 바꾸며,
  글자 수가 달라지면 그 문단의 줄은 다시 나눕니다. `search`가 여러 군데에 있으면 전부 바꾸지 않고 번째와
  앞뒤 글을 알려 주므로, `occurrence`(1부터)를 넣어 다시 부릅니다. 여러 서식에 걸친 글, 암호·변경 추적
  문서는 이유를 적고 거절합니다.
- 사람과 함께 고치기: `open_in_viewer`로 문서를 또렷 데스크톱에 띄우면 사람이 요소나 영역에 주석을 남깁니다.
  에이전트는 `list_annotations`로 읽고 `replace_text`(`in_place`)로 고친 뒤 `resolve_annotation`으로 해결
  표시합니다. 또렷은 고친 문서와 해결 표시를 바로 보여 줍니다. 앱 위치는 macOS는 번들 ID
  `dev.207studio.ttoryeot`, Windows는 `%LOCALAPPDATA%\Programs\또렷\또렷.exe`, 그 밖에는 `PATH`의
  `ttoryeot-viewer`로 찾고, `TTORYEOT_VIEWER`에 실행 파일 경로를 주면 그것을 띄웁니다.
- 서버는 클라이언트가 건넨 경로의 파일을 서버 권한으로 읽습니다. 믿는 클라이언트에만 연결하세요.

## 에이전트 스킬

[`skills/`](skills/)에 [Agent Skills](https://agentskills.io) 형식의 스킬이 있습니다. Claude Code, Codex 같은 에이전트가
읽습니다.

| 스킬 | 쓰는 때 |
|---|---|
| [`hangul-documents`](skills/hangul-documents/SKILL.md) | 한/글 HWP·HWPX를 읽고, 고치고, 한/글과 같은 모양으로 그릴 때. 파일 구조, 단위, 줄 배치, 한/글 12.30 배치 규칙, 글꼴, 서식을 지키며 고치는 법 |
| [`hangul-cli`](skills/hangul-cli/SKILL.md) | `ttoryeot` CLI로 문서를 검사하고 PDF·PNG·텍스트를 만들 때 |

```bash
# Claude Code
mkdir -p ~/.claude/skills && cp -R skills/hangul-documents skills/hangul-cli ~/.claude/skills/
# Codex
mkdir -p ~/.agents/skills && cp -R skills/hangul-documents skills/hangul-cli ~/.agents/skills/
```

## 라이브러리로 쓰기

```rust
use std::path::PathBuf;
use ttoryeot::fonts::FontBook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("문서.hwp")?;
    let mut fonts = FontBook::scan(&[PathBuf::from("글꼴")]);
    let output = ttoryeot::to_pdf(&bytes, None, &mut fonts)?;
    std::fs::write("문서.pdf", &output.pdf)?;
    println!("{}쪽, 경고 {}개", output.pages, output.warnings.len());
    Ok(())
}
```

암호 문서는 두 번째 인자로 암호를 넘깁니다: `to_pdf(&bytes, Some("암호"), &mut fonts)`.
PDF 바이트만 필요하면 `render_pdf`를 씁니다.

macOS 앱에서 문서를 거듭 연다면 `ttoryeot_alloc::Allocator`를 전역 할당자로 등록하세요.
시스템 할당자가 놓인 큰 블록을 쥐고 있어 최대 메모리가 호출할수록 늘어나는 일을 막습니다.
`ttoryeot-ffi`와 CLI는 이미 등록해 두었고, macOS 밖에서는 시스템 할당자와 같습니다.
글꼴 폴더는 호출하는 쪽이 정합니다. 운영체제 기본 폴더는 CLI가 찾습니다.

iOS 기기·시뮬레이터용 XCFramework와 Swift 호출 방법은 [iOS 문서](https://github.com/207studio-oss/ttoryeot/blob/main/ttoryeot-ffi/ios/README.md)에 있습니다.

문서를 먼저 검사만 하려면 `hwp_core::parse`가 문서 모델과 경고를 돌려줍니다.
파서는 암호·형식 오류를 `hwp_core::Fatal`로 구분해 알립니다.

## 정확도

아래 수치는 이전 코드에서 비공개 말뭉치를 한/글 기준 PDF와 비교한 값입니다. 글꼴 대체 변경과의
통합 뒤에는 다시 재지 않았으며, 환경과 문서 묶음이 다른 두 줄을 직접 비교하면 안 됩니다.

| 환경 | 문서·쪽 | 쪽 수 일치 | 화소 차이 % ↓ | 잉크 F1 ↑ | 글자 F1 ↑ |
|---|---:|---:|---:|---:|---:|
| Windows, 한컴 2024 글꼴 | 276개·1,594쪽 | 274/276 | 2.4456 | 0.9288 | 0.9800 |
| macOS, 한컴 글꼴 범위 | 254개·1,527쪽 | 249/254 | 3.5129 | 0.8977 | 0.9769 |

- 화소 차이는 72dpi로 래스터화해 잉크 밝기 192 미만을 비교한 값입니다.
- 글자 F1은 NFKC 뒤 공백·제어 문자·U+FFFD를 뺀 문자 다중집합 비교입니다.
- 각 수치는 문서별 값의 산술 평균입니다. 후보 생성 실패는 0개였습니다.
- 마지막 Windows 측정의 쪽 수 차이는 2/276입니다. macOS 5/254는 이 수치를 낸 수정 전에 잰 값입니다.
  남은 둘은 한/글이 다른 글꼴로 줄을 다시 나눈 글상자와, 메모를 함께 찍어 줄인 기준 PDF입니다.

측정 방법과 한계는 [정확도 문서](https://github.com/207studio-oss/ttoryeot/blob/main/docs/accuracy.md)에, 재현 명령은 채점기 도움말에 적었습니다.
**이 수치는 출시 품질 보증이 아니며, 말뭉치가 비공개라 밖에서 그대로 재현할 수 없습니다.**
기준 문서와 비교하려면 한/글에서 직접 PDF로 내보낸 뒤 채점기를 쓰세요.

## 지원 범위

| 입력 | 읽기 | PDF | 비고 |
|---|---|---|---|
| HWP 5.0.x | 지원 | 지원 | 배포용, EncryptVersion 0(DES)·4(AES) 암호 포함 |
| HWPX | 지원 | 지원 | 암호 HWPX 포함 |
| HWP 3.0 | 미지원 | — | 구별된 오류로 알림 |
| HWPML(.hml) | 미지원 | — | 구별된 오류로 알림 |
| HWP5 EncryptVersion 1~3 | 미지원 | — | 지원하지 않는 버전과 지원 범위를 오류에 적음 |
| 문서에 내장된 글꼴 | 지원 | 지원 | HWP5 `BinData`·HWPX `isEmbedded`, 메모리 색인, `fsType` 제한 적용 |

- 읽기, 변환, 텍스트 추출, PDF·PNG 렌더가 중심입니다. 편집은 HWPX의 짧은 글 바꾸기 하나뿐이고,
  서식·표·도형 편집과 이진 HWP 쓰기는 하지 않습니다.
- macOS에서 주로 검증했습니다. Windows·Linux용 실행 파일은 교차 빌드로 만들며, 두 플랫폼 모두
  한/글과의 화면 일치는 아직 확인하지 않았습니다.
- Windows에서 확인한 범위와 남은 위험은 [Windows 문서](https://github.com/207studio-oss/ttoryeot/blob/main/docs/windows.md)에 적었습니다.
- 글꼴이 다르면 같은 문서도 다르게 보입니다. 한/글과 같은 결과가 필요하면 같은 글꼴을 넣으세요.

## 저장소 구조

| 경로 | 크레이트 | 하는 일 |
|---|---|---|
| `hwp-core/` | `ttoryeot-hwp-core` | 엔진. HWP5·HWPX 파서, 공통 문서 모델 |
| `ttoryeot/` | `ttoryeot` | 엔진. 쪽 배치, 글꼴 색인·대체, krilla PDF 쓰기 |
| `ttoryeot-cli/` | `ttoryeot-cli` | 플랫폼. `ttoryeot` 명령줄 도구, `hwp-dump` |
| `ttoryeot-mcp/` | `ttoryeot-mcp` | 플랫폼. stdio MCP 서버 |
| `ttoryeot-ffi/` | `ttoryeot-ffi` | 플랫폼. C ABI, Swift 다리, iOS XCFramework |
| `ttoryeot-score/` | `ttoryeot-score` | 플랫폼. 기준 PDF와 후보 PDF 비교 채점기 |
| `ttoryeot-alloc/` | `ttoryeot-alloc` | 공용. CLI와 FFI가 함께 쓰는 전역 할당자 |
| `assets/fonts/` | — | 시험용 Liberation Sans (OFL) |

전체 구조와 데이터 흐름은 [아키텍처 문서](https://github.com/207studio-oss/ttoryeot/blob/main/ARCHITECTURE.md)에 있습니다.

## 개발

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

합성 시험은 별도 자료 없이 돕니다. 말뭉치가 필요한 골든 시험은 `HWP_GOLDEN_DIR`을
지정했을 때만 돌고, 없으면 건너뜁니다.

```bash
HWP_GOLDEN_DIR=/path/to/documents cargo test -p ttoryeot-hwp-core
```

채점기는 기준 PDF 목록과 후보를 받습니다. 자세한 사용법은 `cargo run -p ttoryeot-score --`
또는 [정확도 문서](https://github.com/207studio-oss/ttoryeot/blob/main/docs/accuracy.md)를 보세요.

성능 기준선은 저장소 안의 합성 문서로 잽니다.

```powershell
pwsh -File tools\bench.ps1
```

이 PC에서 잰 쪽/초와 최대 메모리, 측정 방법과 한계는 [성능 문서](https://github.com/207studio-oss/ttoryeot/blob/main/docs/performance.md)에 있습니다.

기여 규칙은 [CONTRIBUTING.md](https://github.com/207studio-oss/ttoryeot/blob/main/CONTRIBUTING.md)에 있습니다.

## 라이선스

- **엔진 라이브러리**(`ttoryeot-hwp-core`, `ttoryeot`, `ttoryeot-ffi`): [AGPL-3.0-only](https://github.com/207studio-oss/ttoryeot/blob/main/LICENSE-AGPL-3.0) 또는 [또렷 상용 라이선스](https://github.com/207studio-oss/ttoryeot/blob/main/COMMERCIAL-LICENSE.md) 중 하나를 골라 쓸 수 있습니다. 엔진을 자기 제품에 넣으면서 소스를 공개하지 않으려면 상용 라이선스가 필요합니다. 개발자 계정당 1회 30,000원이고, 제품 매출이 1억 원을 넘으면 넘는 금액의 3%를 냅니다. 일부 조항은 아직 확정 전입니다.
- **CLI·MCP**(`ttoryeot-cli`, `ttoryeot-mcp`)와 채점기(`ttoryeot-score`): AGPL-3.0-only입니다. 도구로 쓰는 것은 무료입니다.
- **할당자**(`ttoryeot-alloc`)와 **krilla**(`vendor/krilla`): MIT OR Apache-2.0입니다.

크레이트별 구성은 [LICENSING.md](https://github.com/207studio-oss/ttoryeot/blob/main/LICENSING.md)에 있습니다.
옮겨 쓴 제3자 코드와 글꼴 고지는 [THIRD_PARTY_NOTICES.md](https://github.com/207studio-oss/ttoryeot/blob/main/THIRD_PARTY_NOTICES.md)에,
의존성 라이선스 목록은 [third-party/dependencies.md](https://github.com/207studio-oss/ttoryeot/blob/main/third-party/dependencies.md)에 있습니다.

한글과컴퓨터, 한/글, 함초롬체는 해당 권리자의 상표이거나 저작물입니다. 또렷은 한글과컴퓨터와
관계가 없고, 이 저장소는 한컴 글꼴이나 한/글 프로그램을 포함하지 않습니다.
