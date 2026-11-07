//! 또렷 MCP 서버: HWP/HWPX를 읽어 정보, 텍스트, PDF, PNG를 돌려주고, HWPX는 짧은 글 한 군데를 바꿔 새 HWPX로 저장한다.
//! 원본은 고치지 않는다.

mod raster;

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
use ttoryeot::fonts::FontBook;
use ttoryeot::view::{self, PageSet, TextFormat};

#[global_allocator]
static ALLOCATOR: ttoryeot_alloc::Allocator = ttoryeot_alloc::Allocator;

/// 암호는 이 환경 변수로만 받는다. 도구 인자로 받으면 대화 기록에 남는다.
const PASSWORD_VARIABLE: &str = "TTORYEOT_PASSWORD";
/// 도구 인자는 믿을 수 없다. 장치 파일이나 거대한 파일을 읽느라 서버가 멈추지 않게 막는다.
const MAX_INPUT_BYTES: u64 = 1024 * 1024 * 1024;

const INSTRUCTIONS: &str = "HWP와 HWPX 문서를 읽고, HWPX는 짧은 글 한 군데를 바꿔 새 HWPX로 저장합니다. 원본은 고치지 않습니다. 경로는 절대 경로로 주세요.";

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
    /// 만들 HWPX의 절대 경로. 끝이 .hwpx여야 하고, 이미 있는 파일은 덮어쓰지 않습니다
    output: PathBuf,
    /// 바꿀 글. 서식이 같은 한 조각 안의 짧은 글이어야 합니다
    search: String,
    /// 새 글. 탭과 줄 바꿈은 넣을 수 없습니다
    replacement: String,
    /// 같은 글이 여러 군데일 때 몇 번째인지. 1부터 셉니다. 한 군데뿐이면 생략합니다
    occurrence: Option<usize>,
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
        description = "HWPX 문서의 짧은 글 한 군데를 바꿔 새 HWPX 파일로 저장합니다. 글자 모양, 문단 모양, 줄 간격은 그대로 두고 글만 바꾸며, 이미 있는 파일은 덮어쓰지 않습니다. 같은 글이 여러 군데면 occurrence로 정해야 합니다"
    )]
    async fn replace_text(
        &self,
        Parameters(request): Parameters<ReplaceRequest>,
    ) -> CallToolResult {
        self.run(move |_| {
            let bytes = read_input(&request.path)?;
            let edited = edit::replace_text(
                &bytes,
                &request.search,
                &request.replacement,
                request.occurrence,
            )
            .map_err(|error| explain_edit(error, &bytes, &request.search))?;
            write_new_file(&request.output, "hwpx", &edited)?;
            Ok(vec![json_result(&ReplaceReport {
                output: request.output.display().to_string(),
                occurrence: request.occurrence.unwrap_or(1),
                bytes: edited.len(),
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
