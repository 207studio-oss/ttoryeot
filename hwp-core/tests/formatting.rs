#[path = "support/mod.rs"]
mod support;

use hwp_core::{parse, Document, ParseOptions};
use serde_json::{json, Value};
use support::{cfbw, rec, zipw};

fn para_shape(attr: u32, spacing: i32, extension: Option<(u32, u32)>) -> Vec<u8> {
    let mut data = attr.to_le_bytes().to_vec();
    for value in [100i32, -200, 300, 400, 500, spacing] {
        data.extend(value.to_le_bytes());
    }
    for value in [3u16, 4, 5, 11, 22, 33, 44] {
        data.extend(value.to_le_bytes());
    }
    if let Some((attr3, new_spacing)) = extension {
        data.extend(u32::MAX.to_le_bytes());
        data.extend(attr3.to_le_bytes());
        data.extend(new_spacing.to_le_bytes());
    }
    rec::record(25, 0, &data)
}

fn hwp5(version: u32, records: &[Vec<u8>], children: &[Vec<u8>]) -> Document {
    hwp5_parsed(version, records, children).document
}

fn hwp5_parsed(version: u32, records: &[Vec<u8>], children: &[Vec<u8>]) -> hwp_core::Parsed {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&version.to_le_bytes());
    let mut docinfo = rec::document_properties(1);
    docinfo.extend(records.iter().flatten());
    let mut body = rec::paragraph_header();
    let text: Vec<_> = "A😀BC\r"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    body.extend(rec::record(67, 1, &text));
    body.extend(children.iter().flatten());
    let bytes = cfbw::build(&[
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), docinfo),
        ("BodyText/Section0".to_string(), body),
    ]);
    parse(&bytes, &ParseOptions::default()).expect("synthetic HWP5")
}

fn hwpx(header: &str, paragraphs: &str) -> Document {
    hwpx_parsed(header, paragraphs).document
}

fn hwpx_parsed(header: &str, paragraphs: &str) -> hwp_core::Parsed {
    let section = format!("<sec>{paragraphs}</sec>");
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "Contents/header.xml",
            data: header.as_bytes(),
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: section.as_bytes(),
            deflate: false,
        },
    ]);
    parse(&bytes, &ParseOptions::default()).expect("synthetic HWPX")
}

fn value(document: &Document) -> Value {
    serde_json::to_value(document).unwrap()
}

#[test]
fn line_spacing_type_old_and_new() {
    for (kind, name) in [
        (0, "PERCENT"),
        (1, "FIXED"),
        (2, "BETWEEN_LINES"),
        (3, "AT_LEAST"),
    ] {
        let old = hwp5(0x0500_0204, &[para_shape(kind, 2000, None)], &[]);
        assert_eq!(old.para_prs[0].line_spacing_type, name);
        assert_eq!(old.para_prs[0].line_spacing, 2000.0);
        let new = hwp5(
            0x0500_0205,
            &[para_shape(2, -80, Some((kind | 0x8000_0000, 2000)))],
            &[],
        );
        assert_eq!(new.para_prs[0].line_spacing_type, name);
        assert_eq!(new.para_prs[0].line_spacing, 2000.0);
        assert!(new.sections[0].paragraphs[0].line_segs.is_empty());
    }
    let old = hwp5(0x0500_0204, &[para_shape(1, 2000, Some((0, 160)))], &[]);
    assert_eq!(old.para_prs[0].line_spacing_type, "FIXED");
    assert_eq!(old.para_prs[0].line_spacing, 2000.0);
    let short = hwp5(0x0501_0100, &[para_shape(1, 2000, None)], &[]);
    assert_eq!(short.para_prs[0].line_spacing_type, "FIXED");
    assert_eq!(short.para_prs[0].line_spacing, 2000.0);
}

#[test]
fn para_flags_keep_with_next() {
    for (latin, latin_name) in [(0, "KEEP_WORD"), (1, "HYPHENATION"), (2, "BREAK_WORD")] {
        for (vertical, vertical_name) in [(0, "BASELINE"), (1, "TOP"), (2, "CENTER"), (3, "BOTTOM")]
        {
            let attr = (latin << 5)
                | (vertical << 20)
                | (1 << 8)
                | (37 << 9)
                | (1 << 16)
                | (1 << 17)
                | (1 << 18)
                | (1 << 19)
                | (1 << 22)
                | (1 << 28)
                | (1 << 29)
                | (1 << 30);
            let document = hwp5(0x0500_0500, &[para_shape(attr, 160, None)], &[]);
            let para = &value(&document)["paraPrs"][0];
            assert_eq!(para["breakLatinWord"], latin_name);
            assert_eq!(para["vertAlign"], vertical_name);
            for flag in [
                "widowOrphan",
                "keepWithNext",
                "keepLines",
                "pageBreakBefore",
                "snapToGrid",
                "fontLineHeight",
                "connect",
                "ignoreMargin",
                "tailShape",
            ] {
                assert_eq!(para[flag], true, "{flag}");
            }
            assert_eq!(para["condense"], 37);
            // 문단 여백·간격은 파일에 HWPUNIT의 두 배로 들어 있다.
            assert_eq!(para["marginLeft"], 50.0);
            assert_eq!(para["marginRight"], -100.0);
            assert_eq!(para["indent"], 150.0);
            assert_eq!(para["prev"], 200.0);
            assert_eq!(para["next"], 250.0);
            assert_eq!(para["tabPrIDRef"], 3);
            assert_eq!(para["headingIdRef"], 4);
            assert_eq!(para["borderFillIDRef"], 5);
        }
    }
    let plain = hwp5(0x0500_0500, &[para_shape(0, 160, None)], &[]);
    let para = &value(&plain)["paraPrs"][0];
    for flag in [
        "widowOrphan",
        "keepWithNext",
        "keepLines",
        "pageBreakBefore",
        "fontLineHeight",
        "connect",
        "ignoreMargin",
        "tailShape",
    ] {
        assert_eq!(para[flag], false, "{flag}");
    }
}

#[test]
fn fixed_ls() {
    let document = hwp5(0x0500_0205, &[para_shape(0, 160, Some((1, 2000)))], &[]);
    assert_eq!(document.para_prs[0].line_spacing_type, "FIXED");
    assert_eq!(document.para_prs[0].line_spacing, 2000.0);
    assert!(document.sections[0].paragraphs[0].line_segs.is_empty());
}

#[test]
fn para_flags_are_independent() {
    let flags = [
        (8, "snapToGrid"),
        (16, "widowOrphan"),
        (17, "keepWithNext"),
        (18, "keepLines"),
        (19, "pageBreakBefore"),
        (22, "fontLineHeight"),
        (28, "connect"),
        (29, "ignoreMargin"),
        (30, "tailShape"),
    ];
    for (bit, _) in flags {
        let document = hwp5(0x0500_0500, &[para_shape(1 << bit, 160, None)], &[]);
        let para = &value(&document)["paraPrs"][0];
        for (other, name) in flags {
            assert_eq!(para[name], bit == other, "{name}");
        }
    }
}

/// 7번 비트(한글 줄 나눔 기준 글자)는 한/글이 HWPX에 쓰는 이름 KEEP_WORD로 읽는다.
/// 같은 문서의 HWP와 HWPX에서 이 비트가 선 문단 모양이 HWPX에서 KEEP_WORD였다.
#[test]
fn hangul_letter_break_bit_reads_as_hwpx_keep_word() {
    let letters = hwp5(0x0500_0500, &[para_shape(1 << 7, 160, None)], &[]);
    assert_eq!(letters.para_prs[0].break_non_latin_word, "KEEP_WORD");
    let words = hwp5(0x0500_0500, &[para_shape(0, 160, None)], &[]);
    assert_eq!(words.para_prs[0].break_non_latin_word, "BREAK_WORD");
}

fn char_shape(attr: u32, length: usize) -> Vec<u8> {
    let mut data = vec![0; 42];
    data.extend(1200i32.to_le_bytes());
    data.extend(attr.to_le_bytes());
    data.extend([(-12i8) as u8, 34]);
    for color in [0x0033_2211u32, 0x0066_5544, u32::MAX, 0x0099_8877] {
        data.extend(color.to_le_bytes());
    }
    data.extend(513u16.to_le_bytes());
    data.extend(0x00CC_BBAAu32.to_le_bytes());
    data.truncate(length);
    rec::record(21, 0, &data)
}

#[test]
fn char_shape_extras() {
    let attr = 3 | (1 << 2) | (11 << 4) | (2 << 11) | (1 << 18) | (3 << 21);
    let document = hwp5(0x0500_0300, &[char_shape(attr, 78)], &[]);
    let char_pr = &value(&document)["charPrs"][0];
    assert_eq!(char_pr["underlineShape"], "WAVE");
    assert_eq!(char_pr["symMark"], "CARON");
    assert_eq!(char_pr["borderFillIDRef"], 513);
    assert_eq!(char_pr["strikeoutColor"], 0xAABBCC);
    assert_eq!(char_pr["shadowType"], "CONTINUOUS");
    assert_eq!(char_pr["shadowOffsetX"], -12.0);
    assert_eq!(char_pr["shadowOffsetY"], 34.0);
    assert_eq!(char_pr["shadowColor"], 0x778899);
    assert_eq!(char_pr["bold"], true);
    assert_eq!(char_pr["italic"], true);
    assert_eq!(char_pr["shadow"], true);
    assert_eq!(char_pr["strikeout"], true);
    assert_eq!(char_pr["underline"], "BOTTOM");
    assert_eq!(char_pr["textColor"], 0x112233);
    assert_eq!(char_pr["underlineColor"], 0x445566);
    assert!(char_pr["shadeColor"].is_null());
    let old = hwp5(0x0500_0200, &[char_shape(0, 72)], &[]);
    let char_pr = &value(&old)["charPrs"][0];
    assert_eq!(char_pr["borderFillIDRef"], 0);
    assert_eq!(char_pr["strikeoutColor"], 0);
}

#[test]
fn range_tags() {
    let tags = [(0u32, 4u32, 0xA112_3456u32), (1, 3, 0x0377_8899)];
    let bytes: Vec<_> = tags
        .into_iter()
        .flat_map(|(start, end, tag)| [start, end, tag])
        .flat_map(u32::to_le_bytes)
        .collect();
    let document = hwp5(0x0500_0500, &[], &[rec::record(70, 1, &bytes)]);
    let para = &value(&document)["sections"][0]["paragraphs"][0];
    assert_eq!(
        para["rangeTags"],
        json!([
            {"start": 0, "end": 4, "tag": 0xA112_3456u32},
            {"start": 1, "end": 3, "tag": 0x0377_8899u32}
        ])
    );
    assert_eq!(
        document.sections[0].paragraphs[0].runs[0].items,
        vec![hwp_core::RunItem::Text("A😀BC".to_string())]
    );
}

#[test]
fn hwpx_char_and_para_extras() {
    let document = hwpx(
        r##"<head secCnt="1"><charPr id="0" borderFillIDRef="513" symMark="CARON">
        <underline type="BOTTOM" shape="WAVE" color="#445566"/>
        <strikeout shape="SOLID" color="#AABBCC"/>
        <shadow type="CONTINUOUS" color="#778899" offsetX="-12" offsetY="34"/>
        </charPr><paraPr id="0" condense="37" snapToGrid="1" fontLineHeight="true">
        <breakSetting widowOrphan="1" keepWithNext="true" keepLines="1" pageBreakBefore="true"/>
        <border connect="true" ignoreMargin="1"/><lineSpacing type="FIXED" value="2000"/>
        </paraPr></head>"##,
        "<p><run><t>x</t></run></p>",
    );
    let json = value(&document);
    let char_pr = &json["charPrs"][0];
    assert_eq!(char_pr["underlineShape"], "WAVE");
    assert_eq!(char_pr["symMark"], "CARON");
    assert_eq!(char_pr["borderFillIDRef"], 513);
    assert_eq!(char_pr["strikeoutColor"], 0xAABBCC);
    assert_eq!(char_pr["shadowType"], "CONTINUOUS");
    assert_eq!(char_pr["shadowOffsetX"], -12.0);
    assert_eq!(char_pr["shadowOffsetY"], 34.0);
    assert_eq!(char_pr["shadowColor"], 0x778899);
    let para = &json["paraPrs"][0];
    for flag in [
        "widowOrphan",
        "keepLines",
        "keepWithNext",
        "pageBreakBefore",
        "snapToGrid",
        "fontLineHeight",
        "connect",
        "ignoreMargin",
    ] {
        assert_eq!(para[flag], true, "{flag}");
    }
    assert_eq!(para["condense"], 37);
    assert_eq!(para["lineSpacingType"], "FIXED");
    assert_eq!(para["lineSpacing"], 2000.0);
}

#[test]
fn hwpx_none_outline_and_shadow_are_not_enabled() {
    let document = hwpx(
        r##"<head>
        <charPr id="0"/>
        <charPr id="1"><outline type="NONE"/><shadow type="NONE" color="#C0C0C0" offsetX="10" offsetY="10"/></charPr>
        <charPr id="2"><outline type="SOLID"/><shadow type="DROP"/></charPr>
        <charPr id="3"><outline type="DOT"/><shadow type="CONTINUOUS"/></charPr>
        <charPr id="4"><outline/><shadow/></charPr>
        </head>"##,
        "<p><run><t>Test</t></run></p>",
    );
    let chars = &value(&document)["charPrs"];
    for (index, expected, shadow_type) in [
        (0, false, "NONE"),
        (1, false, "NONE"),
        (2, true, "DROP"),
        (3, true, "CONTINUOUS"),
        (4, false, "NONE"),
    ] {
        assert_eq!(chars[index]["outline"], expected, "outline {index}");
        assert_eq!(chars[index]["shadow"], expected, "shadow {index}");
        assert_eq!(chars[index]["shadowType"], shadow_type);
    }
    assert_eq!(chars[1]["shadowColor"], 0xC0C0C0);
    assert_eq!(chars[1]["shadowOffsetX"], 10.0);
    assert_eq!(chars[1]["shadowOffsetY"], 10.0);
}

#[test]
fn hwpx_markpen_ranges() {
    let document = hwpx(
        "<head secCnt=\"1\"/>",
        r##"<p><run charPrIDRef="0"><t>A<markpenBegin color="#778899"/>😀B</t></run>
        <run charPrIDRef="1"><t>C<markpenEnd/>D<markpenBegin color="#112233"/>E<markpenEnd/></t></run></p>
        <p><run><t>plain</t></run></p>"##,
    );
    let para = &value(&document)["sections"][0]["paragraphs"][0];
    assert_eq!(
        para["markpens"],
        json!([
            {"start": 1, "end": 5, "color": 0x778899},
            {"start": 6, "end": 7, "color": 0x112233}
        ])
    );
    assert_eq!(
        value(&document)["sections"][0]["paragraphs"][1]["markpens"],
        json!([])
    );
    assert_eq!(
        hwp_core::text::text_summary(&document).paragraphs,
        ["A😀BCDE", "plain"]
    );
}

#[test]
fn truncated_shape_extensions_keep_legacy_values() {
    for length in 64..=74 {
        let document = hwp5(0x0500_0300, &[char_shape(0, length)], &[]);
        let character = &value(&document)["charPrs"][0];
        assert_eq!(character["textColor"], 0x112233);
        assert_eq!(
            character["borderFillIDRef"],
            if length >= 70 { 513 } else { 0 }
        );
        assert_eq!(
            character["strikeoutColor"],
            if length >= 74 { 0xAABBCC } else { 0 }
        );
    }
    let full = para_shape(1, 2000, Some((2, 400)));
    let payload = &full[4..];
    for length in 34..54 {
        let short = rec::record(25, 0, &payload[..length]);
        let document = hwp5(0x0500_0205, &[short], &[]);
        assert_eq!(document.para_prs[0].line_spacing_type, "FIXED");
        assert_eq!(document.para_prs[0].line_spacing, 2000.0);
        assert_eq!(document.para_prs[0].margin_left, 50.0);
    }
}

#[test]
fn char_shape_style_codes() {
    for (code, name) in [
        "SOLID",
        "DASH",
        "DOT",
        "DASH_DOT",
        "DASH_DOT_DOT",
        "LONG_DASH",
        "CIRCLE",
        "DOUBLE_SLIM",
        "SLIM_THICK",
        "THICK_SLIM",
        "SLIM_THICK_SLIM",
        "WAVE",
        "DOUBLEWAVE",
        "THICK3D",
        "THICKREV3D",
        "3D",
    ]
    .into_iter()
    .enumerate()
    {
        let document = hwp5(0x0500_0300, &[char_shape((code as u32) << 4, 78)], &[]);
        assert_eq!(value(&document)["charPrs"][0]["underlineShape"], name);
    }
    for (code, name) in [
        "NONE",
        "DOT_ABOVE",
        "RING_ABOVE",
        "CARON",
        "TILDE",
        "SIDE",
        "COLON",
    ]
    .into_iter()
    .enumerate()
    {
        let document = hwp5(0x0500_0300, &[char_shape((code as u32) << 21, 78)], &[]);
        assert_eq!(value(&document)["charPrs"][0]["symMark"], name);
    }
    for (code, name) in ["NONE", "DROP", "CONTINUOUS"].into_iter().enumerate() {
        let document = hwp5(0x0500_0300, &[char_shape((code as u32) << 11, 78)], &[]);
        assert_eq!(value(&document)["charPrs"][0]["shadowType"], name);
    }
    for version in [0x0500_0200, 0x0500_0201, 0x0500_0209, 0x0500_0300] {
        let document = hwp5(version, &[char_shape(0, 78)], &[]);
        assert_eq!(
            value(&document)["charPrs"][0]["borderFillIDRef"],
            if version >= 0x0500_0201 { 513 } else { 0 }
        );
        assert_eq!(
            value(&document)["charPrs"][0]["strikeoutColor"],
            if version >= 0x0500_0300 { 0xAABBCC } else { 0 }
        );
    }
}

#[test]
fn truncated_range_tag_preserves_complete_entries() {
    let valid: Vec<_> = [1u32, 3, 0xA112_3456]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    for extra in 1..12 {
        let mut bytes = valid.clone();
        bytes.extend(vec![0xFF; extra]);
        let parsed = hwp5_parsed(
            0x0500_0500,
            &[],
            &[rec::record(70, 1, &bytes), rec::record(70, 1, &valid)],
        );
        assert_eq!(
            parsed.document.sections[0].paragraphs[0].range_tags.len(),
            2
        );
        assert_eq!(
            parsed.document.sections[0].paragraphs[0].range_tags[1].tag,
            0xA112_3456
        );
        assert!(parsed.warnings.iter().any(|warning| warning.kind
            == hwp_core::WarningKind::RecordTruncated
            && warning.section == Some(0)));
    }
}

#[test]
fn hwpx_nested_and_malformed_markpens_keep_text() {
    let parsed = hwpx_parsed(
        "<head secCnt=\"1\"/>",
        r##"<p><run><t><markpenEnd/>A<markpenBegin color="#112233"/>B<markpenBegin color="#445566"/>C<markpenEnd/>D<markpenEnd/>E<markpenBegin color="private text"/>F<markpenEnd/><markpenBegin color="#778899"/>G</t></run></p>"##,
    );
    assert_eq!(
        hwp_core::text::text_summary(&parsed.document).paragraphs,
        ["ABCDEFG"]
    );
    assert_eq!(
        value(&parsed.document)["sections"][0]["paragraphs"][0]["markpens"],
        json!([
            {"start": 2, "end": 3, "color": 0x445566},
            {"start": 1, "end": 4, "color": 0x112233}
        ])
    );
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::InvalidXml));
    assert!(parsed
        .warnings
        .iter()
        .all(|warning| !warning.detail.contains("private text")
            && !warning.detail.contains("ABCDEFG")));
}

#[test]
fn hwpx_markpens_count_special_characters_and_objects() {
    let document = hwpx(
        "<head secCnt=\"1\"/>",
        r##"<p><run><markpenBegin color="#112233"/><t>A<tab/><lineBreak/><nbSpace/><fwSpace/><hyphen/></t><pic/><t>B<markpenEnd/></t></run></p>"##,
    );
    assert_eq!(
        value(&document)["sections"][0]["paragraphs"][0]["markpens"],
        json!([
            {"start": 0, "end": 22, "color": 0x112233}
        ])
    );
}

#[test]
fn hwpx_track_changes_mark_deleted_and_inserted_ranges() {
    let document = hwpx(
        "<head secCnt=\"1\"/>",
        r##"<p><run><t>ab<deleteBegin Id="1" TcId="1"/>cd<deleteEnd Id="1" TcId="1"/>e<insertBegin Id="2" TcId="2"/>fg</t></run></p><p><run><t>h<insertEnd Id="2" TcId="2"/>i</t></run></p>"##,
    );
    assert!(document.track_changes);
    let paragraphs = &value(&document)["sections"][0]["paragraphs"];
    assert_eq!(
        paragraphs[0]["tracks"],
        json!([
            {"start": 2, "end": 4, "insert": false},
            {"start": 5, "end": 7, "insert": true}
        ])
    );
    assert_eq!(
        paragraphs[1]["tracks"],
        json!([{"start": 0, "end": 1, "insert": true}])
    );
}

#[test]
fn hwpx_non_finite_shadow_offsets_default() {
    let document = hwpx("<head secCnt=\"1\"><charPr id=\"0\"><shadow offsetX=\"NaN\" offsetY=\"inf\"/></charPr></head>", "<p><run><t>x</t></run></p>");
    assert_eq!(document.char_prs[0].shadow_offset_x, 0.0);
    assert_eq!(document.char_prs[0].shadow_offset_y, 0.0);
}
