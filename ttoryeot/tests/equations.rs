//! 수식: 스크립트를 글자 조각·가로줄·선으로 그린다.

use hwp_core::{CharPr, Document, Equation, FontFace, Paragraph, Run, RunItem, Section, Shape};
use std::collections::BTreeMap;
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, PathOp};

fn equation(script: &str) -> RunItem {
    RunItem::Shape(Shape {
        kind: "equation".to_string(),
        width: 6000.0,
        height: 1600.0,
        equation: Some(Equation {
            script: script.to_string(),
            size: 1000.0,
            color: 0,
            baseline: 85.0,
            font: String::new(),
        }),
        ..Shape::default()
    })
}

/// 수식 하나만 담은 문서의 첫 쪽 그림 요소.
fn first_page(item: RunItem) -> Vec<Item> {
    let doc = Document {
        font_faces: BTreeMap::from([("latin".to_string(), vec![FontFace::new("Liberation Sans")])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("latin".to_string(), 0)]),
            ..CharPr::default()
        }],
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

fn drawn_text(items: &[Item]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

fn filled(items: &[Item]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, Item::Fill { .. }))
        .count()
}

/// 네모 경로의 (왼쪽, 위, 오른쪽, 아래).
fn bounds(path: &[PathOp]) -> (f32, f32, f32, f32) {
    let mut area = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for op in path {
        let point = match op {
            PathOp::Move(x, y) | PathOp::Line(x, y) => Some((*x, *y)),
            _ => None,
        };
        if let Some((x, y)) = point {
            area.0 = area.0.min(x);
            area.1 = area.1.min(y);
            area.2 = area.2.max(x);
            area.3 = area.3.max(y);
        }
    }
    area
}

fn stroked(items: &[Item]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, Item::Stroke { .. }))
        .count()
}

#[test]
fn letters_are_drawn_inside_the_object() {
    let items = first_page(equation("x + y"));
    let text = drawn_text(&items);
    assert!(text.contains('x'), "x missing from {text:?}");
    assert!(text.contains('y'), "y missing from {text:?}");
    let run = items
        .iter()
        .find_map(|item| match item {
            Item::Glyphs(run) => Some(run),
            _ => None,
        })
        .expect("a glyph run");
    assert!(run.origin.0 >= 0.0);
    assert!(run.size > 1.0);
}

#[test]
fn fraction_gets_a_bar() {
    let items = first_page(equation("a over b"));
    let text = drawn_text(&items);
    assert!(text.contains('a') && text.contains('b'));
    assert_eq!(filled(&items), 1, "fraction bar");
    let area = items
        .iter()
        .find_map(|item| match item {
            Item::Fill { path, .. } => Some(bounds(path)),
            _ => None,
        })
        .expect("a bar");
    assert!(area.0 >= 0.0, "bar starts inside the page");
    assert!(area.2 > area.0, "bar has width");
    assert!(area.3 > area.1, "bar has thickness");
}

#[test]
fn script_sits_after_its_base_at_a_smaller_size() {
    let items = first_page(equation("x^2"));
    let sizes: Vec<f32> = items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some(run.size),
            _ => None,
        })
        .collect();
    assert_eq!(sizes.len(), 2, "base and superscript");
    assert!(sizes[0] > sizes[1]);
    let text = drawn_text(&items);
    assert!(text.contains('x') && text.contains('2'));
}

#[test]
fn vector_accent_draws_a_line() {
    let items = first_page(equation("vec a"));
    assert!(drawn_text(&items).contains('a'));
    assert_eq!(stroked(&items), 3, "shaft and two arrow heads");
}

#[test]
fn empty_script_draws_nothing() {
    let items = first_page(equation(""));
    assert!(items.is_empty());
}
