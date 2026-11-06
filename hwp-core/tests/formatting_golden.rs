use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    docs: Vec<Input>,
}

#[derive(Deserialize)]
struct Input {
    id: String,
    input: PathBuf,
}

#[derive(Deserialize)]
struct Corpus {
    dpi: u32,
    font_dirs: Vec<PathBuf>,
    summary: Summary,
    docs: Vec<Score>,
}

#[derive(Deserialize)]
struct Summary {
    score: f64,
    page_count_match_rate: f64,
}

#[derive(Deserialize)]
struct Score {
    id: String,
    score: f64,
    status: String,
}

fn scores(corpus: &Corpus) -> Result<BTreeMap<&str, &Score>, &'static str> {
    let mut values = BTreeMap::new();
    for score in &corpus.docs {
        if !score.score.is_finite() || !(0.0..=100.0).contains(&score.score) {
            return Err("invalid document score");
        }
        if values.insert(score.id.as_str(), score).is_some() {
            return Err("duplicate document id");
        }
    }
    if values.is_empty() {
        return Err("empty corpus");
    }
    Ok(values)
}

fn check_gate(old: &Corpus, new: &Corpus, flow: &BTreeSet<String>) -> Result<(), &'static str> {
    if old.dpi != new.dpi || old.font_dirs != new.font_dirs {
        return Err("render conditions differ");
    }
    let old_scores = scores(old)?;
    let new_scores = scores(new)?;
    if old_scores.keys().ne(new_scores.keys()) {
        return Err("document sets differ");
    }
    if flow.is_empty() || flow.iter().any(|id| !new_scores.contains_key(id.as_str())) {
        return Err("flow documents are missing");
    }
    for (id, previous) in &old_scores {
        let current = new_scores[id];
        if current.score + 0.5 < previous.score {
            return Err("document score regressed by more than 0.5");
        }
        if flow.contains(*id) && current.status != "ok" {
            return Err("flow document rendering failed");
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
        if !previous.is_finite()
            || !current.is_finite()
            || !(0.0..=limit).contains(&previous)
            || !(0.0..=limit).contains(&current)
            || current < previous
        {
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
fn golden_f13_flow_layout() {
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
        .expect("resolve golden input directory");
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
    let old: Corpus = read_json(&out.join("fid-swift/corpus.json"));
    let new: Corpus = read_json(&out.join("fid-rust/corpus.json"));
    let mut flow = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for input in manifest.docs {
        assert!(ids.insert(input.id.clone()), "duplicate manifest id");
        let path = if input.input.is_absolute() {
            input.input
        } else {
            manifest_path.parent().unwrap().join(input.input)
        }
        .canonicalize()
        .expect("resolve configured corpus document");
        assert!(path.starts_with(&dir), "document is outside HWP_GOLDEN_DIR");
        let bytes = fs::read(path).expect("read configured corpus document");
        let parsed = hwp_core::parse(&bytes, &hwp_core::ParseOptions::default())
            .unwrap_or_else(|fatal| panic!("configured document parse failed: {}", fatal.kind()));
        let paragraphs: Vec<_> = parsed
            .document
            .sections
            .iter()
            .flat_map(|section| &section.paragraphs)
            .collect();
        if !paragraphs.is_empty()
            && paragraphs
                .iter()
                .all(|paragraph| paragraph.line_segs.is_empty())
        {
            flow.insert(input.id);
        }
    }
    let report_ids: BTreeSet<_> = new.docs.iter().map(|score| score.id.clone()).collect();
    assert_eq!(ids, report_ids, "manifest and report document sets differ");
    check_gate(&old, &new, &flow).expect("F13 Mac flow layout gate");
    println!("documents={},flow_documents={}", ids.len(), flow.len());
}

fn sample() -> Corpus {
    read_sample(
        90.0,
        1.0,
        vec![Score {
            id: "000000000001".to_string(),
            score: 90.0,
            status: "ok".to_string(),
        }],
    )
}

fn read_sample(score: f64, page_count_match_rate: f64, docs: Vec<Score>) -> Corpus {
    Corpus {
        dpi: 150,
        font_dirs: vec![],
        summary: Summary {
            score,
            page_count_match_rate,
        },
        docs,
    }
}

#[test]
fn flow_gate_thresholds_and_missing_results() {
    let flow = BTreeSet::from(["000000000001".to_string()]);
    assert!(check_gate(&sample(), &sample(), &flow).is_ok());
    let mut new = sample();
    new.docs[0].score = 89.5;
    assert!(check_gate(&sample(), &new, &flow).is_ok());
    new.docs[0].score = 89.49;
    assert!(check_gate(&sample(), &new, &flow).is_err());
    for (score, rate) in [(89.99, 1.0), (90.0, 0.99), (f64::NAN, 1.0)] {
        let mut new = sample();
        new.summary = Summary {
            score,
            page_count_match_rate: rate,
        };
        assert!(check_gate(&sample(), &new, &flow).is_err());
    }
    let mut new = sample();
    new.docs.clear();
    assert!(check_gate(&sample(), &new, &flow).is_err());
    let mut new = sample();
    new.docs[0].status = "render_fail".to_string();
    assert!(check_gate(&sample(), &new, &flow).is_err());
    assert!(check_gate(&sample(), &sample(), &BTreeSet::new()).is_err());
    let mut new = sample();
    new.dpi = 300;
    assert!(check_gate(&sample(), &new, &flow).is_err());
}
