#[path = "support/mod.rs"]
mod support;

use hwp_core::{parse, Document, ParseOptions};
use serde_json::{json, Value};

use support::{cfbw, rec, zipw};

fn border_record(flags: u32, offsets: [i16; 4], id: u16) -> Vec<u8> {
    let mut data = flags.to_le_bytes().to_vec();
    for offset in offsets {
        data.extend(offset.to_le_bytes());
    }
    data.extend(id.to_le_bytes());
    rec::record(75, 2, &data)
}

fn hwp5(sections: &[Vec<u8>]) -> hwp_core::Parsed {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    let mut streams = vec![
        ("FileHeader".to_string(), header),
        (
            "DocInfo".to_string(),
            rec::document_properties(sections.len() as u16),
        ),
    ];
    for (index, section) in sections.iter().enumerate() {
        streams.push((format!("BodyText/Section{index}"), section.clone()));
    }
    parse(&cfbw::build(&streams), &ParseOptions::default()).unwrap()
}

fn section(records: &[Vec<u8>]) -> Vec<u8> {
    let mut body = rec::paragraph_header();
    body.extend(rec::record(67, 1, b"x\0"));
    body.extend(rec::record(71, 1, b"dces"));
    for record in records {
        body.extend(record);
    }
    body
}

fn fills(document: &Document, index: usize) -> Value {
    serde_json::to_value(&document.sections[index]).unwrap()["pageBorderFills"].clone()
}

fn expected_fills() -> Value {
    json!([
        {
            "type": "BOTH", "borderFillID": 7, "textBorder": "CONTENT",
            "headerInside": true, "footerInside": false, "fillArea": "PAPER",
            "offsets": {"left": 100.0, "right": 200.0, "top": 300.0, "bottom": 400.0}
        },
        {
            "type": "EVEN", "borderFillID": 8, "textBorder": "PAPER",
            "headerInside": false, "footerInside": true, "fillArea": "PAGE",
            "offsets": {"left": 500.0, "right": 600.0, "top": 700.0, "bottom": 800.0}
        },
        {
            "type": "ODD", "borderFillID": 65535, "textBorder": "CONTENT",
            "headerInside": true, "footerInside": true, "fillArea": "BORDER",
            "offsets": {"left": -32768.0, "right": -1.0, "top": 32767.0, "bottom": 1417.0}
        }
    ])
}

#[test]
fn page_border_fill_three_kinds() {
    let body = section(&[
        border_record(2, [100, 200, 300, 400], 7),
        rec::record(74, 2, &[0; 28]),
        border_record(1 | 4 | (1 << 3), [500, 600, 700, 800], 8),
        border_record(2 | 4 | (2 << 3), [-32768, -1, 32767, 1417], 65535),
    ]);
    let other = section(&[border_record(1, [1, 2, 3, 4], 99)]);
    let parsed = hwp5(&[body, other]);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(fills(&parsed.document, 0), expected_fills());
    assert_eq!(fills(&parsed.document, 1)[0]["type"], "BOTH");
    assert_eq!(fills(&parsed.document, 1)[0]["borderFillID"], 99);
    let without = hwp5(&[section(&[rec::record(74, 2, &[0; 28])])]);
    let mut before = serde_json::to_value(&without.document.sections[0]).unwrap();
    let mut after = serde_json::to_value(&parsed.document.sections[0]).unwrap();
    before.as_object_mut().unwrap().remove("pageBorderFills");
    after.as_object_mut().unwrap().remove("pageBorderFills");
    assert_eq!(before, after);
}

fn hwpx(sec_pr: &str) -> Document {
    let xml = format!(
        r#"<s:sec xmlns:s="urn:s" xmlns:p="urn:p"><p:p><p:run><p:secPr>{sec_pr}</p:secPr><p:t>x</p:t></p:run></p:p></s:sec>"#
    );
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "Contents/header.xml",
            data: br#"<head secCnt="1"/>"#,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: xml.as_bytes(),
            deflate: false,
        },
    ]);
    let parsed = parse(&bytes, &ParseOptions::default()).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    parsed.document
}

#[test]
fn hwpx_page_border_fill() {
    let xml = r#"
        <p:pageBorderFill p:type="ODD" p:borderFillIDRef="65535" p:textBorder="CONTENT"
            p:headerInside="true" p:footerInside="1" p:fillArea="BORDER">
            <p:offset p:left="-32768" p:right="-1" p:top="32767" p:bottom="1417"/>
        </p:pageBorderFill>
        <p:pageBorderFill type="BOTH" borderFillIDRef="7" textBorder="CONTENT"
            headerInside="1" footerInside="false" fillArea="PAPER">
            <p:offset left="100" right="200" top="300" bottom="400"/>
        </p:pageBorderFill>
        <p:pageBorderFill type="EVEN" borderFillIDRef="8" textBorder="PAPER"
            headerInside="0" footerInside="true" fillArea="PAGE">
            <p:offset left="500" right="600" top="700" bottom="800"/>
        </p:pageBorderFill>"#;
    let document = hwpx(xml);
    let expected = expected_fills();
    assert_eq!(
        fills(&document, 0),
        json!([expected[2], expected[0], expected[1]])
    );
    let mut before = serde_json::to_value(&hwpx("").sections[0]).unwrap();
    let mut after = serde_json::to_value(&document.sections[0]).unwrap();
    before.as_object_mut().unwrap().remove("pageBorderFills");
    after.as_object_mut().unwrap().remove("pageBorderFills");
    assert_eq!(before, after);
}

#[test]
fn absent_page_border_fills_and_xml_defaults() {
    assert_eq!(fills(&hwp5(&[section(&[])]).document, 0), json!([]));
    assert_eq!(fills(&hwpx(""), 0), json!([]));
    assert_eq!(
        fills(&hwpx("<p:pageBorderFill/>"), 0),
        json!([{
            "type": "BOTH", "borderFillID": 0, "textBorder": "PAPER",
            "headerInside": false, "footerInside": false, "fillArea": "PAPER",
            "offsets": {"left": 0.0, "right": 0.0, "top": 0.0, "bottom": 0.0}
        }])
    );
    let document = hwpx(
        r#"<p:pageBorderFill><p:offset left="NaN" right="inf" top="-inf" bottom="123"/></p:pageBorderFill>"#,
    );
    assert_eq!(
        fills(&document, 0)[0]["offsets"],
        json!({
            "left": 0.0, "right": 0.0, "top": 0.0, "bottom": 123.0
        })
    );
}

#[test]
fn truncated_page_border_fill_keeps_following_kind() {
    for length in 0..14 {
        let parsed = hwp5(&[section(&[
            border_record(1, [1, 2, 3, 4], 7),
            rec::record(75, 2, &vec![0; length]),
            border_record(0, [5, 6, 7, 8], 9),
        ])]);
        let values = fills(&parsed.document, 0);
        assert_eq!(values.as_array().unwrap().len(), 2);
        assert_eq!(values[0]["type"], "BOTH");
        assert_eq!(values[1]["type"], "ODD");
        assert_eq!(values[1]["borderFillID"], 9);
        assert!(parsed.warnings.iter().any(|warning| warning.kind
            == hwp_core::WarningKind::RecordTruncated
            && warning.section == Some(0)));
    }
}

fn load_fatal_allowlist() -> std::collections::BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .join("tools/hwp_golden/fatal_allow.txt");
    let mut allowed = std::collections::BTreeMap::new();
    for line in std::fs::read_to_string(path)
        .expect("read fatal allowlist")
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        if let (Some(file), Some(kind)) = (parts.next(), parts.next()) {
            allowed.insert(file.to_string(), kind.to_string());
        }
    }
    allowed
}

#[test]
fn golden_page_border_fill_three_per_section() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("read HWP_GOLDEN_DIR")
        .map(|entry| entry.expect("read corpus entry").path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("hwp"))
        })
        .collect();
    files.sort();
    let allowed = load_fatal_allowlist();
    let mut documents = 0;
    let mut sections = 0;
    for path in files {
        let bytes = std::fs::read(&path).expect("read corpus input");
        if hwp_core::detect::detect(&bytes) == hwp_core::detect::Format::Hwp3 {
            continue;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parsed = match parse(&bytes, &ParseOptions::default()) {
            Ok(parsed) => parsed,
            Err(fatal) if allowed.get(&name).map(String::as_str) == Some(fatal.kind()) => continue,
            Err(fatal) => panic!("HWP5 parse failed: {}", fatal.kind()),
        };
        documents += 1;
        for section in &parsed.document.sections {
            assert_eq!(
                section.page_border_fills.len(),
                3,
                "document {documents}, section {sections}"
            );
            let kinds: Vec<_> = section
                .page_border_fills
                .iter()
                .map(|fill| fill.kind.as_str())
                .collect();
            assert_eq!(kinds, ["BOTH", "EVEN", "ODD"]);
            sections += 1;
        }
    }
    assert_eq!(documents, 158);
    assert!(sections >= documents);
    println!("documents={documents} sections={sections}");
}
