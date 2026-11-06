//! 문서가 열리는지 센다. 배포용·암호 문서는 따로 센다.

use crate::golden::Kind;
use hwp_core::hwpx::zip::MiniZip;
use hwp_core::{Fatal, ParseOptions, parse};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// HWP5 FileHeader 속성의 비트(HWP 5.0 명세 표 3).
const HWP5_PASSWORD: u32 = 0x2;
const HWP5_DISTRIBUTION: u32 = 0x4;
const HWPX_MANIFEST: &str = "META-INF/manifest.xml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    None,
    Distribution,
    Password,
}

pub struct Opened {
    pub kind: Kind,
    pub protection: Protection,
    pub result: Result<(), Fatal>,
}

pub fn open(path: &Path, kind: Kind, password: Option<&str>) -> Opened {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Opened {
                kind,
                protection: Protection::None,
                result: Err(Fatal::Internal(format!("read: {}", error.kind()))),
            };
        }
    };
    let options = ParseOptions {
        password: password.map(str::to_string),
        ..ParseOptions::default()
    };
    Opened {
        kind,
        protection: protection(&bytes),
        result: parse(&bytes, &options).map(|_| ()),
    }
}

/// 파서와 따로, 파일 머리만 보고 배포용·암호 문서인지 가린다.
fn protection(bytes: &[u8]) -> Protection {
    if let Some(flags) = hwp5_flags(bytes) {
        if flags & HWP5_PASSWORD != 0 {
            return Protection::Password;
        }
        if flags & HWP5_DISTRIBUTION != 0 {
            return Protection::Distribution;
        }
        return Protection::None;
    }
    let encrypted = MiniZip::new(bytes)
        .and_then(|zip| zip.read(HWPX_MANIFEST))
        .is_some_and(|manifest| contains(&manifest, b"encryption-data"));
    if encrypted {
        Protection::Password
    } else {
        Protection::None
    }
}

fn hwp5_flags(bytes: &[u8]) -> Option<u32> {
    let mut file = hwp_core::hwp5::container::open(bytes).ok()?;
    let mut header = Vec::new();
    file.open_stream("FileHeader")
        .ok()?
        .read_to_end(&mut header)
        .ok()?;
    let flags = header.get(36..40)?;
    Some(u32::from_le_bytes([flags[0], flags[1], flags[2], flags[3]]))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[derive(Serialize, Default)]
pub struct Counts {
    pub files: usize,
    pub opened: usize,
    pub distribution: Tally,
    pub password: Tally,
    pub fatal: BTreeMap<&'static str, usize>,
}

#[derive(Serialize, Default)]
pub struct Tally {
    pub opened: usize,
    pub total: usize,
}

#[derive(Serialize, Default)]
pub struct Report {
    pub all: Counts,
    pub by_format: BTreeMap<&'static str, Counts>,
}

impl Counts {
    fn add(&mut self, opened: &Opened) {
        self.files += 1;
        let ok = opened.result.is_ok();
        self.opened += usize::from(ok);
        let tally = match opened.protection {
            Protection::None => None,
            Protection::Distribution => Some(&mut self.distribution),
            Protection::Password => Some(&mut self.password),
        };
        if let Some(tally) = tally {
            tally.total += 1;
            tally.opened += usize::from(ok);
        }
        if let Err(fatal) = &opened.result {
            *self.fatal.entry(fatal.kind()).or_default() += 1;
        }
    }
}

impl Report {
    pub fn add(&mut self, opened: &Opened) {
        self.all.add(opened);
        self.by_format
            .entry(opened.kind.label())
            .or_default()
            .add(opened);
    }
}
