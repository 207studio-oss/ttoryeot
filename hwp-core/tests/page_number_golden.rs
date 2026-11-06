use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use hwp_core::hwp5::record::Record;
use hwp_core::{parse, ParseOptions};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, PartialEq)]
struct RawControls {
    page_nums: BTreeMap<(u32, usize, u16), usize>,
    hiding: BTreeMap<u32, usize>,
    auto_nums: BTreeMap<u32, usize>,
}

fn collect_raw(records: &[Record], counts: &mut RawControls) -> usize {
    let mut page_nums = 0;
    for record in records {
        if record.tag == 71 {
            let mut reader = record.reader();
            let id = reader.u32().to_be_bytes();
            let attr = reader.u32();
            match &id {
                b"pgnp" => {
                    let _ = reader.u16();
                    let _ = reader.u16();
                    let _ = reader.u16();
                    let key = (attr, record.data.len(), reader.u16());
                    *counts.page_nums.entry(key).or_default() += 1;
                    page_nums += 1;
                }
                b"pghd" => *counts.hiding.entry(attr).or_default() += 1,
                b"atno" => *counts.auto_nums.entry(attr).or_default() += 1,
                _ => {}
            }
        }
        page_nums += collect_raw(&record.children, counts);
    }
    page_nums
}

fn count_page_nums(value: &Value) -> usize {
    match value {
        Value::Object(fields) => {
            usize::from(fields.get("t").and_then(Value::as_str) == Some("pageNum"))
                + fields.values().map(count_page_nums).sum::<usize>()
        }
        Value::Array(children) => children.iter().map(count_page_nums).sum(),
        _ => 0,
    }
}

fn document_page_nums(bytes: &[u8]) -> usize {
    let parsed = parse(bytes, &ParseOptions::default())
        .unwrap_or_else(|fatal| panic!("configured document parse failed: {}", fatal.kind()));
    parsed
        .document
        .sections
        .iter()
        .map(|section| {
            let paragraphs =
                serde_json::to_value(&section.paragraphs).expect("serialize paragraphs");
            count_page_nums(&paragraphs)
        })
        .sum()
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read configured corpus directory") {
        let entry = entry.expect("read corpus entry");
        let kind = entry.file_type().expect("read corpus entry type");
        if kind.is_dir() {
            collect_files(&entry.path(), files);
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("hwp"))
        {
            files.push(entry.path());
        }
    }
}

fn read_stream(file: &mut cfb::CompoundFile<Cursor<&[u8]>>, path: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.open_stream(path)
        .expect("open configured HWP5 stream")
        .read_to_end(&mut bytes)
        .expect("read configured HWP5 stream");
    bytes
}

#[test]
fn golden_f4_page_number_controls() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let dir = PathBuf::from(dir)
        .canonicalize()
        .expect("resolve corpus directory");
    let mut files = Vec::new();
    collect_files(&dir, &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus has no HWP files");
    let mut counts = RawControls::default();
    let mut documents = 0;
    for path in files {
        let path = path.canonicalize().expect("resolve corpus document");
        assert!(path.starts_with(&dir), "document is outside HWP_GOLDEN_DIR");
        let bytes = fs::read(path).expect("read configured corpus document");
        let Ok(mut file) = cfb::OpenOptions::new().open_with(Cursor::new(bytes.as_slice())) else {
            continue;
        };
        if !file.is_stream("/FileHeader") {
            continue;
        }
        let header = read_stream(&mut file, "/FileHeader");
        if header.len() < 40 || !header.starts_with(b"HWP Document File") {
            continue;
        }
        let flags = u32::from_le_bytes(header[36..40].try_into().expect("HWP5 flags"));
        if flags & 6 != 0 {
            continue;
        }
        let streams: Vec<String> = file
            .walk()
            .filter(|entry| entry.is_stream())
            .filter_map(|entry| {
                let name = entry
                    .path()
                    .components()
                    .filter_map(|component| match component {
                        Component::Normal(name) => Some(name.to_string_lossy()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let index = name.strip_prefix("BodyText/Section")?;
                (!index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
                    .then_some(format!("/{name}"))
            })
            .collect();
        let mut raw_page_nums = 0;
        for stream in streams {
            let mut data = read_stream(&mut file, &stream);
            if flags & 1 != 0 {
                let mut decoded = Vec::new();
                flate2::read::DeflateDecoder::new(data.as_slice())
                    .read_to_end(&mut decoded)
                    .expect("inflate HWP5 section");
                data = decoded;
            }
            let (records, warnings) = Record::parse_checked(&data, None);
            assert!(warnings.is_empty(), "truncated corpus section records");
            raw_page_nums += collect_raw(&records, &mut counts);
        }
        if raw_page_nums > 0 {
            assert_eq!(
                document_page_nums(&bytes),
                raw_page_nums,
                "pgnp controls were lost"
            );
            documents += 1;
        }
    }
    println!("documents={documents}");
    for ((attr, size, side), count) in &counts.page_nums {
        println!(
            "pgnp_attr={attr},size={size},format={},position={},side={side},count={count}",
            attr & 0xFF,
            (attr >> 8) & 0xF
        );
    }
    for (attr, count) in &counts.hiding {
        println!("pghd_attr={attr},count={count}");
    }
    for (attr, count) in &counts.auto_nums {
        println!(
            "atno_attr={attr},kind={},format={},supscript={},count={count}",
            attr & 0xF,
            (attr >> 4) & 0xFF,
            (attr >> 12) & 1
        );
    }
    assert_eq!(documents, 23);
    let mut page_nums = BTreeMap::new();
    for ((attr, size, side), count) in counts.page_nums {
        *page_nums.entry((attr & 0xFFF, size, side)).or_insert(0) += count;
    }
    assert_eq!(
        page_nums,
        BTreeMap::from([((0, 16, 45), 1), ((5 << 8, 16, 45), 37)])
    );
    assert_eq!(counts.hiding, BTreeMap::from([(32, 22)]));
    let mut auto_nums = BTreeMap::new();
    for (attr, count) in counts.auto_nums {
        *auto_nums.entry(attr & 0x1FFF).or_insert(0) += count;
    }
    assert_eq!(auto_nums, BTreeMap::from([(0, 23), (1, 1)]));
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
    score: f64,
    status: String,
}

fn valid(value: f64, limit: f64) -> bool {
    value.is_finite() && (0.0..=limit).contains(&value)
}

fn scores(corpus: &Corpus) -> Result<BTreeMap<&str, f64>, &'static str> {
    let mut scores = BTreeMap::new();
    for doc in &corpus.docs {
        if doc.id.is_empty()
            || doc.status != "ok"
            || !valid(doc.score, 100.0)
            || scores.insert(doc.id.as_str(), doc.score).is_some()
        {
            return Err("invalid, failed, or duplicate document result");
        }
    }
    if scores.is_empty() {
        return Err("empty corpus report");
    }
    Ok(scores)
}

fn check_gate(old: &Corpus, new: &Corpus, ids: &BTreeSet<String>) -> Result<(), &'static str> {
    if old.dpi != new.dpi || old.font_dirs != new.font_dirs {
        return Err("render conditions differ");
    }
    let old_scores = scores(old)?;
    let new_scores = scores(new)?;
    if old_scores.keys().ne(new_scores.keys())
        || new_scores
            .keys()
            .copied()
            .ne(ids.iter().map(String::as_str))
    {
        return Err("manifest and report document sets differ");
    }
    for (id, previous) in old_scores {
        if new_scores[id] + 0.5 < previous {
            return Err("document score regressed by more than 0.5");
        }
    }
    for (previous, current, limit) in [
        (old.summary.score, new.summary.score, 100.0),
        (
            old.summary.page_count_match_rate,
            new.summary.page_count_match_rate,
            1.0,
        ),
    ] {
        if !valid(previous, limit) || !valid(current, limit) || current < previous {
            return Err("summary score or page match rate regressed or is invalid");
        }
    }
    Ok(())
}

fn report_directory(requested: &Path) -> Result<PathBuf, &'static str> {
    let out = requested
        .canonicalize()
        .map_err(|_| "resolve existing golden reports directory")?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("repository root")?
        .canonicalize()
        .map_err(|_| "resolve repository")?;
    if out.starts_with(repo) {
        return Err("golden reports must be outside the repository");
    }
    Ok(out)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("read configured golden report"))
        .expect("decode configured golden report")
}

#[test]
fn golden_f4_mac_corpus() {
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
    let manifest_path = PathBuf::from(manifest)
        .canonicalize()
        .expect("resolve Hancom manifest");
    let manifest: Manifest = read_json(&manifest_path);
    let requested = std::env::var_os("HWP_GOLDEN_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/hwp_golden_out"));
    let out = report_directory(&requested).expect("private golden reports directory");
    let mut ids = BTreeSet::new();
    let mut page_number_documents = 0;
    for input in manifest.docs {
        assert!(
            !input.id.is_empty() && ids.insert(input.id),
            "invalid or duplicate manifest id"
        );
        let path = if input.input.is_absolute() {
            input.input
        } else {
            manifest_path
                .parent()
                .expect("manifest parent")
                .join(input.input)
        }
        .canonicalize()
        .expect("resolve configured corpus document");
        assert!(path.starts_with(&dir), "document is outside HWP_GOLDEN_DIR");
        let bytes = fs::read(path).expect("read configured corpus document");
        page_number_documents += usize::from(document_page_nums(&bytes) > 0);
    }
    assert!(
        page_number_documents > 0,
        "no page-number documents in Hancom set"
    );
    check_gate(
        &read_json(&out.join("fid-swift/corpus.json")),
        &read_json(&out.join("fid-rust/corpus.json")),
        &ids,
    )
    .expect("F4 Mac corpus gate");
    println!(
        "documents={},page_number_documents={page_number_documents}",
        ids.len()
    );
}

#[test]
fn raw_and_model_counts_visit_nested_controls() {
    let nested = Record {
        tag: 71,
        level: 1,
        data: [
            u32::from_be_bytes(*b"pgnp").to_le_bytes().as_slice(),
            &(5u32 << 8).to_le_bytes(),
            &[0, 0, 0, 0, 0, 0, 45, 0],
        ]
        .concat(),
        children: vec![],
    };
    let parent = Record {
        tag: 66,
        level: 0,
        data: vec![],
        children: vec![nested],
    };
    let mut counts = RawControls::default();
    assert_eq!(collect_raw(&[parent], &mut counts), 1);
    assert_eq!(counts.page_nums, BTreeMap::from([((5 << 8, 16, 45), 1)]));
    assert_eq!(
        count_page_nums(
            &serde_json::json!({"runs":[{"items":[{"t":"pageNum","v":{}},{"t":"header","v":{"paragraphs":[{"items":[{"t":"pageNum","v":{}}]}]}}]}]})
        ),
        2
    );
}

#[test]
fn mac_page_number_gate_and_private_reports() {
    assert!(report_directory(Path::new(env!("CARGO_MANIFEST_DIR"))).is_err());
    let old = Corpus {
        dpi: 150,
        font_dirs: vec![],
        summary: Summary {
            score: 90.0,
            page_count_match_rate: 1.0,
        },
        docs: vec![Score {
            id: "000000000001".into(),
            score: 90.0,
            status: "ok".into(),
        }],
    };
    let ids = BTreeSet::from(["000000000001".to_string()]);
    assert!(check_gate(&old, &old, &ids).is_ok());
    let mut new = old.clone();
    new.docs[0].score = 89.5;
    assert!(check_gate(&old, &new, &ids).is_ok());
    for score in [89.49, f64::NAN, f64::INFINITY, -1.0, 101.0] {
        new.docs[0].score = score;
        assert!(check_gate(&old, &new, &ids).is_err());
    }
    new = old.clone();
    new.summary.score = 89.99;
    assert!(check_gate(&old, &new, &ids).is_err());
    new = old.clone();
    new.summary.page_count_match_rate = 0.99;
    assert!(check_gate(&old, &new, &ids).is_err());
    new = old.clone();
    new.docs.clear();
    assert!(check_gate(&old, &new, &ids).is_err());
    new = old.clone();
    new.docs[0].status = "render_fail".into();
    assert!(check_gate(&old, &new, &ids).is_err());
    new = old.clone();
    new.docs.push(new.docs[0].clone());
    assert!(check_gate(&old, &new, &ids).is_err());
    new = old.clone();
    new.dpi = 300;
    assert!(check_gate(&old, &new, &ids).is_err());
    new = old.clone();
    new.font_dirs.push("synthetic-fonts".into());
    assert!(check_gate(&old, &new, &ids).is_err());
    assert!(check_gate(&old, &old, &BTreeSet::new()).is_err());
}
