#[path = "support/mod.rs"]
mod support;

use hwp_core::{parse, ParseOptions};

use support::{cfbw, rec, zipw};

const CONTENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:metadata><opf:title>합성 제목</opf:title><opf:meta name="creator" content="text">합성 작성자</opf:meta></opf:metadata><opf:manifest><opf:item id="section0" href="Contents/section0.xml"/></opf:manifest></opf:package>"#;
const VERSION: &str = r#"<?xml version="1.0" encoding="UTF-8"?><hv:HCFVersion xmlns:hv="urn:hv" major="5" minor="1" micro="1" buildNumber="0"/>"#;
const HEADER: &str =
    r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="urn:hh" secCnt="1"/>"#;
const SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hs="urn:hs" xmlns:hp="urn:hp"><hp:p><hp:run><hp:t>본문</hp:t></hp:run></hp:p></hs:sec>"#;

fn entry<'a>(name: &'a str, data: &'a str) -> zipw::Entry<'a> {
    zipw::Entry {
        name,
        data: data.as_bytes(),
        deflate: true,
    }
}

#[test]
fn hwpx_reports_title_author_and_version() {
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        entry("version.xml", VERSION),
        entry("Contents/header.xml", HEADER),
        entry("Contents/section0.xml", SECTION),
        entry("Contents/content.hpf", CONTENT),
    ]);
    let info = parse(&bytes, &ParseOptions::default()).unwrap().info;
    assert!(!info.encrypted);
    assert_eq!(info.metadata.title.as_deref(), Some("합성 제목"));
    assert_eq!(info.metadata.author.as_deref(), Some("합성 작성자"));
    assert_eq!(info.version.as_deref(), Some("5.1.1.0"));
}

fn hwp5(extra: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[4, 3, 2, 5]);
    let mut body = rec::paragraph_header();
    body.extend(rec::record(67, 1, b"x\0"));
    let mut streams = vec![
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), rec::document_properties(1)),
        ("BodyText/Section0".to_string(), body),
    ];
    streams.extend(extra);
    cfbw::build(&streams)
}

#[test]
fn hwp5_reports_its_version_and_survives_a_missing_summary() {
    let info = parse(&hwp5(Vec::new()), &ParseOptions::default())
        .unwrap()
        .info;
    assert_eq!(info.version.as_deref(), Some("5.2.3.4"));
    assert!(!info.encrypted);
    assert!(!info.distribution);
    assert_eq!(info.metadata.title, None);
}

#[test]
fn hwp5_ignores_a_corrupt_summary() {
    let info = parse(
        &hwp5(vec![(
            "\u{5}HwpSummaryInformation".to_string(),
            vec![0xFF; 7],
        )]),
        &ParseOptions::default(),
    )
    .unwrap()
    .info;
    assert_eq!(info.metadata.title, None);
}
