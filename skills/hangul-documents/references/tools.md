# 또렷 도구

공개 저장소: <https://github.com/207studio-oss/ttoryeot> (엔진·CLI·MCP, AGPL-3.0-only. 엔진 라이브러리는 상용 라이선스도 고를 수 있다).
미리 빌드한 실행 파일은 Releases에 운영체제별로 있고, macOS 실행 파일은 Apple 공증을 받았다.

## MCP 서버 `ttoryeot-mcp`

```bash
claude mcp add --scope user ttoryeot -- ttoryeot-mcp            # Claude Code
codex mcp add ttoryeot -- ttoryeot-mcp                           # Codex
```

| 도구 | 하는 일 |
|---|---|
| `read_document_info` | 형식, 쪽 수, 메타데이터, 글꼴, 암호 여부 |
| `extract_text` | 평문(쪽은 폼 피드) 또는 markdown, 쪽 범위 |
| `render_pdf` / `render_page_png` | 새 PDF 파일 / 한 쪽 PNG |
| `find_components` | 쪽의 문단·표·칸·그림 등의 종류·경로(예: `표 > 2행 3열 > 문단`)·글·자리(pt). `contains`·`x,y`로 거른다 |
| `replace_text` | HWPX 짧은 글 바꾸기. `output`에 새 HWPX, 또는 `in_place=true`로 그 문서를 바로(사본은 백업 폴더). `\n`은 줄 바꿈 |
| `open_in_viewer` | 문서를 또렷 데스크톱 앱에 띄운다(꺼져 있으면 켜고, 켜져 있으면 새 탭) |
| `list_annotations` / `resolve_annotation` | 사람이 데스크톱 앱에서 남긴 주석(고른 자리·대상 요소·댓글·잘라 그린 그림) 읽기 / 해결 표시 |
| `list_fonts` | 찾은 글꼴과 문서 글꼴의 설치 여부 |

- 암호는 도구 인자로 받지 않는다. 서버를 띄울 때 환경 변수 `TTORYEOT_PASSWORD`로 준다.
- 경로는 절대 경로로 준다. `render_pdf`·`replace_text(output)`는 있는 파일을 덮어쓰지 않는다.
- 주석은 앱 데이터 폴더(macOS `~/Library/Application Support/Ttoryeot/notes`, Windows `%APPDATA%\Ttoryeot\notes`,
  그 밖 `$XDG_DATA_HOME/ttoryeot/notes`)에 문서마다 JSON 하나로 있다. `TTORYEOT_NOTES_DIR`로 바꾼다.

## 사람과 함께 고치기

1. 사람이 "또렷에서 ○○ 열어 줘" → `open_in_viewer`.
2. 사람이 데스크톱 앱의 주석 모드(⌘⇧A)에서 문단·표·그림이나 영역을 골라 댓글을 남긴다.
3. "주석 반영해 줘" → `list_annotations`로 읽고, 댓글대로 `replace_text(in_place=true)`로 고친다. 앱은 바뀐 파일을 바로 다시 그린다.
4. `resolve_annotation`에 무엇을 했는지 한두 문장으로 적어 해결 표시한다. 고칠 수 없으면 해결하지 말고 까닭을 말한다.

## CLI `ttoryeot`

```bash
ttoryeot info 문서.hwp --json
ttoryeot text 문서.hwpx --format markdown --pages 1-3
ttoryeot render 문서.hwp -o 문서.pdf --font ~/Library/Fonts
ttoryeot png 문서.hwpx --pages 1 --dpi 144 -o 그림폴더
ttoryeot fonts
```

## 데스크톱 앱(무료, 소스 비공개)

<https://github.com/207studio-oss/ttoryeot-desktop/releases/latest> — macOS(공증 dmg), Windows(설치 파일·zip).
HWP·HWPX를 탭으로 열고, HWPX는 글자를 눌러 보이는 그대로 고치며(Ctrl+Enter·Shift+Enter 줄 바꿈), 주석을 남긴다.
