# HWP 5 바이너리

## 컨테이너

- MS-CFB(OLE 복합 문서). 머리 `D0 CF 11 E0 A1 B1 1A E1`.
- 표준을 벗어난 CFB가 실제로 있다. 알려진 변형: 머리의 mini stream cutoff가 4096이 아니고(0) 모든 스트림이 일반 섹터에 있다.
  일반 CFB 라이브러리가 거절하면 머리·FAT·디렉터리를 직접 읽어 표준 CFB로 다시 쓴 뒤 연다.

| 스트림 | 내용 |
|---|---|
| `FileHeader` | 서명 `HWP Document File`, 판, 속성 플래그, 암호 판(EncryptVersion). 48바이트보다 짧으면 EncryptVersion이 없다 |
| `DocInfo` | 글꼴·글자 모양·문단 모양·스타일·테두리/배경·번호 매기기 등 문서 전체 정의 |
| `BodyText/Section0`… | 구역마다 본문 레코드 |
| `ViewText/Section0`… | 배포용 문서의 본문(암호화). 있으면 `BodyText` 대신 이것을 읽는다 |
| `BinData/BIN0001.xxx`… | 그림·OLE·내장 글꼴 |
| `HwpSummaryInformation` | MS-OLEPS 속성 집합(제목, 지은이 등) |
| `PrvText`, `PrvImage` | 미리 보기 글·그림. 배치에는 쓰지 않는다 |

## FileHeader 속성 플래그

- 비트 0: 압축. 본문·DocInfo 스트림이 **raw deflate**(zlib 머리 없음)로 압축됐다.
- 비트 1: 암호.
- 비트 2: 배포용. 단, 표시만 있고 `ViewText` 없이 `BodyText`에 평문을 둔 파일도 있다. 그러면 `BodyText`를 읽는다.

`BinData` 항목은 저마다 압축 방식이 있다: 0 = 문서 설정을 따름, 1 = 압축, 2 = 압축 안 함.

## 레코드

스트림 내용은 레코드의 줄이다. 머리 4바이트(little-endian u32):

```
tag   = header & 0x3FF          (10비트)
level = (header >> 10) & 0x3FF  (10비트, 부모-자식 깊이)
size  = (header >> 20) & 0xFFF  (12비트). 0xFFF면 다음 u32가 크기
```

자주 쓰는 태그(HWPTAG_BEGIN = 16 기준):

| 태그 | 이름 | 태그 | 이름 |
|---|---|---|---|
| 16 | DOCUMENT_PROPERTIES | 66 | PARA_HEADER |
| 17 | ID_MAPPINGS | 67 | PARA_TEXT |
| 18 | BIN_DATA | 68 | PARA_CHAR_SHAPE |
| 19 | FACE_NAME | 69 | PARA_LINE_SEG |
| 20 | BORDER_FILL | 70 | PARA_RANGE_TAG |
| 21 | CHAR_SHAPE | 71 | CTRL_HEADER |
| 22 | TAB_DEF | 72 | LIST_HEADER |
| 23 | NUMBERING | 73 | PAGE_DEF |
| 24 | BULLET | 74 | FOOTNOTE_SHAPE |
| 25 | PARA_SHAPE | 75 | PAGE_BORDER_FILL |
| 26 | STYLE | 76 | SHAPE_COMPONENT |
| 27 | DOC_DATA | 77 | TABLE |
| 28 | DISTRIBUTE_DOC_DATA | 78–86 | 선·사각형·타원·호·다각형·곡선·OLE·그림·묶음 |
| 30 | COMPATIBLE_DOCUMENT | 87 | CTRL_DATA |
| 31 | LAYOUT_COMPATIBILITY | 88 | EQEDIT(수식) |
| 32 | TRACK_CHANGE(설정, 모든 문서에 있음) | 90 | TEXT_ART |
| 92 | MEMO_SHAPE | 91 | FORM |
| 94 | FORBIDDEN_CHAR | 93 | MEMO_LIST |
| 96 | TRACK_CHANGE_CONTENT(남은 변경이 있을 때만) | 95 | CHART_DATA |

변경 추적 여부는 태그 32가 아니라 96이 있는지로 본다.

## 문단 글(PARA_TEXT)

UTF-16LE. 32 미만의 코드는 제어 문자이고, 차지하는 칸 수가 다르다. 줄 배치의 `textpos`는 이 칸 수로 센다.

| 코드 | 뜻 | 칸 |
|---|---|---|
| 0, 25–29 | 쓰지 않음/예약 | 1 |
| 13 | 문단 끝 | 1 |
| 10 | 줄 바꿈 | 1 |
| 24 | 하이픈 | 1 |
| 30 | 묶음 빈칸 | 1 |
| 31 | 고정폭 빈칸 | 1 |
| 9 | 탭(뒤에 폭·종류 자료) | 8 |
| 4–8, 19, 20 | 인라인 제어 | 8 |
| 1–3, 11, 12, 14–18, 21–23 | 확장 제어(표·그림·각주·머리말 등). CTRL_HEADER 레코드와 차례로 짝지음 | 8 |

## 배포용 문서

`ViewText/SectionN`은 첫 레코드가 태그 28(크기 256바이트 키 자료)이다. 그 256바이트로 AES-128 키를 만들고 나머지를 ECB로
풀면 보통 레코드가 나온다(문서가 압축이면 그다음 raw deflate). 배포용은 보기 제한이지 암호가 아니다. 인쇄·복사 제한을 다시
배포할 때는 문서를 만든 사람의 뜻을 따른다.

## 암호 문서

EncryptVersion 0은 DES, 4는 AES-128 봉투를 쓴다(공개 구현: Volexity hwp-extract, rhwp/hwpjs). 암호를 모르면 열 수 없다.
암호는 명령줄 인자로 받지 말고 환경 변수 같은 기록에 남지 않는 길로 받는다.
