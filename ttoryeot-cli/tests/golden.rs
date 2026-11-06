use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use hwp_core::model::{
    Caption, Cell, Document, Note, Paragraph, Picture, Run, RunItem, Section, Shape, ShapeChild,
    Table,
};
use hwp_core::text::text_summary;

const SWIFT_KEY: &str = "swift@05f647b7";

#[test]
fn golden_text() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped");
        return;
    };
    let root = repo_root();
    let files = collect_files(Path::new(&dir));
    assert!(
        !files.is_empty(),
        "HWP_GOLDEN_DIR has no .hwp or .hwpx files"
    );

    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/hwp_golden");
    fs::create_dir_all(&target).expect("create golden target");

    let reference_path = target.join("ref.jsonl");
    let hash_path = target.join("ref.hash");
    let hash = corpus_hash(&files);
    let cached = fs::read_to_string(&hash_path)
        .map(|value| value.trim() == hash)
        .unwrap_or(false)
        && reference_path.is_file();
    if !cached {
        let reference = root.join("tools/hwp_golden/reference.py");
        let output = Command::new("python3")
            .arg(&reference)
            .args(&files)
            .output()
            .expect("run reference.py");
        assert!(
            output.status.success(),
            "reference.py failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        write_atomic(&reference_path, &output.stdout);
        write_atomic(&hash_path, format!("{hash}\n").as_bytes());
    }

    let rust_path = target.join("rust.jsonl");
    let dump = Command::new(env!("CARGO_BIN_EXE_hwp-dump"))
        .args(["--format", "text"])
        .args(&files)
        .output()
        .expect("run hwp-dump --format text");
    assert!(
        dump.status.success(),
        "hwp-dump --format text failed: {}",
        String::from_utf8_lossy(&dump.stderr)
    );
    write_atomic(&rust_path, &dump.stdout);

    let allow = load_fatal_allowlist(&root.join("tools/hwp_golden/fatal_allow.txt"));
    check_fatal_allowlist(&rust_path, &allow);
    let text_allow = load_text_allow(&root.join("tools/hwp_golden/text_allow.txt"));

    let baseline = root.join("tools/hwp_golden/baseline_text.json");
    let per_path = target.join("rust-per.json");
    let rust_summary = compare(
        &baseline,
        &reference_path,
        &rust_path,
        "rust",
        0.0005,
        Some(&per_path),
    );
    compare(&baseline, &reference_path, &rust_path, SWIFT_KEY, 0.0, None);
    check_per_document(&baseline, &per_path, &text_allow);
    println!("rust: {rust_summary}");
    println!("swift baseline: pass");
}

#[test]
fn text_dump_traverses_children_notes_headers() {
    let paragraph = |text: &str| Paragraph {
        runs: vec![Run {
            items: vec![RunItem::Text(text.to_string())],
            ..Run::default()
        }],
        ..Paragraph::default()
    };
    let caption = |text: &str| Caption {
        paragraphs: vec![paragraph(text)],
        ..Caption::default()
    };
    let table = Table {
        cells: vec![Cell {
            paragraphs: vec![paragraph("cell")],
            ..Cell::default()
        }],
        caption: Some(caption("table cap")),
        ..Table::default()
    };
    let picture = Picture {
        caption: Some(caption("picture cap")),
        ..Picture::default()
    };
    let child = Shape {
        paragraphs: vec![paragraph("child")],
        children: vec![ShapeChild {
            item: Box::new(RunItem::Picture(Picture::default())),
            frame: [[0.0, 0.0], [0.0, 0.0]],
            transform: None,
        }],
        ..Shape::default()
    };
    let shape = Shape {
        paragraphs: vec![paragraph("box")],
        caption: Some(caption("shape cap")),
        children: vec![ShapeChild {
            item: Box::new(RunItem::Shape(child)),
            frame: [[0.0, 0.0], [0.0, 0.0]],
            transform: None,
        }],
        ..Shape::default()
    };
    let body = Paragraph {
        runs: vec![Run {
            items: vec![
                RunItem::Text("body".to_string()),
                RunItem::Tab,
                RunItem::Table(table),
                RunItem::Picture(picture),
                RunItem::Shape(shape),
                RunItem::Note(Note {
                    paragraphs: vec![paragraph("note")],
                    ..Note::default()
                }),
                RunItem::Header {
                    paragraphs: vec![paragraph("head")],
                    applies: "BOTH".to_string(),
                },
                RunItem::Footer {
                    paragraphs: vec![paragraph("foot")],
                    applies: "BOTH".to_string(),
                },
            ],
            ..Run::default()
        }],
        ..Paragraph::default()
    };
    let document = Document {
        sections: vec![Section {
            paragraphs: vec![body],
            header: vec![paragraph("ignored")],
            footer: vec![paragraph("ignored")],
            ..Section::default()
        }],
        ..Document::default()
    };

    let summary = text_summary(&document);
    assert_eq!(summary.tables, 1);
    assert_eq!(summary.pictures, 2);
    assert_eq!(
        summary.paragraphs,
        [
            "table cap",
            "cell",
            "picture cap",
            "shape cap",
            "box",
            "child",
            "note",
            "head",
            "foot",
            "body\t"
        ]
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate is inside repository")
        .to_path_buf()
}

fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read HWP_GOLDEN_DIR")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_hwp(path))
        .collect();
    files.sort();
    files
}

fn is_hwp(path: &Path) -> bool {
    path.extension()
        .map(|extension| {
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            extension == "hwp" || extension == "hwpx"
        })
        .unwrap_or(false)
}

fn corpus_hash(files: &[PathBuf]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for path in files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        for byte in name
            .bytes()
            .chain(fs::read(path).expect("read corpus file"))
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

fn write_atomic(path: &Path, bytes: &[u8]) {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).expect("write golden cache");
    fs::rename(&temporary, path).expect("replace golden cache");
}

fn compare(
    baseline: &Path,
    reference: &Path,
    candidate: &Path,
    key: &str,
    tolerance: f64,
    per: Option<&Path>,
) -> String {
    let mut command = Command::new("python3");
    command
        .arg(repo_root().join("tools/hwp_golden/compare.py"))
        .arg(reference)
        .arg(candidate)
        .arg("--baseline")
        .arg(baseline)
        .arg("--baseline-key")
        .arg(key)
        .arg("--tolerance")
        .arg(tolerance.to_string());
    if let Some(per) = per {
        command.arg("--per").arg(per);
    }
    let output = command.output().expect("run compare.py");
    assert!(
        output.status.success(),
        "baseline {key} failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn load_fatal_allowlist(path: &Path) -> BTreeMap<String, String> {
    let mut allowed = BTreeMap::new();
    let text = fs::read_to_string(path).expect("read fatal allowlist");
    for line in text.lines() {
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

fn check_fatal_allowlist(candidate: &Path, allowed: &BTreeMap<String, String>) {
    let text = fs::read_to_string(candidate).expect("read rust dump");
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line).expect("parse rust dump line");
        if value["ok"].as_bool() == Some(true) {
            continue;
        }
        let file = value["file"].as_str().unwrap_or_default();
        let kind = value["fatal"]["kind"].as_str().unwrap_or_default();
        assert_eq!(
            allowed.get(file).map(String::as_str),
            Some(kind),
            "unexpected fatal for {file}: {kind}"
        );
    }
}

fn load_text_allow(path: &Path) -> BTreeSet<String> {
    let mut allowed = BTreeSet::new();
    for line in fs::read_to_string(path)
        .expect("read text allowlist")
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(file) = line.split_whitespace().next() {
            allowed.insert(file.to_string());
        }
    }
    allowed
}

fn check_per_document(baseline: &Path, current: &Path, allowed: &BTreeSet<String>) {
    let baseline: Value = serde_json::from_slice(&fs::read(baseline).expect("read baseline"))
        .expect("parse baseline");
    let current: Value = serde_json::from_slice(&fs::read(current).expect("read per-doc report"))
        .expect("parse per-doc report");
    let stored = baseline["rust"]["per"]
        .as_object()
        .expect("baseline rust.per");
    let current = current.as_object().expect("per-doc report object");
    let mut failures = Vec::new();
    for (file, values) in stored {
        if allowed.contains(file) {
            continue;
        }
        let previous = values[0].as_f64().unwrap_or(0.0);
        let now = current
            .get(file)
            .and_then(|values| values[0].as_f64())
            .unwrap_or(0.0);
        if now < previous - 0.01 {
            failures.push(format!("{file}: {now:.4} < {previous:.4}"));
        }
    }
    assert!(
        failures.is_empty(),
        "document text f1 regressions:\n{}",
        failures.join("\n")
    );
}
