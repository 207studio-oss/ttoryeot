#[path = "support/mod.rs"]
mod support;

use hwp_core::{parse, Document, ParseOptions, RunItem};
use serde_json::{json, Value};

use support::{cfbw, rec, zipw};

fn control(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut data: Vec<u8> = id.iter().rev().copied().collect();
    data.extend_from_slice(payload);
    data
}

fn cold(count: u16, gap: u16) -> Vec<u8> {
    let mut payload = ((count << 2) | 0x1000).to_le_bytes().to_vec();
    payload.extend_from_slice(&gap.to_le_bytes());
    payload.extend_from_slice(&[0; 8]);
    control(b"cold", &payload)
}

fn paragraph(level: u16, columns: Option<&[u8]>) -> Vec<u8> {
    let mut data = rec::record(66, level, &[0; 22]);
    data.extend(rec::record(67, level + 1, b"x\0"));
    if let Some(columns) = columns {
        data.extend(rec::record(71, level + 1, columns));
    }
    data
}

fn hwp5(section: &[u8]) -> Document {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    let bytes = cfbw::build(&[
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), rec::document_properties(1)),
        ("BodyText/Section0".to_string(), section.to_vec()),
    ]);
    parse(&bytes, &ParseOptions::default()).unwrap().document
}

fn hwpx(body: &str) -> Document {
    let section = format!(r#"<sec xmlns:hp="urn:hp">{body}</sec>"#);
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/header.xml",
            data: br#"<head secCnt="1"/>"#,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: section.as_bytes(),
            deflate: false,
        },
    ]);
    parse(&bytes, &ParseOptions::default()).unwrap().document
}

fn column_json(document: &Document, index: usize) -> Value {
    serde_json::to_value(&document.sections[0].paragraphs[index]).unwrap()["columns"].clone()
}

#[test]
fn second_column_def_on_paragraph() {
    let mut cols = paragraph(0, Some(&cold(1, 0)));
    cols.extend(paragraph(0, Some(&cold(2, 500))));
    cols.extend(paragraph(0, None));
    let binary = hwp5(&cols);
    let xml = hwpx(
        r#"<hp:p><hp:run><hp:ctrl><hp:colPr colCount="1" sameGap="0" sameSz="1"/></hp:ctrl></hp:run></hp:p>
        <hp:p><hp:run><hp:ctrl><hp:colPr colCount="2" sameGap="500" sameSz="1"/></hp:ctrl></hp:run></hp:p>
        <hp:p><hp:run><hp:t>x</hp:t></hp:run></hp:p>"#,
    );
    for document in [binary, xml] {
        assert_eq!(column_json(&document, 0)["count"], 1);
        assert_eq!(column_json(&document, 1)["count"], 2);
        assert_eq!(column_json(&document, 1)["gap"], 500.0);
        assert!(column_json(&document, 2).is_null());
        assert_eq!(document.sections[0].columns, 1);
        assert_eq!(document.sections[0].column_gap, 0.0);
    }
}

#[test]
fn textbox_cold_ignored() {
    let mut body = paragraph(0, None);
    body.extend(rec::record(71, 1, &control(b"gso ", &[0; 40])));
    body.extend(rec::record(76, 2, &[0; 48]));
    body.extend(rec::record(72, 3, &[0; 22]));
    body.extend(paragraph(3, Some(&cold(3, 900))));
    let nested_only = hwp5(&body);
    assert_eq!(nested_only.sections[0].columns, 1);
    assert_eq!(nested_only.sections[0].column_gap, 1134.0);
    body.extend(paragraph(0, Some(&cold(2, 500))));
    let binary = hwp5(&body);
    let xml = hwpx(
        r#"<hp:p><hp:run><hp:rect><hp:drawText><hp:subList><hp:p><hp:run><hp:ctrl>
        <hp:colPr colCount="3" sameGap="900"/></hp:ctrl><hp:t>box</hp:t></hp:run></hp:p>
        </hp:subList></hp:drawText></hp:rect></hp:run></hp:p>
        <hp:p><hp:run><hp:ctrl><hp:colPr colCount="2" sameGap="500"/></hp:ctrl></hp:run></hp:p>"#,
    );
    for document in [binary, xml] {
        let section = &document.sections[0];
        assert_eq!(section.columns, 2);
        assert_eq!(section.column_gap, 500.0);
        assert!(column_json(&document, 0).is_null());
        let shape = section.paragraphs[0].runs[0]
            .items
            .iter()
            .find_map(|item| match item {
                RunItem::Shape(shape) => Some(shape),
                _ => None,
            })
            .unwrap();
        assert_eq!(shape.paragraphs.len(), 1);
        assert!(serde_json::to_value(&shape.paragraphs[0]).unwrap()["columns"].is_null());
    }
}

#[test]
fn cell_cold_ignored() {
    let mut body = paragraph(0, None);
    body.extend(rec::record(71, 1, &control(b"tbl ", &[0; 44])));
    let mut table = vec![0; 22];
    table[4..6].copy_from_slice(&1u16.to_le_bytes());
    table[6..8].copy_from_slice(&1u16.to_le_bytes());
    body.extend(rec::record(77, 2, &table));
    let mut cell = vec![0; 34];
    cell[12..14].copy_from_slice(&1u16.to_le_bytes());
    cell[14..16].copy_from_slice(&1u16.to_le_bytes());
    body.extend(rec::record(72, 2, &cell));
    body.extend(paragraph(2, Some(&cold(3, 900))));
    body.extend(paragraph(0, Some(&cold(2, 500))));
    let binary = hwp5(&body);
    let xml = hwpx(
        r#"<hp:p><hp:run><hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc>
        <hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/>
        <hp:subList><hp:p><hp:run><hp:ctrl><hp:colPr colCount="3" sameGap="900"/></hp:ctrl>
        <hp:t>cell</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>
        <hp:p><hp:run><hp:ctrl><hp:colPr colCount="2" sameGap="500"/></hp:ctrl></hp:run></hp:p>"#,
    );
    for document in [binary, xml] {
        let section = &document.sections[0];
        assert_eq!(section.columns, 2);
        assert_eq!(section.column_gap, 500.0);
        assert!(column_json(&document, 0).is_null());
        let table = section.paragraphs[0].runs[0]
            .items
            .iter()
            .find_map(|item| match item {
                RunItem::Table(table) => Some(table),
                _ => None,
            })
            .unwrap();
        assert_eq!(table.cells.len(), 1);
        assert_eq!(table.cells[0].paragraphs.len(), 1);
        assert!(table.cells[0].paragraphs[0].columns.is_none());
    }
}

#[test]
fn header_footer_and_note_columns_ignored() {
    for (id, name) in [
        (b"head", "header"),
        (b"foot", "footer"),
        (b"fn  ", "footNote"),
        (b"en  ", "endNote"),
    ] {
        let mut body = paragraph(0, None);
        body.extend(rec::record(71, 1, &control(id, &[0; 8])));
        body.extend(rec::record(72, 2, &[0; 8]));
        body.extend(paragraph(2, Some(&cold(3, 900))));
        body.extend(paragraph(0, Some(&cold(2, 500))));
        let binary = hwp5(&body);
        let xml = hwpx(&format!(
            r#"<hp:p><hp:run><hp:ctrl><hp:{name}><hp:subList><hp:p><hp:run><hp:ctrl>
            <hp:colPr colCount="3" sameGap="900"/></hp:ctrl><hp:t>nested</hp:t></hp:run></hp:p>
            </hp:subList></hp:{name}></hp:ctrl></hp:run></hp:p>
            <hp:p><hp:run><hp:ctrl><hp:colPr colCount="2" sameGap="500"/></hp:ctrl></hp:run></hp:p>"#
        ));
        for document in [binary, xml] {
            assert_eq!(document.sections[0].columns, 2, "{name}");
            assert_eq!(document.sections[0].column_gap, 500.0, "{name}");
            let paragraphs = document.sections[0].paragraphs[0].runs[0]
                .items
                .iter()
                .find_map(|item| match item {
                    RunItem::Header { paragraphs, .. } | RunItem::Footer { paragraphs, .. } => {
                        Some(paragraphs)
                    }
                    RunItem::Note(note) => Some(&note.paragraphs),
                    _ => None,
                })
                .unwrap();
            assert_eq!(paragraphs.len(), 1);
            assert!(paragraphs[0].columns.is_none(), "{name}");
        }
    }
}

#[test]
fn column_widths_and_line() {
    let mut payload = ((2u16 << 2) | (1 << 10)).to_le_bytes().to_vec();
    payload.extend_from_slice(&500u16.to_le_bytes());
    payload.extend_from_slice(&12000u16.to_le_bytes());
    payload.extend_from_slice(&18000u16.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&[3, 5]);
    payload.extend_from_slice(&0x00563412u32.to_le_bytes());
    let binary = hwp5(&paragraph(0, Some(&control(b"cold", &payload))));
    let xml = hwpx(
        r##"<hp:p><hp:run><hp:ctrl><hp:colPr colCount="2" sameGap="500" sameSz="0" layout="RIGHT">
        <hp:colLine type="DOT" width="0.3 mm" color="#123456"/>
        <hp:colSz width="12000" gap="500"/><hp:colSz width="18000" gap="0"/>
        </hp:colPr></hp:ctrl></hp:run></hp:p>"##,
    );
    for document in [binary, xml] {
        assert_eq!(
            column_json(&document, 0),
            json!({
                "count": 2, "gap": 500.0, "sameWidth": false,
                "widths": [12000.0, 18000.0], "lineType": "DOT",
                "lineWidth": 0.3, "lineColor": 0x123456, "direction": "RIGHT"
            })
        );
    }
}

#[test]
fn same_width_columns_keep_separator() {
    let mut payload = ((3u16 << 2) | (2 << 10) | 0x1000).to_le_bytes().to_vec();
    payload.extend_from_slice(&700u16.to_le_bytes());
    payload.extend_from_slice(&0xBEEFu16.to_le_bytes());
    payload.extend_from_slice(&[1, 12]);
    payload.extend_from_slice(&0x00332211u32.to_le_bytes());
    let binary = hwp5(&paragraph(0, Some(&control(b"cold", &payload))));
    let xml = hwpx(
        r##"<hp:p><hp:run><hp:ctrl><hp:colPr colCount="3" sameGap="700" sameSz="true" layout="MIRROR">
        <hp:colLine type="SOLID" width="2.0 mm" color="#112233"/>
        </hp:colPr></hp:ctrl></hp:run></hp:p>"##,
    );
    for document in [binary, xml] {
        assert_eq!(
            column_json(&document, 0),
            json!({
                "count": 3, "gap": 700.0, "sameWidth": true, "widths": [],
                "lineType": "SOLID", "lineWidth": 2.0,
                "lineColor": 0x112233, "direction": "MIRROR"
            })
        );
    }
}

#[test]
fn truncated_column_definitions_are_bounded() {
    let data = cold(255, 500);
    for length in 0..data.len() {
        let document = hwp5(&paragraph(0, Some(&data[..length])));
        if length < 8 {
            assert!(document.sections[0].paragraphs[0].columns.is_none());
        } else {
            assert_eq!(column_json(&document, 0)["count"], 255);
            assert_eq!(column_json(&document, 0)["widths"], json!([]));
        }
    }
    let mut unequal = control(b"cold", &((255u16 << 2).to_le_bytes()));
    unequal.extend_from_slice(&500u16.to_le_bytes());
    unequal.extend_from_slice(&12000u16.to_le_bytes());
    let document = hwp5(&paragraph(0, Some(&unequal)));
    assert_eq!(column_json(&document, 0)["widths"], json!([]));
    assert_eq!(column_json(&document, 0)["lineType"], "NONE");
}

#[test]
fn probe_uses_paragraph_columns() {
    let document = hwpx(
        r#"<hp:p><hp:run><hp:ctrl><hp:colPr colCount="1" sameGap="0"/></hp:ctrl><hp:t>title</hp:t></hp:run>
        <hp:linesegarray><hp:lineseg vertpos="0"/></hp:linesegarray></hp:p>
        <hp:p><hp:run><hp:ctrl><hp:colPr colCount="2" sameGap="500"/></hp:ctrl><hp:t>body</hp:t></hp:run>
        <hp:linesegarray><hp:lineseg vertpos="1000"/></hp:linesegarray></hp:p>
        <hp:p><hp:run><hp:t>next</hp:t></hp:run>
        <hp:linesegarray><hp:lineseg vertpos="500"/></hp:linesegarray></hp:p>"#,
    );
    let lines = hwp_core::probe::probe(&document);
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.page, line.col))
            .collect::<Vec<_>>(),
        [(1, 0), (1, 0), (1, 1)]
    );
    assert_eq!(lines[2].x_pt, 300.14);
}

#[test]
fn probe_uses_unequal_column_widths() {
    let document = hwpx(
        r#"<hp:p><hp:run><hp:ctrl><hp:colPr colCount="3" sameGap="500" sameSz="0">
        <hp:colSz width="12000"/><hp:colSz width="18000"/><hp:colSz width="11520"/>
        </hp:colPr></hp:ctrl><hp:t>abc</hp:t></hp:run><hp:linesegarray>
        <hp:lineseg vertpos="1000" textpos="0"/>
        <hp:lineseg vertpos="500" textpos="1"/>
        <hp:lineseg vertpos="0" textpos="2"/></hp:linesegarray></hp:p>"#,
    );
    let lines = hwp_core::probe::probe(&document);
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.page, line.col, line.x_pt))
            .collect::<Vec<_>>(),
        [(1, 0, 85.04), (1, 1, 210.04), (1, 2, 395.04)]
    );
}
