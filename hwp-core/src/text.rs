use crate::model::{Caption, Document, RunItem};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextSummary {
    pub tables: usize,
    pub pictures: usize,
    pub paragraphs: Vec<String>,
    /// 글자 모양 번호마다 그 모양으로 쓴 글자(빈칸 제외).
    pub char_prs: std::collections::BTreeMap<i32, std::collections::BTreeSet<char>>,
}

pub fn text_summary(document: &Document) -> TextSummary {
    let mut summary = TextSummary::default();
    for section in &document.sections {
        paragraphs(&section.paragraphs, &mut summary);
    }
    summary
}

fn paragraphs(values: &[crate::model::Paragraph], summary: &mut TextSummary) {
    for paragraph in values {
        let mut line = String::new();
        for run in &paragraph.runs {
            for item in &run.items {
                if let RunItem::Text(text) = item {
                    let chars = text.chars().filter(|ch| !ch.is_whitespace());
                    let used = summary.char_prs.entry(run.char_pr_id).or_default();
                    used.extend(chars);
                }
            }
            summary.char_prs.retain(|_, chars| !chars.is_empty());
            for item in &run.items {
                item_text(item, &mut line, summary);
            }
        }
        summary.paragraphs.push(line);
        for memo in &paragraph.memos {
            paragraphs(&memo.paragraphs, summary);
        }
    }
}

fn item_text(item: &RunItem, line: &mut String, summary: &mut TextSummary) {
    match item {
        RunItem::Text(value) => line.push_str(value),
        RunItem::Compose(compose) => line.push_str(&compose.text),
        RunItem::Tab | RunItem::SizedTab(_) | RunItem::FormattedTab(_) => line.push('\t'),
        RunItem::Table(table) => {
            summary.tables += 1;
            caption(&table.caption, summary);
            for cell in &table.cells {
                paragraphs(&cell.paragraphs, summary);
            }
        }
        RunItem::Picture(picture) => {
            summary.pictures += 1;
            caption(&picture.caption, summary);
        }
        RunItem::Shape(shape) => {
            caption(&shape.caption, summary);
            paragraphs(&shape.paragraphs, summary);
            for child in &shape.children {
                item_text(&child.item, line, summary);
            }
        }
        RunItem::Note(note) => paragraphs(&note.paragraphs, summary),
        RunItem::Header {
            paragraphs: value, ..
        }
        | RunItem::Footer {
            paragraphs: value, ..
        } => {
            paragraphs(value, summary);
        }
        RunItem::LineBreak
        | RunItem::AutoNum(_)
        | RunItem::Control(_)
        | RunItem::Skip(_)
        | RunItem::PageHiding { .. }
        | RunItem::NewNum(_)
        | RunItem::PageNum { .. } => {}
    }
}

fn caption(caption: &Option<Caption>, summary: &mut TextSummary) {
    if let Some(caption) = caption {
        paragraphs(&caption.paragraphs, summary);
    }
}
