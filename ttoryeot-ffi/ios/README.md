# iOS에서 PDF 만들기

`ttoryeot-ffi`는 HWP/HWPX 바이트와 글꼴 바이트를 받아 PDF와 변환 결과를 돌려줍니다.
입력 파일을 임시 디렉터리에 쓰지 않습니다. 앱 UI와 문서 가져오기 경로는 이 저장소에
들어 있지 않습니다.

## XCFramework 만들기

Rust 1.99 이상, Xcode와 다음 Rust 타깃이 필요합니다.

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
./ttoryeot-ffi/ios/build-ios.sh
```

`dist/Ttoryeot.xcframework`에 arm64 iOS 기기와 arm64 시뮬레이터 라이브러리가 생깁니다.
기본 최소 배포 버전은 iOS 15입니다. 다른 경로를 원하면 스크립트의 첫 인자로 지정합니다.
이미 있는 출력 경로는 덮어쓰지 않습니다. Intel 시뮬레이터용 슬라이스는 포함하지 않습니다.
Rust 1.99.1과 Xcode 27.1 조합에서는 Apple `nm`이 미리 빌드된 Rust 표준 라이브러리의 일부
LLVM 비트코드에 진단을 낼 수 있습니다. 앱 링크와 시뮬레이터 실행은 별도로 확인해야 합니다.

Xcode 프로젝트에 XCFramework를 연결하고 `ttoryeot-ffi/swift/Ttoryeot.swift`를 앱 타깃에
추가합니다. 렌더러 C 모듈 이름은 `CTtoryeot`입니다. 같은 라이브러리에 파서 ABI와
`CHWPCore` 모듈도 들어 있으므로 기존 `libhwp_core.a`와 중복 `CHWPCore` modulemap은 앱 타깃에서
제외하고 이 산출물 하나로 두 ABI를 부를 수 있습니다. 이 저장소와 산출물은 207studio의 독점
라이선스를 따릅니다. 앱에 넣는 글꼴은 별도로 사용·배포 허가를 확인해야 합니다.
검증 예제는 `Security`와 `CoreFoundation`을 링크하며 PDF를 확인하는 데 `PDFKit`을 씁니다.

## Swift 호출

```swift
import Foundation
import PDFKit

let documentData = try Data(contentsOf: documentURL)
let fontData = try Data(contentsOf: fontURL, options: .alwaysMapped)
let output = try await Task.detached {
    try Ttoryeot.render(data: documentData, password: nil, fonts: [fontData])
}.value
let pdf = PDFDocument(data: output.pdf)
print(output.pages, output.warnings, output.fontFaces)
```

호출은 동기식이므로 큰 문서는 메인 스레드 밖에서 변환하세요. 암호가 있으면 `password`에
문자열을 넘깁니다. `Ttoryeot.Failure.kind`는 `PasswordProtected`, `WrongPassword`,
`UnsupportedEncryption`, `CorruptDocument`, `Hwp3Unsupported`, `HwpmlUnsupported`, `Pdf`,
`Internal` 등을 구분합니다. `detail`에는 파서가 제공한 상세가 담깁니다.

글꼴은 운영체제에 등록된 이름이 아니라 읽을 수 있는 TTF·OTF·TTC 파일의 바이트로 넘깁니다.
문서에 글꼴이 내장되어 있으면 엔진이 그 글꼴도 사용합니다. 이름과 기존 대체 규칙으로
찾지 못할 때는 PDF 포함이 허용된 색인 얼굴을 순서대로 살펴봅니다. 일치하는 글꼴이 없거나
글리프가 다르면 배치와 쪽 나눔이 달라질 수 있습니다. 같은 문서를 일관되게 변환하려면
앱이 제공하는 글꼴 파일과 순서를 일정하게 유지하세요. 글꼴 파일은 `.alwaysMapped`로 읽어 넘기면
엔진이 복사하지 않고 매핑된 쪽을 그대로 읽으므로, 한글 글꼴 여러 벌을 넘겨도 앱 메모리에 그
크기가 더해지지 않습니다. 저장소의 Liberation Sans는 시험용이며
한글 글리프를 제공하지 않습니다. 앱에는 사용 권한이 있는 한글 글꼴을 별도로 제공하세요.

`fontFaces.provided`와 `documentEmbedded`는 각각 앱이 넘긴 글꼴과 문서 내장 글꼴에서 색인한
얼굴 수이고 `hasEmbeddableFace`는 PDF에 포함할
수 있는 얼굴이 하나 이상인지 나타냅니다. 이 값들은 문서가 요구하는 모든 글꼴이나 글리프가
있는지 보증하지 않습니다. `warnings`는 파서 경고입니다. 글꼴별 대체와 누락 글리프 목록은
아직 제공하지 않습니다.

## C 소유권과 결과

`ttoryeot-ffi/include/ttoryeot.h`에 전체 계약이 있습니다. 입력 범위는 호출이 끝날 때까지
유효해야 합니다. 성공한 C 호출의 결과는 변환 성공·실패와 관계없이
`ttoryeot_result_free`로 한 번 해제합니다. PDF와 UTF-8 JSON 포인터는 그 결과가 살아 있는
동안만 유효합니다. JSON의 `schema`는 1입니다. `ok`가 거짓이면 `error.kind`와 `error.detail`을,
참이면 `pages`, `warnings`, `fontFaces`를 읽습니다. 잘못된 C 인자는 -1로 돌려줍니다.

## 확인 범위

합성 HWPX의 표 글자와 합성 암호 HWP의 PDF 텍스트, 암호 오류는 `ttoryeot-ffi/ios/renderer-smoke.swift`로
PDFKit에서 확인할 수 있습니다. 켜진 arm64 iPad 시뮬레이터가 있으면 다음 명령으로 기기·
시뮬레이터 링크, Swift 6 동시성 검사와 시뮬레이터 실행을 함께 확인합니다.

```bash
./ttoryeot-ffi/ios/verify-ios.sh dist/Ttoryeot.xcframework <시뮬레이터-ID>
```

선택적으로 세 번째·네 번째 인자에 [공개 표본](accuracy.md#내려받기와-재현)의 `plain.hwp`와
한글 글꼴 파일을 주면 한국어 `안내문` 텍스트와 24쪽도 확인합니다.

이 자료는 한/글 기준 PDF가 아니므로 시각적 일치 시험은
아닙니다. HWP 3.0, HWPML, HWP5 EncryptVersion 1~3은 지원하지 않습니다. 표·쪽 나눔과
글꼴 대체의 정확도 한계는 [정확도 문서](accuracy.md)를 참고하세요.
