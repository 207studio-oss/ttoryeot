#[path = "support/mod.rs"]
mod support;

use std::collections::BTreeSet;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use hwp_core::detect::{detect, Format};
use hwp_core::hwp5::record::Record;
use hwp_core::hwpx::{dom, zip::MiniZip};
use hwp_core::{parse, ParseOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Serialize)]
struct Distribution {
    hwp5: [[usize; 2]; 2],
    hwpx: [[usize; 2]; 2],
    hwpx_missing_attribute: [usize; 2],
    skipped: usize,
    documents: usize,
}

fn hwp5_cells(records: &[Record], counts: &mut Distribution) {
    for record in records {
        if record.tag == 71 && record.data.starts_with(b" lbt") {
            if let Some(index) = record.children.iter().position(|child| child.tag == 77) {
                let table = &record.children[index].data;
                assert!(table.len() >= 18, "short corpus TABLE record");
                for cell in record.children[index + 1..]
                    .iter()
                    .filter(|child| child.tag == 72)
                {
                    assert!(cell.data.len() >= 34, "short corpus cell LIST_HEADER");
                    let attr = u32::from_le_bytes(cell.data[4..8].try_into().unwrap());
                    let specified = usize::from(attr & 0x0001_0000 != 0);
                    let differs = usize::from(cell.data[24..32] != table[10..18]);
                    counts.hwp5[specified][differs] += 1;
                }
            }
        }
        hwp5_cells(&record.children, counts);
    }
}

fn xml_margins(node: Option<&dom::Node>) -> [f64; 4] {
    ["left", "right", "top", "bottom"]
        .map(|name| node.and_then(|node| node.num(name)).unwrap_or(0.0))
}

fn hwpx_cells(node: &dom::Node, counts: &mut Distribution) {
    if node.name == "tbl" {
        let inherited = xml_margins(node.first("inMargin"));
        for cell in node.all("tr").flat_map(|row| row.all("tc")) {
            let own = cell.first("cellMargin");
            if let Some(attr) = cell.attr("hasMargin") {
                let specified = usize::from(matches!(attr, "1" | "true"));
                let differs = usize::from(xml_margins(own) != inherited);
                counts.hwpx[specified][differs] += 1;
            } else {
                counts.hwpx_missing_attribute[usize::from(own.is_some())] += 1;
            }
        }
    }
    for child in &node.children {
        hwpx_cells(child, counts);
    }
}

fn raw_hwp5(bytes: &[u8], counts: &mut Distribution) -> bool {
    let Ok(mut file) = cfb::OpenOptions::new().open_with(Cursor::new(bytes)) else {
        return false;
    };
    let mut header = Vec::new();
    let Ok(mut stream) = file.open_stream("/FileHeader") else {
        return false;
    };
    stream.read_to_end(&mut header).expect("read HWP5 header");
    drop(stream);
    if header.len() < 40 || !header.starts_with(b"HWP Document File") {
        return false;
    }
    let flags = u32::from_le_bytes(header[36..40].try_into().unwrap());
    if flags & 6 != 0 {
        return false;
    }
    let streams: Vec<_> = file
        .walk()
        .filter(|entry| entry.is_stream())
        .filter_map(|entry| {
            let path = entry.path().to_path_buf();
            let parent = path.parent()?.file_name()?;
            let suffix = path.file_name()?.to_str()?.strip_prefix("Section")?;
            (parent == "BodyText"
                && !suffix.is_empty()
                && suffix.bytes().all(|byte| byte.is_ascii_digit()))
            .then_some(path)
        })
        .collect();
    assert!(!streams.is_empty(), "HWP5 corpus document has no sections");
    for path in streams {
        let mut data = Vec::new();
        file.open_stream(path)
            .expect("open HWP5 section")
            .read_to_end(&mut data)
            .expect("read HWP5 section");
        if flags & 1 != 0 {
            let mut decoded = Vec::new();
            flate2::read::DeflateDecoder::new(data.as_slice())
                .read_to_end(&mut decoded)
                .expect("inflate HWP5 section");
            data = decoded;
        }
        let (records, warnings) = Record::parse_checked(&data, None);
        assert!(warnings.is_empty(), "truncated HWP5 corpus records");
        hwp5_cells(&records, counts);
    }
    true
}

fn check_inherited(value: &Value) -> usize {
    match value {
        Value::Object(fields) => {
            let mut cells = 0;
            if let Some(inherited) = fields.get("inMargins") {
                for cell in fields["cells"].as_array().expect("table cells") {
                    if cell["hasMargin"] == false {
                        assert_eq!(cell["margins"], *inherited, "cell did not inherit margins");
                    }
                    cells += 1;
                }
            }
            cells + fields.values().map(check_inherited).sum::<usize>()
        }
        Value::Array(values) => values.iter().map(check_inherited).sum(),
        _ => 0,
    }
}

fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read configured corpus directory") {
        let entry = entry.expect("read corpus entry");
        let kind = entry.file_type().expect("read corpus entry type");
        if kind.is_dir() {
            files(&entry.path(), found);
        } else if kind.is_file()
            && entry.path().extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("hwp") || ext.eq_ignore_ascii_case("hwpx")
            })
        {
            found.push(entry.path());
        }
    }
}

fn repository() -> PathBuf {
    let git = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("locate repository");
    assert!(git.status.success(), "locate repository");
    PathBuf::from(
        String::from_utf8(git.stdout)
            .expect("repository path encoding")
            .trim(),
    )
}

fn output_path(repo: &Path, requested: &Path) -> Result<PathBuf, &'static str> {
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| "read working directory")?
            .join(requested)
    };
    let mut normalized = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(part.as_os_str()),
        }
    }
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(existing.file_name().ok_or("invalid output path")?);
        existing = existing.parent().ok_or("invalid output path")?;
    }
    let mut resolved = existing.canonicalize().map_err(|_| "resolve output path")?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    if resolved.starts_with(repo.canonicalize().map_err(|_| "resolve repository")?) {
        return Err("HWP_GOLDEN_OUT must be outside the repository");
    }
    Ok(resolved)
}

fn configured_output() -> PathBuf {
    output_path(
        &repository(),
        &std::env::var_os("HWP_GOLDEN_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("hwp_golden_out")),
    )
    .expect("private golden output directory")
}

#[test]
fn golden_f10_margin_distribution() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let out = configured_output();
    let dir = PathBuf::from(dir)
        .canonicalize()
        .expect("resolve corpus directory");
    let mut paths = Vec::new();
    files(&dir, &mut paths);
    paths.sort();
    let mut counts = Distribution::default();
    for path in paths {
        let path = path.canonicalize().expect("resolve corpus document");
        assert!(path.starts_with(&dir), "document is outside HWP_GOLDEN_DIR");
        let bytes = fs::read(path).expect("read corpus document");
        match detect(&bytes) {
            Format::Cfb if raw_hwp5(&bytes, &mut counts) => {}
            Format::Zip => {
                let zip = MiniZip::new(&bytes).expect("read HWPX ZIP");
                if zip.read("META-INF/manifest.xml").is_some_and(|manifest| {
                    manifest
                        .windows(b"encryption-data".len())
                        .any(|part| part == b"encryption-data")
                }) {
                    counts.skipped += 1;
                    continue;
                }
                for entry in zip.entries() {
                    if entry
                        .name
                        .strip_prefix("Contents/section")
                        .and_then(|name| name.strip_suffix(".xml"))
                        .is_some_and(|index| {
                            !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
                        })
                    {
                        let xml = zip.read_entry(entry).expect("read HWPX section");
                        let root = dom::parse(&xml).expect("parse HWPX section XML");
                        hwpx_cells(&root, &mut counts);
                    }
                }
            }
            _ => {
                counts.skipped += 1;
                continue;
            }
        }
        let parsed = parse(&bytes, &ParseOptions::default())
            .unwrap_or_else(|fatal| panic!("configured corpus parse failed: {}", fatal.kind()));
        let value =
            serde_json::to_value(parsed.document).expect("serialize document for margin checks");
        check_inherited(&value);
        counts.documents += 1;
    }
    assert!(counts.documents > 0, "no corpus documents checked");
    for bins in [&counts.hwp5, &counts.hwpx] {
        assert!(
            bins[0][1] > 0 && bins[1][1] > 0,
            "missing specified or inherited differing margins"
        );
    }
    fs::create_dir_all(&out).expect("create private output directory");
    fs::write(
        out.join("f10-margins.json"),
        serde_json::to_vec(&counts).unwrap(),
    )
    .expect("write numeric margin distribution");
    println!("documents={}", counts.documents);
}

#[derive(Clone, Deserialize)]
struct Corpus {
    dpi: u32,
    font_dirs: Vec<PathBuf>,
    summary: Summary,
    docs: Vec<Score>,
}

#[derive(Clone, Deserialize)]
struct Summary {
    score: f64,
    page_count_match_rate: f64,
}

#[derive(Clone, Deserialize)]
struct Score {
    id: String,
    status: String,
    score: f64,
}

#[derive(Deserialize)]
struct Manifest {
    docs: Vec<Input>,
}

#[derive(Deserialize)]
struct Input {
    id: String,
    input: PathBuf,
}

fn mac_gate(old: &Corpus, new: &Corpus, ids: &BTreeSet<String>) -> Result<(), &'static str> {
    if ids.is_empty() || old.dpi != new.dpi || old.font_dirs != new.font_dirs {
        return Err("empty manifest or render conditions differ");
    }
    for report in [old, new] {
        let mut seen = BTreeSet::new();
        for doc in &report.docs {
            if !seen.insert(doc.id.clone())
                || doc.status != "ok"
                || !(0.0..=100.0).contains(&doc.score)
            {
                return Err("duplicate, failed or invalid document result");
            }
        }
        if seen != *ids
            || !(0.0..=100.0).contains(&report.summary.score)
            || !(0.0..=1.0).contains(&report.summary.page_count_match_rate)
        {
            return Err("manifest mismatch or invalid summary");
        }
    }
    for doc in &old.docs {
        let current = new
            .docs
            .iter()
            .find(|current| current.id == doc.id)
            .unwrap();
        if current.score + 0.5 < doc.score {
            return Err("document score regressed by more than 0.5");
        }
    }
    if new.summary.score < old.summary.score
        || new.summary.page_count_match_rate < old.summary.page_count_match_rate
    {
        return Err("summary score or page count match rate regressed");
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("read configured golden report"))
        .expect("decode configured golden report")
}

#[test]
fn golden_f10_mac_corpus() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let Some(manifest) = std::env::var_os("HWP_GOLDEN_HANCOM") else {
        println!("skipped: HWP_GOLDEN_HANCOM is unset; Mac reports are required");
        return;
    };
    let dir = PathBuf::from(dir)
        .canonicalize()
        .expect("resolve corpus directory");
    let manifest = PathBuf::from(manifest)
        .canonicalize()
        .expect("resolve Hancom manifest");
    let inputs: Manifest = read_json(&manifest);
    let mut ids = BTreeSet::new();
    for input in inputs.docs {
        assert!(
            !input.id.is_empty() && ids.insert(input.id),
            "invalid or duplicate manifest id"
        );
        let path = if input.input.is_absolute() {
            input.input
        } else {
            manifest.parent().unwrap().join(input.input)
        };
        let path = path.canonicalize().expect("resolve corpus document");
        assert!(
            path.is_file() && path.starts_with(&dir),
            "document is outside HWP_GOLDEN_DIR"
        );
    }
    let out = configured_output();
    mac_gate(
        &read_json(&out.join("fid-swift/corpus.json")),
        &read_json(&out.join("fid-rust/corpus.json")),
        &ids,
    )
    .expect("F10 Mac corpus gate");
    println!("documents={}", ids.len());
}

#[test]
fn margin_report_counts_raw_flags_and_nested_cells() {
    let mut table = vec![0; 18];
    for (index, margin) in [10i16, 20, 30, 40].iter().enumerate() {
        table[10 + index * 2..12 + index * 2].copy_from_slice(&margin.to_le_bytes());
    }
    let cell = |attr: u32, equal: bool| {
        let mut data = vec![0; 34];
        data[4..8].copy_from_slice(&attr.to_le_bytes());
        if equal {
            data[24..32].copy_from_slice(&table[10..18]);
        }
        Record {
            tag: 72,
            level: 2,
            data,
            children: vec![],
        }
    };
    let children = vec![
        cell(0, false),
        Record {
            tag: 77,
            level: 2,
            data: table.clone(),
            children: vec![],
        },
        cell(0, true),
        cell(0, false),
        cell(1 << 16, true),
        cell(1 << 16, false),
        cell((1 << 17) | (1 << 18), false),
    ];
    let nested = Record {
        tag: 71,
        level: 1,
        data: b" lbt".to_vec(),
        children,
    };
    let mut counts = Distribution::default();
    hwp5_cells(
        &[Record {
            tag: 66,
            level: 0,
            data: vec![],
            children: vec![nested],
        }],
        &mut counts,
    );
    assert_eq!(counts.hwp5, [[1, 2], [1, 1]]);
    let xml = br#"<sec><tbl><inMargin left="10"/><tr>
<tc hasMargin="0"><cellMargin left="20"/></tc>
<tc hasMargin="1"><cellMargin left="20"/></tc>
<tc hasMargin="true"><cellMargin left="10"/></tc>
<tc/><tc><cellMargin left="20"/></tc>
</tr></tbl></sec>"#;
    hwpx_cells(&dom::parse(xml).unwrap(), &mut counts);
    assert_eq!(counts.hwpx, [[0, 1], [1, 1]]);
    assert_eq!(counts.hwpx_missing_attribute, [1, 1]);
    let value = serde_json::json!({"nested": [{
        "inMargins": {"left": 10}, "cells": [{"hasMargin": false, "margins": {"left": 10}}]
    }]});
    assert_eq!(check_inherited(&value), 1);
}

#[test]
fn margin_report_reads_compressed_and_plain_hwp5_sections() {
    let mut table = vec![0; 18];
    table[10..12].copy_from_slice(&10i16.to_le_bytes());
    let mut cell = vec![0; 34];
    cell[4..8].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    let mut body = support::rec::record(71, 0, b" lbt");
    body.extend(support::rec::record(77, 1, &table));
    body.extend(support::rec::record(72, 1, &cell));
    for flags in [0u32, 1, 2, 4] {
        let mut header = vec![0; 40];
        header[..17].copy_from_slice(b"HWP Document File");
        header[36..40].copy_from_slice(&flags.to_le_bytes());
        let section = if flags & 1 == 0 {
            body.clone()
        } else {
            support::cfbw::compress(&body)
        };
        let bytes = support::cfbw::build(&[
            ("FileHeader".into(), header),
            ("BodyText/Section0".into(), section),
            ("BodyText/NotASection".into(), vec![0xFF]),
        ]);
        let mut counts = Distribution::default();
        assert_eq!(raw_hwp5(&bytes, &mut counts), flags & 6 == 0);
        assert_eq!(counts.hwp5[1][1], usize::from(flags & 6 == 0));
    }
}

#[test]
fn margin_report_rejects_repository_output() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert!(output_path(repo, &repo.join("not-created/f10")).is_err());
    assert!(output_path(repo, &repo.join("hwp-core/../not-created/f10")).is_err());
    assert!(output_path(repo, &repo.join("../f10-private-report")).is_ok());
}

#[test]
fn margin_mac_gate_checks_thresholds_and_complete_results() {
    let old = Corpus {
        dpi: 150,
        font_dirs: vec![],
        summary: Summary {
            score: 90.0,
            page_count_match_rate: 1.0,
        },
        docs: vec![Score {
            id: "1".into(),
            status: "ok".into(),
            score: 90.0,
        }],
    };
    let ids = BTreeSet::from(["1".to_string()]);
    assert!(mac_gate(&old, &old, &ids).is_ok());
    let mut new = old.clone();
    new.docs[0].score = 89.5;
    assert!(mac_gate(&old, &new, &ids).is_ok());
    for score in [89.49, f64::NAN, f64::INFINITY, -1.0, 101.0] {
        new.docs[0].score = score;
        assert!(mac_gate(&old, &new, &ids).is_err());
    }
    for invalid in 0..8 {
        new = old.clone();
        match invalid {
            0 => new.docs.clear(),
            1 => new.docs.push(new.docs[0].clone()),
            2 => new.docs[0].status = "render_fail".into(),
            3 => new.summary.score = 89.99,
            4 => new.summary.page_count_match_rate = 0.99,
            5 => new.summary.score = f64::NAN,
            6 => new.dpi = 300,
            _ => new.font_dirs.push("synthetic-fonts".into()),
        }
        assert!(mac_gate(&old, &new, &ids).is_err());
    }
    assert!(mac_gate(&old, &old, &BTreeSet::new()).is_err());
}
