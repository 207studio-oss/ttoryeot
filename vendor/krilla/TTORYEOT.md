# krilla 0.8.2 수정본

crates.io의 krilla 0.8.2에 아래 변경만 더했습니다. 라이선스는 원본과 같은 MIT OR Apache-2.0입니다.
원본은 https://github.com/LaurenzV/krilla 입니다.

- `src/graphics/image.rs`: 8비트 회색·RGB이고 비월·투명색이 없는 PNG는 풀지 않고 IDAT를 그대로
  넣습니다. `/DecodeParms`의 `/Predictor 15`가 PNG 필터를 되돌립니다. 그 밖의 PNG와 체크섬이
  틀린 PNG는 원래 경로로 갑니다. 압축을 풀어 보지는 않으므로, 체크섬은 맞고 압축 데이터가 깨진
  PNG는 원본처럼 PDF 쓰기를 실패시키지 않고 PDF 읽는 쪽으로 넘어갑니다.
- `src/graphics/image.rs`: 풀어야 하는 8비트 PNG(알파, 팔레트, tRNS, 비월 등)는 색과 알파를 나눠
  행마다 Up 필터를 걸고 `/Predictor 15`로 적습니다. 풀어 놓은 크기가 PNG 파일의 2.5배 이하면
  `fdeflate`로, 그보다 잘 눌리는 파일이면 deflate 수준 2로 압축합니다. 16비트 PNG와 GIF, WebP는
  원본 경로 그대로입니다.
- `src/stream.rs`, `src/graphics/image.rs`: `flate2` 대신 `miniz_oxide`로 바로 압축합니다. `flate2`는 한
  프로그램 안에서 기능이 합쳐져, `hayro`처럼 `zlib-rs`를 켜는 의존이 생기면 압축 결과가 달라집니다.
  이렇게 하면 PDF 바이트가 무엇과 함께 빌드되었는지에 좌우되지 않습니다.
- `Cargo.toml`: 패키지 이름을 `ttoryeot-krilla`로 바꿨습니다(라이브러리 이름은 `krilla` 그대로).
  수정본을 crates.io에 따로 올려야 또렷을 설치해도 같은 PDF가 나옵니다. `flate2` 대신
  `miniz_oxide`를, 체크섬 검사에 쓰는 `crc32fast`와 빠른 압축에 쓰는 `fdeflate`를 더했습니다.
- `src/lib.rs`: `#![allow(deprecated)]`로 원본의 경고를 끕니다. 경로 의존성은 cargo가 경고를 숨기지 않습니다.
- `LICENSE_*`: crates.io 묶음에 없어 원본 저장소에서 가져왔습니다.

공개 API로는 압축된 데이터를 넣을 수 없어 수정본을 둡니다. 원본과의 차이는 이렇게 봅니다.

    cargo fetch && diff -ru ~/.cargo/registry/src/*/krilla-0.8.2 vendor/krilla

krilla를 올릴 때는 새 버전에 같은 변경을 다시 적용하고 `ttoryeot/tests/png_embed.rs`를 돌립니다.
