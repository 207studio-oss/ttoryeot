#[path = "support/mod.rs"]
mod support;

use hwp_core::model::RunItem;
use hwp_core::{parse, Fatal, ParseOptions, WarningKind};

use support::odfcrypt::{self, Sealed};
use support::zipw::{self, Entry};

const PASSWORD: &str = "봄학기-2026!";
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="urn:hh" secCnt="1"><hh:charProperties><hh:charPr id="0" height="1000"/></hh:charProperties></hh:head>"#;

/// 압축해도 1KiB를 넘도록 문자를 섞은 본문 — 검사합이 앞 1KiB만 보는 경로를 탄다.
fn long_text() -> String {
    let mut state = 0x2545_f491_u32;
    (0..3000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            char::from_u32(0xac00 + state % 11172).expect("hangul syllable")
        })
        .collect()
}

fn section(text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hs="urn:hs" xmlns:hp="urn:hp"><hp:p paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p></hs:sec>"#
    )
}

fn encrypted_hwpx(text: &str, password: &str) -> Vec<u8> {
    let header = odfcrypt::seal(
        "Contents/header.xml",
        HEADER.as_bytes(),
        password,
        &[1; 16],
        &[2; 16],
    );
    let body = section(text);
    let section = odfcrypt::seal(
        "Contents/section0.xml",
        body.as_bytes(),
        password,
        &[3; 16],
        &[4; 16],
    );
    build(&header, &section)
}

fn build(header: &Sealed, section: &Sealed) -> Vec<u8> {
    let manifest = odfcrypt::manifest(&[header, section]);
    let content = br#"<?xml version="1.0" encoding="UTF-8"?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest><opf:item id="section0" href="Contents/section0.xml"/></opf:manifest></opf:package>"#;
    zipw::build(&[
        Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        Entry {
            name: "Contents/header.xml",
            data: &header.data,
            deflate: false,
        },
        Entry {
            name: "Contents/section0.xml",
            data: &section.data,
            deflate: false,
        },
        Entry {
            name: "Contents/content.hpf",
            data: content,
            deflate: true,
        },
        Entry {
            name: "META-INF/manifest.xml",
            data: manifest.as_bytes(),
            deflate: true,
        },
    ])
}

#[test]
fn password_opens_hwpx_without_a_padding_block() {
    let text = long_text();
    let (text, body) = (1..1000)
        .find_map(|len| {
            let text: String = text.chars().take(len).collect();
            let body = section(&text);
            support::cfbw::compress(body.as_bytes())
                .len()
                .is_multiple_of(16)
                .then_some((text, body))
        })
        .expect("aligned compressed XML");
    let header = odfcrypt::seal(
        "Contents/header.xml",
        HEADER.as_bytes(),
        PASSWORD,
        &[1; 16],
        &[2; 16],
    );
    let body = odfcrypt::seal_without_padding(
        "Contents/section0.xml",
        body.as_bytes(),
        PASSWORD,
        &[3; 16],
        &[4; 16],
    );
    let options = ParseOptions {
        password: Some(PASSWORD.to_string()),
        ..Default::default()
    };
    let parsed = parse(&build(&header, &body), &options).unwrap();
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        [RunItem::Text(text)]
    );
}

#[test]
fn password_opens_hwpx_with_hancom_plaintext_checksum() {
    let text = long_text();
    let header = odfcrypt::seal_hancom(
        "Contents/header.xml",
        HEADER.as_bytes(),
        PASSWORD,
        &[1; 16],
        &[2; 16],
    );
    let body = odfcrypt::seal_hancom(
        "Contents/section0.xml",
        section(&text).as_bytes(),
        PASSWORD,
        &[3; 16],
        &[4; 16],
    );
    let options = ParseOptions {
        password: Some(PASSWORD.to_string()),
        ..Default::default()
    };
    let parsed = parse(&build(&header, &body), &options).unwrap();
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        [RunItem::Text(text)]
    );
}

fn with_password(password: &str) -> ParseOptions {
    ParseOptions {
        password: Some(password.to_string()),
        ..ParseOptions::default()
    }
}

fn first_text(document: &hwp_core::model::Document) -> String {
    document.sections[0].paragraphs[0].runs[0]
        .items
        .iter()
        .filter_map(|item| match item {
            RunItem::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn password_opens_encrypted_hwpx() {
    let text = long_text();
    let bytes = encrypted_hwpx(&text, PASSWORD);
    let parsed = parse(&bytes, &with_password(PASSWORD)).expect("right password opens");
    assert_eq!(first_text(&parsed.document), text);
    assert_eq!(parsed.document.char_prs.len(), 1);
    assert!(parsed
        .warnings
        .iter()
        .all(|warning| warning.kind != WarningKind::SectionUnreadable));
}

#[test]
fn missing_password_is_password_protected() {
    let bytes = encrypted_hwpx("짧은 글", PASSWORD);
    assert_eq!(
        parse(&bytes, &ParseOptions::default()),
        Err(Fatal::PasswordProtected)
    );
}

#[test]
fn wrong_password_is_distinct_from_missing() {
    let bytes = encrypted_hwpx(&long_text(), PASSWORD);
    assert_eq!(
        parse(&bytes, &with_password("봄학기-2027!")),
        Err(Fatal::WrongPassword)
    );
    assert_eq!(parse(&bytes, &with_password("")), Err(Fatal::WrongPassword));
}

#[test]
fn one_corrupt_entry_warns_and_keeps_the_rest() {
    let header = odfcrypt::seal(
        "Contents/header.xml",
        HEADER.as_bytes(),
        PASSWORD,
        &[1; 16],
        &[2; 16],
    );
    let body = section("남는 본문");
    let mut section = odfcrypt::seal(
        "Contents/section0.xml",
        body.as_bytes(),
        PASSWORD,
        &[3; 16],
        &[4; 16],
    );
    section.data[0] ^= 0xff;
    let parsed = parse(&build(&header, &section), &with_password(PASSWORD));
    assert_eq!(parsed, Err(Fatal::NoReadableSection));

    let mut header = odfcrypt::seal(
        "Contents/header.xml",
        HEADER.as_bytes(),
        PASSWORD,
        &[1; 16],
        &[2; 16],
    );
    let section = odfcrypt::seal(
        "Contents/section0.xml",
        body.as_bytes(),
        PASSWORD,
        &[3; 16],
        &[4; 16],
    );
    header.data[0] ^= 0xff;
    let parsed =
        parse(&build(&header, &section), &with_password(PASSWORD)).expect("section still opens");
    assert_eq!(first_text(&parsed.document), "남는 본문");
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::SectionUnreadable
            && warning.detail.contains("header.xml")));
}

#[test]
fn unsupported_cipher_is_reported_with_or_without_a_password() {
    let manifest = br#"<?xml version="1.0" encoding="UTF-8"?><odf:manifest xmlns:odf="urn:odf"><odf:file-entry odf:full-path="Contents/section0.xml"><odf:encryption-data odf:checksum-type="SHA1/1K" odf:checksum="AAAA"><odf:algorithm odf:algorithm-name="Blowfish CFB" odf:initialisation-vector="AAAAAAAAAAA="/><odf:key-derivation odf:key-derivation-name="PBKDF2" odf:iteration-count="1024" odf:salt="AAAAAAAAAAAAAAAAAAAAAA=="/></odf:encryption-data></odf:file-entry></odf:manifest>"#;
    let bytes = zipw::build(&[
        Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        Entry {
            name: "Contents/section0.xml",
            data: b"sealed",
            deflate: false,
        },
        Entry {
            name: "META-INF/manifest.xml",
            data: manifest,
            deflate: true,
        },
    ]);
    assert!(matches!(
        parse(&bytes, &ParseOptions::default()),
        Err(Fatal::UnsupportedEncryptionDetail(detail))
            if detail.contains("HWPX manifest uses an unsupported encryption scheme")
    ));
    assert!(matches!(
        parse(&bytes, &with_password(PASSWORD)),
        Err(Fatal::UnsupportedEncryptionDetail(detail))
            if detail.contains("HWPX manifest uses an unsupported encryption scheme")
    ));
}

#[test]
fn password_is_not_printed_by_debug() {
    let options = with_password(PASSWORD);
    assert!(!format!("{options:?}").contains(PASSWORD));
}
