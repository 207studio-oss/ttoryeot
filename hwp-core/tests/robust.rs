#[path = "support/mod.rs"]
mod support;

use hwp_core::hwpx::zip::{MiniZip, ZipError};
use support::zipw;

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32"))
}

fn central_offsets(bytes: &[u8], count: usize) -> Vec<usize> {
    let eocd = bytes.len() - 22;
    assert_eq!(read_u32(bytes, eocd), 0x0605_4b50);
    let mut position = read_u32(bytes, eocd + 16) as usize;
    let mut offsets = Vec::new();
    for _ in 0..count {
        assert_eq!(read_u32(bytes, position), 0x0201_4b50);
        offsets.push(position);
        let name = usize::from(u16::from_le_bytes(
            bytes[position + 28..position + 30]
                .try_into()
                .expect("name"),
        ));
        let extra = usize::from(u16::from_le_bytes(
            bytes[position + 30..position + 32]
                .try_into()
                .expect("extra"),
        ));
        let comment = usize::from(u16::from_le_bytes(
            bytes[position + 32..position + 34]
                .try_into()
                .expect("comment"),
        ));
        position += 46 + name + extra + comment;
    }
    offsets
}

#[test]
fn zip_marks_the_second_entry_sharing_a_range_unreadable() {
    let mut bytes = zipw::build(&[
        zipw::Entry {
            name: "a.txt",
            data: b"first payload",
            deflate: false,
        },
        zipw::Entry {
            name: "b.txt",
            data: b"second payload",
            deflate: false,
        },
    ]);
    let entries = central_offsets(&bytes, 2);
    let first = read_u32(&bytes, entries[0] + 42);
    bytes[entries[1] + 42..entries[1] + 46].copy_from_slice(&first.to_le_bytes());
    let zip = MiniZip::new(&bytes).expect("archive still opens");
    assert_eq!(zip.read("a.txt").as_deref(), Some(&b"first payload"[..]));
    assert_eq!(zip.read("b.txt"), None);
    assert_eq!(zip.read_entry(&zip.entries()[1]), Err(ZipError::Malformed));
}

fn hwpx(extra: &[zipw::Entry<'_>]) -> Vec<u8> {
    let header = br#"<head secCnt="1"/>"#;
    let content = br#"<opf:package xmlns:opf="urn:opf"><opf:item id="a" href="BinData/a.png"/></opf:package>"#;
    let section = br#"<s:sec xmlns:s="urn:s" xmlns:p="urn:p" xmlns:hc="urn:hc"><p:p><p:run><p:t>text</p:t><p:pic><p:sz width="10" height="10"/><hc:img binaryItemIDRef="a"/></p:pic></p:run></p:p></s:sec>"#;
    let mut entries = vec![
        zipw::Entry {
            name: "Contents/header.xml",
            data: header,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/content.hpf",
            data: content,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: section,
            deflate: false,
        },
    ];
    entries.extend(extra.iter().map(|entry| zipw::Entry {
        name: entry.name,
        data: entry.data,
        deflate: entry.deflate,
    }));
    zipw::build(&entries)
}

#[test]
fn corrupted_local_header_on_unread_entry_keeps_the_document() {
    let clean_bytes = hwpx(&[
        zipw::Entry {
            name: "Preview/PrvImage.png",
            data: b"preview",
            deflate: false,
        },
        zipw::Entry {
            name: "BinData/a.png",
            data: b"picture",
            deflate: false,
        },
    ]);
    let clean = hwp_core::parse(&clean_bytes, &Default::default()).expect("clean document parses");
    let mut bytes = clean_bytes.clone();
    let offsets = central_offsets(&bytes, 5);
    let local = read_u32(&bytes, offsets[3] + 42) as usize;
    bytes[local..local + 4].copy_from_slice(&[0, 0, 0, 0]);
    let parsed = hwp_core::parse(&bytes, &Default::default()).expect("corrupt preview is ignored");
    assert_eq!(parsed.document, clean.document);
    assert_eq!(parsed.warnings.len(), clean.warnings.len());
}

#[test]
fn corrupted_bin_data_keeps_the_picture_and_warns() {
    let mut bytes = hwpx(&[zipw::Entry {
        name: "BinData/a.png",
        data: b"picture",
        deflate: false,
    }]);
    let offsets = central_offsets(&bytes, 4);
    let local = read_u32(&bytes, offsets[3] + 42) as usize;
    bytes[local..local + 4].copy_from_slice(&[0, 0, 0, 0]);
    let parsed = hwp_core::parse(&bytes, &Default::default()).expect("document still parses");
    let paragraph = &parsed.document.sections[0].paragraphs[0];
    assert!(paragraph.runs[0].items.iter().any(
        |item| matches!(item, hwp_core::RunItem::Picture(picture) if picture.binary_id == "a")
    ));
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::MissingBinData));
}

#[test]
fn zip_total_decode_budget_is_enforced() {
    let bytes = zipw::build(&[zipw::Entry {
        name: "a.txt",
        data: &[7u8; 100],
        deflate: false,
    }]);
    let zip = MiniZip::with_budget(&bytes, 50).expect("zip parses");
    let entry = &zip.entries()[0];
    assert_eq!(zip.read_entry(entry), Err(ZipError::TooLarge));
}

#[test]
fn zip_rejects_zero_size_deflate_entry() {
    let bytes = zipw::build(&[zipw::Entry {
        name: "empty.txt",
        data: b"",
        deflate: true,
    }]);
    let zip = MiniZip::new(&bytes).expect("zip parses");
    let entry = &zip.entries()[0];
    assert_eq!(zip.read_entry(entry), Err(ZipError::Malformed));
}

fn deep_hwpx(levels: usize) -> Vec<u8> {
    let mut inner = String::new();
    for _ in 0..levels {
        inner = format!("<p:container>{inner}</p:container>");
    }
    hwpx_section(&format!("<p:p><p:run>{inner}</p:run></p:p>"))
}

fn hwpx_section(body: &str) -> Vec<u8> {
    let xml =
        format!("<s:sec xmlns:s=\"urn:s\" xmlns:p=\"urn:p\" xmlns:hc=\"urn:hc\">{body}</s:sec>");
    zipw::build(&[
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
    ])
}

fn deep_hwpx_tables(levels: usize) -> Vec<u8> {
    let mut inner = "<p:t>leaf</p:t>".to_string();
    for _ in 0..levels {
        inner = format!(
            "<p:tbl rowCnt=\"1\" colCnt=\"1\"><p:tr><p:tc><p:cellAddr colAddr=\"0\" rowAddr=\"0\"/><p:cellSpan colSpan=\"1\" rowSpan=\"1\"/><p:cellSz width=\"100\" height=\"100\"/><p:subList><p:p><p:run>{inner}</p:run></p:p></p:subList></p:tc></p:tr></p:tbl>"
        );
    }
    hwpx_section(&format!("<p:p><p:run>{inner}</p:run></p:p>"))
}

fn deep_hwpx_text_boxes(levels: usize) -> Vec<u8> {
    let mut inner = "<p:t>leaf</p:t>".to_string();
    for _ in 0..levels {
        let mut containers = inner;
        for _ in 0..4 {
            containers = format!("<p:container>{containers}</p:container>");
        }
        inner = format!(
            "<p:rect><p:drawText><p:subList><p:p><p:run>{containers}</p:run></p:p></p:subList></p:drawText></p:rect>"
        );
    }
    hwpx_section(&format!(
        "<p:p><p:run><p:t>before</p:t></p:run></p:p><p:p><p:run>{inner}</p:run></p:p><p:p><p:run><p:t>after</p:t></p:run></p:p>"
    ))
}

fn parse_on_stack(bytes: Vec<u8>, stack: usize) -> hwp_core::Parsed {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || hwp_core::parse(&bytes, &hwp_core::ParseOptions::default()))
        .expect("spawn deep parse thread")
        .join()
        .expect("deep parse thread")
        .expect("deep document parses without a stack overflow")
}

#[test]
fn deep_hwpx_nesting_is_bounded_on_small_stack() {
    let parsed = parse_on_stack(deep_hwpx(40), 512 * 1024);
    assert!(!parsed.document.sections.is_empty());
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::NestingTooDeep));
}

#[test]
fn deep_hwpx_tables_are_bounded_on_small_stack() {
    let parsed = parse_on_stack(deep_hwpx_tables(40), 512 * 1024);
    assert!(hwp_core::text::text_summary(&parsed.document).tables > 0);
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::NestingTooDeep));
}

#[test]
fn deep_hwpx_text_boxes_are_bounded_on_small_stack() {
    let parsed = parse_on_stack(deep_hwpx_text_boxes(27), 512 * 1024);
    assert!(!parsed.document.sections.is_empty());
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::NestingTooDeep));
}

#[test]
fn hwpx_xml_depth_limit_preserves_surrounding_text() {
    let parsed = parse_on_stack(deep_hwpx_text_boxes(40), 512 * 1024);
    let paragraphs = hwp_core::text::text_summary(&parsed.document).paragraphs;
    assert_eq!(paragraphs.first().map(String::as_str), Some("before"));
    assert_eq!(paragraphs.last().map(String::as_str), Some("after"));
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::NestingTooDeep));
}

#[test]
fn hwpx_manifest_non_element_encryption_mentions_preserve_document() {
    let clean = hwp_core::parse(&hwpx(&[]), &Default::default()).expect("clean document parses");
    for depth in [1, 300] {
        for content in [
            "<!-- <m:encryption-data/> -->",
            "<extension value=\"encryption-data\"/>",
            "encryption-data",
            "<![CDATA[<m:encryption-data/>]]>",
            "<?extension encryption-data?>",
            "<not-encryption-data/>",
        ] {
            let manifest = format!(
                "<m:manifest xmlns:m=\"urn:manifest\">{}{content}{}</m:manifest>",
                "<extension>".repeat(depth),
                "</extension>".repeat(depth)
            );
            let bytes = hwpx(&[zipw::Entry {
                name: "META-INF/manifest.xml",
                data: manifest.as_bytes(),
                deflate: false,
            }]);
            let parsed = hwp_core::parse(&bytes, &Default::default())
                .unwrap_or_else(|error| panic!("depth {depth}, {content}: {error:?}"));
            assert_eq!(parsed, clean, "depth {depth}, {content}");
        }
    }
}

#[test]
fn hwpx_manifest_encryption_elements_are_detected_at_any_depth() {
    for depth in [0, 300] {
        for content in [
            "<encryption-data/>",
            "<encryption-data></encryption-data>",
            "<m:encryption-data xmlns:m=\"urn:manifest\"/>",
            "<m:encryption-data xmlns:m=\"urn:manifest\"></m:encryption-data>",
        ] {
            let manifest = format!(
                "{}{content}{}",
                "<extension>".repeat(depth),
                "</extension>".repeat(depth)
            );
            let bytes = hwpx(&[zipw::Entry {
                name: "META-INF/manifest.xml",
                data: manifest.as_bytes(),
                deflate: false,
            }]);
            assert!(
                matches!(
                    hwp_core::parse(&bytes, &Default::default()),
                    Err(hwp_core::Fatal::UnsupportedEncryptionDetail(detail))
                        if detail.contains("unsupported encryption scheme")
                ),
                "depth {depth}, {content}"
            );
        }
    }
}

fn hwp5_bytes(section: &[u8]) -> Vec<u8> {
    let mut header = vec![0u8; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    let mut doc_info = support::rec::document_properties(1);
    doc_info.extend(support::rec::record(17, 0, &[]));
    support::cfbw::build(&[
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), doc_info),
        ("BodyText/Section0".to_string(), section.to_vec()),
    ])
}

fn common(width: u32, height: u32) -> Vec<u8> {
    let mut data = vec![0u8; 36];
    data[12..16].copy_from_slice(&width.to_le_bytes());
    data[16..20].copy_from_slice(&height.to_le_bytes());
    data
}

fn table_data() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    for _ in 0..4 {
        data.extend_from_slice(&0i16.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data
}

fn cell_list_header() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0i16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&100u32.to_le_bytes());
    data.extend_from_slice(&100u32.to_le_bytes());
    for _ in 0..4 {
        data.extend_from_slice(&0i16.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes());
    data
}

fn table_control(level: u16, depth: usize) -> Vec<u8> {
    let mut paragraph = support::rec::record(66, level + 2, &[0u8; 22]);
    if depth > 1 {
        paragraph.extend(table_control(level + 3, depth - 1));
    } else {
        paragraph.extend(support::rec::record(67, level + 2, b"x\0"));
    }
    let mut list = support::rec::record(72, level + 1, &cell_list_header());
    list.extend(paragraph);
    let mut table = support::rec::record(77, level + 1, &table_data());
    table.extend(list);
    let mut control_data = b" lbt".to_vec();
    control_data.extend(common(100, 100));
    control_data.extend_from_slice(&0u32.to_le_bytes());
    let mut control = support::rec::record(71, level, &control_data);
    control.extend(table);
    control
}

fn gso_control(level: u16, component: Vec<u8>) -> Vec<u8> {
    let mut control_data = b" osg".to_vec();
    control_data.extend(common(100, 100));
    let mut control = support::rec::record(71, level, &control_data);
    control.extend(component);
    control
}

fn text_box_component(level: u16) -> Vec<u8> {
    let mut paragraph = support::rec::record(66, level + 2, &[0u8; 22]);
    paragraph.extend(support::rec::record(67, level + 2, b"x\0"));
    let mut list = support::rec::record(72, level + 1, &[0u8; 12]);
    list.extend(paragraph);
    let mut component = support::rec::record(76, level, &[0u8; 8]);
    component.extend(list);
    component
}

fn group_component(level: u16, depth: usize) -> Vec<u8> {
    let mut component = support::rec::record(76, level, &[0u8; 8]);
    if depth == 0 {
        component.extend(support::rec::record(79, level + 1, &[]));
    } else {
        component.extend(support::rec::record(86, level + 1, &[]));
        component.extend(group_component(level + 1, depth - 1));
    }
    component
}

#[test]
fn deep_hwp5_tables_are_bounded_on_small_stack() {
    let mut section = support::rec::record(66, 0, &[0u8; 22]);
    section.extend(table_control(1, 40));
    let parsed = parse_on_stack(hwp5_bytes(&section), 256 * 1024);
    assert!(hwp_core::text::text_summary(&parsed.document).tables > 0);
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == hwp_core::WarningKind::NestingTooDeep));
}

#[test]
fn deep_hwp5_text_boxes_and_groups_are_bounded_on_small_stack() {
    let mut section = Vec::new();
    for _ in 0..40 {
        section.extend(support::rec::record(66, 0, &[0u8; 22]));
        section.extend(gso_control(1, text_box_component(2)));
        section.extend(gso_control(1, group_component(2, 4)));
    }
    let parsed = parse_on_stack(hwp5_bytes(&section), 256 * 1024);
    assert!(!parsed.document.sections.is_empty());
}

#[test]
fn huge_section_index_reports_one_gap() {
    let xml = br#"<s:sec xmlns:s="urn:s"/>"#;
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "Contents/header.xml",
            data: br#"<head secCnt="2"/>"#,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: xml,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section4294967294.xml",
            data: xml,
            deflate: false,
        },
    ]);
    let parsed = hwp_core::parse(&bytes, &Default::default()).expect("gap document parses");
    let missing: Vec<_> = parsed
        .warnings
        .iter()
        .filter(|warning| warning.kind == hwp_core::WarningKind::SectionMissing)
        .collect();
    assert_eq!(missing.len(), 1);
    assert!(parsed.warnings.len() <= 2, "{:?}", parsed.warnings);
    assert!(missing[0].detail.contains("4294967293"));
}

#[test]
fn long_lone_surrogate_run_has_bounded_warnings() {
    let mut text = Vec::new();
    for _ in 0..10_000 {
        text.extend_from_slice(&0xD800u16.to_le_bytes());
    }
    let mut section = support::rec::paragraph_header();
    section.extend(support::rec::record(67, 1, &text));
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    let bytes = support::cfbw::build(&[
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), support::rec::document_properties(1)),
        ("BodyText/Section0".to_string(), section),
    ]);
    let parsed = hwp_core::parse(&bytes, &Default::default()).expect("surrogate document parses");
    let warnings: Vec<_> = parsed
        .warnings
        .iter()
        .filter(|warning| warning.kind == hwp_core::WarningKind::LoneSurrogate)
        .collect();
    assert!(!warnings.is_empty());
    assert!(warnings.len() <= 102, "{} warnings", warnings.len());
}

#[test]
fn invalid_section_xml_drops_warnings_from_paragraphs_read_before_the_error() {
    let mut broken_body = String::new();
    for _ in 0..40 {
        broken_body = format!("<p:container>{broken_body}</p:container>");
    }
    let broken = format!(
        "<s:sec xmlns:s=\"urn:s\" xmlns:p=\"urn:p\"><p:p><p:run>{broken_body}</p:run></p:p><p:p>"
    );
    let bytes = zipw::build(&[
        zipw::Entry {
            name: "Contents/header.xml",
            data: br#"<head secCnt="2"/>"#,
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: broken.as_bytes(),
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section1.xml",
            data: b"<sec><p><run><t>ok</t></run></p></sec>",
            deflate: false,
        },
    ]);
    let parsed = hwp_core::parse(&bytes, &hwp_core::ParseOptions::default()).expect("parses");
    assert_eq!(parsed.document.sections.len(), 1);
    let kinds: Vec<_> = parsed.warnings.iter().map(|warning| warning.kind).collect();
    assert!(kinds.contains(&hwp_core::WarningKind::InvalidXml));
    assert!(!kinds.contains(&hwp_core::WarningKind::NestingTooDeep));
}

#[test]
fn xml_depth_limit_replaces_the_nesting_warning_and_comes_first() {
    let mut deep = String::new();
    for _ in 0..300 {
        deep = format!("<p:container>{deep}</p:container>");
    }
    let bytes = hwpx_section(&format!("<p:p><p:run>{deep}</p:run></p:p>"));
    let parsed = hwp_core::parse(&bytes, &hwp_core::ParseOptions::default()).expect("parses");
    let nesting: Vec<_> = parsed
        .warnings
        .iter()
        .filter(|warning| warning.kind == hwp_core::WarningKind::NestingTooDeep)
        .collect();
    assert_eq!(nesting.len(), 1);
    assert!(
        nesting[0].detail.starts_with("XML depth limit"),
        "{}",
        nesting[0].detail
    );
}

#[test]
fn track_change_marker_directly_under_the_section_root_is_seen() {
    let bytes = hwpx_section("<p:deleteBegin/><p:p/>");
    let parsed = hwp_core::parse(&bytes, &hwp_core::ParseOptions::default()).expect("parses");
    assert!(parsed.document.track_changes);
}
