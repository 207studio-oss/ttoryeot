use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use hwp_core::detect::{detect, Format};
use hwp_core::{parse, ParseOptions};
use serde::Deserialize;
use serde_json::Value;

const WRAPS: [&str; 6] = [
    "SQUARE",
    "TIGHT",
    "THROUGH",
    "TOP_AND_BOTTOM",
    "BEHIND_TEXT",
    "IN_FRONT_OF_TEXT",
];

fn load_fatal_allowlist() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .join("tools/hwp_golden/fatal_allow.txt");
    let mut allowed = BTreeMap::new();
    for line in fs::read_to_string(path)
        .expect("read fatal allowlist")
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        if let (Some(file), Some(kind)) = (parts.next(), parts.next()) {
            allowed.insert(file.to_string(), kind.to_string());
        }
    }
    allowed
}

fn count_wraps(value: &Value, counts: &mut [usize; 6]) {
    match value {
        Value::Object(fields) => {
            if let Some(wrap) = fields.get("textWrap") {
                let index = WRAPS
                    .iter()
                    .position(|name| wrap.as_str() == Some(name))
                    .expect("unknown text wrap in parsed position");
                counts[index] += 1;
            }
            for child in fields.values() {
                count_wraps(child, counts);
            }
        }
        Value::Array(children) => {
            for child in children {
                count_wraps(child, counts);
            }
        }
        _ => {}
    }
}

#[test]
fn golden_f9_text_wrap_distribution() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let dir = PathBuf::from(dir)
        .canonicalize()
        .expect("resolve corpus directory");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("read configured corpus directory")
        .map(|entry| entry.expect("read corpus entry").path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("hwp"))
        })
        .collect();
    files.sort();
    let allowed = load_fatal_allowlist();
    let mut counts = [0; 6];
    let mut documents = 0;
    for file in files {
        let file = file.canonicalize().expect("resolve corpus document");
        assert!(file.starts_with(&dir), "document is outside HWP_GOLDEN_DIR");
        let bytes = fs::read(&file).expect("read configured corpus document");
        if detect(&bytes) == Format::Hwp3 {
            continue;
        }
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parsed = match parse(&bytes, &ParseOptions::default()) {
            Ok(parsed) => parsed,
            Err(fatal) if allowed.get(&name).map(String::as_str) == Some(fatal.kind()) => continue,
            Err(fatal) => panic!("HWP5 parse failed: {}", fatal.kind()),
        };
        let value = serde_json::to_value(&parsed.document).expect("serialize parsed document");
        count_wraps(&value, &mut counts);
        documents += 1;
    }
    assert!(documents > 0, "no HWP5 documents were checked");
    assert!(
        counts[1..].iter().sum::<usize>() > 0,
        "textWrap is only SQUARE"
    );
    println!("documents={documents}");
    for (name, count) in WRAPS.iter().zip(counts) {
        println!("{name}={count}");
    }
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

fn scores(corpus: &Corpus) -> Result<BTreeMap<&str, f64>, &'static str> {
    let mut values = BTreeMap::new();
    for doc in &corpus.docs {
        if doc.id.is_empty() || doc.status != "ok" || !valid_score(doc.score, 100.0) {
            return Err("invalid or failed document result");
        }
        if values.insert(doc.id.as_str(), doc.score).is_some() {
            return Err("duplicate document id");
        }
    }
    if values.is_empty() {
        return Err("empty corpus report");
    }
    Ok(values)
}

fn valid_score(value: f64, max: f64) -> bool {
    value.is_finite() && (0.0..=max).contains(&value)
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
    for (previous, current, max) in [
        (old.summary.score, new.summary.score, 100.0),
        (
            old.summary.page_count_match_rate,
            new.summary.page_count_match_rate,
            1.0,
        ),
    ] {
        if !valid_score(previous, max) || !valid_score(current, max) || current < previous {
            return Err("summary score or page match rate regressed or is invalid");
        }
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let bytes = fs::read(path).expect("read configured golden input");
    serde_json::from_slice(&bytes).expect("decode configured golden input")
}

#[test]
fn golden_f9_mac_corpus() {
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
    let out = std::env::var_os("HWP_GOLDEN_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("hwp_golden_out"))
        .canonicalize()
        .expect("resolve existing golden reports directory");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    assert!(
        !out.starts_with(repo),
        "golden reports must be outside the repository"
    );
    let mut ids = BTreeSet::new();
    for input in manifest.docs {
        assert!(
            !input.id.is_empty() && ids.insert(input.id),
            "invalid or duplicate manifest id"
        );
        let path = if input.input.is_absolute() {
            input.input
        } else {
            manifest_path.parent().unwrap().join(input.input)
        }
        .canonicalize()
        .expect("resolve configured corpus document");
        assert!(
            path.is_file() && path.starts_with(&dir),
            "document is outside HWP_GOLDEN_DIR"
        );
    }
    let old: Corpus = read_json(&out.join("fid-swift/corpus.json"));
    let new: Corpus = read_json(&out.join("fid-rust/corpus.json"));
    check_gate(&old, &new, &ids).expect("F9 Mac corpus gate");
    println!("documents={}", ids.len());
}

#[test]
fn wrap_distribution_visits_nested_objects() {
    let value = serde_json::json!({
        "pos": {"textWrap": "SQUARE"},
        "paragraphs": [{"items": [
            {"pos": {"textWrap": "TOP_AND_BOTTOM"}},
            {"children": [{"pos": {"textWrap": "THROUGH"}}]}
        ]}]
    });
    let mut counts = [0; 6];
    count_wraps(&value, &mut counts);
    assert_eq!(counts, [1, 0, 1, 1, 0, 0]);
}

#[test]
fn mac_gate_thresholds_and_incomplete_results() {
    let sample = Corpus {
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
    assert!(check_gate(&sample, &sample, &ids).is_ok());
    let mut new = sample.clone();
    new.docs[0].score = 89.5;
    assert!(check_gate(&sample, &new, &ids).is_ok());
    for score in [89.49, f64::NAN, f64::INFINITY, -1.0, 101.0] {
        new.docs[0].score = score;
        assert!(check_gate(&sample, &new, &ids).is_err());
    }
    for field in ["score", "page_count_match_rate"] {
        new = sample.clone();
        if field == "score" {
            new.summary.score = 89.99;
        } else {
            new.summary.page_count_match_rate = 0.99;
        }
        assert!(check_gate(&sample, &new, &ids).is_err());
    }
    new = sample.clone();
    new.docs.clear();
    assert!(check_gate(&sample, &new, &ids).is_err());
    new = sample.clone();
    new.docs.push(new.docs[0].clone());
    assert!(check_gate(&sample, &new, &ids).is_err());
    new = sample.clone();
    new.docs[0].status = "render_fail".into();
    assert!(check_gate(&sample, &new, &ids).is_err());
    new = sample.clone();
    new.dpi = 300;
    assert!(check_gate(&sample, &new, &ids).is_err());
    new = sample.clone();
    new.font_dirs.push(PathBuf::from("synthetic-fonts"));
    assert!(check_gate(&sample, &new, &ids).is_err());
    assert!(check_gate(&sample, &sample, &BTreeSet::new()).is_err());
}
