//! 또렷 MCP 서버: HWP/HWPX를 읽어 정보, 텍스트, PDF, PNG를 돌려주고, HWPX는 짧은 글 한 군데를 바꾼다.
//! 바꾼 문서는 새 HWPX로 저장하거나, 사람과 함께 고칠 때는 백업을 남기고 그 문서를 바로 고친다.
//! 문서를 또렷 데스크톱에 띄우고, 사람이 거기서 남긴 주석(댓글)을 읽고 해결로 표시하며, 쪽의 구성 요소를 찾는다.

mod raster;

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use clap::Parser;
use hwp_core::hwpx::edit::{self, EditError};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::stdio;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use ttoryeot::components::PageComponents;
use ttoryeot::fonts::FontBook;
use ttoryeot::notes::{self, Note, OpenRequest};
use ttoryeot::view::{self, PageSet, TextFormat};

#[global_allocator]
static ALLOCATOR: ttoryeot_alloc::Allocator = ttoryeot_alloc::Allocator;

/// 암호는 이 환경 변수로만 받는다. 도구 인자로 받으면 대화 기록에 남는다.
const PASSWORD_VARIABLE: &str = "TTORYEOT_PASSWORD";
/// 도구 인자는 믿을 수 없다. 장치 파일이나 거대한 파일을 읽느라 서버가 멈추지 않게 막는다.
const MAX_INPUT_BYTES: u64 = 1024 * 1024 * 1024;

const INSTRUCTIONS: &str = "HWP와 HWPX 문서를 읽고, HWPX는 짧은 글 한 군데를 바꿉니다. 바꾼 문서는 output에 새 HWPX로 저장합니다. \
사람과 함께 문서를 고칠 때는 이렇게 합니다: open_in_viewer로 문서를 또렷 데스크톱에 띄우고, 사람이 요소나 영역을 골라 주석(댓글)을 \
남겼다고 하면 list_annotations로 읽고, replace_text를 in_place=true로 불러 그 문서를 바로 고친 뒤(고치기 전 사본은 백업 폴더에 남습니다), \
resolve_annotation으로 무엇을 했는지 적어 해결 표시합니다. 또렷 데스크톱은 고친 문서와 해결 표시를 바로 보여 줍니다. \
쪽의 문단·표·칸·그림은 find_components로 찾습니다. 경로는 절대 경로로 주세요.";
/// 주석 폴더를 바꾸는 환경 변수. 데스크톱 앱과 같은 값을 써야 같은 주석을 본다.
const NOTES_VARIABLE: &str = "TTORYEOT_NOTES_DIR";
/// 한 번에 돌려주는 주석 그림의 최대 수.
const MAX_NOTE_IMAGES: usize = 8;
/// 주석 그림을 자를 때 사방에 더하는 여백(pt).
const NOTE_IMAGE_MARGIN: f32 = 12.0;
/// find_components가 돌려주는 요소의 최대 수.
const MAX_COMPONENTS: usize = 500;
/// 요소 글을 돌려줄 때의 최대 글자 수.
const COMPONENT_TEXT_CHARS: usize = 400;
/// 데스크톱 앱 실행 파일을 직접 정하는 환경 변수. 없으면 운영체제마다 설치된 앱을 찾는다.
const VIEWER_VARIABLE: &str = "TTORYEOT_VIEWER";
#[cfg(target_os = "macos")]
const VIEWER_BUNDLE: &str = "dev.207studio.ttoryeot";
/// 앱을 띄운 뒤 떠 있다는 알림을 기다리는 시간.
const LAUNCH_WAIT: Duration = Duration::from_secs(20);
/// 떠 있는 앱이 열기 요청을 가져가기를 기다리는 시간.
const HANDOFF_WAIT: Duration = Duration::from_secs(5);
/// 바로 고치기 전에 남기는 백업을 문서마다 몇 개까지 두는가. 오래된 것부터 지운다.
const BACKUPS_KEPT: usize = 20;

#[derive(Parser)]
#[command(
    name = "ttoryeot-mcp",
    version,
    about = "또렷 MCP 서버 — 표준 입출력으로 HWP/HWPX 도구를 제공합니다"
)]
struct Cli {
    /// 글꼴 파일이나 폴더. 여러 번 줄 수 있고, 없으면 시스템 글꼴을 찾습니다
    #[arg(long = "font", value_name = "경로")]
    fonts: Vec<PathBuf>,
}

#[derive(Clone)]
struct Server {
    fonts: Arc<Vec<PathBuf>>,
    password: Arc<Option<String>>,
    // 큰 문서 여러 개를 한꺼번에 그리면 메모리가 모자랄 수 있어 한 번에 하나만 처리한다.
    slots: Arc<Semaphore>,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentRequest {
    /// HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum FormatRequest {
    #[default]
    Plain,
    Markdown,
}

#[derive(Deserialize, JsonSchema)]
struct TextRequest {
    /// HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
    /// plain 또는 markdown. 기본은 plain
    #[serde(default)]
    format: FormatRequest,
    /// 쪽 범위. 예: "1-3,7". 없으면 모든 쪽
    pages: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct PdfRequest {
    /// HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
    /// 만들 PDF의 절대 경로. 끝이 .pdf여야 하고, 이미 있는 파일은 덮어쓰지 않습니다
    output: PathBuf,
    /// 쪽 범위. 예: "1-3,7". 없으면 모든 쪽
    pages: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct PngRequest {
    /// HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
    /// 쪽 번호. 1부터 셉니다
    page: u32,
    /// 해상도. 기본은 144
    dpi: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
struct ReplaceRequest {
    /// HWPX 파일의 절대 경로
    path: PathBuf,
    /// 만들 HWPX의 절대 경로. 끝이 .hwpx여야 하고, 이미 있는 파일은 덮어쓰지 않습니다. in_place가 true면 비웁니다
    output: Option<PathBuf>,
    /// true면 새 파일을 만들지 않고 path의 문서를 바로 고칩니다. 고치기 전 사본은 백업 폴더에 남습니다.
    /// 사람과 함께 또렷 데스크톱에서 주석을 보며 고칠 때 씁니다. 기본은 false
    #[serde(default)]
    in_place: bool,
    /// 바꿀 글. 서식이 같은 한 조각 안의 짧은 글이어야 합니다
    search: String,
    /// 새 글. `\n`은 문단 안 줄 바꿈이 됩니다. 탭은 넣을 수 없습니다
    replacement: String,
    /// 같은 글이 여러 군데일 때 몇 번째인지. 1부터 셉니다. 한 군데뿐이면 생략합니다
    occurrence: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
struct AnnotationsRequest {
    /// 또렷 데스크톱에서 주석을 남긴 HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
    /// 해결로 표시한 주석도 돌려줄지. 기본은 false
    #[serde(default)]
    include_resolved: bool,
    /// 주석마다 고른 자리를 잘라 그린 PNG를 함께 돌려줄지. 기본은 true(최대 8장)
    images: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct ResolveRequest {
    /// 주석을 남긴 문서의 절대 경로
    path: PathBuf,
    /// 주석 번호(list_annotations의 id)
    id: u32,
    /// 무엇을 했는지 한두 문장. 예: "09:30을 10:00으로 바꿔 보고서-수정.hwpx로 저장"
    resolution: Option<String>,
    /// false면 해결 표시를 거둡니다. 기본은 true
    resolved: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct ComponentsRequest {
    /// HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
    /// 쪽 번호. 1부터 셉니다
    page: u32,
    /// 이 글이 든 요소만 돌려줍니다
    contains: Option<String>,
    /// 이 점(pt, 쪽 왼쪽 위 원점, y는 아래로)에 놓인 요소만 돌려줍니다. y와 함께 줍니다
    x: Option<f32>,
    y: Option<f32>,
}

#[derive(Deserialize, JsonSchema)]
struct ViewerRequest {
    /// 열 HWP 또는 HWPX 파일의 절대 경로
    path: PathBuf,
    /// 보여 줄 쪽. 1부터 셉니다. 없으면 첫 쪽
    page: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
struct FontsRequest {
    /// 이 문서가 쓰는 글꼴만 확인합니다. 없으면 찾은 글꼴 수와 폴더만 알려 줍니다
    document: Option<PathBuf>,
}

#[derive(Serialize)]
struct TextMeta<'a> {
    page_count: usize,
    pages: &'a [usize],
    warnings: usize,
}

#[derive(Serialize)]
struct PdfReport {
    output: String,
    page_count: usize,
    pages: Vec<usize>,
    bytes: usize,
    warnings: usize,
}

#[derive(Serialize)]
struct PngMeta {
    page: u32,
    page_count: usize,
    dpi: u32,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct ReplaceReport {
    output: String,
    occurrence: usize,
    bytes: usize,
    /// 바로 고쳤을 때 고치기 전 사본의 경로.
    #[serde(skip_serializing_if = "Option::is_none")]
    backup: Option<String>,
}

#[derive(Serialize)]
struct ViewerReport {
    path: String,
    /// `launched`: 또렷을 새로 띄웠다. `running`: 떠 있던 또렷에 탭으로 열었다.
    viewer: &'static str,
    /// 사람이 이 문서에 남기는 주석이 쌓이는 파일.
    notes_file: String,
    next: &'static str,
}

#[derive(Serialize)]
struct NoteView<'a> {
    #[serde(flatten)]
    note: &'a Note,
    /// 주석을 남긴 뒤 문서가 바뀌었는가. 바뀌었으면 자리와 글이 지금 문서와 다를 수 있다.
    document_changed: Option<bool>,
}

#[derive(Serialize)]
struct AnnotationsReport<'a> {
    document: &'a str,
    notes_file: String,
    open: usize,
    resolved: usize,
    notes: Vec<NoteView<'a>>,
    /// 그림을 붙이지 못한 까닭. 문서를 읽지 못했을 때 등.
    #[serde(skip_serializing_if = "Option::is_none")]
    images_skipped: Option<String>,
}

#[derive(Serialize)]
struct ResolveReport {
    id: u32,
    resolved: bool,
    open: usize,
}

#[derive(Serialize)]
struct ComponentView {
    id: usize,
    kind: &'static str,
    name: String,
    path: String,
    text: String,
    bounds: [f32; 4],
    hidden: bool,
    ink: bool,
}

#[derive(Serialize)]
struct ComponentsReport {
    page: u32,
    page_count: usize,
    width: f32,
    height: f32,
    total: usize,
    components: Vec<ComponentView>,
}

#[derive(Serialize)]
struct FontsReport {
    paths: Vec<String>,
    faces: usize,
    fonts: BTreeMap<String, bool>,
}

fn read_input(path: &Path) -> Result<Vec<u8>, String> {
    let failure = |error: &dyn std::fmt::Display| {
        format!("{}을(를) 읽지 못했습니다: {error}", path.display())
    };
    let metadata = std::fs::metadata(path).map_err(|error| failure(&error))?;
    if !metadata.is_file() {
        return Err(failure(&"일반 파일이 아닙니다"));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(failure(&"파일이 너무 큽니다"));
    }
    std::fs::read(path).map_err(|error| failure(&error))
}

fn page_set(pages: Option<&str>) -> Result<Option<PageSet>, String> {
    pages.map(PageSet::parse).transpose()
}

fn no_pages_selected(page_count: usize) -> String {
    format!("고른 쪽이 없습니다. 이 문서는 {page_count}쪽입니다")
}

fn explain(error: ttoryeot::Error) -> String {
    use hwp_core::Fatal;
    match &error {
        ttoryeot::Error::Parse(Fatal::PasswordProtected) => {
            format!(
                "암호가 필요한 문서입니다. 서버를 띄울 때 환경 변수 {PASSWORD_VARIABLE}를 주세요"
            )
        }
        ttoryeot::Error::Parse(Fatal::WrongPassword) => {
            format!("{PASSWORD_VARIABLE}의 암호가 이 문서와 맞지 않습니다")
        }
        _ => error.to_string(),
    }
}

/// 원본을 지키려고 있는 파일은 어떤 것도 덮어쓰지 않는다. 원본도 여기에 포함된다.
fn write_new_file(path: &Path, extension: &str, content: &[u8]) -> Result<(), String> {
    let failure = |error: &dyn std::fmt::Display| {
        format!("{}을(를) 쓰지 못했습니다: {error}", path.display())
    };
    if !path
        .extension()
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
    {
        return Err(failure(&format!("경로의 끝이 .{extension}여야 합니다")));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| failure(&error))?;
    file.write_all(content).map_err(|error| {
        let _ = std::fs::remove_file(path);
        failure(&error)
    })
}

/// 있는 파일을 바꾼다. 같은 폴더의 임시 파일에 다 쓴 뒤 이름을 바꿔 넣어, 쓰다 실패해도 문서가 깨지지 않는다.
fn replace_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let failure = |error: &dyn std::fmt::Display| {
        format!("{}을(를) 쓰지 못했습니다: {error}", path.display())
    };
    let name = path
        .file_name()
        .ok_or_else(|| failure(&"파일 이름이 없습니다"))?
        .to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.ttoryeot-tmp"));
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(content)?;
        file.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            failure(&error)
        })
}

/// 바로 고치기 전 문서를 주석 폴더의 `backups`에 남기고 그 경로를 돌려준다. 문서마다 최근 [`BACKUPS_KEPT`]개만 둔다.
fn back_up(path: &Path, content: &[u8]) -> Result<PathBuf, String> {
    let dir = notes_dir()?.join("backups");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("백업 폴더를 만들지 못했습니다: {error}"))?;
    let key = notes::file_name(path);
    let key = key.trim_end_matches(".json");
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let backup = dir.join(format!("{key}-{:020}-{stem}.hwpx", unix_nanos()));
    write_new_file(&backup, "hwpx", content)?;
    let mut kept: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(&format!("{key}-")))
                })
                .collect()
        })
        .unwrap_or_default();
    kept.sort();
    let excess = kept.len().saturating_sub(BACKUPS_KEPT);
    for old in &kept[..excess] {
        let _ = std::fs::remove_file(old);
    }
    Ok(backup)
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// `done`이 참이 될 때까지 `limit` 동안 기다린다.
fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = std::time::Instant::now();
    loop {
        if done() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 또렷 데스크톱을 띄워 문서를 연다. 띄우기만 하고 기다리지 않는다.
fn launch_viewer(path: &Path) -> Result<(), String> {
    let missing = || {
        format!(
            "또렷 데스크톱 앱을 찾지 못했습니다. 앱을 설치하거나 환경 변수 {VIEWER_VARIABLE}에 앱 실행 파일 경로를 주세요"
        )
    };
    let failed = |error: std::io::Error| format!("또렷 데스크톱을 띄우지 못했습니다: {error}");
    // MCP 서버의 표준 입출력은 대화 통로라 앱에 물려주지 않는다.
    let detached = |command: &mut Command| {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(command, 0);
    };
    if let Some(program) = std::env::var_os(VIEWER_VARIABLE).filter(|value| !value.is_empty()) {
        let mut command = Command::new(program);
        command.arg(path);
        detached(&mut command);
        return command.spawn().map(drop).map_err(failed);
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/open");
        command.args(["-b", VIEWER_BUNDLE, "--args"]).arg(path);
        detached(&mut command);
        let status = command.status().map_err(failed)?;
        if status.success() {
            Ok(())
        } else {
            Err(missing())
        }
    }
    #[cfg(windows)]
    {
        let program = std::env::var_os("LOCALAPPDATA")
            .map(|base| {
                PathBuf::from(base)
                    .join("Programs")
                    .join("또렷")
                    .join("또렷.exe")
            })
            .filter(|program| program.is_file())
            .ok_or_else(missing)?;
        let mut command = Command::new(program);
        command.arg(path);
        detached(&mut command);
        command.spawn().map(drop).map_err(failed)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut command = Command::new("ttoryeot-viewer");
        command.arg(path);
        detached(&mut command);
        command
            .spawn()
            .map(drop)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => missing(),
                _ => failed(error),
            })
    }
}

/// 떠 있는 또렷에 열기 요청을 두고 가져가기를 기다린다. 가져가지 않으면 요청을 거두고 `false`.
fn hand_over(dir: &Path, request: &OpenRequest) -> Result<bool, String> {
    let token = format!("{:020}-{}", unix_nanos(), std::process::id());
    let posted = notes::post_open(dir, request, &token)?;
    if wait_until(HANDOFF_WAIT, || !posted.exists()) {
        return Ok(true);
    }
    let _ = std::fs::remove_file(&posted);
    Ok(false)
}

fn open_in_viewer(request: ViewerRequest) -> Result<ViewerReport, String> {
    let path = std::fs::canonicalize(&request.path)
        .map_err(|error| format!("{}을(를) 찾지 못했습니다: {error}", request.path.display()))?;
    if !path.is_file() {
        return Err(format!("{}은(는) 일반 파일이 아닙니다", path.display()));
    }
    let known = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("hwp") || extension.eq_ignore_ascii_case("hwpx")
    });
    if !known {
        return Err("또렷은 .hwp와 .hwpx 문서만 엽니다".to_string());
    }
    if request.page == Some(0) {
        return Err("쪽은 1부터 셉니다".to_string());
    }
    let dir = notes_dir()?;
    let open = OpenRequest {
        path: path.display().to_string(),
        page: request.page,
    };
    let viewer = if notes::viewer_running(&dir, unix_seconds()) {
        if !hand_over(&dir, &open)? {
            return Err(format!(
                "또렷 데스크톱이 떠 있지만 {}초 안에 요청을 받지 않았습니다. 또렷 창이 멈췄는지 확인해 주세요",
                HANDOFF_WAIT.as_secs()
            ));
        }
        "running"
    } else {
        launch_viewer(&path)?;
        if !wait_until(LAUNCH_WAIT, || notes::viewer_running(&dir, unix_seconds())) {
            return Err(format!(
                "또렷 데스크톱을 띄웠지만 {}초 안에 응답이 없습니다. 에이전트와 함께 쓰는 기능이 든 또렷인지 확인해 주세요",
                LAUNCH_WAIT.as_secs()
            ));
        }
        // 띄울 때 넘긴 문서는 앱이 스스로 연다. 쪽을 정했으면 그 쪽으로 옮기도록 요청을 한 번 더 둔다.
        if open.page.is_some() {
            hand_over(&dir, &open)?;
        }
        "launched"
    };
    Ok(ViewerReport {
        notes_file: notes::file_path(&dir, &path).display().to_string(),
        path: open.path,
        viewer,
        next: "사람이 또렷에서 주석을 남겼다고 하면 list_annotations로 읽고, replace_text(in_place=true)로 고친 뒤 resolve_annotation으로 해결 표시하세요",
    })
}

fn explain_edit(error: EditError, bytes: &[u8], search: &str) -> String {
    let EditError::Ambiguous { count } = error else {
        return error.to_string();
    };
    let mut message = format!(
        "같은 글이 {count}군데 있습니다. occurrence에 몇 번째인지 1~{count} 중에서 정해 주세요."
    );
    for found in edit::find_text(bytes, search).unwrap_or_default() {
        message.push_str(&format!(
            "\n{}번째 (구역 {}): {}[{search}]{} / 문단 시작 \"{}\"",
            found.occurrence, found.section, found.before, found.after, found.paragraph
        ));
    }
    message
}

fn default_font_paths() -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut paths: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/System/Library/Fonts"));
        paths.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        paths.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/share/fonts"));
        paths.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(&home).join(".local/share/fonts"));
            paths.push(PathBuf::from(&home).join(".fonts"));
        }
    }
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("WINDIR") {
            paths.push(PathBuf::from(root).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            paths.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    paths.retain(|path| path.is_dir() && seen.insert(path.clone()));
    paths
}

/// 데스크톱 앱이 주석을 두는 폴더. 두 쪽이 같은 규칙을 쓴다.
fn notes_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(NOTES_VARIABLE).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let home = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    #[cfg(target_os = "macos")]
    let base = home("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(windows)]
    let base = home("APPDATA").map(PathBuf::from);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = home("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    base.map(|base| base.join("Ttoryeot").join("notes"))
        .ok_or_else(|| format!("주석 폴더를 정하지 못했습니다. {NOTES_VARIABLE}를 주세요"))
}

fn component_text(component: &ttoryeot::components::Component) -> String {
    let text = if component.content.is_empty() {
        &component.text
    } else {
        &component.content
    };
    text.chars().take(COMPONENT_TEXT_CHARS).collect()
}

fn component_view(
    page: &PageComponents,
    component: &ttoryeot::components::Component,
) -> ComponentView {
    let b = component.bounds;
    ComponentView {
        id: component.id,
        kind: component.kind.key(),
        name: component.name.clone(),
        path: page.path(component.id),
        text: component_text(component),
        bounds: [b.x, b.y, b.width, b.height],
        hidden: component.hidden,
        ink: component.ink,
    }
}

fn json_result<T: Serialize>(value: &T) -> Result<ContentBlock, String> {
    serde_json::to_string(value)
        .map(ContentBlock::text)
        .map_err(|error| error.to_string())
}

impl Server {
    fn new(fonts: Vec<PathBuf>, password: Option<String>) -> Self {
        Self {
            fonts: Arc::new(fonts),
            password: Arc::new(password),
            slots: Arc::new(Semaphore::new(1)),
            tool_router: Self::tool_router(),
        }
    }

    fn load_fonts(&self) -> Result<FontBook, String> {
        let fonts = FontBook::scan(&self.fonts);
        if fonts.face_count() == 0 {
            return Err(
                "글꼴을 하나도 찾지 못했습니다. 서버에 --font로 글꼴 폴더를 주세요".to_string(),
            );
        }
        Ok(fonts)
    }

    /// 주석마다 고른 자리를 여백과 함께 잘라 그린다. 앞에서부터 최대 [`MAX_NOTE_IMAGES`]장이다.
    fn note_images(
        &self,
        bytes: &[u8],
        notes: &[&Note],
    ) -> Result<Vec<(u32, raster::PageImage)>, String> {
        let mut fonts = self.load_fonts()?;
        let output =
            ttoryeot::to_pdf(bytes, self.password.as_deref(), &mut fonts).map_err(explain)?;
        let picked: Vec<&Note> = notes.iter().copied().take(MAX_NOTE_IMAGES).collect();
        let requests: Vec<raster::Crop> = picked
            .iter()
            .map(|note| {
                let [x, y, w, h] = note.bounds;
                raster::Crop {
                    page: note.page.saturating_sub(1),
                    rect: [
                        x - NOTE_IMAGE_MARGIN,
                        y - NOTE_IMAGE_MARGIN,
                        w + 2.0 * NOTE_IMAGE_MARGIN,
                        h + 2.0 * NOTE_IMAGE_MARGIN,
                    ],
                }
            })
            .collect();
        let images = raster::crops(output.pdf, raster::DEFAULT_DPI, &requests)?;
        Ok(picked.iter().map(|note| note.id).zip(images).collect())
    }

    /// 문서 처리는 CPU만 쓰므로 비동기 실행기를 막지 않게 따로 돌린다.
    async fn run(
        &self,
        work: impl FnOnce(&Self) -> Result<Vec<ContentBlock>, String> + Send + 'static,
    ) -> CallToolResult {
        let server = self.clone();
        let outcome = async {
            let _slot = server
                .slots
                .clone()
                .acquire_owned()
                .await
                .map_err(|error| error.to_string())?;
            tokio::task::spawn_blocking(move || work(&server))
                .await
                .map_err(|_| "문서를 처리하다 내부 오류가 났습니다".to_string())?
        };
        match outcome.await {
            Ok(content) => CallToolResult::success(content),
            Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
        }
    }
}

#[tool_router]
impl Server {
    #[tool(description = "문서의 형식, 쪽 수, 메타데이터, 사용 글꼴, 암호화 여부를 알려 줍니다")]
    async fn read_document_info(
        &self,
        Parameters(request): Parameters<DocumentRequest>,
    ) -> CallToolResult {
        self.run(move |server| {
            let bytes = read_input(&request.path)?;
            let mut fonts = server.load_fonts()?;
            let report =
                view::inspect(&bytes, server.password.as_deref(), &mut fonts).map_err(explain)?;
            Ok(vec![json_result(&report)?])
        })
        .await
    }

    #[tool(
        description = "문서의 글을 평문이나 markdown으로 뽑습니다. 평문은 쪽을 폼 피드로 나눕니다"
    )]
    async fn extract_text(&self, Parameters(request): Parameters<TextRequest>) -> CallToolResult {
        self.run(move |server| {
            let pages = page_set(request.pages.as_deref())?;
            let bytes = read_input(&request.path)?;
            let mut fonts = server.load_fonts()?;
            let format = match request.format {
                FormatRequest::Plain => TextFormat::Plain,
                FormatRequest::Markdown => TextFormat::Markdown,
            };
            let extracted = view::extract_text(
                &bytes,
                server.password.as_deref(),
                &mut fonts,
                format,
                pages.as_ref(),
            )
            .map_err(explain)?;
            if extracted.pages.is_empty() {
                return Err(no_pages_selected(extracted.page_count));
            }
            let meta = json_result(&TextMeta {
                page_count: extracted.page_count,
                pages: &extracted.pages,
                warnings: extracted.warnings.len(),
            })?;
            Ok(vec![ContentBlock::text(extracted.text), meta])
        })
        .await
    }

    #[tool(description = "문서를 새 PDF 파일로 만듭니다. 이미 있는 파일은 덮어쓰지 않습니다")]
    async fn render_pdf(&self, Parameters(request): Parameters<PdfRequest>) -> CallToolResult {
        self.run(move |server| {
            let pages = page_set(request.pages.as_deref())?;
            let bytes = read_input(&request.path)?;
            let mut fonts = server.load_fonts()?;
            let selected = view::selected_pdf(
                &bytes,
                server.password.as_deref(),
                &mut fonts,
                pages.as_ref(),
            )
            .map_err(explain)?;
            if selected.pages.is_empty() {
                return Err(no_pages_selected(selected.page_count));
            }
            write_new_file(&request.output, "pdf", &selected.pdf)?;
            Ok(vec![json_result(&PdfReport {
                output: request.output.display().to_string(),
                page_count: selected.page_count,
                bytes: selected.pdf.len(),
                pages: selected.pages,
                warnings: selected.warnings.len(),
            })?])
        })
        .await
    }

    #[tool(description = "문서의 한 쪽을 PNG 이미지로 돌려줍니다")]
    async fn render_page_png(&self, Parameters(request): Parameters<PngRequest>) -> CallToolResult {
        self.run(move |server| {
            let dpi = request.dpi.unwrap_or(raster::DEFAULT_DPI);
            let pages = PageSet::parse(&request.page.to_string())?;
            let bytes = read_input(&request.path)?;
            let mut fonts = server.load_fonts()?;
            let selected =
                view::selected_pdf(&bytes, server.password.as_deref(), &mut fonts, Some(&pages))
                    .map_err(explain)?;
            if selected.pages.is_empty() {
                return Err(no_pages_selected(selected.page_count));
            }
            let mut content = Vec::new();
            raster::render_pages(selected.pdf, dpi, |_, image| {
                let encoded = base64::engine::general_purpose::STANDARD.encode(&image.png);
                content.push(ContentBlock::image(encoded, "image/png"));
                content.push(json_result(&PngMeta {
                    page: request.page,
                    page_count: selected.page_count,
                    dpi,
                    width: image.width,
                    height: image.height,
                })?);
                Ok(())
            })?;
            Ok(content)
        })
        .await
    }

    #[tool(
        description = "HWPX 문서의 짧은 글 한 군데를 바꿉니다. 글자 모양, 문단 모양, 줄 간격은 그대로 두고 글만 바꾸며, 글자 수가 달라지면 그 문단의 줄은 다시 나눕니다. 보통은 output에 새 HWPX로 저장하고 이미 있는 파일은 덮어쓰지 않습니다. 사람과 함께 또렷 데스크톱에서 주석을 보며 고칠 때는 in_place=true로 그 문서를 바로 고칩니다(고치기 전 사본은 백업 폴더에 남고, 또렷은 고친 문서를 바로 다시 보여 줍니다). 같은 글이 여러 군데면 occurrence로 정해야 합니다"
    )]
    async fn replace_text(
        &self,
        Parameters(request): Parameters<ReplaceRequest>,
    ) -> CallToolResult {
        self.run(move |_| {
            let output = match (request.in_place, &request.output) {
                (false, Some(output)) => Some(output.clone()),
                (true, None) => None,
                (true, Some(_)) => {
                    return Err("in_place가 true면 output을 비워 주세요".to_string());
                }
                (false, None) => {
                    return Err(
                        "output에 새 HWPX 경로를 주거나, 이 문서를 바로 고치려면 in_place를 true로 주세요"
                            .to_string(),
                    );
                }
            };
            let bytes = read_input(&request.path)?;
            let edited = edit::replace_text(
                &bytes,
                &request.search,
                &request.replacement,
                request.occurrence,
            )
            .map_err(|error| explain_edit(error, &bytes, &request.search))?;
            let (written, backup) = match output {
                Some(output) => {
                    write_new_file(&output, "hwpx", &edited)?;
                    (output, None)
                }
                None => {
                    let path = std::fs::canonicalize(&request.path).map_err(|error| {
                        format!("{}을(를) 찾지 못했습니다: {error}", request.path.display())
                    })?;
                    if !path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("hwpx"))
                    {
                        return Err("바로 고칠 수 있는 것은 .hwpx 문서뿐입니다".to_string());
                    }
                    let backup = back_up(&path, &bytes)?;
                    replace_file(&path, &edited)?;
                    (path, Some(backup.display().to_string()))
                }
            };
            Ok(vec![json_result(&ReplaceReport {
                output: written.display().to_string(),
                occurrence: request.occurrence.unwrap_or(1),
                bytes: edited.len(),
                backup,
            })?])
        })
        .await
    }

    #[tool(
        description = "문서를 또렷 데스크톱 앱에 띄웁니다. 앱이 꺼져 있으면 켜고, 켜져 있으면 새 탭으로 열어 앞으로 가져옵니다. 사람이 문서를 보며 요소나 영역에 주석(댓글)을 남기게 할 때 씁니다. 주석은 list_annotations로 읽습니다"
    )]
    async fn open_in_viewer(
        &self,
        Parameters(request): Parameters<ViewerRequest>,
    ) -> CallToolResult {
        // 앱을 기다리는 동안 문서 처리 자리를 잡지 않는다.
        let outcome = tokio::task::spawn_blocking(move || open_in_viewer(request))
            .await
            .map_err(|_| "또렷 데스크톱을 여는 중 내부 오류가 났습니다".to_string())
            .and_then(|result| result)
            .and_then(|report| json_result(&report));
        match outcome {
            Ok(content) => CallToolResult::success(vec![content]),
            Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
        }
    }

    #[tool(
        description = "사람이 또렷 데스크톱의 주석 모드에서 문서의 요소나 영역을 골라 남긴 주석(댓글)을 돌려줍니다. 주석마다 번호, 쪽, 고른 자리(pt), 대상 요소의 경로(예: 표 > 2행 3열 > 문단)와 글, 댓글, 적어 둔 바꿀 글, 해결 여부, 남긴 뒤 문서가 바뀌었는지가 있고, 고른 부분을 잘라 그린 PNG도 붙습니다. 댓글은 고쳐 달라는 요청이므로 처리한 뒤 resolve_annotation으로 해결 표시하세요"
    )]
    async fn list_annotations(
        &self,
        Parameters(request): Parameters<AnnotationsRequest>,
    ) -> CallToolResult {
        self.run(move |server| {
            let dir = notes_dir()?;
            let book = notes::load(&dir, &request.path)?;
            let bytes = read_input(&request.path);
            let current = bytes.as_ref().ok().map(|bytes| notes::fingerprint(bytes));
            let shown: Vec<&Note> = book
                .notes
                .iter()
                .filter(|note| request.include_resolved || !note.resolved)
                .collect();
            let mut images_skipped = None;
            let mut images = Vec::new();
            if request.images.unwrap_or(true) && !shown.is_empty() {
                match bytes.as_ref() {
                    Ok(bytes) => match server.note_images(bytes, &shown) {
                        Ok(found) => images = found,
                        Err(message) => images_skipped = Some(message),
                    },
                    Err(message) => images_skipped = Some(message.clone()),
                }
            }
            let report = AnnotationsReport {
                document: &book.document,
                notes_file: notes::file_path(&dir, &request.path).display().to_string(),
                open: book.open_count(),
                resolved: book.notes.len() - book.open_count(),
                notes: shown
                    .iter()
                    .map(|note| NoteView {
                        note,
                        document_changed: current
                            .as_ref()
                            .map(|current| *current != note.document_fingerprint),
                    })
                    .collect(),
                images_skipped,
            };
            let mut content = vec![json_result(&report)?];
            for (id, image) in images {
                content.push(ContentBlock::text(format!("주석 {id}의 자리")));
                content.push(ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(&image.png),
                    "image/png",
                ));
            }
            Ok(content)
        })
        .await
    }

    #[tool(
        description = "주석을 처리했으면 해결로 표시합니다. 무엇을 했는지 resolution에 적어 두면 사람이 또렷 데스크톱에서 봅니다. resolved=false면 해결 표시를 거둡니다"
    )]
    async fn resolve_annotation(
        &self,
        Parameters(request): Parameters<ResolveRequest>,
    ) -> CallToolResult {
        self.run(move |_| {
            let dir = notes_dir()?;
            let mut book = notes::load(&dir, &request.path)?;
            let resolved = request.resolved.unwrap_or(true);
            let note = book
                .get_mut(request.id)
                .ok_or_else(|| format!("주석 {}이(가) 없습니다", request.id))?;
            note.resolved = resolved;
            note.resolution = if resolved {
                request.resolution.filter(|text| !text.trim().is_empty())
            } else {
                None
            };
            notes::save(&dir, &book)?;
            Ok(vec![json_result(&ResolveReport {
                id: request.id,
                resolved,
                open: book.open_count(),
            })?])
        })
        .await
    }

    #[tool(
        description = "쪽의 구성 요소(문단, 표, 칸, 그림, 도형, 머리말 등)를 종류, 이름, 경로(예: 표 > 2행 3열 > 문단), 글, 자리(pt)와 함께 돌려줍니다. contains로 글을, x와 y로 한 점을 걸러 찾습니다"
    )]
    async fn find_components(
        &self,
        Parameters(request): Parameters<ComponentsRequest>,
    ) -> CallToolResult {
        self.run(move |server| {
            let bytes = read_input(&request.path)?;
            let mut fonts = server.load_fonts()?;
            let (_, pages) =
                ttoryeot::to_pdf_with_components(&bytes, server.password.as_deref(), &mut fonts)
                    .map_err(explain)?;
            let page_count = pages.len();
            let page = (request.page as usize)
                .checked_sub(1)
                .and_then(|index| pages.get(index))
                .ok_or_else(|| {
                    format!(
                        "{}쪽이 없습니다. 이 문서는 {page_count}쪽입니다",
                        request.page
                    )
                })?;
            let point = match (request.x, request.y) {
                (Some(x), Some(y)) => Some((x, y)),
                (None, None) => None,
                _ => return Err("x와 y는 함께 주세요".to_string()),
            };
            let needle = request.contains.as_deref().filter(|text| !text.is_empty());
            let matching: Vec<_> = page
                .components
                .iter()
                .filter(|c| point.is_none_or(|(x, y)| c.contains(x, y, 2.0)))
                .filter(|c| {
                    needle
                        .is_none_or(|needle| c.content.contains(needle) || c.text.contains(needle))
                })
                .collect();
            Ok(vec![json_result(&ComponentsReport {
                page: request.page,
                page_count,
                width: page.width,
                height: page.height,
                total: matching.len(),
                components: matching
                    .iter()
                    .take(MAX_COMPONENTS)
                    .map(|c| component_view(page, c))
                    .collect(),
            })?])
        })
        .await
    }

    #[tool(
        description = "서버가 찾은 글꼴 폴더와 개수를 알려 주고, 문서를 주면 그 문서의 글꼴이 있는지 확인합니다"
    )]
    async fn list_fonts(&self, Parameters(request): Parameters<FontsRequest>) -> CallToolResult {
        self.run(move |server| {
            let mut book = server.load_fonts()?;
            let mut fonts = BTreeMap::new();
            if let Some(path) = &request.document {
                let bytes = read_input(path)?;
                let options = hwp_core::ParseOptions {
                    password: server.password.as_ref().clone(),
                    ..hwp_core::ParseOptions::default()
                };
                let parsed = hwp_core::parse(&bytes, &options)
                    .map_err(|error| explain(ttoryeot::Error::Parse(error)))?;
                book.attach_document(&parsed.document);
                for name in view::used_fonts(&parsed.document) {
                    let present = book.find(&name, false).is_some();
                    fonts.insert(name, present);
                }
            }
            Ok(vec![json_result(&FontsReport {
                paths: server
                    .fonts
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                faces: book.face_count(),
                fonts,
            })?])
        })
        .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "ttoryeot-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let fonts = if cli.fonts.is_empty() {
        default_font_paths()
    } else {
        cli.fonts
    };
    let password = std::env::var(PASSWORD_VARIABLE)
        .ok()
        .filter(|value| !value.is_empty());
    let service = Server::new(fonts, password).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
