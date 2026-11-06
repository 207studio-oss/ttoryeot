//! 바탕쪽: 쪽마다 본문 뒤에 깔린다. 홀수·짝수 바탕쪽이 있으면 양쪽 바탕쪽보다 앞선다.

use hwp_core::{
    Document, LineSeg, Margins, MasterPage, Paragraph, Pos, Run, RunItem, Section, Shape,
};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::Item;

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

fn line() -> Vec<LineSeg> {
    vec![LineSeg {
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        ..LineSeg::default()
    }]
}

fn text(text: &str) -> Vec<RunItem> {
    vec![RunItem::Text(text.to_string())]
}

fn master(applies: &str, items: Vec<RunItem>) -> MasterPage {
    MasterPage {
        applies: applies.to_string(),
        paragraphs: vec![paragraph(items, line())],
    }
}

fn render(master_pages: Vec<MasterPage>, pages: usize) -> Vec<Vec<Item>> {
    let mut paragraphs = vec![paragraph(text("x"), line())];
    for _ in 1..pages {
        paragraphs.push(Paragraph {
            page_break: true,
            ..paragraph(text("x"), line())
        });
    }
    let doc = Document {
        sections: vec![Section {
            paragraphs,
            master_pages,
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

fn drawn_text(items: &[Item]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn master_page_is_drawn_behind_every_page() {
    let pages = render(vec![master("BOTH", text("M"))], 2);
    let texts: Vec<String> = pages.iter().map(|items| drawn_text(items)).collect();
    assert_eq!(texts, ["Mx", "Mx"]);
}

#[test]
fn odd_and_even_master_pages_override_both() {
    let pages = render(vec![master("BOTH", text("B")), master("ODD", text("O"))], 3);
    let texts: Vec<String> = pages.iter().map(|items| drawn_text(items)).collect();
    assert_eq!(texts, ["Ox", "Bx", "Ox"]);
}

/// 줄 정보 없이 저장된 글상자 글도 흘려 본 줄 높이로 가운데에 맞춘다.
#[test]
fn centered_text_box_without_line_segments_uses_flowed_height() {
    let shape = Shape {
        kind: "rect".to_string(),
        width: 10000.0,
        height: 4000.0,
        pos: Pos {
            treat_as_char: false,
            vert_rel_to: "PAPER".to_string(),
            horz_rel_to: "PAPER".to_string(),
            vert_offset: 1000.0,
            horz_offset: 1000.0,
            text_wrap: "IN_FRONT_OF_TEXT".to_string(),
            ..Pos::default()
        },
        text_margins: Margins::default(),
        text_vert_align: "CENTER".to_string(),
        paragraphs: vec![paragraph(text("T"), Vec::new())],
        ..Shape::default()
    };
    let pages = render(
        vec![MasterPage {
            applies: "BOTH".to_string(),
            paragraphs: vec![paragraph(vec![RunItem::Shape(shape)], line())],
        }],
        1,
    );
    let baseline = pages[0]
        .iter()
        .find_map(|item| match item {
            Item::Glyphs(run) if run.text == "T" => Some(run.origin.1),
            _ => None,
        })
        .expect("글상자 글");
    // 상자 위 10pt, 높이 40pt, 줄 10pt: 줄 위가 25pt이고 기준선은 그 아래 8.5pt 안팎이다.
    assert!((31.0..36.0).contains(&baseline), "baseline {baseline}");
}
