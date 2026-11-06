//! 또렷 명령줄 도구: HWP/HWPX 문서를 읽어 PDF, PNG, 텍스트로 내보낸다. 원본은 고치지 않는다.

mod raster;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use ttoryeot::fonts::FontBook;
use ttoryeot::view::{self, PageSet, TextFormat};

#[global_allocator]
static ALLOCATOR: ttoryeot_alloc::Allocator = ttoryeot_alloc::Allocator;

/// 기본으로 설치되어 있는지 확인할 만한 한/글 글꼴 이름.
const HANCOM_FONTS: &[&str] = &[
    "함초롬바탕",
    "함초롬돋움",
    "함초롬돋움 LVT",
    "바탕",
    "돋움",
    "굴림",
    "맑은 고딕",
    "HY신명조",
];

#[derive(Parser)]
#[command(
    name = "ttoryeot",
    version,
    about = "또렷 — HWP/HWPX 문서를 한/글과 같은 모양의 PDF로 그립니다",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// HWP/HWPX 문서를 PDF로 그립니다
    Render(RenderArgs),
    /// 쪽을 PNG 이미지로 그립니다
    Png(PngArgs),
    /// 문서의 글자를 평문이나 markdown으로 꺼냅니다
    Text(TextArgs),
    /// 쪽 수, 문서 속성, 사용 글꼴, 암호화 여부를 보여 줍니다
    Info(InfoArgs),
    /// 이 컴퓨터에서 찾은 글꼴과 한/글 글꼴 설치 상태를 확인합니다
    Fonts(FontsArgs),
}

#[derive(Args)]
struct RenderArgs {
    /// HWP 또는 HWPX 파일
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// PDF를 쓸 경로 (기본: 입력과 같은 이름의 .pdf)
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// 글꼴 폴더나 글꼴 파일. 여러 번 쓸 수 있고, 없으면 시스템 글꼴을 씁니다
    #[arg(long = "font", value_name = "PATH")]
    fonts: Vec<PathBuf>,

    /// 암호를 읽을 환경 변수 이름. 암호를 인자로 받으면 셸 기록에 남습니다
    #[arg(long, value_name = "VAR")]
    password_env: Option<String>,

    /// 결과를 JSON 한 줄로 출력합니다
    #[arg(long)]
    json: bool,

    /// 결과 문구를 쓰지 않습니다
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct DocumentArgs {
    /// HWP 또는 HWPX 파일
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// 글꼴 폴더나 글꼴 파일. 여러 번 쓸 수 있고, 없으면 시스템 글꼴을 씁니다
    #[arg(long = "font", value_name = "PATH")]
    fonts: Vec<PathBuf>,

    /// 암호를 읽을 환경 변수 이름. 암호를 인자로 받으면 셸 기록에 남습니다
    #[arg(long, value_name = "VAR")]
    password_env: Option<String>,

    /// 결과를 JSON 한 줄로 출력합니다
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct PngArgs {
    #[command(flatten)]
    document: DocumentArgs,

    /// PNG를 쓸 폴더 (기본: 입력과 같은 폴더)
    #[arg(short, long, value_name = "DIR")]
    output: Option<PathBuf>,

    /// 쪽 범위. 예: 1-3,5,7-
    #[arg(long, value_name = "RANGE")]
    pages: Option<String>,

    /// 해상도 (1에서 1200)
    #[arg(long, default_value_t = raster::DEFAULT_DPI)]
    dpi: u32,
}

#[derive(Clone, Copy, ValueEnum)]
enum TextFormatArg {
    Plain,
    Markdown,
}

#[derive(Args)]
struct TextArgs {
    #[command(flatten)]
    document: DocumentArgs,

    /// 평문은 쪽에 그려진 그대로, markdown은 문서 구조대로 꺼냅니다
    #[arg(long, value_enum, default_value = "plain")]
    format: TextFormatArg,

    /// 쪽 범위. 예: 1-3,5,7-
    #[arg(long, value_name = "RANGE")]
    pages: Option<String>,

    /// 글자를 쓸 파일 (기본: 표준 출력)
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct InfoArgs {
    #[command(flatten)]
    document: DocumentArgs,
}

#[derive(Args)]
struct FontsArgs {
    /// 글꼴 폴더나 글꼴 파일. 여러 번 쓸 수 있고, 없으면 시스템 글꼴을 씁니다
    #[arg(long = "font", value_name = "PATH")]
    fonts: Vec<PathBuf>,

    /// 이 문서의 글자 모양이 쓰는 글꼴만 확인합니다(문서에 내장된 글꼴 포함)
    #[arg(long, value_name = "INPUT")]
    document: Option<PathBuf>,

    /// --document 문서의 암호를 읽을 환경 변수 이름
    #[arg(long, value_name = "VAR")]
    password_env: Option<String>,

    /// 결과를 JSON으로 출력합니다
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Render(args) => finish(render(args)),
        Command::Png(args) => finish(png(args)),
        Command::Text(args) => finish(text(args)),
        Command::Info(args) => finish(info(args)),
        Command::Fonts(args) => finish(fonts(args)),
    }
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("오류: {message}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Serialize)]
struct RenderReport {
    input: String,
    output: String,
    pages: usize,
    bytes: usize,
    warnings: usize,
}

fn render(args: RenderArgs) -> Result<(), String> {
    let bytes = read_input(&args.input)?;
    let password = password(args.password_env.as_deref())?;
    let mut fonts = load_fonts(&args.fonts)?;

    let output = ttoryeot::to_pdf(&bytes, password.as_deref(), &mut fonts).map_err(explain)?;
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| args.input.with_extension("pdf"));
    refuse_overwriting_input(&args.input, &output_path)?;
    write_atomic(&output_path, &output.pdf)?;

    if args.json {
        let report = RenderReport {
            input: args.input.display().to_string(),
            output: output_path.display().to_string(),
            pages: output.pages,
            bytes: output.pdf.len(),
            warnings: output.warnings.len(),
        };
        let line = serde_json::to_string(&report).map_err(|error| error.to_string())?;
        println!("{line}");
    } else if !args.quiet {
        let mut line = format!(
            "{} — {}쪽, {}바이트",
            output_path.display(),
            commas(output.pages),
            commas(output.pdf.len())
        );
        if !output.warnings.is_empty() {
            line.push_str(&format!(", 경고 {}개", commas(output.warnings.len())));
        }
        println!("{line}");
    }
    Ok(())
}

fn read_input(input: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(input)
        .map_err(|error| format!("{}을(를) 읽지 못했습니다: {error}", input.display()))
}

fn password(variable: Option<&str>) -> Result<Option<String>, String> {
    match variable {
        Some(name) => match std::env::var(name) {
            Ok(value) if !value.is_empty() => Ok(Some(value)),
            _ => Err(format!("환경 변수 {name}가 없거나 비어 있습니다")),
        },
        None => Ok(None),
    }
}

fn load_fonts(given: &[PathBuf]) -> Result<FontBook, String> {
    let paths = font_paths(given)?;
    let fonts = FontBook::scan(&paths);
    if fonts.face_count() == 0 {
        return Err(format!(
            "글꼴을 하나도 찾지 못했습니다: {}. --font로 글꼴 폴더를 지정하세요",
            display_paths(&paths)
        ));
    }
    Ok(fonts)
}

/// 이 도구는 보기 전용이다. 어떤 출력 경로도 원본을 덮어쓰지 못한다.
fn refuse_overwriting_input(input: &Path, output: &Path) -> Result<(), String> {
    let same = match (input.canonicalize(), output.canonicalize()) {
        (Ok(input), Ok(output)) => input == output,
        _ => false,
    };
    if same {
        return Err(format!("원본을 덮어쓸 수 없습니다: {}", output.display()));
    }
    Ok(())
}

fn page_set(value: Option<&str>) -> Result<Option<PageSet>, String> {
    value.map(PageSet::parse).transpose()
}

fn no_pages_selected(total: usize) -> String {
    format!("쪽 범위에 해당하는 쪽이 없습니다. 이 문서는 {total}쪽입니다")
}

fn print_json<T: Serialize>(report: &T) -> Result<(), String> {
    let line = serde_json::to_string(report).map_err(|error| error.to_string())?;
    println!("{line}");
    Ok(())
}

#[derive(Serialize)]
struct PngFile {
    page: usize,
    path: String,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct PngReport {
    input: String,
    page_count: usize,
    dpi: u32,
    files: Vec<PngFile>,
    warnings: usize,
}

fn png(args: PngArgs) -> Result<(), String> {
    let document = &args.document;
    let pages = page_set(args.pages.as_deref())?;
    let bytes = read_input(&document.input)?;
    let password = password(document.password_env.as_deref())?;
    let mut fonts = load_fonts(&document.fonts)?;
    let selected = view::selected_pdf(&bytes, password.as_deref(), &mut fonts, pages.as_ref())
        .map_err(explain)?;
    if selected.pages.is_empty() {
        return Err(no_pages_selected(selected.page_count));
    }
    let folder = args.output.clone().unwrap_or_else(|| {
        document
            .input
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf)
    });
    let stem = document
        .input
        .file_stem()
        .map_or_else(|| "page".into(), |stem| stem.to_string_lossy());
    let mut files = Vec::new();
    raster::render_pages(selected.pdf, args.dpi, |index, image| {
        let page = selected.pages[index];
        let path = folder.join(format!("{stem}-{page:04}.png"));
        refuse_overwriting_input(&document.input, &path)?;
        write_atomic(&path, &image.png)?;
        files.push(PngFile {
            page,
            path: path.display().to_string(),
            width: image.width,
            height: image.height,
        });
        Ok(())
    })?;

    if document.json {
        print_json(&PngReport {
            input: document.input.display().to_string(),
            page_count: selected.page_count,
            dpi: args.dpi,
            files,
            warnings: selected.warnings.len(),
        })
    } else {
        for file in &files {
            println!("{} — {}×{}", file.path, file.width, file.height);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct TextReport<'a> {
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    format: &'static str,
    page_count: usize,
    pages: &'a [usize],
    text: &'a str,
    warnings: usize,
}

fn text(args: TextArgs) -> Result<(), String> {
    let document = &args.document;
    let pages = page_set(args.pages.as_deref())?;
    let bytes = read_input(&document.input)?;
    let password = password(document.password_env.as_deref())?;
    let mut fonts = load_fonts(&document.fonts)?;
    let (format, name) = match args.format {
        TextFormatArg::Plain => (TextFormat::Plain, "plain"),
        TextFormatArg::Markdown => (TextFormat::Markdown, "markdown"),
    };
    let extracted = view::extract_text(
        &bytes,
        password.as_deref(),
        &mut fonts,
        format,
        pages.as_ref(),
    )
    .map_err(explain)?;
    if extracted.pages.is_empty() {
        return Err(no_pages_selected(extracted.page_count));
    }
    if let Some(output) = &args.output {
        refuse_overwriting_input(&document.input, output)?;
        write_atomic(output, extracted.text.as_bytes())?;
    }
    if document.json {
        print_json(&TextReport {
            input: document.input.display().to_string(),
            output: args.output.as_ref().map(|path| path.display().to_string()),
            format: name,
            page_count: extracted.page_count,
            pages: &extracted.pages,
            text: &extracted.text,
            warnings: extracted.warnings.len(),
        })
    } else {
        if args.output.is_none() {
            println!("{}", extracted.text);
        }
        Ok(())
    }
}

fn info(args: InfoArgs) -> Result<(), String> {
    let document = &args.document;
    let bytes = read_input(&document.input)?;
    let password = password(document.password_env.as_deref())?;
    let mut fonts = load_fonts(&document.fonts)?;
    let report = view::inspect(&bytes, password.as_deref(), &mut fonts).map_err(explain)?;
    if document.json {
        return print_json(&report);
    }
    println!("형식: {}", report.format);
    if let Some(version) = &report.version {
        println!("버전: {version}");
    }
    let encrypted = match (report.encrypted, report.opened) {
        (false, _) => "없음",
        (true, true) => "있음 (열었음)",
        (true, false) => "있음 (열지 못함. --password-env로 암호를 주세요)",
    };
    println!("암호: {encrypted}");
    if report.distribution {
        println!("배포용 문서: 예");
    }
    if let Some(pages) = report.pages {
        println!("쪽 수: {}", commas(pages));
    }
    let metadata = &report.metadata;
    for (label, value) in [
        ("제목", &metadata.title),
        ("주제", &metadata.subject),
        ("지은이", &metadata.author),
        ("키워드", &metadata.keywords),
        ("설명", &metadata.comments),
        ("마지막 저장", &metadata.last_author),
        ("만든 때", &metadata.created),
        ("고친 때", &metadata.modified),
    ] {
        if let Some(value) = value {
            println!("{label}: {value}");
        }
    }
    if !report.fonts.is_empty() {
        println!("글꼴");
        for font in &report.fonts {
            let state = if font.available { "있음" } else { "없음" };
            println!("  {}: {state}", font.name);
        }
    }
    if !report.warnings.is_empty() {
        println!("경고: {}개", commas(report.warnings.len()));
    }
    Ok(())
}

#[derive(Serialize)]
struct FontsReport {
    paths: Vec<String>,
    faces: usize,
    fonts: BTreeMap<String, bool>,
}

fn fonts(args: FontsArgs) -> Result<(), String> {
    let paths = font_paths(&args.fonts)?;
    let mut book = FontBook::scan(&paths);
    let faces = book.face_count();
    let found: BTreeMap<String, bool> = match &args.document {
        Some(input) => {
            let bytes = read_input(input)?;
            let options = hwp_core::ParseOptions {
                password: password(args.password_env.as_deref())?,
                ..hwp_core::ParseOptions::default()
            };
            let parsed = hwp_core::parse(&bytes, &options)
                .map_err(|error| explain(ttoryeot::Error::Parse(error)))?;
            book.attach_document(&parsed.document);
            view::used_fonts(&parsed.document)
                .into_iter()
                .map(|name| {
                    let present = book.find(&name, false).is_some();
                    (name, present)
                })
                .collect()
        }
        None => HANCOM_FONTS
            .iter()
            .map(|name| ((*name).to_string(), book.has(name)))
            .collect(),
    };

    if args.json {
        let report = FontsReport {
            paths: paths
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            faces,
            fonts: found,
        };
        let line = serde_json::to_string(&report).map_err(|error| error.to_string())?;
        println!("{line}");
    } else {
        println!("글꼴 경로");
        for path in &paths {
            println!("  {}", path.display());
        }
        println!("색인한 얼굴: {}개", commas(faces));
        for (name, present) in &found {
            println!("  {name}: {}", if *present { "있음" } else { "없음" });
        }
    }

    if faces == 0 {
        return Err(format!(
            "글꼴을 하나도 찾지 못했습니다: {}",
            display_paths(&paths)
        ));
    }
    Ok(())
}

/// 운영체제가 기본으로 쓰는 글꼴 폴더 중 실제로 있는 것.
///
/// 한/글 문서는 보통 함초롬체를 가리킨다. 그 글꼴이 없으면 `ttoryeot::subst`의 대체 규칙이
/// 비슷한 계열로 그린다. 한/글과 같은 모양이 필요하면 함초롬체를 설치하거나 `--font`로 넣는다.
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

fn font_paths(given: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if given.is_empty() {
        let defaults = default_font_paths();
        if defaults.is_empty() {
            return Err("시스템 글꼴 폴더를 찾지 못했습니다. --font로 지정하세요".to_string());
        }
        return Ok(defaults);
    }
    for path in given {
        if !path.exists() {
            return Err(format!("글꼴 경로가 없습니다: {}", path.display()));
        }
    }
    Ok(given.to_vec())
}

fn explain(error: ttoryeot::Error) -> String {
    use hwp_core::Fatal;
    match &error {
        ttoryeot::Error::Parse(Fatal::PasswordProtected) => {
            "암호가 필요한 문서입니다. --password-env로 암호가 담긴 환경 변수를 지정하세요"
                .to_string()
        }
        ttoryeot::Error::Parse(Fatal::WrongPassword) => "암호가 맞지 않습니다".to_string(),
        ttoryeot::Error::Parse(Fatal::UnsupportedEncryption) => {
            "이 암호 방식은 아직 지원하지 않습니다".to_string()
        }
        ttoryeot::Error::Parse(Fatal::UnsupportedEncryptionDetail(detail)) => {
            format!("이 암호 방식은 아직 지원하지 않습니다: {detail}")
        }
        ttoryeot::Error::Parse(Fatal::CorruptDocument(detail)) => {
            format!("문서가 손상된 것 같습니다: {detail}")
        }
        ttoryeot::Error::Parse(Fatal::Hwp3Unsupported) => {
            "HWP 3.0 문서는 아직 지원하지 않습니다".to_string()
        }
        ttoryeot::Error::Parse(Fatal::HwpmlUnsupported) => {
            "HWPML(.hml) 문서는 아직 지원하지 않습니다".to_string()
        }
        ttoryeot::Error::Parse(Fatal::NotHwp) => {
            "HWP/HWPX 문서가 아닙니다 (파일 머리를 확인하세요)".to_string()
        }
        ttoryeot::Error::Parse(Fatal::NoReadableSection) => {
            "읽을 수 있는 구역이 없습니다".to_string()
        }
        ttoryeot::Error::Parse(Fatal::Internal(detail)) => {
            format!("문서를 읽다가 문제가 생겼습니다: {detail}")
        }
        ttoryeot::Error::Pdf(detail) => format!("PDF를 쓰지 못했습니다: {detail}"),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}을(를) 만들지 못했습니다: {error}", parent.display()))?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".part");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, bytes)
        .map_err(|error| format!("{}을(를) 쓰지 못했습니다: {error}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("{}을(를) 저장하지 못했습니다: {error}", path.display())
    })
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn commas(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::commas;

    #[test]
    fn commas_groups_thousands() {
        assert_eq!(commas(0), "0");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1_000), "1,000");
        assert_eq!(commas(1_234_567), "1,234,567");
    }
}
