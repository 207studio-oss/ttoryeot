use serde::Serialize;
use std::collections::BTreeMap;

use super::{fix, fix_map, Gradient, HwpUnit};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharPr {
    pub id: i32,
    pub height: HwpUnit,
    pub text_color: u32,
    pub shade_color: Option<u32>,
    pub font_ref: BTreeMap<String, i32>,
    pub ratio: BTreeMap<String, HwpUnit>,
    pub spacing: BTreeMap<String, HwpUnit>,
    pub rel_sz: BTreeMap<String, HwpUnit>,
    pub offset: BTreeMap<String, HwpUnit>,
    pub bold: bool,
    pub italic: bool,
    pub underline: String,
    pub underline_color: u32,
    pub underline_shape: String,
    pub sym_mark: String,
    #[serde(rename = "borderFillIDRef")]
    pub border_fill_id_ref: i32,
    pub strikeout_color: u32,
    pub shadow_type: String,
    pub shadow_offset_x: HwpUnit,
    pub shadow_offset_y: HwpUnit,
    pub shadow_color: u32,
    pub strikeout: bool,
    pub supscript: bool,
    pub sub_script: bool,
    pub emboss: bool,
    pub engrave: bool,
    pub outline: bool,
    pub shadow: bool,
    pub use_font_space: bool,
    pub kerning: bool,
}

impl Default for CharPr {
    fn default() -> Self {
        Self {
            id: 0,
            height: 1000.0,
            text_color: 0,
            shade_color: None,
            font_ref: BTreeMap::new(),
            ratio: BTreeMap::new(),
            spacing: BTreeMap::new(),
            rel_sz: BTreeMap::new(),
            offset: BTreeMap::new(),
            bold: false,
            italic: false,
            underline: "NONE".to_string(),
            underline_color: 0,
            underline_shape: "SOLID".to_string(),
            sym_mark: "NONE".to_string(),
            border_fill_id_ref: 0,
            strikeout_color: 0,
            shadow_type: "NONE".to_string(),
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
            shadow_color: 0,
            strikeout: false,
            supscript: false,
            sub_script: false,
            emboss: false,
            engrave: false,
            outline: false,
            shadow: false,
            use_font_space: false,
            kerning: false,
        }
    }
}

impl CharPr {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.height, 1000.0);
        fix(&mut self.shadow_offset_x, 0.0);
        fix(&mut self.shadow_offset_y, 0.0);
        fix_map(&mut self.ratio, 100.0);
        fix_map(&mut self.spacing, 0.0);
        fix_map(&mut self.rel_sz, 100.0);
        fix_map(&mut self.offset, 0.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParaPr {
    pub id: i32,
    pub align: String,
    pub vert_align: String,
    pub margin_left: HwpUnit,
    pub margin_right: HwpUnit,
    pub indent: HwpUnit,
    pub prev: HwpUnit,
    pub next: HwpUnit,
    pub line_spacing_type: String,
    pub line_spacing: HwpUnit,
    pub heading_type: String,
    pub heading_id_ref: i32,
    pub heading_level: i32,
    #[serde(rename = "tabPrIDRef")]
    pub tab_pr_id_ref: i32,
    #[serde(rename = "borderFillIDRef")]
    pub border_fill_id_ref: i32,
    pub break_latin_word: String,
    pub break_non_latin_word: String,
    /// 한 줄로 입력: BREAK(나눔)·SQUEEZE(자간 줄여 한 줄)·KEEP.
    pub line_wrap: String,
    pub keep_with_next: bool,
    pub page_break_before: bool,
    pub widow_orphan: bool,
    pub keep_lines: bool,
    pub snap_to_grid: bool,
    pub condense: u8,
    pub font_line_height: bool,
    pub connect: bool,
    pub ignore_margin: bool,
    pub tail_shape: bool,
}

impl Default for ParaPr {
    fn default() -> Self {
        Self {
            id: 0,
            align: "JUSTIFY".to_string(),
            vert_align: "BASELINE".to_string(),
            margin_left: 0.0,
            margin_right: 0.0,
            indent: 0.0,
            prev: 0.0,
            next: 0.0,
            line_spacing_type: "PERCENT".to_string(),
            line_spacing: 160.0,
            heading_type: "NONE".to_string(),
            heading_id_ref: 0,
            heading_level: 0,
            tab_pr_id_ref: 0,
            border_fill_id_ref: 0,
            break_latin_word: "KEEP_WORD".to_string(),
            break_non_latin_word: "BREAK_WORD".to_string(),
            line_wrap: "BREAK".to_string(),
            keep_with_next: false,
            page_break_before: false,
            widow_orphan: false,
            keep_lines: false,
            snap_to_grid: true,
            condense: 0,
            font_line_height: false,
            connect: false,
            ignore_margin: false,
            tail_shape: false,
        }
    }
}

impl ParaPr {
    /// 개요는 구역의 번호 모양을 쓰고, 문단 번호는 자신의 참조를 쓴다.
    pub fn numbering_id(&self, outline_numbering_id: Option<i32>) -> i32 {
        if self.heading_type == "OUTLINE" {
            outline_numbering_id.unwrap_or(self.heading_id_ref)
        } else {
            self.heading_id_ref
        }
    }

    pub(crate) fn fix_non_finite(&mut self) {
        for value in [
            &mut self.margin_left,
            &mut self.margin_right,
            &mut self.indent,
            &mut self.prev,
            &mut self.next,
        ] {
            fix(value, 0.0);
        }
        fix(&mut self.line_spacing, 160.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub width: Option<HwpUnit>,
    pub kind: String,
    pub leader: String,
}

impl Default for Tab {
    fn default() -> Self {
        Self {
            width: None,
            kind: "LEFT".to_string(),
            leader: "NONE".to_string(),
        }
    }
}

impl Tab {
    pub(crate) fn fix_non_finite(&mut self) {
        if let Some(width) = self.width.as_mut() {
            if !width.is_finite() {
                self.width = None;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabStop {
    pub position: HwpUnit,
    pub kind: String,
    pub leader: String,
}

impl Default for TabStop {
    fn default() -> Self {
        Self {
            position: 0.0,
            kind: "LEFT".to_string(),
            leader: "NONE".to_string(),
        }
    }
}

impl TabStop {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.position, 0.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TabPr {
    pub id: i32,
    pub auto_left: bool,
    pub auto_right: bool,
    pub stops: Vec<TabStop>,
}

impl TabPr {
    pub(crate) fn fix_non_finite(&mut self) {
        for stop in &mut self.stops {
            stop.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Border {
    #[serde(rename = "type")]
    pub border_type: String,
    pub width: HwpUnit,
    pub color: u32,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            border_type: "NONE".to_string(),
            width: 0.1,
            color: 0,
        }
    }
}

impl Border {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.width, 0.1);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BorderFill {
    pub id: i32,
    pub left: Border,
    pub right: Border,
    pub top: Border,
    pub bottom: Border,
    pub diagonal: Border,
    pub slash: String,
    pub back_slash: String,
    pub fill_color: Option<u32>,
    pub gradient: Option<Vec<(u32, HwpUnit)>>,
    /// 그러데이션 전체(꼴·중심·단계). 렌더러가 쓴다. 브리지 JSON(v1)에는 `gradient` 색 목록만 나간다.
    #[serde(skip)]
    pub gradation: Option<Gradient>,
}

impl Default for BorderFill {
    fn default() -> Self {
        Self {
            id: 0,
            left: Border::default(),
            right: Border::default(),
            top: Border::default(),
            bottom: Border::default(),
            diagonal: Border::default(),
            slash: "NONE".to_string(),
            back_slash: "NONE".to_string(),
            fill_color: None,
            gradient: None,
            gradation: None,
        }
    }
}

impl BorderFill {
    pub(crate) fn fix_non_finite(&mut self) {
        for border in [
            &mut self.left,
            &mut self.right,
            &mut self.top,
            &mut self.bottom,
            &mut self.diagonal,
        ] {
            border.fix_non_finite();
        }
        if let Some(gradient) = self.gradient.as_mut() {
            for stop in gradient {
                fix(&mut stop.1, 0.0);
            }
        }
        if let Some(gradation) = self.gradation.as_mut() {
            gradation.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub width: HwpUnit,
    pub height: HwpUnit,
    pub margin_left: HwpUnit,
    pub margin_right: HwpUnit,
    pub margin_top: HwpUnit,
    pub margin_bottom: HwpUnit,
    pub header: HwpUnit,
    pub footer: HwpUnit,
    pub gutter: HwpUnit,
    pub landscape: bool,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            width: 59528.0,
            height: 84186.0,
            margin_left: 8504.0,
            margin_right: 8504.0,
            margin_top: 5668.0,
            margin_bottom: 4252.0,
            header: 4252.0,
            footer: 4252.0,
            gutter: 0.0,
            landscape: false,
        }
    }
}

impl Page {
    pub(crate) fn fix_non_finite(&mut self) {
        let defaults = Page::default();
        for (value, fallback) in [
            (&mut self.width, defaults.width),
            (&mut self.height, defaults.height),
            (&mut self.margin_left, defaults.margin_left),
            (&mut self.margin_right, defaults.margin_right),
            (&mut self.margin_top, defaults.margin_top),
            (&mut self.margin_bottom, defaults.margin_bottom),
            (&mut self.header, defaults.header),
            (&mut self.footer, defaults.footer),
            (&mut self.gutter, defaults.gutter),
        ] {
            fix(value, fallback);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LineSeg {
    pub textpos: i32,
    pub vertpos: HwpUnit,
    pub vertsize: HwpUnit,
    pub textheight: HwpUnit,
    pub baseline: HwpUnit,
    pub spacing: HwpUnit,
    pub horzpos: HwpUnit,
    pub horzsize: HwpUnit,
    pub flags: u32,
}

impl LineSeg {
    pub(crate) fn fix_non_finite(&mut self) {
        for value in [
            &mut self.vertpos,
            &mut self.vertsize,
            &mut self.textheight,
            &mut self.baseline,
            &mut self.spacing,
            &mut self.horzpos,
            &mut self.horzsize,
        ] {
            fix(value, 0.0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pos {
    pub treat_as_char: bool,
    pub vert_rel_to: String,
    pub horz_rel_to: String,
    pub width_rel_to: String,
    pub height_rel_to: String,
    pub vert_align: String,
    pub horz_align: String,
    pub vert_offset: HwpUnit,
    pub horz_offset: HwpUnit,
    pub flow_with_text: bool,
    pub allow_overlap: bool,
    pub affect_l_spacing: bool,
    pub text_wrap: String,
}

impl Default for Pos {
    fn default() -> Self {
        Self {
            treat_as_char: true,
            vert_rel_to: "PARA".to_string(),
            horz_rel_to: "COLUMN".to_string(),
            width_rel_to: "ABSOLUTE".to_string(),
            height_rel_to: "ABSOLUTE".to_string(),
            vert_align: "TOP".to_string(),
            horz_align: "LEFT".to_string(),
            vert_offset: 0.0,
            horz_offset: 0.0,
            flow_with_text: true,
            allow_overlap: false,
            affect_l_spacing: false,
            text_wrap: "SQUARE".to_string(),
        }
    }
}

impl Pos {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.vert_offset, 0.0);
        fix(&mut self.horz_offset, 0.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Margins {
    pub left: HwpUnit,
    pub right: HwpUnit,
    pub top: HwpUnit,
    pub bottom: HwpUnit,
}

impl Margins {
    pub(crate) fn fix_non_finite(&mut self) {
        for value in [
            &mut self.left,
            &mut self.right,
            &mut self.top,
            &mut self.bottom,
        ] {
            fix(value, 0.0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParaHead {
    pub auto_indent: bool,
    pub use_inst_width: bool,
    pub width_adjust: HwpUnit,
    pub text_offset_percent: bool,
    pub text_offset: HwpUnit,
}

impl Default for ParaHead {
    fn default() -> Self {
        Self {
            auto_indent: false,
            use_inst_width: false,
            width_adjust: 0.0,
            text_offset_percent: true,
            text_offset: 0.0,
        }
    }
}

impl ParaHead {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.width_adjust, 0.0);
        fix(&mut self.text_offset, 0.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberingHead {
    pub level: i32,
    pub format: String,
    pub text: String,
    pub align: String,
    pub start: i32,
    #[serde(rename = "charPrID")]
    pub char_pr_id: Option<i32>,
    pub para: ParaHead,
}

impl Default for NumberingHead {
    fn default() -> Self {
        Self {
            level: 1,
            format: "DIGIT".to_string(),
            text: "^1.".to_string(),
            align: "LEFT".to_string(),
            start: 1,
            char_pr_id: None,
            para: ParaHead::default(),
        }
    }
}

impl NumberingHead {
    pub(crate) fn fix_non_finite(&mut self) {
        self.para.fix_non_finite();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Numbering {
    pub id: i32,
    pub start: i32,
    pub heads: Vec<NumberingHead>,
}

impl Numbering {
    pub(crate) fn fix_non_finite(&mut self) {
        for head in &mut self.heads {
            head.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bullet {
    pub id: i32,
    pub char: String,
    #[serde(rename = "charPrID")]
    pub char_pr_id: Option<i32>,
    pub para: ParaHead,
}

impl Default for Bullet {
    fn default() -> Self {
        Self {
            id: 0,
            char: "•".to_string(),
            char_pr_id: None,
            para: ParaHead::default(),
        }
    }
}

impl Bullet {
    pub(crate) fn fix_non_finite(&mut self) {
        self.para.fix_non_finite();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFace {
    pub id: i32,
    pub name: String,
    #[serde(rename = "type")]
    pub face_type: i32,
    pub alt_name: Option<String>,
    pub alt_type: i32,
    pub default_name: Option<String>,
    pub embedded: bool,
    pub binary_ref: Option<String>,
}

impl FontFace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: 0,
            name: name.into(),
            face_type: 0,
            alt_name: None,
            alt_type: 0,
            default_name: None,
            embedded: false,
            binary_ref: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub id: i32,
    #[serde(rename = "paraPrID")]
    pub para_pr_id: i32,
    #[serde(rename = "charPrID")]
    pub char_pr_id: i32,
    /// 영문 이름("Page Number" 같은 한/글 기본 스타일 이름). 배치 안에서만 쓰고 모델 JSON에는 싣지 않는다.
    #[serde(skip)]
    pub eng_name: String,
}
