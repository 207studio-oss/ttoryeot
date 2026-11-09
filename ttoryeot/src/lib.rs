//! 또렷: hwp-core 문서 모델을 한/글처럼 쪽에 배치해 PDF로 그린다.
//!
//! ```no_run
//! use std::path::PathBuf;
//! use ttoryeot::fonts::FontBook;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("문서.hwp")?;
//! let mut fonts = FontBook::scan(&[PathBuf::from("글꼴")]);
//! let output = ttoryeot::to_pdf(&bytes, None, &mut fonts)?;
//! std::fs::write("문서.pdf", &output.pdf)?;
//! println!("{}쪽", output.pages);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

pub mod components;
mod equation;
pub mod fonts;
mod hft;
pub use layout::text::lang_of;
mod layout;
mod markdown;
pub mod notes;
pub mod pdf;
pub mod scene;
pub mod subst;
pub mod view;

use fonts::FontBook;
use hwp_core::{Document, ParseOptions, Warning};

/// 문서를 읽거나 PDF를 쓰지 못한 까닭.
#[derive(Debug)]
pub enum Error {
    /// 문서를 읽지 못했다. 암호 오류·형식 오류가 여기에 온다.
    Parse(hwp_core::Fatal),
    /// 쪽 배치는 끝났지만 PDF를 쓰지 못했다.
    Pdf(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(error) => write!(formatter, "문서를 읽지 못했습니다: {error}"),
            Error::Pdf(detail) => write!(formatter, "PDF를 쓰지 못했습니다: {detail}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Parse(error) => Some(error),
            Error::Pdf(_) => None,
        }
    }
}

impl From<hwp_core::Fatal> for Error {
    fn from(error: hwp_core::Fatal) -> Self {
        Error::Parse(error)
    }
}

/// 한 번의 변환 결과. `pdf`는 완성된 PDF 바이트다.
pub struct Output {
    pub pdf: Vec<u8>,
    pub pages: usize,
    pub warnings: Vec<Warning>,
}

/// 문서를 쪽마다의 그릴 것 목록으로 배치한다.
///
/// 문서에 내장된 글꼴은 [`FontBook::attach_document`]로 이 색인에 붙인 뒤 쓴다. 글꼴 자료는
/// 문서 메모리에서만 읽고 디스크에 쓰지 않는다. 내장 글꼴이 없는 문서는 색인을 그대로 쓴다.
///
/// 다음 문서를 배치하면 앞 문서의 내장 얼굴을 지운다. 얼굴 번호는 다시 쓰지 않으므로
/// 돌려준 [`scene::Page`]는 그 전에 [`pdf::write`]로 다 써야 하며, 지운 얼굴을 가리키는
/// 쪽을 나중에 쓰면 새 얼굴로 그리지 않고 오류가 난다.
pub fn render(doc: &Document, fonts: &mut FontBook) -> Vec<scene::Page> {
    fonts.attach_document(doc);
    layout::Renderer::new(doc, fonts).layout().pages
}

/// [`render`]와 같은 배치에서 쪽마다 구성 요소(문단·표·칸·개체·머리말 등)의 자리도 함께 돌려준다.
///
/// 두 목록은 같은 쪽 수와 같은 쪽 번호를 쓴다. 구성 요소는 배치하면서 적은 것이라 그린 것과 어긋나지 않는다.
pub fn render_with_components(
    doc: &Document,
    fonts: &mut FontBook,
) -> (Vec<scene::Page>, Vec<components::PageComponents>) {
    fonts.attach_document(doc);
    let layout = layout::Renderer::new(doc, fonts)
        .recording_components()
        .layout();
    (layout.pages, layout.components)
}

/// HWP·HWPX 바이트를 읽어 PDF로 만든다. 암호 문서는 `password`가 맞아야 열린다.
///
/// 글꼴은 미리 [`FontBook`]에 색인해 두어야 한다. 아무 글꼴도 없으면 글자가 빈 칸으로 남는다.
pub fn to_pdf(bytes: &[u8], password: Option<&str>, fonts: &mut FontBook) -> Result<Output, Error> {
    Ok(convert(bytes, password, fonts, false)?.0)
}

/// [`to_pdf`]와 같은 PDF에 더해, 쪽마다 구성 요소의 자리를 돌려준다. 좌표는 [`components`]에 적었다.
pub fn to_pdf_with_components(
    bytes: &[u8],
    password: Option<&str>,
    fonts: &mut FontBook,
) -> Result<(Output, Vec<components::PageComponents>), Error> {
    convert(bytes, password, fonts, true)
}

fn convert(
    bytes: &[u8],
    password: Option<&str>,
    fonts: &mut FontBook,
    record: bool,
) -> Result<(Output, Vec<components::PageComponents>), Error> {
    let options = ParseOptions {
        password: password.map(str::to_string),
        ..ParseOptions::default()
    };
    let hwp_core::Parsed {
        document, warnings, ..
    } = hwp_core::parse(bytes, &options)?;
    let (pages, components) = if record {
        render_with_components(&document, fonts)
    } else {
        (render(&document, fonts), Vec::new())
    };
    drop(document);
    let count = pages.len();
    let pdf = pdf::write(pages, fonts).map_err(Error::Pdf)?;
    let output = Output {
        pdf,
        pages: count,
        warnings,
    };
    Ok((output, components))
}

/// PDF 바이트만 필요한 경우의 짧은 길.
pub fn render_pdf(
    bytes: &[u8],
    password: Option<&str>,
    fonts: &mut FontBook,
) -> Result<Vec<u8>, Error> {
    Ok(to_pdf(bytes, password, fonts)?.pdf)
}
