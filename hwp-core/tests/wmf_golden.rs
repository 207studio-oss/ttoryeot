use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use hwp_core::detect::{detect, Format};
use hwp_core::hwp5::{docinfo, record};
use hwp_core::hwpx::zip::MiniZip;
use hwp_core::{parse, Document, ParseOptions, Parsed, WarningKind};

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read corpus directory") {
        let entry = entry.expect("read corpus entry");
        let kind = entry.file_type().expect("read corpus entry type");
        if kind.is_dir() {
            collect_files(&entry.path(), files);
        } else if kind.is_file() {
            let extension = entry
                .path()
                .extension()
                .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
            if matches!(extension.as_deref(), Some("hwp") | Some("hwpx")) {
                files.push(entry.path());
            }
        }
    }
}

fn output_directory(repo: &Path, requested: &Path) -> Result<PathBuf, &'static str> {
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| "read working directory")?
            .join(requested)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(existing.file_name().ok_or("invalid output directory")?);
        existing = existing.parent().ok_or("invalid output directory")?;
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|_| "resolve output directory")?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    let repo = repo.canonicalize().map_err(|_| "resolve repository")?;
    if resolved.starts_with(&repo) {
        return Err("HWP_GOLDEN_OUT must be outside the repository");
    }
    Ok(resolved)
}

fn read_stream(file: &mut cfb::CompoundFile<Cursor<&[u8]>>, path: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.open_stream(path)
        .expect("open corpus stream")
        .read_to_end(&mut bytes)
        .expect("read corpus stream");
    bytes
}

fn check_bmp(bytes: &[u8]) {
    assert!(bytes.starts_with(b"BM"), "invalid BMP signature");
    assert!(bytes.len() >= 26, "truncated BMP header");
    let size = u32::from_le_bytes(bytes[2..6].try_into().expect("BMP size")) as usize;
    let offset = u32::from_le_bytes(bytes[10..14].try_into().expect("BMP offset")) as usize;
    let dib_size = u32::from_le_bytes(bytes[14..18].try_into().expect("DIB size")) as usize;
    assert!(size == bytes.len(), "invalid BMP size");
    assert!(bytes[6..10] == [0; 4], "invalid BMP reserved fields");
    assert!(
        matches!(dib_size, 12 | 40 | 52 | 56 | 108 | 124),
        "invalid DIB header"
    );
    assert!(
        offset >= 14 + dib_size && offset < size,
        "invalid BMP pixel offset"
    );
}

fn count_blob(
    parsed: &Parsed,
    id: &str,
    converted: &mut usize,
    vector: &mut usize,
    unsupported: &mut usize,
) {
    let blob = parsed
        .document
        .bins
        .iter()
        .find(|blob| blob.id == id)
        .expect("WMF blob retains its id");
    if blob.data.starts_with(b"BM") {
        check_bmp(&blob.data);
        *converted += 1;
    } else if parsed.warnings.iter().any(|warning| {
        warning.kind == WarningKind::UnsupportedObject
            && warning.detail == format!("{id}: WMF vector-only")
    }) {
        *vector += 1;
    } else {
        *unsupported += 1;
    }
}

#[test]
fn golden_wmf_conversion_counts() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root");
    let requested = std::env::var_os("HWP_GOLDEN_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/hwp_golden_out"));
    let output = output_directory(repo, &requested).expect("private golden output directory");
    let mut files = Vec::new();
    collect_files(Path::new(&dir), &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus has no HWP files");
    let (mut total, mut converted, mut vector, mut unsupported) = (0, 0, 0, 0);
    for path in files {
        let bytes = fs::read(&path).expect("read corpus input");
        match detect(&bytes) {
            Format::Cfb => {
                let Ok(mut file) = cfb::OpenOptions::new().open_with(Cursor::new(bytes.as_slice()))
                else {
                    continue;
                };
                if !file.is_stream("/FileHeader") || !file.is_stream("/DocInfo") {
                    continue;
                }
                let header = read_stream(&mut file, "/FileHeader");
                if header.len() < 40 || !header.starts_with(b"HWP Document File") {
                    continue;
                }
                let flags = u32::from_le_bytes(header[36..40].try_into().expect("flags"));
                let version = u32::from_le_bytes(header[32..36].try_into().expect("version"));
                let raw = read_stream(&mut file, "/DocInfo");
                let info = if flags & 1 != 0 {
                    let mut info = Vec::new();
                    flate2::read::DeflateDecoder::new(raw.as_slice())
                        .read_to_end(&mut info)
                        .expect("inflate corpus DocInfo");
                    info
                } else {
                    raw
                };
                let (records, _) = record::parse_checked(&info, None);
                let descriptors = docinfo::parse_doc_info(
                    &records,
                    version,
                    &mut Document::default(),
                    &mut Vec::new(),
                );
                let wmf_ids: Vec<_> = descriptors
                    .names
                    .iter()
                    .filter_map(|(index, name)| {
                        name.rsplit_once('.')
                            .filter(|(_, ext)| ext.eq_ignore_ascii_case("wmf"))
                            .map(|_| format!("bin{index}"))
                    })
                    .collect();
                if wmf_ids.is_empty() {
                    continue;
                }
                let parsed =
                    parse(&bytes, &ParseOptions::default()).expect("parse WMF corpus document");
                for id in wmf_ids {
                    total += 1;
                    count_blob(&parsed, &id, &mut converted, &mut vector, &mut unsupported);
                }
            }
            Format::Zip => {
                let Some(zip) = MiniZip::new(&bytes) else {
                    continue;
                };
                let wmf_entries: Vec<&str> = zip
                    .entries()
                    .iter()
                    .filter_map(|entry| {
                        if !entry.name.starts_with("BinData/") {
                            return None;
                        }
                        let base = entry.name.rsplit('/').next()?;
                        let (stem, extension) = base.rsplit_once('.')?;
                        extension.eq_ignore_ascii_case("wmf").then_some(stem)
                    })
                    .collect();
                if wmf_entries.is_empty() {
                    continue;
                }
                let parsed = parse(&bytes, &ParseOptions::default())
                    .expect("parse HWPX WMF corpus document");
                for id in wmf_entries {
                    total += 1;
                    count_blob(&parsed, id, &mut converted, &mut vector, &mut unsupported);
                }
            }
            _ => {}
        }
    }
    let report = serde_json::json!({
        "wmf": total, "converted": converted, "vector": vector, "unsupported": unsupported
    });
    let report_path = output_directory(repo, &output.join("wmf.json"))
        .expect("private golden report destination");
    fs::create_dir_all(&output).expect("create private output directory");
    fs::write(report_path, format!("{report}\n")).expect("write numeric WMF report");
    println!("{report}");
    assert_eq!(total, 8, "WMF corpus count changed");
    assert_eq!(unsupported, 0, "WMF is neither converted nor vector-only");
    assert_eq!(converted + vector, total);
}

#[test]
fn wmf_golden_rejects_repository_output() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root");
    for path in [
        repo.to_path_buf(),
        repo.join("evidence/wmf"),
        repo.join("hwp-core/../private-wmf"),
    ] {
        assert!(output_directory(repo, &path).is_err());
    }
    assert!(output_directory(repo, &std::env::temp_dir().join("hwp-f14-private-output")).is_ok());
}
