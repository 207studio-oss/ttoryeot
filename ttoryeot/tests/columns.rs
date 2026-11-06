//! 여러 단 구역의 단 구분선.

use hwp_core::{ColumnDef, Document, LineSeg, Paragraph, Pos, Run, RunItem, Section, Shape};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, PathOp, Stroke};

/// 기본 쪽(A4, 왼쪽 여백 85.04pt, 본문 위 99.2pt)에 2단, 단 사이 28.34pt. 첫 단에 두 줄, 둘째 단에 한 줄.
fn two_columns(line_type: &str) -> Document {
    let line = |vertpos: f64| LineSeg {
        vertpos,
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        horzsize: 19843.0,
        ..LineSeg::default()
    };
    let first = Paragraph {
        line_segs: vec![line(0.0), line(1600.0)],
        columns: Some(ColumnDef {
            count: 2,
            gap: 2834.0,
            line_type: line_type.to_string(),
            line_width: 0.2,
            ..ColumnDef::default()
        }),
        ..Paragraph::default()
    };
    let second = Paragraph {
        line_segs: vec![line(0.0)],
        ..Paragraph::default()
    };
    Document {
        sections: vec![Section {
            columns: 2,
            column_gap: 2834.0,
            paragraphs: vec![first, second],
            ..Section::default()
        }],
        ..Document::default()
    }
}

/// 첫 쪽에서 세로로 20pt 넘게 그은 선: (x, 위, 아래, 선).
fn vertical_lines(doc: &Document) -> Vec<(f32, f32, f32, Stroke)> {
    let pages = ttoryeot::render(doc, &mut FontBook::scan(&[]));
    pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Stroke { path, stroke } => match path.as_slice() {
                [PathOp::Move(x0, y0), PathOp::Line(x1, y1)] if x0 == x1 && y1 - y0 > 20.0 => {
                    Some((*x0, *y0, *y1, stroke.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn dotted_column_line_runs_down_the_gap_centre_as_hancom_dashes() {
    let lines = vertical_lines(&two_columns("DOT"));
    assert_eq!(lines.len(), 1);
    let (x, top, bottom, stroke) = &lines[0];
    assert!((x - 297.64).abs() < 0.01, "x {x}");
    // 첫 단의 첫 줄 위에서 둘째 줄 아래까지.
    assert!((top - 99.2).abs() < 0.01, "top {top}");
    assert!((bottom - 125.2).abs() < 0.01, "bottom {bottom}");
    assert!(
        (stroke.width - 0.567).abs() < 0.001,
        "width {}",
        stroke.width
    );
    // 0.2mm는 1/600인치 격자 5칸이고, 한/글 PDF는 켜짐 7칸·꺼짐 11칸으로 긋는다.
    let unit = 72.0 / 600.0;
    assert_eq!(stroke.dash, vec![7.0 * unit, 11.0 * unit]);
}

#[test]
fn solid_column_line_has_no_dashes_and_none_draws_nothing() {
    let lines = vertical_lines(&two_columns("SOLID"));
    assert_eq!(lines.len(), 1);
    assert!(lines[0].3.dash.is_empty());
    assert!(vertical_lines(&two_columns("NONE")).is_empty());
}

#[test]
fn separator_starts_below_a_float_spanning_both_columns() {
    let line_top = |width: f64, text: bool| {
        let mut doc = two_columns("SOLID");
        let section = &mut doc.sections[0];
        let first = &mut section.paragraphs[0];
        first.line_segs.truncate(1);
        let mut items = Vec::new();
        if text {
            items.push(RunItem::Text("A".to_string()));
        }
        items.push(RunItem::Shape(Shape {
            width,
            height: 6000.0,
            line_type: "NONE".to_string(),
            pos: Pos {
                treat_as_char: false,
                ..Pos::default()
            },
            ..Shape::default()
        }));
        first.runs = vec![Run {
            items,
            ..Run::default()
        }];
        let second = &mut section.paragraphs[1];
        second.line_segs[0].vertpos = 8000.0;
        let mut following = second.line_segs[0].clone();
        following.vertpos = 9600.0;
        second.line_segs.push(following);
        vertical_lines(&doc)[0].1
    };
    let body_top = line_top(45_000.0, false);
    assert!((body_top - 179.2).abs() < 0.01, "body top {body_top}");
    let narrow_top = line_top(10_000.0, false);
    assert!((narrow_top - 99.2).abs() < 0.01, "narrow top {narrow_top}");
    let text_top = line_top(45_000.0, true);
    assert!((text_top - 99.2).abs() < 0.01, "text top {text_top}");
}

#[test]
fn line_past_the_body_bottom_ends_the_column_line_at_its_top() {
    let mut doc = two_columns("DOT");
    let section = &mut doc.sections[0];
    let page = &section.page;
    let body = page.height - page.margin_top - page.header - page.margin_bottom - page.footer;
    section.paragraphs[0].line_segs[1].vertpos = body - 500.0;
    let lines = vertical_lines(&doc);
    assert_eq!(lines.len(), 1);
    let expected = 99.2 + ((body - 500.0) / 100.0) as f32;
    assert!(
        (lines[0].2 - expected).abs() < 0.01,
        "bottom {}",
        lines[0].2
    );
}
