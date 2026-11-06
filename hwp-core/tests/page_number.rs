mod support;

use hwp_core::model::{Document, RunItem};
use hwp_core::{parse, ParseOptions};
use support::{cfbw, rec};

fn control(id: &[u8; 4], attr: u32, tail: &[u16]) -> Vec<u8> {
    let mut data: Vec<_> = id.iter().rev().copied().collect();
    data.extend(attr.to_le_bytes());
    data.extend(tail.iter().flat_map(|unit| unit.to_le_bytes()));
    data
}

fn fixture(controls: &[Vec<u8>], text: &[u16], shapes: &[(u32, u32)]) -> Document {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&0x0500_0500u32.to_le_bytes());
    header[36..40].copy_from_slice(&1u32.to_le_bytes());
    let mut section = rec::paragraph_header();
    let text: Vec<_> = text.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    section.extend(rec::record(67, 1, &text));
    let shapes: Vec<_> = shapes
        .iter()
        .flat_map(|(position, id)| position.to_le_bytes().into_iter().chain(id.to_le_bytes()))
        .collect();
    section.extend(rec::record(68, 1, &shapes));
    for data in controls {
        section.extend(rec::record(71, 1, data));
    }
    let bytes = cfbw::build(&[
        ("FileHeader".into(), header),
        (
            "DocInfo".into(),
            cfbw::compress(&rec::document_properties(1)),
        ),
        ("BodyText/Section0".into(), cfbw::compress(&section)),
    ]);
    let parsed = parse(&bytes, &ParseOptions::default()).expect("parse synthetic HWP5");
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    parsed.document
}

fn items(data: Vec<u8>) -> Vec<RunItem> {
    fixture(&[data], &[], &[]).sections[0].paragraphs[0]
        .runs
        .iter()
        .flat_map(|run| run.items.clone())
        .collect()
}

#[test]
fn pgnp_to_page_num() {
    let data = control(
        b"pgnp",
        5 << 8,
        &[b'*' as u16, b'[' as u16, b']' as u16, b'-' as u16],
    );
    assert_eq!(data.len(), 16);
    assert_eq!(
        items(data),
        vec![RunItem::PageNum {
            pos: "BOTTOM_CENTER".into(),
            format: "DIGIT".into(),
            side: "-".into(),
        }]
    );
    for (position, name) in [
        "NONE",
        "TOP_LEFT",
        "TOP_CENTER",
        "TOP_RIGHT",
        "BOTTOM_LEFT",
        "BOTTOM_CENTER",
        "BOTTOM_RIGHT",
        "OUTSIDE_TOP",
        "OUTSIDE_BOTTOM",
        "INSIDE_TOP",
        "INSIDE_BOTTOM",
        "BOTTOM_CENTER",
        "BOTTOM_CENTER",
        "BOTTOM_CENTER",
        "BOTTOM_CENTER",
        "BOTTOM_CENTER",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            items(control(
                b"pgnp",
                0xFFFF_F000 | ((position as u32) << 8) | 1,
                &[0; 4]
            )),
            vec![RunItem::PageNum {
                pos: name.into(),
                format: "CIRCLED_DIGIT".into(),
                side: String::new()
            }]
        );
    }
    for (code, name) in [
        (0, "DIGIT"),
        (2, "ROMAN_CAPITAL"),
        (8, "HANGUL_SYLLABLE"),
        (16, "DECAGON_CIRCLE_HANJA"),
        (0x80, "SYMBOL"),
        (0x81, "USER_CHAR"),
        (0xFF, "DIGIT"),
    ] {
        assert_eq!(
            items(control(b"pgnp", code | (10 << 8), &[1, 2, 3, 0x00B7])),
            vec![RunItem::PageNum {
                pos: "INSIDE_BOTTOM".into(),
                format: name.into(),
                side: "·".into()
            }]
        );
    }
}

#[test]
fn pghd_to_page_hiding() {
    for attr in 0..64 {
        assert_eq!(
            items(control(b"pghd", attr, &[])),
            vec![RunItem::PageHiding {
                header: attr & 1 != 0,
                footer: attr & 2 != 0,
                page_num: attr & 32 != 0
            }]
        );
    }
    let mut data = control(b"pghd", 32, &[]);
    data.truncate(6);
    assert_eq!(
        items(data),
        vec![RunItem::PageHiding {
            header: false,
            footer: false,
            page_num: true
        }]
    );
}

#[test]
fn atno_format_circled() {
    for (code, number, text) in [
        (1, 7, "(⑦)"),
        (1, 21, "(㉑)"),
        (1, 50, "(㊿)"),
        (1, 51, "(51)"),
        (2, 14, "(XIV)"),
        (7, 1, "(ⓐ)"),
        (8, 1, "(가)"),
        (0x80, 1, "()"),
        (0x81, 1, "()"),
        (0xFF, 7, "(7)"),
    ] {
        assert_eq!(
            items(control(
                b"atno",
                0xFFFF_E000 | (code << 4) | 1,
                &[number, b'*' as u16, b'(' as u16, b')' as u16]
            )),
            vec![RunItem::Text(text.into())]
        );
    }
}

#[test]
fn atno_superscript_run() {
    let text = [b'a' as u16, 18, 0, 0, 0, 0, 0, 0, 0, b'b' as u16];
    let document = fixture(
        &[control(
            b"atno",
            (1 << 12) | 1,
            &[10, 0, b'(' as u16, b')' as u16],
        )],
        &text,
        &[(0, 7)],
    );
    let runs = &document.sections[0].paragraphs[0].runs;
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].items, vec![RunItem::Text("a".into())]);
    assert_eq!(runs[1].items, vec![RunItem::Text("(10)".into())]);
    assert_eq!(
        runs[2].items,
        vec![RunItem::Skip(4), RunItem::Text("b".into())]
    );
    for (index, run) in runs.iter().enumerate() {
        assert_eq!(run.char_pr_id, 7);
        let json = serde_json::to_value(run).unwrap();
        if index == 1 {
            assert_eq!(json["supscript"], true);
        } else {
            assert_eq!(json["supscript"], false);
        }
    }
}

#[test]
fn atno_plain_and_page_preserve_run_boundaries() {
    let text = [b'a' as u16, 18, 0, 0, 0, 0, 0, 0, 0, b'b' as u16];
    for (attr, expected) in [
        (1, vec![RunItem::Text("10".into()), RunItem::Skip(6)]),
        ((1 << 12) | (1 << 4), vec![RunItem::AutoNum("PAGE".into())]),
    ] {
        let document = fixture(&[control(b"atno", attr, &[10, 0, 0, 0])], &text, &[(0, 7)]);
        let runs = &document.sections[0].paragraphs[0].runs;
        assert_eq!(runs.len(), 1);
        let mut expected_items = vec![RunItem::Text("a".into())];
        expected_items.extend(expected);
        expected_items.push(RunItem::Text("b".into()));
        assert_eq!(runs[0].items, expected_items);
        assert_eq!(serde_json::to_value(&runs[0]).unwrap()["supscript"], false);
    }
}

#[test]
fn trailing_plain_atno_keeps_last_nonempty_run() {
    let document = fixture(
        &[control(b"atno", 1, &[10, 0, 0, 0])],
        &[b'a' as u16, 13],
        &[(0, 7), (1, 9)],
    );
    let runs = &document.sections[0].paragraphs[0].runs;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].char_pr_id, 7);
    assert_eq!(
        runs[0].items,
        vec![RunItem::Text("a".into()), RunItem::Text("10".into())]
    );
}

#[test]
fn atno_trailing_superscript_and_char_shape_change() {
    let auto = control(b"atno", (1 << 12) | (1 << 4) | 1, &[7, 0, 0, 0]);
    let document = fixture(
        &[auto.clone(), control(b"atno", 1, &[8, 0, 0, 0])],
        &[b'a' as u16],
        &[(0, 7)],
    );
    let runs = &document.sections[0].paragraphs[0].runs;
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[1].items, vec![RunItem::Text("⑦".into())]);
    assert_eq!(serde_json::to_value(&runs[1]).unwrap()["supscript"], true);
    assert_eq!(runs[2].items, vec![RunItem::Text("8".into())]);
    assert_eq!(serde_json::to_value(&runs[2]).unwrap()["supscript"], false);
    let text = [18, 0, 0, 0, 0, 0, 0, 0, b'b' as u16];
    let document = fixture(&[auto], &text, &[(0, 7), (8, 9)]);
    let runs = &document.sections[0].paragraphs[0].runs;
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].char_pr_id, 7);
    assert_eq!(runs[0].items, vec![RunItem::Text("⑦".into())]);
    assert_eq!(runs[1].char_pr_id, 7);
    assert_eq!(runs[1].items, vec![RunItem::Skip(7)]);
    assert_eq!(runs[2].char_pr_id, 9);
    assert_eq!(runs[2].items, vec![RunItem::Text("b".into())]);
}
