use crate::model::{Caption, Document, Paragraph, RunItem};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ShapeFillDistribution {
    pub shapes: usize,
    pub solid: usize,
    pub gradient: usize,
    pub image: usize,
    pub unfilled: usize,
}

pub fn shape_fill_distribution(document: &Document) -> ShapeFillDistribution {
    let mut counts = ShapeFillDistribution::default();
    for section in &document.sections {
        paragraphs(&section.paragraphs, &mut counts);
        paragraphs(&section.header, &mut counts);
        paragraphs(&section.footer, &mut counts);
    }
    counts
}

fn paragraphs(values: &[Paragraph], counts: &mut ShapeFillDistribution) {
    for paragraph in values {
        for item in paragraph.runs.iter().flat_map(|run| &run.items) {
            visit(item, counts);
        }
        for memo in &paragraph.memos {
            paragraphs(&memo.paragraphs, counts);
        }
    }
}

fn caption(value: &Option<Caption>, counts: &mut ShapeFillDistribution) {
    if let Some(value) = value {
        paragraphs(&value.paragraphs, counts);
    }
}

fn visit(item: &RunItem, counts: &mut ShapeFillDistribution) {
    match item {
        RunItem::Shape(shape) => {
            counts.shapes += 1;
            counts.solid += usize::from(shape.fill_color.is_some());
            counts.gradient += usize::from(shape.gradient.is_some());
            counts.image += usize::from(shape.image_fill.is_some());
            counts.unfilled += usize::from(
                shape.fill_color.is_none()
                    && shape.gradient.is_none()
                    && shape.image_fill.is_none(),
            );
            paragraphs(&shape.paragraphs, counts);
            caption(&shape.caption, counts);
            for child in &shape.children {
                visit(&child.item, counts);
            }
        }
        RunItem::Table(table) => {
            caption(&table.caption, counts);
            for cell in &table.cells {
                paragraphs(&cell.paragraphs, counts);
            }
        }
        RunItem::Picture(picture) => caption(&picture.caption, counts),
        RunItem::Note(note) => paragraphs(&note.paragraphs, counts),
        RunItem::Header {
            paragraphs: values, ..
        }
        | RunItem::Footer {
            paragraphs: values, ..
        } => paragraphs(values, counts),
        RunItem::Text(_)
        | RunItem::Tab
        | RunItem::SizedTab(_)
        | RunItem::FormattedTab(_)
        | RunItem::LineBreak
        | RunItem::AutoNum(_)
        | RunItem::Control(_)
        | RunItem::Compose(_)
        | RunItem::Skip(_)
        | RunItem::PageHiding { .. }
        | RunItem::NewNum(_)
        | RunItem::PageNum { .. } => {}
    }
}
