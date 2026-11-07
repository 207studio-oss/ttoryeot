//! 배치하면서 적은 구성 요소 목록: 이름, 포함 관계, 자리, 눈에 보이는 것을 그렸는지.

use hwp_core::{
    Cell, Document, Equation, LineSeg, Paragraph, ParseOptions, Picture, Pos, Run, RunItem,
    Section, Shape, Table, parse,
};
use ttoryeot::components::{Component, Kind, PageComponents};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::Rect;

fn fonts() -> FontBook {
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    );
    let mut book = FontBook::scan(&[]);
    book.add_bytes(font.into(), std::fs::read(font).unwrap());
    book
}

fn components(doc: &Document) -> Vec<PageComponents> {
    ttoryeot::render_with_components(doc, &mut fonts()).1
}

fn fixture() -> Document {
    parse(
        include_bytes!("../../hwp-core/tests/fixtures/synthetic.hwpx"),
        &ParseOptions::default(),
    )
    .expect("synthetic HWPX")
    .document
}

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

fn text(value: &str) -> Paragraph {
    paragraph(vec![RunItem::Text(value.to_string())])
}

fn document(paragraphs: Vec<Paragraph>) -> Document {
    Document {
        sections: vec![Section {
            paragraphs,
            ..Section::default()
        }],
        ..Document::default()
    }
}

fn find(page: &PageComponents, kind: Kind) -> Vec<&Component> {
    page.components
        .iter()
        .filter(|component| component.kind == kind)
        .collect()
}

fn center(component: &Component) -> (f32, f32) {
    let b = component.bounds;
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}

fn named<'a>(page: &'a PageComponents, kind: Kind, text: &str) -> &'a Component {
    page.components
        .iter()
        .find(|component| component.kind == kind && component.text == text)
        .unwrap_or_else(|| panic!("no {kind:?} with text {text:?}"))
}

fn floating_rect(text_wrap: &str) -> RunItem {
    RunItem::Shape(Shape {
        kind: "rect".to_string(),
        width: 6000.0,
        height: 3000.0,
        fill_color: Some(0x00FF_0000),
        fill_alpha: 1.0,
        pos: Pos {
            treat_as_char: false,
            vert_rel_to: "PAPER".to_string(),
            horz_rel_to: "PAPER".to_string(),
            vert_offset: 30000.0,
            horz_offset: 20000.0,
            text_wrap: text_wrap.to_string(),
            ..Pos::default()
        },
        ..Shape::default()
    })
}

#[test]
fn page_and_body_frame_every_page() {
    let doc = fixture();
    let (pages, parts) = ttoryeot::render_with_components(&doc, &mut fonts());
    assert_eq!(parts.len(), pages.len());
    let page = &parts[0];
    assert_eq!((page.width, page.height), (pages[0].width, pages[0].height));
    let root = &page.components[0];
    assert_eq!(root.kind, Kind::Page);
    assert_eq!(root.parent, None);
    assert_eq!(root.bounds, Rect::new(0.0, 0.0, page.width, page.height));
    let body = &page.components[1];
    assert_eq!((body.kind, body.parent), (Kind::Body, Some(0)));
    assert!(root.unseen() && body.unseen());
    for component in &page.components {
        if let Some(parent) = component.parent {
            assert!(parent < component.id, "parent comes first");
            assert_eq!(component.depth, page.components[parent].depth + 1);
        }
    }
}

#[test]
fn paragraph_is_named_with_its_first_words_and_found_by_point() {
    let parts = components(&fixture());
    let page = &parts[0];
    let paragraph = named(page, Kind::Paragraph, "SECTION_0");
    assert_eq!(paragraph.name, "문단");
    assert!(paragraph.ink && !paragraph.unseen());
    let (x, y) = center(paragraph);
    let hit = page.at(x, y, 0.0).expect("component under the paragraph");
    assert_eq!(hit.id, paragraph.id);
    assert_eq!(page.path(hit.id), "문단");
}

#[test]
fn table_cell_paragraph_has_the_table_path() {
    let parts = components(&fixture());
    let page = &parts[0];
    let alpha = named(page, Kind::Paragraph, "ALPHA");
    assert_eq!(page.path(alpha.id), "표 > 1행 1열 > 문단");
    let beta = named(page, Kind::Paragraph, "BETA");
    assert_eq!(page.path(beta.id), "표 > 1행 2열 > 문단");
    let (x, y) = center(beta);
    assert_eq!(page.at(x, y, 0.0).map(|c| c.id), Some(beta.id));
    let cell = &page.components[beta.parent.unwrap()];
    assert_eq!((cell.kind, cell.name.as_str()), (Kind::Cell, "1행 2열"));
    let table = &page.components[cell.parent.unwrap()];
    assert_eq!(table.kind, Kind::Table);
    assert!(!table.floating, "the fixture table sits in the text line");
}

#[test]
fn dragging_over_one_cell_lists_it_and_its_ancestors_but_not_the_neighbor() {
    let parts = components(&fixture());
    let page = &parts[0];
    let beta = named(page, Kind::Paragraph, "BETA");
    let (x, y) = center(beta);
    let listed: Vec<String> = page
        .in_rect(Rect::new(x - 1.0, y - 1.0, 2.0, 2.0))
        .iter()
        .map(|component| page.path(component.id))
        .collect();
    assert!(
        listed.contains(&"표 > 1행 2열 > 문단".to_string()),
        "{listed:?}"
    );
    assert!(listed.contains(&"표 > 1행 2열".to_string()), "{listed:?}");
    assert!(listed.contains(&"표".to_string()), "{listed:?}");
    assert!(
        !listed.iter().any(|path| path.contains("1행 1열")),
        "{listed:?}"
    );
}

#[test]
fn empty_header_and_footer_of_the_fixture_are_listed_without_ink() {
    let parts = components(&fixture());
    let page = &parts[0];
    for (kind, name) in [(Kind::Header, "머리말"), (Kind::Footer, "꼬리말")] {
        let found = find(page, kind);
        assert_eq!(found.len(), 1, "{name}");
        let component = found[0];
        assert_eq!(component.name, name);
        assert!(!component.ink && !component.hidden && component.unseen());
        let (x, y) = center(component);
        assert_eq!(page.at(x, y, 0.0).map(|c| c.kind), Some(kind));
        let dragged: Vec<_> = page
            .in_rect(component.bounds)
            .iter()
            .map(|c| c.kind)
            .collect();
        assert!(dragged.contains(&kind), "{dragged:?}");
    }
}

#[test]
fn header_with_text_has_ink_and_a_paragraph_child() {
    let mut doc = document(vec![text("BODY")]);
    doc.sections[0].header = vec![text("HEAD")];
    let parts = components(&doc);
    let page = &parts[0];
    let header = find(page, Kind::Header)[0];
    assert!(header.ink && !header.hidden && !header.unseen());
    let child = named(page, Kind::Paragraph, "HEAD");
    assert_eq!(page.path(child.id), "머리말 > 문단");
    assert_eq!(page.components[child.parent.unwrap()].kind, Kind::Header);
}

#[test]
fn hidden_header_is_listed_as_hidden_and_draws_nothing() {
    let hide = RunItem::PageHiding {
        header: true,
        footer: false,
        page_num: false,
    };
    let mut doc = document(vec![paragraph(vec![hide, RunItem::Text("BODY".into())])]);
    doc.sections[0].header = vec![text("HEAD")];
    let parts = components(&doc);
    let page = &parts[0];
    let header = find(page, Kind::Header)[0];
    assert!(header.hidden && !header.ink && header.unseen());
    assert!(
        !page.components.iter().any(|c| c.text == "HEAD"),
        "hidden header paragraphs are not laid out"
    );
    let marker = find(page, Kind::Control)
        .into_iter()
        .find(|c| c.name.starts_with("감추기"))
        .expect("hiding control marker");
    assert!(marker.name.contains("머리말"), "{}", marker.name);
    assert!(marker.unseen());
    let anchor = &page.components[marker.parent.unwrap()];
    assert_eq!(anchor.kind, Kind::Paragraph);
    assert!(marker.bounds.right() <= anchor.bounds.x);
}

#[test]
fn page_number_is_a_component_and_hiding_marks_it_hidden() {
    let number = RunItem::PageNum {
        pos: "BOTTOM_CENTER".to_string(),
        format: "DIGIT".to_string(),
        side: String::new(),
    };
    let shown = components(&document(vec![paragraph(vec![number.clone()])]));
    let number_part = find(&shown[0], Kind::PageNumber);
    assert_eq!(number_part.len(), 1);
    assert_eq!(number_part[0].text, "1");
    assert!(number_part[0].ink && !number_part[0].hidden);
    assert_eq!(shown[0].path(number_part[0].id), "쪽 번호");

    let hide = RunItem::PageHiding {
        header: false,
        footer: false,
        page_num: true,
    };
    let hidden = components(&document(vec![paragraph(vec![number, hide])]));
    let number_part = find(&hidden[0], Kind::PageNumber);
    assert_eq!(number_part.len(), 1);
    assert!(number_part[0].hidden && !number_part[0].ink);
}

#[test]
fn empty_table_cell_is_unseen_and_the_filled_cell_is_not() {
    let cell = |col: i32, paragraph: Paragraph| Cell {
        col,
        width: 10000.0,
        height: 2000.0,
        paragraphs: vec![paragraph],
        ..Cell::default()
    };
    let table = RunItem::Table(Table {
        rows: 1,
        cols: 2,
        width: 20000.0,
        height: 2000.0,
        cells: vec![cell(0, text("FULL")), cell(1, paragraph(Vec::new()))],
        ..Table::default()
    });
    let parts = components(&document(vec![paragraph(vec![table])]));
    let page = &parts[0];
    let cells = find(page, Kind::Cell);
    assert_eq!(cells.len(), 2);
    assert_eq!(
        (cells[0].name.as_str(), cells[0].unseen()),
        ("1행 1열", false)
    );
    assert_eq!(
        (cells[1].name.as_str(), cells[1].unseen()),
        ("1행 2열", true)
    );
    let (x, y) = center(cells[1]);
    let hit = page.at(x, y, 0.0).expect("component in the empty cell");
    assert_eq!(page.path(hit.id), "표 > 1행 2열 > 문단");
    assert!(hit.unseen());
    let table = find(page, Kind::Table)[0];
    assert!(!table.unseen(), "the table holds visible text");
}

#[test]
fn equation_is_named_with_its_script() {
    let equation = RunItem::Shape(Shape {
        width: 6000.0,
        height: 2000.0,
        equation: Some(Equation {
            script: "x over y".to_string(),
            size: 1000.0,
            ..Equation::default()
        }),
        ..Shape::default()
    });
    let parts = components(&document(vec![paragraph(vec![equation])]));
    let page = &parts[0];
    let found = find(page, Kind::Equation);
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].name.as_str(), found[0].text.as_str()),
        ("수식", "x over y")
    );
    assert!(found[0].ink && !found[0].floating);
    assert_eq!(page.path(found[0].id), "수식");
}

#[test]
fn picture_without_image_data_has_no_ink() {
    let picture = RunItem::Picture(Picture {
        width: 8000.0,
        height: 4000.0,
        ..Picture::default()
    });
    let parts = components(&document(vec![paragraph(vec![picture])]));
    let page = &parts[0];
    let found = find(page, Kind::Picture);
    assert_eq!(found.len(), 1);
    assert!(!found[0].ink && found[0].unseen());
    assert!(
        (found[0].bounds.width - 80.0).abs() < 0.01,
        "{:?}",
        found[0].bounds
    );
    assert!(
        (found[0].bounds.height - 40.0).abs() < 0.01,
        "{:?}",
        found[0].bounds
    );
}

#[test]
fn floating_shape_behind_the_text_is_flagged() {
    let parts = components(&document(vec![paragraph(vec![
        floating_rect("BEHIND_TEXT"),
        RunItem::Text("TEXT".into()),
    ])]));
    let page = &parts[0];
    let shapes = find(page, Kind::Shape);
    assert_eq!(shapes.len(), 1);
    let shape = shapes[0];
    assert_eq!(shape.name, "사각형");
    assert!(shape.floating && shape.behind && shape.ink && shape.unseen());
    assert!((shape.bounds.x - 200.0).abs() < 0.01, "{:?}", shape.bounds);
    assert!((shape.bounds.y - 300.0).abs() < 0.01, "{:?}", shape.bounds);
    let text = named(page, Kind::Paragraph, "TEXT");
    assert!(
        !text.unseen(),
        "the anchor paragraph keeps its own ink status"
    );
}

#[test]
fn hit_testing_prefers_the_object_in_front_of_one_behind_the_text() {
    let parts = components(&document(vec![paragraph(vec![
        floating_rect("BEHIND_TEXT"),
        floating_rect("IN_FRONT_OF_TEXT"),
    ])]));
    let page = &parts[0];
    let shapes = find(page, Kind::Shape);
    assert_eq!(shapes.len(), 2);
    let (behind, front) = (
        shapes.iter().find(|c| c.behind).unwrap(),
        shapes.iter().find(|c| !c.behind).unwrap(),
    );
    assert!(front.floating && !front.unseen());
    let (x, y) = center(behind);
    assert_eq!(page.at(x, y, 0.0).map(|c| c.id), Some(front.id));
}

#[test]
fn shape_far_off_the_page_is_hidden() {
    let off = RunItem::Shape(Shape {
        width: 6000.0,
        height: 3000.0,
        fill_color: Some(0x0000_00FF),
        fill_alpha: 1.0,
        pos: Pos {
            treat_as_char: false,
            vert_rel_to: "PAPER".to_string(),
            horz_rel_to: "PAPER".to_string(),
            vert_offset: 30000.0,
            horz_offset: 900_000.0,
            ..Pos::default()
        },
        ..Shape::default()
    });
    let parts = components(&document(vec![paragraph(vec![off])]));
    let shape = find(&parts[0], Kind::Shape)[0];
    assert!(shape.hidden && shape.unseen());
}

#[test]
fn footnote_is_a_component_with_its_paragraph() {
    let note = hwp_core::Note {
        number: 1,
        paragraphs: vec![text("NOTE")],
        ..hwp_core::Note::default()
    };
    let parts = components(&document(vec![paragraph(vec![
        RunItem::Text("BODY".into()),
        RunItem::Note(note),
    ])]));
    let page = &parts[0];
    let child = named(page, Kind::Paragraph, "NOTE");
    assert_eq!(page.path(child.id), "각주 > 문단");
    assert!(!find(page, Kind::Footnote)[0].unseen());
}

#[test]
fn master_page_sits_behind_the_page_content() {
    let mut doc = document(vec![text("BODY")]);
    doc.sections[0].master_pages = vec![hwp_core::MasterPage {
        applies: "BOTH".to_string(),
        paragraphs: vec![text("MASTER")],
    }];
    let parts = components(&doc);
    let page = &parts[0];
    let master = find(page, Kind::MasterPage)[0];
    assert!(master.behind && master.unseen());
    let child = named(page, Kind::Paragraph, "MASTER");
    assert_eq!(page.path(child.id), "바탕쪽 > 문단");
}

#[test]
fn field_control_gets_a_marker_left_of_its_paragraph() {
    let doc = document(vec![paragraph(vec![
        RunItem::Control("hp:fieldBegin".into()),
        RunItem::Text("LINK".into()),
    ])]);
    let parts = components(&doc);
    let page = &parts[0];
    let marker = find(page, Kind::Control)[0];
    assert_eq!(marker.name, "필드 시작");
    let anchor = named(page, Kind::Paragraph, "LINK");
    assert_eq!(marker.parent, Some(anchor.id));
    assert!(marker.bounds.right() <= anchor.bounds.x && marker.hidden);
    assert_eq!(page.path(marker.id), "문단 > 필드 시작");
}

#[test]
fn recording_does_not_change_what_is_drawn() {
    let doc = fixture();
    let plain = ttoryeot::render(&doc, &mut fonts());
    let (recorded, _) = ttoryeot::render_with_components(&doc, &mut fonts());
    assert_eq!(plain.len(), recorded.len());
    for (a, b) in plain.iter().zip(&recorded) {
        assert_eq!(a.items.len(), b.items.len());
        assert_eq!((a.width, a.height, a.scale), (b.width, b.height, b.scale));
    }
}
