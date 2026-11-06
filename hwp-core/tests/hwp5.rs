#[path = "support/mod.rs"]
mod support;

use hwp_core::hwp5::record::parse_checked;
use hwp_core::{parse, Fatal, ParseOptions, WarningKind};

use support::{cfbw, rec};

fn file_header(flags: u32) -> Vec<u8> {
    let mut header = vec![0u8; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    header[36..40].copy_from_slice(&flags.to_le_bytes());
    header
}

fn hwp5(flags: u32, sections: &[u16], declared: u16) -> Vec<u8> {
    cfbw::build(&hwp5_streams(flags, sections, declared))
}

fn hwp5_streams(flags: u32, sections: &[u16], declared: u16) -> Vec<(String, Vec<u8>)> {
    let compressed = flags & 1 != 0;
    let mut doc_info = rec::document_properties(declared);
    doc_info.extend(rec::record(17, 0, &[]));
    let section_data = {
        let mut data = rec::paragraph_header();
        data.extend(rec::record(rec::T_PARA_TEXT, 1, b"x\0"));
        data
    };
    let mut streams = vec![("FileHeader".to_string(), file_header(flags))];
    streams.push((
        "DocInfo".to_string(),
        if compressed {
            cfbw::compress(&doc_info)
        } else {
            doc_info
        },
    ));
    for index in sections {
        let name = format!("BodyText/Section{index}");
        let data = if compressed {
            cfbw::compress(&section_data)
        } else {
            section_data.clone()
        };
        streams.push((name, data));
    }
    streams
}

#[test]
fn opens_minimal_hwp5() {
    let parsed = parse(&hwp5(1, &[0], 1), &ParseOptions::default()).expect("parse");
    assert_eq!(parsed.document.sections.len(), 1);
    assert!(parsed.document.wchar_positions);
    assert!(parsed.warnings.is_empty());
    let paragraph = &parsed.document.sections[0].paragraphs[0];
    assert_eq!(paragraph.para_pr_id, 0);
    assert_eq!(paragraph.runs.len(), 1);
    assert_eq!(
        paragraph.runs[0].items,
        vec![hwp_core::RunItem::Text("x".to_string())]
    );
}

#[test]
fn password_flag_is_fatal() {
    assert_eq!(
        parse(&hwp5(0x2, &[0], 1), &ParseOptions::default()),
        Err(Fatal::PasswordProtected)
    );
    let with_password = ParseOptions {
        password: Some("암호".to_string()),
        ..ParseOptions::default()
    };
    assert!(matches!(
        parse(&hwp5(0x2, &[0], 1), &with_password),
        Err(Fatal::UnsupportedEncryptionDetail(detail))
            if detail.contains("shorter than 48 bytes")
    ));
}

#[test]
fn distribution_without_viewtext_reads_body_text() {
    let parsed = parse(&hwp5(0x4, &[0], 1), &ParseOptions::default()).expect("parse");
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        vec![hwp_core::RunItem::Text("x".to_string())]
    );
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::SectionMissing));
    assert_eq!(
        parse(&hwp5(0x4, &[], 1), &ParseOptions::default()),
        Err(Fatal::NoReadableSection)
    );
}

#[test]
fn cutoff_zero_container_reads_like_the_standard_one() {
    for flags in [0, 1] {
        let streams = hwp5_streams(flags, &[0, 1], 2);
        let raw = cfbw::build_without_mini_stream(&streams);
        assert!(cfb::CompoundFile::open(std::io::Cursor::new(&raw)).is_err());
        assert_eq!(
            parse(&raw, &ParseOptions::default()),
            parse(&cfbw::build(&streams), &ParseOptions::default())
        );
    }
    let password = cfbw::build_without_mini_stream(&hwp5_streams(0x2, &[0], 1));
    assert_eq!(
        parse(&password, &ParseOptions::default()),
        Err(Fatal::PasswordProtected)
    );
}

#[test]
fn broken_cutoff_zero_container_is_not_hwp() {
    let raw = cfbw::build_without_mini_stream(&hwp5_streams(1, &[0], 1));
    // FAT이 맨 끝에 있으니 어디서 잘려도 FAT이 모자란다.
    for len in 0..raw.len() {
        assert_eq!(
            parse(&raw[..len], &ParseOptions::default()),
            Err(Fatal::NotHwp),
            "len {len}"
        );
    }
    // FAT이 디렉터리 섹터를 자기 자신에 잇는 고리.
    let mut cyclic = raw.clone();
    let directory = u32::from_le_bytes(cyclic[48..52].try_into().unwrap()) as usize;
    let fat = (u32::from_le_bytes(cyclic[76..80].try_into().unwrap()) as usize + 1) * 512;
    cyclic[fat + directory * 4..fat + directory * 4 + 4]
        .copy_from_slice(&(directory as u32).to_le_bytes());
    assert_eq!(parse(&cyclic, &ParseOptions::default()), Err(Fatal::NotHwp));

    // 모든 스트림이 자료 섹터 전체를 한 사슬로 함께 가리키면 스트림 합이 파일보다 크다.
    let mut shared = raw.clone();
    let data_sectors = directory;
    for id in 0..data_sectors {
        let next = if id + 1 == data_sectors {
            0xFFFF_FFFE
        } else {
            id as u32 + 1
        };
        shared[fat + id * 4..fat + id * 4 + 4].copy_from_slice(&next.to_le_bytes());
    }
    let entries = (directory + 1) * 512;
    for entry in shared[entries..fat].as_chunks_mut::<128>().0 {
        if entry[66] == 2 {
            entry[116..120].copy_from_slice(&0u32.to_le_bytes());
            entry[120..124].copy_from_slice(&((data_sectors * 512) as u32).to_le_bytes());
        }
    }
    assert_eq!(parse(&shared, &ParseOptions::default()), Err(Fatal::NotHwp));
}

#[test]
fn section_gap_warns_and_keeps_order() {
    let parsed = parse(&hwp5(0, &[0, 2], 2), &ParseOptions::default()).expect("parse");
    assert_eq!(parsed.document.sections.len(), 2);
    let missing: Vec<_> = parsed
        .warnings
        .iter()
        .filter(|warning| warning.kind == WarningKind::SectionMissing)
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].section, Some(1));
    assert!(!parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::CountMismatch));
}

#[test]
fn truncated_record_keeps_prefix() {
    let mut bytes = rec::record(66, 0, &[1, 2, 3]);
    bytes.extend(rec::header(67, 0, 10));
    bytes.extend([7, 8, 9]);
    let (records, warnings) = parse_checked(&bytes, Some(0));
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].tag, 67);
    assert_eq!(records[1].data, [7, 8, 9]);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, WarningKind::RecordTruncated);
}

#[test]
fn properties_count_mismatch_warns() {
    let parsed = parse(&hwp5(0, &[0], 2), &ParseOptions::default()).expect("parse");
    assert_eq!(parsed.document.sections.len(), 1);
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::CountMismatch));
}

#[test]
fn uncompressed_streams() {
    let parsed = parse(&hwp5(0, &[0, 1], 2), &ParseOptions::default()).expect("parse");
    assert_eq!(parsed.document.sections.len(), 2);
    assert!(!parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::SectionUnreadable));
}
