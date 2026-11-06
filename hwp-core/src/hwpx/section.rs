use crate::hwp5::docinfo::SHAPE_LINE_TYPES;
use crate::hwp5::table::normalize_grid;
use crate::model::{
    Caption, Cell, ColumnDef, Compose, Equation, HwpUnit, LineSeg, Margins, MasterPage, Note,
    NoteShape, Page, Paragraph, Picture, Pos, Run, RunItem, Section, Shape, Tab, Table,
};
use crate::report::{Warning, WarningKind};
use crate::{FIXED_SPACE, NO_BREAK_SPACE};
use std::collections::BTreeMap;

use super::columns::parse_columns;
use super::dom::{self, Node};
use super::header::{color, tab_kind, tab_leader};
use super::markpen::Markpens;
use super::narrow;

struct Parser<'a> {
    section: Section,
    master_pages: &'a BTreeMap<String, Node>,
    warnings: &'a mut Vec<Warning>,
    section_index: Option<u32>,
    columns_defined: bool,
    note_count: i32,
    depth: usize,
    max_depth: usize,
    nesting_warned: bool,
}

/// 구역 XML을 문단 단위로 읽어 구역과 변경 추적 표시 유무를 돌려준다. 잘못된 XML이면 `None`이고
/// 그때까지 쌓인 경고를 남기지 않는다.
pub(super) fn read_section(
    data: &[u8],
    section_index: Option<u32>,
    max_depth: usize,
    master_pages: &BTreeMap<String, Node>,
    warnings: &mut Vec<Warning>,
) -> Option<(Section, bool)> {
    let warnings_before = warnings.len();
    let mut parser = Parser {
        section: Section::default(),
        master_pages,
        warnings,
        section_index,
        columns_defined: false,
        note_count: 0,
        depth: 0,
        max_depth,
        nesting_warned: false,
    };
    let mut paragraphs = Vec::new();
    let mut track_changes = false;
    let depth_limited = dom::for_each_root_child(data, &mut |node| {
        track_changes |= has_track_change(node);
        if node.name == "p" {
            paragraphs.push(parser.parse_paragraph(node));
        }
    });
    let mut section = parser.section;
    match depth_limited {
        None => {
            warnings.truncate(warnings_before);
            None
        }
        Some(false) => {
            section.paragraphs = paragraphs;
            Some((section, track_changes))
        }
        // 깊이 제한 경고는 구역 경고의 맨 앞에 오고 중첩 경고를 대신하는데, 다 읽어야 알 수 있다.
        Some(true) => {
            warnings.truncate(warnings_before);
            let root = dom::parse(data)?;
            let section = parse_section(&root, section_index, max_depth, master_pages, warnings);
            Some((section, track_changes))
        }
    }
}

fn has_track_change(node: &Node) -> bool {
    ["deleteBegin", "insertBegin"]
        .iter()
        .any(|marker| node.name == *marker || node.find(marker).is_some())
}

fn parse_section(
    root: &Node,
    section_index: Option<u32>,
    max_depth: usize,
    master_pages: &BTreeMap<String, Node>,
    warnings: &mut Vec<Warning>,
) -> Section {
    if root.depth_limited {
        warnings.push(Warning::new(
            WarningKind::NestingTooDeep,
            section_index,
            "XML depth limit reached; nested content omitted",
        ));
    }
    let mut parser = Parser {
        section: Section::default(),
        master_pages,
        warnings,
        section_index,
        columns_defined: false,
        note_count: 0,
        depth: 0,
        max_depth,
        nesting_warned: root.depth_limited,
    };
    let paragraphs = root
        .all("p")
        .map(|node| parser.parse_paragraph(node))
        .collect();
    parser.section.paragraphs = paragraphs;
    parser.section
}

impl Parser<'_> {
    fn enter(&mut self) -> bool {
        if self.depth >= self.max_depth {
            if !self.nesting_warned {
                self.warnings.push(Warning::new(
                    WarningKind::NestingTooDeep,
                    self.section_index,
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

    fn parse_paragraph(&mut self, node: &Node) -> Paragraph {
        if !self.enter() {
            return Paragraph::default();
        }
        let mut paragraph = Paragraph {
            para_pr_id: narrow(node.int("paraPrIDRef").unwrap_or(0)),
            style_id: narrow(node.int("styleIDRef").unwrap_or(0)),
            page_break: node.bool("pageBreak"),
            column_break: node.bool("columnBreak"),
            ..Paragraph::default()
        };
        let mut markpens = Markpens::default();
        for run_node in node.all("run") {
            let mut run = Run {
                char_pr_id: narrow(run_node.int("charPrIDRef").unwrap_or(0)),
                ..Run::default()
            };
            for child in &run_node.children {
                let start = run.items.len();
                if markpens.marker(child) {
                    continue;
                }
                match child.name.as_str() {
                    "t" => {
                        read_text(child, &mut run.items, &mut markpens);
                        continue;
                    }
                    "tab" => run.items.push(tab_item(child)),
                    "tbl" => run.items.push(RunItem::Table(self.parse_table(child))),
                    "pic" => run
                        .items
                        .push(RunItem::Picture(parse_picture(child, &mut |paragraph| {
                            self.parse_paragraph(paragraph)
                        }))),
                    "rect" | "ellipse" | "line" | "polygon" | "curve" | "arc" | "container"
                    | "textart" | "ole" | "chart" | "connectLine" => {
                        run.items.push(RunItem::Shape(self.parse_shape(child)))
                    }
                    "equation" => run.items.push(RunItem::Shape(parse_equation(child))),
                    "compose" => run.items.push(RunItem::Compose(Compose {
                        text: child.attr("composeText").unwrap_or_default().to_string(),
                        shape: child.attr("circleType").unwrap_or_default().to_string(),
                        char_size: narrow(child.int("charSz").unwrap_or(0)),
                        overlap: child.attr("composeType") == Some("OVERLAP"),
                    })),
                    "secPr" => {
                        self.parse_sec_pr(child);
                        // 구역 정의도 줄 배치 번호에서 컨트롤 하나(8칸)를 차지한다.
                        run.items.push(RunItem::Skip(8));
                    }
                    "ctrl" => self.parse_ctrl(child, &mut run.items, &mut paragraph.columns),
                    "fieldBegin" | "fieldEnd" | "bookmark" => {}
                    _ => {}
                }
                markpens.advance(&run.items[start..]);
            }
            paragraph.runs.push(run);
        }
        let (ranges, tracks, malformed) = markpens.finish();
        paragraph.markpens = ranges;
        paragraph.tracks = tracks;
        if malformed {
            self.warnings.push(Warning::new(
                WarningKind::InvalidXml,
                self.section_index,
                "markpen markers are unbalanced or have an invalid color",
            ));
        }
        if let Some(lines) = node.first("linesegarray") {
            for line in lines.all("lineseg") {
                let mut segment = LineSeg {
                    textpos: narrow(line.int("textpos").unwrap_or(0)),
                    vertpos: line.num("vertpos").unwrap_or(0.0),
                    vertsize: line.num("vertsize").unwrap_or(0.0),
                    textheight: line.num("textheight").unwrap_or(0.0),
                    baseline: line.num("baseline").unwrap_or(0.0),
                    spacing: line.num("spacing").unwrap_or(0.0),
                    horzpos: line.num("horzpos").unwrap_or(0.0),
                    horzsize: line.num("horzsize").unwrap_or(0.0),
                    flags: 0,
                };
                if let Some(flags) = line.int("flags") {
                    match u32::try_from(flags) {
                        Ok(value) => segment.flags = value,
                        Err(_) => self.warnings.push(Warning::new(
                            WarningKind::InvalidLineSegFlags,
                            self.section_index,
                            "hp:lineseg flags are out of range",
                        )),
                    }
                }
                paragraph.line_segs.push(segment);
            }
        }
        self.depth -= 1;
        paragraph
    }

    fn parse_sec_pr(&mut self, node: &Node) {
        self.section.outline_numbering_id = node.int("outlineShapeIDRef").map(narrow);
        self.section.page_border_fills = node
            .all("pageBorderFill")
            .map(super::page_border_fill::parse_fill)
            .collect();
        if let Some(tab_stop) = node.num("tabStopVal").or_else(|| node.num("tabStop")) {
            if tab_stop > 0.0 {
                self.section.tab_stop = tab_stop;
            }
        }
        if let Some(footnote_pr) = node.first("footNotePr") {
            self.parse_footnote_pr(footnote_pr);
        }
        let master_pages = self.master_pages;
        for reference in node.all("masterPage") {
            let Some(root) = reference.attr("idRef").and_then(|id| master_pages.get(id)) else {
                continue;
            };
            let paragraphs = self.sub_list_paragraphs(root);
            self.section.master_pages.push(MasterPage {
                applies: root.attr("type").unwrap_or("BOTH").to_string(),
                paragraphs,
            });
        }
        let Some(page_pr) = node.first("pagePr") else {
            return;
        };
        let mut page = Page::default();
        if let Some(width) = page_pr.num("width") {
            page.width = width;
        }
        if let Some(height) = page_pr.num("height") {
            page.height = height;
        }
        page.landscape = page_pr.attr("landscape").unwrap_or("WIDELY") != "WIDELY";
        if let Some(margin) = page_pr.first("margin") {
            if let Some(value) = margin.num("left") {
                page.margin_left = value;
            }
            if let Some(value) = margin.num("right") {
                page.margin_right = value;
            }
            if let Some(value) = margin.num("top") {
                page.margin_top = value;
            }
            if let Some(value) = margin.num("bottom") {
                page.margin_bottom = value;
            }
            if let Some(value) = margin.num("header") {
                page.header = value;
            }
            if let Some(value) = margin.num("footer") {
                page.footer = value;
            }
            page.gutter = margin.num("gutter").unwrap_or(0.0);
        }
        if page.landscape && page.width < page.height {
            std::mem::swap(&mut page.width, &mut page.height);
        }
        self.section.page = page;
    }

    fn parse_ctrl(
        &mut self,
        node: &Node,
        items: &mut Vec<RunItem>,
        columns: &mut Option<ColumnDef>,
    ) {
        for child in &node.children {
            match child.name.as_str() {
                "header" => {
                    let paragraphs = self.sub_list_paragraphs(child);
                    if self.section.header.is_empty() {
                        self.section.header = paragraphs.clone();
                    }
                    items.push(RunItem::Header {
                        paragraphs,
                        applies: child.attr("applyPageType").unwrap_or("BOTH").to_string(),
                    });
                }
                "footer" => {
                    let paragraphs = self.sub_list_paragraphs(child);
                    if self.section.footer.is_empty() {
                        self.section.footer = paragraphs.clone();
                    }
                    items.push(RunItem::Footer {
                        paragraphs,
                        applies: child.attr("applyPageType").unwrap_or("BOTH").to_string(),
                    });
                }
                "colPr" => {
                    if self.depth == 1 {
                        let definition = parse_columns(child);
                        if !self.columns_defined {
                            self.section.columns = definition.count;
                            self.section.column_gap = definition.gap;
                            self.columns_defined = true;
                        }
                        if columns.is_none() {
                            *columns = Some(definition);
                        }
                    }
                    items.push(RunItem::Skip(8));
                }
                "pageHiding" => items.push(RunItem::PageHiding {
                    header: child.bool("hideHeader"),
                    footer: child.bool("hideFooter"),
                    page_num: child.bool("hidePageNum"),
                }),
                "newNum" => {
                    if child.attr("numType").unwrap_or("PAGE") == "PAGE" {
                        items.push(RunItem::NewNum(narrow(child.int("num").unwrap_or(1))));
                    }
                }
                "autoNum" => items.push(RunItem::AutoNum(
                    child.attr("numType").unwrap_or("PAGE").to_string(),
                )),
                "pageNum" => items.push(RunItem::PageNum {
                    pos: child.attr("pos").unwrap_or("BOTTOM_CENTER").to_string(),
                    format: child.attr("formatType").unwrap_or("DIGIT").to_string(),
                    side: child.attr("sideChar").unwrap_or("").to_string(),
                }),
                "footNote" => items.push(RunItem::Note(self.parse_note(child, false))),
                "endNote" => items.push(RunItem::Note(self.parse_note(child, true))),
                "pageNumCtrl" | "#text" => {}
                _ => items.push(RunItem::Control(format!("hp:{}", child.name))),
            }
        }
    }

    fn parse_note(&mut self, node: &Node, endnote: bool) -> Note {
        let mut note = Note {
            endnote,
            number: narrow(node.int("number").unwrap_or(1)),
            ..Note::default()
        };
        self.note_count += 1;
        note.seq = self.note_count;
        let mut format = None;
        if let Some(list) = node.first("subList") {
            // 번호(autoNum)는 문단에 그대로 둔다. 한/글은 각주 글 앞에 "1)"처럼 찍고, 배치는 그 자리에서 번호를 그린다.
            let auto_num = list
                .all("p")
                .next()
                .and_then(|paragraph| paragraph.find("autoNum"));
            if let Some(auto_num) = auto_num {
                if let Some(auto_format) = auto_num.find("autoNumFormat") {
                    note.prefix = auto_format.attr("prefixChar").unwrap_or("").to_string();
                    note.suffix = auto_format.attr("suffixChar").unwrap_or("").to_string();
                    format = auto_format.attr("type").map(note_format_code);
                }
            }
            note.paragraphs = list
                .all("p")
                .map(|paragraph| self.parse_paragraph(paragraph))
                .collect();
        }
        note.format = match format {
            Some(format) => format,
            None if endnote => 0,
            None => self.section.footnote_shape.format,
        };
        note
    }

    fn parse_footnote_pr(&mut self, node: &Node) {
        let mut shape = NoteShape::default();
        if let Some(format) = node.find("autoNumFormat") {
            if let Some(kind) = format.attr("type") {
                shape.format = note_format_code(kind);
            }
        }
        if let Some(line) = node.find("noteLine") {
            if let Some(length) = line.num("length") {
                shape.line_length = length;
            }
            shape.line_type = note_line_type(line.attr("type"));
            shape.line_width = note_width(line.attr("width"));
            shape.line_color = color(line.attr("color")).unwrap_or(0);
        }
        if let Some(spacing) = node.find("noteSpacing") {
            shape.above_line = spacing.num("aboveLine").unwrap_or(shape.above_line);
            shape.below_line = spacing.num("belowLine").unwrap_or(shape.below_line);
            shape.between_notes = spacing.num("betweenNotes").unwrap_or(shape.between_notes);
        }
        self.section.footnote_shape = shape;
    }

    fn sub_list_paragraphs(&mut self, node: &Node) -> Vec<Paragraph> {
        let Some(list) = node.first("subList") else {
            return Vec::new();
        };
        list.all("p")
            .map(|paragraph| self.parse_paragraph(paragraph))
            .collect()
    }

    fn parse_table(&mut self, node: &Node) -> Table {
        let mut table = Table {
            rows: narrow(node.int("rowCnt").unwrap_or(0)),
            cols: narrow(node.int("colCnt").unwrap_or(0)),
            cell_spacing: node.num("cellSpacing").unwrap_or(0.0),
            border_fill_id: narrow(node.int("borderFillIDRef").unwrap_or(0)),
            z_order: narrow(node.int("zOrder").unwrap_or(0)),
            repeat_header: node.bool("repeatHeader"),
            page_break_mode: node.attr("pageBreak").unwrap_or("CELL").to_string(),
            ..Table::default()
        };
        if let Some(size) = node.first("sz") {
            table.width = size.num("width").unwrap_or(0.0);
            table.height = size.num("height").unwrap_or(0.0);
        }
        table.pos = parse_pos(node);
        table.out_margins = parse_margins(node.first("outMargin"));
        table.in_margins = parse_margins(node.first("inMargin"));
        table.caption = parse_caption(node.first("caption"), &mut |paragraph| {
            self.parse_paragraph(paragraph)
        });
        if let Some(text_wrap) = node.attr("textWrap") {
            table.pos.text_wrap = text_wrap.to_string();
        }
        for row in node.all("tr") {
            for cell_node in row.all("tc") {
                let mut cell = Cell {
                    border_fill_id: narrow(cell_node.int("borderFillIDRef").unwrap_or(0)),
                    header: cell_node.bool("header"),
                    ..Cell::default()
                };
                if let Some(address) = cell_node.first("cellAddr") {
                    cell.col = narrow(address.int("colAddr").unwrap_or(0));
                    cell.row = narrow(address.int("rowAddr").unwrap_or(0));
                }
                if let Some(span) = cell_node.first("cellSpan") {
                    cell.col_span = narrow(span.int("colSpan").unwrap_or(1));
                    cell.row_span = narrow(span.int("rowSpan").unwrap_or(1));
                }
                if let Some(size) = cell_node.first("cellSz") {
                    cell.width = size.num("width").unwrap_or(0.0);
                    cell.height = size.num("height").unwrap_or(0.0);
                }
                let margin = cell_node.first("cellMargin");
                cell.has_margin = cell_node
                    .attr("hasMargin")
                    .map_or(margin.is_some(), |value| matches!(value, "1" | "true"));
                if cell.has_margin {
                    cell.margins = parse_margins(margin);
                } else {
                    cell.margins = table.in_margins.clone();
                }
                if let Some(list) = cell_node.first("subList") {
                    cell.vert_align = list.attr("vertAlign").unwrap_or("TOP").to_string();
                    cell.paragraphs = list
                        .all("p")
                        .map(|paragraph| self.parse_paragraph(paragraph))
                        .collect();
                }
                table.cells.push(cell);
            }
        }
        normalize_grid(&mut table);
        table
    }

    fn parse_shape(&mut self, node: &Node) -> Shape {
        super::shape::parse_shape(node, self)
    }
}

impl super::shape::ShapeContext for Parser<'_> {
    fn paragraph(&mut self, node: &Node) -> Paragraph {
        self.parse_paragraph(node)
    }

    fn enter(&mut self) -> bool {
        Parser::enter(self)
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

fn read_text(node: &Node, items: &mut Vec<RunItem>, markpens: &mut Markpens) {
    for child in &node.children {
        if markpens.marker(child) {
            continue;
        }
        let start = items.len();
        match child.name.as_str() {
            "#text" => push_text(items, &child.text),
            "nbSpace" => push_text(items, NO_BREAK_SPACE.encode_utf8(&mut [0; 4])),
            "fwSpace" => push_text(items, FIXED_SPACE.encode_utf8(&mut [0; 4])),
            "hyphen" => push_text(items, "-"),
            "tab" => items.push(tab_item(child)),
            "lineBreak" => items.push(RunItem::LineBreak),
            _ => push_text(items, &child.all_text()),
        }
        markpens.advance(&items[start..]);
    }
}

fn push_text(items: &mut Vec<RunItem>, text: &str) {
    if !text.is_empty() {
        items.push(RunItem::Text(text.to_string()));
    }
}

fn tab_item(node: &Node) -> RunItem {
    let width = node
        .num("width")
        .filter(|value| value.is_finite() && *value > 0.0);
    let leader = tab_leader(node.attr("leader"));
    let kind = tab_kind(node.attr("type"));
    if leader != "NONE" || kind != "LEFT" {
        return RunItem::FormattedTab(Tab {
            width,
            kind,
            leader,
        });
    }
    match width {
        Some(width) => RunItem::SizedTab(width),
        None => RunItem::Tab,
    }
}

pub(super) fn parse_picture<F>(node: &Node, paragraph: &mut F) -> Picture
where
    F: FnMut(&Node) -> Paragraph,
{
    let mut picture = Picture::default();
    if let Some(size) = node.first("sz") {
        picture.width = size.num("width").unwrap_or(0.0);
        picture.height = size.num("height").unwrap_or(0.0);
    }
    picture.pos = parse_pos(node);
    picture.out_margins = parse_margins(node.first("outMargin"));
    if let Some(text_wrap) = node.attr("textWrap") {
        picture.pos.text_wrap = text_wrap.to_string();
    }
    picture.z_order = narrow(node.int("zOrder").unwrap_or(0));
    if let Some(image) = node.find("img") {
        picture.binary_id = image.attr("binaryItemIDRef").unwrap_or("").to_string();
    }
    if let Some(flip) = node.first("flip") {
        picture.flip_h = flip.bool("horizontal");
        picture.flip_v = flip.bool("vertical");
    }
    if let Some(rotation) = node.first("rotationInfo") {
        picture.rotation = rotation.num("angle").unwrap_or(0.0);
    }
    if let Some(clip) = node.first("imgClip") {
        let left = clip.num("left").unwrap_or(0.0);
        let top = clip.num("top").unwrap_or(0.0);
        let right = clip.num("right").unwrap_or(0.0);
        let bottom = clip.num("bottom").unwrap_or(0.0);
        if right > left && bottom > top {
            picture.clip = Some([[left, top], [right - left, bottom - top]]);
        }
    }
    if let Some(dimension) = node.first("imgDim") {
        picture.orig_width = dimension.num("dimwidth").unwrap_or(0.0);
        picture.orig_height = dimension.num("dimheight").unwrap_or(0.0);
    }
    picture.caption = parse_caption(node.first("caption"), paragraph);
    // 한/글은 lineShape가 적힌 그림에 테두리를 그린다. 굵기 0은 가장 가는 선(0.1mm)이다.
    if let Some(line) = node.first("lineShape") {
        let style = line.attr("style").unwrap_or("SOLID");
        if style != "NONE" {
            let width = line.num("width").unwrap_or(0.0) / 7200.0 * 25.4;
            picture.outline = Some(crate::model::Border {
                border_type: style.to_string(),
                width: width.max(0.1),
                color: color(line.attr("color")).unwrap_or(0),
            });
        }
    }
    picture
}

pub(super) fn parse_caption<F>(node: Option<&Node>, paragraph: &mut F) -> Option<Caption>
where
    F: FnMut(&Node) -> Paragraph,
{
    let node = node?;
    let mut caption = Caption {
        side: node.attr("side").unwrap_or("LEFT").to_string(),
        gap: node.num("gap").unwrap_or(850.0),
        width: node.num("width").unwrap_or(0.0),
        full_size: node.bool("fullSz"),
        paragraphs: Vec::new(),
    };
    if let Some(list) = node.first("subList") {
        caption.paragraphs = list.all("p").map(&mut *paragraph).collect();
    }
    Some(caption)
}

fn parse_equation(node: &Node) -> Shape {
    let mut shape = Shape {
        kind: "equation".to_string(),
        line_type: "NONE".to_string(),
        ..Shape::default()
    };
    if let Some(size) = node.first("sz") {
        shape.width = size.num("width").unwrap_or(0.0);
        shape.height = size.num("height").unwrap_or(0.0);
    }
    shape.pos = parse_pos(node);
    shape.out_margins = parse_margins(node.first("outMargin"));
    shape.z_order = narrow(node.int("zOrder").unwrap_or(0));
    if let Some(text_wrap) = node.attr("textWrap") {
        shape.pos.text_wrap = text_wrap.to_string();
    }
    let mut equation = Equation {
        script: node.first("script").map(Node::all_text).unwrap_or_default(),
        ..Equation::default()
    };
    if let Some(unit) = node.num("baseUnit") {
        if unit > 0.0 && unit < 100_000.0 {
            equation.size = unit;
        }
    }
    equation.color = color(node.attr("textColor")).unwrap_or(0);
    if let Some(baseline) = node.num("baseLine") {
        equation.baseline = baseline;
    }
    equation.font = node.attr("font").unwrap_or("").to_string();
    shape.equation = Some(equation);
    shape
}

pub(super) fn parse_pos(object: &Node) -> Pos {
    let mut pos = Pos::default();
    if let Some(size) = object.first("sz") {
        pos.width_rel_to = size.attr("widthRelTo").unwrap_or("ABSOLUTE").to_string();
        pos.height_rel_to = size.attr("heightRelTo").unwrap_or("ABSOLUTE").to_string();
    }
    let Some(node) = object.first("pos") else {
        return pos;
    };
    pos.treat_as_char = node.bool("treatAsChar");
    pos.vert_rel_to = node.attr("vertRelTo").unwrap_or("PARA").to_string();
    pos.horz_rel_to = node.attr("horzRelTo").unwrap_or("COLUMN").to_string();
    pos.vert_align = node.attr("vertAlign").unwrap_or("TOP").to_string();
    pos.horz_align = node.attr("horzAlign").unwrap_or("LEFT").to_string();
    pos.vert_offset = node.num("vertOffset").unwrap_or(0.0);
    pos.horz_offset = node.num("horzOffset").unwrap_or(0.0);
    pos.flow_with_text = node.bool("flowWithText");
    pos.allow_overlap = node.bool("allowOverlap");
    pos.affect_l_spacing = node.bool("affectLSpacing");
    pos.text_wrap = node.attr("textWrap").unwrap_or("SQUARE").to_string();
    pos
}

pub(super) fn parse_margins(node: Option<&Node>) -> Margins {
    let mut margins = Margins::default();
    let Some(node) = node else {
        return margins;
    };
    margins.left = node.num("left").unwrap_or(0.0);
    margins.right = node.num("right").unwrap_or(0.0);
    margins.top = node.num("top").unwrap_or(0.0);
    margins.bottom = node.num("bottom").unwrap_or(0.0);
    margins
}

fn note_format_code(name: &str) -> i32 {
    const NAMES: [&str; 17] = [
        "DIGIT",
        "CIRCLED_DIGIT",
        "ROMAN_CAPITAL",
        "ROMAN_SMALL",
        "LATIN_CAPITAL",
        "LATIN_SMALL",
        "CIRCLED_LATIN_CAPITAL",
        "CIRCLED_LATIN_SMALL",
        "HANGUL_SYLLABLE",
        "CIRCLED_HANGUL_SYLLABLE",
        "HANGUL_JAMO",
        "CIRCLED_HANGUL_JAMO",
        "HANGUL_PHONETIC",
        "IDEOGRAPH",
        "CIRCLED_IDEOGRAPH",
        "DECAGON_CIRCLE",
        "DECAGON_CIRCLE_HANJA",
    ];
    if let Some(index) = NAMES.iter().position(|candidate| *candidate == name) {
        return index as i32;
    }
    match name {
        "SYMBOL" => 0x80,
        "USER_CHAR" => 0x81,
        _ => name.parse().unwrap_or(0),
    }
}

fn note_line_type(value: Option<&str>) -> i32 {
    let Some(value) = value else {
        return 1;
    };
    if let Ok(index) = value.parse::<i32>() {
        return index;
    }
    SHAPE_LINE_TYPES
        .iter()
        .position(|kind| *kind == value)
        .map_or(1, |index| index as i32)
}

fn note_width(value: Option<&str>) -> HwpUnit {
    let Some(value) = value else {
        return 0.12;
    };
    value
        .trim()
        .strip_suffix("mm")
        .map(str::trim)
        .and_then(|number| number.parse().ok())
        .unwrap_or(0.12)
}
