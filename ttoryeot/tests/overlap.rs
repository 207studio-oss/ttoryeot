//! 겹침을 허용하지 않는 개체: 앞 개체 아래로 내려가도 본문에 안 들어가면 다음 쪽 본문 맨 위에 놓인다.

use hwp_core::{
    Border, BorderFill, Cell, Document, LineSeg, Margins, Paragraph, Pos, Run, RunItem, Section,
    Table,
};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, PathOp};

// 선은 쪽 원점과 원점에서의 거리를 따로 격자에 맞추므로 원래 자리에서 격자 한 칸까지 벗어난다.
const LINE_TOLERANCE: f32 = 72.0 / 600.0;

/// 문단 기준으로 띄운 한 칸 표. 칸 테두리 `fill`, 바깥 여백 위아래 283.
fn table(
    text_wrap: &str,
    allow_overlap: bool,
    height: f64,
    vert_offset: f64,
    fill: i32,
) -> RunItem {
    RunItem::Table(Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height,
        pos: Pos {
            treat_as_char: false,
            text_wrap: text_wrap.to_string(),
            allow_overlap,
            vert_offset,
            ..Pos::default()
        },
        out_margins: Margins {
            top: 283.0,
            bottom: 283.0,
            ..Margins::default()
        },
        cells: vec![Cell {
            width: 10000.0,
            height,
            border_fill_id: fill,
            ..Cell::default()
        }],
        ..Table::default()
    })
}

/// 테두리 없는 어울림 표(높이 40000) 위에 자리 차지 표(높이 30000, 위 테두리)를 겹쳐 띄운 문서.
/// 쪽 수와 둘째 표 위 테두리의 (쪽, y pt).
fn second_table(allow_overlap: bool, inline_first: bool) -> (usize, (usize, f32)) {
    let mut first = table("SQUARE", false, 40000.0, 0.0, 0);
    if let RunItem::Table(table) = &mut first {
        table.pos.treat_as_char = inline_first;
    }
    let (pages, tops) = render_pair(
        first,
        table("TOP_AND_BOTTOM", allow_overlap, 30000.0, 1000.0, 1),
    );
    assert_eq!(tops.len(), 1, "lines {tops:?}");
    (pages, tops[0])
}

fn render_pair(first: RunItem, second: RunItem) -> (usize, Vec<(usize, f32)>) {
    let seg = LineSeg {
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        ..LineSeg::default()
    };
    render_line(first, second, seg)
}

/// 두 개체를 한 줄 `seg`에 담은 문서의 쪽 수와 가로 괘선의 (쪽, y pt).
fn render_line(first: RunItem, second: RunItem, seg: LineSeg) -> (usize, Vec<(usize, f32)>) {
    let solid = Border {
        border_type: "SOLID".to_string(),
        width: 0.4,
        color: 0,
    };
    let doc = Document {
        border_fills: vec![
            BorderFill {
                id: 1,
                top: solid.clone(),
                ..BorderFill::default()
            },
            BorderFill {
                id: 2,
                top: solid.clone(),
                bottom: solid,
                ..BorderFill::default()
            },
        ],
        sections: vec![Section {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    char_pr_id: 0,
                    items: vec![first, second],
                    supscript: false,
                }],
                line_segs: vec![seg],
                ..Paragraph::default()
            }],
            ..Section::default()
        }],
        ..Document::default()
    };
    let pages = ttoryeot::render(&doc, &mut FontBook::scan(&[]));
    let tops: Vec<(usize, f32)> = pages
        .iter()
        .enumerate()
        .flat_map(|(index, page)| {
            page.items.iter().filter_map(move |item| match item {
                Item::Stroke { path, .. } => match path.as_slice() {
                    [PathOp::Move(_, y0), PathOp::Line(_, y1)] if y0 == y1 => Some((index, *y0)),
                    _ => None,
                },
                _ => None,
            })
        })
        .collect();
    (pages.len(), tops)
}

#[test]
fn overlapping_table_that_does_not_fit_below_moves_to_the_next_page_top() {
    let page = Section::default().page;
    // 첫 표 아래(본문 위 + 400pt)로 내리면 본문 아래를 넘는다: 다음 쪽 본문 맨 위 + 바깥 여백, 세로 오프셋은 버린다.
    let expected = ((page.margin_top + page.header + 283.0) / 100.0) as f32;
    let (pages, (index, top)) = second_table(false, false);
    assert_eq!((pages, index), (2, 1));
    assert!(
        (top - expected).abs() <= LINE_TOLERANCE,
        "top {top} expected {expected}"
    );
}

#[test]
fn table_allowed_to_overlap_stays_at_its_offset() {
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 1000.0 + 283.0) / 100.0) as f32;
    let (pages, (index, top)) = second_table(true, false);
    assert_eq!((pages, index), (1, 0));
    assert!(
        (top - expected).abs() <= LINE_TOLERANCE,
        "top {top} expected {expected}"
    );
}

#[test]
fn floating_table_respects_the_space_occupied_by_an_inline_table() {
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 283.0) / 100.0) as f32;
    let (pages, (index, top)) = second_table(false, true);
    assert_eq!((pages, index), (2, 1));
    assert!(
        (top - expected).abs() <= LINE_TOLERANCE,
        "top {top} expected {expected}"
    );
}

#[test]
fn floating_table_allowed_to_overlap_an_inline_table_stays_at_its_offset() {
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 1000.0 + 283.0) / 100.0) as f32;
    let (pages, (index, top)) = second_table(true, true);
    assert_eq!((pages, index), (1, 0));
    assert!(
        (top - expected).abs() <= LINE_TOLERANCE,
        "top {top} expected {expected}"
    );
}

#[test]
fn floating_table_before_an_inline_table_keeps_its_anchor() {
    let mut inline = table("SQUARE", false, 40000.0, 0.0, 0);
    if let RunItem::Table(table) = &mut inline {
        table.pos.treat_as_char = true;
    }
    let (pages, tops) = render_pair(table("TOP_AND_BOTTOM", false, 30000.0, 1000.0, 1), inline);
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 1000.0 + 283.0) / 100.0) as f32;
    assert_eq!(pages, 1);
    assert_eq!(tops.len(), 1);
    assert_eq!(tops[0].0, 0);
    assert!((tops[0].1 - expected).abs() <= LINE_TOLERANCE);
}

#[test]
fn table_moved_below_whose_stored_height_fits_stays_on_the_page_though_its_lines_run_taller() {
    let stored = 24000.0;
    let lines = 26000.0;
    let mut second = table("TOP_AND_BOTTOM", false, stored, 1000.0, 1);
    if let RunItem::Table(table) = &mut second {
        table.cells[0].paragraphs = vec![Paragraph {
            line_segs: vec![LineSeg {
                vertsize: lines,
                textheight: lines,
                baseline: lines * 0.85,
                ..LineSeg::default()
            }],
            ..Paragraph::default()
        }];
    }
    let (pages, tops) = render_pair(table("SQUARE", false, 40000.0, 0.0, 0), second);
    let page = Section::default().page;
    let expected =
        ((page.margin_top + page.header + 283.0 + 40000.0 + 283.0 + 283.0) / 100.0) as f32;
    assert_eq!(pages, 1);
    assert_eq!(tops.len(), 1, "lines {tops:?}");
    assert_eq!(tops[0].0, 0);
    assert!(
        (tops[0].1 - expected).abs() <= LINE_TOLERANCE,
        "top {} expected {expected}",
        tops[0].1
    );
}

#[test]
fn split_table_moved_below_whose_stored_height_fits_stays_on_the_page_within_that_height() {
    let stored = 24000.0;
    let rows = [12000.0, 13000.0];
    let mut second = table("TOP_AND_BOTTOM", false, stored, 1000.0, 2);
    if let RunItem::Table(table) = &mut second {
        table.rows = 2;
        table.cells = rows
            .iter()
            .zip(0..)
            .map(|(&height, row)| Cell {
                row,
                width: 10000.0,
                height,
                border_fill_id: 2,
                ..Cell::default()
            })
            .collect();
    }
    let (pages, edges) = render_pair(table("SQUARE", false, 40000.0, 0.0, 0), second);
    let page = Section::default().page;
    let top = page.margin_top + page.header + 283.0 + 40000.0 + 283.0 + 283.0;
    let expected = [top, top + rows[0], top + stored].map(|y| (y / 100.0) as f32);
    assert_eq!(pages, 1);
    assert!(
        edges.iter().all(|(index, _)| *index == 0),
        "edges {edges:?}"
    );
    let mut ys: Vec<f32> = edges.iter().map(|(_, y)| *y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert!(
        ys.len() == expected.len()
            && ys
                .iter()
                .zip(expected)
                .all(|(y, e)| (y - e).abs() <= LINE_TOLERANCE),
        "edges {ys:?} expected {expected:?}"
    );
}

#[test]
fn table_without_a_stored_height_that_does_not_fit_below_moves_to_the_next_page_top() {
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 283.0) / 100.0) as f32;
    let placed = ["NONE", "CELL"].map(|mode| {
        let mut second = table("TOP_AND_BOTTOM", false, 30000.0, 1000.0, 1);
        if let RunItem::Table(table) = &mut second {
            table.height = 0.0;
            table.page_break_mode = mode.to_string();
        }
        let (pages, tops) = render_pair(table("SQUARE", false, 40000.0, 0.0, 0), second);
        (mode, pages, tops)
    });
    for (_, pages, tops) in &placed {
        assert!(
            *pages == 2
                && tops.len() == 1
                && tops[0].0 == 1
                && (tops[0].1 - expected).abs() <= LINE_TOLERANCE,
            "expected 2 pages and one top at (1, {expected}): {placed:?}"
        );
    }
}

#[test]
fn floating_table_moved_to_the_next_page_keeps_its_continuation() {
    let mut second = table("TOP_AND_BOTTOM", false, 30000.0, 1000.0, 1);
    if let RunItem::Table(table) = &mut second {
        table.cells[0].paragraphs = (0..2)
            .map(|_| Paragraph {
                line_segs: vec![LineSeg {
                    vertsize: 1000.0,
                    textheight: 1000.0,
                    baseline: 850.0,
                    ..LineSeg::default()
                }],
                ..Paragraph::default()
            })
            .collect();
    }
    let (pages, tops) = render_pair(table("SQUARE", false, 40000.0, 0.0, 0), second);
    assert_eq!(pages, 3);
    assert_eq!(
        tops.iter().map(|(page, _)| *page).collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn floating_table_clears_the_line_spacing_under_an_inline_table() {
    // 글자처럼 놓인 표(2000 + 바깥 여백 566)가 든 줄 아래 줄 간격 780까지 비켜 간다.
    let mut inline = table("SQUARE", false, 2000.0, 0.0, 0);
    if let RunItem::Table(table) = &mut inline {
        table.pos.treat_as_char = true;
    }
    let seg = LineSeg {
        vertsize: 2566.0,
        textheight: 2566.0,
        baseline: 2181.0,
        spacing: 780.0,
        ..LineSeg::default()
    };
    let (pages, tops) = render_line(inline, table("TOP_AND_BOTTOM", false, 3000.0, 0.0, 1), seg);
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 2566.0 + 780.0 + 283.0) / 100.0) as f32;
    assert_eq!(pages, 1);
    assert_eq!(tops.len(), 1, "lines {tops:?}");
    assert!(
        (tops[0].1 - expected).abs() <= LINE_TOLERANCE,
        "top {} expected {expected}",
        tops[0].1
    );
}

#[test]
fn shorter_object_in_a_line_of_objects_only_puts_its_85_percent_on_the_baseline() {
    // 글자 없이 높이 2000·1000 개체만 있는 줄(기준선 1700): 작은 개체의 아래는 기준선 + 150.
    let inline = |height| {
        let mut item = table("SQUARE", false, height, 0.0, 1);
        if let RunItem::Table(table) = &mut item {
            table.pos.treat_as_char = true;
            table.out_margins = Margins::default();
        }
        item
    };
    let seg = LineSeg {
        vertsize: 2000.0,
        textheight: 2000.0,
        baseline: 1700.0,
        ..LineSeg::default()
    };
    let (_, tops) = render_line(inline(2000.0), inline(1000.0), seg);
    let page = Section::default().page;
    let body = page.margin_top + page.header;
    let expected = [body, body + 850.0].map(|top| (top / 100.0) as f32);
    assert_eq!(tops.len(), 2, "lines {tops:?}");
    for ((_, top), expected) in tops.iter().zip(expected) {
        assert!(
            (top - expected).abs() <= LINE_TOLERANCE,
            "top {top} expected {expected}"
        );
    }
}

#[test]
fn inline_object_shorter_than_its_line_sits_on_the_line_bottom() {
    // 줄 높이 1600(기준선 1360) 안의 높이 1000 표: 한/글은 기준선이 아니라 줄 아래에 맞춘다.
    let mut inline = table("SQUARE", false, 1000.0, 0.0, 1);
    if let RunItem::Table(table) = &mut inline {
        table.pos.treat_as_char = true;
        table.out_margins = Margins::default();
    }
    let seg = LineSeg {
        vertsize: 1600.0,
        textheight: 1600.0,
        baseline: 1360.0,
        ..LineSeg::default()
    };
    let (_, tops) = render_line(inline, RunItem::Text("가".to_string()), seg);
    let page = Section::default().page;
    let expected = ((page.margin_top + page.header + 600.0) / 100.0) as f32;
    assert_eq!(tops.len(), 1, "lines {tops:?}");
    assert!(
        (tops[0].1 - expected).abs() <= LINE_TOLERANCE,
        "top {} expected {expected}",
        tops[0].1
    );
}
