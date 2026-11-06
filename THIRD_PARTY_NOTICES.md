# 제3자 고지

또렷은 아래 제3자 코드·자료·글꼴·크레이트를 씁니다. 이 저장소는
[AGPL-3.0](LICENSE-AGPL-3.0) 또는 [또렷 상용 라이선스](COMMERCIAL-LICENSE.md) 중 하나를 골라 쓸 수 있습니다(`ttoryeot-alloc`과 `vendor/krilla`는 MIT OR Apache-2.0). 구성은 [LICENSING.md](LICENSING.md)를 보세요. 아래 제3자 코드는 각자의 허용 라이선스를 그대로 따릅니다.

## 옮겨 쓴 코드

### rhwp — 한/글 글꼴 치환 규칙

- 출처: <https://github.com/edwardkim/rhwp> (커밋 `680111ec7bea2fe11110de18c3676ba5a1cf7847`)
- 사용 허가: MIT License
- 옮긴 것: `ttoryeot/data/font-subst.tsv`의 한/글 글꼴 치환 규칙 표. rhwp의
  `src/renderer/font_rule_projections/layout_name.rs`에 있는 영문 칸 규칙 35개, HFT 규칙 117개,
  TTF 규칙 19개와 각 규칙의 언어 칸 조건, `src/renderer/style_resolver.rs`의 조회 순서를 옮겼습니다.
- 옮기지 않은 것: rhwp의 글자 폭 표. 또렷은 글꼴 파일에서 직접 잽니다.

```text
MIT License

Copyright (c) 2025-2026 Edward Kim

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### hwpjs — HWP5 EncryptVersion 0 DES 봉투 규칙 참고

- 출처: <https://github.com/ohah/hwpjs> (커밋 `c4b78a186cd35fb0c6bddf5db76b2c0ba36b87b1`)
- 사용 허가: MIT License
- 참고한 것: `src/hwp5/password_legacy_key.zig`의 UTF-16LE 키 유도, `password_footer.zig`의
  footer 표식·CRC, `password_compact_trailer.zig`의 크기·정렬 규칙,
  `password_properties.zig`의 보호된 DocInfo 256바이트 앞머리 제거. 코드를 그대로 옮기지 않고
  알고리즘과 상수만 참고해 Rust로 다시 썼습니다.
- hwpjs는 같은 규칙을 rhwp(MIT, 위 항목)에서 옮겼다고 적고 있습니다.

```text
MIT License

Copyright (c) 2021 ohah <bookyoon173@gmail.com>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Volexity hwp-extract — HWP5 EncryptVersion 4 알고리즘 참고

- 출처: Volexity, Inc.
- 사용 허가: BSD 3-Clause (전문: [`hwp-core/licenses/Volexity-BSD-3-Clause.md`](hwp-core/licenses/Volexity-BSD-3-Clause.md))
- 참고한 것: `hwp-core/src/hwp5/password.rs`의 HWP5 EncryptVersion 4 암호 해제 알고리즘.
  코드를 그대로 옮기지 않고 알고리즘과 상수만 참고해 Rust로 다시 썼습니다.

```text
Modified BSD License

Copyright © 2024, Volexity, Inc
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.
3. Neither the name of the Volexity, Inc nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS “AS IS” AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL Volexity, Inc BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
OF THE POSSIBILITY OF SUCH DAMAGE.
```

## 글꼴

### Liberation Sans

- 파일: `assets/fonts/LiberationSans-Regular.ttf`
- 사용 허가: SIL Open Font License 1.1 (전문: [`assets/fonts/LICENSE_LIBERATION`](assets/fonts/LICENSE_LIBERATION))
- 쓰는 곳: 시험에서 글자 폭을 재현할 때만 씁니다. 제품 렌더링용으로 넣은 글꼴이 아닙니다.
- 저작권: Digitized data copyright (c) 2010 Google Corporation with Reserved Font Arimo,
  Tinos and Cousine. Copyright (c) 2012 Red Hat, Inc. with Reserved Font Name Liberation.

## 의존 크레이트

`cargo about`로 만든 전체 목록과 라이선스 전문은
[`third-party/dependencies.md`](third-party/dependencies.md)에 있습니다. 릴리스 묶음에도 함께
넣습니다. 라이선스를 다시 만들려면:

```bash
cargo install cargo-about --locked --features cli
./tools/licenses.sh
```

## 시험 자료

`hwp-core/tests/fixtures/synthetic.hwpx`는 이 저장소에서 손으로 만든 최소 HWPX입니다.
실제 문서나 제3자 코드를 담지 않았습니다. 출처 기록은
[`hwp-core/tests/fixtures/NOTICE-synthetic.md`](hwp-core/tests/fixtures/NOTICE-synthetic.md)에 있습니다.
