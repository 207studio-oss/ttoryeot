mod body;
mod columns;
mod page_border_fill;
mod props;
mod ranges;
mod shape;

pub use body::*;
pub use columns::*;
pub use page_border_fill::*;
pub use props::*;
pub use ranges::*;
pub use shape::*;

use serde::Serialize;
use std::collections::BTreeMap;

pub type HwpUnit = f64;
pub type Point = [HwpUnit; 2];
pub type Rect = [Point; 2];
pub type Transform = [HwpUnit; 6];

fn fix(value: &mut HwpUnit, fallback: HwpUnit) {
    if !value.is_finite() {
        *value = fallback;
    }
}

fn fix_map(values: &mut BTreeMap<String, HwpUnit>, fallback: HwpUnit) {
    for value in values.values_mut() {
        fix(value, fallback);
    }
}

fn fix_point(point: &mut Point) {
    fix(&mut point[0], 0.0);
    fix(&mut point[1], 0.0);
}

fn fix_rect(rect: &mut Rect) {
    for point in rect.iter_mut() {
        fix_point(point);
    }
}

fn rect_is_finite(rect: &Rect) -> bool {
    rect.iter().flatten().all(|value| value.is_finite())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobKind {
    Picture,
    Font,
}

/// BinData 이름이 글꼴 파일인지. HWP5는 `BIN0001.ttf`, HWPX는 `BinData/font.ttf`처럼
/// 확장자로 구분한다. 그림과 달리 글꼴은 WMF 변환을 거치지 않고 그대로 색인한다.
pub(crate) fn is_font_file(name: &str) -> bool {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    matches!(
        extension.map(str::to_ascii_lowercase).as_deref(),
        Some("ttf" | "otf" | "ttc" | "otc")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub id: String,
    pub kind: BlobKind,
    pub data: std::sync::Arc<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub font_faces: BTreeMap<String, Vec<FontFace>>,
    pub char_prs: Vec<CharPr>,
    pub para_prs: Vec<ParaPr>,
    pub tab_prs: Vec<TabPr>,
    pub border_fills: Vec<BorderFill>,
    pub numberings: Vec<Numbering>,
    pub bullets: Vec<Bullet>,
    pub styles: Vec<Style>,
    pub sections: Vec<Section>,
    pub memo_shapes: Vec<MemoShape>,
    pub wchar_positions: bool,
    /// 본문에 변경 추적 표시(지운 글·넣은 글)가 있다. 한/글은 이런 문서를 풍선 칸을 남기고 줄여 찍는다.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub track_changes: bool,
    /// 변경 추적 설정(`trackchageConfig flags`)에 4가 켜져 있다. 한/글은 이때 넣은 글·지운 글을 빨간 글자와 선으로 표시하지 않는다.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub track_markup_hidden: bool,
    #[serde(skip)]
    pub bins: Vec<Blob>,
}

impl Document {
    pub fn sanitize_non_finite(&mut self) {
        for value in &mut self.char_prs {
            value.fix_non_finite();
        }
        for value in &mut self.para_prs {
            value.fix_non_finite();
        }
        for value in &mut self.tab_prs {
            value.fix_non_finite();
        }
        for value in &mut self.border_fills {
            value.fix_non_finite();
        }
        for value in &mut self.numberings {
            value.fix_non_finite();
        }
        for value in &mut self.bullets {
            value.fix_non_finite();
        }
        for value in &mut self.memo_shapes {
            value.fix_non_finite();
        }
        for value in &mut self.sections {
            value.fix_non_finite();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_font_file;

    #[test]
    fn font_extensions_are_case_insensitive_and_cover_otf_ttc_otc() {
        for name in [
            "BinData/font.ttf",
            "BinData/font.TTF",
            "BinData/font.otf",
            "BinData/font.OTF",
            "BinData/font.ttc",
            "BinData/font.TTC",
            "BinData/font.otc",
            "BinData/font.OTC",
        ] {
            assert!(is_font_file(name), "{name}");
        }
        for name in ["BinData/font.png", "BinData/font.ttf.bak", "BinData/font"] {
            assert!(!is_font_file(name), "{name}");
        }
    }
}
