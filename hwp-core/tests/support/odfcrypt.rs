//! ODF 방식(PBKDF2-HMAC-SHA1 → AES-256-CBC, sha256-1k 검사합)으로 HWPX 항목을 봉인하는 시험용 도구.

use std::io::Write;

use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes::{Aes256, Block};
use flate2::write::DeflateEncoder;
use flate2::Compression;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

pub struct Sealed {
    pub data: Vec<u8>,
    pub manifest_entry: String,
}

pub fn seal(name: &str, plain: &[u8], password: &str, salt: &[u8; 16], iv: &[u8; 16]) -> Sealed {
    seal_profile(name, plain, password, salt, iv, Profile::Odf)
}

pub fn seal_without_padding(
    name: &str,
    plain: &[u8],
    password: &str,
    salt: &[u8; 16],
    iv: &[u8; 16],
) -> Sealed {
    seal_profile(name, plain, password, salt, iv, Profile::Unpadded)
}

pub fn seal_hancom(
    name: &str,
    plain: &[u8],
    password: &str,
    salt: &[u8; 16],
    iv: &[u8; 16],
) -> Sealed {
    seal_profile(name, plain, password, salt, iv, Profile::Hancom)
}

#[derive(Clone, Copy, PartialEq)]
enum Profile {
    Odf,
    Unpadded,
    Hancom,
}

fn seal_profile(
    name: &str,
    plain: &[u8],
    password: &str,
    salt: &[u8; 16],
    iv: &[u8; 16],
    profile: Profile,
) -> Sealed {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(plain).expect("deflate");
    let mut data = encoder.finish().expect("finish deflate");
    let checked = if profile == Profile::Hancom {
        plain
    } else {
        &data
    };
    let checksum = Sha256::digest(&checked[..checked.len().min(1024)]);
    if profile == Profile::Odf {
        let pad = 16 - data.len() % 16;
        data.extend(std::iter::repeat_n(pad as u8, pad));
    } else if profile == Profile::Hancom {
        data.resize(data.len().next_multiple_of(16), 0);
    }
    assert!(data.len().is_multiple_of(16));
    let key = pbkdf2_sha1(&Sha256::digest(password.as_bytes()), salt, 1024);
    let cipher = Aes256::new(&key.into());
    let mut previous = *iv;
    let (blocks, _) = data.as_chunks_mut::<16>();
    for chunk in blocks {
        for (byte, prev) in chunk.iter_mut().zip(previous.iter()) {
            *byte ^= prev;
        }
        let mut block = Block::from(*chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        previous = *chunk;
    }
    let manifest_entry = format!(
        r#"<odf:file-entry odf:full-path="{name}" odf:media-type="application/xml" odf:size="{size}"><odf:encryption-data odf:checksum-type="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0#sha256-1k" odf:checksum="{checksum}"><odf:algorithm odf:algorithm-name="http://www.w3.org/2001/04/xmlenc#aes256-cbc" odf:initialisation-vector="{iv}"/><odf:key-derivation odf:key-derivation-name="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0#pbkdf2" odf:key-size="32" odf:iteration-count="1024" odf:salt="{salt}"/><odf:start-key-generation odf:start-key-generation-name="http://www.w3.org/2000/09/xmldsig#sha256" odf:key-size="32"/></odf:encryption-data></odf:file-entry>"#,
        size = plain.len(),
        checksum = base64(&checksum),
        iv = base64(iv),
        salt = base64(salt),
    );
    Sealed {
        data,
        manifest_entry,
    }
}

pub fn manifest(entries: &[&Sealed]) -> String {
    let body: String = entries
        .iter()
        .map(|entry| entry.manifest_entry.as_str())
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><odf:manifest xmlns:odf="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">{body}</odf:manifest>"#
    )
}

fn pbkdf2_sha1(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    for (index, block) in key.chunks_mut(20).enumerate() {
        let mut mac = Hmac::<Sha1>::new_from_slice(password).expect("hmac key");
        mac.update(salt);
        mac.update(&(index as u32 + 1).to_be_bytes());
        let mut u = mac.finalize().into_bytes();
        let mut t = u;
        for _ in 1..iterations {
            let mut mac = Hmac::<Sha1>::new_from_slice(password).expect("hmac key");
            mac.update(&u);
            u = mac.finalize().into_bytes();
            for (t, u) in t.iter_mut().zip(u.iter()) {
                *t ^= u;
            }
        }
        block.copy_from_slice(&t[..block.len()]);
    }
    key
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let value = chunk.iter().enumerate().fold(0u32, |acc, (index, byte)| {
            acc | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(TABLE[(value >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
