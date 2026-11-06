pub const T_DOCUMENT_PROPERTIES: u16 = 16;
pub const T_PARA_HEADER: u16 = 66;
pub const T_PARA_TEXT: u16 = 67;
pub const T_LIST_HEADER: u16 = 72;
pub const T_MEMO_SHAPE: u16 = 92;
pub const T_MEMO_LIST: u16 = 93;

pub fn record(tag: u16, level: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = header(tag, level, data.len()).to_vec();
    bytes.extend_from_slice(data);
    bytes
}

pub fn header(tag: u16, level: u16, size: usize) -> Vec<u8> {
    if size < 0xFFF {
        (u32::from(tag) | (u32::from(level) << 10) | ((size as u32) << 20))
            .to_le_bytes()
            .to_vec()
    } else {
        let mut bytes = (u32::from(tag) | (u32::from(level) << 10) | (0xFFF << 20))
            .to_le_bytes()
            .to_vec();
        bytes.extend_from_slice(&(size as u32).to_le_bytes());
        bytes
    }
}

pub fn document_properties(section_count: u16) -> Vec<u8> {
    record(T_DOCUMENT_PROPERTIES, 0, &section_count.to_le_bytes())
}

pub fn paragraph_header() -> Vec<u8> {
    record(T_PARA_HEADER, 0, &[0; 22])
}
