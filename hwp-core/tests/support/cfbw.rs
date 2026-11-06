use std::io::{Cursor, Write};
use std::path::Path;

use flate2::write::DeflateEncoder;
use flate2::Compression;

pub fn build(streams: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut file = cfb::OpenOptions::new()
        .create_with(Cursor::new(Vec::new()))
        .expect("create cfb");
    for (name, data) in streams {
        let path = format!("/{name}");
        if let Some(parent) = Path::new(&path).parent() {
            if parent != Path::new("/") {
                file.create_storage_all(parent).expect("create storage");
            }
        }
        let mut stream = file.create_stream(&path).expect("create stream");
        stream.write_all(data).expect("write stream");
    }
    file.flush().expect("flush cfb");
    file.into_inner().into_inner()
}

pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("deflate");
    encoder.finish().expect("finish deflate")
}

const SECTOR: usize = 512;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FAT_SECTOR: u32 = 0xFFFF_FFFD;
const FREE: u32 = 0xFFFF_FFFF;

/// 일부 HWP 작성기가 내는 CFB: 머리의 mini stream cutoff가 0이고 모든 스트림이 512바이트 일반
/// 섹터에 있다. 섹터는 스트림 자료, 디렉터리, FAT 순서다. cfb 크레이트는 이 파일을 거절한다.
pub fn build_without_mini_stream(streams: &[(String, Vec<u8>)]) -> Vec<u8> {
    // 디렉터리 항목: (이름, 종류, 부모 번호, 자료). 0번은 뿌리.
    let mut entries: Vec<(String, u8, usize, &[u8])> = vec![("Root Entry".into(), 5, 0, &[])];
    for (path, data) in streams {
        let mut parent = 0;
        let parts: Vec<&str> = path.split('/').collect();
        for (depth, part) in parts.iter().enumerate() {
            let is_stream = depth + 1 == parts.len();
            let found = entries.iter().position(|(name, kind, owner, _)| {
                name == part && *owner == parent && *kind == 1 && !is_stream
            });
            parent = match found {
                Some(index) => index,
                None => {
                    let kind = if is_stream { 2 } else { 1 };
                    let data: &[u8] = if is_stream { data } else { &[] };
                    entries.push(((*part).to_string(), kind, parent, data));
                    entries.len() - 1
                }
            };
        }
    }

    let mut body = Vec::new();
    let mut fat = Vec::new();
    let mut starts = vec![END_OF_CHAIN; entries.len()];
    for (index, (_, kind, _, data)) in entries.iter().enumerate() {
        if *kind != 2 || data.is_empty() {
            continue;
        }
        starts[index] = fat.len() as u32;
        let sectors = data.len().div_ceil(SECTOR);
        for offset in 1..sectors {
            fat.push((starts[index] as usize + offset) as u32);
        }
        fat.push(END_OF_CHAIN);
        body.extend_from_slice(data);
        body.resize(fat.len() * SECTOR, 0);
    }

    // 형제는 (이름 길이, 대문자 이름) 순으로 오른쪽에 잇는다.
    let mut links = vec![(FREE, FREE, FREE); entries.len()];
    for parent in 0..entries.len() {
        let mut children: Vec<usize> = (1..entries.len())
            .filter(|index| entries[*index].2 == parent)
            .collect();
        children.sort_by_key(|index| {
            let name = &entries[*index].0;
            (name.encode_utf16().count(), name.to_uppercase())
        });
        if let Some(first) = children.first() {
            links[parent].2 = *first as u32;
        }
        for pair in children.windows(2) {
            links[pair[0]].1 = pair[1] as u32;
        }
    }
    let directory_start = fat.len() as u32;
    let mut directory = Vec::new();
    for (index, (name, kind, _, data)) in entries.iter().enumerate() {
        let mut entry = [0u8; 128];
        let units: Vec<u16> = name.encode_utf16().chain([0]).collect();
        for (slot, unit) in units.iter().enumerate() {
            entry[slot * 2..slot * 2 + 2].copy_from_slice(&unit.to_le_bytes());
        }
        entry[64..66].copy_from_slice(&((units.len() * 2) as u16).to_le_bytes());
        entry[66] = *kind;
        entry[67] = 1;
        let (left, right, child) = links[index];
        entry[68..72].copy_from_slice(&left.to_le_bytes());
        entry[72..76].copy_from_slice(&right.to_le_bytes());
        entry[76..80].copy_from_slice(&child.to_le_bytes());
        entry[116..120].copy_from_slice(&starts[index].to_le_bytes());
        entry[120..124].copy_from_slice(&(data.len() as u32).to_le_bytes());
        directory.extend_from_slice(&entry);
    }
    directory.resize(directory.len().div_ceil(SECTOR) * SECTOR, 0);
    let directory_sectors = directory.len() / SECTOR;
    for offset in 1..directory_sectors {
        fat.push(directory_start + offset as u32);
    }
    fat.push(END_OF_CHAIN);
    body.extend_from_slice(&directory);

    let entries_per_sector = SECTOR / 4;
    let fat_start = fat.len();
    let mut fat_sectors = 1;
    while (fat_start + fat_sectors).div_ceil(entries_per_sector) > fat_sectors {
        fat_sectors += 1;
    }
    assert!(fat_sectors <= 109, "fixture too large for header DIFAT");
    fat.extend(std::iter::repeat_n(FAT_SECTOR, fat_sectors));
    fat.resize(fat_sectors * entries_per_sector, FREE);
    for entry in &fat {
        body.extend_from_slice(&entry.to_le_bytes());
    }

    let mut header = vec![0u8; SECTOR];
    header[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    header[24..26].copy_from_slice(&0x3Eu16.to_le_bytes());
    header[26..28].copy_from_slice(&3u16.to_le_bytes());
    header[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    header[30..32].copy_from_slice(&9u16.to_le_bytes());
    header[32..34].copy_from_slice(&6u16.to_le_bytes());
    header[44..48].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
    header[48..52].copy_from_slice(&directory_start.to_le_bytes());
    header[60..64].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
    header[68..72].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
    for (slot, difat) in header[76..].as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let id = if slot < fat_sectors {
            (fat_start + slot) as u32
        } else {
            FREE
        };
        difat.copy_from_slice(&id.to_le_bytes());
    }
    header.extend_from_slice(&body);
    header
}
