//! 머리말 쪽 종류: 본문에 컨트롤이 없어 구역 머리말을 쓸 때도, 그 머리말을 정한 컨트롤이 홀수 쪽 전용이면 짝수 쪽에는 두지 않는다.

use hwp_core::{
    Border, BorderFill, Cell, Document, LineSeg, Paragraph, Run, RunItem, Section, Table,
};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, PathOp};

fn line() -> Vec<LineSeg> {
    vec![LineSeg {
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        ..LineSeg::default()
    }]
}

fn paragraph(items: Vec<RunItem>) -> Paragraph {
    Paragraph {
        runs: vec![Run {
            char_pr_id: 0,
            items,
            supscript: false,
        }],
        line_segs: line(),
        ..Paragraph::default()
    }
}

/// 칸 테두리가 `fill`이고 칸 안에 `items`를 담은 글자처럼 놓인 한 칸 표.
fn table(fill: i32, items: Vec<RunItem>) -> RunItem {
    RunItem::Table(Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 1000.0,
        cells: vec![Cell {
            width: 10000.0,
            height: 1000.0,
            border_fill_id: fill,
            paragraphs: vec![paragraph(items)],
            ..Cell::default()
        }],
        ..Table::default()
    })
}

/// 쪽마다 가로 선 수.
fn rules_per_page(applies: &str) -> Vec<usize> {
    let header = vec![paragraph(vec![table(1, Vec::new())])];
    let control = RunItem::Header {
        paragraphs: header.clone(),
        applies: applies.to_string(),
    };
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
        sections: vec![Section {
            paragraphs: vec![
                paragraph(vec![table(0, vec![control])]),
                Paragraph {
                    page_break: true,
                    ..paragraph(Vec::new())
                },
            ],
            header,
            ..Section::default()
        }],
        ..Document::default()
    };
    ttoryeot::render(&doc, &mut FontBook::scan(&[]))
        .iter()
        .map(|page| {
            page.items
                .iter()
                .filter(|item| match item {
                    Item::Stroke { path, .. } => matches!(
                        path.as_slice(),
                        [PathOp::Move(_, y0), PathOp::Line(_, y1)] if y0 == y1
                    ),
                    _ => false,
                })
                .count()
        })
        .collect()
}

#[test]
fn odd_page_header_inside_a_table_is_left_off_even_pages() {
    assert_eq!(rules_per_page("ODD"), vec![1, 0]);
}

#[test]
fn header_for_both_pages_inside_a_table_repeats() {
    assert_eq!(rules_per_page("BOTH"), vec![1, 1]);
}
