mod columns;
mod crypt;
pub mod dom;
pub mod header;
mod markpen;
mod page_border_fill;
pub mod section;
mod shape;
pub mod zip;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::info::{non_empty, DocumentInfo, Metadata};
use crate::model::{Blob, BlobKind, Document};
use crate::report::{Fatal, Warning, WarningKind};
use crate::{ParseOptions, Parsed};

const HEADER_PATH: &str = "Contents/header.xml";
const CONTENT_PATH: &str = "Contents/content.hpf";
const SECTION_PREFIX: &str = "Contents/section";
const MASTER_PAGE_PREFIX: &str = "Contents/masterpage";
const BIN_DATA_PREFIX: &str = "BinData/";

pub fn parse(bytes: &[u8], options: &ParseOptions) -> Result<Parsed, Fatal> {
    let zip = zip::MiniZip::new(bytes).ok_or(Fatal::NotHwp)?;
    let mut warnings = Vec::new();
    let opened = crypt::unlock(&zip, options.password.as_deref(), &mut warnings)?;
    let encrypted = !opened.is_empty();
    let zip = Package { zip, opened };
    // HWPX 줄 배치(lineseg textpos)도 HWP 5처럼 글자를 UTF-16 칸으로, 탭·컨트롤·개체를 8칸으로 센다.
    let mut document = Document {
        wchar_positions: true,
        ..Document::default()
    };
    if let Some(data) = zip.read(HEADER_PATH) {
        match dom::parse(&data) {
            Some(root) => {
                if root.depth_limited {
                    warnings.push(Warning::new(
                        WarningKind::NestingTooDeep,
                        None,
                        "header XML depth limit reached; nested content omitted",
                    ));
                }
                header::parse_header(&root, &mut document, &mut warnings);
            }
            None => warnings.push(Warning::new(
                WarningKind::DocInfoUnreadable,
                None,
                "Contents/header.xml is not valid XML",
            )),
        }
    } else {
        warnings.push(Warning::new(
            WarningKind::DocInfoUnreadable,
            None,
            "Contents/header.xml is missing",
        ));
    }

    let sections = section_entries(&zip);
    if sections.is_empty() {
        return Err(Fatal::NoReadableSection);
    }
    warn_section_gaps(&sections, &mut warnings);
    check_section_count(&zip, sections.len(), &mut warnings);
    check_manifest(&zip, &sections, &mut warnings);
    load_bin_data(&zip, &mut document, &mut warnings);

    let master_pages = master_page_entries(&zip);
    let mut parsed_sections = Vec::new();
    for (index, name) in &sections {
        let section = Some(*index);
        let Some(data) = zip.read(name) else {
            warnings.push(Warning::new(
                WarningKind::SectionUnreadable,
                section,
                format!("{name} is unreadable"),
            ));
            continue;
        };
        let Some((parsed, track_changes)) = section::read_section(
            &data,
            section,
            usize::from(options.max_depth),
            &master_pages,
            &mut warnings,
        ) else {
            warnings.push(Warning::new(
                WarningKind::InvalidXml,
                section,
                format!("{name} is not valid XML"),
            ));
            continue;
        };
        document.track_changes |= track_changes;
        parsed_sections.push(parsed);
    }
    if parsed_sections.is_empty() {
        return Err(Fatal::NoReadableSection);
    }
    document.sections = parsed_sections;
    let info = DocumentInfo {
        version: read_version(&zip),
        encrypted,
        distribution: false,
        metadata: read_metadata(&zip),
    };
    Ok(Parsed {
        document,
        warnings,
        info,
    })
}

fn read_version(zip: &Package<'_>) -> Option<String> {
    let data = zip.read("version.xml")?;
    let root = dom::parse(&data)?;
    let part = |name: &str| root.attr(name).unwrap_or("0").to_string();
    root.attr("major")?;
    Some(
        ["major", "minor", "micro", "buildNumber"]
            .map(part)
            .join("."),
    )
}

fn read_metadata(zip: &Package<'_>) -> Metadata {
    let mut metadata = Metadata::default();
    let Some(root) = zip.read(CONTENT_PATH).and_then(|data| dom::parse(&data)) else {
        return metadata;
    };
    metadata.title = root
        .find("title")
        .and_then(|node| non_empty(&node.all_text()));
    let mut nodes = Vec::new();
    root.find_all("meta", &mut nodes);
    for node in nodes {
        let slot = match node.attr("name") {
            Some("subject") => &mut metadata.subject,
            Some("creator") => &mut metadata.author,
            Some("keyword") => &mut metadata.keywords,
            Some("description") => &mut metadata.comments,
            Some("lastsaveby") => &mut metadata.last_author,
            Some("CreatedDate") => &mut metadata.created,
            Some("ModifiedDate") => &mut metadata.modified,
            _ => continue,
        };
        *slot = non_empty(&node.all_text());
    }
    metadata
}

/// 압축 꾸러미와 그 안에서 암호를 푼 항목. 암호가 걸린 항목은 푼 내용으로만 읽는다.
struct Package<'a> {
    zip: zip::MiniZip<'a>,
    opened: BTreeMap<String, Option<Vec<u8>>>,
}

impl Package<'_> {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        match self.opened.get(name) {
            Some(opened) => opened.clone(),
            None => self.zip.read(name),
        }
    }

    fn entries(&self) -> &[zip::Entry] {
        self.zip.entries()
    }
}

/// 바탕쪽 파일들을 `id`로 찾을 수 있게 읽어 둔다. `id`가 없으면 파일 이름으로 찾는다.
fn master_page_entries(zip: &Package<'_>) -> BTreeMap<String, dom::Node> {
    zip.entries()
        .iter()
        .filter(|entry| entry.name.starts_with(MASTER_PAGE_PREFIX) && entry.name.ends_with(".xml"))
        .filter_map(|entry| {
            let root = dom::parse(&zip.read(&entry.name)?)?;
            let id = root.attr("id").map(str::to_string).unwrap_or_else(|| {
                entry.name["Contents/".len()..entry.name.len() - ".xml".len()].to_string()
            });
            Some((id, root))
        })
        .collect()
}

fn section_entries(zip: &Package<'_>) -> Vec<(u32, String)> {
    let mut sections: Vec<(u32, String)> = zip
        .entries()
        .iter()
        .filter_map(|entry| {
            let suffix = entry.name.strip_prefix(SECTION_PREFIX)?;
            let suffix = suffix.strip_suffix(".xml")?;
            if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let index = suffix.parse::<u32>().ok()?;
            if entry.name != format!("{SECTION_PREFIX}{index}.xml") {
                return None;
            }
            Some((index, entry.name.clone()))
        })
        .collect();
    sections.sort_by_key(|(index, _)| *index);
    sections
}

fn warn_section_gaps(sections: &[(u32, String)], warnings: &mut Vec<Warning>) {
    let mut next = 0u32;
    for (index, _) in sections {
        if *index > next {
            let last = index.saturating_sub(1);
            let missing = u64::from(*index) - u64::from(next);
            warnings.push(Warning::new(
                WarningKind::SectionMissing,
                Some(next),
                format!("Contents/section{next}.xml..section{last}.xml missing ({missing})"),
            ));
        }
        next = index.saturating_add(1);
    }
}

fn check_section_count(zip: &Package<'_>, actual: usize, warnings: &mut Vec<Warning>) {
    let Some(data) = zip.read(HEADER_PATH) else {
        return;
    };
    let Some(root) = dom::parse(&data) else {
        return;
    };
    let Some(declared) = root.int("secCnt") else {
        return;
    };
    if declared < 0 || declared as usize != actual {
        warnings.push(Warning::new(
            WarningKind::CountMismatch,
            None,
            format!("header.xml declares {declared} section(s), found {actual}"),
        ));
    }
}

struct ManifestItem {
    id: String,
    href: String,
    media_type: Option<String>,
}

fn read_manifest(zip: &Package<'_>) -> Option<Vec<ManifestItem>> {
    let data = zip.read(CONTENT_PATH)?;
    let root = dom::parse(&data)?;
    let mut nodes = Vec::new();
    root.find_all("item", &mut nodes);
    Some(
        nodes
            .into_iter()
            .filter_map(|node| {
                Some(ManifestItem {
                    id: node.attr("id")?.to_string(),
                    href: node.attr("href")?.to_string(),
                    media_type: node.attr("media-type").map(str::to_string),
                })
            })
            .collect(),
    )
}

fn check_manifest(zip: &Package<'_>, sections: &[(u32, String)], warnings: &mut Vec<Warning>) {
    let Some(items) = read_manifest(zip) else {
        return;
    };
    let declared: Vec<String> = items
        .iter()
        .filter_map(|item| {
            let href = &item.href;
            if href.starts_with(SECTION_PREFIX) && href.ends_with(".xml") {
                Some(href.clone())
            } else if href.starts_with("section") && href.ends_with(".xml") {
                Some(format!("Contents/{href}"))
            } else {
                None
            }
        })
        .collect();
    if declared.is_empty() {
        return;
    }
    let mut declared = declared;
    declared.sort();
    let mut found: Vec<String> = sections.iter().map(|(_, name)| name.clone()).collect();
    found.sort();
    if declared != found {
        warnings.push(Warning::new(
            WarningKind::CountMismatch,
            None,
            format!(
                "content.hpf declares {} section(s), found {}",
                declared.len(),
                found.len()
            ),
        ));
    }
}

fn load_bin_data(zip: &Package<'_>, document: &mut Document, warnings: &mut Vec<Warning>) {
    let items = read_manifest(zip).unwrap_or_default();
    let linked_refs: BTreeSet<String> = document
        .font_faces
        .values()
        .flatten()
        .filter_map(|face| face.binary_ref.clone())
        .collect();
    // HWPX에서 글꼴로 쓸 수 있는 BinData는 `isEmbedded`인 얼굴이 가리키는 항목뿐이다.
    // `isEmbedded="0"`인 얼굴의 binaryItemIDRef는 글꼴 포함 선언이 아니다.
    let font_refs: BTreeSet<String> = document
        .font_faces
        .values()
        .flatten()
        .filter(|face| face.embedded)
        .filter_map(|face| face.binary_ref.clone())
        .collect();
    let mut loaded = BTreeSet::new();
    let mut loaded_hrefs = BTreeSet::new();
    for item in &items {
        if !item.href.starts_with(BIN_DATA_PREFIX) {
            continue;
        }
        let Some(data) = zip.read(&item.href) else {
            warnings.push(Warning::new(
                WarningKind::MissingBinData,
                None,
                format!("{} is missing", item.href),
            ));
            continue;
        };
        if loaded.insert(item.id.clone()) {
            // 글꼴 얼굴이 가리키는 항목은 isEmbedded일 때만 글꼴이다. 가리키지 않는
            // 항목만 media-type·확장자로 판별한다.
            let is_font = if linked_refs.contains(&item.id) {
                font_refs.contains(&item.id)
            } else {
                item.media_type
                    .as_deref()
                    .is_some_and(|media_type| media_type.to_ascii_lowercase().contains("font"))
                    || crate::model::is_font_file(&item.href)
            };
            let (kind, data) = if is_font {
                (BlobKind::Font, data)
            } else {
                (
                    BlobKind::Picture,
                    crate::wmf::picture_data(data, &item.href, &item.id, warnings),
                )
            };
            document.bins.push(Blob {
                id: item.id.clone(),
                kind,
                data: Arc::new(data),
            });
            loaded_hrefs.insert(item.href.clone());
        }
    }
    for entry in zip.entries() {
        if !entry.name.starts_with(BIN_DATA_PREFIX) || loaded_hrefs.contains(&entry.name) {
            continue;
        }
        let Some(base) = entry.name.rsplit('/').next() else {
            continue;
        };
        let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);
        if stem.is_empty() || loaded.contains(stem) {
            continue;
        }
        let Some(data) = zip.read(&entry.name) else {
            continue;
        };
        loaded.insert(stem.to_string());
        // 글꼴 얼굴 선언이 하나라도 있으면 embedded로 선언된 항목만 글꼴이다. 확장자만으로
        // `isEmbedded="0"` 항목을 글꼴로 승격하지 않는다. 선언이 전혀 없는 문서에서만
        // 확장자로 판별한다.
        let is_font = font_refs.contains(stem)
            || (linked_refs.is_empty() && crate::model::is_font_file(&entry.name));
        let (kind, data) = if is_font {
            (BlobKind::Font, data)
        } else {
            (
                BlobKind::Picture,
                crate::wmf::picture_data(data, &entry.name, stem, warnings),
            )
        };
        document.bins.push(Blob {
            id: stem.to_string(),
            kind,
            data: Arc::new(data),
        });
    }
}

pub(crate) fn narrow(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
