# synthetic.hwpx 출처

`synthetic.hwpx`는 이 프로젝트에서 손으로 만든 최소 HWPX 시험 파일입니다.
실제 문서나 제3자 코드를 담지 않았습니다.

- 쓰는 곳: 파서가 구역 11개, 테두리 채움, 표·머리말 머리 정보를 읽는지 확인하는 시험

이 파일은 OWPML 스키마 검증용도, 중립 IR 직렬화 예제도 아닙니다. 화면 일치(visual parity)는
확인하지 않았습니다.

## synthetic-password-v0.hwp

EncryptVersion 0(DES) 암호 경로를 확인하려고 이 프로젝트에서 만든 최소 HWP5 파일입니다.
암호는 공개 시험용 `12345`이며 실제 사용자 암호가 아닙니다. 첫 문단은
`EncryptVersion 0 synthetic HWP5` 한 줄뿐입니다.

## synthetic-unsupported-encryption-v3.hwp

같은 생성기에서 FileHeader의 EncryptVersion만 3으로 바꾼 파일입니다. 스트림은 v0 형식 그대로라
복호화하지 않고 지원하지 않는 버전 오류를 확인하는 데만 씁니다.

## synthetic-font-families.hwpx

`tools/font-family-fixture.py`로 만든 HWPX입니다. 바탕·휴먼명조·궁서·돋움·굴림·맑은 고딕을 문단마다
하나씩 적고 `<글꼴 이름> 가나다 ABC`를 씁니다. 글꼴 파일은 넣지 않았습니다. 찾지 못한 글꼴이 명조·고딕
계열의 대체 글꼴로 갈리는지 확인하는 시험에 씁니다.
