//! `\x05HwpSummaryInformation` 스트림: OLE 속성 집합(MS-OLEPS)에서 문서 속성을 읽는다.

use crate::info::{non_empty, Metadata};

pub(super) const SUMMARY_STREAM: &str = "\u{5}HwpSummaryInformation";

const VT_LPSTR: u32 = 0x1E;
const VT_LPWSTR: u32 = 0x1F;
const VT_FILETIME: u32 = 0x40;
const FILETIME_TICKS_PER_SECOND: u64 = 10_000_000;
const FILETIME_TO_UNIX_SECONDS: u64 = 11_644_473_600;

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

pub(super) fn parse(data: &[u8]) -> Metadata {
    let mut metadata = Metadata::default();
    if data.get(..2) != Some(&[0xFE, 0xFF]) {
        return metadata;
    }
    let Some(section) = u32_at(data, 44).map(|offset| offset as usize) else {
        return metadata;
    };
    let count = u32_at(data, section.saturating_add(4)).unwrap_or(0) as usize;
    for index in 0..count.min(256) {
        let entry = section.saturating_add(8).saturating_add(index * 8);
        let (Some(id), Some(offset)) = (u32_at(data, entry), u32_at(data, entry + 4)) else {
            break;
        };
        let value = value_at(data, section.saturating_add(offset as usize));
        let slot = match id {
            2 => &mut metadata.title,
            3 => &mut metadata.subject,
            4 => &mut metadata.author,
            5 => &mut metadata.keywords,
            6 => &mut metadata.comments,
            8 => &mut metadata.last_author,
            12 => &mut metadata.created,
            13 => &mut metadata.modified,
            _ => continue,
        };
        *slot = value;
    }
    metadata
}

fn value_at(data: &[u8], offset: usize) -> Option<String> {
    let kind = u32_at(data, offset)?;
    let body = offset.checked_add(4)?;
    match kind {
        VT_LPWSTR => {
            let count = u32_at(data, body)? as usize;
            let start = body.checked_add(4)?;
            let bytes = data.get(start..start.checked_add(count.checked_mul(2)?)?)?;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect();
            non_empty(&String::from_utf16_lossy(&units))
        }
        VT_LPSTR => {
            let count = u32_at(data, body)? as usize;
            let start = body.checked_add(4)?;
            let bytes = data.get(start..start.checked_add(count)?)?;
            non_empty(&String::from_utf8_lossy(bytes))
        }
        VT_FILETIME => {
            let low = u64::from(u32_at(data, body)?);
            let high = u64::from(u32_at(data, body.checked_add(4)?)?);
            filetime((high << 32) | low)
        }
        _ => None,
    }
}

fn filetime(ticks: u64) -> Option<String> {
    let seconds = (ticks / FILETIME_TICKS_PER_SECOND).checked_sub(FILETIME_TO_UNIX_SECONDS)?;
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Howard Hinnant의 civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property_set(properties: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut data = vec![0xFE, 0xFF];
        data.resize(44, 0);
        data.extend(48u32.to_le_bytes());
        let table = 8 + properties.len() * 8;
        let mut values: Vec<u8> = Vec::new();
        let mut entries = Vec::new();
        for (id, value) in properties {
            entries.extend(id.to_le_bytes());
            entries.extend(((table + values.len()) as u32).to_le_bytes());
            values.extend(value);
        }
        data.extend(((table + values.len()) as u32).to_le_bytes());
        data.extend((properties.len() as u32).to_le_bytes());
        data.extend(entries);
        data.extend(values);
        data
    }

    fn wide(text: &str) -> Vec<u8> {
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let mut value = VT_LPWSTR.to_le_bytes().to_vec();
        value.extend((units.len() as u32).to_le_bytes());
        value.extend(units.iter().flat_map(|unit| unit.to_le_bytes()));
        value
    }

    #[test]
    fn reads_strings_and_filetimes() {
        let mut created = VT_FILETIME.to_le_bytes().to_vec();
        created.extend(133_800_000_000_000_000u64.to_le_bytes());
        let metadata = parse(&property_set(&[
            (2, wide("합성 제목")),
            (4, wide("작성자")),
            (12, created),
            (99, wide("모르는 속성")),
        ]));
        assert_eq!(metadata.title.as_deref(), Some("합성 제목"));
        assert_eq!(metadata.author.as_deref(), Some("작성자"));
        assert_eq!(metadata.created.as_deref(), Some("2024-12-30T02:40:00Z"));
        assert_eq!(metadata.subject, None);
    }

    #[test]
    fn damaged_streams_give_no_metadata() {
        assert_eq!(parse(b""), Metadata::default());
        assert_eq!(parse(&[0xFE, 0xFF, 0, 0]), Metadata::default());
        let whole = property_set(&[(2, wide("제목"))]);
        for length in 0..whole.len() {
            assert_eq!(parse(&whole[..length]).subject, None);
        }
        let mut huge = whole.clone();
        let count_at = 48 + 8 + 8 + 4;
        huge[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&huge).title, None);
    }
}
