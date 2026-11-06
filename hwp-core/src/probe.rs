use serde::Serialize;

mod shapes;
pub use shapes::{shape_fill_distribution, ShapeFillDistribution};

use crate::model::{ColumnDef, Document, HwpUnit, LineSeg, Paragraph, RunItem, Section};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeLine {
    pub sec: usize,
    pub page: i32,
    pub col: i32,
    pub para: usize,
    pub line: usize,
    #[serde(rename = "y_pt")]
    pub y_pt: f64,
    #[serde(rename = "x_pt")]
    pub x_pt: f64,
    pub text: String,
    pub head: String,
}

#[derive(Debug, Clone, Copy)]
struct TablePlacement {
    treat_as_char: bool,
    height: HwpUnit,
}

#[derive(Debug, Clone)]
struct Atom {
    ch: char,
    filler: bool,
    table: Option<TablePlacement>,
}

impl Atom {
    fn text(ch: char) -> Self {
        Self {
            ch,
            filler: false,
            table: None,
        }
    }

    fn filler() -> Self {
        Self {
            ch: ' ',
            filler: true,
            table: None,
        }
    }

    fn object(ch: char, table: Option<TablePlacement>) -> Self {
        Self {
            ch,
            filler: false,
            table,
        }
    }
}

#[derive(Debug, Clone)]
struct SectionGeometry {
    body_left: HwpUnit,
    body_top: HwpUnit,
    body_width: HwpUnit,
    body_height: HwpUnit,
    columns: i32,
    gap: HwpUnit,
    widths: Vec<HwpUnit>,
}

impl SectionGeometry {
    fn new(section: &Section) -> Self {
        let page = &section.page;
        Self {
            body_left: page.margin_left + page.gutter,
            body_top: page.margin_top + page.header,
            body_width: page.width - page.margin_left - page.margin_right - page.gutter,
            body_height: page.height
                - page.margin_top
                - page.header
                - page.margin_bottom
                - page.footer,
            columns: section.columns.max(1),
            gap: section.column_gap,
            widths: Vec::new(),
        }
    }

    fn set_columns(&mut self, columns: &ColumnDef) {
        self.columns = columns.count.max(1);
        self.gap = columns.gap;
        self.widths = if columns.same_width {
            Vec::new()
        } else {
            columns.widths.clone()
        };
    }

    fn column_width(&self, column: usize) -> HwpUnit {
        if let Some(width) = self.widths.get(column) {
            return *width;
        }
        let count = HwpUnit::from(self.columns);
        (self.body_width - self.gap * (count - 1.0)) / count
    }

    fn column_left(&self, column: i32) -> HwpUnit {
        if self.widths.is_empty() {
            return self.body_left + HwpUnit::from(column) * (self.column_width(0) + self.gap);
        }
        self.body_left
            + (0..column)
                .map(|index| self.column_width(index as usize) + self.gap)
                .sum::<HwpUnit>()
    }
}

pub fn probe(document: &Document) -> Vec<ProbeLine> {
    let mut lines = Vec::new();
    let wchar = document.wchar_positions;
    let mut page = 0i32;
    for (sec_index, section) in document.sections.iter().enumerate() {
        page += 1;
        let mut geometry = SectionGeometry::new(section);
        let mut last_vert = -1.0;
        let mut column = 0i32;
        for (para_index, paragraph) in section.paragraphs.iter().enumerate() {
            if let Some(columns) = &paragraph.columns {
                if geometry.columns != columns.count {
                    column = 0;
                }
                geometry.set_columns(columns);
            }
            let stream = atoms(paragraph, wchar);
            let ranges = line_ranges(&stream, &paragraph.line_segs);
            for (line_index, (segment, range)) in
                paragraph.line_segs.iter().zip(ranges.iter()).enumerate()
            {
                let forced_page = line_index == 0 && paragraph.page_break && last_vert >= 0.0;
                let forced_column = line_index == 0 && paragraph.column_break && last_vert >= 0.0;
                let back_up = segment.vertpos < last_vert - 0.5;
                if back_up || forced_page || forced_column {
                    if !forced_page
                        && !segment.is_page_first_line()
                        && column + 1 < geometry.columns
                    {
                        column += 1;
                    } else {
                        page += 1;
                        column = 0;
                    }
                }
                last_vert = segment.vertpos;
                let line_top = segment.vertpos;
                let y_pt = (geometry.body_top + line_top + segment.baseline) / 100.0;
                let x_pt = (geometry.column_left(column) + segment.horzpos) / 100.0;
                let text = line_text(&stream, range);
                lines.push(ProbeLine {
                    sec: sec_index,
                    page,
                    col: column,
                    para: para_index,
                    line: line_index,
                    y_pt,
                    x_pt,
                    text,
                    head: if line_index == 0 {
                        paragraph.head_text.clone().unwrap_or_default()
                    } else {
                        String::new()
                    },
                });
                if let Some(overflow) = table_overflow(&stream, range, segment.vertpos, &geometry) {
                    page += overflow;
                    column = 0;
                    last_vert = -1.0;
                }
            }
        }
    }
    lines
}

impl LineSeg {
    fn is_page_first_line(&self) -> bool {
        self.flags & 1 != 0
    }
}

fn line_ranges(stream: &[Atom], segments: &[LineSeg]) -> Vec<std::ops::Range<usize>> {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let start = usize::try_from(segment.textpos)
                .unwrap_or(0)
                .min(stream.len());
            let end = segments
                .get(index + 1)
                .and_then(|next| usize::try_from(next.textpos).ok())
                .unwrap_or(stream.len())
                .min(stream.len());
            start..end.max(start)
        })
        .collect()
}

fn line_text(stream: &[Atom], range: &std::ops::Range<usize>) -> String {
    stream[range.clone()]
        .iter()
        .filter(|atom| !atom.filler && !matches!(atom.ch, '\u{FFFC}' | '\u{200B}'))
        .map(|atom| atom.ch)
        .collect()
}

fn table_overflow(
    stream: &[Atom],
    range: &std::ops::Range<usize>,
    vertpos: HwpUnit,
    geometry: &SectionGeometry,
) -> Option<i32> {
    if geometry.body_height <= 0.0 {
        return None;
    }
    let mut overflow = 0;
    for atom in &stream[range.clone()] {
        let Some(table) = atom.table else {
            continue;
        };
        if !table.treat_as_char {
            continue;
        }
        let total = vertpos + table.height;
        if total > geometry.body_height {
            overflow = overflow.max((total / geometry.body_height).floor() as i32);
        }
    }
    (overflow > 0).then_some(overflow)
}

fn atoms(paragraph: &Paragraph, wchar: bool) -> Vec<Atom> {
    let mut stream = Vec::new();
    for run in &paragraph.runs {
        for item in &run.items {
            let start = stream.len();
            match item {
                RunItem::Text(value) => {
                    for ch in value.chars() {
                        stream.push(Atom::text(ch));
                        if wchar {
                            for _ in 1..ch.len_utf16() {
                                stream.push(Atom::filler());
                            }
                        }
                    }
                }
                RunItem::Skip(count) => {
                    if wchar {
                        let count = usize::try_from(*count).unwrap_or(0);
                        for _ in 0..count {
                            stream.push(Atom::filler());
                        }
                    }
                }
                RunItem::Tab | RunItem::SizedTab(_) | RunItem::FormattedTab(_) => {
                    stream.push(Atom::object(tab_char(item), None));
                    if wchar {
                        pad(&mut stream, start, 8);
                    }
                }
                RunItem::LineBreak => stream.push(Atom::text('\n')),
                RunItem::Table(table) => {
                    stream.push(Atom::object(
                        '\u{FFFC}',
                        Some(TablePlacement {
                            treat_as_char: table.pos.treat_as_char,
                            height: table.height,
                        }),
                    ));
                    if wchar {
                        pad(&mut stream, start, 8);
                    }
                }
                RunItem::Picture(_) | RunItem::Shape(_) => {
                    stream.push(Atom::object('\u{FFFC}', None));
                    if wchar {
                        pad(&mut stream, start, 8);
                    }
                }
                RunItem::AutoNum(_)
                | RunItem::Control(_)
                | RunItem::Compose(_)
                | RunItem::Note(_)
                | RunItem::Header { .. }
                | RunItem::Footer { .. }
                | RunItem::PageHiding { .. }
                | RunItem::NewNum(_)
                | RunItem::PageNum { .. } => {
                    stream.push(Atom::object('\u{200B}', None));
                    pad(&mut stream, start, if wchar { 8 } else { 1 });
                }
            }
        }
    }
    stream
}

fn tab_char(item: &RunItem) -> char {
    match item {
        RunItem::SizedTab(_) => '\u{FFFC}',
        RunItem::FormattedTab(tab) => match tab.width {
            Some(width) if width.is_finite() && width > 0.0 => '\u{FFFC}',
            _ => '\t',
        },
        _ => '\t',
    }
}

fn pad(stream: &mut Vec<Atom>, start: usize, width: usize) {
    while stream.len() - start < width {
        stream.push(Atom::filler());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Page, Paragraph, Run, Section, Table};

    fn paragraph(segments: Vec<LineSeg>) -> Paragraph {
        Paragraph {
            runs: vec![Run {
                items: vec![RunItem::Text("가나다라".to_string())],
                ..Run::default()
            }],
            line_segs: segments,
            ..Paragraph::default()
        }
    }

    fn segment(vertpos: f64, textpos: i32, flags: u32) -> LineSeg {
        LineSeg {
            textpos,
            vertpos,
            vertsize: 1000.0,
            baseline: 850.0,
            horzpos: 0.0,
            horzsize: 40000.0,
            flags,
            ..LineSeg::default()
        }
    }

    fn section(paragraphs: Vec<Paragraph>, columns: i32, height: f64) -> Section {
        Section {
            page: Page {
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
            },
            paragraphs,
            columns,
            column_gap: 1134.0,
            ..Section::default()
        }
        .with_body_height(height)
    }

    trait BodyHeight {
        fn with_body_height(self, height: f64) -> Self;
    }

    impl BodyHeight for Section {
        fn with_body_height(mut self, height: f64) -> Self {
            self.page.height = height
                + self.page.margin_top
                + self.page.header
                + self.page.margin_bottom
                + self.page.footer;
            self
        }
    }

    #[test]
    fn probe_page_break_rules() {
        let flowing = paragraph(vec![
            segment(1000.0, 0, 0),
            segment(500.0, 2, 0),
            segment(100.0, 4, 0),
            segment(1000.0, 6, 0),
            segment(500.0, 8, 1),
        ]);
        let forced = Paragraph {
            page_break: true,
            ..paragraph(vec![segment(100.0, 0, 0)])
        };
        let doc = Document {
            sections: vec![section(vec![flowing, forced], 2, 5000.0)],
            wchar_positions: true,
            ..Document::default()
        };
        let lines = probe(&doc);
        let pages: Vec<(i32, i32)> = lines.iter().map(|line| (line.page, line.col)).collect();
        assert_eq!(pages, [(1, 0), (1, 1), (2, 0), (2, 0), (3, 0), (4, 0)]);
    }

    #[test]
    fn probe_text_positions_wchar() {
        let paragraph = Paragraph {
            runs: vec![Run {
                items: vec![
                    RunItem::Text("가".to_string()),
                    RunItem::Skip(2),
                    RunItem::Text("나".to_string()),
                    RunItem::Tab,
                    RunItem::Text("다".to_string()),
                ],
                ..Run::default()
            }],
            line_segs: vec![segment(0.0, 0, 0), segment(1000.0, 4, 0)],
            ..Paragraph::default()
        };
        let doc = Document {
            sections: vec![section(vec![paragraph], 1, 5000.0)],
            wchar_positions: true,
            ..Document::default()
        };
        let lines = probe(&doc);
        assert_eq!(lines[0].text, "가나");
        assert_eq!(lines[1].text, "\t다");
    }

    #[test]
    fn probe_table_overflow_adds_pages() {
        let mut para = paragraph(vec![segment(0.0, 0, 0)]);
        para.runs[0].items.insert(
            0,
            RunItem::Table(Table {
                height: 12000.0,
                pos: crate::model::Pos {
                    treat_as_char: true,
                    ..crate::model::Pos::default()
                },
                ..Table::default()
            }),
        );
        para.line_segs = vec![segment(0.0, 0, 0), segment(1000.0, 9, 0)];
        let doc = Document {
            sections: vec![section(vec![para], 1, 5000.0)],
            wchar_positions: true,
            ..Document::default()
        };
        let lines = probe(&doc);
        assert_eq!(lines[0].page, 1);
        assert_eq!(lines[1].page, 3);
    }
}
