#![forbid(unsafe_code)]

pub mod detect;
pub mod frame;
pub mod hwp5;
pub mod hwpx;
pub mod info;
pub mod model;
pub mod num;
pub mod probe;
pub mod report;
pub mod text;
mod wmf;

pub use detect::{detect, Format};
pub use info::{DocumentInfo, Metadata};
pub use model::*;
pub use report::{Fatal, Warning, WarningKind};

pub const SCHEMA_VERSION: u32 = 1;

/// 고정폭 빈칸. 한/글은 글꼴과 상관없이 1/4 em으로 둔다(U+2005 FOUR-PER-EM SPACE).
pub const FIXED_SPACE: char = '\u{2005}';
/// 묶음 빈칸. 빈칸 폭이되 자간을 넣지 않고 줄을 나누지 않는다. 글에 들어 있는 U+00A0은 글꼴 글자로 따로 다룬다.
pub const NO_BREAK_SPACE: char = '\u{2007}';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatHint {
    #[default]
    Auto,
    Hwp,
    Hwpx,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ParseOptions {
    pub hint: FormatHint,
    pub max_depth: u8,
    /// 암호 HWP5/HWPX를 열 암호. 없으면 암호 문서는 `Fatal::PasswordProtected`다.
    pub password: Option<String>,
}

impl std::fmt::Debug for ParseOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ParseOptions")
            .field("hint", &self.hint)
            .field("max_depth", &self.max_depth)
            .field("password", &self.password.as_ref().map(|_| "<set>"))
            .finish()
    }
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            hint: FormatHint::Auto,
            max_depth: 32,
            password: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub document: Document,
    pub warnings: Vec<Warning>,
    pub info: DocumentInfo,
}

pub fn parse(bytes: &[u8], opts: &ParseOptions) -> Result<Parsed, Fatal> {
    on_parser_stack(|| parse_document(bytes, opts))?
}

pub(crate) fn on_parser_stack<T: Send>(operation: impl FnOnce() -> T + Send) -> Result<T, Fatal> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(16 << 20)
            .spawn_scoped(scope, operation)
            .map_err(|error| Fatal::Internal(format!("parse thread could not start: {error}")))?
            .join()
            .map_err(|payload| Fatal::Internal(panic_detail(payload)))
    })
}

pub(crate) fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "panic".to_string()
    }
}

pub(crate) fn parse_document(bytes: &[u8], opts: &ParseOptions) -> Result<Parsed, Fatal> {
    let mut parsed = match detect::detect(bytes) {
        Format::Cfb => hwp5::parse(bytes, opts),
        Format::Zip => hwpx::parse(bytes, opts),
        Format::Hwp3 => Err(Fatal::Hwp3Unsupported),
        Format::Hwpml => Err(Fatal::HwpmlUnsupported),
        Format::Unknown => Err(Fatal::NotHwp),
    }?;
    num::resolve_heads(&mut parsed.document);
    Ok(parsed)
}
