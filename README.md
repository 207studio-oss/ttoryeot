# 또렷 (Ttoryeot)

HWP/HWPX 문서를 한/글에서 연 것과 같은 모양으로 PDF로 그리는 Rust 엔진입니다.
문서를 읽는 파서, 쪽에 배치하는 렌더러, 한/글 기준 PDF와 비교하는 채점기로 이루어져 있습니다.
명령줄 도구와 MCP 서버로 PDF, PNG, 텍스트를 뽑습니다. 편집기는 아닙니다. 다만 HWPX의 짧은 글
한 군데는 서식을 건드리지 않고 바꿔 새 HWPX로 저장할 수 있습니다.

**Ttoryeot** is a Rust engine for Korean HWP/HWPX documents. It renders PDF and PNG
with the same layout as Hancom Office (한/글) and extracts plain text or markdown, through a
command-line tool (`ttoryeot`) and a stdio MCP server (`ttoryeot-mcp`). It is not an editor.
The one edit it makes is replacing a short piece of text in an HWPX and saving a new HWPX with
all formatting untouched. It never modifies the source file.

## 무엇을 하나요

- HWP 5.x와 HWPX를 하나의 문서 모델로 읽습니다.
- 글·표·그림·도형·수식·머리말·꼬리말·각주·단을 쪽에 배치합니다.
- 쪽 크기·여백·테두리·쪽 번호·머리말 높이 등 한/글이 저장한 배치 값을 그대로 씁니다.
- 암호를 아는 HWP5(EncryptVersion 0·4)·HWPX 문서와 배포용 문서를 엽니다.
- 문서를 PDF, 쪽 PNG, 평문, markdown으로 내보냅니다.
- AI 도구가 문서를 읽도록 MCP 서버를 함께 제공합니다.
- 쪽마다 문단·표·칸·개체·머리말 같은 구성 요소의 이름과 자리를 물을 수 있습니다.
- HWPX의 짧은 글 한 군데를 서식을 그대로 두고 바꿔 새 HWPX로 저장합니다.
- 한/글이 내보낸 기준 PDF와 화소·글자·잉크를 자동으로 비교해 수치를 냅니다.

원본 파일은 고치지 않습니다. 글 바꾸기도 늘 새 파일에 씁니다.

## 설치

`ttoryeot`(명령줄)와 `ttoryeot-mcp`(MCP 서버)는 별개 실행 파일입니다. 필요한 것만 설치하세요.

```bash
cargo install ttoryeot-cli --locked
cargo install ttoryeot-mcp --locked
```

Rust 1.99 이상이 필요합니다.
미리 빌드한 실행 파일은 [Releases](https://github.com/207studio-oss/ttoryeot/releases)에서 받습니다.
macOS(arm64, x86_64), Linux(x86_64), Windows(x86_64)용 압축 파일에 두 실행 파일이 함께 들어 있습니다.
`SHA256SUMS`로 내려받은 파일을 확인할 수 있습니다.

저장소에서 직접 빌드하려면 다음과 같이 합니다.

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
| `replace_text` | `path`, `output`, `search`, `replacement`, `occurrence`(선택) | 새로 만든 HWPX의 경로와 바꾼 번째 |
| `render_page_png` | `path`, `page`, `dpi` | PNG 이미지 |
| `list_fonts` | `document`(선택) | 찾은 글꼴과 문서가 쓰는 글꼴의 상태 |

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
- `replace_text`는 HWPX만 받고 확장자가 `.hwpx`인 새 파일만 만듭니다. 글자 모양, 문단 모양,
  줄 간격은 그대로 두고 글만 바꿉니다. `search`가 여러 군데에 있으면 전부 바꾸지 않고 번째와
  앞뒤 글을 알려 주므로, `occurrence`(1부터)를 넣어 다시 부릅니다. 여러 서식에 걸친 글,
  줄 배치를 정확히 옮길 수 없는 글, 암호·변경 추적 문서는 이유를 적고 거절합니다.
- 서버는 클라이언트가 건넨 경로의 파일을 서버 권한으로 읽습니다. 믿는 클라이언트에만 연결하세요.

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
