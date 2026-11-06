//! 한/글이 설치한 HFT 글꼴의 영문 글자 폭만 읽는다.
//!
//! 한/글은 글꼴 종류가 HFT인 영문 글꼴을 자기 HFT 파일로 그리고 그 폭으로 줄을 나눈다.
//! 또렷은 HFT 모양을 그리지 않는다. 대신할 TrueType 글꼴로 그리되 폭은 HFT 파일의 것을 쓴다.
//! HFT 파일은 사용자 컴퓨터에 한/글과 함께 설치된 것만 읽고, 저장소나 앱에 넣지 않는다.
//!
//! 읽는 자리(HCI Poppy·한양신명조 영문 파일에서 확인):
//! - 0x17A: u16 한 em의 단위 수
//! - 0x1AA: u32 폭 표의 자리
//! - 폭 표: u16 ×2(쓰지 않음), u16 첫 글자 번호, u16 끝 글자 번호(끝은 빼고), u16(쓰지 않음),
//!   그다음 글자마다 u16 폭

use crate::fonts::key;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// HFT 영문 파일 하나의 글자 폭.
#[derive(Debug, Clone, PartialEq)]
pub struct HftWidths {
    em: f32,
    first: u32,
    widths: Vec<u16>,
}

const MAGIC: &[u8] = b"Han Unified Font File";

impl HftWidths {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if !bytes.starts_with(MAGIC) {
            return None;
        }
        let u16_at = |at: usize| {
            bytes
                .get(at..at + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
        };
        let em = u16_at(0x17A)?;
        let table = u16_at(0x1AA)? as usize | (u16_at(0x1AC)? as usize) << 16;
        let first = u16_at(table + 4)?;
        let end = u16_at(table + 6)?;
        if em == 0 || end <= first || end - first > 0x400 {
            return None;
        }
        let widths = (0..usize::from(end - first))
            .map(|index| u16_at(table + 10 + 2 * index))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            em: f32::from(em),
            first: u32::from(first),
            widths,
        })
    }

    /// 글자 폭(em). 표 밖이거나 폭이 0인 글자, ASCII가 아닌 글자는 모른다.
    /// 표의 0x80 위쪽은 HFT 자체 부호라 유니코드와 맞지 않는다.
    pub fn advance(&self, ch: char) -> Option<f32> {
        let code = u32::from(ch);
        if !(0x20..0x7F).contains(&code) {
            return None;
        }
        let width = *self.widths.get(code.checked_sub(self.first)? as usize)?;
        (width > 0).then(|| f32::from(width) / self.em)
    }
}

/// `hftinfo.dat`의 영문 글꼴 정의: 이름 → 보통·굵게·기울임·굵은 기울임 파일.
#[derive(Debug, Default)]
pub struct HftIndex {
    latin: HashMap<String, (PathBuf, [String; 4])>,
    loaded: HashMap<PathBuf, Option<usize>>,
    tables: Vec<HftWidths>,
}

impl HftIndex {
    /// `hftinfo.dat` 하나를 더한다. 같은 이름은 먼저 더한 것을 쓴다.
    pub fn add_info(&mut self, path: &Path, bytes: &[u8]) {
        let Some(dir) = path.parent() else {
            return;
        };
        let text = decode_utf16(bytes);
        let mut in_latin = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_latin = line == "[Font Definition - Latin]";
                continue;
            }
            if !in_latin {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let fields: Vec<&str> = value.split(',').map(str::trim).collect();
            if fields.len() < 4 || fields[0].is_empty() {
                continue;
            }
            let files = [0, 1, 2, 3].map(|index| fields[index].to_string());
            let mut names = vec![name.trim()];
            if let Some(english) = fields.get(6).filter(|english| !english.is_empty()) {
                names.push(english);
            }
            for name in names {
                self.latin
                    .entry(key(name))
                    .or_insert_with(|| (dir.to_path_buf(), files.clone()));
            }
        }
    }

    /// 영문 HFT 글꼴의 폭 표 번호. 굵게·기울임 파일이 없으면 보통 파일을 쓴다.
    pub fn find(&mut self, name: &str, bold: bool, italic: bool) -> Option<usize> {
        let (dir, files) = self.latin.get(&key(name))?;
        let wanted = usize::from(bold) | usize::from(italic) << 1;
        let file = [wanted, 0]
            .into_iter()
            .map(|index| &files[index])
            .find(|file| !file.is_empty())?;
        let path = find_file(dir, file)?;
        if let Some(id) = self.loaded.get(&path) {
            return *id;
        }
        let id = std::fs::read(&path)
            .ok()
            .and_then(|bytes| HftWidths::parse(&bytes))
            .map(|table| {
                self.tables.push(table);
                self.tables.len() - 1
            });
        self.loaded.insert(path, id);
        id
    }

    pub fn advance(&self, id: usize, ch: char) -> Option<f32> {
        self.tables.get(id)?.advance(ch)
    }
}

/// 파일 이름의 대소문자가 `hftinfo.dat`과 다를 수 있어 폴더에서 이름을 맞춰 찾는다.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

fn decode_utf16(bytes: &[u8]) -> String {
    let body = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
    let units: Vec<u16> = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HCI Poppy 영문 파일과 같은 자리에 같은 값을 둔 작은 HFT.
    fn sample(em: u16, widths: &[(char, u16)]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x300];
        bytes[..MAGIC.len()].copy_from_slice(MAGIC);
        bytes[0x17A..0x17C].copy_from_slice(&em.to_le_bytes());
        bytes[0x1AA..0x1AE].copy_from_slice(&0x200u32.to_le_bytes());
        bytes[0x204..0x206].copy_from_slice(&0x20u16.to_le_bytes());
        bytes[0x206..0x208].copy_from_slice(&0x80u16.to_le_bytes());
        for (ch, width) in widths {
            let at = 0x20A + 2 * (*ch as usize - 0x20);
            bytes[at..at + 2].copy_from_slice(&width.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn reads_widths_in_em_units() {
        let table = HftWidths::parse(&sample(512, &[(' ', 128), ('<', 310)])).expect("parse");
        assert_eq!(table.advance(' '), Some(0.25));
        assert!((table.advance('<').unwrap() - 0.6055).abs() < 1e-3);
        assert_eq!(table.advance('a'), None, "폭 0은 모른다");
        assert_eq!(table.advance('·'), None, "ASCII 밖은 모른다");
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(HftWidths::parse(b"OTTO"), None);
    }

    #[test]
    fn info_maps_korean_and_english_names_to_files() {
        let dir = std::env::temp_dir().join(format!("ttoryeot-hft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("POPPY.HFT"), sample(512, &[('<', 310)])).unwrap();
        let info = "\u{feff}[Font Definition - Hangul]\r\n명조=HGMJ.HFT,,,,x,1042,Myeongjo\r\n\
                    [Font Definition - Latin]\r\nHCI Poppy=poppy.hft,,,,휴먼컴퓨터,1033,HCI Poppy\r\n";
        let bytes: Vec<u8> = info.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut index = HftIndex::default();
        index.add_info(&dir.join("hftinfo.dat"), &bytes);
        assert!(
            index.find("명조", false, false).is_none(),
            "한글 칸은 읽지 않는다"
        );
        let id = index
            .find("hci poppy", true, false)
            .expect("굵게 없으면 보통 파일");
        assert!((index.advance(id, '<').unwrap() - 0.6055).abs() < 1e-3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
