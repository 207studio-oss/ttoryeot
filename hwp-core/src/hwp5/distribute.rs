use aes::cipher::{BlockCipherDecrypt, KeyInit};
use aes::{Aes128, Block};

use crate::report::{Warning, WarningKind};

use super::reader::BinReader;

const DISTRIBUTION_TAG: u32 = 28;
const KEY_DATA_SIZE: usize = 256;

pub(super) fn decrypt(
    data: &[u8],
    section: Option<u32>,
    warnings: &mut Vec<Warning>,
) -> Result<Vec<u8>, Warning> {
    let mut reader = BinReader::new(data);
    if reader.remaining() < 4 {
        return Err(unreadable(section, "distribution record header is missing"));
    }
    let header = reader.u32();
    let tag = header & 0x3ff;
    if tag != DISTRIBUTION_TAG {
        return Err(unreadable(
            section,
            format!("distribution record tag is {tag}, expected {DISTRIBUTION_TAG}"),
        ));
    }
    let size = match header >> 20 {
        0xfff => {
            if reader.remaining() < 4 {
                return Err(unreadable(
                    section,
                    "distribution record extended size is missing",
                ));
            }
            reader.u32() as usize
        }
        size => size as usize,
    };
    if size != KEY_DATA_SIZE {
        return Err(unreadable(
            section,
            format!("distribution record size is {size}, expected {KEY_DATA_SIZE}"),
        ));
    }
    let key_data: &[u8; KEY_DATA_SIZE] = reader
        .bytes(KEY_DATA_SIZE)
        .try_into()
        .map_err(|_| unreadable(section, "distribution record 28 key data is truncated"))?;
    let key = derive_key(key_data);
    let cipher = Aes128::new(&key.into());
    let encrypted = reader.bytes(reader.remaining());
    let (blocks, tail) = encrypted.as_chunks::<16>();
    if !tail.is_empty() {
        warnings.push(Warning::new(
            WarningKind::RecordTruncated,
            section,
            format!("distribution AES data has {} trailing byte(s)", tail.len()),
        ));
    }
    if blocks.is_empty() {
        return Err(unreadable(
            section,
            "distribution AES data has no complete block",
        ));
    }
    let mut decrypted = Vec::with_capacity(blocks.len() * 16);
    for bytes in blocks {
        let mut block = Block::from(*bytes);
        cipher.decrypt_block(&mut block);
        decrypted.extend_from_slice(&block);
    }
    warnings.push(Warning::new(
        WarningKind::DistributionDecrypted,
        section,
        format!("{} distribution byte(s) decrypted", decrypted.len()),
    ));
    Ok(decrypted)
}

fn unreadable(section: Option<u32>, detail: impl Into<String>) -> Warning {
    Warning::new(WarningKind::SectionUnreadable, section, detail)
}

fn derive_key(encoded: &[u8; KEY_DATA_SIZE]) -> [u8; 16] {
    let mut data = *encoded;
    let seed = BinReader::new(&data).u32();
    let mut state = seed;
    let mut key = 0u8;
    let mut remaining = 0;
    for (index, byte) in data.iter_mut().enumerate() {
        if remaining == 0 {
            key = (msvc_rand(&mut state) & 0xff) as u8;
            remaining = (msvc_rand(&mut state) & 0xf) + 1;
        }
        if index >= 4 {
            *byte ^= key;
        }
        remaining -= 1;
    }
    let mut aes_key = [0; 16];
    let offset = 4 + (seed & 0xf) as usize;
    for (destination, source) in aes_key.iter_mut().zip(data.iter().skip(offset)) {
        *destination = *source;
    }
    aes_key
}

fn msvc_rand(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(214013).wrapping_add(2531011);
    (*state >> 16) & 0x7fff
}

#[cfg(test)]
mod tests {
    use super::msvc_rand;

    #[test]
    fn msvc_rand_sequence() {
        let mut state = 1;
        let sequence: Vec<_> = (0..10).map(|_| msvc_rand(&mut state)).collect();
        assert_eq!(
            sequence,
            [41, 18467, 6334, 26500, 19169, 15724, 11478, 29358, 26962, 24464]
        );
    }
}
