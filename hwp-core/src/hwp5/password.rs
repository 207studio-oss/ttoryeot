//! HWP5 password streams. EncryptVersion 0 uses DES, EncryptVersion 4 uses AES-128.
//! Algorithm reference: Volexity hwp-extract (AES) and the rhwp/hwpjs DES envelope.
//! Notices: licenses/Volexity-BSD-3-Clause.md and THIRD_PARTY_NOTICES.md (MIT).

use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Block as AesBlock};
use des::cipher::Block as DesBlock;
use des::Des;
use sha1::{Digest, Sha1};

const FOOTER_MARKER: [u8; 4] = [0x38, 0xa4, 0xce, 0x59];

pub(super) enum PasswordCipher {
    Des(Box<Des>),
    Aes(Box<Aes128>),
}

/// Result of checking the optional 8-byte footer that follows the ciphertext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FooterIntegrity {
    /// The footer was present and its CRC32 matches the decrypted payload.
    Verified,
    /// No footer was present. Older fixtures and some writers omit it.
    Absent,
    /// The footer was present but its CRC32 does not match. Corruption and a wrong
    /// password both produce this, so callers must not report it as a wrong password.
    Mismatch,
}

/// A protected stream that cannot be decrypted as a whole cipher block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamError {
    Truncated { len: usize, block: usize },
}

/// Result of checking the EncryptVersion 0 compact trailer inside a compressed stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompactTrailer {
    Absent,
    Valid,
    Mismatch(&'static str),
}

/// `EncryptVersion` 0 is DES, 4 is AES-128.
pub(super) fn supported_version(version: u32) -> bool {
    matches!(version, 0 | 4)
}

impl PasswordCipher {
    /// Builds a cipher for a supported version. Callers check `supported_version` first.
    pub(super) fn new(password: &str, version: u32) -> Option<Self> {
        match version {
            0 => Some(Self::Des(Box::new(
                Des::new_from_slice(&des_key(password)).expect("DES key is eight bytes"),
            ))),
            4 => Some(Self::Aes(Box::new(
                Aes128::new_from_slice(&aes_key(password)).expect("SHA-1 contains an AES-128 key"),
            ))),
            _ => None,
        }
    }

    pub(super) fn uses_des(&self) -> bool {
        matches!(self, Self::Des(_))
    }

    /// Decrypt one protected stream in place.
    ///
    /// A trailing 8-byte footer (`38 a4 ce 59`, CRC32 of the plaintext) is removed when
    /// present. It is not part of the ciphertext and must not reach the inflater. Synthetic
    /// and older fixtures may omit it, so its absence is tolerated. A CRC mismatch is
    /// reported, not treated as a password failure.
    pub(super) fn decrypt(&self, bytes: &mut Vec<u8>) -> Result<FooterIntegrity, StreamError> {
        let block = match self {
            Self::Des(_) => 8,
            Self::Aes(_) => 16,
        };
        let footer = bytes.len() >= 8
            && bytes[bytes.len() - 8..bytes.len() - 4] == FOOTER_MARKER
            && (bytes.len() - 8) > 0
            && (bytes.len() - 8).is_multiple_of(block);
        let payload_len = if footer { bytes.len() - 8 } else { bytes.len() };
        let expected = footer.then(|| {
            u32::from_le_bytes(
                bytes[payload_len + 4..payload_len + 8]
                    .try_into()
                    .expect("footer has a four-byte checksum"),
            )
        });
        match self {
            Self::Des(cipher) => decrypt_des(cipher, &mut bytes[..payload_len])?,
            Self::Aes(cipher) => decrypt_aes(cipher, &mut bytes[..payload_len]),
        }
        let integrity = match expected {
            Some(expected) if crc32fast::hash(&bytes[..payload_len]) != expected => {
                FooterIntegrity::Mismatch
            }
            Some(_) => FooterIntegrity::Verified,
            None => FooterIntegrity::Absent,
        };
        bytes.truncate(payload_len);
        Ok(integrity)
    }
}

/// Checks the EncryptVersion 0 compact trailer that follows a DEFLATE stream:
/// a little-endian CRC32 of the plaintext, the plaintext length, then zero padding to
/// an 8-byte boundary. Streams without the exact layout are treated as not carrying it.
pub(super) fn validate_compact_trailer(
    raw: &[u8],
    consumed: usize,
    plain: &[u8],
) -> CompactTrailer {
    let Some(end) = consumed.checked_add(8) else {
        return CompactTrailer::Absent;
    };
    let padding = (8 - end % 8) % 8;
    if raw.len() != end + padding {
        return CompactTrailer::Absent;
    }
    if !raw[end..].iter().all(|byte| *byte == 0) {
        return CompactTrailer::Mismatch("zero padding");
    }
    let expected = u32::from_le_bytes(
        raw[consumed..consumed + 4]
            .try_into()
            .expect("trailer has a four-byte checksum"),
    );
    if crc32fast::hash(plain) != expected {
        return CompactTrailer::Mismatch("CRC32");
    }
    let size = u32::from_le_bytes(
        raw[consumed + 4..end]
            .try_into()
            .expect("trailer has a four-byte length"),
    );
    if usize::try_from(size).ok() != Some(plain.len()) {
        return CompactTrailer::Mismatch("length");
    }
    CompactTrailer::Valid
}

fn aes_key(password: &str) -> [u8; 16] {
    let mut hash = Sha1::new();
    let mut previous = 236u8;
    for byte in password.bytes() {
        hash.update([previous.rotate_left(1), byte]);
        previous = byte;
    }
    let digest = hash.finalize();
    digest[..16]
        .try_into()
        .expect("SHA-1 contains an AES-128 key")
}

fn decrypt_aes(cipher: &Aes128, bytes: &mut [u8]) {
    let mut register = 0u128;
    for byte in bytes {
        let ciphertext = *byte;
        let mut plaintext = 0;
        for bit in (0..8).rev() {
            let mut block = AesBlock::from(register.to_be_bytes());
            cipher.encrypt_block(&mut block);
            let input = (ciphertext >> bit) & 1;
            plaintext |= (input ^ (block[0] >> 7)) << bit;
            register = (register << 1) | u128::from(input);
        }
        *byte = plaintext;
    }
}

/// Legacy key derivation: UTF-16LE password code units are rolled into seven bytes with a
/// rotate-left XOR, then 56 bits are packed seven bits per key byte.
fn des_key(password: &str) -> [u8; 8] {
    let mut rolling = [0u8; 7];
    let mut position = 0usize;
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

fn decrypt_des(cipher: &Des, bytes: &mut [u8]) -> Result<(), StreamError> {
    if !bytes.len().is_multiple_of(8) {
        return Err(StreamError::Truncated {
            len: bytes.len(),
            block: 8,
        });
    }
    for chunk in bytes.as_chunks_mut::<8>().0 {
        let mut block = DesBlock::<Des>::from(*chunk);
        cipher.decrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    Ok(())
}

pub(super) fn protected_stream(name: &str) -> bool {
    name == "DocInfo"
        || name.starts_with("BodyText/")
        || name.starts_with("BinData/")
        || name.starts_with("Scripts/")
}

#[cfg(test)]
mod tests {
    use super::{aes_key, des_key, validate_compact_trailer, CompactTrailer};

    #[test]
    fn compact_trailer_checks_crc_length_padding_and_alignment() {
        let plain = b"plain";
        let consumed = 5usize;
        let end = consumed + 8;
        let padding = (8 - end % 8) % 8;
        let mut raw = vec![0u8; end + padding];
        raw[consumed..consumed + 4].copy_from_slice(&crc32fast::hash(plain).to_le_bytes());
        raw[consumed + 4..end].copy_from_slice(&(plain.len() as u32).to_le_bytes());
        assert_eq!(
            validate_compact_trailer(&raw, consumed, plain),
            CompactTrailer::Valid
        );
        raw[end + padding - 1] = 1;
        assert_eq!(
            validate_compact_trailer(&raw, consumed, plain),
            CompactTrailer::Mismatch("zero padding")
        );
        raw[end + padding - 1] = 0;
        raw[consumed] ^= 1;
        assert_eq!(
            validate_compact_trailer(&raw, consumed, plain),
            CompactTrailer::Mismatch("CRC32")
        );
        raw[consumed] ^= 1;
        raw[consumed + 4] = 4;
        assert_eq!(
            validate_compact_trailer(&raw, consumed, plain),
            CompactTrailer::Mismatch("length")
        );
        assert_eq!(
            validate_compact_trailer(&raw[..raw.len() - 1], consumed, plain),
            CompactTrailer::Absent
        );
    }

    #[test]
    fn des_key_matches_independent_vectors() {
        assert_eq!(
            des_key("12345"),
            [0xc4, 0x34, 0xb2, 0x00, 0x06, 0x30, 0x00, 0xd0]
        );
        assert_eq!(
            des_key("helloworld"),
            [0xa6, 0xde, 0x7c, 0x7a, 0xda, 0x5c, 0xf6, 0x62]
        );
        assert_eq!(des_key(""), [0; 8]);
    }

    #[test]
    fn aes_key_matches_independent_vector() {
        assert_eq!(
            aes_key("helloworld"),
            [
                0x20, 0x7a, 0xe2, 0xca, 0x4c, 0xd8, 0x7f, 0xcf, 0x0d, 0x01, 0x56, 0x9d, 0x8c, 0x14,
                0x71, 0x5a,
            ]
        );
    }
}
