//! 암호 HWPX: `META-INF/manifest.xml`의 ODF `encryption-data`(PBKDF2-HMAC-SHA1 → AES-256-CBC → raw deflate)를 푼다.

use std::collections::BTreeMap;
use std::io::Read;

use aes::cipher::{BlockCipherDecrypt, KeyInit};
use aes::{Aes256, Block};
use hmac::{Hmac, Mac};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::zip::MiniZip;
use crate::report::{Fatal, Warning, WarningKind};

const MANIFEST_PATH: &str = "META-INF/manifest.xml";
const AES256_CBC: &str = "http://www.w3.org/2001/04/xmlenc#aes256-cbc";
const PBKDF2: &str = "#pbkdf2";
const SHA256_START_KEY: &str = "http://www.w3.org/2000/09/xmldsig#sha256";
const SHA256_1K: &str = "#sha256-1k";
const SHA1_1K: &str = "#sha1-1k";
const CHECKSUM_SPAN: usize = 1024;
const MAX_PLAIN_SIZE: u64 = 512 * 1024 * 1024;
const MAX_ITERATIONS: u32 = 10_000_000;

#[derive(Debug, Default, Clone)]
struct Sealed {
    algorithm: String,
    iv: Vec<u8>,
    derivation: String,
    salt: Vec<u8>,
    iterations: u32,
    key_size: usize,
    start_key: String,
    checksum_type: String,
    checksum: Vec<u8>,
    size: Option<u64>,
}

/// 암호가 걸린 꾸러미 항목을 이름별로 푼다. 값이 `None`이면 그 항목은 풀지 못했다. 암호가 없는 꾸러미는 빈 표다.
pub(super) fn unlock(
    zip: &MiniZip<'_>,
    password: Option<&str>,
    warnings: &mut Vec<Warning>,
) -> Result<BTreeMap<String, Option<Vec<u8>>>, Fatal> {
    let manifest = sealed_entries(zip);
    if !manifest.encrypted {
        return Ok(BTreeMap::new());
    }
    // The manifest is plaintext, so an unsupported scheme can be named before a password is
    // requested, matching the HWP5 order.
    if manifest.unnamed || manifest.entries.values().any(|entry| !supported(entry)) {
        return Err(Fatal::UnsupportedEncryptionDetail(
            "HWPX manifest uses an unsupported encryption scheme".to_string(),
        ));
    }
    let Some(password) = password else {
        return Err(Fatal::PasswordProtected);
    };
    let start_key = Sha256::digest(password.as_bytes());
    let mut keys: Vec<(&[u8], u32, [u8; 32])> = Vec::new();
    let mut opened = BTreeMap::new();
    let mut failures = Vec::new();
    let mut verified = false;
    for (name, entry) in &manifest.entries {
        let key = match keys
            .iter()
            .find(|(salt, iterations, _)| *salt == entry.salt && *iterations == entry.iterations)
        {
            Some((_, _, key)) => *key,
            None => {
                let key = pbkdf2_sha1(&start_key, &entry.salt, entry.iterations);
                keys.push((&entry.salt, entry.iterations, key));
                key
            }
        };
        let result = match zip.read(name) {
            None => Err("is unreadable"),
            Some(data) => match aes256_cbc_decrypt(&key, &entry.iv, &data) {
                None => Err("is not a whole AES-CBC stream"),
                Some(decrypted) => {
                    let compressed = checked_compressed(entry, &decrypted);
                    if compressed.is_some() {
                        verified = true;
                    }
                    match inflate(compressed.unwrap_or(&decrypted), entry.size) {
                        Some(plain)
                            if compressed.is_some()
                                || checksum(entry, &plain) == entry.checksum =>
                        {
                            verified = true;
                            Ok(plain)
                        }
                        Some(_) => Err("checksum does not match the password"),
                        None => Err("does not inflate after decryption"),
                    }
                }
            },
        };
        if let Err(reason) = &result {
            failures.push(format!("{name} {reason}"));
        }
        opened.insert(name.clone(), result.ok());
    }
    if !verified {
        return Err(Fatal::WrongPassword);
    }
    for detail in failures {
        warnings.push(Warning::new(WarningKind::SectionUnreadable, None, detail));
    }
    Ok(opened)
}

fn supported(entry: &Sealed) -> bool {
    entry.algorithm == AES256_CBC
        && entry.iv.len() == 16
        && entry.derivation.ends_with(PBKDF2)
        && entry.key_size == 32
        && (1..=MAX_ITERATIONS).contains(&entry.iterations)
        && entry.start_key == SHA256_START_KEY
        && (entry.checksum_type.ends_with(SHA256_1K) || entry.checksum_type.ends_with(SHA1_1K))
}

fn checksum(entry: &Sealed, decrypted: &[u8]) -> Vec<u8> {
    let span = &decrypted[..decrypted.len().min(CHECKSUM_SPAN)];
    if entry.checksum_type.ends_with(SHA1_1K) {
        Sha1::digest(span).to_vec()
    } else {
        Sha256::digest(span).to_vec()
    }
}

fn checked_compressed<'a>(entry: &Sealed, decrypted: &'a [u8]) -> Option<&'a [u8]> {
    if checksum(entry, decrypted) == entry.checksum {
        return Some(decrypted);
    }
    let pad = usize::from(*decrypted.last()?);
    if !(1..=16).contains(&pad) || pad > decrypted.len() {
        return None;
    }
    let unpadded = &decrypted[..decrypted.len() - pad];
    (checksum(entry, unpadded) == entry.checksum).then_some(unpadded)
}

#[derive(Default)]
struct Manifest {
    entries: BTreeMap<String, Sealed>,
    encrypted: bool,
    unnamed: bool,
}

fn sealed_entries(zip: &MiniZip<'_>) -> Manifest {
    let mut manifest = Manifest::default();
    let Some(data) = zip.read(MANIFEST_PATH) else {
        return manifest;
    };
    let mut reader = Reader::from_reader(data.as_slice());
    let mut current: Option<(String, Option<u64>)> = None;
    let mut entry: Option<Sealed> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let local = element.local_name();
                match local.as_ref() {
                    "file-entry" => {
                        current = attr(&element, "full-path").map(|path| {
                            let size = attr(&element, "size").and_then(|size| size.parse().ok());
                            (path, size)
                        });
                        entry = None;
                    }
                    "encryption-data" => {
                        manifest.encrypted = true;
                        manifest.unnamed |= current.is_none();
                        entry = Some(Sealed {
                            checksum_type: attr(&element, "checksum-type").unwrap_or_default(),
                            checksum: attr(&element, "checksum")
                                .and_then(|value| base64(&value))
                                .unwrap_or_default(),
                            size: current.as_ref().and_then(|(_, size)| *size),
                            ..Sealed::default()
                        });
                    }
                    "algorithm" => {
                        if let Some(entry) = entry.as_mut() {
                            entry.algorithm = attr(&element, "algorithm-name").unwrap_or_default();
                            entry.iv = attr(&element, "initialisation-vector")
                                .and_then(|value| base64(&value))
                                .unwrap_or_default();
                        }
                    }
                    "key-derivation" => {
                        if let Some(entry) = entry.as_mut() {
                            entry.derivation =
                                attr(&element, "key-derivation-name").unwrap_or_default();
                            entry.salt = attr(&element, "salt")
                                .and_then(|value| base64(&value))
                                .unwrap_or_default();
                            entry.iterations = attr(&element, "iteration-count")
                                .and_then(|value| value.parse().ok())
                                .unwrap_or(0);
                            entry.key_size = attr(&element, "key-size")
                                .and_then(|value| value.parse().ok())
                                .unwrap_or(16);
                        }
                    }
                    "start-key-generation" => {
                        if let Some(entry) = entry.as_mut() {
                            entry.start_key =
                                attr(&element, "start-key-generation-name").unwrap_or_default();
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(element)) if element.local_name().as_ref() == "file-entry" => {
                if let (Some((path, _)), Some(sealed_entry)) = (current.take(), entry.take()) {
                    manifest.entries.insert(path, sealed_entry);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                if let (Some((path, _)), Some(sealed_entry)) = (current.take(), entry.take()) {
                    manifest.entries.insert(path, sealed_entry);
                }
                break;
            }
            Ok(_) => {}
        }
    }
    manifest
}

fn attr(element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == name)
        .map(|attribute| attribute.value.into_owned())
}

pub(crate) fn pbkdf2_sha1(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let prf = Hmac::<Sha1>::new_from_slice(password).expect("HMAC accepts any key length");
    let mut key = [0u8; 32];
    for (index, block) in key.chunks_mut(20).enumerate() {
        let mut mac = prf.clone();
        mac.update(salt);
        mac.update(&(index as u32 + 1).to_be_bytes());
        let mut u = mac.finalize().into_bytes();
        let mut t = u;
        for _ in 1..iterations {
            let mut mac = prf.clone();
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

/// AES-256-CBC를 푼다. 한/글의 무채움 스트림도 있으므로 채움 여부는 검사합으로 정한다.
fn aes256_cbc_decrypt(key: &[u8; 32], iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || !data.len().is_multiple_of(16) {
        return None;
    }
    let cipher = Aes256::new(key.into());
    let mut previous: [u8; 16] = iv.try_into().ok()?;
    let mut plain = Vec::with_capacity(data.len());
    let (blocks, _) = data.as_chunks::<16>();
    for chunk in blocks {
        let mut block = Block::from(*chunk);
        cipher.decrypt_block(&mut block);
        plain.extend(
            block
                .iter()
                .zip(previous.iter())
                .map(|(byte, prev)| byte ^ prev),
        );
        previous = *chunk;
    }
    Some(plain)
}

fn inflate(data: &[u8], declared: Option<u64>) -> Option<Vec<u8>> {
    let limit = declared.unwrap_or(MAX_PLAIN_SIZE).min(MAX_PLAIN_SIZE);
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(data)
        .take(limit + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() as u64 > limit {
        return None;
    }
    Some(out)
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypts_an_unpadded_aes_block_without_discarding_its_last_byte() {
        use aes::cipher::BlockCipherEncrypt;
        let key = [0u8; 32];
        let plain = [0u8; 16];
        let mut encrypted = Block::from(plain);
        Aes256::new(&key.into()).encrypt_block(&mut encrypted);
        assert_eq!(
            aes256_cbc_decrypt(&key, &[0; 16], &encrypted),
            Some(plain.to_vec())
        );
    }

    #[test]
    fn pbkdf2_matches_rfc6070() {
        let key = pbkdf2_sha1(b"password", b"salt", 2);
        assert_eq!(
            &key[..20],
            &[
                0xea, 0x6c, 0x01, 0x4d, 0xc7, 0x2d, 0x6f, 0x8c, 0xcd, 0x1e, 0xd9, 0x2a, 0xce, 0x1d,
                0x41, 0xf0, 0xd8, 0xde, 0x89, 0x57
            ]
        );
        let key = pbkdf2_sha1(
            b"passwordPASSWORDpassword",
            b"saltSALTsaltSALTsaltSALTsaltSALTsalt",
            4096,
        );
        assert_eq!(
            &key[..25],
            &[
                0x3d, 0x2e, 0xec, 0x4f, 0xe4, 0x1c, 0x84, 0x9b, 0x80, 0xc8, 0xd8, 0x36, 0x62, 0xc0,
                0xe4, 0x4a, 0x8b, 0x29, 0x1a, 0x96, 0x4c, 0xf2, 0xf0, 0x70, 0x38
            ]
        );
    }

    #[test]
    fn base64_decodes_padded_text() {
        assert_eq!(base64("aGVsbG8=").as_deref(), Some(b"hello".as_slice()));
        assert_eq!(base64("aGk=").as_deref(), Some(b"hi".as_slice()));
        assert_eq!(base64("!!"), None);
    }
}
