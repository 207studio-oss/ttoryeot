use serde::Serialize;

use super::props::{Border, LineSeg, Margins, Page, Pos, Tab};
use super::{
    fix, fix_rect, rect_is_finite, ColumnDef, HwpUnit, Markpen, PageBorderFill, RangeTag, Rect,
    Shape, TrackMark, Transform,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub col: i32,
    pub row: i32,
    pub col_span: i32,
    pub row_span: i32,
    pub width: HwpUnit,
    pub height: HwpUnit,
    #[serde(rename = "borderFillID")]
    pub border_fill_id: i32,
    pub margins: Margins,
    pub vert_align: String,
    pub paragraphs: Vec<Paragraph>,
    pub has_margin: bool,
    pub header: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            col_span: 1,
            row_span: 1,
            width: 0.0,
            height: 0.0,
            border_fill_id: 0,
            margins: Margins::default(),
            vert_align: "TOP".to_string(),
            paragraphs: Vec::new(),
            has_margin: false,
            header: false,
        }
    }
}

impl Cell {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.width, 0.0);
        fix(&mut self.height, 0.0);
        self.margins.fix_non_finite();
        for paragraph in &mut self.paragraphs {
            paragraph.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub side: String,
    pub gap: HwpUnit,
    pub width: HwpUnit,
    pub full_size: bool,
    pub paragraphs: Vec<Paragraph>,
}

impl Default for Caption {
    fn default() -> Self {
        Self {
            side: "LEFT".to_string(),
            gap: 0.0,
            width: 0.0,
            full_size: false,
            paragraphs: Vec::new(),
        }
    }
}

impl Caption {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.gap, 0.0);
        fix(&mut self.width, 0.0);
        for paragraph in &mut self.paragraphs {
            paragraph.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub rows: i32,
    pub cols: i32,
    pub width: HwpUnit,
    pub height: HwpUnit,
    pub pos: Pos,
    pub out_margins: Margins,
    pub in_margins: Margins,
    #[serde(rename = "borderFillID")]
    pub border_fill_id: i32,
    pub cells: Vec<Cell>,
    pub cell_spacing: HwpUnit,
    pub z_order: i32,
    pub repeat_header: bool,
    pub page_break_mode: String,
    pub caption: Option<Caption>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            rows: 0,
            cols: 0,
            width: 0.0,
            height: 0.0,
            pos: Pos::default(),
            out_margins: Margins::default(),
            in_margins: Margins::default(),
            border_fill_id: 0,
            cells: Vec::new(),
            cell_spacing: 0.0,
            z_order: 0,
            repeat_header: false,
            page_break_mode: "CELL".to_string(),
            caption: None,
        }
    }
}

impl Table {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.width, 0.0);
        fix(&mut self.height, 0.0);
        fix(&mut self.cell_spacing, 0.0);
        self.pos.fix_non_finite();
        self.out_margins.fix_non_finite();
        self.in_margins.fix_non_finite();
        for cell in &mut self.cells {
            cell.fix_non_finite();
        }
        if let Some(caption) = self.caption.as_mut() {
            caption.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    pub width: HwpUnit,
    pub height: HwpUnit,
    pub pos: Pos,
    pub out_margins: Margins,
    #[serde(rename = "binaryID")]
    pub binary_id: String,
    pub z_order: i32,
    #[serde(rename = "borderFillID")]
    pub border_fill_id: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub rotation: HwpUnit,
    pub crop_left: HwpUnit,
    pub crop_right: HwpUnit,
    pub crop_top: HwpUnit,
    pub crop_bottom: HwpUnit,
    pub orig_width: HwpUnit,
    pub orig_height: HwpUnit,
    pub clip: Option<Rect>,
    pub caption: Option<Caption>,
    /// 그림 테두리 선(mm). 선을 적지 않은 그림은 None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Border>,
}

impl Default for Picture {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            pos: Pos::default(),
            out_margins: Margins::default(),
            binary_id: String::new(),
            z_order: 0,
            border_fill_id: 0,
            flip_h: false,
            flip_v: false,
            rotation: 0.0,
            crop_left: 0.0,
            crop_right: 0.0,
            crop_top: 0.0,
            crop_bottom: 0.0,
            orig_width: 0.0,
            orig_height: 0.0,
            clip: None,
            caption: None,
            outline: None,
        }
    }
}

impl Picture {
    pub(crate) fn fix_non_finite(&mut self) {
        for value in [
            &mut self.width,
            &mut self.height,
            &mut self.rotation,
            &mut self.crop_left,
            &mut self.crop_right,
            &mut self.crop_top,
            &mut self.crop_bottom,
            &mut self.orig_width,
            &mut self.orig_height,
        ] {
            fix(value, 0.0);
        }
        self.pos.fix_non_finite();
        self.out_margins.fix_non_finite();
        if let Some(clip) = self.clip.as_ref() {
            if !rect_is_finite(clip) {
                self.clip = None;
            }
        }
        if let Some(caption) = self.caption.as_mut() {
            caption.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeChild {
    pub item: Box<RunItem>,
    pub frame: Rect,
    pub transform: Option<Transform>,
}

impl ShapeChild {
    pub(crate) fn fix_non_finite(&mut self) {
        fix_rect(&mut self.frame);
        if let Some(transform) = self.transform.as_mut() {
            if !transform.iter().all(|value| value.is_finite()) {
                *transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }
        }
        self.item.fix_non_finite();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Equation {
    pub script: String,
    pub size: HwpUnit,
    pub color: u32,
    pub baseline: HwpUnit,
    pub font: String,
}

impl Default for Equation {
    fn default() -> Self {
        Self {
            script: String::new(),
            size: 1000.0,
            color: 0,
            baseline: 85.0,
            font: String::new(),
        }
    }
}

impl Equation {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.size, 1000.0);
        fix(&mut self.baseline, 85.0);
    }
}

/// 글자 겹치기(HWPX `hp:compose`, HWP 5 `tcmp`): 테두리 모양 안에 글자를 겹치거나 나란히 놓는다.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Compose {
    pub text: String,
    /// HWPX `circleType` 이름(`SHAPE_CIRCLE` 등).
    pub shape: String,
    /// 안쪽 글자 크기를 줄이는 정도(pt).
    pub char_size: i32,
    /// 안쪽 글자를 한자리에 겹치면 참, 나란히 펼치면 거짓.
    pub overlap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub endnote: bool,
    pub number: i32,
    pub seq: i32,
    pub format: i32,
    pub prefix: String,
    pub suffix: String,
    pub paragraphs: Vec<Paragraph>,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            endnote: false,
            number: 1,
            seq: 0,
            format: 0,
            prefix: String::new(),
            suffix: String::new(),
            paragraphs: Vec::new(),
        }
    }
}

impl Note {
    pub(crate) fn fix_non_finite(&mut self) {
        for paragraph in &mut self.paragraphs {
            paragraph.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteShape {
    pub format: i32,
    pub line_length: HwpUnit,
    pub above_line: HwpUnit,
    pub below_line: HwpUnit,
    pub between_notes: HwpUnit,
    pub line_type: i32,
    pub line_width: HwpUnit,
    pub line_color: u32,
}

impl Default for NoteShape {
    fn default() -> Self {
        Self {
            format: 0,
            line_length: -1.0,
            above_line: 850.0,
            below_line: 567.0,
            between_notes: 283.0,
            line_type: 1,
            line_width: 0.12,
            line_color: 0,
        }
    }
}

impl NoteShape {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.line_length, -1.0);
        fix(&mut self.above_line, 850.0);
        fix(&mut self.below_line, 567.0);
        fix(&mut self.between_notes, 283.0);
        fix(&mut self.line_width, 0.12);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoShape {
    pub id: i32,
    pub width: HwpUnit,
    pub line_type: i32,
    pub line_width: i32,
    pub line_color: u32,
    pub fill_color: u32,
    pub active_color: u32,
    pub memo_type: i32,
}

impl Default for MemoShape {
    fn default() -> Self {
        Self {
            id: 0,
            width: 0.0,
            line_type: 0,
            line_width: 0,
            line_color: 0,
            fill_color: 0,
            active_color: 0,
            memo_type: 0,
        }
    }
}

impl MemoShape {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.width, 0.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    #[serde(rename = "shapeID")]
    pub shape_id: i32,
    pub paragraphs: Vec<Paragraph>,
}

impl Memo {
    pub(crate) fn fix_non_finite(&mut self) {
        for paragraph in &mut self.paragraphs {
            paragraph.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "t",
    content = "v",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[expect(
    clippy::large_enum_variant,
    reason = "F12 preserves the public RunItem::Shape(Shape) contract"
)]
pub enum RunItem {
    Text(String),
    Table(Table),
    Picture(Picture),
    Shape(Shape),
    Tab,
    SizedTab(HwpUnit),
    FormattedTab(Tab),
    LineBreak,
    AutoNum(String),
    Control(String),
    Note(Note),
    Compose(Compose),
    Skip(i32),
    Header {
        paragraphs: Vec<Paragraph>,
        applies: String,
    },
    Footer {
        paragraphs: Vec<Paragraph>,
        applies: String,
    },
    PageHiding {
        header: bool,
        footer: bool,
        page_num: bool,
    },
    NewNum(i32),
    PageNum {
        pos: String,
        format: String,
        side: String,
    },
}

impl RunItem {
    pub(crate) fn fix_non_finite(&mut self) {
        match self {
            RunItem::Table(table) => table.fix_non_finite(),
            RunItem::Picture(picture) => picture.fix_non_finite(),
            RunItem::Shape(shape) => shape.fix_non_finite(),
            RunItem::SizedTab(value) => fix(value, 0.0),
            RunItem::FormattedTab(tab) => tab.fix_non_finite(),
            RunItem::Note(note) => note.fix_non_finite(),
            RunItem::Header { paragraphs, .. } | RunItem::Footer { paragraphs, .. } => {
                for paragraph in paragraphs {
                    paragraph.fix_non_finite();
                }
            }
            RunItem::Text(_)
            | RunItem::Tab
            | RunItem::LineBreak
            | RunItem::AutoNum(_)
            | RunItem::Control(_)
            | RunItem::Compose(_)
            | RunItem::Skip(_)
            | RunItem::PageHiding { .. }
            | RunItem::NewNum(_)
            | RunItem::PageNum { .. } => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[serde(rename = "charPrID")]
    pub char_pr_id: i32,
    pub items: Vec<RunItem>,
    pub supscript: bool,
}

impl Run {
    pub(crate) fn fix_non_finite(&mut self) {
        for item in &mut self.items {
            item.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    #[serde(rename = "paraPrID")]
    pub para_pr_id: i32,
    #[serde(rename = "styleID")]
    pub style_id: i32,
    pub runs: Vec<Run>,
    pub line_segs: Vec<LineSeg>,
    pub page_break: bool,
    pub column_break: bool,
    pub memos: Vec<Memo>,
    pub head_text: Option<String>,
    pub columns: Option<ColumnDef>,
    pub range_tags: Vec<RangeTag>,
    pub markpens: Vec<Markpen>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<TrackMark>,
    /// HWPX 구역 XML에서 이 문단의 자리. HWP 5와 바탕쪽 문단은 `None`이다.
    #[serde(skip)]
    pub origin: Option<ParagraphOrigin>,
}

/// HWPX 문단이 구역 XML 어디에 있는가. 글 고치기가 찾을 글 없이 문단을 가리킬 때 쓴다
/// (`hwpx::edit::fill_empty_paragraph`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParagraphOrigin {
    /// 구역 파일 번호(`Contents/section0.xml`이면 0).
    pub section: u32,
    /// 그 구역 XML에서 문단(`p`) 요소가 나오는 차례(0부터, 표 칸 안 문단도 센다).
    pub ordinal: u32,
}

impl Paragraph {
    pub(crate) fn fix_non_finite(&mut self) {
        if let Some(columns) = &mut self.columns {
            columns.fix_non_finite();
        }
        for run in &mut self.runs {
            run.fix_non_finite();
        }
        for line_seg in &mut self.line_segs {
            line_seg.fix_non_finite();
        }
        for memo in &mut self.memos {
            memo.fix_non_finite();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub page: Page,
    pub paragraphs: Vec<Paragraph>,
    pub header: Vec<Paragraph>,
    pub footer: Vec<Paragraph>,
    pub columns: i32,
    pub column_gap: HwpUnit,
    pub tab_stop: HwpUnit,
    /// 구역의 개요 번호 모양. 문단 개요는 이 번호 모양을 따른다.
    #[serde(skip)]
    pub outline_numbering_id: Option<i32>,
    pub footnote_shape: NoteShape,
    pub page_border_fills: Vec<PageBorderFill>,
    /// 바탕쪽. 쪽마다 본문 뒤에 깔린다.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub master_pages: Vec<MasterPage>,
}

/// 바탕쪽 하나: 놓일 쪽(`BOTH`·`ODD`·`EVEN`)과 내용.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterPage {
    pub applies: String,
    pub paragraphs: Vec<Paragraph>,
}

impl Default for Section {
    fn default() -> Self {
        Self {
            page: Page::default(),
            paragraphs: Vec::new(),
            header: Vec::new(),
            footer: Vec::new(),
            columns: 1,
            column_gap: 1134.0,
            tab_stop: 4000.0,
            outline_numbering_id: None,
            footnote_shape: NoteShape::default(),
            page_border_fills: Vec::new(),
            master_pages: Vec::new(),
        }
    }
}

impl Section {
    pub(crate) fn fix_non_finite(&mut self) {
        self.page.fix_non_finite();
        for paragraph in self
            .paragraphs
            .iter_mut()
            .chain(&mut self.header)
            .chain(&mut self.footer)
            .chain(
                self.master_pages
                    .iter_mut()
                    .flat_map(|master| &mut master.paragraphs),
            )
        {
            paragraph.fix_non_finite();
        }
        fix(&mut self.column_gap, 1134.0);
        fix(&mut self.tab_stop, 4000.0);
        self.footnote_shape.fix_non_finite();
        for fill in &mut self.page_border_fills {
            fill.offsets.fix_non_finite();
        }
    }
}
