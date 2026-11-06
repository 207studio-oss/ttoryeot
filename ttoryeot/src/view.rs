//! 보기 전용 조회: 문서 정보, 쪽 범위, 텍스트 추출.

use std::collections::{BTreeSet, HashMap};

use hwp_core::{Document, Fatal, Format, Metadata, ParseOptions, Parsed, Warning};
use serde::Serialize;

use crate::fonts::FontBook;
use crate::layout::Renderer;
use crate::scene;
use crate::{Error, markdown, pdf};

/// 사람이 쓰는 쪽 범위. `1-3,5,7-`처럼 1부터 세고, 끝을 비우면 마지막 쪽까지다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSet(Vec<(usize, Option<usize>)>);

impl PageSet {
    pub fn parse(value: &str) -> Result<Self, String> {
        let number = |text: &str| {
            text.trim()
                .parse::<usize>()
                .ok()
                .filter(|page| *page > 0)
                .ok_or_else(|| format!("쪽 범위가 올바르지 않습니다: {value}"))
        };
        let mut ranges = Vec::new();
        for part in value.split(',') {
            let range = match part.split_once('-') {
                Some((first, "")) => (number(first)?, None),
                Some((first, last)) => (number(first)?, Some(number(last)?)),
                None => (number(part)?, Some(number(part)?)),
            };
            if range.1.is_some_and(|last| last < range.0) {
                return Err(format!("쪽 범위가 올바르지 않습니다: {value}"));
            }
            ranges.push(range);
        }
        Ok(Self(ranges))
    }

    /// 전체 `total`쪽에서 고른 쪽 번호(0부터). 정렬되어 있고 겹치지 않으며 문서 밖 쪽은 뺀다.
    pub fn select(&self, total: usize) -> Vec<usize> {
        let mut pages = BTreeSet::new();
        for (first, last) in &self.0 {
            let last = last.unwrap_or(total).min(total);
            pages.extend(*first - 1..last);
        }
        pages.into_iter().collect()
    }
}

fn selected(pages: Option<&PageSet>, total: usize) -> Vec<usize> {
    pages.map_or_else(|| (0..total).collect(), |set| set.select(total))
}

fn parse(bytes: &[u8], password: Option<&str>) -> Result<Parsed, Fatal> {
    hwp_core::parse(
        bytes,
        &ParseOptions {
            password: password.map(str::to_string),
            ..ParseOptions::default()
        },
    )
}

/// 본문 글자가 쓰는 글꼴 이름(글자마다 그 언어 칸이 가리키는 글꼴).
pub fn used_fonts(document: &Document) -> BTreeSet<String> {
    hwp_core::text::text_summary(document)
        .char_prs
        .into_iter()
        .filter_map(|(id, chars)| {
            let char_pr = document.char_prs.get(usize::try_from(id).ok()?)?;
            let langs: BTreeSet<_> = chars.into_iter().map(crate::lang_of).collect();
            Some(
                langs
                    .into_iter()
                    .filter_map(|lang| char_pr.font_ref.get(lang).map(|index| (lang, *index)))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .filter_map(|(lang, index)| {
            let faces = document.font_faces.get(lang)?;
            let face = faces.get(usize::try_from(index).ok()?)?;
            Some(face.name.clone())
        })
        .filter(|name| !name.is_empty())
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct UsedFont {
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentReport {
    pub format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub encrypted: bool,
    /// 암호를 몰라 본문을 열지 못했으면 false다. 이때 쪽 수와 글꼴은 비어 있다.
    pub opened: bool,
    pub distribution: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<usize>,
    pub metadata: Metadata,
    pub fonts: Vec<UsedFont>,
    pub warnings: Vec<Warning>,
}

/// 문서 정보를 모은다. 암호 때문에 열 수 없는 문서도 오류가 아니라 `opened: false`로 알린다.
pub fn inspect(
    bytes: &[u8],
    password: Option<&str>,
    fonts: &mut FontBook,
) -> Result<DocumentReport, Error> {
    let format = match hwp_core::detect(bytes) {
        Format::Zip => "hwpx",
        _ => "hwp5",
    };
    let parsed = match parse(bytes, password) {
        Ok(parsed) => parsed,
        Err(
            Fatal::PasswordProtected
            | Fatal::UnsupportedEncryption
            | Fatal::UnsupportedEncryptionDetail(_),
        ) => {
            return Ok(DocumentReport {
                format,
                version: None,
                encrypted: true,
                opened: false,
                distribution: false,
                pages: None,
                metadata: Metadata::default(),
                fonts: Vec::new(),
                warnings: Vec::new(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let Parsed {
        document,
        warnings,
        info,
    } = parsed;
    let pages = crate::render(&document, fonts).len();
    let fonts = used_fonts(&document)
        .into_iter()
        .map(|name| UsedFont {
            available: fonts.find(&name, false).is_some(),
            name,
        })
        .collect();
    Ok(DocumentReport {
        format,
        version: info.version,
        encrypted: info.encrypted,
        opened: true,
        distribution: info.distribution,
        pages: Some(pages),
        metadata: info.metadata,
        fonts,
        warnings,
    })
}

/// 고른 쪽만 담은 PDF.
pub struct SelectedPdf {
    pub pdf: Vec<u8>,
    pub page_count: usize,
    /// PDF에 담긴 쪽의 원래 번호(1부터). 비어 있으면 PDF도 비어 있다.
    pub pages: Vec<usize>,
    pub warnings: Vec<Warning>,
}

pub fn selected_pdf(
    bytes: &[u8],
    password: Option<&str>,
    fonts: &mut FontBook,
    pages: Option<&PageSet>,
) -> Result<SelectedPdf, Error> {
    let Parsed {
        document, warnings, ..
    } = parse(bytes, password)?;
    let all = crate::render(&document, fonts);
    drop(document);
    let page_count = all.len();
    let chosen = selected(pages, page_count);
    let numbers = chosen.iter().map(|page| page + 1).collect();
    let chosen: BTreeSet<usize> = chosen.into_iter().collect();
    let scene: Vec<scene::Page> = all
        .into_iter()
        .enumerate()
        .filter_map(|(index, page)| chosen.contains(&index).then_some(page))
        .collect();
    let pdf = if scene.is_empty() {
        Vec::new()
    } else {
        pdf::write(scene, fonts).map_err(Error::Pdf)?
    };
    Ok(SelectedPdf {
        pdf,
        page_count,
        pages: numbers,
        warnings,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    /// 쪽에 그려진 줄 그대로. 본문 다음에 각주, 머리말, 꼬리말, 쪽 번호가 오고, 쪽은 폼 피드로 나눈다.
    Plain,
    /// 문서 구조를 따른다. 머리말, 꼬리말은 빼고, 고른 쪽에 걸친 문단과 표는 통째로 넣는다.
    Markdown,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextOutput {
    pub page_count: usize,
    /// 고른 쪽 번호(1부터). 비어 있으면 범위 안에 쪽이 없다는 뜻이다.
    pub pages: Vec<usize>,
    pub text: String,
    pub warnings: Vec<Warning>,
}

fn plain_page(lines: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for line in lines {
        if !line.is_empty() || out.last().is_some_and(|last| !last.is_empty()) {
            out.push(line);
        }
    }
    while out.last().is_some_and(|last| last.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

pub fn extract_text(
    bytes: &[u8],
    password: Option<&str>,
    fonts: &mut FontBook,
    format: TextFormat,
    pages: Option<&PageSet>,
) -> Result<TextOutput, Error> {
    let Parsed {
        document, warnings, ..
    } = parse(bytes, password)?;
    fonts.attach_document(&document);
    let layout = Renderer::new(&document, fonts).recording_text().layout();
    let page_count = layout.pages.len();
    let chosen = selected(pages, page_count);
    let text = match format {
        TextFormat::Plain => chosen
            .iter()
            .map(|index| plain_page(&layout.text[*index]))
            .collect::<Vec<_>>()
            .join("\u{C}"),
        TextFormat::Markdown => {
            let wanted: BTreeSet<usize> = chosen.iter().copied().collect();
            let mut paragraphs: HashMap<usize, BTreeSet<usize>> = HashMap::new();
            for span in &layout.paragraphs {
                if wanted
                    .range(span.first_page..=span.last_page)
                    .next()
                    .is_some()
                {
                    paragraphs
                        .entry(span.section)
                        .or_default()
                        .insert(span.paragraph);
                }
            }
            markdown::write(&document, &paragraphs)
        }
    };
    Ok(TextOutput {
        page_count,
        pages: chosen.iter().map(|page| page + 1).collect(),
        text,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use hwp_core::{CharPr, FontFace, Paragraph, Run, RunItem, Section};

    use super::*;

    #[test]
    fn page_sets_clamp_sort_and_reject_bad_ranges() {
        let set = PageSet::parse("7-,1-3,2,5").unwrap();
        assert_eq!(set.select(8), [0, 1, 2, 4, 6, 7]);
        assert_eq!(set.select(2), [0, 1]);
        assert!(PageSet::parse("9").unwrap().select(2).is_empty());
        for bad in ["", "0", "3-2", "a", "1,,2", "-3"] {
            assert!(PageSet::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn used_fonts_lists_only_fonts_of_runs_with_text() {
        let char_pr = |face: i32| CharPr {
            font_ref: BTreeMap::from([("hangul".to_string(), face), ("hanja".to_string(), 3)]),
            ..CharPr::default()
        };
        let run = |char_pr_id: i32, text: &str| Run {
            char_pr_id,
            items: vec![RunItem::Text(text.to_string())],
            supscript: false,
        };
        let document = Document {
            font_faces: BTreeMap::from([
                (
                    "hangul".to_string(),
                    vec![
                        FontFace::new("쓰는 글꼴"),
                        FontFace::new("빈 칸 글꼴"),
                        FontFace::new("안 쓰는 글꼴"),
                    ],
                ),
                ("hanja".to_string(), vec![FontFace::new("한자 글꼴"); 4]),
            ]),
            char_prs: vec![char_pr(0), char_pr(1), char_pr(2)],
            sections: vec![Section {
                paragraphs: vec![Paragraph {
                    runs: vec![run(0, "가"), run(1, " ")],
                    ..Paragraph::default()
                }],
                ..Section::default()
            }],
            ..Document::default()
        };
        assert_eq!(
            used_fonts(&document),
            BTreeSet::from(["쓰는 글꼴".to_string()])
        );
    }
}
