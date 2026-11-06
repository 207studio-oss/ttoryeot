#[path = "support/mod.rs"]
mod support;

use hwp_core::{parse, Document, ParseOptions};
use serde_json::{json, Value};

use support::{cfbw, rec, zipw};

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn hwp5(master_lists: &[&str]) -> Document {
    let mut body = rec::paragraph_header();
    body.extend(rec::record(rec::T_PARA_TEXT, 1, &utf16("x")));
    body.extend(rec::record(71, 1, b"dces"));
    for text in master_lists {
        body.extend(rec::record(rec::T_LIST_HEADER, 2, &[0; 34]));
        body.extend(rec::record(rec::T_PARA_HEADER, 2, &[0; 22]));
        body.extend(rec::record(rec::T_PARA_TEXT, 3, &utf16(text)));
    }
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    let streams = vec![
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), rec::document_properties(1)),
        ("BodyText/Section0".to_string(), body),
    ];
    parse(&cfbw::build(&streams), &ParseOptions::default())
        .unwrap()
        .document
}

fn hwpx(sec_pr: &str, master_pages: &[(&str, String)]) -> Document {
    let xml = format!(
        r#"<s:sec xmlns:s="urn:s" xmlns:p="urn:p"><p:p><p:run><p:secPr>{sec_pr}</p:secPr><p:t>x</p:t></p:run></p:p></s:sec>"#
    );
    let mut entries = vec![
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
    ];
    for (name, data) in master_pages {
        entries.push(zipw::Entry {
            name,
            data: data.as_bytes(),
            deflate: false,
        });
    }
    let parsed = parse(&zipw::build(&entries), &ParseOptions::default()).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    parsed.document
}

/// 바탕쪽마다 놓일 쪽과 글.
fn summary(document: &Document) -> Value {
    let section = serde_json::to_value(&document.sections[0]).unwrap();
    let pages = section["masterPages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Value::Array(
        pages
            .iter()
            .map(|page| {
                let text: String = page["paragraphs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|para| para["runs"].as_array().unwrap().clone())
                    .flat_map(|run| run["items"].as_array().unwrap().clone())
                    .filter_map(|item| item["v"].as_str().map(str::to_string))
                    .collect();
                json!([page["applies"], text])
            })
            .collect(),
    )
}

#[test]
fn hwp5_lists_under_section_definition_are_master_pages() {
    assert_eq!(summary(&hwp5(&[])), json!([]));
    assert_eq!(summary(&hwp5(&["바탕"])), json!([["BOTH", "바탕"]]));
    assert_eq!(
        summary(&hwp5(&["양", "홀", "짝"])),
        json!([["BOTH", "양"], ["ODD", "홀"], ["EVEN", "짝"]])
    );
}

fn master_xml(id: &str, kind: &str, text: &str) -> String {
    format!(
        r#"<m:masterPage xmlns:m="urn:m" xmlns:p="urn:p" id="{id}" type="{kind}"><p:subList><p:p><p:run><p:t>{text}</p:t></p:run></p:p></p:subList></m:masterPage>"#
    )
}

#[test]
fn hwpx_section_reads_referenced_master_pages() {
    let files = [
        (
            "Contents/masterpage0.xml",
            master_xml("masterpage0", "BOTH", "양"),
        ),
        (
            "Contents/masterpage1.xml",
            master_xml("masterpage1", "EVEN", "짝"),
        ),
    ];
    let document = hwpx(
        r#"<p:masterPage idRef="masterpage1"/><p:masterPage idRef="masterpage0"/><p:masterPage idRef="none"/>"#,
        &files,
    );
    assert_eq!(summary(&document), json!([["EVEN", "짝"], ["BOTH", "양"]]));
    assert_eq!(
        summary(&hwpx("", &files)),
        json!([]),
        "가리키지 않은 바탕쪽은 쓰지 않는다"
    );
}
