# HWPX

## 꾸러미

ZIP(머리 `PK`). 첫 항목 `mimetype`(압축 안 함) = `application/hwp+zip`.

| 항목 | 내용 |
|---|---|
| `Contents/header.xml` | 글꼴·글자 모양(`charPr`)·문단 모양(`paraPr`)·스타일·테두리 등 정의 |
| `Contents/section0.xml`… | 구역마다 본문. 이름이 `section<숫자>.xml`과 정확히 맞는 것만 구역이다 |
| `Contents/content.hpf` | OPF 목록(manifest, spine) |
| `BinData/…` | 그림·내장 글꼴 |
| `META-INF/manifest.xml` | 암호 문서면 ODF `encryption-data` |
| `Preview/PrvText.txt`, `PrvImage.png` | 미리 보기 |
| `settings.xml`, `version.xml` | 설정·판 |

구역 번호에 빈칸이 있으면(section0, section2) 빠진 구역이 있다는 뜻이다. 순서는 번호로 정한다.

## 구역 XML의 본문

```xml
<hp:p paraPrIDRef="20" styleIDRef="0" pageBreak="0" columnBreak="0">
  <hp:run charPrIDRef="11">
    <hp:t>2026학년도 1학기<hp:lineBreak/>주요 일정</hp:t>
  </hp:run>
  <hp:linesegarray>
    <hp:lineseg textpos="0" vertpos="0" vertsize="1900" textheight="1900"
                baseline="1615" spacing="572" horzpos="0" horzsize="41500" flags="393216"/>
  </hp:linesegarray>
</hp:p>
```

- `p` 문단 → `run`(글자 모양 하나) → `t`(글). 표·그림·도형은 `run` 안의 `tbl`·`pic`·`rect` 등이고, 표 칸의 글은
  `tc/subList/p`로 다시 문단이다. 앞붙이(`hp:`)는 문서마다 다를 수 있으니 이름 공간이 아니라 지역 이름으로 읽는다.
- `t` 안의 자식 요소: `tab`(폭·채움), `lineBreak`(강제 줄 바꿈), `nbSpace`(묶음 빈칸), `fwSpace`(고정폭 빈칸), `hyphen`.
  자식 요소가 글을 끊는다. 고칠 때 한 `t`의 이어진 글만 한 조각으로 본다.
- 변경 추적: `t` 안의 `insertBegin`/`deleteBegin`…`End`. 지운 글도 `t` 안에 남는다.
- `ctrl` 안에 머리말·꼬리말(`header`/`footer`의 `subList`), 단(`colPr`), 쪽 번호(`pageNum`), 감추기(`pageHiding`),
  새 번호(`newNum`), 각주(`footNote`) 등이 있다.

## 줄 배치 `linesegarray`

한/글이 저장할 때 계산한 줄마다의 자리다. 한/글과 같은 모양을 내려면 이 값을 그대로 써야 한다.

| 속성 | 뜻(HWPUNIT) |
|---|---|
| `textpos` | 이 줄이 시작하는 글 자리. UTF-16 칸으로 세고, 탭·컨트롤·개체는 8칸이다(HWP 5와 같다) |
| `vertpos` | 줄 위의 세로 자리(단 위 기준) |
| `vertsize` / `textheight` | 줄 높이 / 글 높이 |
| `baseline` | 줄 위에서 기준선까지 |
| `spacing` | 줄 간격 |
| `horzpos` / `horzsize` | 줄의 왼쪽 자리와 폭(왼쪽 여백만 넣고 들여쓰기·내어쓰기는 그릴 때 더한다) |

- `vertpos`가 앞 줄보다 작아지면 다음 단이나 쪽으로 넘어간 줄이다.
- 줄 배치가 없는 문단(다른 도구가 만든 문서, 글을 고쳐 지운 문단)은 여는 쪽이 글자 폭으로 다시 나눈다.
- 다른 도구가 쓴 문서에는 표 칸 속 문단들의 `vertpos`가 모두 0으로 겹쳐 저장된 것이 있다. 한/글은 앞 문단 아래에 다시 쌓는다.

## 단위와 값

- 길이는 HWPUNIT(1/7200인치). `charPr height` 1000 = 10pt. 비율 값(장평 `ratio`, 자간 `spacing`, 상대 크기 `relSz`)은 %이고
  언어 칸(hangul, latin, hanja, japanese, other, symbol, user)마다 따로 있다.
- 부호 있는 32비트 값(음수 오프셋·여백)을 부호 없이 적는 경우가 있다. 2^31 이상 2^32 미만 정수는 음수로 되돌려 읽는다.

## 내장 글꼴

`isEmbedded="1"`인 글꼴 얼굴의 `binaryItemIDRef`가 가리키는 `BinData` 항목만 글꼴이다. `isEmbedded="0"`의 참조는 포함 선언이
아니다. 글꼴 선언이 하나도 없을 때만 확장자(`.ttf` 등)로 글꼴을 짐작한다. 내장 글꼴은 메모리에만 올리고 디스크에 풀지 않는다.

## 암호 HWPX

`META-INF/manifest.xml`의 ODF `encryption-data`: PBKDF2-HMAC-SHA1로 키를 만들고 AES-256-CBC로 푼 뒤 raw deflate를 푼다.

## 큰 구역

구역 XML은 수 MB가 흔하다. 뿌리의 자식(`p`)을 하나씩 읽고 버리면 문서 전체 트리를 메모리에 두지 않아도 된다. 깊이 제한(예: 256)을
두고, 넘으면 그 안을 건너뛰며 경고한다.
