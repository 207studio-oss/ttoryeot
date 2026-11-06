# 정확도

## 요약

비공개 말뭉치에서 한/글이 내보낸 기준 PDF와 비교한 값입니다. 두 실행은 환경과 문서 묶음이
달라서 서로 직접 비교하면 안 됩니다.

아래 실행은 글꼴 이름을 모두 찾지 못했을 때 색인된 다른 얼굴을 마지막 대체로 쓰는 변경
이전에 측정했습니다. 그 변경 뒤 비공개 말뭉치 전체는 다시 채점하지 않았습니다.

| 실행 | 코드 | 문서·쪽 | 쪽 수 일치 | 화소 차이 % ↓ | 잉크 F1 ↑ | 글자 F1 ↑ |
|---|---|---:|---:|---:|---:|---:|
| Windows, 한컴 2024 글꼴 | `502c134` | 276개·1,594쪽 | 274/276 | 2.4456 | 0.9288 | 0.9800 |
| macOS, 한컴 글꼴 범위 | `b8903497` | 254개·1,527쪽 | 249/254 | 3.5129 | 0.8977 | 0.9769 |

- 두 실행 모두 후보 PDF 생성 실패는 0개입니다.
- 같은 조건에서 쪽 나눔 수정 전 `main`(`34e558c`)은 270/276, 2.5959%, 0.9156, 0.9800입니다.
  쪽 나눔 수정(`25bfb21`~`c267e4c`)으로 바뀐 문서는 7개이고 모두 나아졌습니다.
- macOS 실행은 100% 배율 기준 PDF만 모은 253개·1,524쪽 하위 묶음도 따로 기록했습니다.
  전체와 같은 결론입니다.
- 254개 실행에서 표 넘침 판단을 고친 뒤(`b8903497`) 한 문서가 27쪽에서 26쪽으로 바뀌었고,
  그 문서의 화소 차이는 4.7222→2.6758%, 잉크 F1은 0.7026→0.9424가 됐습니다.
  나머지 253개 문서의 문서별 지표는 그대로였습니다.

## 함초롬체가 없을 때

이 컴퓨터에서 기준 PDF가 있는 24개(72쪽, HWP 19개·HWPX 5개)만 함초롬체를 빼고 다시
쟀습니다. 위 276개·254개 실행과 환경이 달라 서로 직접 비교하면 안 됩니다.

- 포함: Windows 글꼴 폴더(`함초롬바탕`·`함초롬돋움` 포함, 248얼굴)
- 제외: 같은 폴더에서 함초롬체 파일 8개만 뺀 것(240얼굴). 두 실행 모두 후보 PDF 생성 실패 0개.

| 묶음 | 문서 | 기준 쪽 | 후보 쪽(포함/제외) | 쪽 수 일치(포함/제외) | 글자 F1(포함/제외) |
|---|---:|---:|---:|---:|---:|
| 전체 | 24 | 72 | 72 / 72 | 21/24 / 21/24 | 0.993342 / 0.993342 |
| HWP | 19 | 57 | 57 / 57 | 16/19 / 16/19 | 0.996505 / 0.996505 |
| HWPX | 5 | 15 | 15 / 15 | 5/5 / 5/5 | 0.981320 / 0.981320 |

쪽 수와 글자 F1은 문서별로도 같았습니다(바뀐 문서 0개). 함초롬체가 없을 때 대체 글꼴의
글자 폭이 쪽 나눔을 바꿀 만큼 다르지 않았고, PDF에서 뽑은 글자도 같았습니다. 대신 화소와
잉크는 달라졌습니다.

| 묶음 | 화소 차이 % ↓(포함 → 제외) | 잉크 F1 ↑(포함 → 제외) | 변화 |
|---|---:|---:|---|
| 전체 | 2.9158 → 2.9541 | 0.8927 → 0.8912 | 화소 +0.0383%p, 잉크 −0.0015 |
| HWP | 3.1840 → 3.2314 | 0.8762 → 0.8744 | 화소 +0.0474%p, 잉크 −0.0018 |
| HWPX | 1.8967 → 1.9004 | 0.9554 → 0.9552 | 화소 +0.0037%p, 잉크 −0.0002 |

100% 배율 기준 PDF만 고른 22개(기준 67쪽)에서도 결론은 같았습니다. 후보 쪽은 66/66, 쪽 수
일치는 21/22, 글자 F1은 0.993786으로 같았고, 화소 차이는 1.7320 → 1.7738, 잉크 F1은
0.9599 → 0.9583이었습니다.

## 무엇을 비교했나

1. 한/글에서 문서를 열고 PDF로 내보내 기준을 만듭니다.
2. 또렷으로 같은 문서를 PDF로 만듭니다.
3. 두 PDF를 같은 설정으로 래스터화합니다(72dpi).
4. 화소·잉크·글자를 문서별로 재고, 문서별 값의 산술 평균을 냅니다.

### 화소 차이

- 잉크 밝기 192 미만을 잉크로 봅니다.
- 허용 거리 1px 안의 잉크는 같은 것으로 봅니다.
- 기준과 후보의 잉크가 다른 화소 비율을 문서별로 내고 평균합니다.
- 쪽 크기가 다르면 그 쪽은 전체 차이로 셉니다.

### 잉크 F1

기준 잉크와 후보 잉크의 정밀도·재현율을 1px 허용 거리로 계산한 뒤 F1을 냅니다.
글자가 아니라 **칠해진 모양**을 보므로 도형·표·그림도 함께 반영됩니다.

### 글자 F1

PDF에서 뽑은 글자를 NFKC로 정규화하고, 공백·제어 문자·U+FFFD를 뺀 뒤 문자 다중집합으로
비교합니다. 순서·줄바꿈은 보지 않습니다. 글자가 겹쳐 찍혀도 한 번만 셉니다.

### 구조 불일치(줄바꿈·쪽 나눔·줄 자리·괘선·그림)

화소 차이는 글꼴 모양 차이까지 섞어 셉니다. 구조 비교는 **자리만** 보므로, 같은 글꼴을 쓰면
0이어야 하는 값입니다. 두 PDF를 같은 길(`ttoryeot-score/src/structure.rs`)로 읽습니다.

- **줄**: 기준선이 같은(글자 크기의 10% 안) 글자를 왼쪽부터 이은 것. 표의 같은 높이 칸들은 한 줄이 됩니다.
- **줄바꿈 불일치**: 두 문서의 줄 글을 최장 공통 부분열로 짝지은 뒤 짝이 없는 줄의 수. 줄바꿈 자리가 하나 다르면 양쪽에 하나씩, 둘이 생깁니다.
- **쪽 나눔 불일치**: 글은 같은데 다른 쪽에 놓인 줄. 쪽 나눔·표 행 나눔이 다르면 생깁니다.
- **세로·가로 불일치**: 같은 쪽 같은 글인데 기준선 y 또는 시작 x가 허용 오차를 넘은 줄.
- **괘선 불일치**: 두께 2.5pt 이하, 길이 3pt 이상인 곧은 칠·선을 괘선으로 보고, 같은 자리의 토막을 이은 뒤 자리와 양 끝이 허용 오차 안에서 짝이 없는 수. 표 행 높이·셀 나눔이 다르면 생깁니다.
- **그림 불일치**: 그림 상자 네 변이 허용 오차 안에서 짝이 없는 수.
- **괘선 묶음의 한계**: 0.54pt 안의 선은 평균 자리 한 줄로 묶은 뒤 짝을 찾습니다. 양쪽의 겹선 간격이 이 경계를 사이에 두고 다르면(예를 들어 0.48pt와 0.6pt) 자리가 허용 오차 안이어도 짝 없는 선으로 세고, 반대로 묶인 선의 작은 어긋남은 드러나지 않습니다. 괘선 불일치 0은 모든 선 자리가 같다는 뜻이 아닙니다. 현재 네 묶음에는 이렇게 세는 경우가 없고, 경계를 0.50, 0.54, 0.58로 바꿔도 145개 문서의 점수가 같습니다.
- **허용 오차 1pt**: 72dpi 래스터의 한 화소이며 잉크 허용 반경과 같습니다. 한/글은 가로 괘선 끝을 선 굵기의 반만큼 늘여 그려서, 이보다 좁히면 그리는 법의 차이를 자리 차이로 셉니다. 허용 안의 어긋남도 숨기지 않도록 짝지은 줄·괘선의 최대 편차를 함께 적습니다.

`--candidate self`(기준 PDF를 자기 자신과 비교)는 모든 문서에서 0입니다.

### 쪽 수 일치

기준 PDF와 후보 PDF의 쪽 수가 같은 문서의 수입니다. 쪽 수가 다르면 배치가 어긋났다는
뜻이므로 가장 먼저 보는 지표입니다.

## 남은 쪽 수 차이

| 실행 | 다른 문서 수 | 비고 |
|---|---:|---|
| Windows 276개 | 2 | 14번 3/2쪽(기준/후보), 15번 3/2쪽(기준 PDF가 75%로 줄여 찍힘) |
| macOS 254개 | 5 | 쪽 나눔 수정 전에 잰 값. 26·96·115번 문서는 30/60쪽, 194번은 47/45쪽, 198번은 27/26쪽 |

쪽 나눔 수정(`25bfb21`~`c267e4c`)에서 고친 것(Windows, 같은 기준 PDF로 확인):

- 26·96·115번: 한/글이 아닌 도구가 쓴 문서라 칸 속 문단의 줄 위치가 모두 0으로 겹쳐 저장됐습니다.
  한/글은 그 문단을 다시 쌓아 행을 키우고, 뒤 문단을 표의 실제 아래에 쌓습니다. 그래서 쪽 끝 그림이
  다음 쪽으로 넘어가 쪽 수가 두 배가 됩니다. 같은 서식의 83·97번은 그림이 쪽에 남아 쪽 수가 그대로입니다.
- 194번: 줄 배치가 없는 문단에서 컨트롤 뒤의 넓은 표를 둘째 줄로 내려 행이 18pt씩 커졌습니다.
- 198번은 이 수정 전에 이미 Windows 실행에서 맞았습니다.

남은 둘:

- 14번(맥 한/글 기준): 한/글이 글상자 속 문단을 한 줄 더 나눠 글상자가 약 14pt 커졌고, 마지막 줄이 셋째
  쪽으로 갔습니다. 또렷의 글자 폭으로는 그 줄이 0.18pt 넘칠 뿐이고, 같은 문서의 본문 줄은 1%쯤 넘쳐도
  한/글이 다시 나누지 않았습니다. 글자 폭만으로 다시 나눌지 정하면 다른 줄이 틀어지므로 고치지 않았습니다.
- 15번: 기준 PDF가 메모를 함께 찍어 75%로 줄인 것입니다. 100% 배율로 다시 만들어야 판단할 수 있습니다.

저장된 줄 전체를 다시 나누거나 그림만 다시 나누는 실험은 다른 문서에서 회귀해 되돌렸습니다.
기준 PDF에 글리프 없는 쪽이 있어도 잉크가 있으면 잘못된 기준이라고 단정하지 않습니다.

## 암호·배포용 문서

### 공개 표본 수치

공개 저장소의 실제 파일을 고정 커밋에서 내려받아 이 컴퓨터(Windows)에서 잰 값입니다. 표본은
저장소에 커밋하지 않습니다. 아래 표의 문단 수는 표 셀·머리말 등 안쪽 문단을 포함한
`text_summary` 값입니다.

| 표본 | 보호 방식 | 암호 | 구역 | 문단 | 표 | 그림 | 경고 | PDF 쪽 | PDF 바이트 |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| pyhwp `password-12345.hwp` | HWP5 EncryptVersion 0 DES | 12345 | 1 | 2 | 0 | 1 | 0 | 1 | 1,958 |
| rhwp `hwp3-sample16-hwp5-2024-password-123456.hwp` | HWP5 EncryptVersion 4 AES | 123456 | 1 | 4,282 | 91 | 7 | 3 | 64 | 283,626 |
| rhwp `HWP5-password-123456.hwpx` | HWPX AES-256-CBC | 123456 | 1 | 418 | 2 | 2 | 0 | 24 | 63,055 |
| rhwp `distribution_doc.hwpx` | 배포용 HWP5 ViewText | — | 1 | 132 | 8 | 2 | 1 | 3 | 60,960 |
| pyhwp `viewtext.hwp` | 배포용 HWP5 ViewText | — | 1 | 1 | 0 | 0 | 1 | 1 | 1,414 |
| rhwp `HWP5-nopassword-123456.hwp` | 암호 없음(대조군) | — | 1 | 418 | 2 | 2 | 0 | 24 | 63,055 |

- EncryptVersion 0 표본은 암호 `12345`로 열렸고 경고가 없었습니다. 같은 파일을 암호 없이 열면
  `PasswordProtected`, 다른 암호로 열면 `WrongPassword`입니다.
- EncryptVersion 4 표본의 경고 3개는 모두 `UnsupportedObject`입니다. 보호된 DocInfo 앞머리를
  정리하면서 이전에 남던 `CountMismatch` 1개가 사라졌습니다.
- 배포용 두 표본의 경고 1개는 `DistributionDecrypted`(ViewText 복호화)입니다.
- `distribution_doc.hwpx`는 확장자와 달리 HWP5 CFB 파일이며, 표 8개와 그림 2개를 읽었습니다.
- HWPX 표본과 암호 없는 대조군은 같은 쪽 수·바이트로 나왔습니다.
- 공개 표본 검증은 `EncryptVersion 0` 합성 시험과 별개입니다. 합성 HWP5·CLI 시험은 저장소 안에서
  항상 돌고, 위 표본은 아래 명령으로만 재현합니다.

### 표본 출처와 라이선스

- pyhwp: AGPL-3.0 저장소의 시험 fixture입니다. 파일 자체의 재배포 조건은 별도로 확인하지
  않았으므로 저장소에 넣지 않고 내려받기만 합니다.
- rhwp: MIT 저장소의 `samples/` 파일입니다. 코드 라이선스와 문서 원저작권은 별개일 수 있어
  역시 저장소에 넣지 않습니다.
- 두 저장소의 표본 이름·암호는 공개 시험용이며 실제 사용자 암호가 아닙니다.

### 내려받기와 재현

```bash
mkdir -p /tmp/ttoryeot-public-samples
cd /tmp/ttoryeot-public-samples

# pyhwp, master 83239f0d3bdf438b2c9f7dcff455a6e841154a39
curl -L --fail -O https://raw.githubusercontent.com/mete0r/pyhwp/83239f0d3bdf438b2c9f7dcff455a6e841154a39/tests/hwp5_tests/fixtures/password-12345.hwp
curl -L --fail -O https://raw.githubusercontent.com/mete0r/pyhwp/83239f0d3bdf438b2c9f7dcff455a6e841154a39/tests/hwp5_tests/fixtures/viewtext.hwp

# rhwp, main 1a76570e833917d15817415a53c09ad61ab3203f
curl -L --fail -o hwp5-v4.hwp https://raw.githubusercontent.com/edwardkim/rhwp/1a76570e833917d15817415a53c09ad61ab3203f/samples/hwp3-sample16-hwp5-2024-password-123456.hwp
curl -L --fail -o password.hwpx https://raw.githubusercontent.com/edwardkim/rhwp/1a76570e833917d15817415a53c09ad61ab3203f/samples/HWP5-password-123456.hwpx
curl -L --fail -o distribution.hwp https://raw.githubusercontent.com/edwardkim/rhwp/1a76570e833917d15817415a53c09ad61ab3203f/samples/task1768/distribution_doc.hwpx
curl -L --fail -o plain.hwp https://raw.githubusercontent.com/edwardkim/rhwp/1a76570e833917d15817415a53c09ad61ab3203f/samples/HWP5-nopassword-123456.hwp
```

SHA-256(내려받은 파일이 위 표본과 같은지 확인):

```text
e86aac42e3efbcc2e06cc50e628705dce710bec0c252961b7d52c87e3c6b0660  password-12345.hwp
c428c034255d8226791983620c9059d3f92bf98a205b97c3ef204cdf89c8cb2c  viewtext.hwp
59d4bed335b9552fe78fa68d2a56f7cfa3d586bcdeaaba839af80df13f3e08dc  hwp5-v4.hwp
93e7a62565e0f3efa4feee2812aaf518347dbbcc09d2f26a0d9385f9a4e26060  password.hwpx
f217c02af7bc2a6641b9544afd29765ddccdd15021507b7554521f68b9d560dc  distribution.hwp
a34ecb8cde85b6db49c64a954cb7fa5d23b5f49367bc4753c90bfe89a075b50d  plain.hwp
```

Windows PowerShell에서는 `curl.exe` 대신 `Invoke-WebRequest -OutFile`, 해시는
`Get-FileHash -Algorithm SHA256`을 씁니다.

```bash
# EncryptVersion 0. 공개 예제 암호이므로 셸 기록에 남아도 되는 값입니다.
TTORYEOT_PASSWORD=12345 cargo run --release -p ttoryeot-cli -- \
  render /tmp/ttoryeot-public-samples/password-12345.hwp \
  --font assets/fonts --password-env TTORYEOT_PASSWORD --json

# EncryptVersion 4
TTORYEOT_PASSWORD=123456 cargo run --release -p ttoryeot-cli -- \
  render /tmp/ttoryeot-public-samples/hwp5-v4.hwp \
  --font assets/fonts --password-env TTORYEOT_PASSWORD --json

# 암호 HWPX
TTORYEOT_PASSWORD=123456 cargo run --release -p ttoryeot-cli -- \
  render /tmp/ttoryeot-public-samples/password.hwpx \
  --font assets/fonts --password-env TTORYEOT_PASSWORD --json

# 배포용 HWP5 (암호 없음)
cargo run --release -p ttoryeot-cli -- \
  render /tmp/ttoryeot-public-samples/distribution.hwp \
  --font assets/fonts --json
```

PDF 쪽·바이트는 글꼴과 플랫폼에 따라 달라질 수 있습니다. 위 값은 `assets/fonts`만 지정한
Windows 실행의 값입니다.

### 비공개 말뭉치

- 암호 없이 말뭉치를 열면 296/299개가 열리고 3개는 `PasswordProtected`입니다.
- HWP5 EncryptVersion 0 표본은 DES 경로로 열립니다. 지원하지 않는 EncryptVersion 1~3은
  암호 없이도 `UnsupportedEncryption`에 버전과 지원 범위를 적어 알립니다.

## 재현

말뭉치와 기준 PDF는 비공개라 이 저장소에 없습니다. 같은 방식으로 재려면:

```bash
# 1. 문서 목록과 기준 PDF 목록을 만든다
cargo run --release -p ttoryeot-score -- need-refs \
  --documents /path/to/documents --manifest /path/to/lists --root /path/to/refs \
  --out missing.txt

# 2. 채점한다
cargo run --release -p ttoryeot-score -- score \
  --documents /path/to/documents \
  --manifest /path/to/lists \
  --candidate ttoryeot \
  --fonts /path/to/fonts \
  --out scores.json
```

`--candidate`에는 `ttoryeot`, `self`(기준 PDF를 후보로 써서 자기 비교), `blank`(빈 PDF)를
쓸 수 있습니다. 채점기 전체 사용법은 `cargo run -p ttoryeot-score --`로 봅니다.

함초롬체가 없을 때를 재려면 `--fonts`에 함초롬체 파일만 뺀 글꼴 폴더를 주고 같은 명령을
한 번 더 돌립니다. 두 결과 JSON에서 문서별 `candidate_pages`와 `text_f1`을 묶음별로
평균 내면 위 표가 됩니다.

## 한계

- **이 수치는 출시 품질 보증이 아닙니다.** 비공개 말뭉치에서 잰 값이고, 밖에서 그대로
  재현할 수 없습니다.
- 글꼴이 다르면 같은 문서도 다르게 보입니다. 기준과 후보의 글꼴 조건을 맞추지 않은 수치는
  비교하지 않습니다.
- 취소선 색은 반영하지만 선 종류는 구분하지 않고 실선으로 그립니다.
- 전체 PR에 대한 독립 리뷰는 표 넘침 수정 범위에 대해서만 받았습니다. 채점은 작성자의
  JSON을 독립 비교한 것이지 전체 말뭉치를 다시 돌린 것은 아닙니다.
- Windows 실행은 Xcode·iPad 검증이 아닙니다. macOS 실행은 Windows 검증을 대신하지 않습니다.
- GitHub Actions에서 말뭉치를 돌리지 않습니다. CI는 단위·통합 시험만 확인합니다.
