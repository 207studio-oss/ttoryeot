//! HWP5의 CFB 껍데기를 연다. cfb 크레이트가 거절한 파일은 머리·FAT·디렉터리를 직접 읽어
//! 표준 CFB로 다시 쓴 뒤 연다. 알려진 변형은 머리의 mini stream cutoff가 4096이 아니고
//! (0이면) 모든 스트림이 일반 섹터에 있는 파일이다.

use std::borrow::Cow;
use std::io::{Cursor, Write};

use cfb::CompoundFile;

use crate::report::Fatal;

pub type Container<'a> = CompoundFile<Cursor<Cow<'a, [u8]>>>;

const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const HEADER_LEN: usize = 512;
const DIFAT_IN_HEADER: usize = 109;
const ENTRY_LEN: usize = 128;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE: u32 = 0xFFFF_FFFF;
const NO_ENTRY: u32 = 0xFFFF_FFFF;
const STORAGE: u8 = 1;
const STREAM: u8 = 2;

pub fn open(bytes: &[u8]) -> Result<Container<'_>, Fatal> {
    if let Ok(file) = cfb::OpenOptions::new().open_with(Cursor::new(Cow::Borrowed(bytes))) {
        return Ok(file);
    }
    let rebuilt = read_streams(bytes)
        .and_then(|streams| write_standard(&streams))
        .ok_or(Fatal::NotHwp)?;
    cfb::OpenOptions::new()
        .open_with(Cursor::new(Cow::Owned(rebuilt)))
        .map_err(|_| Fatal::NotHwp)
}

struct Raw<'a> {
    bytes: &'a [u8],
    sector_len: usize,
    fat: Vec<u32>,
}

impl Raw<'_> {
    fn sector(&self, id: u32) -> Option<&[u8]> {
        let start = (id as usize).checked_add(1)?.checked_mul(self.sector_len)?;
        self.bytes.get(start..start.checked_add(self.sector_len)?)
    }

    fn chain(&self, start: u32) -> Option<Vec<u8>> {
        let mut data = Vec::new();
        for id in chain(start, &self.fat)? {
            data.extend_from_slice(self.sector(id)?);
        }
        Some(data)
    }
}

/// FAT(또는 mini FAT)을 따라간 섹터 번호. 고리나 범위 밖 번호면 None.
fn chain(start: u32, table: &[u32]) -> Option<Vec<u32>> {
    let mut ids = Vec::new();
    let mut id = start;
    while id != END_OF_CHAIN {
        if ids.len() >= table.len() {
            return None;
        }
        ids.push(id);
        id = *table.get(id as usize)?;
    }
    Some(ids)
}

fn words(data: &[u8]) -> impl Iterator<Item = u32> + '_ {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
}

/// (경로, 내용). 경로는 "BodyText/Section0"처럼 `/`로 잇는다.
fn read_streams(bytes: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let header = bytes.get(..HEADER_LEN)?;
    if header[..8] != MAGIC {
        return None;
    }
    let u16_at = |offset: usize| u16::from_le_bytes([header[offset], header[offset + 1]]);
    let u32_at = |offset: usize| {
        u32::from_le_bytes([
            header[offset],
            header[offset + 1],
            header[offset + 2],
            header[offset + 3],
        ])
    };
    let major = u16_at(26);
    let sector_shift = u16_at(30);
    let mini_shift = u16_at(32);
    if !matches!((major, sector_shift), (3, 9) | (4, 12)) || mini_shift != 6 {
        return None;
    }
    let sector_len = 1usize << sector_shift;
    let mut raw = Raw {
        bytes,
        sector_len,
        fat: Vec::new(),
    };

    let mut fat_sectors: Vec<u32> = words(&header[76..76 + DIFAT_IN_HEADER * 4]).collect();
    let mut difat = u32_at(68);
    let sector_count = bytes.len() / sector_len;
    for _ in 0..(u32_at(72) as usize).min(sector_count) {
        let sector = raw.sector(difat)?;
        let (entries, next) = sector.split_at(sector_len - 4);
        fat_sectors.extend(words(entries));
        difat = words(next).next()?;
    }
    let fat_count = u32_at(44) as usize;
    for id in fat_sectors
        .into_iter()
        .filter(|id| *id != FREE)
        .take(fat_count)
    {
        let fat: Vec<u32> = words(raw.sector(id)?).collect();
        raw.fat.extend(fat);
    }

    let directory = raw.chain(u32_at(48))?;
    let entries = directory.as_chunks::<ENTRY_LEN>().0;
    let root = entries.first()?;
    let cutoff = u64::from(u32_at(56));
    let mini_stream = raw.chain(entry_u32(root, 116))?;
    let mini_fat: Vec<u32> = match u32_at(64) {
        0 => Vec::new(),
        _ => words(&raw.chain(u32_at(60))?).collect(),
    };
    let mini_len = 1usize << mini_shift;

    let mut streams = Vec::new();
    // 올바른 CFB에서는 섹터 하나가 한 스트림에만 속한다. 여러 항목이 같은 사슬을 가리켜 파일보다
    // 큰 자료를 만들면 거절한다.
    let mut total = 0usize;
    let mut pending = vec![(entry_u32(root, 76), String::new())];
    let mut visited = vec![false; entries.len()];
    while let Some((index, parent)) = pending.pop() {
        if index == NO_ENTRY {
            continue;
        }
        let entry = entries.get(index as usize)?;
        if std::mem::replace(visited.get_mut(index as usize)?, true) {
            return None;
        }
        pending.push((entry_u32(entry, 68), parent.clone()));
        pending.push((entry_u32(entry, 72), parent.clone()));
        let name = entry_name(entry)?;
        let path = if parent.is_empty() {
            name
        } else {
            format!("{parent}/{name}")
        };
        let size = match major {
            3 => u64::from(entry_u32(entry, 120)),
            _ => u64::from_le_bytes(entry[120..128].try_into().ok()?),
        };
        let start = entry_u32(entry, 116);
        match entry[66] {
            STORAGE => pending.push((entry_u32(entry, 76), path)),
            STREAM => {
                let mut data = if size < cutoff {
                    let mut data = Vec::new();
                    for id in chain(start, &mini_fat)? {
                        let offset = (id as usize).checked_mul(mini_len)?;
                        data.extend_from_slice(mini_stream.get(offset..offset + mini_len)?);
                    }
                    data
                } else if size == 0 {
                    Vec::new()
                } else {
                    raw.chain(start)?
                };
                data.truncate(usize::try_from(size).ok()?);
                total += data.len();
                if total > bytes.len() {
                    return None;
                }
                streams.push((path, data));
            }
            _ => {}
        }
    }
    Some(streams)
}

fn entry_u32(entry: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        entry[offset],
        entry[offset + 1],
        entry[offset + 2],
        entry[offset + 3],
    ])
}

fn entry_name(entry: &[u8]) -> Option<String> {
    let len = usize::from(u16::from_le_bytes([entry[64], entry[65]]));
    let units = entry.get(..len.checked_sub(2)?)?;
    let units = units
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}

fn write_standard(streams: &[(String, Vec<u8>)]) -> Option<Vec<u8>> {
    let mut file = cfb::OpenOptions::new()
        .create_with(Cursor::new(Vec::new()))
        .ok()?;
    for (path, data) in streams {
        let path = format!("/{path}");
        if let Some((parent, _)) = path
            .rsplit_once('/')
            .filter(|(parent, _)| !parent.is_empty())
        {
            file.create_storage_all(parent).ok()?;
        }
        file.create_stream(&path).ok()?.write_all(data).ok()?;
    }
    file.flush().ok()?;
    Some(file.into_inner().into_inner())
}
