//! 쪽 나눔: 저장된 줄 배치와 한/글이 실제로 쌓는 높이가 다를 때 쪽 수가 한/글과 같아야 한다.

use hwp_core::{
    Cell, CharPr, ColumnDef, Document, FontFace, LineSeg, Paragraph, Pos, Run, RunItem, Section,
    Shape, Table,
};
use std::collections::BTreeMap;
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, Page, PathOp};

/// 기본 쪽(A4, 위 여백 5668 + 머리말 4252)의 본문 위(pt).
const BODY_TOP: f32 = 99.2;

fn paragraph(items: Vec<RunItem>, line_segs: Vec<LineSeg>) -> Paragraph {
    Paragraph {
        runs: vec![Run {
            char_pr_id: 0,
            items,
            supscript: false,
        }],
        line_segs,
        ..Paragraph::default()
    }
}

/// 10pt 글자 한 줄의 저장된 줄 배치.
fn line(vertpos: f64, vertsize: f64, spacing: f64) -> LineSeg {
    LineSeg {
        vertpos,
        vertsize,
        textheight: vertsize.min(1000.0),
        baseline: vertsize.min(1000.0) * 0.85,
        spacing,
        horzsize: 42520.0,
        ..LineSeg::default()
    }
}

fn text(letter: &str, line_segs: Vec<LineSeg>) -> Paragraph {
    paragraph(vec![RunItem::Text(letter.to_string())], line_segs)
}

fn document(paragraphs: Vec<Paragraph>) -> Document {
    Document {
        font_faces: BTreeMap::from([("latin".to_string(), vec![FontFace::new("Liberation Sans")])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("latin".to_string(), 0)]),
            ..CharPr::default()
        }],
        sections: vec![Section {
            paragraphs,
            ..Section::default()
        }],
        wchar_positions: true,
        ..Document::default()
    }
}

fn render(doc: &Document) -> Vec<Page> {
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    );
    let mut book = FontBook::scan(&[]);
    book.add_bytes(font.into(), std::fs::read(font).unwrap());
    ttoryeot::render(doc, &mut book)
}

/// `letter`로 시작하는 글자 묶음의 (쪽 번호, 기준선 y).
fn find(pages: &[Page], letter: char) -> (usize, f32) {
    pages
        .iter()
        .enumerate()
        .find_map(|(number, page)| {
            page.items.iter().find_map(|item| match item {
                Item::Glyphs(run) if run.text.starts_with(letter) => Some((number, run.origin.1)),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("{letter} is not drawn"))
}

#[test]
fn wide_inline_table_after_a_control_stays_on_the_first_flowed_line() {
    // 줄 배치가 없는 문단: 컨트롤(8자리) 뒤에 본문(425.2pt)보다 넓은 표. 한/글은 표를 첫 줄에 둔다.
    let table = Table {
        rows: 1,
        cols: 1,
        width: 50000.0,
        height: 2000.0,
        cells: vec![Cell {
            width: 50000.0,
            height: 2000.0,
            paragraphs: vec![text("A", vec![line(0.0, 1000.0, 600.0)])],
            ..Cell::default()
        }],
        ..Table::default()
    };
    let doc = document(vec![paragraph(
        vec![RunItem::Control("secd".to_string()), RunItem::Table(table)],
        Vec::new(),
    )]);
    let (page, baseline) = find(&render(&doc), 'A');
    assert_eq!(page, 0);
    assert!(
        (baseline - (BODY_TOP + 8.5)).abs() < 0.5,
        "baseline {baseline}"
    );
}

/// 한 칸 표: 칸 속 문단 A·B·C의 첫 줄이 모두 0에 저장됐다(한/글이 아닌 도구가 쓴 문서). 칸·표 높이는 한 줄만큼이다.
fn overlapping_cell_table() -> Table {
    let lines = || vec![line(0.0, 1000.0, 600.0)];
    Table {
        rows: 1,
        cols: 1,
        width: 20000.0,
        height: 1000.0,
        page_break_mode: "NONE".to_string(),
        cells: vec![Cell {
            width: 20000.0,
            height: 1000.0,
            paragraphs: vec![text("A", lines()), text("B", lines()), text("C", lines())],
            ..Cell::default()
        }],
        ..Table::default()
    }
}

/// 본문: 높이 `spacer`인 저장된 한 줄, 그 아래 `tables`를 글자처럼 놓은 저장된 한 줄(높이는 첫 표의 저장된 높이),
/// 그 아래 `gap`만큼 띄워 저장된 "Z" 한 줄.
fn letter_after(tables: Vec<Table>, spacer: f64, gap: f64) -> Document {
    let height = tables[0].height;
    let table_top = spacer + 600.0;
    let z_top = table_top + height + 600.0 + gap;
    document(vec![
        text("S", vec![line(0.0, spacer, 600.0)]),
        paragraph(
            tables.into_iter().map(RunItem::Table).collect(),
            vec![line(table_top, height, 600.0)],
        ),
        text("Z", vec![line(z_top, 1000.0, 600.0)]),
    ])
}

/// 본문: 높이 `spacer`인 저장된 한 줄, 그 아래 겹친 칸 표 한 줄, 그 아래 `gap`만큼 띄워 저장된 "Z" 한 줄.
fn table_then_letter(spacer: f64, gap: f64) -> Document {
    letter_after(vec![overlapping_cell_table()], spacer, gap)
}

#[test]
fn cell_paragraphs_stored_on_top_of_each_other_are_stacked() {
    let pages = render(&table_then_letter(1000.0, 0.0));
    let (_, a) = find(&pages, 'A');
    let (_, b) = find(&pages, 'B');
    let (_, c) = find(&pages, 'C');
    // 한/글은 앞 문단 아래(줄 높이 10pt + 줄 간격 6pt)에 다음 문단을 놓는다.
    assert!((b - a - 16.0).abs() < 0.01, "A {a} B {b}");
    assert!((c - b - 16.0).abs() < 0.01, "B {b} C {c}");
}

#[test]
fn paragraph_after_a_table_grown_by_stacked_cells_moves_down() {
    let pages = render(&table_then_letter(1000.0, 0.0));
    let (_, a) = find(&pages, 'A');
    let (page, z) = find(&pages, 'Z');
    assert_eq!(page, 0);
    // 표는 세 줄(10 + 6 + 10 + 6 + 10pt)로 커지고, Z는 그 아래 줄 간격 6pt 뒤에 온다.
    assert!((z - a - (42.0 + 6.0)).abs() < 0.01, "A {a} Z {z}");
}

#[test]
fn column_separator_respects_a_spanning_float_and_a_grown_table() {
    // 60pt 떠 있는 개체 뒤에 다시 쌓아 42pt로 커진 표와 다음 문단을 놓는다.
    let mut doc = table_then_letter(7400.0, 0.0);
    let section = &mut doc.sections[0];
    section.columns = 2;
    section.column_gap = 2834.0;
    let first = &mut section.paragraphs[0];
    first.columns = Some(ColumnDef {
        count: 2,
        gap: 2834.0,
        line_type: "SOLID".to_string(),
        line_width: 0.2,
        ..ColumnDef::default()
    });
    first.runs[0].items = vec![RunItem::Shape(Shape {
        width: 45000.0,
        height: 6000.0,
        line_type: "NONE".to_string(),
        pos: Pos {
            treat_as_char: false,
            ..Pos::default()
        },
        ..Shape::default()
    })];

    let pages = render(&doc);
    let (page, a) = find(&pages, 'A');
    let (z_page, z) = find(&pages, 'Z');
    assert_eq!((page, z_page), (0, 0));
    assert!((z - a - 48.0).abs() < 0.01, "A {a} Z {z}");
    let (top, bottom) = pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Stroke { path, .. } => match path.as_slice() {
                [PathOp::Move(x0, y0), PathOp::Line(x1, y1)]
                    if (x0 - 297.64).abs() < 0.01 && x0 == x1 =>
                {
                    Some((*y0, *y1))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("column separator");
    assert!((top - (BODY_TOP + 80.0)).abs() < 0.01, "top {top}");
    assert!(
        (bottom - (BODY_TOP + 138.0)).abs() < 0.01,
        "bottom {bottom}"
    );
}

#[test]
fn paragraph_after_a_grown_table_stacks_from_its_real_bottom_not_its_stored_gap() {
    // 저장된 Z 자리에는 표 아래로 16pt가 더 비어 있다. 한/글은 저장된 자리를 쓰지 않고 표 아래에 쌓는다.
    let pages = render(&table_then_letter(1000.0, 1600.0));
    let (_, a) = find(&pages, 'A');
    let (page, z) = find(&pages, 'Z');
    assert_eq!(page, 0);
    assert!((z - a - (42.0 + 6.0)).abs() < 0.01, "A {a} Z {z}");
}

#[test]
fn paragraph_pushed_past_the_body_bottom_by_a_grown_table_starts_the_next_page() {
    // 본문 높이 65762. 저장된 배치로는 Z가 첫 쪽 끝(63100..64100)에 들어가지만, 표가 32pt 커지면
    // (표 아래 65700) Z는 넘친다.
    let pages = render(&table_then_letter(60900.0, 0.0));
    assert_eq!(pages.len(), 2);
    assert_eq!(find(&pages, 'C').0, 0);
    let (page, z) = find(&pages, 'Z');
    assert_eq!(page, 1);
    assert!((z - (BODY_TOP + 8.5)).abs() < 0.01, "Z {z}");
}

#[test]
fn cell_paragraphs_that_overlap_only_by_negative_line_spacing_keep_their_stored_place() {
    // 줄 간격이 음수인 문단은 저장된 자리(앞 줄 위 + 높이 + 줄 간격)가 앞 줄과 겹쳐도 그대로 둔다.
    let mut table = overlapping_cell_table();
    table.cells[0].paragraphs = vec![
        text("A", vec![line(0.0, 1000.0, -400.0)]),
        text("B", vec![line(600.0, 1000.0, -400.0)]),
    ];
    table.height = 1600.0;
    table.cells[0].height = 1600.0;
    let doc = document(vec![paragraph(
        vec![RunItem::Table(table)],
        vec![line(0.0, 1600.0, 600.0)],
    )]);
    let pages = render(&doc);
    let (_, a) = find(&pages, 'A');
    let (_, b) = find(&pages, 'B');
    assert!((b - a - 6.0).abs() < 0.01, "A {a} B {b}");
}

#[test]
fn stacked_cells_that_already_fit_the_stored_height_leave_later_paragraphs_in_place() {
    // 다시 쌓은 칸(42pt)이 저장된 높이 42.81pt 안에 든다. 표가 커지지 않았으니 Z는 저장된 자리(16pt 띄움) 그대로다.
    let mut table = overlapping_cell_table();
    table.height = 4281.0;
    table.cells[0].height = 4281.0;
    let pages = render(&letter_after(vec![table], 1000.0, 1600.0));
    let (_, a) = find(&pages, 'A');
    let (_, z) = find(&pages, 'Z');
    assert!((z - a - (42.81 + 6.0 + 16.0)).abs() < 0.01, "A {a} Z {z}");
}

#[test]
fn flowed_line_holding_a_stacked_table_is_not_grown_twice() {
    // 줄 배치가 없는 문단은 다시 쌓은 표 높이(42pt)로 줄을 잡는다. 뒤 문단을 그만큼 한 번 더 내리면 안 된다.
    let doc = document(vec![
        paragraph(vec![RunItem::Table(overlapping_cell_table())], Vec::new()),
        paragraph(vec![RunItem::Text("Z".to_string())], Vec::new()),
    ]);
    let pages = render(&doc);
    let (_, a) = find(&pages, 'A');
    let (_, z) = find(&pages, 'Z');
    assert!((z - a - (42.0 + 6.0)).abs() < 0.01, "A {a} Z {z}");
}

#[test]
fn two_stacked_tables_on_one_line_grow_it_by_the_taller_one() {
    let tables = vec![overlapping_cell_table(), overlapping_cell_table()];
    let pages = render(&letter_after(tables, 1000.0, 0.0));
    let (_, a) = find(&pages, 'A');
    let (_, z) = find(&pages, 'Z');
    assert!((z - a - (42.0 + 6.0)).abs() < 0.01, "A {a} Z {z}");
}

#[test]
fn table_line_that_no_longer_fits_after_stacking_starts_the_next_page() {
    // 표 줄은 저장된 높이(63000..64000)로는 첫 쪽에 들지만, 42pt로 커지면 본문 아래(65762)를 넘는다.
    let pages = render(&table_then_letter(62400.0, 0.0));
    assert_eq!(pages.len(), 2);
    let (page, a) = find(&pages, 'A');
    assert_eq!(page, 1);
    assert!((a - (BODY_TOP + 8.5)).abs() < 0.01, "A {a}");
    let (page, z) = find(&pages, 'Z');
    assert_eq!(page, 1);
    assert!((z - a - (42.0 + 6.0)).abs() < 0.01, "A {a} Z {z}");
}

#[test]
fn stored_page_break_after_a_moved_paragraph_does_not_leave_it_alone_on_a_page() {
    // Z는 커진 표에 밀려 둘째 쪽으로 간다. 저장된 배치에서 다음 쪽 첫 줄인 W는 한/글처럼 Z 바로 아래에 온다.
    let mut doc = table_then_letter(60900.0, 0.0);
    doc.sections[0]
        .paragraphs
        .push(text("W", vec![line(0.0, 1000.0, 600.0)]));
    let pages = render(&doc);
    assert_eq!(pages.len(), 2);
    let (page, z) = find(&pages, 'Z');
    assert_eq!(page, 1);
    let (page, w) = find(&pages, 'W');
    assert_eq!(page, 1);
    assert!((w - z - 16.0).abs() < 0.01, "Z {z} W {w}");
}
