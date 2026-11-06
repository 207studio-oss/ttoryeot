/// 이름표(`name`) 표의 (표 시작, 이름 개수, 문자열 시작) 위치.
fn name_table(bytes: &[u8]) -> (usize, usize, usize) {
    let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    for index in 0..tables {
        let record = 12 + index * 16;
        if &bytes[record..record + 4] != b"name" {
            continue;
        }
        let offset = u32::from_be_bytes(
            bytes[record + 8..record + 12]
                .try_into()
                .expect("name offset"),
        ) as usize;
        let count = usize::from(u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]));
        let strings = usize::from(u16::from_be_bytes([bytes[offset + 4], bytes[offset + 5]]));
        return (offset, count, strings);
    }
    panic!("test font has no name table");
}

/// 이름표의 문자열 하나를 더 짧은 이름으로 바꾼다. 글꼴 파일의 다른 위치는 그대로 둔다.
pub fn rename_font(data: &[u8], old: &str, new: &str) -> Vec<u8> {
    assert!(new.len() <= old.len(), "{new:?} is longer than {old:?}");
    let mut bytes = data.to_vec();
    let (table, count, strings) = name_table(&bytes);
    for index in 0..count {
        let record = table + 6 + index * 12;
        let platform = u16::from_be_bytes([bytes[record], bytes[record + 1]]);
        let length = usize::from(u16::from_be_bytes([bytes[record + 8], bytes[record + 9]]));
        let offset = usize::from(u16::from_be_bytes([bytes[record + 10], bytes[record + 11]]));
        let start = table + strings + offset;
        let end = start + length;
        let unicode = matches!(platform, 0 | 3);
        let old_bytes: Vec<u8> = if unicode {
            old.encode_utf16().flat_map(u16::to_be_bytes).collect()
        } else {
            old.as_bytes().to_vec()
        };
        if bytes.get(start..end) != Some(old_bytes.as_slice()) {
            continue;
        }
        let new_bytes: Vec<u8> = if unicode {
            new.encode_utf16().flat_map(u16::to_be_bytes).collect()
        } else {
            new.as_bytes().to_vec()
        };
        bytes[start..start + new_bytes.len()].copy_from_slice(&new_bytes);
        bytes[record + 8..record + 10].copy_from_slice(&(new_bytes.len() as u16).to_be_bytes());
    }
    bytes
}
