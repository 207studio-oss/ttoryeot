#[path = "support/mod.rs"]
mod support;

use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Block};
use hwp_core::text::text_summary;
use hwp_core::{parse, Fatal, ParseOptions, WarningKind};

use support::{cfbw, rec};

const SEED: u32 = 0x1234_567f;
const KEY: [u8; 16] = [
    0x94, 0x93, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0x56, 0x57, 0x50, 0x51, 0x52, 0x53, 0x6c, 0x6d, 0x6e,
];

fn section(text: &str) -> Vec<u8> {
    let mut data = rec::paragraph_header();
    let text: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    data.extend(rec::record(rec::T_PARA_TEXT, 1, &text));
    let padding = (16 - (data.len() + 4) % 16) % 16;
    data.extend(rec::record(0, 0, &vec![0; padding]));
    data
}

fn distribution_header() -> [u8; 256] {
    let mut data = std::array::from_fn(|index| index as u8);
    data[..4].copy_from_slice(&SEED.to_le_bytes());
    data
}

fn view_text(text: &str, compressed: bool) -> Vec<u8> {
    let mut data = section(text);
    if compressed {
        data = cfbw::compress(&data);
        data.resize(data.len().div_ceil(16) * 16, 0);
    }
    let cipher = Aes128::new(&KEY.into());
    for chunk in data.as_chunks_mut::<16>().0 {
        let mut block = Block::from(*chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    let mut stream = rec::record(28, 0, &distribution_header());
    stream.extend(data);
    stream
}

fn document(flags: u32, sections: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[5, 0, 5, 0]);
    header[36..40].copy_from_slice(&flags.to_le_bytes());
    let mut doc_info = rec::document_properties(sections.len() as u16);
    if flags & 1 != 0 {
        doc_info = cfbw::compress(&doc_info);
    }
    let mut streams = vec![
        ("FileHeader".to_string(), header),
        ("DocInfo".to_string(), doc_info),
        ("BodyText/Section0".to_string(), section("unused body")),
    ];
    streams.extend(
        sections
            .iter()
            .map(|(index, data)| (format!("ViewText/Section{index}"), data.clone())),
    );
    cfbw::build(&streams)
}

#[test]
fn distribution_roundtrip_fixture() {
    for compressed in [false, true] {
        let bytes = document(
            4 | u32::from(compressed),
            &[(0, view_text("배포용 본문", compressed))],
        );
        let parsed = parse(&bytes, &ParseOptions::default()).expect("distribution parses");
        assert_eq!(text_summary(&parsed.document).paragraphs, ["배포용 본문"]);
        assert_eq!(parsed.warnings.len(), 1, "{:#?}", parsed.warnings);
        assert_eq!(parsed.warnings[0].kind, WarningKind::DistributionDecrypted);
        assert_eq!(parsed.warnings[0].section, Some(0));
    }
}

#[test]
fn ecb_tail_warns() {
    for compressed in [false, true] {
        for tail in 1..16 {
            let mut stream = view_text("kept prefix", compressed);
            stream.extend(vec![0xff; tail]);
            let bytes = document(4 | u32::from(compressed), &[(0, stream)]);
            let parsed = parse(&bytes, &ParseOptions::default()).expect("aligned prefix parses");
            assert_eq!(text_summary(&parsed.document).paragraphs, ["kept prefix"]);
            assert_eq!(parsed.warnings.len(), 2, "{:#?}", parsed.warnings);
            let warning = parsed
                .warnings
                .iter()
                .find(|warning| warning.kind == WarningKind::RecordTruncated)
                .expect("partial AES block warns");
            assert_eq!(warning.section, Some(0));
            assert!(warning.detail.contains(&format!("{tail} trailing byte(s)")));
            assert!(parsed
                .warnings
                .iter()
                .any(|warning| warning.kind == WarningKind::DistributionDecrypted));
        }
    }
}

#[test]
fn distribution_password_flag_remains_fatal() {
    for compressed in [false, true] {
        let bytes = document(
            6 | u32::from(compressed),
            &[(0, view_text("protected", compressed))],
        );
        assert_eq!(
            parse(&bytes, &ParseOptions::default()),
            Err(Fatal::PasswordProtected)
        );
    }
}

#[test]
fn distribution_section_gaps_use_viewtext() {
    let bytes = document(
        4,
        &[
            (2, view_text("second", false)),
            (0, view_text("first", false)),
        ],
    );
    let parsed = parse(&bytes, &ParseOptions::default()).expect("sections parse");
    assert_eq!(
        text_summary(&parsed.document).paragraphs,
        ["first", "second"]
    );
    let gaps: Vec<_> = parsed
        .warnings
        .iter()
        .filter(|warning| warning.kind == WarningKind::SectionMissing)
        .collect();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].section, Some(1));
    assert_eq!(gaps[0].detail, "ViewText/Section1..1 missing (1)");
}

#[test]
fn extended_distribution_header() {
    let stream = view_text("extended header", false);
    let mut extended = rec::header(28, 0, 0xfff);
    extended[4..8].copy_from_slice(&256u32.to_le_bytes());
    extended.extend_from_slice(&stream[4..]);
    let parsed = parse(&document(4, &[(0, extended)]), &ParseOptions::default())
        .expect("extended size parses");
    assert_eq!(
        text_summary(&parsed.document).paragraphs,
        ["extended header"]
    );
    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].kind, WarningKind::DistributionDecrypted);
}

#[test]
fn malformed_distribution_header_skips_only_its_section() {
    let mut short_data = rec::header(28, 0, 256);
    short_data.extend_from_slice(&[0; 255]);
    let mut tail_only = rec::record(28, 0, &distribution_header());
    tail_only.extend_from_slice(&[0xff; 15]);
    let malformed = [
        Vec::new(),
        vec![0; 3],
        rec::record(27, 0, &distribution_header()),
        rec::record(28, 0, &[0; 255]),
        rec::record(28, 0, &[0; 257]),
        short_data,
        rec::header(28, 0, 0xfff)[..4].to_vec(),
        rec::header(28, 0, u32::MAX as usize),
        rec::record(28, 0, &distribution_header()),
        tail_only,
    ];
    for broken in malformed {
        assert_eq!(
            parse(
                &document(4, &[(0, broken.clone())]),
                &ParseOptions::default()
            ),
            Err(Fatal::NoReadableSection)
        );
        let bytes = document(4, &[(0, broken), (1, view_text("readable", false))]);
        let parsed = parse(&bytes, &ParseOptions::default()).expect("next section survives");
        assert_eq!(text_summary(&parsed.document).paragraphs, ["readable"]);
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.kind == WarningKind::SectionUnreadable
                && warning.section == Some(0)));
        assert!(!parsed
            .warnings
            .iter()
            .any(|warning| warning.kind == WarningKind::DistributionDecrypted
                && warning.section == Some(0)));
    }
}
