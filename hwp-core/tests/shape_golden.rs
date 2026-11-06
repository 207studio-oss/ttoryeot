use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use hwp_core::probe::shape_fill_distribution;
use hwp_core::{parse, ParseOptions};
use serde::Serialize;

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
    let repo = repo.canonicalize().map_err(|_| "resolve repository")?;
    if resolved.starts_with(repo) {
        return Err("HWP_GOLDEN_OUT must be outside the repository");
    }
    Ok(resolved)
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

#[derive(Default, Serialize)]
struct Counts {
    documents: usize,
    parsed: usize,
    fatal: usize,
    shapes: usize,
    solid: usize,
    gradient: usize,
    image: usize,
    unfilled: usize,
}

#[test]
fn golden_f12_shape_fill_distribution() {
    let Some(dir) = std::env::var_os("HWP_GOLDEN_DIR") else {
        println!("skipped: HWP_GOLDEN_DIR is unset");
        return;
    };
    let git = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("locate repository");
    assert!(git.status.success(), "locate repository");
    let repo = PathBuf::from(
        String::from_utf8(git.stdout)
            .expect("repository path")
            .trim(),
    );
    let requested = std::env::var_os("HWP_GOLDEN_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/hwp_golden_out"));
    let destination =
        output_path(&repo, &requested.join("shape-fills.json")).expect("private golden output");
    let dir = PathBuf::from(dir)
        .canonicalize()
        .expect("resolve configured corpus directory");
    let mut inputs = Vec::new();
    files(&dir, &mut inputs);
    inputs.sort();
    assert!(!inputs.is_empty(), "no HWP/HWPX corpus inputs");
    let mut report: BTreeMap<&str, Counts> = BTreeMap::new();
    for path in inputs {
        let path = path.canonicalize().expect("resolve corpus input");
        assert!(
            path.starts_with(&dir),
            "input outside configured corpus directory"
        );
        let format = if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("hwpx"))
        {
            "hwpx"
        } else {
            "hwp5"
        };
        let counts = report.entry(format).or_default();
        counts.documents += 1;
        let bytes = fs::read(path).expect("read corpus input");
        match parse(&bytes, &ParseOptions::default()) {
            Ok(parsed) => {
                counts.parsed += 1;
                let fills = shape_fill_distribution(&parsed.document);
                counts.shapes += fills.shapes;
                counts.solid += fills.solid;
                counts.gradient += fills.gradient;
                counts.image += fills.image;
                counts.unfilled += fills.unfilled;
            }
            Err(_) => counts.fatal += 1,
        }
    }
    let report = serde_json::to_string(&report).expect("serialize numeric shape report");
    fs::create_dir_all(destination.parent().expect("output parent"))
        .expect("create private output directory");
    fs::write(destination, format!("{report}\n")).expect("write numeric shape report");
    println!("{report}");
}

#[test]
fn shape_report_rejects_repository_output() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for requested in [
        repo.to_path_buf(),
        repo.join("evidence/f12"),
        repo.join("hwp-core/../shape-fills.json"),
    ] {
        assert!(output_path(repo, &requested).is_err());
    }
    assert!(output_path(repo, &std::env::temp_dir().join("hwp-f12-output")).is_ok());
}
