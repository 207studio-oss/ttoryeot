# 라이선스 구성

또렷의 코드는 207studio가 저작권을 가집니다. 엔진 라이브러리(`ttoryeot-hwp-core`, `ttoryeot`,
`ttoryeot-ffi`)는 **AGPL-3.0-only** 또는 **또렷 상용 라이선스**([COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md))
가운데 하나를 골라 쓸 수 있습니다. 명령줄 도구와 MCP 서버는 **AGPL-3.0-only** 하나뿐입니다.

| 크레이트 | 하는 일 | 라이선스 | 공개 |
|---|---|---|---|
| `ttoryeot-hwp-core` | 엔진: HWP/HWPX 해석 | AGPL-3.0-only 또는 상용 (선택) | 공개 |
| `ttoryeot` | 엔진: 배치·글꼴·PDF | AGPL-3.0-only 또는 상용 (선택) | 공개 |
| `ttoryeot-ffi` | 엔진 C ABI·Swift 묶음 | AGPL-3.0-only 또는 상용 (선택) | 공개 |
| `ttoryeot-cli` | 명령줄 도구 | AGPL-3.0-only | 공개 |
| `ttoryeot-mcp` | MCP 서버 | AGPL-3.0-only | 공개 |
| `ttoryeot-alloc` | 전역 할당자(엔진에 의존하지 않음) | MIT OR Apache-2.0 | 공개 |
| `vendor/krilla` | 고친 PDF 라이브러리(제3자) | MIT OR Apache-2.0 (원 라이선스 그대로) | 공개 |
| `ttoryeot-score` | 기준 PDF와 비교하는 채점기(말뭉치는 비공개) | AGPL-3.0-only | 공개 |

`ttoryeot-ffi`와 `ttoryeot-score`는 `publish = false`라서 crates.io에 올리지 않습니다. 소스는 이 저장소에 있습니다.
각 크레이트 폴더에는 해당 라이선스 전문이 들어 있습니다. 엔진 라이브러리의 상용 라이선스 제안은 이 문서와
[COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md)에 있고, 크레이트 메타데이터의 `license` 값은 `AGPL-3.0-only`입니다.

## 누가 무엇을 내야 하나

- **CLI·MCP를 도구로 쓰는 사람(개인·회사 모두)**: 무료입니다. 고치지 않고 쓰면 AGPL에서 지킬 것은
  저작권·라이선스 고지를 지우지 않는 것뿐입니다. MCP 서버를 고쳐 네트워크로 남에게 제공하면 고친 소스를
  그 사용자에게 공개해야 합니다(AGPL 13조). CLI·MCP 소스를 가져다 고치거나 자기 제품에 묶어 배포하는 경우에도
  AGPL-3.0-only 조건을 따릅니다. 이 둘에는 상용 라이선스를 제공하지 않습니다.
- **엔진 라이브러리를 자기 프로그램·서비스에 넣는 사람**: 둘 중 하나를 고릅니다.
  - **AGPL-3.0-only**: 그 프로그램 전체를 AGPL로 공개하면 무료입니다.
  - **상용 라이선스**: 소스를 공개하지 않으려면 상용 라이선스를 삽니다. 개발자 계정당 1회 30,000원이고,
    이용자 제품 매출이 1억 원을 넘으면 넘는 금액의 3%를 냅니다. 매출 정의 같은 일부 조항은 아직 확정 전이며
    COMMERCIAL-LICENSE.md에 `확정 필요`로 표시해 두었습니다. 구매 창구는 아직 열리지 않았습니다.
- **기여하는 사람**: 기여물은 AGPL-3.0과 상용 라이선스로 함께 배포됩니다. [CLA.md](CLA.md)를 보세요.
- **할당자와 krilla**: 각자의 MIT OR Apache-2.0 조건을 따릅니다. 제3자 고지는
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 있습니다.
