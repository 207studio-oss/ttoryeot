#[path = "support/mod.rs"]
mod support;

use hwp_core::num::{format_code, format_name, format_number, resolve_heads};
use hwp_core::*;
use support::{cfbw, rec, zipw};

fn paragraph(id: i32) -> Paragraph {
    Paragraph {
        para_pr_id: id,
        ..Paragraph::default()
    }
}

fn heading(id: i32, numbering: i32, level: i32) -> ParaPr {
    ParaPr {
        id,
        heading_type: "NUMBER".into(),
        heading_id_ref: numbering,
        heading_level: level,
        ..ParaPr::default()
    }
}

fn numbering(id: i32) -> Numbering {
    Numbering {
        id,
        start: 1,
        heads: (1..=7)
            .map(|level| NumberingHead {
                level,
                text: (1..=level)
                    .map(|n| format!("^{n}"))
                    .collect::<Vec<_>>()
                    .join("."),
                ..NumberingHead::default()
            })
            .collect(),
    }
}

fn heads(document: &Document) -> Vec<Option<&str>> {
    document
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter().map(|p| p.head_text.as_deref()))
        .collect()
}

#[test]
fn numbering_names_roundtrip() {
    for code in (0..=16).chain([0x80, 0x81]) {
        let name = format_name(code).expect("known format code");
        assert_eq!(format_code(name), Some(code));
    }
    assert_eq!(format_name(17), None);
    assert_eq!(format_name(0xFF), None);
    assert_eq!(format_code("unrecognized"), None);
}

#[test]
fn format_strings_each_kind() {
    for (format, number, expected) in [
        ("DIGIT", 123, "123"),
        ("CIRCLED_DIGIT", 1, "①"),
        ("CIRCLED_DIGIT", 20, "⑳"),
        ("CIRCLED_DIGIT", 21, "㉑"),
        ("CIRCLED_DIGIT", 35, "㉟"),
        ("CIRCLED_DIGIT", 36, "㊱"),
        ("CIRCLED_DIGIT", 50, "㊿"),
        ("ROMAN_CAPITAL", 1994, "MCMXCIV"),
        ("ROMAN_SMALL", 49, "xlix"),
        ("LATIN_CAPITAL", 26, "Z"),
        ("LATIN_SMALL", 26, "z"),
        ("CIRCLED_LATIN_CAPITAL", 1, "Ⓐ"),
        ("CIRCLED_LATIN_SMALL", 26, "ⓩ"),
        ("HANGUL_SYLLABLE", 1, "가"),
        ("HANGUL_SYLLABLE", 14, "하"),
        ("HANGUL_SYLLABLE", 15, "거"),
        ("HANGUL_SYLLABLE", 28, "허"),
        ("HANGUL_SYLLABLE", 29, "고"),
        ("HANGUL_SYLLABLE", 43, "구"),
        ("HANGUL_SYLLABLE", 57, "그"),
        ("HANGUL_SYLLABLE", 71, "기"),
        ("HANGUL_SYLLABLE", 84, "히"),
        ("CIRCLED_HANGUL_SYLLABLE", 1, "㉮"),
        ("CIRCLED_HANGUL_SYLLABLE", 14, "㉻"),
        ("HANGUL_JAMO", 1, "ㄱ"),
        ("HANGUL_JAMO", 14, "ㅎ"),
        ("CIRCLED_HANGUL_JAMO", 1, "㉠"),
        ("CIRCLED_HANGUL_JAMO", 14, "㉭"),
        ("HANGUL_PHONETIC", 1, "일"),
        ("HANGUL_PHONETIC", 1023, "천이십삼"),
        ("IDEOGRAPH", 1, "一"),
        ("IDEOGRAPH", 2, "二"),
        ("IDEOGRAPH", 3, "三"),
        ("IDEOGRAPH", 1023, "千二十三"),
        ("CIRCLED_IDEOGRAPH", 1, "㊀"),
        ("CIRCLED_IDEOGRAPH", 10, "㊉"),
        ("DECAGON_CIRCLE", 1, "갑"),
        ("DECAGON_CIRCLE", 10, "계"),
        ("DECAGON_CIRCLE_HANJA", 1, "甲"),
        ("DECAGON_CIRCLE_HANJA", 10, "癸"),
    ] {
        assert_eq!(format_number(number, format), expected, "{format} {number}");
    }
    for (format, beyond) in [
        ("CIRCLED_DIGIT", 51),
        ("ROMAN_CAPITAL", 4000),
        ("ROMAN_SMALL", 4000),
        ("LATIN_CAPITAL", 27),
        ("LATIN_SMALL", 27),
        ("CIRCLED_LATIN_CAPITAL", 27),
        ("CIRCLED_LATIN_SMALL", 27),
        ("HANGUL_SYLLABLE", 85),
        ("CIRCLED_HANGUL_SYLLABLE", 15),
        ("HANGUL_JAMO", 15),
        ("CIRCLED_HANGUL_JAMO", 15),
        ("HANGUL_PHONETIC", 10000),
        ("IDEOGRAPH", 10000),
        ("CIRCLED_IDEOGRAPH", 11),
        ("DECAGON_CIRCLE", 11),
        ("DECAGON_CIRCLE_HANJA", 11),
    ] {
        for number in [i64::MIN, -1, 0, beyond, i64::MAX] {
            assert_eq!(
                format_number(number, format),
                number.to_string(),
                "{format}"
            );
        }
    }
}

#[test]
fn multi_level_counters_reset() {
    let mut first = numbering(41);
    first.heads[1].start = 3;
    first.heads[1].format = "LATIN_SMALL".into();
    let mut document = Document {
        numberings: vec![first, numbering(97)],
        para_prs: vec![
            heading(10, 41, 0),
            heading(20, 41, 1),
            heading(30, 41, 2),
            heading(40, 97, 0),
        ],
        sections: vec![
            Section {
                paragraphs: [10, 20, 30, 20, 40, 10, 20, 30, 40]
                    .into_iter()
                    .map(paragraph)
                    .collect(),
                ..Section::default()
            },
            Section {
                paragraphs: vec![paragraph(10), paragraph(20)],
                ..Section::default()
            },
        ],
        ..Document::default()
    };
    let definitions = document.numberings.clone();
    resolve_heads(&mut document);
    let expected = [
        "1", "1.c", "1.c.1", "1.d", "1", "2", "2.c", "2.c.1", "2", "3", "3.c",
    ];
    assert_eq!(
        heads(&document),
        expected.into_iter().map(Some).collect::<Vec<_>>()
    );
    assert_eq!(document.numberings, definitions);
    resolve_heads(&mut document);
    assert_eq!(
        heads(&document),
        expected.into_iter().map(Some).collect::<Vec<_>>()
    );
}

#[test]
fn bullet_head_text() {
    let mut document = Document {
        bullets: vec![
            Bullet {
                id: 11,
                char: "\u{00AD}".into(),
                ..Bullet::default()
            },
            Bullet {
                id: 23,
                char: "◆".into(),
                ..Bullet::default()
            },
        ],
        para_prs: vec![
            ParaPr {
                heading_type: "BULLET".into(),
                ..heading(1, 11, 0)
            },
            ParaPr {
                heading_type: "BULLET".into(),
                ..heading(2, 23, 0)
            },
        ],
        sections: vec![Section {
            paragraphs: vec![paragraph(1), paragraph(2), paragraph(0)],
            ..Section::default()
        }],
        ..Document::default()
    };
    let definitions = document.bullets.clone();
    resolve_heads(&mut document);
    assert_eq!(heads(&document), vec![Some("-"), Some("◆"), None]);
    assert_eq!(document.bullets, definitions);
}

#[test]
fn outline_level_text() {
    let mut numbering = numbering(1);
    numbering.heads[0].format = "ROMAN_CAPITAL".into();
    numbering.heads[1].format = "HANGUL_SYLLABLE".into();
    numbering.heads[1].text = "제^1장 (^2) ^0 ^8 ^^".into();
    let mut document = Document {
        numberings: vec![numbering],
        para_prs: vec![
            ParaPr {
                heading_type: "OUTLINE".into(),
                ..heading(1, 1, 0)
            },
            ParaPr {
                heading_type: "OUTLINE".into(),
                ..heading(2, 1, 1)
            },
        ],
        sections: vec![Section {
            paragraphs: vec![paragraph(1), paragraph(2)],
            ..Section::default()
        }],
        ..Document::default()
    };
    resolve_heads(&mut document);
    assert_eq!(
        heads(&document),
        vec![Some("I"), Some("제I장 (가) ^0 ^8 ^^")]
    );
}

#[test]
fn hwpx_outline_uses_each_sections_numbering_definition() {
    let header = br#"<head>
        <numbering id="1"><paraHead level="1" numFormat="DIGIT" start="1">^1.</paraHead>
          <paraHead level="2" numFormat="CIRCLED_DIGIT" start="1">^2</paraHead></numbering>
        <numbering id="2"><paraHead level="1" numFormat="ROMAN_CAPITAL" start="1">^1.</paraHead>
          <paraHead level="2" numFormat="CIRCLED_DIGIT" start="1">^2</paraHead></numbering>
        <paraPr id="0"><heading type="OUTLINE" idRef="0" level="0"/></paraPr>
        <paraPr id="1"><heading type="OUTLINE" idRef="0" level="1"/></paraPr>
        <paraPr id="2"><heading type="NUMBER" idRef="1" level="0"/></paraPr>
        <paraPr id="3"><heading type="OUTLINE" idRef="1" level="0"/></paraPr>
    </head>"#;
    let section0 = br#"<sec>
        <p paraPrIDRef="0"><run><secPr outlineShapeIDRef="1"/><t>Question</t></run></p>
        <p paraPrIDRef="1"><run><t>Choice</t></run></p>
        <p paraPrIDRef="1"><run><t>Choice</t></run></p>
        <p paraPrIDRef="0"><run><t>Question</t></run></p>
        <p paraPrIDRef="1"><run><t>Choice</t></run></p>
    </sec>"#;
    let section1 = br#"<sec>
        <p paraPrIDRef="0"><run><secPr outlineShapeIDRef="2"/><t>Question</t></run></p>
        <p paraPrIDRef="1"><run><t>Choice</t></run></p>
        <p paraPrIDRef="2"><run><t>Numbered paragraph</t></run></p>
        <p paraPrIDRef="3"><run><t>Question</t></run></p>
    </sec>"#;
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "Contents/header.xml",
            data: header,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: section0,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section1.xml",
            data: section1,
            deflate: false,
        },
    ]);
    let document = parse(&bytes, &ParseOptions::default())
        .expect("synthetic HWPX")
        .document;
    assert_eq!(
        heads(&document),
        ["1.", "①", "②", "2.", "①", "I.", "①", "3.", "II."].map(Some)
    );
}

fn hwpx(format: &str, text: &str) -> Parsed {
    let header = format!(
        r#"<head><numbering id="1" start="1"><paraHead level="1" numFormat="{format}" start="1">{text}</paraHead></numbering><paraPr id="0"><heading type="NUMBER" idRef="1" level="0"/></paraPr></head>"#
    );
    let section = br#"<sec><p paraPrIDRef="0"><run><t>body</t></run></p><p paraPrIDRef="0"><run><t>next</t></run></p></sec>"#;
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "Contents/header.xml",
            data: header.as_bytes(),
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: section,
            deflate: false,
        },
    ]);
    parse(&bytes, &ParseOptions::default()).expect("parse synthetic numbering HWPX")
}

#[test]
fn symbol_has_no_counter_glyph() {
    let parsed = hwpx("SYMBOL", "★^1.");
    assert_eq!(heads(&parsed.document), vec![Some("★."), Some("★.")]);
    assert_eq!(format_number(7, "SYMBOL"), "");
    assert!(!parsed
        .warnings
        .iter()
        .any(|w| w.kind == WarningKind::UnknownNumberFormat));
}

#[test]
fn user_char_has_no_counter_glyph() {
    let parsed = hwpx("USER_CHAR", "◆^1");
    assert_eq!(heads(&parsed.document), vec![Some("◆"), Some("◆")]);
    assert_eq!(format_number(7, "USER_CHAR"), "");
    assert!(!parsed
        .warnings
        .iter()
        .any(|w| w.kind == WarningKind::UnknownNumberFormat));
}

#[test]
fn unknown_hwpx_format_warns_without_document_text() {
    let parsed = hwpx("PRIVATE_ATTRIBUTE", "prefix^1");
    assert_eq!(parsed.document.numberings[0].heads[0].format, "DIGIT");
    assert_eq!(
        heads(&parsed.document),
        vec![Some("prefix1"), Some("prefix2")]
    );
    let warning = parsed
        .warnings
        .iter()
        .find(|w| w.kind == WarningKind::UnknownNumberFormat)
        .expect("unknown format warning");
    assert!(!warning.detail.contains("PRIVATE_ATTRIBUTE"));
    assert!(!warning.detail.contains("prefix"));
}

#[test]
fn missing_definitions_and_invalid_levels_have_no_head() {
    let mut document = Document {
        numberings: vec![numbering(1)],
        para_prs: vec![heading(1, 1, -1), heading(2, 1, 7), heading(3, 9, 0)],
        sections: vec![Section {
            paragraphs: vec![paragraph(1), paragraph(2), paragraph(3), paragraph(99)],
            ..Section::default()
        }],
        ..Document::default()
    };
    resolve_heads(&mut document);
    assert_eq!(heads(&document), vec![None; 4]);
}

fn numbering_fmt_bytes() -> Vec<u8> {
    let mut file_header = vec![0; 256];
    file_header[..17].copy_from_slice(b"HWP Document File");
    let mut doc_info = Vec::new();
    let mut body = Vec::new();
    for (index, code) in [0u32, 1, 2, 6, 8, 10, 16].into_iter().enumerate() {
        let mut para_shape = vec![0; 42];
        let attr = (2 << 23) | ((index as u32) << 25);
        para_shape[..4].copy_from_slice(&attr.to_le_bytes());
        para_shape[30..32].copy_from_slice(&1u16.to_le_bytes());
        doc_info.extend(rec::record(25, 0, &para_shape));
        body.extend_from_slice(&(code << 5).to_le_bytes());
        body.extend_from_slice(&[0; 4]);
        body.extend_from_slice(&u32::MAX.to_le_bytes());
        let text: Vec<_> = format!("^{}.", index + 1).encode_utf16().collect();
        body.extend_from_slice(&(text.len() as u16).to_le_bytes());
        for unit in text {
            body.extend_from_slice(&unit.to_le_bytes());
        }
    }
    body.extend_from_slice(&1u16.to_le_bytes());
    doc_info.extend(rec::record(23, 0, &body));
    let mut section = Vec::new();
    for id in 0..7u16 {
        let mut header = vec![0; 22];
        header[8..10].copy_from_slice(&id.to_le_bytes());
        section.extend(rec::record(66, 0, &header));
    }
    cfbw::build(&[
        ("FileHeader".into(), file_header),
        ("DocInfo".into(), doc_info),
        ("BodyText/Section0".into(), section),
    ])
}

#[test]
fn numbering_fmt() {
    let bytes = numbering_fmt_bytes();
    let parsed = parse(&bytes, &ParseOptions::default()).expect("parse synthetic numbering_fmt");
    assert_eq!(
        heads(&parsed.document),
        vec![
            Some("1."),
            Some("①."),
            Some("I."),
            Some("Ⓐ."),
            Some("가."),
            Some("ㄱ."),
            Some("甲.")
        ]
    );
}

#[test]
fn numbering_raw_dump_contains_attributes_without_text() {
    let heads = hwp_core::hwp5::numbering::numbering_raw(&numbering_fmt_bytes()).expect("raw");
    let value = serde_json::to_value(heads).expect("raw JSON");
    let heads = value.as_array().expect("level heads");
    assert_eq!(heads.len(), 7);
    for (head, code) in heads.iter().zip([0, 1, 2, 6, 8, 10, 16]) {
        assert_eq!(head["code"], code);
        assert_eq!(head["attr"], code << 5);
        assert_eq!(head["numberingId"], 1);
        assert_eq!(head.as_object().expect("raw head").len(), 4);
    }
}

#[test]
fn probe_emits_head_on_first_line_only() {
    let mut parsed = hwpx("CIRCLED_DIGIT", "^1.");
    let paragraph = &mut parsed.document.sections[0].paragraphs[0];
    paragraph.line_segs = vec![
        LineSeg::default(),
        LineSeg {
            textpos: 2,
            ..LineSeg::default()
        },
    ];
    let lines = hwp_core::probe::probe(&parsed.document);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].head, "①.");
    assert_eq!(lines[1].head, "");
    assert_eq!(lines[0].text, "bo");
    assert_eq!(lines[1].text, "dy");
    let value = serde_json::to_value(&parsed.document).expect("serialize headText");
    assert_eq!(value["sections"][0]["paragraphs"][0]["headText"], "①.");
}

#[test]
fn nested_paragraph_heads_follow_document_order() {
    let caption = || {
        Some(Caption {
            paragraphs: vec![paragraph(1)],
            ..Caption::default()
        })
    };
    let mut parent = paragraph(1);
    parent.runs.push(Run {
        items: vec![
            RunItem::Table(Table {
                caption: caption(),
                cells: vec![Cell {
                    paragraphs: vec![paragraph(1)],
                    ..Cell::default()
                }],
                ..Table::default()
            }),
            RunItem::Picture(Picture {
                caption: caption(),
                ..Picture::default()
            }),
            RunItem::Shape(Shape {
                caption: caption(),
                paragraphs: vec![paragraph(1)],
                children: vec![ShapeChild {
                    item: Box::new(RunItem::Note(Note {
                        paragraphs: vec![paragraph(1)],
                        ..Note::default()
                    })),
                    frame: [[0.0; 2]; 2],
                    transform: None,
                }],
                ..Shape::default()
            }),
        ],
        ..Run::default()
    });
    parent.memos.push(Memo {
        paragraphs: vec![paragraph(1)],
        ..Memo::default()
    });
    let mut document = Document {
        numberings: vec![numbering(1)],
        para_prs: vec![heading(1, 1, 0)],
        sections: vec![Section {
            paragraphs: vec![parent, paragraph(1)],
            ..Section::default()
        }],
        ..Document::default()
    };
    resolve_heads(&mut document);
    let value = serde_json::to_value(&document).expect("nested heads JSON");
    let parent = &value["sections"][0]["paragraphs"][0];
    let items = &parent["runs"][0]["items"];
    assert_eq!(parent["headText"], "1");
    assert_eq!(items[0]["v"]["caption"]["paragraphs"][0]["headText"], "2");
    assert_eq!(items[0]["v"]["cells"][0]["paragraphs"][0]["headText"], "3");
    assert_eq!(items[1]["v"]["caption"]["paragraphs"][0]["headText"], "4");
    assert_eq!(items[2]["v"]["caption"]["paragraphs"][0]["headText"], "5");
    assert_eq!(items[2]["v"]["paragraphs"][0]["headText"], "6");
    assert_eq!(
        items[2]["v"]["children"][0]["item"]["v"]["paragraphs"][0]["headText"],
        "7"
    );
    assert_eq!(parent["memos"][0]["paragraphs"][0]["headText"], "8");
    assert_eq!(value["sections"][0]["paragraphs"][1]["headText"], "9");
}

#[test]
fn header_footer_caches_do_not_advance_counters_twice() {
    let mut parent = paragraph(1);
    parent.runs.push(Run {
        items: vec![
            RunItem::Header {
                paragraphs: vec![paragraph(1)],
                applies: "BOTH".into(),
            },
            RunItem::Footer {
                paragraphs: vec![paragraph(1)],
                applies: "BOTH".into(),
            },
        ],
        ..Run::default()
    });
    let mut document = Document {
        numberings: vec![numbering(1)],
        para_prs: vec![heading(1, 1, 0)],
        sections: vec![
            Section {
                paragraphs: vec![parent, paragraph(1)],
                header: vec![paragraph(1)],
                footer: vec![paragraph(1)],
                ..Section::default()
            },
            Section {
                paragraphs: vec![paragraph(1)],
                ..Section::default()
            },
        ],
        ..Document::default()
    };
    resolve_heads(&mut document);
    assert_eq!(heads(&document), vec![Some("1"), Some("4"), Some("5")]);
    assert_eq!(
        document.sections[0].header[0].head_text.as_deref(),
        Some("2")
    );
    assert_eq!(
        document.sections[0].footer[0].head_text.as_deref(),
        Some("3")
    );
    let items = &document.sections[0].paragraphs[0].runs[0].items;
    let RunItem::Header { paragraphs, .. } = &items[0] else {
        panic!("header item");
    };
    assert_eq!(*paragraphs, document.sections[0].header);
    let RunItem::Footer { paragraphs, .. } = &items[1] else {
        panic!("footer item");
    };
    assert_eq!(*paragraphs, document.sections[0].footer);
}

#[test]
fn all_seven_level_counters_reset_to_their_starts() {
    let mut numbering = numbering(1);
    for head in &mut numbering.heads {
        head.start = head.level + 1;
    }
    let mut document = Document {
        numberings: vec![numbering],
        para_prs: (0..7).map(|level| heading(level, 1, level)).collect(),
        sections: vec![Section {
            paragraphs: [0, 1, 2, 3, 4, 5, 6, 0, 6]
                .into_iter()
                .map(paragraph)
                .collect(),
            ..Section::default()
        }],
        ..Document::default()
    };
    resolve_heads(&mut document);
    assert_eq!(
        document.sections[0].paragraphs[6].head_text.as_deref(),
        Some("2.3.4.5.6.7.8")
    );
    assert_eq!(
        document.sections[0].paragraphs[8].head_text.as_deref(),
        Some("3.2.3.4.5.6.8")
    );
}
