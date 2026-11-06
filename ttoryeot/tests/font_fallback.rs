use hwp_core::{ParseOptions, parse};
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::Item;

mod common;
use common::rename_font;

fn fixture() -> hwp_core::Document {
    parse(
        include_bytes!("../../hwp-core/tests/fixtures/synthetic.hwpx"),
        &ParseOptions::default(),
    )
    .expect("synthetic HWPX")
    .document
}

fn text(document: &hwp_core::Document, fonts: &mut FontBook) -> String {
    ttoryeot::render(document, fonts)
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn supplied_font_draws_text_when_document_family_is_unavailable() {
    let mut fonts = FontBook::default();
    fonts.add_bytes(
        "test.ttf".into(),
        include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf").to_vec(),
    );
    let text = text(&fixture(), &mut fonts);
    assert!(text.contains("SECTION_0"), "rendered text: {text}");
    assert!(text.contains("ALPHA"), "rendered text: {text}");
    assert!(text.contains("BETA"), "rendered text: {text}");
}

#[test]
fn a_later_supplied_face_draws_glyphs_missing_from_the_first() {
    let original = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");
    let mut without_a = original.to_vec();
    let table_count = u16::from_be_bytes([without_a[4], without_a[5]]) as usize;
    let cmap = (0..table_count)
        .map(|index| 12 + index * 16)
        .find(|offset| &without_a[*offset..*offset + 4] == b"cmap")
        .expect("test font has a cmap table");
    let cmap_start =
        u32::from_be_bytes(without_a[cmap + 8..cmap + 12].try_into().unwrap()) as usize;
    let subtable_offset = u32::from_be_bytes(
        without_a[cmap_start + 8..cmap_start + 12]
            .try_into()
            .unwrap(),
    ) as usize;
    let subtable = cmap_start + subtable_offset;
    assert_eq!(
        u16::from_be_bytes([without_a[subtable], without_a[subtable + 1]]),
        4
    );
    let segments =
        u16::from_be_bytes([without_a[subtable + 6], without_a[subtable + 7]]) as usize / 2;
    let first_start = subtable + 14 + segments * 2 + 2;
    assert_eq!(
        u16::from_be_bytes([without_a[first_start], without_a[first_start + 1]]),
        32
    );
    without_a[first_start..first_start + 2].copy_from_slice(&66u16.to_be_bytes());

    let mut fonts = FontBook::default();
    fonts.add_bytes("no-a.ttf".into(), without_a);
    fonts.add_bytes("complete.ttf".into(), original.to_vec());
    assert_eq!(fonts.face_count(), 2);
    let first = fonts.find("Liberation Sans", false).expect("first face").0;
    let first_glyph = fonts.glyph(first, 'A').map(|glyph| glyph.glyph);
    assert!(
        first_glyph.is_none_or(|glyph| glyph == 0),
        "first glyph: {first_glyph:?}"
    );
    let text = text(&fixture(), &mut fonts);
    assert!(text.contains("ALPHA"), "rendered text: {text}");
}

fn bundled(family: &str) -> Vec<u8> {
    let original = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");
    let renamed = rename_font(original, "Liberation Sans", family);
    rename_font(&renamed, "LiberationSans", &family.replace(' ', ""))
}

/// 문단 차례대로 `ABC`를 그린 얼굴. 시험 글꼴에는 한글이 없어 라틴 글자로 문단을 가린다.
fn faces_by_paragraph(fonts: &mut FontBook) -> Vec<usize> {
    let document = parse(
        include_bytes!("../../hwp-core/tests/fixtures/synthetic-font-families.hwpx"),
        &ParseOptions::default(),
    )
    .expect("synthetic font-families HWPX")
    .document;
    ttoryeot::render(&document, fonts)
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Item::Glyphs(run) if run.text.contains("ABC") => Some(run.face),
            _ => None,
        })
        .collect()
}

#[test]
fn bundled_noto_faces_split_document_fonts_into_serif_and_gothic() {
    for order in [
        ["Noto Serif KR", "Noto Sans KR"],
        ["Noto Sans KR", "Noto Serif KR"],
    ] {
        let mut fonts = FontBook::default();
        for family in order {
            fonts.add_bytes(format!("/bundled/{family}").into(), bundled(family));
        }
        let serif = fonts.find("Noto Serif KR", false).expect("serif face").0;
        let sans = fonts.find("Noto Sans KR", false).expect("sans face").0;
        let faces = faces_by_paragraph(&mut fonts);
        let expected = [serif, serif, serif, sans, sans, sans];
        assert_eq!(faces, expected, "faces added in order {order:?}");
    }
}
