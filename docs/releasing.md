# 릴리스

> 이 저장소는 GitHub Actions를 쓰지 않습니다. 검사와 묶음은 모두 로컬에서 만듭니다.

## 준비

1. `Cargo.toml`의 `[workspace.package] version`을 올립니다. 크레이트는 이 값을 함께 씁니다.
2. `CHANGELOG.md`에 바뀐 점과 검증 결과를 적습니다.
3. `tools/check.sh`(서식, clippy, 시험, 릴리스 빌드, 경계 검사)와 `tools/check-publish.sh`를 통과시킵니다.
4. 커밋하고 `main`에 올립니다.

## 묶음 만들기

```bash
tools/release-local.sh v0.3.0 dist/
```

macOS arm64·x86_64, Windows x86_64(mingw-w64 교차 빌드), Linux x86_64(cargo-zigbuild, glibc 2.17)용 묶음과
`SHA256SUMS`를 만듭니다. 묶음에는 실행 파일(`ttoryeot`, `ttoryeot-mcp`), README, LICENSING.md,
LICENSE-AGPL-3.0, `ttoryeot-alloc/LICENSE-MIT`·`ttoryeot-alloc/LICENSE-APACHE`(할당자), THIRD_PARTY_NOTICES, CHANGELOG, 의존 크레이트
라이선스 묶음(krilla 포함, `third-party/dependencies.md`), 빌드 정보(`BUILD.txt`)가 들어갑니다. 스크립트는 아무것도 올리지 않습니다.

## 태그와 GitHub 릴리스

```bash
git tag -a v0.3.0 -m "또렷 0.3.0" && git push origin v0.3.0
gh release create v0.3.0 dist/*.tar.gz dist/*.zip dist/SHA256SUMS --title "또렷 v0.3.0" --verify-tag
```

이미 받은 사람이 있으면 같은 버전을 덮지 말고 패치 버전을 올립니다.

## crates.io

`hwp-core` 이름은 다른 사람이 이미 쓰고 있어 패키지 이름은 `ttoryeot-hwp-core`입니다. 라이브러리
이름은 그대로 `hwp_core`입니다. 패치한 krilla도 `ttoryeot-krilla`로 올립니다. 의존 순서대로 올립니다.

```bash
(cd vendor/krilla && cargo publish)
cargo publish -p ttoryeot-hwp-core
cargo publish -p ttoryeot
cargo publish -p ttoryeot-alloc
cargo publish -p ttoryeot-cli
cargo publish -p ttoryeot-mcp
```

`tools/check-publish.sh`는 올리지 않고 같은 순서를 `--dry-run`으로 확인합니다. 의존 크레이트 라이선스가
바뀌었다면 먼저 `tools/licenses.sh`로 다시 만듭니다.
