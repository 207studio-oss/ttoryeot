//! 골든셋 읽기: 문서 목록과 한/글 기준 PDF 목록.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Hwp,
    Hwpx,
}

impl Kind {
    pub fn of(path: &Path) -> Option<Kind> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "hwp" => Some(Kind::Hwp),
            "hwpx" => Some(Kind::Hwpx),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Hwp => "hwp",
            Kind::Hwpx => "hwpx",
        }
    }
}

/// `dir` 아래의 .hwp·.hwpx 파일을 경로 순으로 모은다.
pub fn documents(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if Kind::of(&path).is_some() {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

#[derive(Deserialize)]
struct Manifest {
    docs: Vec<ManifestDoc>,
}

#[derive(Deserialize)]
struct ManifestDoc {
    input: String,
    reference: String,
    reference_source: Option<String>,
}

/// 기준 PDF가 있는 문서 하나.
pub struct Reference {
    pub document: PathBuf,
    pub pdf: PathBuf,
    pub kind: Kind,
    /// 기준 PDF를 만든 곳. 목록에 따로 적히지 않았으면 기본 출처다.
    pub source: String,
    /// 문서가 처음 나온 목록의 이름표(`--manifest`의 `이름=경로`).
    pub set: String,
}

pub const DEFAULT_SOURCE: &str = "manifest-default";
pub const DEFAULT_SET: &str = "manifest";

/// 목록마다 몇 개를 새로 더했고 몇 개는 앞 목록의 기준 PDF를 바꿨는지.
pub struct SetCount {
    pub set: String,
    pub added: usize,
    pub replaced: usize,
}

/// `spec`은 경로 구분자(Windows `;`)로 이은 `[이름=]목록 경로`들이다. 앞 목록부터 읽는다.
/// 뒤 목록에 이미 나온 문서가 또 있으면 그 기준 PDF와 출처만 바꾸고 자리와 이름표는 둔다(다시 만든 기준 PDF).
/// `.json`은 `docs[].input/reference/reference_source` manifest로 문서는 `documents_dir`, PDF는 manifest 옆에서 파일 이름으로 찾는다.
/// 그 밖은 탭으로 나눈 `pdf 상대경로, 문서 상대경로[, 쪽 수[, 출처]]` 줄 목록으로 PDF는 목록 폴더 기준, 문서는 `documents_dir`에서 파일 이름으로 찾는다.
pub fn references(
    spec: &str,
    documents_dir: &Path,
) -> Result<(Vec<Reference>, Vec<SetCount>), String> {
    let mut out: Vec<Reference> = Vec::new();
    let mut at: HashMap<PathBuf, usize> = HashMap::new();
    let mut counts = Vec::new();
    for part in std::env::split_paths(spec) {
        let part = part.to_string_lossy().into_owned();
        let (set, path) = match part.split_once('=') {
            Some((set, path)) if !set.is_empty() && !set.contains(['/', '\\', ':']) => {
                (set.to_string(), PathBuf::from(path))
            }
            _ => (DEFAULT_SET.to_string(), PathBuf::from(part)),
        };
        let listed = if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            json_entries(&path, documents_dir)
        } else {
            tab_entries(&path, documents_dir)
        }
        .map_err(|error| format!("{set}: {error}"))?;
        let mut count = SetCount {
            set: set.clone(),
            added: 0,
            replaced: 0,
        };
        for entry in listed {
            if let Some(&index) = at.get(&entry.document) {
                out[index].pdf = entry.pdf;
                out[index].source = entry.source;
                count.replaced += 1;
            } else {
                at.insert(entry.document.clone(), out.len());
                out.push(Reference {
                    set: set.clone(),
                    ..entry
                });
                count.added += 1;
            }
        }
        counts.push(count);
    }
    Ok((out, counts))
}

fn json_entries(manifest: &Path, documents_dir: &Path) -> Result<Vec<Reference>, String> {
    let text = fs::read_to_string(manifest).map_err(|error| format!("manifest: {error}"))?;
    let parsed: Manifest =
        serde_json::from_str(&text).map_err(|error| format!("manifest: {error}"))?;
    let pdf_dir = manifest.parent().unwrap_or(Path::new("."));
    parsed
        .docs
        .into_iter()
        .enumerate()
        .map(|(index, doc)| {
            entry(
                documents_dir.join(file_name(&doc.input)),
                pdf_dir.join(file_name(&doc.reference)),
                doc.reference_source,
            )
            .map_err(|error| format!("manifest doc #{index}: {error}"))
        })
        .collect()
}

fn tab_entries(list: &Path, documents_dir: &Path) -> Result<Vec<Reference>, String> {
    let text = fs::read_to_string(list).map_err(|error| format!("list: {error}"))?;
    let pdf_dir = list.parent().unwrap_or(Path::new("."));
    text.trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|(number, line)| {
            let columns: Vec<&str> = line.split('\t').map(str::trim).collect();
            let [pdf, document, rest @ ..] = columns.as_slice() else {
                return Err(format!("list line {}: needs pdf and document", number + 1));
            };
            let source = rest.get(1).filter(|text| !text.is_empty());
            entry(
                documents_dir.join(file_name(document)),
                pdf_dir.join(pdf),
                source.map(|text| text.to_string()),
            )
            .map_err(|error| format!("list line {}: {error}", number + 1))
        })
        .collect()
}

fn entry(document: PathBuf, pdf: PathBuf, source: Option<String>) -> Result<Reference, String> {
    let kind = Kind::of(&document).ok_or("not hwp/hwpx")?;
    if !document.is_file() || !pdf.is_file() {
        return Err("document or pdf missing".to_string());
    }
    Ok(Reference {
        document,
        pdf,
        kind,
        source: source.unwrap_or_else(|| DEFAULT_SOURCE.to_string()),
        set: DEFAULT_SET.to_string(),
    })
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ttoryeot-score-golden-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        fs::create_dir_all(dir.join("files")).unwrap();
        fs::create_dir_all(dir.join("json")).unwrap();
        fs::create_dir_all(dir.join("tab/pdfs")).unwrap();
        for name in ["a.hwp", "b.hwpx", "c.hwp"] {
            fs::write(dir.join("files").join(name), b"").unwrap();
        }
        for name in [
            "json/a.pdf",
            "json/b.pdf",
            "tab/pdfs/a.pdf",
            "tab/pdfs/c.pdf",
        ] {
            fs::write(dir.join(name), b"").unwrap();
        }
        fs::write(
            dir.join("json/manifest.json"),
            r#"{"docs": [
                {"input": "x/a.hwp", "reference": "a.pdf"},
                {"input": "b.hwpx", "reference": "b.pdf", "reference_source": "win"}
            ]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("tab/list.txt"),
            "\u{feff}# pdf_rel\tsource_rel\tref_pages\thangul_version\n\
             pdfs/c.pdf\tfiles/c.hwp\t3\t12.30 mac\n\
             pdfs/a.pdf\tfiles/a.hwp\t1\t12.30 mac\n",
        )
        .unwrap();
        dir
    }

    fn spec(parts: &[PathBuf]) -> String {
        std::env::join_paths(parts)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn later_list_adds_new_documents_and_replaces_earlier_pdfs_keeping_the_set() {
        let dir = scratch("merge");
        let spec = spec(&[
            PathBuf::from(format!(
                "tuned={}",
                dir.join("json/manifest.json").display()
            )),
            PathBuf::from(format!("heldout={}", dir.join("tab/list.txt").display())),
        ]);
        let (references, counts) = references(&spec, &dir.join("files")).unwrap();
        let summary: Vec<(String, String, String)> = references
            .iter()
            .map(|reference| {
                (
                    reference
                        .document
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into(),
                    reference.set.clone(),
                    reference.source.clone(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("a.hwp".into(), "tuned".into(), "12.30 mac".into()),
                ("b.hwpx".into(), "tuned".into(), "win".into()),
                ("c.hwp".into(), "heldout".into(), "12.30 mac".into()),
            ]
        );
        assert_eq!(references[0].pdf, dir.join("tab").join("pdfs/a.pdf"));
        let counts: Vec<(&str, usize, usize)> = counts
            .iter()
            .map(|count| (count.set.as_str(), count.added, count.replaced))
            .collect();
        assert_eq!(counts, [("tuned", 2, 0), ("heldout", 1, 1)]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unlabeled_json_manifest_keeps_the_default_set_and_source() {
        let dir = scratch("plain");
        let manifest = dir.join("json/manifest.json");
        let (references, _) = references(&manifest.to_string_lossy(), &dir.join("files")).unwrap();
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].set, DEFAULT_SET);
        assert_eq!(references[0].source, DEFAULT_SOURCE);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_row_without_its_document_is_an_error_with_the_line_number() {
        let dir = scratch("missing");
        fs::remove_file(dir.join("files/c.hwp")).unwrap();
        let list = dir.join("tab/list.txt");
        let error = references(&format!("heldout={}", list.display()), &dir.join("files"))
            .err()
            .unwrap();
        assert_eq!(error, "heldout: list line 2: document or pdf missing");
        fs::remove_dir_all(dir).unwrap();
    }
}
