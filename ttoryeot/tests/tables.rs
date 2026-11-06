//! 표 격자: 칸 폭에서 열 경계를 푼다. 표 테두리: 한/글처럼 셀 간격이 있을 때만 바깥 틀을 그린다.

use hwp_core::{
    Border, BorderFill, Caption, Cell, CharPr, Document, FontFace, Paragraph, Run, RunItem,
    Section, Table,
};
use std::collections::BTreeMap;
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, PathOp};

fn text(text: &str) -> Paragraph {
    Paragraph {
        runs: vec![Run {
            char_pr_id: 0,
            items: vec![RunItem::Text(text.to_string())],
            supscript: false,
        }],
        ..Paragraph::default()
    }
}

/// `item` 하나만 담은 문서의 첫 쪽 그림 요소.
fn first_page(item: RunItem, border_fills: Vec<BorderFill>) -> Vec<Item> {
    let doc = Document {
        font_faces: BTreeMap::from([("latin".to_string(), vec![FontFace::new("Liberation Sans")])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("latin".to_string(), 0)]),
            ..CharPr::default()
        }],
        border_fills,
        sections: vec![Section {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    char_pr_id: 0,
                    items: vec![item],
                    supscript: false,
                }],
                ..Paragraph::default()
            }],
            ..Section::default()
        }],
        ..Document::default()
    };
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    );
    let mut book = FontBook::scan(&[]);
    book.add_bytes(font.into(), std::fs::read(font).unwrap());
    ttoryeot::render(&doc, &mut book).swap_remove(0).items
}

/// 첫 쪽 글자들의 (글자, 쪽 위 x).
fn placed_chars(item: RunItem) -> Vec<(char, f32)> {
    first_page(item, Vec::new())
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some((run.text.chars().next()?, run.origin.0)),
            _ => None,
        })
        .collect()
}

/// 첫 쪽의 곧은 선들: (시작, 끝).
fn lines(item: RunItem, border_fills: Vec<BorderFill>) -> Vec<((f32, f32), (f32, f32))> {
    first_page(item, border_fills)
        .iter()
        .filter_map(|item| match item {
            Item::Stroke { path, .. } => match path.as_slice() {
                [PathOp::Move(x0, y0), PathOp::Line(x1, y1)] => Some(((*x0, *y0), (*x1, *y1))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn columns_without_a_single_cell_are_solved_from_spanning_cells() {
    // 열 폭 [1000, 1000, 3000, 2000]. 홑칸은 첫 열과 끝 열에만 있다.
    let cell = |row: i32, col: i32, col_span: i32, width: f64, letter: &str| Cell {
        row,
        col,
        col_span,
        width,
        height: 1000.0,
        paragraphs: vec![text(letter)],
        ..Cell::default()
    };
    let table = Table {
        rows: 2,
        cols: 4,
        width: 7000.0,
        height: 2000.0,
        cells: vec![
            cell(0, 0, 1, 1000.0, "A"),
            cell(0, 1, 2, 4000.0, "B"),
            cell(0, 3, 1, 2000.0, "C"),
            cell(1, 0, 2, 2000.0, "D"),
            cell(1, 2, 2, 5000.0, "E"),
        ],
        ..Table::default()
    };
    let chars = placed_chars(RunItem::Table(table));
    let x = |letter: char| chars.iter().find(|(ch, _)| *ch == letter).unwrap().1;
    // 셋째 열은 둘째 줄 첫 칸(두 열, 2000) 끝에서 시작한다.
    assert!((x('E') - x('A') - 20.0).abs() < 0.01, "chars {chars:?}");
    assert!((x('C') - x('A') - 50.0).abs() < 0.01, "chars {chars:?}");
}

#[test]
fn right_border_reaches_the_table_width_when_a_row_is_short() {
    let solid = Border {
        border_type: "SOLID".to_string(),
        width: 0.12,
        color: 0,
    };
    let fills = vec![BorderFill {
        id: 1,
        left: solid.clone(),
        right: solid,
        ..BorderFill::default()
    }];
    let cell = |row: i32, col: i32, col_span: i32, width: f64| Cell {
        row,
        col,
        col_span,
        width,
        height: 1000.0,
        border_fill_id: 1,
        paragraphs: vec![text("A")],
        ..Cell::default()
    };
    let table = Table {
        rows: 2,
        cols: 2,
        width: 10000.0,
        height: 2000.0,
        cells: vec![
            cell(0, 0, 2, 10000.0),
            cell(1, 0, 1, 4000.0),
            cell(1, 1, 1, 5901.0),
        ],
        ..Table::default()
    };
    let lines = lines(RunItem::Table(table), fills);
    let xs: Vec<f32> = lines
        .iter()
        .filter(|((x0, _), (x1, _))| x0 == x1)
        .map(|((x, _), _)| *x)
        .collect();
    let (left, right) = (
        xs.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
    );
    assert!((right - left - 100.0).abs() < 0.2, "lines {lines:?}");
    assert_eq!(
        xs.iter().filter(|x| **x == right).count(),
        2,
        "lines {lines:?}"
    );
}

/// 표 테두리는 네 변 모두 실선, 셀 테두리는 위 변만 실선인 한 칸 표.
fn framed_table(cell_spacing: f64) -> (RunItem, Vec<BorderFill>) {
    let solid = |width: f64| Border {
        border_type: "SOLID".to_string(),
        width,
        color: 0,
    };
    let fills = vec![
        BorderFill {
            id: 1,
            left: solid(0.12),
            right: solid(0.12),
            top: solid(0.12),
            bottom: solid(0.12),
            ..BorderFill::default()
        },
        BorderFill {
            id: 2,
            top: solid(0.4),
            ..BorderFill::default()
        },
    ];
    let table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 2000.0,
        cell_spacing,
        border_fill_id: 1,
        cells: vec![Cell {
            width: 10000.0,
            height: 2000.0,
            border_fill_id: 2,
            paragraphs: vec![text("A")],
            ..Cell::default()
        }],
        ..Table::default()
    };
    (RunItem::Table(table), fills)
}

#[test]
fn table_border_is_not_drawn_around_touching_cells() {
    let (table, fills) = framed_table(0.0);
    let lines = lines(table, fills);
    assert_eq!(lines.len(), 1, "lines {lines:?}");
    let ((x0, y0), (x1, y1)) = lines[0];
    assert!(y0 == y1 && (x1 - x0 - 100.0).abs() < 0.5, "lines {lines:?}");
}

#[test]
fn table_border_frames_spaced_cells() {
    let (table, fills) = framed_table(200.0);
    let lines = lines(table, fills);
    let vertical = lines.iter().filter(|((x0, _), (x1, _))| x0 == x1).count();
    let horizontal = lines.iter().filter(|((_, y0), (_, y1))| y0 == y1).count();
    assert_eq!((vertical, horizontal), (2, 3), "lines {lines:?}");
}

#[test]
fn horizontal_borders_reach_half_the_side_border_width() {
    // 왼쪽은 그리지 않는 1.5mm, 오른쪽은 그리는 1mm. 위는 홑선, 아래는 겹선.
    let border = |border_type: &str, width: f64| Border {
        border_type: border_type.to_string(),
        width,
        color: 0,
    };
    let fills = vec![BorderFill {
        id: 1,
        left: border("NONE", 1.5),
        right: border("SOLID", 1.0),
        top: border("SOLID", 0.12),
        bottom: border("DOUBLE_SLIM", 0.5),
        ..BorderFill::default()
    }];
    let table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 2000.0,
        cells: vec![Cell {
            width: 10000.0,
            height: 2000.0,
            border_fill_id: 1,
            paragraphs: vec![text("A")],
            ..Cell::default()
        }],
        ..Table::default()
    };
    let lines = lines(RunItem::Table(table), fills);
    let mut horizontal: Vec<_> = lines.iter().filter(|((_, y0), (_, y1))| y0 == y1).collect();
    horizontal.sort_by(|a, b| a.0.1.total_cmp(&b.0.1));
    assert_eq!(horizontal.len(), 3, "lines {lines:?}");
    let right_x = lines
        .iter()
        .find(|((x0, _), (x1, _))| x0 == x1)
        .unwrap()
        .0
        .0;
    let left_x = right_x - 100.0;
    let mm = 72.0 / 25.4;
    // 홑선은 양쪽 모두 세로 변 굵기의 반만큼 바깥으로 나간다.
    let ((top_from, _), (top_to, _)) = *horizontal[0];
    assert!(
        (top_from - (left_x - 0.75 * mm)).abs() < 0.05,
        "top {:?}",
        horizontal[0]
    );
    assert!(
        (top_to - (right_x + 0.5 * mm)).abs() < 0.05,
        "top {:?}",
        horizontal[0]
    );
    // 겹선은 그리지 않는 왼쪽 변에서는 나가고, 그린 오른쪽 변에서는 그 안에서 멈춘다.
    for &&((from, _), (to, _)) in &horizontal[1..] {
        assert!(
            (from - (left_x - 0.75 * mm)).abs() < 0.05,
            "bottom {horizontal:?}"
        );
        assert!(
            (to - (right_x - 0.5 * mm)).abs() < 0.05,
            "bottom {horizontal:?}"
        );
    }
}

/// 한 줄짜리 문단. 한/글은 쪽에서 나뉜 칸의 다음 조각 줄을 0에서 다시 세므로,
/// `vertpos`가 앞줄보다 뒤로 가지 않는 줄이 나오면 새 조각이 된다.
fn line(vertpos: f64) -> hwp_core::LineSeg {
    hwp_core::LineSeg {
        textpos: 0,
        vertpos,
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        spacing: 0.0,
        horzpos: 0.0,
        horzsize: 10000.0,
        flags: 0,
    }
}

fn cell_at(row: i32, height: f64, letters: &[&str]) -> Cell {
    Cell {
        row,
        col: 0,
        width: 10000.0,
        height,
        vert_align: "CENTER".to_string(),
        paragraphs: letters
            .iter()
            .map(|letter| Paragraph {
                line_segs: vec![line(0.0)],
                runs: vec![Run {
                    char_pr_id: 0,
                    items: vec![RunItem::Text((*letter).to_string())],
                    supscript: false,
                }],
                ..Paragraph::default()
            })
            .collect(),
        ..Cell::default()
    }
}

/// `item` 하나만 담은 문서의 쪽마다의 그림 요소.
fn pages(item: RunItem) -> Vec<Vec<Item>> {
    pages_of(vec![Paragraph {
        runs: vec![Run {
            char_pr_id: 0,
            items: vec![item],
            supscript: false,
        }],
        ..Paragraph::default()
    }])
}

/// 문단들을 담은 문서의 쪽마다의 그림 요소.
fn pages_of(paragraphs: Vec<Paragraph>) -> Vec<Vec<Item>> {
    pages_with(paragraphs, Vec::new())
}

/// 문단 모양까지 받아 그린다.
fn pages_with(paragraphs: Vec<Paragraph>, para_prs: Vec<hwp_core::ParaPr>) -> Vec<Vec<Item>> {
    let doc = Document {
        para_prs,
        font_faces: BTreeMap::from([("latin".to_string(), vec![FontFace::new("Liberation Sans")])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("latin".to_string(), 0)]),
            ..CharPr::default()
        }],
        sections: vec![Section {
            paragraphs,
            ..Section::default()
        }],
        ..Document::default()
    };
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    );
    let mut book = FontBook::scan(&[]);
    book.add_bytes(font.into(), std::fs::read(font).unwrap());
    ttoryeot::render(&doc, &mut book)
        .into_iter()
        .map(|page| page.items)
        .collect()
}

/// 쪽에서 글자 하나의 기준선 y.
fn baseline(items: &[Item], letter: char) -> Option<f32> {
    items.iter().find_map(|item| match item {
        Item::Glyphs(run) if run.text.starts_with(letter) => Some(run.origin.1),
        _ => None,
    })
}

/// 쪽 아래 끝에서 잘린 칸은 다음 쪽에서 남은 줄만, 남은 높이만큼 그린다.
#[test]
fn cell_cut_at_page_bottom_continues_with_remaining_lines() {
    let table = Table {
        rows: 2,
        cols: 1,
        width: 10000.0,
        height: 15000.0,
        page_break_mode: "CELL".to_string(),
        cells: vec![cell_at(0, 10000.0, &["A"]), cell_at(1, 1000.0, &["B", "C"])],
        ..Table::default()
    };
    let pages = pages(RunItem::Table(table));
    assert_eq!(pages.len(), 2);
    // 본문 위 99.2 + 첫 칸 높이 100pt + 남은 높이(50pt) 가운데 + 글자 높이 85%.
    let first = baseline(&pages[0], 'B').expect("B on page 1");
    assert!((first - 227.7).abs() < 0.01, "B baseline {first}");
    // 다음 쪽은 본문 위에서 남은 줄(10pt)만 그린다.
    let second = baseline(&pages[1], 'C').expect("C on page 2");
    assert!((second - 107.7).abs() < 0.01, "C baseline {second}");
    assert!(baseline(&pages[0], 'C').is_none(), "C drawn twice");
    assert!(baseline(&pages[1], 'B').is_none(), "B drawn twice");
}

#[test]
fn table_taller_than_the_body_breaks_between_rows_without_saved_page_fragments() {
    let mut cells = vec![
        cell_at(0, 30000.0, &["A"]),
        cell_at(1, 30000.0, &["B"]),
        cell_at(2, 30000.0, &["C"]),
    ];
    for cell in &mut cells {
        for para in &mut cell.paragraphs {
            para.line_segs.clear();
        }
    }
    let table = Table {
        rows: 3,
        cols: 1,
        width: 10000.0,
        height: 90000.0,
        cells,
        ..Table::default()
    };
    let pages = pages(RunItem::Table(table));
    assert_eq!(pages.len(), 2);
    assert!(baseline(&pages[0], 'A').is_some());
    assert!(baseline(&pages[0], 'B').is_some());
    assert!(baseline(&pages[1], 'C').is_some());
    assert!(baseline(&pages[0], 'C').is_none());
}

/// 내용은 남은 자리에 들어가지만 저장된 행 높이는 넘는 행은 쪽 아래 끝까지 늘여 그리고 다음 쪽으로 넘긴다.
#[test]
fn row_taller_than_remaining_space_is_stretched_to_the_page_bottom() {
    let table = Table {
        rows: 3,
        cols: 1,
        width: 10000.0,
        height: 12000.0,
        page_break_mode: "CELL".to_string(),
        cells: vec![
            cell_at(0, 4000.0, &["A"]),
            cell_at(1, 10000.0, &["B"]),
            cell_at(2, 2000.0, &["C"]),
        ],
        ..Table::default()
    };
    let pages = pages(RunItem::Table(table));
    assert_eq!(pages.len(), 2);
    // 본문 위 99.2 + 40pt + 늘인 80pt 가운데 + 8.5pt.
    let stretched = baseline(&pages[0], 'B').expect("B on page 1");
    assert!((stretched - 182.7).abs() < 0.01, "B baseline {stretched}");
    let next = baseline(&pages[1], 'C').expect("C on page 2");
    assert!((next - 112.7).abs() < 0.01, "C baseline {next}");
    assert!(baseline(&pages[1], 'B').is_none(), "B drawn twice");
}

/// 여러 행에 걸친 칸 가운데서 표가 나뉘면 쪽의 마지막 행을 쪽 아래 끝까지 늘여 그린다.
#[test]
fn row_above_a_split_inside_a_merged_cell_is_stretched_to_the_page_bottom() {
    let mut merged = cell_at(1, 40000.0, &["M"]);
    merged.col = 1;
    merged.row_span = 2;
    // 마지막 행의 글은 세 줄(30pt)이라 첫 조각에 남는 20pt에 들어가지 않는다.
    let mut last = cell_at(2, 30000.0, &["C", "D", "E"]);
    for (k, para) in last.paragraphs.iter_mut().enumerate() {
        para.line_segs = vec![line(1000.0 * k as f64)];
    }
    let table = Table {
        rows: 3,
        cols: 2,
        width: 20000.0,
        height: 62000.0,
        page_break_mode: "CELL".to_string(),
        cells: vec![
            cell_at(0, 50000.0, &["A"]),
            cell_at(1, 10000.0, &["B"]),
            last,
            merged,
        ],
        ..Table::default()
    };
    let pages = pages(RunItem::Table(table));
    assert_eq!(pages.len(), 2);
    // 저장된 첫 조각 높이 620pt: 본문 위 94.2 + 500pt + 120pt로 늘인 행의 가운데 + 8.5pt.
    let b = baseline(&pages[0], 'B').expect("B on page 1");
    assert!((b - 662.7).abs() < 0.01, "B baseline {b}");
}

/// 두 병합 칸이 같은 마지막 행에서 만나면 그 행은 두 칸이 요구하는 높이 중 큰 쪽을 따른다.
#[test]
fn merged_cells_ending_on_the_same_row_take_the_taller_needed_height() {
    let merged = |row: i32, col: i32, span: i32| Cell {
        row,
        col,
        row_span: span,
        width: 10000.0,
        height: 9000.0,
        vert_align: "CENTER".to_string(),
        paragraphs: vec![Paragraph {
            line_segs: vec![line(0.0)],
            runs: vec![Run {
                char_pr_id: 0,
                items: vec![RunItem::Text("M".to_string())],
                supscript: false,
            }],
            ..Paragraph::default()
        }],
        ..Cell::default()
    };
    let column = |row: i32, letter: &str| Cell {
        row,
        col: 2,
        width: 10000.0,
        height: 2000.0,
        vert_align: "CENTER".to_string(),
        paragraphs: vec![Paragraph {
            line_segs: vec![line(0.0)],
            runs: vec![Run {
                char_pr_id: 0,
                items: vec![RunItem::Text(letter.to_string())],
                supscript: false,
            }],
            ..Paragraph::default()
        }],
        ..Cell::default()
    };
    let table = Table {
        rows: 3,
        cols: 3,
        width: 30000.0,
        height: 4000.0,
        page_break_mode: "CELL".to_string(),
        cells: vec![
            merged(0, 0, 3),
            merged(1, 1, 2),
            column(0, "A"),
            column(1, "B"),
            column(2, "C"),
        ],
        ..Table::default()
    };
    let pages = pages(RunItem::Table(table));
    assert_eq!(pages.len(), 2);
    // 남는 높이 9000−4000=5000, 9000−2000=7000 중 큰 7000이 마지막 행 높이가 된다.
    let c = baseline(&pages[1], 'C').expect("C on page 2");
    assert!((c - 137.7).abs() < 0.01, "C baseline {c}");
}

/// 제목 줄을 되풀이하는 표에서 쪽 끝까지 늘인 마지막 행 뒤에 제목만 있는 쪽을 만들지 않는다.
#[test]
fn stretched_last_row_does_not_add_a_header_only_page() {
    let mut header = cell_at(0, 3000.0, &["H"]);
    header.header = true;
    let table = Table {
        rows: 3,
        cols: 1,
        width: 10000.0,
        height: 12000.0,
        page_break_mode: "CELL".to_string(),
        repeat_header: true,
        cells: vec![
            header,
            cell_at(1, 3000.0, &["A"]),
            cell_at(2, 8000.0, &["B"]),
        ],
        ..Table::default()
    };
    let pages = pages(RunItem::Table(table));
    assert_eq!(pages.len(), 1);
    // 본문 위 99.2 + 30 + 30 + 늘인 60pt 가운데 + 8.5.
    let b = baseline(&pages[0], 'B').expect("B on page 1");
    assert!((b - 192.7).abs() < 0.01, "B baseline {b}");
    assert!(baseline(&pages[0], 'H').is_some());
}

/// 줄 정보 없는 칸 글을 `count`줄 담은 칸. 한/글은 이런 칸을 다시 배치한다.
fn reflowed_cell(row: i32, letter: &str, count: usize) -> Cell {
    let mut cell = cell_at(row, 1000.0, &vec![letter; count]);
    for para in &mut cell.paragraphs {
        para.line_segs.clear();
    }
    cell
}

/// 표 하나를 담은 문단. 저장된 줄은 표 높이만큼의 한 줄이다(한/글이 다시 배치하기 전 값).
fn table_paragraph(table: Table, vertpos: f64) -> Paragraph {
    let height = table.height;
    Paragraph {
        line_segs: vec![hwp_core::LineSeg {
            vertsize: height,
            textheight: height,
            baseline: height * 0.85,
            ..line(vertpos)
        }],
        runs: vec![Run {
            char_pr_id: 0,
            items: vec![RunItem::Table(table)],
            supscript: false,
        }],
        ..Paragraph::default()
    }
}

/// 본문과 함께 흐르는 표의 칸 글이 줄 정보 없이 저장되면, 다시 배치해 늘어난 표는 쪽에서 행 사이로 나뉜다.
/// 표 문단에 저장된 줄이 있어도 마찬가지다.
#[test]
fn floating_table_grown_by_reflowed_cells_breaks_across_pages() {
    let mut table = Table {
        rows: 3,
        cols: 1,
        width: 10000.0,
        height: 42000.0,
        cells: vec![
            cell_at(0, 40000.0, &["A"]),
            reflowed_cell(1, "B", 30),
            cell_at(2, 1000.0, &["C"]),
        ],
        ..Table::default()
    };
    table.pos.treat_as_char = false;
    table.pos.text_wrap = "TOP_AND_BOTTOM".to_string();
    let pages = pages_of(vec![table_paragraph(table, 0.0)]);
    assert_eq!(pages.len(), 2, "pages");
    assert!(baseline(&pages[0], 'A').is_some());
    assert!(
        baseline(&pages[0], 'B').is_none(),
        "grown row left on page 1"
    );
    assert!(baseline(&pages[1], 'B').is_some());
    assert!(baseline(&pages[1], 'C').is_some());
}

/// 글자처럼 취급한 표가 칸을 다시 배치해 쪽에 남은 높이를 넘으면, 나누지 않고 표째 다음 쪽 맨 위로 간다.
#[test]
fn inline_table_grown_by_reflowed_cells_moves_to_the_next_page() {
    let table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 1000.0,
        cells: vec![reflowed_cell(0, "B", 30)],
        ..Table::default()
    };
    let title = Paragraph {
        line_segs: vec![line(0.0)],
        ..text("T")
    };
    let pages = pages_of(vec![title, table_paragraph(table, 30000.0)]);
    assert_eq!(pages.len(), 2, "pages");
    assert!(baseline(&pages[0], 'T').is_some());
    assert!(baseline(&pages[0], 'B').is_none(), "table left on page 1");
    let top = baseline(&pages[1], 'B').expect("table on page 2");
    assert!(top < 130.0, "table not at page top: {top}");
}

/// 본문과 함께 흐르는 표 하나("A")와 그 뒤 글 줄("X")을 담은 문단. 문단 모양은 `para_pr_id`.
fn float_then_text(para_pr_id: i32, text_vertpos: f64) -> Paragraph {
    let mut table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 5000.0,
        cells: vec![cell_at(0, 5000.0, &["A"])],
        ..Table::default()
    };
    table.pos.treat_as_char = false;
    table.pos.vert_rel_to = "PARA".to_string();
    table.pos.flow_with_text = true;
    table.pos.text_wrap = "TOP_AND_BOTTOM".to_string();
    Paragraph {
        para_pr_id,
        line_segs: vec![hwp_core::LineSeg {
            textpos: 0,
            ..line(text_vertpos)
        }],
        runs: vec![Run {
            char_pr_id: 0,
            items: vec![RunItem::Table(table), RunItem::Text("X".to_string())],
            supscript: false,
        }],
        ..Paragraph::default()
    }
}

fn para_prs_with_prev(prev: f64) -> Vec<hwp_core::ParaPr> {
    vec![
        hwp_core::ParaPr::default(),
        hwp_core::ParaPr {
            id: 1,
            prev,
            ..hwp_core::ParaPr::default()
        },
    ]
}

/// 문단 기준 표는 문단 위 여백과 상관없이 앞 문단이 끝난 자리에 놓인다(한/글 12.30 PDF).
#[test]
fn paragraph_anchored_table_ignores_space_before() {
    let render = |prev: f64| {
        let title = Paragraph {
            line_segs: vec![line(0.0)],
            ..text("T")
        };
        let pages = pages_with(
            vec![title, float_then_text(1, 1000.0 + 5000.0 + prev)],
            para_prs_with_prev(prev),
        );
        baseline(&pages[0], 'A').expect("table on page 1")
    };
    assert_eq!(render(0.0), render(2000.0));
}

/// 글 줄만 다음 쪽에 저장된 문단은 표를 이 쪽 문단 시작 자리에 두고 글만 넘긴다.
#[test]
fn table_stays_when_only_the_text_line_is_saved_on_the_next_page() {
    let title = Paragraph {
        line_segs: vec![line(20000.0)],
        ..text("T")
    };
    let pages = pages_with(
        vec![title, float_then_text(0, 0.0)],
        para_prs_with_prev(0.0),
    );
    assert_eq!(pages.len(), 2, "pages");
    assert!(baseline(&pages[0], 'A').is_some(), "table moved to page 2");
    assert!(baseline(&pages[1], 'X').is_some(), "text left on page 1");
}

/// 마지막 행만 늘여 한 조각에 들어간 문단 기준 표는 쪽을 넘기지 않는다.
/// 다음 문단이 새 쪽에 저장돼 있으면 그 쪽 넘김을 따른다(한/글 12.30 PDF).
#[test]
fn stretched_single_piece_table_keeps_the_next_saved_page_break() {
    let mut table = Table {
        rows: 3,
        cols: 1,
        width: 10000.0,
        height: 12000.0,
        page_break_mode: "CELL".to_string(),
        cells: vec![
            cell_at(0, 3000.0, &["H"]),
            cell_at(1, 3000.0, &["A"]),
            cell_at(2, 8000.0, &["B"]),
        ],
        ..Table::default()
    };
    table.pos.treat_as_char = false;
    table.pos.vert_rel_to = "PARA".to_string();
    table.pos.flow_with_text = true;
    table.pos.text_wrap = "TOP_AND_BOTTOM".to_string();
    // 새 쪽 첫 줄이 표 위 끝(0)보다 아래에 저장돼 있어도 쪽 넘김이다.
    let next = Paragraph {
        line_segs: vec![line(1000.0)],
        ..text("N")
    };
    let pages = pages_of(vec![table_paragraph(table, 20000.0), next]);
    assert_eq!(pages.len(), 2, "pages");
    assert!(baseline(&pages[0], 'B').is_some(), "table left page 1");
    assert!(baseline(&pages[1], 'N').is_some(), "saved page break lost");
}

#[test]
fn caption_below_an_inline_table_is_drawn_under_it() {
    let table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 2000.0,
        cells: vec![Cell {
            width: 10000.0,
            height: 2000.0,
            paragraphs: vec![text("A")],
            ..Cell::default()
        }],
        caption: Some(Caption {
            side: "BOTTOM".to_string(),
            gap: 850.0,
            width: 10000.0,
            paragraphs: vec![text("Z")],
            ..Caption::default()
        }),
        ..Table::default()
    };
    let items = first_page(RunItem::Table(table), Vec::new());
    let y = |letter: char| {
        items
            .iter()
            .find_map(|item| match item {
                Item::Glyphs(run) if run.text.starts_with(letter) => Some(run.origin.1),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{letter} not drawn"))
    };
    // 캡션은 표(20pt) 아래 간격(8.5pt)을 두고 놓인다.
    assert!(y('Z') - y('A') > 28.0, "A {} Z {}", y('A'), y('Z'));
    // 셀 내용은 셀 테두리에서 잘린다.
    assert!(items.iter().any(|item| matches!(item, Item::Clip(_))));
}

#[test]
fn caption_below_a_floating_table_is_drawn_under_it() {
    let mut table = Table {
        rows: 1,
        cols: 1,
        width: 10000.0,
        height: 2000.0,
        cells: vec![Cell {
            width: 10000.0,
            height: 2000.0,
            paragraphs: vec![text("A")],
            ..Cell::default()
        }],
        caption: Some(Caption {
            side: "BOTTOM".to_string(),
            gap: 850.0,
            width: 10000.0,
            paragraphs: vec![text("Z")],
            ..Caption::default()
        }),
        ..Table::default()
    };
    table.pos.treat_as_char = false;
    table.pos.vert_rel_to = "PARA".to_string();
    table.pos.text_wrap = "SQUARE".to_string();
    let items = first_page(RunItem::Table(table), Vec::new());
    let y = |letter: char| {
        items
            .iter()
            .find_map(|item| match item {
                Item::Glyphs(run) if run.text.starts_with(letter) => Some(run.origin.1),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{letter} not drawn"))
    };
    // 떠 있는 개체도 캡션을 개체 아래 간격을 두고 그린다.
    assert!(y('Z') - y('A') > 28.0, "A {} Z {}", y('A'), y('Z'));
}
