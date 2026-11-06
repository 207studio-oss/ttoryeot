const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_MAGIC: [u8; 4] = [b'P', b'K', 3, 4];
const HWP3_MAGIC: &[u8] = b"HWP Document File V3";
const XML_MAGIC: &[u8] = b"<?xml";
const HWPML_MAGIC: &[u8] = b"<HWPML";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Cfb,
    Zip,
    Hwp3,
    Hwpml,
    Unknown,
}

pub fn detect(bytes: &[u8]) -> Format {
    if bytes.starts_with(&CFB_MAGIC) {
        return Format::Cfb;
    }
    if bytes.starts_with(&ZIP_MAGIC) {
        return Format::Zip;
    }
    if bytes.starts_with(HWP3_MAGIC) {
        return Format::Hwp3;
    }
    if looks_like_hwpml(bytes) {
        return Format::Hwpml;
    }
    Format::Unknown
}

fn looks_like_hwpml(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let rest = &bytes[start..];
    if !rest.starts_with(XML_MAGIC) {
        return false;
    }
    rest.windows(HWPML_MAGIC.len())
        .any(|window| window == HWPML_MAGIC)
}
