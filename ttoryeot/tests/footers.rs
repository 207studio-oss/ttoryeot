//! 꼬리말 세로 자리: 칸에 들어가는 내용은 칸 아래에 붙고, 넘치는 내용은 칸 위에서 시작한다.

use hwp_core::{
    Border, BorderFill, Cell, Document, LineSeg, Margins, Paragraph, Pos, Run, RunItem, Section,
    Table,
};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, PathOp};

/// 위 테두리만 있는 한 칸 표(높이 1582, 바깥 여백 위아래 283)를 문단 기준으로 띄운 꼬리말 문단.
/// 내용 높이는 283 + 1582 + 283 = 2148.
fn footer_with_table() -> Paragraph {
    let table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 1582.0,
        pos: Pos {
            treat_as_char: false,
            text_wrap: "TOP_AND_BOTTOM".to_string(),
            ..Pos::default()
        },
        out_margins: Margins {
            top: 283.0,
            bottom: 283.0,
            ..Margins::default()
        },
        cells: vec![Cell {
            width: 10000.0,
            height: 1582.0,
            border_fill_id: 1,
            ..Cell::default()
        }],
        ..Table::default()
    };
    Paragraph {
        runs: vec![Run {
            char_pr_id: 0,
            items: vec![RunItem::Table(table)],
            supscript: false,
        }],
        line_segs: vec![LineSeg {
            vertsize: 1000.0,
            textheight: 1000.0,
            baseline: 850.0,
            ..LineSeg::default()
        }],
        ..Paragraph::default()
    }
}

/// 테두리는 장치 단위(1/600인치) 격자에 놓인다.
const GRID: f64 = 72.0 / 600.0;

/// 꼬리말 칸 높이가 `footer`인 쪽에서 꼬리말 표 위 테두리의 y(pt).
fn footer_table_top(footer: f64) -> f32 {
    let mut section = Section {
        paragraphs: vec![Paragraph {
            line_segs: vec![LineSeg {
                vertsize: 1000.0,
                ..LineSeg::default()
            }],
            ..Paragraph::default()
        }],
        footer: vec![footer_with_table()],
        ..Section::default()
    };
    section.page.footer = footer;
    let doc = Document {
        border_fills: vec![BorderFill {
            id: 1,
            top: Border {
                border_type: "SOLID".to_string(),
                width: 0.4,
                color: 0,
            },
            ..BorderFill::default()
        }],
        sections: vec![section],
        ..Document::default()
    };
    let pages = ttoryeot::render(&doc, &mut FontBook::scan(&[]));
    let tops: Vec<f32> = pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Stroke { path, .. } => match path.as_slice() {
                [PathOp::Move(_, y0), PathOp::Line(_, y1)] if y0 == y1 => Some(*y0),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(tops.len(), 1, "lines {tops:?}");
    tops[0]
}

#[test]
fn footer_that_fits_sits_on_the_bottom_margin() {
    let page = Section::default().page;
    // 칸 4252 안에 내용 2148이 들어간다: 내용 아래가 아래 여백 위(쪽 높이 - 아래 여백)에 닿는다.
    let expected = (page.height - page.margin_bottom - 2148.0 + 283.0) / 100.0;
    let top = footer_table_top(4252.0);
    assert!(
        (f64::from(top) - expected).abs() <= GRID / 2.0,
        "top {top} expected {expected}"
    );
}

#[test]
fn footer_taller_than_its_area_starts_at_the_area_top() {
    let page = Section::default().page;
    // 칸 1417보다 내용 2148이 크다: 칸 위에서 시작해 아래 여백으로 넘친다.
    let expected = (page.height - page.margin_bottom - 1417.0 + 283.0) / 100.0;
    let top = footer_table_top(1417.0);
    assert!(
        (f64::from(top) - expected).abs() <= GRID / 2.0,
        "top {top} expected {expected}"
    );
}
