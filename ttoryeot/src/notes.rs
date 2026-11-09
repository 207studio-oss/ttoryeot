//! 문서에 남긴 주석(댓글) 묶음. 사람이 요소나 영역을 골라 댓글을 남기면, 에이전트가 그것을 읽고 처리한 뒤
//! 해결로 표시한다. 문서 파일은 건드리지 않고, 문서마다 JSON 파일 하나를 주어진 폴더에 둔다.
//!
//! 같은 폴더로 데스크톱 앱과 에이전트 도구가 문서 열기도 주고받는다(아래 [`OpenRequest`]).
//!
//! 폴더를 정하는 일(운영체제마다 다른 앱 데이터 폴더)은 이 엔진이 아니라 부르는 쪽이 한다.

use crate::components::{Component, Kind, PageComponents};
use crate::scene::Rect;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 저장 형식의 판. 읽을 수 없는 판이면 지우지 않고 오류로 알린다.
pub const FORMAT: u32 = 1;
/// 영역 주석 하나에 적는 대상의 최대 수.
const AREA_TARGETS: usize = 40;

/// 주석이 가리키는 요소 하나.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    /// 요소 종류. `paragraph`, `table`, `cell`, `picture` 같은 영문 키.
    pub kind: String,
    /// 보이는 이름. 예: `문단`, `2행 3열`.
    pub name: String,
    /// 쪽에서의 경로. 예: `표 > 2행 3열 > 문단`.
    pub path: String,
    /// 문단이면 글 전체, 아니면 요약. 없으면 빈 문자열.
    pub text: String,
    /// 쪽 위 자리(pt, 왼쪽 위 원점, y는 아래로): x, y, 너비, 높이.
    pub bounds: [f32; 4],
}

impl Target {
    pub fn from_component(page: &PageComponents, component: &Component) -> Self {
        let text = if component.content.is_empty() {
            component.text.clone()
        } else {
            component.content.clone()
        };
        Self {
            kind: component.kind.key().to_string(),
            name: component.name.clone(),
            path: page.path(component.id),
            text,
            bounds: rect_array(component.bounds),
        }
    }
}

/// 영역 안의 요소 가운데 에이전트에게 쓸모 있는 것만 고른다. 쪽·본문·자리 없는 컨트롤과 빈 문단은 뺀다.
pub fn area_targets<'a>(
    page: &PageComponents,
    components: impl IntoIterator<Item = &'a Component>,
) -> Vec<Target> {
    components
        .into_iter()
        .filter(|c| !matches!(c.kind, Kind::Page | Kind::Body | Kind::Control))
        .filter(|c| c.kind != Kind::Paragraph || !c.content.trim().is_empty())
        .take(AREA_TARGETS)
        .map(|c| Target::from_component(page, c))
        .collect()
}

/// 주석 하나.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// 문서 안에서 1부터 늘어나는 번호. 지운 번호는 다시 쓰지 않는다.
    pub id: u32,
    /// 쪽 번호. 1부터 센다.
    pub page: usize,
    /// 끌어서 고른 영역이면 `true`, 요소 하나를 눌러 골랐으면 `false`.
    pub area: bool,
    /// 고른 자리(pt): x, y, 너비, 높이.
    pub bounds: [f32; 4],
    pub targets: Vec<Target>,
    pub comment: String,
    /// 사람이 적어 둔 바꿀 글. 문단 하나를 골랐을 때만 쓴다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// 남긴 시각(유닉스 초).
    pub created: u64,
    /// 주석을 남길 때 문서 바이트의 지문. 지금 지문과 다르면 문서가 그 뒤에 바뀐 것이다.
    pub document_fingerprint: String,
    #[serde(default)]
    pub resolved: bool,
    /// 해결하며 남긴 말.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

/// 문서 하나의 주석 묶음.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoteBook {
    pub format: u32,
    /// 문서의 절대 경로.
    pub document: String,
    pub next_id: u32,
    pub notes: Vec<Note>,
}

/// 새 주석에 넣을 내용.
pub struct Draft {
    pub page: usize,
    pub area: bool,
    pub bounds: Rect,
    pub targets: Vec<Target>,
    pub comment: String,
    pub replacement: Option<String>,
}

impl NoteBook {
    pub fn new(document: &Path) -> Self {
        Self {
            format: FORMAT,
            document: key(document),
            next_id: 1,
            notes: Vec::new(),
        }
    }

    /// 주석을 더하고 번호를 돌려준다.
    pub fn add(&mut self, draft: Draft, fingerprint: String, created: u64) -> u32 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.notes.push(Note {
            id,
            page: draft.page,
            area: draft.area,
            bounds: rect_array(draft.bounds),
            targets: draft.targets,
            comment: draft.comment,
            replacement: draft.replacement.filter(|text| !text.is_empty()),
            created,
            document_fingerprint: fingerprint,
            resolved: false,
            resolution: None,
        });
        id
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Note> {
        self.notes.iter_mut().find(|note| note.id == id)
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.notes.len();
        self.notes.retain(|note| note.id != id);
        self.notes.len() != before
    }

    pub fn open_count(&self) -> usize {
        self.notes.iter().filter(|note| !note.resolved).count()
    }
}

/// 문서 바이트의 지문(FNV-1a 64비트, 16진수). 같은 바이트면 언제나 같다.
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:016x}", fnv(bytes))
}

/// 문서를 가리키는 키. 실제 파일이면 정규화한 절대 경로다.
pub fn key(document: &Path) -> String {
    std::fs::canonicalize(document)
        .unwrap_or_else(|_| document.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// 문서의 주석 파일 이름. 경로가 같으면 같은 이름이다.
pub fn file_name(document: &Path) -> String {
    format!("{:016x}.json", fnv(key(document).as_bytes()))
}

pub fn file_path(dir: &Path, document: &Path) -> PathBuf {
    dir.join(file_name(document))
}

/// 주석을 읽는다. 아직 없으면 빈 묶음이다.
pub fn load(dir: &Path, document: &Path) -> Result<NoteBook, String> {
    let path = file_path(dir, document);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NoteBook::new(document));
        }
        Err(error) => return Err(format!("주석 파일을 읽지 못했습니다: {error}")),
    };
    let book: NoteBook = serde_json::from_str(&text)
        .map_err(|error| format!("주석 파일을 읽지 못했습니다({}): {error}", path.display()))?;
    if book.format != FORMAT {
        return Err(format!(
            "주석 파일의 판({})을 이 또렷은 읽지 못합니다",
            book.format
        ));
    }
    Ok(book)
}

/// 주석을 쓴다. 같은 폴더의 임시 파일에 다 쓴 뒤 이름을 바꿔 넣어, 읽는 쪽이 반쯤 쓴 파일을 보지 않게 한다.
pub fn save(dir: &Path, book: &NoteBook) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("주석 폴더를 만들지 못했습니다: {error}"))?;
    let path = file_path(dir, Path::new(&book.document));
    let text = serde_json::to_string_pretty(book).map_err(|error| error.to_string())?;
    write_atomic(&path, text.as_bytes())
        .map_err(|error| format!("주석 파일을 쓰지 못했습니다: {error}"))
}

/// 같은 폴더의 임시 파일에 다 쓴 뒤 이름을 바꿔 넣는다.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&temp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })
}

// 데스크톱 앱과 에이전트 도구(MCP 서버) 사이의 문서 열기. 둘은 같은 주석 폴더를 본다.
// 앱은 떠 있는 동안 몇 초마다 [`Presence`]를 고쳐 쓰고, 에이전트는 앱이 떠 있으면 [`OpenRequest`]를
// 요청 폴더에 둔다. 앱은 요청을 가져가며 지우고 그 문서를 탭으로 연다.

/// 앱이 떠 있음을 알리는 파일.
const PRESENCE_FILE: &str = "viewer.json";
/// 열기 요청을 두는 폴더.
const OPEN_DIR: &str = "open";
/// 이 시간(초)보다 오래 소식이 없으면 앱이 꺼진 것으로 본다. 앱은 이보다 자주 알린다.
pub const PRESENCE_TIMEOUT: u64 = 6;

/// 떠 있는 데스크톱 앱.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    /// 마지막으로 알린 시각(유닉스 초).
    pub seen: u64,
    /// 앱의 프로세스 번호. 끝낼 때 자기가 쓴 파일만 지우는 데 쓴다.
    pub process: u32,
}

/// 앱에 문서를 열어 달라는 요청.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenRequest {
    /// 문서의 절대 경로.
    pub path: String,
    /// 보여 줄 쪽. 1부터 센다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
}

/// 앱이 떠 있다고 알린다.
pub fn announce(dir: &Path, presence: &Presence) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let text = serde_json::to_string(presence).map_err(|error| error.to_string())?;
    write_atomic(&dir.join(PRESENCE_FILE), text.as_bytes()).map_err(|error| error.to_string())
}

/// 앱이 끝날 때 부른다. 다른 앱이 그 뒤에 알렸으면 그 알림은 남긴다.
pub fn withdraw(dir: &Path, process: u32) {
    if present(dir).is_some_and(|presence| presence.process == process) {
        let _ = std::fs::remove_file(dir.join(PRESENCE_FILE));
    }
}

/// 마지막 알림. 없거나 읽을 수 없으면 `None`.
pub fn present(dir: &Path) -> Option<Presence> {
    let text = std::fs::read_to_string(dir.join(PRESENCE_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// `now`(유닉스 초)에 앱이 떠 있는가.
pub fn viewer_running(dir: &Path, now: u64) -> bool {
    present(dir).is_some_and(|presence| now.saturating_sub(presence.seen) <= PRESENCE_TIMEOUT)
}

/// 열기 요청을 둔다. `token`은 요청마다 다른 값이어야 한다(예: 시각과 프로세스 번호). 요청 파일 경로를 돌려준다.
pub fn post_open(dir: &Path, request: &OpenRequest, token: &str) -> Result<PathBuf, String> {
    let inbox = dir.join(OPEN_DIR);
    std::fs::create_dir_all(&inbox)
        .map_err(|error| format!("열기 요청 폴더를 만들지 못했습니다: {error}"))?;
    let path = inbox.join(format!("{token}.json"));
    let text = serde_json::to_string(request).map_err(|error| error.to_string())?;
    write_atomic(&path, text.as_bytes())
        .map_err(|error| format!("열기 요청을 쓰지 못했습니다: {error}"))?;
    Ok(path)
}

/// 쌓인 열기 요청을 가져가며 지운다. 앱 둘이 함께 떠 있어도 요청 하나는 한 앱만 가져간다.
pub fn take_open_requests(dir: &Path) -> Vec<OpenRequest> {
    let Ok(entries) = std::fs::read_dir(dir.join(OPEN_DIR)) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok();
            // 지우기에 성공한 쪽만 요청을 가져간다.
            std::fs::remove_file(&path).ok()?;
            serde_json::from_str(&text?).ok()
        })
        .collect()
}

fn rect_array(rect: Rect) -> [f32; 4] {
    [rect.x, rect.y, rect.width, rect.height]
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ttoryeot-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn draft(comment: &str) -> Draft {
        Draft {
            page: 1,
            area: false,
            bounds: Rect::new(10.0, 20.0, 30.0, 40.0),
            targets: vec![Target {
                kind: "paragraph".into(),
                name: "문단".into(),
                path: "표 > 2행 3열 > 문단".into(),
                text: "09:30 시작".into(),
                bounds: [10.0, 20.0, 30.0, 40.0],
            }],
            comment: comment.into(),
            replacement: Some(String::new()),
        }
    }

    #[test]
    fn saves_and_loads_a_book_and_never_reuses_ids() {
        let dir = scratch("roundtrip");
        let document = dir.join("문서.hwpx");
        assert_eq!(load(&dir, &document).unwrap().notes.len(), 0);

        let mut book = NoteBook::new(&document);
        assert_eq!(book.add(draft("10시로"), "f".into(), 1), 1);
        assert_eq!(book.add(draft("굵게"), "f".into(), 2), 2);
        assert!(book.remove(2));
        assert_eq!(book.add(draft("다시"), "f".into(), 3), 3);
        assert_eq!(book.notes[0].replacement, None);
        book.get_mut(1).unwrap().resolved = true;
        save(&dir, &book).unwrap();

        let loaded = load(&dir, &document).unwrap();
        assert_eq!(loaded, book);
        assert_eq!(loaded.open_count(), 1);
        assert!(!dir.join(format!("{}.tmp", file_name(&document))).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_a_broken_file_instead_of_starting_over() {
        let dir = scratch("broken");
        let document = dir.join("문서.hwpx");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file_path(&dir, &document), "{ not json").unwrap();
        assert!(
            load(&dir, &document)
                .unwrap_err()
                .contains("읽지 못했습니다")
        );
        std::fs::write(
            file_path(&dir, &document),
            r#"{"format":99,"document":"x","next_id":1,"notes":[]}"#,
        )
        .unwrap();
        assert!(load(&dir, &document).unwrap_err().contains("판"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hands_open_requests_to_one_running_viewer() {
        let dir = scratch("viewer");
        assert!(!viewer_running(&dir, 100));
        assert!(take_open_requests(&dir).is_empty());

        let presence = Presence {
            seen: 100,
            process: 7,
        };
        announce(&dir, &presence).unwrap();
        assert!(viewer_running(&dir, 100 + PRESENCE_TIMEOUT));
        assert!(!viewer_running(&dir, 101 + PRESENCE_TIMEOUT));

        let first = OpenRequest {
            path: "/문서/가.hwpx".into(),
            page: Some(2),
        };
        let second = OpenRequest {
            path: "/문서/나.hwp".into(),
            page: None,
        };
        let posted = post_open(&dir, &first, "1-7").unwrap();
        post_open(&dir, &second, "2-7").unwrap();
        assert!(posted.exists());
        assert_eq!(take_open_requests(&dir), vec![first, second]);
        assert!(!posted.exists());
        assert!(take_open_requests(&dir).is_empty());

        withdraw(&dir, 8);
        assert_eq!(present(&dir), Some(presence));
        withdraw(&dir, 7);
        assert_eq!(present(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fingerprints_and_file_names_are_stable() {
        assert_eq!(fingerprint(b""), "cbf29ce484222325");
        assert_eq!(fingerprint(b"a"), fingerprint(b"a"));
        assert_ne!(fingerprint(b"a"), fingerprint(b"b"));
        let a = Path::new("/nope/a.hwpx");
        assert_eq!(file_name(a), file_name(a));
        assert_ne!(file_name(a), file_name(Path::new("/nope/b.hwpx")));
        assert!(file_name(a).ends_with(".json"));
    }
}
