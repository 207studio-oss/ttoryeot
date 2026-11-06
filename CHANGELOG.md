# 변경 기록

## 아직 배포하지 않음

### 라이선스

- 엔진 라이브러리(`ttoryeot-hwp-core`, `ttoryeot`, `ttoryeot-ffi`)는 AGPL-3.0-only 또는 또렷 상용
  라이선스 가운데 골라 쓸 수 있습니다. 상용 라이선스의 일부 조항은 아직 확정 전입니다.
- CLI(`ttoryeot-cli`), MCP 서버(`ttoryeot-mcp`), 채점기(`ttoryeot-score`)는 AGPL-3.0-only입니다.
  할당자와 krilla는 MIT OR Apache-2.0입니다.
- 저장소 주소를 `https://github.com/207studio-oss/ttoryeot`로 정했습니다.

### 오픈소스 공개 준비

- 명령줄에 `png`, `text`, `info`를 더했습니다. 모든 명령이 `--json`을 받습니다.
- stdio MCP 서버 `ttoryeot-mcp`를 더했습니다. 도구는 `read_document_info`, `extract_text`,
  `render_pdf`, `render_page_png`, `list_fonts`입니다.
- crates.io용 이름을 정했습니다. 패키지 이름은 `ttoryeot-hwp-core`, `ttoryeot-krilla`이고,
  라이브러리 이름 `hwp_core`는 그대로입니다.
- 릴리스 압축 파일에 `ttoryeot-mcp`를 넣고, Linux와 Windows 대상을 더했습니다.
- `tools/perf/mem.sh`, `cold.sh`는 문서별 글꼴 목록을 `FONTMAP`으로, `same-output.sh`는
  채점 묶음 폴더를 `SCORE`로 받습니다.
- 개인 경로와 내부 문서 ID를 막는 `tools/check-private.sh`를 더했습니다.

### 모듈 경계

- 엔진(`hwp-core`, `ttoryeot`)에서 플랫폼 코드를 뺐습니다. C ABI와 Swift 다리, iOS 스크립트는
  `ttoryeot-ffi`로, `hwp-dump`와 운영체제 글꼴 폴더는 `ttoryeot-cli`로, 전역 할당자는 새
  `ttoryeot-alloc`으로 옮겼습니다. C ABI와 XCFramework의 머리글, 심볼은 그대로입니다.
- 라이브러리 사용자가 바뀌는 곳은 세 가지입니다. `ttoryeot::fonts::default_paths`와
  `hwp_core::ffi`, `ttoryeot::allocator`가 없어졌습니다. `tools/build-ios.sh`와
  `tools/verify-ios.sh`는 `ttoryeot-ffi/ios/`로 옮겼습니다.
- 경계 검사(`tools/check-boundaries.py`)와 모듈별 CI를 넣었습니다.

### hwp-core

- HWPX 구역 XML을 문단 단위로 읽고 버려, 본문이 큰 문서의 최대 메모리를 줄였습니다.
- HWP5 EncryptVersion 0(DES) 암호 문서를 엽니다. UTF-16LE 암호 키 유도, DES-ECB, 스트림
  footer CRC 검사, 보호된 DocInfo의 256바이트 앞머리 제거, DEFLATE 뒤의 inner compact
  trailer(CRC32·길이·0 정렬) 검사를 포함합니다.
- 지원하지 않는 HWP5 EncryptVersion(1~3 등)은 `UnsupportedEncryption` 종류를 유지하면서
  버전과 지원 범위(0 DES, 4 AES)를 상세 문구에 적습니다. FileHeader가 평문이므로 암호를
  요구하기 전에 검사하며, HWPX manifest도 지원하지 않는 방식이면 암호를 요구하기 전에
  알립니다.
- 암호 없음·오답·지원하지 않는 방식은 계속 서로 다른 `Fatal`로 구분합니다. 스트림 footer
  CRC 불일치와 DES 블록 경계가 아닌 잘린 스트림은 `WrongPassword`가 아니라 손상으로
  다룹니다. DocInfo가 아닌 스트림은 `StreamChecksumMismatch` 경고를 남기고 읽을 수 있는
  부분을 계속 읽으며, 읽을 수 있는 구역이 없으면 `CorruptDocument`로 알립니다.
- `Fatal::UnsupportedEncryption`은 단위 variant로 남기고 상세 문구는 새
  `Fatal::UnsupportedEncryptionDetail`에 담습니다. 손상은 새 `Fatal::CorruptDocument`로
  구분합니다.
- HWP5 `BinData`와 HWPX `isEmbedded` 글꼴을 그림이 아니라 글꼴 얼굴로 읽습니다.
  HWPX manifest의 `media-type`도 글꼴 판별에 씁니다.
- HWPX manifest에 이미 읽은 `BinData` 항목을 이름이 다른 항목으로 한 번 더 넣지 않습니다.
- HWPX manifest에 없는 ZIP `BinData`는 글꼴 얼굴 선언이 있으면 `isEmbedded`인 항목만 글꼴로
  보고, 확장자만으로 `isEmbedded="0"` 항목을 글꼴로 올리지 않습니다.
- HWPX 구역의 `outlineShapeIDRef`와 HWP5 구역 정의의 개요 번호 참조를 읽어 개요 문단의
  번호 모양을 고릅니다. 일반 문단 번호는 기존처럼 문단의 번호 참조를 사용합니다.
- HWPX 글자 외곽선과 그림자의 `type="NONE"`은 공통 모델에서도 꺼진 상태로 읽습니다.

### iOS 연결

- PDF 바이트, 쪽 수, 파서 경고, 글꼴 얼굴 상태와 구분된 오류를 반환하는 C ABI와 Swift
  래퍼를 더했습니다. iOS 기기·arm64 시뮬레이터 XCFramework는 파서 C ABI도 함께 제공합니다.

### ttoryeot

- 표 테두리 선을 한/글 12.30.0처럼 쪽 본문 원점과 원점에서 변까지의 거리를 따로 1/600인치 격자에
  반올림해 놓습니다. 겹선 두 선은 격자에 맞춘 선 굵기의 1.5배씩 벌립니다. 변경 추적으로 줄인 쪽은
  배율이 한/글과 약 0.1% 달라 이전 규칙을 그대로 씁니다. 우리 선 자리 중 기준 PDF와 같은 비율이
  홀드아웃에서 59.3%에서 91.1%로 늘었습니다.
- 칸 너비 합이 표 너비보다 짧은 줄이 있는 표도 오른쪽 끝을 한/글처럼 저장된 표 너비에 둡니다. 전에는
  짧은 줄이 열 너비를 정해 오른쪽 테두리가 약 1pt 안쪽에 그려졌습니다.
- 8비트 회색·RGB PNG는 풀고 다시 압축하지 않고 압축 데이터를 그대로 PDF에 넣습니다. 합성
  1200×800 RGB 그림 하나당 시간이 약 2.4ms에서 0.02ms로 줄었습니다. 이를 위해 krilla 0.8.2
  수정본을 `vendor/krilla`에 둡니다.
- 8비트 RGBA·팔레트·투명색 PNG는 풀어서 색과 알파를 나누되, deflate 수준 6 대신 Up 필터와
  `fdeflate`(잘 눌리는 그림은 수준 2)로 압축합니다. 큰 RGBA 그림이 많은 문서는 3~4배 빨라집니다.
- 위 PNG 중 체크섬은 맞고 압축 데이터가 깨진 그림이 있어도 문서 전체의 PDF 쓰기가 실패하지 않습니다.
- macOS에서 문서를 거듭 열어도 최대 메모리가 늘지 않게 하는 `allocator::Allocator`를 더했습니다.
  `ttoryeot-ffi`와 CLI는 등록해 두었고, Rust로 직접 쓰는 앱은 전역 할당자로 등록해야 합니다.
  macOS 밖에서는 시스템 할당자와 같습니다.
- 문단을 기준으로 가로 배치된 떠 있는 개체를 문단 좌우 여백 안에서 정렬합니다. 문서 첫머리의
  두 단을 가로지르는 개체 위로 단 구분선이 잘못 올라가는 경우를 바로잡았습니다.
- 글자색과 별도로 저장된 취소선 색을 사용합니다. 서로 이웃한 취소선도 색이 다르면 나눠
  그립니다. 취소선 모양은 아직 실선만 지원합니다.
- 이름과 대체 규칙에서 얼굴을 찾지 못하면 PDF 포함이 허용된 색인 얼굴도 시도합니다.
  선택한 얼굴에 글리프가 없을 때도 다른 포함 가능한 얼굴을 찾습니다. 같은 글꼴과 같은
  쪽 나눔을 보장하지는 않습니다.
- 문서에 내장된 글꼴을 문서별 메모리 색인으로 붙여 씁니다. 글꼴 파일을 디스크에 쓰지 않습니다.
- 내장 얼굴을 설치된 얼굴보다 먼저 고르고, 앞 문서의 내장 얼굴은 다음 문서 전에 지웁니다.
  얼굴 번호를 다시 쓰지 않아 지난 `Page`가 다음 문서의 얼굴로 조용히 그려지지 않습니다.
- 후보 이름의 내장·설치 얼굴을 먼저 찾고, 실패할 때만 대체 이름을 같은 차례로 찾습니다.
- OS/2 `fsType`을 OpenType 규격대로 풉니다. `0x0000`·`0x0004`·`0x0008`은 부분 집합으로
  넣고, `0x0002`(포함 금지)·`0x0200`(비트맵만 허가)·알 수 없는 값은 윤곽선도 남기지 않고
  글꼴이 없을 때와 같은 대체 사슬로 그립니다. `0x0100`(부분 집합 금지)은 전체 포함을
  지원하지 않으므로 대체 사슬로 그리며 절대 부분 집합으로 넣지 않습니다. 대체는 한 문서
  글꼴마다 하나의 얼굴로 정해지고, 대체 글꼴의 텍스트 층으로 검색·복사를 지킵니다. OS/2
  버전, 예약 비트, 표 없음은 보수적으로 다룹니다.
- `FontBook::attach_document`, `FontBook::document_face_count`, `FontBook::embedding`을 더했습니다.
- 줄 배치가 저장되지 않은 문단에서 컨트롤 바로 뒤의 넓은 표를 둘째 줄로 내리지 않습니다. 빈 첫 줄만큼
  표가 커져 다음 쪽으로 밀리던 문제입니다.
- 나뉘지 않는 표에서 칸 속 문단의 줄 위치가 앞 문단과 겹치게 저장됐으면 한/글처럼 앞 문단 아래에 다시
  쌓고 행을 키웁니다. 그 표 뒤의 본문 문단은 표의 실제 아래부터 쌓고, 본문 아래를 넘으면 다음 쪽으로
  옮깁니다. 비공개 말뭉치 276개(Windows)에서 쪽 수 일치가 270에서 274로 늘었습니다.

### 검증

- 괘선 묶음 경계를 잰 값으로 시험합니다. 양쪽 자리가 허용 오차 안인 겹선을 부동소수 어긋남 때문에 짝
  없는 선으로 세지 않는지 확인합니다. 정확도 문서에 괘선 묶음의 한계를 적었습니다. 렌더러와 점수는
  바뀌지 않습니다.
- 합성 구역별 개요 번호, 문단 기준 개체의 좌우 여백과 여러 단을 가로지르는 개체를
  시험합니다.
- EncryptVersion 0 합성 HWP5를 엔진과 CLI에서 열고, 암호 없음·오답·EncryptVersion 3 오류를
  각각 확인하는 시험을 더했습니다. EncryptVersion 3은 암호 없이도 버전을 알립니다.
- 커밋한 v0 fixture와 합성 스트림에 inner compact trailer를 넣어 CRC32·길이·정렬 검사를
  확인하고, v4 fixture에 outer footer와 CRC 불일치 시험을 더했습니다. 올바른 암호에서
  footer CRC나 암호문 1바이트가 손상돼도 `WrongPassword`가 아니라 경고·손상으로 끝나는지
  시험합니다.
- `.gitattributes`에 `*.hwp binary`를 더해 Windows 체크아웃에서도 HWP fixture가 텍스트로
  정규화되지 않게 했습니다.
- 공개 실제 표본(pyhwp, rhwp)으로 EncryptVersion 0·4, 암호 HWPX, 배포용 HWP5를 확인했습니다.
  수치와 고정 커밋·SHA-256·내려받기 명령은 [정확도 문서](docs/accuracy.md#암호배포용-문서)에
  적었습니다.
- 합성 HWPX로 내장 글꼴이 디스크 없이 PDF에 들어가는지, `fsType` 제한과 문서별 색인 분리를
  시험합니다.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --locked` 전체 시험이 통과합니다.
- 함초롬체를 뺀 채점 결과는 [정확도 문서](docs/accuracy.md)에 숫자로 적었습니다.

## 0.1.0 — 2026-10-27

첫 배포입니다. 다른 저장소에 있던 엔진과 채점기를
독립 저장소로 옮기고, 명령줄 도구·문서·CI·릴리스 구성을 더했습니다.

### hwp-core

- HWP 5.x의 비표준 CFB 컨테이너와 HWPX를 읽어 하나의 문서 모델로 만듭니다.
- 문단·탭 단위, 표·그림·도형·수식·각주·머리말·꼬리말·단·쪽 번호를 모델에 담습니다.
- 알려진 암호로 HWP5 EncryptVersion 4와 암호 HWPX를 엽니다. 배포용 문서도 읽습니다.
- 암호 없음·오답·지원하지 않는 방식·HWP 3.0·HWPML을 구별된 오류로 알립니다.
- C ABI(`staticlib`)와 JSON 프레임, `hwp-dump` 디버그 도구가 있습니다.

### ttoryeot

- 문서 모델을 쪽별 장면으로 배치하고 krilla로 PDF를 씁니다.
- 글꼴 색인, 한/글 글꼴 이름 대체 규칙, 글자 폭·잉크 상자 계산을 합니다.
- 병합 셀, 제목 행 반복, 이어지는 표 조각, 그림 자르기, 글상자, 음수 좌표를 다룹니다.
- `to_pdf`가 PDF 바이트·쪽 수·경고를 함께 돌려줍니다. `Error`로 파싱 실패와 PDF 실패를 나눕니다.
- `fonts::default_paths`가 운영체제 기본 글꼴 폴더를 찾습니다.

### ttoryeot-cli

- `ttoryeot render`로 HWP/HWPX를 PDF로 바꿉니다. 출력 기본값은 입력과 같은 이름의 `.pdf`입니다.
- `--font`로 글꼴 폴더·파일을 지정하고, `--password-env`로 암호를 환경 변수에서 읽습니다.
- `--json` 결과, `ttoryeot fonts` 글꼴 진단을 제공합니다.
- PDF는 임시 파일에 쓴 뒤 이름을 바꿔, 중간에 끊겨도 반쯤 쓴 파일이 남지 않습니다.

### ttoryeot-score

- 여러 기준 목록을 합치고 문서 묶음·형식별로 집계합니다.
- 기준 PDF와 후보를 같은 hayro 경로로 래스터화·글자 추출합니다.
- 문서 이름·원문·그림을 결과에 쓰지 않습니다.

### 검증

- 단위·통합 시험 320개가 통과합니다(`cargo test --workspace`).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`가 통과합니다.
- 비공개 말뭉치 기준 수치는 [README](README.md#정확도)와 [정확도 문서](docs/accuracy.md)에 적었습니다.

### 알려진 한계

- HWP5 EncryptVersion 0 암호는 지원하지 않습니다.
- 문서에 내장된 글꼴로 그리지 않습니다.
- 276개 중 7개, 254개 중 5개 문서의 쪽 수가 한/글과 다릅니다.
- macOS에서 주로 검증했습니다. Linux·Windows는 빌드와 단위 시험만 확인했습니다.
- 앱·GUI는 이 저장소에 없습니다. 엔진과 명령줄 도구만 있습니다.
