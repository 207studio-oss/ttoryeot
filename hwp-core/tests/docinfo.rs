#[path = "support/mod.rs"]
mod support;

use hwp_core::hwp5::docinfo::parse_doc_info;
use hwp_core::hwp5::record::Record;
use hwp_core::model::{BlobKind, Document, ParaHead};
use hwp_core::{parse, ParseOptions, WarningKind};

use support::{cfbw, rec};

fn u16_bytes(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}

fn u32_bytes(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

fn hwp_string(value: &str) -> Vec<u8> {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut bytes = u16_bytes(units.len() as u16).to_vec();
    for unit in units {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

fn parse_docinfo(records: &[Vec<u8>]) -> (Document, Vec<hwp_core::Warning>) {
    let bytes: Vec<u8> = records.iter().flatten().copied().collect();
    let parsed = Record::parse(&bytes);
    let mut document = Document::default();
    let mut warnings = Vec::new();
    parse_doc_info(&parsed, 0x0500_0500, &mut document, &mut warnings);
    (document, warnings)
}

fn char_shape(attr: u32) -> Vec<u8> {
    let mut body = Vec::new();
    for _ in 0..7 {
        body.extend(u16_bytes(0));
    }
    body.extend([100u8; 7]);
    body.extend([0u8; 7]);
    body.extend([100u8; 7]);
    body.extend([0u8; 7]);
    body.extend(u32_bytes(1000));
    body.extend(u32_bytes(attr));
    body.extend([0, 0]);
    body.extend(u32_bytes(0x0012_3456));
    body.extend(u32_bytes(0));
    body.extend(u32_bytes(0xFFFF_FFFF));
    body.extend(u32_bytes(0x0012_3456));
    body.extend(u16_bytes(0));
    body.extend(u32_bytes(0));
    rec::record(21, 0, &body)
}

fn border_fill(types: [u8; 5], fill: Option<u32>) -> Vec<u8> {
    let mut body = u16_bytes(0x48).to_vec();
    for kind in types.iter().take(4) {
        body.push(*kind);
        body.push(2);
        body.extend(u32_bytes(0x0012_3456));
    }
    body.push(types[4]);
    body.push(2);
    body.extend(u32_bytes(0x0065_4321));
    match fill {
        Some(color) => {
            body.extend(u32_bytes(1));
            body.extend(u32_bytes(color));
            body.extend(u32_bytes(0));
            body.extend(u32_bytes(u32::MAX));
        }
        None => body.extend(u32_bytes(0)),
    }
    rec::record(20, 0, &body)
}

fn numbering_head(attr: u32, text: &str) -> Vec<u8> {
    let mut body = u32_bytes(attr).to_vec();
    body.extend(u16_bytes(0));
    body.extend(u16_bytes(50));
    body.extend(u32_bytes(0xFFFF_FFFF));
    body.extend(hwp_string(text));
    body
}

fn bullet(attr: u32, distance: u16, character: u16) -> Vec<u8> {
    let mut body = u32_bytes(attr).to_vec();
    body.extend(u16_bytes(0));
    body.extend(u16_bytes(distance));
    body.extend(u32_bytes(0xFFFF_FFFF));
    body.extend(u16_bytes(character));
    body.extend(u32_bytes(0));
    body.extend([0u8; 4]);
    body.extend(u16_bytes(0));
    rec::record(24, 0, &body)
}

#[test]
fn char_shape_underline_strike() {
    let records: Vec<Vec<u8>> = [
        1u32 << 2,
        3u32 << 2,
        2u32 << 2,
        (1u32 << 18) | (15u32 << 26),
        (1u32 << 18) | (12u32 << 26),
    ]
    .into_iter()
    .map(char_shape)
    .collect();
    let (document, _) = parse_docinfo(&records);
    assert_eq!(document.char_prs.len(), 5);
    assert_eq!(document.char_prs[0].underline, "BOTTOM");
    assert_eq!(document.char_prs[1].underline, "TOP");
    assert_eq!(document.char_prs[2].underline, "NONE");
    assert!(!document.char_prs[3].strikeout);
    assert!(document.char_prs[4].strikeout);
    assert_eq!(document.char_prs[0].text_color, 0x563412);
    assert_eq!(document.char_prs[0].shade_color, None);
}

#[test]
fn border_fill_side_layout() {
    let records = vec![
        border_fill([3, 8, 9, 10, 11], None),
        border_fill([255, 0, 0, 0, 255], None),
        border_fill([0, 0, 0, 0, 0], Some(0x0011_2233)),
    ];
    let (document, _) = parse_docinfo(&records);
    let fill = &document.border_fills[0];
    assert_eq!(fill.id, 1);
    assert_eq!(fill.left.border_type, "DOT");
    assert_eq!(fill.right.border_type, "DOUBLE_SLIM");
    assert_eq!(fill.top.border_type, "SLIM_THICK");
    assert_eq!(fill.bottom.border_type, "THICK_SLIM");
    assert_eq!(fill.diagonal.border_type, "SLIM_THICK_SLIM");
    assert_eq!(fill.left.width, 0.15);
    assert_eq!(fill.left.color, 0x563412);
    assert_eq!(fill.diagonal.color, 0x214365);
    assert_eq!(fill.slash, "SLASH");
    assert_eq!(fill.back_slash, "BACK");
    assert_eq!(document.border_fills[1].left.border_type, "SOLID");
    assert_eq!(document.border_fills[1].diagonal.border_type, "SOLID");
    assert_eq!(document.border_fills[2].fill_color, Some(0x332211));
}

#[test]
fn numbering_bullet_heads() {
    let mut numbering = Vec::new();
    for (attr, text) in [
        (0x0C, "^1."),
        (0x10C, "^2."),
        (0x0C, "^3)"),
        (0x0C, "^4"),
        (0x0C, "^5"),
        (0x0C, "^6"),
        (0x0C, "^7"),
    ] {
        numbering.extend(numbering_head(attr, text));
    }
    numbering.extend(u16_bytes(1));
    let records = vec![
        rec::record(23, 0, &numbering),
        bullet(0x08, 50, 0x274D),
        bullet(0x10, 300, 0x00AD),
    ];
    let (document, _) = parse_docinfo(&records);
    let value = &document.numberings[0];
    assert_eq!(value.id, 1);
    assert_eq!(value.heads.len(), 7);
    assert_eq!(value.heads[0].format, "DIGIT");
    assert_eq!(value.heads[1].format, "HANGUL_SYLLABLE");
    assert_eq!(value.heads[0].text, "^1.");
    assert_eq!(value.heads[0].char_pr_id, None);
    assert_eq!(
        value.heads[0].para,
        ParaHead {
            auto_indent: true,
            use_inst_width: true,
            width_adjust: 0.0,
            text_offset_percent: true,
            text_offset: 50.0,
        }
    );
    assert_eq!(value.start, 1);
    assert_eq!(document.bullets[0].char, "\u{274D}");
    assert!(document.bullets[0].para.text_offset_percent);
    assert!(!document.bullets[1].para.text_offset_percent);
    assert_eq!(document.bullets[1].para.text_offset, 300.0);
}

#[test]
fn numbering_code_table() {
    let names = [
        "DIGIT",
        "CIRCLED_DIGIT",
        "ROMAN_CAPITAL",
        "ROMAN_SMALL",
        "LATIN_CAPITAL",
        "LATIN_SMALL",
        "CIRCLED_LATIN_CAPITAL",
        "CIRCLED_LATIN_SMALL",
        "HANGUL_SYLLABLE",
        "CIRCLED_HANGUL_SYLLABLE",
        "HANGUL_JAMO",
        "CIRCLED_HANGUL_JAMO",
        "HANGUL_PHONETIC",
        "IDEOGRAPH",
        "CIRCLED_IDEOGRAPH",
        "DECAGON_CIRCLE",
        "DECAGON_CIRCLE_HANJA",
    ];
    for (code, expected) in names.into_iter().enumerate() {
        let mut body = Vec::new();
        for level in 1..=7 {
            body.extend(numbering_head(
                ((code as u32) << 5) | 0x1F,
                &format!("^{level}."),
            ));
        }
        body.extend(u16_bytes(1));
        let (document, warnings) = parse_docinfo(&[rec::record(23, 0, &body)]);
        assert!(warnings.is_empty());
        for head in &document.numberings[0].heads {
            assert_eq!(head.format, expected, "HWP5 number format code {code}");
        }
    }
}

#[test]
fn unknown_numbering_code_warns_and_uses_digit() {
    let mut body = Vec::new();
    for level in 1..=7 {
        body.extend(numbering_head(31 << 5, &format!("private^{level}")));
    }
    body.extend(u16_bytes(1));
    let (document, warnings) = parse_docinfo(&[rec::record(23, 0, &body)]);
    assert!(document.numberings[0]
        .heads
        .iter()
        .all(|head| head.format == "DIGIT"));
    assert_eq!(warnings.len(), 7);
    assert!(warnings
        .iter()
        .all(|warning| warning.kind == WarningKind::UnknownNumberFormat));
    assert!(warnings
        .iter()
        .all(|warning| !warning.detail.contains("private")));
}

/// HWP 5 탭 위치는 HWPUNIT의 두 배로 저장된다(HWPX switch의 default 값과 같고, case 값의 두 배).
#[test]
fn tab_def_stops() {
    let mut body = u32_bytes(3).to_vec();
    body.extend(u32_bytes(2));
    body.extend(u32_bytes(2000));
    body.push(2);
    body.push(3);
    body.extend(u16_bytes(0));
    body.extend((-5i32).to_le_bytes());
    body.push(0);
    body.push(0);
    body.extend(u16_bytes(0));
    let records = vec![rec::record(22, 0, &body)];
    let (document, _) = parse_docinfo(&records);
    let tab = &document.tab_prs[0];
    assert_eq!(tab.id, 0);
    assert!(tab.auto_left && tab.auto_right);
    assert_eq!(tab.stops.len(), 1);
    assert_eq!(tab.stops[0].position, 1000.0);
    assert_eq!(tab.stops[0].kind, "CENTER");
    assert_eq!(tab.stops[0].leader, "DOT");
}

#[test]
fn face_name_alt_default() {
    let mut mappings = u32_bytes(0).to_vec();
    for count in [1u32, 1, 0, 0, 0, 0, 0] {
        mappings.extend(u32_bytes(count));
    }
    let mut first = vec![0x80 | 0x40 | 0x20 | 0x02];
    first.extend(hwp_string("본문"));
    first.push(1);
    first.extend(hwp_string("대체"));
    first.extend([0u8; 10]);
    first.extend(hwp_string("Default"));
    let mut second = vec![0x01];
    second.extend(hwp_string("Latin"));
    let records = vec![
        rec::record(17, 0, &mappings),
        rec::record(19, 0, &first),
        rec::record(19, 0, &second),
    ];
    let (document, _) = parse_docinfo(&records);
    let hangul = &document.font_faces["hangul"][0];
    assert_eq!(hangul.id, 0);
    assert_eq!(hangul.name, "본문");
    assert_eq!(hangul.face_type, 2);
    assert_eq!(hangul.alt_name.as_deref(), Some("대체"));
    assert_eq!(hangul.alt_type, 1);
    assert_eq!(hangul.default_name.as_deref(), Some("Default"));
    let latin = &document.font_faces["latin"][0];
    assert_eq!(latin.id, 0);
    assert_eq!(latin.name, "Latin");
    assert_eq!(latin.face_type, 1);
    assert_eq!(latin.alt_name, None);
    assert_eq!(latin.default_name, None);
    assert_eq!(document.font_faces.len(), 2);
}

#[test]
fn memo_shape_docinfo() {
    let mut body = u32_bytes(15591).to_vec();
    body.extend([1, 1]);
    body.extend(u32_bytes(0x00AE_D7B6));
    body.extend(u32_bytes(0x00E9_FFF0));
    body.extend(u32_bytes(0x00C7_F1CF));
    body.extend(u32_bytes(0));
    let records = vec![rec::record(rec::T_MEMO_SHAPE, 0, &body)];
    let (document, _) = parse_docinfo(&records);
    let shape = &document.memo_shapes[0];
    assert_eq!(shape.id, 1);
    assert_eq!(shape.width, 15591.0);
    assert_eq!(shape.line_type, 1);
    assert_eq!(shape.line_width, 1);
    assert_eq!(shape.line_color, 0xB6D7AE);
    assert_eq!(shape.fill_color, 0xF0FFE9);
    assert_eq!(shape.active_color, 0xCFF1C7);
    assert_eq!(shape.memo_type, 0);
}

fn file_header(flags: u32) -> Vec<u8> {
    let mut header = vec![0u8; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    header[36..40].copy_from_slice(&flags.to_le_bytes());
    header
}

fn bin_data(kind: u8, mode: u8, id: u16, extension: &str) -> Vec<u8> {
    let mut body = u16_bytes(u16::from(kind) | (u16::from(mode) << 4)).to_vec();
    if kind == 0 {
        body.extend(hwp_string("file:///linked.png"));
        body.extend(hwp_string("linked.png"));
    } else {
        body.extend(u16_bytes(id));
        body.extend(hwp_string(extension));
    }
    rec::record(18, 0, &body)
}

fn hwp5_with_bins() -> Vec<u8> {
    let mut doc_info = rec::document_properties(1);
    doc_info.extend(bin_data(1, 2, 1, "png"));
    doc_info.extend(bin_data(1, 1, 2, "bin"));
    doc_info.extend(bin_data(0, 0, 0, ""));
    doc_info.extend(bin_data(1, 0, 4, "dat"));
    doc_info.extend(bin_data(1, 1, 5, "raw"));
    doc_info.extend(bin_data(1, 2, 6, "missing"));

    let mut section = rec::paragraph_header();
    section.extend(rec::record(rec::T_PARA_TEXT, 1, b"x\0"));
    let streams = vec![
        ("FileHeader".to_string(), file_header(1)),
        ("DocInfo".to_string(), cfbw::compress(&doc_info)),
        ("BinData/BIN0001.png".to_string(), b"raw-png".to_vec()),
        (
            "BinData/BIN0002.bin".to_string(),
            cfbw::compress(b"compressed-bin"),
        ),
        (
            "BinData/BIN0004.dat".to_string(),
            cfbw::compress(b"document-mode"),
        ),
        (
            "BinData/BIN0005.raw".to_string(),
            b"plain-fallback".to_vec(),
        ),
        ("BodyText/Section0".to_string(), cfbw::compress(&section)),
    ];
    cfbw::build(&streams)
}

fn hwp5_with_font_bin() -> Vec<u8> {
    let mut doc_info = rec::document_properties(1);
    doc_info.extend(bin_data(1, 1, 7, "TTF"));

    let mut section = rec::paragraph_header();
    section.extend(rec::record(rec::T_PARA_TEXT, 1, b"x\0"));
    let streams = vec![
        ("FileHeader".to_string(), file_header(1)),
        ("DocInfo".to_string(), cfbw::compress(&doc_info)),
        (
            "BinData/BIN0007.TTF".to_string(),
            cfbw::compress(b"raw-font"),
        ),
        ("BodyText/Section0".to_string(), cfbw::compress(&section)),
    ];
    cfbw::build(&streams)
}

#[test]
fn font_bin_data_is_kept_as_a_font_blob() {
    let parsed = parse(&hwp5_with_font_bin(), &ParseOptions::default()).expect("parse");
    let font = parsed
        .document
        .bins
        .iter()
        .find(|blob| blob.id == "bin1")
        .expect("font blob");
    assert_eq!(font.kind, BlobKind::Font);
    assert_eq!(font.data.as_slice(), b"raw-font");
}

#[test]
fn bin_data_modes_and_link() {
    let parsed = parse(&hwp5_with_bins(), &ParseOptions::default()).expect("parse");
    let data = |id: &str| {
        parsed
            .document
            .bins
            .iter()
            .find(|blob| blob.id == id)
            .map(|blob| blob.data.as_slice())
    };
    assert_eq!(data("bin1"), Some(&b"raw-png"[..]));
    assert_eq!(data("bin2"), Some(&b"compressed-bin"[..]));
    assert_eq!(data("bin4"), Some(&b"document-mode"[..]));
    assert_eq!(data("bin5"), Some(&b"plain-fallback"[..]));
    assert_eq!(data("bin3"), None);
    assert_eq!(data("bin6"), None);
    assert_eq!(
        parsed
            .warnings
            .iter()
            .filter(|warning| warning.kind == WarningKind::LinkedPicture)
            .count(),
        1
    );
    assert_eq!(
        parsed
            .warnings
            .iter()
            .filter(|warning| warning.kind == WarningKind::MissingBinData)
            .count(),
        1
    );
}
