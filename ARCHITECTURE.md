# 구조

## 한눈에

```
HWP/HWPX 바이트
      │
      ▼
 hwp-core          파일 형식 감지 → CFB/HWPX 해제 → 암호 해제 → 구역·문단·표·도형 파싱
      │            → 공통 문서 모델(Document) + 경고(Warning)
      ▼
 ttoryeot          글꼴 색인 → 쪽 나눔 → 글자·표·그림 배치 → 장면(Page[]) → krilla PDF
      │
      ▼
   PDF 바이트
```

채점기는 이 출력을 한/글이 내보낸 기준 PDF와 같은 경로로 래스터화해 비교합니다.

## 크레이트

| 크레이트 | 종류 | 책임 |
|---|---|---|
| `hwp-core` (패키지 `ttoryeot-hwp-core`) | 엔진 라이브러리 | 입력 해석, 문서 모델, 응답 프레임 |
| `ttoryeot` | 엔진 라이브러리 | 배치, 글꼴, PDF 쓰기 |
| `ttoryeot-cli` | 실행 파일 | 명령줄 인터페이스, 운영체제 글꼴 폴더, `hwp-dump` |
| `ttoryeot-mcp` | 실행 파일 | stdio MCP 서버, 운영체제 글꼴 폴더 |
| `ttoryeot-ffi` | `staticlib` + 라이브러리 | C ABI, Swift 다리, iOS XCFramework |
| `ttoryeot-score` | 실행 파일 | 기준/후보 비교, 수치 집계 |
| `ttoryeot-alloc` | 라이브러리 | 플랫폼 모듈이 함께 쓰는 전역 할당자 |

## 모듈 경계

엔진은 `hwp-core`와 `ttoryeot`입니다. 나머지 크레이트는 모두 플랫폼 모듈입니다. 의존은 플랫폼에서
엔진으로 한 방향만 흐릅니다.

```text
ttoryeot-cli    ┐
ttoryeot-mcp    │
ttoryeot-ffi    ├──▶ ttoryeot ──▶ hwp-core
ttoryeot-score  ┘

ttoryeot-cli, ttoryeot-mcp, ttoryeot-ffi ──▶ ttoryeot-alloc
```

규칙은 다음과 같습니다.

- 엔진은 플랫폼 모듈에 의존하지 않습니다. 엔진 크레이트와 `vendor/`만 작업공간 의존으로 둡니다.
- 엔진은 대상별 의존을 두지 않고, `cfg`는 `test`만 씁니다. `unsafe`, 실행 파일, `staticlib`,
  빌드 스크립트, 내보내는 C 심볼, 전역 할당자, 환경 변수, 운영체제 글꼴 경로도 두지 않습니다.
- 플랫폼 모듈은 엔진과 `ttoryeot-alloc`에만 의존합니다. 플랫폼 모듈끼리는 의존하지 않습니다.
- `ttoryeot-alloc`은 작업공간의 다른 크레이트에 의존하지 않습니다. 엔진은 이 크레이트를 쓰지 않습니다.

`tools/check-boundaries.py`가 이 규칙을 검사합니다. `tools/test-check-boundaries.sh`는 위반을
일부러 넣어 검사가 실제로 실패하는지 봅니다. `tools/check-module-removal.sh`는 모듈을 하나씩 지운
트리가 빌드되는지 봅니다. `ttoryeot*` 디렉터리는 작업공간에 자동으로 들어오므로 새 모듈은 위 표를
고치지 않아도 같은 규칙을 받습니다.

| 모듈 | 디렉터리 | 지우는 법 |
|---|---|---|
| CLI | `ttoryeot-cli/` | 디렉터리를 지우고 `tools/release-local.sh`에서 CLI 빌드를 뺍니다 |
| MCP | `ttoryeot-mcp/` | 디렉터리를 지우고 `tools/release-local.sh`에서 MCP 빌드를 뺍니다 |
| FFI·iOS | `ttoryeot-ffi/` | 디렉터리를 지웁니다 |
| 채점기 | `ttoryeot-score/` | 디렉터리를 지웁니다 |
| 할당자 | `ttoryeot-alloc/` | CLI, MCP, FFI의 등록을 뺀 뒤 디렉터리를 지웁니다 |

작업공간 의존 선언은 `[workspace.dependencies]`에 남아도 쓰는 크레이트가 없으면 빌드에 영향을 주지
않습니다. `Cargo.lock`은 다음 빌드 때 저절로 줄어듭니다.

`hayro-syntax`는 `flate2`의 `zlib-rs` 기능을 켭니다. PDF 쓰기는 `flate2`를 거치지 않고 `miniz_oxide`를
직접 부르므로, `hayro`를 함께 빌드해도 PDF 바이트는 그대로입니다.

데스크톱·모바일 앱은 이 저장소에 두지 않습니다. 엔진을 쓰는 앱은 같은 경계 규칙을 따르는
플랫폼 모듈로 만듭니다.

## 파서

1. **형식 감지** — 파일 머리와 컨테이너 구조로 HWP5·HWPX를 가릅니다.
2. **컨테이너** — HWP5는 비표준 CFB, HWPX는 ZIP+XML입니다. HWP5는 압축 구역을 풉니다.
3. **암호** — HWP5 EncryptVersion 0(DES)·4(AES)와 암호 HWPX를 풉니다. 배포용 문서는 ViewText
   구역을 읽습니다. FileHeader와 HWPX manifest는 평문이므로 지원하지 않는 방식은 암호를
   요구하기 전에 알립니다. 암호가 없거나 틀리면 `Fatal`로 구분하고, footer CRC 불일치나
   DES 블록 경계가 아닌 스트림처럼 무결성 검사에 실패한 손상은 `WrongPassword`가 아니라
   `CorruptDocument`와 경고로 다룹니다. HWP5 1~3처럼 지원하지 않는 버전은
   `UnsupportedEncryption` 종류와 상세 문구에 버전·지원 범위를 적습니다.
4. **모델** — DocInfo(글자·문단·테두리·번호·글꼴)와 구역별 본문을 읽어 `Document`로 만듭니다.
   HWP5 태그와 HWPX 요소를 같은 의미로 맞춥니다.
5. **경고** — 읽었지만 해석하지 못한 요소는 `Warning`으로 모아 둡니다. 문서 전체를 버리지
   않습니다.

파서는 16MB 스택을 가진 스레드에서 돌고, 패닉은 `Fatal::Internal`로 바꿉니다.

## 렌더러

1. **글꼴** — `FontBook`이 글꼴 파일의 이름표만 먼저 읽고, 실제로 쓰는 얼굴만 메모리에 올립니다.
   문서에 내장된 HWP5 `BinData`·HWPX `isEmbedded` 글꼴은 문서별 메모리 색인으로 붙이며
   디스크에 쓰지 않습니다. 문서가 적은 글꼴 이름은 후보 전체에서 내장 얼굴 → 설치된 얼굴
   차례로 찾고, 후보에서 모두 실패할 때만 `data/font-subst.tsv`의 대체 규칙을 같은 차례로
   씁니다. 이름을 모두 찾지 못하면 포함 가능한 색인 얼굴을 순서대로 시도합니다. OS/2
   `fsType`이 PDF 글꼴 자료 포함을 막는 얼굴은 배치에도 쓰지 않고, 글꼴이 없을
   때와 같은 대체 사슬로 그립니다. 윤곽선도 글꼴 자료이므로 포함 금지 얼굴에서는 그리지
   않습니다. `0x0004`·`0x0008`은 `0x0000`처럼 부분 집합으로 넣고, `0x0100`(부분 집합 금지)과
   `0x0200`(비트맵만 허가)은 전체 포함·내장 비트맵을 지원하지 않으므로 대체 사슬로 넘깁니다.
2. **쪽 나눔** — 한/글이 저장한 쪽 크기·여백·머리말 높이·줄 배치를 기준으로 쪽을 나눕니다.
   저장된 배치가 있으면 다시 계산하지 않습니다.
3. **배치** — 문단·표·그림·도형·수식·각주·머리말·꼬리말·단을 문서 순서대로 놓습니다.
   표가 쪽 아래로 넘치면 다음 쪽 조각을 만들고, 제목 행을 반복합니다.
4. **장면** — `Page`마다 그릴 것(`Glyphs`, `Fill`, `Stroke`, `Image`, `Clip`, `Transform`)을
   순서대로 담습니다. 좌표 단위는 pt입니다.
5. **PDF** — krilla로 장면을 씁니다. 글꼴은 쓰인 글자만 부분 집합으로 넣습니다.

## 채점기

`ttoryeot-score`는 세 가지 일을 합니다.

- `opens` — 문서를 열 수 있는지, 어떤 경고가 나오는지 셉니다.
- `score` — 기준 PDF와 후보 PDF를 72dpi로 래스터화하고 글자를 뽑아 화소·잉크·글자 지표를 냅니다.
- `need-refs` — 기준 PDF가 없는 문서 목록을 만듭니다.

기준과 후보는 같은 hayro 경로로 처리합니다. 후보 PDF는 메모리에서만 다루고, 결과에는 문서
이름·원문·그림을 쓰지 않습니다.

## 시험

- 단위 시험은 문서 모델과 배치 계산을 직접 확인합니다.
- 통합 시험은 합성 HWPX(`hwp-core/tests/fixtures/synthetic.hwpx`)와 시험용 글꼴로 PDF까지 냅니다.
- 골든 시험은 `HWP_GOLDEN_DIR`이 있을 때만 돕니다. 말뭉치는 저장소에 없습니다.
- 버그 수정에는 수정 전에 실패하는 시험이 붙습니다.

## 정하지 않은 것

- 편집·저장·주석
- HWP 3.0과 HWPML
- 앱 UI
