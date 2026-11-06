use crate::model::{
    Caption, Compose, Document, HwpUnit, MasterPage, Note, NoteShape, Page, Paragraph, Picture,
    RunItem, Section, Shape,
};
use crate::num;
use crate::report::Warning;

use super::columns::read_columns;
use super::para::parse_paragraph;
use super::reader::BinReader;
use super::record::Record;
pub(super) use super::shape::Common;
use super::shape::{
    apply_geometry, equation_info, group_children, is_container, parse_common, read_picture,
    read_text_box, shape_component,
};
use super::table::{parse_table, table_record};
use super::tags::{
    T_EQEDIT, T_FOOTNOTE_SHAPE, T_LIST_HEADER, T_PAGE_DEF, T_PARA_HEADER, T_SC_PICTURE,
    T_SHAPE_COMPONENT,
};

const BORDER_WIDTHS: [HwpUnit; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];
const APPLIES: [&str; 4] = ["BOTH", "EVEN", "ODD", "UNKNOWN"];
const PAGE_NUMBER_POSITIONS: [&str; 11] = [
    "NONE",
    "TOP_LEFT",
    "TOP_CENTER",
    "TOP_RIGHT",
    "BOTTOM_LEFT",
    "BOTTOM_CENTER",
    "BOTTOM_RIGHT",
    "OUTSIDE_TOP",
    "OUTSIDE_BOTTOM",
    "INSIDE_TOP",
    "INSIDE_BOTTOM",
];

pub struct ParseState {
    pub(super) max_bin: usize,
    columns_defined: bool,
    note_count: i32,
    depth: u32,
    max_depth: u32,
    nesting_warned: bool,
    pub(super) lone_surrogate_count: usize,
}

impl Default for ParseState {
    fn default() -> Self {
        Self {
            max_bin: 0,
            columns_defined: false,
            note_count: 0,
            depth: 0,
            max_depth: crate::ParseOptions::default().max_depth.into(),
            nesting_warned: false,
            lone_surrogate_count: 0,
        }
    }
}

pub struct ParseContext<'a> {
    pub(crate) document: &'a Document,
    pub(crate) section: &'a mut Section,
    pub(crate) state: &'a mut ParseState,
    pub(crate) section_index: Option<u32>,
    pub(crate) warnings: &'a mut Vec<Warning>,
}

impl<'a> ParseContext<'a> {
    pub fn new(
        document: &'a Document,
        section: &'a mut Section,
        state: &'a mut ParseState,
        section_index: Option<u32>,
        warnings: &'a mut Vec<Warning>,
    ) -> Self {
        Self {
            document,
            section,
            state,
            section_index,
            warnings,
        }
    }
}

impl ParseState {
    pub(super) fn is_top_level(&self) -> bool {
        self.depth == 0
    }

    pub fn with_max_bin(mut self, max_bin: usize) -> Self {
        self.max_bin = max_bin;
        self
    }

    pub fn with_max_depth(mut self, max_depth: u8) -> Self {
        self.max_depth = u32::from(max_depth);
        self
    }

    pub(super) fn enter(&mut self, warnings: &mut Vec<Warning>, section: Option<u32>) -> bool {
        if self.depth >= self.max_depth {
            if !self.nesting_warned {
                warnings.push(Warning::new(
                    crate::WarningKind::NestingTooDeep,
                    section,
                    format!(
                        "nesting depth limit {} reached; nested content omitted",
                        self.max_depth
                    ),
                ));
                self.nesting_warned = true;
            }
            return false;
        }
        self.depth += 1;
        true
    }

    pub(super) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

pub(super) struct ListGroup<'a> {
    pub header: &'a Record,
    pub paragraphs: Vec<&'a Record>,
}

pub(super) fn list_groups(children: &[Record]) -> Vec<ListGroup<'_>> {
    let mut groups = Vec::new();
    for child in children {
        if child.tag == T_LIST_HEADER {
            groups.push(ListGroup {
                header: child,
                paragraphs: child
                    .children
                    .iter()
                    .filter(|paragraph| paragraph.tag == T_PARA_HEADER)
                    .collect(),
            });
        } else if child.tag == T_PARA_HEADER {
            if let Some(group) = groups.last_mut() {
                group.paragraphs.push(child);
            }
        }
    }
    groups
}

pub(super) fn list_attr(header: &Record) -> u32 {
    let mut reader = header.reader();
    let _ = reader.i16();
    let _ = reader.u16();
    reader.u32()
}

pub(super) fn parse_caption(group: &ListGroup<'_>, context: &mut ParseContext<'_>) -> Caption {
    let mut reader = group.header.reader();
    let _ = reader.u16();
    let _ = reader.u32();
    let _ = reader.u16();
    let props = reader.u32();
    let width = HwpUnit::from(reader.u32());
    let gap = HwpUnit::from(reader.u16());
    let _ = reader.u32();
    let side = match props & 3 {
        0 => "LEFT",
        1 => "RIGHT",
        2 => "TOP",
        _ => "BOTTOM",
    };
    Caption {
        side: side.to_string(),
        gap,
        width,
        full_size: props & 4 != 0,
        paragraphs: parse_nested(&group.paragraphs, context),
    }
}

pub(super) fn parse_nested(records: &[&Record], context: &mut ParseContext<'_>) -> Vec<Paragraph> {
    let section = context.section_index;
    if !context.state.enter(context.warnings, section) {
        return Vec::new();
    }
    let paragraphs = records
        .iter()
        .map(|record| parse_paragraph(record, context))
        .collect();
    context.state.leave();
    paragraphs
}

fn fourcc(value: u32) -> String {
    let bytes = value.to_be_bytes();
    if bytes.is_ascii() {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        "????".to_string()
    }
}

pub(super) fn rgb(color: u32) -> u32 {
    ((color & 0xFF) << 16) | (color & 0xFF00) | ((color >> 16) & 0xFF)
}

fn decoration(value: u16) -> String {
    if value == 0 {
        return String::new();
    }
    char::from_u32(u32::from(value))
        .map(String::from)
        .unwrap_or_default()
}

fn parse_page_def(data: &[u8]) -> Page {
    let mut reader = BinReader::new(data);
    let mut page = Page {
        width: HwpUnit::from(reader.u32()),
        height: HwpUnit::from(reader.u32()),
        margin_left: HwpUnit::from(reader.u32()),
        margin_right: HwpUnit::from(reader.u32()),
        margin_top: HwpUnit::from(reader.u32()),
        margin_bottom: HwpUnit::from(reader.u32()),
        header: HwpUnit::from(reader.u32()),
        footer: HwpUnit::from(reader.u32()),
        gutter: HwpUnit::from(reader.u32()),
        landscape: false,
    };
    page.landscape = reader.u32() & 1 != 0;
    if page.landscape && page.width < page.height {
        std::mem::swap(&mut page.width, &mut page.height);
    }
    page
}

pub fn note_shape(data: &[u8]) -> NoteShape {
    let mut shape = NoteShape::default();
    let mut reader = BinReader::new(data);
    if reader.remaining() < 12 {
        return shape;
    }
    shape.format = (reader.u32() & 0xFF) as i32;
    let _ = reader.u16();
    let _ = reader.u16();
    let _ = reader.u16();
    let _ = reader.u16();
    if reader.remaining() < 16 {
        return shape;
    }
    shape.line_length = HwpUnit::from(reader.i32());
    shape.above_line = HwpUnit::from(reader.u16());
    shape.below_line = HwpUnit::from(reader.u16());
    shape.between_notes = HwpUnit::from(reader.u16());
    shape.line_type = i32::from(reader.u8());
    shape.line_width = BORDER_WIDTHS[usize::from(reader.u8()).min(15)];
    if reader.remaining() >= 4 {
        shape.line_color = rgb(reader.u32());
    }
    shape
}

pub(super) fn auto_number_supscript(record: &Record) -> bool {
    let mut reader = record.reader();
    if reader.u32() != u32::from_be_bytes(*b"atno") {
        return false;
    }
    let attr = reader.u32();
    attr & 0xF != 0 && attr & (1 << 12) != 0
}

pub fn parse_control(record: &Record, context: &mut ParseContext<'_>) -> Option<RunItem> {
    let mut reader = record.reader();
    let id = fourcc(reader.u32());
    match id.as_str() {
        "tbl " => {
            let common = parse_common(&mut reader);
            let _ = reader.u32();
            let table_index = table_record(&record.children)?;
            let caption = list_groups(&record.children[..table_index])
                .into_iter()
                .next()
                .map(|group| parse_caption(&group, context));
            let table = parse_table(
                &record.children[table_index],
                &record.children[table_index + 1..],
                &common,
                caption,
                context,
            );
            Some(RunItem::Table(table))
        }
        "gso " => {
            let common = parse_common(&mut reader);
            let component_record = record
                .children
                .iter()
                .find(|child| child.tag == T_SHAPE_COMPONENT)?;
            let caption = list_groups(&record.children)
                .into_iter()
                .next()
                .map(|group| parse_caption(&group, context));
            if let Some(picture_record) = component_record
                .children
                .iter()
                .find(|child| child.tag == T_SC_PICTURE)
            {
                let mut picture = Picture {
                    pos: common.pos,
                    width: common.width,
                    height: common.height,
                    out_margins: common.margins,
                    z_order: common.z_order,
                    caption,
                    ..Picture::default()
                };
                read_picture(&mut picture, picture_record, context.state.max_bin);
                return Some(RunItem::Picture(picture));
            }
            let mut shape = Shape {
                pos: common.pos,
                width: common.width,
                height: common.height,
                out_margins: common.margins,
                z_order: common.z_order,
                caption,
                ..Shape::default()
            };
            let container = is_container(component_record);
            let component = shape_component(&component_record.data, container);
            if component.cur_w > 0.0
                && component.cur_h > 0.0
                && component.cur_w < 1_000_000.0
                && component.cur_h < 1_000_000.0
            {
                shape.width = component.cur_w;
                shape.height = component.cur_h;
            }
            if container {
                shape.kind = "container".to_string();
                shape.line_type = "NONE".to_string();
                shape.children = group_children(component_record, context);
                return Some(RunItem::Shape(shape));
            }
            apply_geometry(&mut shape, component_record, &component, [0.0, 0.0]);
            if let Some(group) = list_groups(&component_record.children).into_iter().next() {
                read_text_box(&mut shape, &group, context);
            }
            Some(RunItem::Shape(shape))
        }
        "eqed" => {
            let common = parse_common(&mut reader);
            let equation_record = record.children.iter().find(|child| child.tag == T_EQEDIT)?;
            let equation = equation_info(&equation_record.data)?;
            Some(RunItem::Shape(Shape {
                kind: "equation".to_string(),
                pos: common.pos,
                width: common.width,
                height: common.height,
                out_margins: common.margins,
                z_order: common.z_order,
                line_type: "NONE".to_string(),
                equation: Some(equation),
                ..Shape::default()
            }))
        }
        "secd" => {
            if reader.remaining() >= 16 {
                reader.skip(14);
                context.section.outline_numbering_id = Some(i32::from(reader.u16()));
            }
            context.section.page_border_fills = super::page_border_fill::parse_fills(
                &record.children,
                context.section_index,
                context.warnings,
            );
            if let Some(page_def) = record.children.iter().find(|child| child.tag == T_PAGE_DEF) {
                context.section.page = parse_page_def(&page_def.data);
            }
            if let Some(footnote) = record
                .children
                .iter()
                .find(|child| child.tag == T_FOOTNOTE_SHAPE)
            {
                context.section.footnote_shape = note_shape(&footnote.data);
            }
            // 구역 정의 아래의 문단 목록은 바탕쪽이다. 하나면 양쪽, 여럿이면 양쪽·홀수·짝수 차례로 본다.
            let groups = list_groups(&record.children);
            let count = groups.len();
            for (index, group) in groups.into_iter().enumerate() {
                let applies = if count == 1 {
                    "BOTH"
                } else {
                    ["BOTH", "ODD", "EVEN"]
                        .get(index)
                        .copied()
                        .unwrap_or("BOTH")
                };
                let paragraphs = parse_nested(&group.paragraphs, context);
                context.section.master_pages.push(MasterPage {
                    applies: applies.to_string(),
                    paragraphs,
                });
            }
            None
        }
        "cold" => {
            if context.state.is_top_level() && !context.state.columns_defined {
                if let Some(columns) = read_columns(&mut reader) {
                    context.section.columns = columns.count;
                    context.section.column_gap = columns.gap;
                    context.state.columns_defined = true;
                }
            }
            None
        }
        "head" | "foot" => {
            let group = list_groups(&record.children).into_iter().next()?;
            let paragraphs = parse_nested(&group.paragraphs, context);
            if id == "head" && context.section.header.is_empty() {
                context.section.header = paragraphs.clone();
            }
            if id == "foot" && context.section.footer.is_empty() {
                context.section.footer = paragraphs.clone();
            }
            let attr = if reader.remaining() >= 4 {
                reader.u32()
            } else {
                u32::MAX
            };
            let applies = APPLIES[(attr & 3) as usize].to_string();
            if id == "head" {
                Some(RunItem::Header {
                    paragraphs,
                    applies,
                })
            } else {
                Some(RunItem::Footer {
                    paragraphs,
                    applies,
                })
            }
        }
        "pgnp" => {
            let attr = reader.u32();
            let _ = reader.u16();
            let _ = reader.u16();
            let _ = reader.u16();
            Some(RunItem::PageNum {
                pos: PAGE_NUMBER_POSITIONS
                    .get(((attr >> 8) & 0xF) as usize)
                    .copied()
                    .unwrap_or("BOTTOM_CENTER")
                    .to_string(),
                format: num::format_name((attr & 0xFF) as u8)
                    .unwrap_or("DIGIT")
                    .to_string(),
                side: decoration(reader.u16()),
            })
        }
        "pghd" => {
            let attr = reader.u32();
            Some(RunItem::PageHiding {
                header: attr & 1 != 0,
                footer: attr & 2 != 0,
                page_num: attr & (1 << 5) != 0,
            })
        }
        "atno" | "nwno" => {
            let attr = reader.u32();
            let kind = attr & 0xF;
            let number = i32::from(reader.u16());
            if id == "nwno" {
                return (kind == 0).then(|| RunItem::NewNum(number.max(1)));
            }
            if kind == 0 {
                return Some(RunItem::AutoNum("PAGE".to_string()));
            }
            let _ = reader.u16();
            let prefix = decoration(reader.u16());
            let suffix = decoration(reader.u16());
            let format = num::format_name(((attr >> 4) & 0xFF) as u8).unwrap_or("DIGIT");
            (number > 0).then(|| {
                let number = num::format_number(i64::from(number), format);
                RunItem::Text(format!("{prefix}{number}{suffix}"))
            })
        }
        "fn  " | "en  " => {
            let mut note = Note {
                endnote: id == "en  ",
                number: reader.u32() as i32,
                prefix: decoration(reader.u16()),
                suffix: decoration(reader.u16()),
                ..Note::default()
            };
            context.state.note_count += 1;
            note.seq = context.state.note_count;
            if !note.endnote {
                note.format = context.section.footnote_shape.format;
            }
            if let Some(group) = list_groups(&record.children).into_iter().next() {
                note.paragraphs = parse_nested(&group.paragraphs, context);
            }
            Some(RunItem::Note(note))
        }
        "tcmp" => {
            let text = reader.hwp_string();
            let shape = COMPOSE_SHAPES
                .get(usize::from(reader.u8()))
                .copied()
                .unwrap_or_default()
                .to_string();
            let char_size = i32::from(reader.i8());
            let overlap = reader.u8() == 0;
            Some(RunItem::Compose(Compose {
                text,
                shape,
                char_size,
                overlap,
            }))
        }
        _ => Some(RunItem::Control(id)),
    }
}

/// HWP 5 글자 겹침의 테두리 종류 번호 차례. HWPX `circleType` 이름으로 옮긴다.
const COMPOSE_SHAPES: [&str; 13] = [
    "SHAPE_CIRCLE",
    "SHAPE_REVERSAL_CIRCLE",
    "SHAPE_RECTANGLE",
    "SHAPE_REVERSAL_RECTANGLE",
    "SHAPE_TRIANGLE",
    "SHAPE_REVERSAL_TRIANGLE",
    "SHAPE_LIGHT",
    "SHAPE_RHOMBUS",
    "SHAPE_REVERSAL_RHOMBUS",
    "SHAPE_ROUNDED_RECT",
    "SHAPE_EMPTY_CIRCULATE_TRIANGLE",
    "SHAPE_THIN_CIRCULATE_TRIANGLE",
    "SHAPE_THICK_CIRCULATE_TRIANGLE",
];

#[cfg(test)]
mod tests {
    use super::{decoration, fourcc, rgb};

    #[test]
    fn fourcc_ascii_and_unknown() {
        assert_eq!(fourcc(u32::from_be_bytes(*b"tbl ")), "tbl ");
        assert_eq!(fourcc(0x80_00_00_01), "????");
    }

    #[test]
    fn decoration_zero_is_empty() {
        assert_eq!(decoration(0), "");
        assert_eq!(decoration(u16::from(b'*')), "*");
    }

    #[test]
    fn colorref_to_rgb() {
        assert_eq!(rgb(0x0000FF), 0xFF0000);
        assert_eq!(rgb(0x123456), 0x563412);
    }

    #[test]
    fn nesting_limit_emits_one_warning() {
        let mut state = crate::hwp5::control::ParseState::default().with_max_depth(1);
        let mut warnings = Vec::new();
        assert!(state.enter(&mut warnings, Some(0)));
        assert!(!state.enter(&mut warnings, Some(0)));
        assert!(!state.enter(&mut warnings, Some(0)));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, crate::WarningKind::NestingTooDeep);
        state.leave();
        assert!(state.enter(&mut warnings, Some(0)));
    }
}
