#[path = "support/mod.rs"]
mod support;

use std::io::Read;

use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Block};
use des::cipher::Block as DesBlock;
use des::Des;
use hwp_core::{parse, Fatal, ParseOptions, RunItem, WarningKind};
use sha1::{Digest, Sha1};
use support::{cfbw, rec};

fn password() -> String {
    format!("시험-{:?}", std::time::SystemTime::now())
}

fn encrypt(bytes: &[u8], password: &str) -> Vec<u8> {
    let mut interleaved = Vec::new();
    let mut previous = 236u8;
    for byte in password.bytes() {
        interleaved.extend([previous.rotate_left(1), byte]);
        previous = byte;
    }
    let key = Sha1::digest(&interleaved);
    let cipher = Aes128::new_from_slice(&key[..16]).unwrap();
    let mut register = [0u8; 16];
    bytes
        .iter()
        .map(|byte| {
            let mut output = 0;
            for bit in (0..8).rev() {
                let mut block = Block::from(register);
                cipher.encrypt_block(&mut block);
                let encrypted = ((byte >> bit) ^ (block[0] >> 7)) & 1;
                output |= encrypted << bit;
                for i in 0..15 {
                    register[i] = (register[i] << 1) | (register[i + 1] >> 7);
                }
                register[15] = (register[15] << 1) | encrypted;
            }
            output
        })
        .collect()
}

fn des_key(password: &str) -> [u8; 8] {
    let mut rolling = [0u8; 7];
    let mut position = 0;
    for unit in password.encode_utf16() {
        for byte in unit.to_le_bytes() {
            rolling[position] = (rolling[position] ^ byte).rotate_left(1);
            position = if position == 6 { 0 } else { position + 1 };
        }
    }
    let mut key = [0u8; 8];
    for bit in 0..56 {
        let source = (rolling[bit / 8] >> (7 - bit % 8)) & 1;
        key[bit / 7] |= source << (7 - bit % 7);
    }
    key
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Trailer {
    Valid,
    WrongCrc,
    WrongLength,
}

/// EncryptVersion 0 compact trailer: CRC32 of the plaintext, its u32 length, zero padding.
fn compact_trailer(consumed: usize, plain: &[u8], align: usize, trailer: Trailer) -> Vec<u8> {
    let mut bytes = crc32fast::hash(plain).to_le_bytes().to_vec();
    bytes.extend_from_slice(&(plain.len() as u32).to_le_bytes());
    if trailer == Trailer::WrongCrc {
        bytes[0] ^= 1;
    }
    if trailer == Trailer::WrongLength {
        bytes[4] = bytes[4].wrapping_add(1);
    }
    let end = consumed + bytes.len();
    bytes.resize(bytes.len() + (align - end % align) % align, 0);
    bytes
}

/// EncryptVersion 0 DES stream: optional inner compact trailer, outer footer (marker + CRC32).
fn encrypt_v0(plain: &[u8], password: &str, compressed: bool, trailer: Trailer) -> Vec<u8> {
    let mut payload = if compressed {
        cfbw::compress(plain)
    } else {
        plain.to_vec()
    };
    if compressed {
        let inner = compact_trailer(payload.len(), plain, 8, trailer);
        payload.extend_from_slice(&inner);
    }
    let cipher = Des::new_from_slice(&des_key(password)).expect("DES key");
    payload.resize(payload.len().div_ceil(8) * 8, 0);
    let checksum = crc32fast::hash(&payload);
    for chunk in payload.as_chunks_mut::<8>().0 {
        let mut block = DesBlock::<Des>::from(*chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    payload.extend_from_slice(&[0x38, 0xa4, 0xce, 0x59]);
    payload.extend_from_slice(&checksum.to_le_bytes());
    payload
}

/// EncryptVersion 4 AES stream: optional inner trailer, 16-byte alignment, outer footer.
fn encrypt_v4(plain: &[u8], password: &str, compressed: bool) -> Vec<u8> {
    let mut payload = if compressed {
        cfbw::compress(plain)
    } else {
        plain.to_vec()
    };
    if compressed {
        let inner = compact_trailer(payload.len(), plain, 16, Trailer::Valid);
        payload.extend_from_slice(&inner);
    }
    payload.resize(payload.len().div_ceil(16) * 16, 0);
    let checksum = crc32fast::hash(&payload);
    let mut sealed = encrypt(&payload, password);
    sealed.extend_from_slice(&[0x38, 0xa4, 0xce, 0x59]);
    sealed.extend_from_slice(&checksum.to_le_bytes());
    sealed
}

fn body_stream(text: &str) -> Vec<u8> {
    let mut body = rec::paragraph_header();
    let mut encoded = Vec::new();
    for unit in text.encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    body.extend(rec::record(rec::T_PARA_TEXT, 1, &encoded));
    body
}

fn hwp_string(value: &str) -> Vec<u8> {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut bytes = (units.len() as u16).to_le_bytes().to_vec();
    for unit in units {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn v0_streams_with_bins(
    password: &str,
    compressed: bool,
    texts: &[&str],
    bins: &[(u16, &str)],
    trailer: Trailer,
) -> Vec<(String, Vec<u8>)> {
    let mut header = vec![0u8; 256];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[0, 0, 1, 5]);
    header[36..40].copy_from_slice(&(2u32 | u32::from(compressed)).to_le_bytes());
    header[44..48].copy_from_slice(&0u32.to_le_bytes());
    // Protected DocInfo starts with a 256-byte prefix inside the first record.
    let mut properties = vec![0x2f; 256];
    properties.extend_from_slice(&(texts.len() as u16).to_le_bytes());
    properties.resize(282, 0);
    let mut info = rec::record(rec::T_DOCUMENT_PROPERTIES, 0, &properties);
    info.extend(rec::record(17, 0, &[]));
    for (id, extension) in bins {
        let mut body = (1u16 | (1 << 4)).to_le_bytes().to_vec();
        body.extend(id.to_le_bytes());
        body.extend(hwp_string(extension));
        info.extend(rec::record(18, 0, &body));
    }
    let mut streams = vec![
        ("FileHeader".into(), header),
        (
            "DocInfo".into(),
            encrypt_v0(&info, password, compressed, trailer),
        ),
    ];
    for (index, text) in texts.iter().enumerate() {
        streams.push((
            format!("BodyText/Section{index}"),
            encrypt_v0(&body_stream(text), password, compressed, trailer),
        ));
    }
    for (id, extension) in bins {
        streams.push((
            format!("BinData/BIN{id:04X}.{extension}"),
            encrypt_v0(b"bin-data", password, true, trailer),
        ));
    }
    streams
}

fn v0_streams(
    password: &str,
    compressed: bool,
    texts: &[&str],
    trailer: Trailer,
) -> Vec<(String, Vec<u8>)> {
    v0_streams_with_bins(password, compressed, texts, &[], trailer)
}

fn fixture_v0(password: &str, compressed: bool) -> Vec<u8> {
    cfbw::build(&v0_streams(password, compressed, &["ABC"], Trailer::Valid))
}

fn v4_streams(password: &str, compressed: bool, version: u32) -> Vec<(String, Vec<u8>)> {
    let mut header = vec![0u8; 256];
    header[..17].copy_from_slice(b"HWP Document File");
    header[32..36].copy_from_slice(&[0, 0, 1, 5]);
    header[36..40].copy_from_slice(&(2u32 | u32::from(compressed)).to_le_bytes());
    header[44..48].copy_from_slice(&version.to_le_bytes());
    let mut info = rec::document_properties(1);
    info.extend(rec::record(17, 0, &[]));
    vec![
        ("FileHeader".into(), header),
        ("DocInfo".into(), encrypt_v4(&info, password, compressed)),
        (
            "BodyText/Section0".into(),
            encrypt_v4(&body_stream("ABC"), password, compressed),
        ),
    ]
}

fn fixture(password: &str, compressed: bool, version: u32) -> Vec<u8> {
    cfbw::build(&v4_streams(password, compressed, version))
}

fn with_password(password: &str) -> ParseOptions {
    ParseOptions {
        password: Some(password.to_string()),
        ..Default::default()
    }
}

fn has_text(document: &hwp_core::model::Document, wanted: &str) -> bool {
    document.sections.iter().any(|section| {
        section.paragraphs.iter().any(|paragraph| {
            paragraph.runs.iter().any(|run| {
                run.items
                    .iter()
                    .any(|item| matches!(item, RunItem::Text(text) if text == wanted))
            })
        })
    })
}

#[test]
fn opens_encrypt_version_0_des_documents_with_the_supplied_password() {
    let password = password();
    for compressed in [false, true] {
        let options = with_password(&password);
        let parsed = parse(&fixture_v0(&password, compressed), &options).expect("v0 opens");
        assert_eq!(
            parsed.document.sections[0].paragraphs[0].runs[0].items,
            [RunItem::Text("ABC".into())]
        );
        assert!(!parsed
            .warnings
            .iter()
            .any(|warning| warning.kind == WarningKind::CountMismatch));
        assert!(!parsed
            .warnings
            .iter()
            .any(|warning| warning.kind == WarningKind::StreamChecksumMismatch));
    }
}

#[test]
fn committed_encrypt_version_0_fixture_opens() {
    let bytes = include_bytes!("fixtures/synthetic-password-v0.hwp");
    let options = ParseOptions {
        password: Some("12345".into()),
        ..Default::default()
    };
    let parsed = parse(bytes, &options).expect("committed v0 fixture opens");
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        [RunItem::Text("EncryptVersion 0 synthetic HWP5".into())]
    );
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
}

#[test]
fn committed_encrypt_version_0_fixture_carries_the_compact_trailer() {
    let bytes = include_bytes!("fixtures/synthetic-password-v0.hwp");
    let mut file = cfb::CompoundFile::open(std::io::Cursor::new(bytes.as_slice()))
        .expect("committed fixture is a CFB");
    let mut stream = file.open_stream("/DocInfo").expect("DocInfo stream");
    let mut sealed = Vec::new();
    stream.read_to_end(&mut sealed).expect("read DocInfo");
    assert_eq!(
        &sealed[sealed.len() - 8..sealed.len() - 4],
        &[0x38, 0xa4, 0xce, 0x59]
    );
    let payload_len = sealed.len() - 8;
    let padding = (8 - payload_len % 8) % 8;
    assert_eq!(padding, 0, "trailer alignment belongs inside the payload");

    let mut payload = sealed[..payload_len].to_vec();
    let cipher = Des::new_from_slice(&des_key("12345")).expect("DES key");
    for chunk in payload.as_chunks_mut::<8>().0 {
        let mut block = DesBlock::<Des>::from(*chunk);
        cipher.decrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    let mut decoder = flate2::read::DeflateDecoder::new(&payload[..]);
    let mut plain = Vec::new();
    decoder.read_to_end(&mut plain).expect("DocInfo deflate");
    let consumed = decoder.total_in() as usize;
    let end = consumed + 8;
    let alignment = (8 - end % 8) % 8;
    assert_eq!(payload.len(), end + alignment);
    assert!(payload[end..].iter().all(|byte| *byte == 0));
    assert_eq!(
        u32::from_le_bytes(payload[consumed..consumed + 4].try_into().unwrap()),
        crc32fast::hash(&plain)
    );
    assert_eq!(
        u32::from_le_bytes(payload[consumed + 4..end].try_into().unwrap()),
        u32::try_from(plain.len()).unwrap()
    );
}

#[test]
fn encrypt_version_0_keeps_missing_and_wrong_password_distinct() {
    let password = password();
    for compressed in [false, true] {
        let bytes = fixture_v0(&password, compressed);
        assert_eq!(
            parse(&bytes, &Default::default()),
            Err(Fatal::PasswordProtected)
        );
        let options = with_password(&format!("{password}-wrong"));
        assert_eq!(parse(&bytes, &options), Err(Fatal::WrongPassword));
    }
}

#[test]
fn opens_compressed_and_uncompressed_documents_with_the_supplied_password() {
    let password = password();
    for compressed in [false, true] {
        let options = with_password(&password);
        let parsed = parse(&fixture(&password, compressed, 4), &options).unwrap();
        assert_eq!(
            parsed.document.sections[0].paragraphs[0].runs[0].items,
            [RunItem::Text("ABC".into())]
        );
    }
}

#[test]
fn missing_and_wrong_passwords_have_distinct_errors() {
    let password = password();
    for compressed in [false, true] {
        let bytes = fixture(&password, compressed, 4);
        assert_eq!(
            parse(&bytes, &Default::default()),
            Err(Fatal::PasswordProtected)
        );
        let options = with_password(&format!("{password}-wrong"));
        assert_eq!(parse(&bytes, &options), Err(Fatal::WrongPassword));
    }
}

#[test]
fn unsupported_version_is_rejected_without_a_password() {
    let password = password();
    for options in [ParseOptions::default(), with_password(&password)] {
        assert!(matches!(
            parse(&fixture(&password, true, 3), &options),
            Err(Fatal::UnsupportedEncryptionDetail(detail))
                if detail.contains("EncryptVersion 3")
                    && detail.contains("supported: 0 DES, 4 AES")
        ));
    }
}

#[test]
fn corrupt_footer_crc_is_a_warning_not_a_wrong_password() {
    let password = password();
    let options = with_password(&password);
    let mut streams = v0_streams(&password, true, &["ABC", "DEF"], Trailer::Valid);
    let body = streams
        .iter_mut()
        .find(|(name, _)| name == "BodyText/Section0")
        .expect("section 0");
    let last = body.1.len() - 1;
    body.1[last] ^= 0xff;
    let parsed = parse(&cfbw::build(&streams), &options).expect("intact payload still opens");
    assert_eq!(parsed.document.sections.len(), 2);
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch
            && warning.section == Some(0)
            && warning.detail.contains("CRC32")
    }));
    assert_eq!(
        parsed.document.sections[1].paragraphs[0].runs[0].items,
        [RunItem::Text("DEF".into())]
    );
}

#[test]
fn corrupt_ciphertext_is_a_warning_not_a_wrong_password() {
    let password = password();
    let options = with_password(&password);
    let mut streams = v0_streams(&password, true, &["ABC", "DEF"], Trailer::Valid);
    let body = streams
        .iter_mut()
        .find(|(name, _)| name == "BodyText/Section0")
        .expect("section 0");
    body.1[0] ^= 0xff;
    let parsed = parse(&cfbw::build(&streams), &options).expect("section 1 still opens");
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch && warning.section == Some(0)
    }));
    assert!(has_text(&parsed.document, "DEF"), "{:?}", parsed.document);
}

#[test]
fn corrupt_encrypted_bin_data_warns_and_keeps_the_document() {
    let password = password();
    let options = with_password(&password);
    let mut streams =
        v0_streams_with_bins(&password, true, &["ABC"], &[(1, "bin")], Trailer::Valid);
    let bin = streams
        .iter_mut()
        .find(|(name, _)| name == "BinData/BIN0001.bin")
        .expect("BinData stream");
    let last = bin.1.len() - 1;
    bin.1[last] ^= 0xff;
    let parsed = parse(&cfbw::build(&streams), &options).expect("document still opens");
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch
            && warning.detail.contains("BinData/BIN0001.bin")
    }));
    assert_eq!(parsed.document.bins.len(), 1);
    assert_eq!(parsed.document.bins[0].data.as_slice(), b"bin-data");
    assert!(has_text(&parsed.document, "ABC"));
}

#[test]
fn truncated_des_section_is_corruption_not_unsupported_encryption() {
    let password = password();
    let options = with_password(&password);
    let mut streams = v0_streams(&password, true, &["ABC"], Trailer::Valid);
    let body = streams
        .iter_mut()
        .find(|(name, _)| name == "BodyText/Section0")
        .expect("section 0");
    body.1.pop();
    assert!(matches!(
        parse(&cfbw::build(&streams), &options),
        Err(Fatal::CorruptDocument(detail))
            if detail.contains("integrity check")
    ));
}

#[test]
fn truncated_des_section_keeps_intact_sections() {
    let password = password();
    let options = with_password(&password);
    let mut streams = v0_streams(&password, true, &["ABC", "DEF"], Trailer::Valid);
    let body = streams
        .iter_mut()
        .find(|(name, _)| name == "BodyText/Section0")
        .expect("section 0");
    body.1.pop();
    let parsed = parse(&cfbw::build(&streams), &options).expect("section 1 still opens");
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        [RunItem::Text("DEF".into())]
    );
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch
            && warning.section == Some(0)
            && warning.detail.contains("not a whole 8-byte cipher block")
    }));
}

#[test]
fn truncated_des_doc_info_is_corruption() {
    let password = password();
    let mut streams = v0_streams(&password, true, &["ABC"], Trailer::Valid);
    let info = streams
        .iter_mut()
        .find(|(name, _)| name == "DocInfo")
        .expect("DocInfo");
    info.1.pop();
    assert!(matches!(
        parse(&cfbw::build(&streams), &with_password(&password)),
        Err(Fatal::CorruptDocument(detail)) if detail.contains("DocInfo")
    ));
}

#[test]
fn legacy_compact_trailer_is_checked() {
    let password = password();
    let options = with_password(&password);
    let streams = v0_streams(&password, true, &["ABC"], Trailer::WrongCrc);
    let parsed = parse(&cfbw::build(&streams), &options).expect("deflate payload still parses");
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch
            && warning.detail.contains("legacy compact trailer CRC32")
    }));
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        [RunItem::Text("ABC".into())]
    );
}

#[test]
fn legacy_compact_trailer_length_is_checked() {
    let password = password();
    let options = with_password(&password);
    let streams = v0_streams(&password, true, &["ABC"], Trailer::WrongLength);
    let parsed = parse(&cfbw::build(&streams), &options).expect("deflate payload still parses");
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch
            && warning.detail.contains("legacy compact trailer length")
    }));
}

#[test]
fn encrypt_version_4_outer_footer_is_verified() {
    let password = password();
    let options = with_password(&password);
    let mut streams = v4_streams(&password, true, 4);
    let body = streams
        .iter_mut()
        .find(|(name, _)| name == "BodyText/Section0")
        .expect("section 0");
    let last = body.1.len() - 1;
    body.1[last] ^= 0xff;
    let parsed = parse(&cfbw::build(&streams), &options).expect("intact payload still opens");
    assert!(parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::StreamChecksumMismatch
            && warning.detail.contains("footer CRC32")
    }));
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        [RunItem::Text("ABC".into())]
    );
}

#[test]
fn encrypt_version_4_corrupt_ciphertext_is_not_a_wrong_password() {
    let password = password();
    let options = with_password(&password);
    let mut streams = v4_streams(&password, true, 4);
    let body = streams
        .iter_mut()
        .find(|(name, _)| name == "BodyText/Section0")
        .expect("section 0");
    body.1[0] ^= 0xff;
    let result = parse(&cfbw::build(&streams), &options);
    match result {
        Ok(parsed) => assert!(parsed.warnings.iter().any(|warning| {
            warning.kind == WarningKind::StreamChecksumMismatch && warning.section == Some(0)
        })),
        Err(Fatal::CorruptDocument(_)) => {}
        other => panic!("corruption must not be a password error: {other:?}"),
    }
}
