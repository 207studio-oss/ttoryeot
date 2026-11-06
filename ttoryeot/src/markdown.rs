//! 문서 모델을 markdown으로 옮긴다. 모양이 아니라 구조만 남긴다.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use hwp_core::{Document, ParaPr, Paragraph, RunItem, Table};

/// 이보다 칸이 많은 표는 격자 대신 줄글로 적는다. 손상된 파일의 큰 행렬 번호를 막는다.
const MAX_GRID_CELLS: usize = 1 << 20;
const MAX_HEADING_LEVEL: i32 = 6;

/// `wanted`는 구역 번호별로 넣을 최상위 문단 번호다.
pub(crate) fn write(document: &Document, wanted: &HashMap<usize, BTreeSet<usize>>) -> String {
    let mut writer = Writer {
        para_prs: document.para_prs.iter().map(|pr| (pr.id, pr)).collect(),
        blocks: Vec::new(),
        notes: Vec::new(),
    };
    for (index, section) in document.sections.iter().enumerate() {
        let Some(paragraphs) = wanted.get(&index) else {
            continue;
        };
        for paragraph in paragraphs {
            if let Some(paragraph) = section.paragraphs.get(*paragraph) {
                writer.paragraph(paragraph);
            }
        }
    }
    let mut out = writer.blocks.join("\n\n");
    for (index, note) in writer.notes.iter().enumerate() {
        out.push_str(&format!("\n\n[^{}]: {note}", index + 1));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

struct Writer<'a> {
    para_prs: HashMap<i32, &'a ParaPr>,
    blocks: Vec<String>,
    notes: Vec<String>,
}

impl Writer<'_> {
    fn paragraph(&mut self, paragraph: &Paragraph) {
        let mut line = String::new();
        if let Some(head) = paragraph
            .head_text
            .as_deref()
            .filter(|head| !head.is_empty())
        {
            line.push_str(&escape(head));
            line.push(' ');
        }
        let mut blocks = Vec::new();
        for run in &paragraph.runs {
            for item in &run.items {
                match item {
                    RunItem::Table(table) => {
                        blocks.push(self.table(table));
                        if let Some(caption) = &table.caption {
                            blocks.push(self.flat(&caption.paragraphs));
                        }
                    }
                    RunItem::Shape(_) => {
                        let mut text = String::new();
                        self.inline(item, &mut text);
                        blocks.push(text);
                    }
                    _ => self.inline(item, &mut line),
                }
            }
        }
        let line = line.trim();
        if !line.is_empty() {
            self.blocks.push(self.heading(paragraph) + line);
        }
        self.blocks
            .extend(blocks.into_iter().filter(|block| !block.trim().is_empty()));
    }

    fn heading(&self, paragraph: &Paragraph) -> String {
        match self.para_prs.get(&paragraph.para_pr_id) {
            Some(pr) if pr.heading_type == "OUTLINE" => {
                let level = pr
                    .heading_level
                    .saturating_add(1)
                    .clamp(1, MAX_HEADING_LEVEL);
                format!("{} ", "#".repeat(level as usize))
            }
            _ => String::new(),
        }
    }

    /// 한 줄 안에 적을 수 있도록 납작하게 옮긴다.
    fn inline(&mut self, item: &RunItem, out: &mut String) {
        match item {
            RunItem::Text(text) => out.push_str(&escape(text)),
            RunItem::Compose(compose) => out.push_str(&escape(&compose.text)),
            RunItem::Tab | RunItem::SizedTab(_) | RunItem::FormattedTab(_) => out.push(' '),
            RunItem::LineBreak => out.push_str("  \n"),
            RunItem::Table(table) => {
                let cells: Vec<String> = table
                    .cells
                    .iter()
                    .map(|cell| self.flat(&cell.paragraphs))
                    .collect();
                out.push_str(&cells.join(" / "));
            }
            RunItem::Picture(picture) => {
                let caption = picture
                    .caption
                    .as_ref()
                    .map(|caption| self.flat(&caption.paragraphs))
                    .filter(|caption| !caption.is_empty());
                match caption {
                    Some(caption) => out.push_str(&format!("[그림: {caption}]")),
                    None => out.push_str("[그림]"),
                }
            }
            RunItem::Shape(shape) => {
                let mut parts = Vec::new();
                if let Some(equation) = &shape.equation {
                    parts.push(format!("`{}`", equation.script.replace('`', "'")));
                }
                if let Some(caption) = &shape.caption {
                    parts.push(self.flat(&caption.paragraphs));
                }
                parts.push(self.flat(&shape.paragraphs));
                for child in &shape.children {
                    let mut text = String::new();
                    self.inline(&child.item, &mut text);
                    parts.push(text);
                }
                parts.retain(|part| !part.trim().is_empty());
                out.push_str(&parts.join(" "));
            }
            RunItem::Note(note) => {
                let text = self.flat(&note.paragraphs);
                self.notes.push(text);
                out.push_str(&format!("[^{}]", self.notes.len()));
            }
            RunItem::AutoNum(_)
            | RunItem::Control(_)
            | RunItem::Skip(_)
            | RunItem::Header { .. }
            | RunItem::Footer { .. }
            | RunItem::PageHiding { .. }
            | RunItem::NewNum(_)
            | RunItem::PageNum { .. } => {}
        }
    }

    fn flat(&mut self, paragraphs: &[Paragraph]) -> String {
        let mut lines = Vec::new();
        for paragraph in paragraphs {
            let mut line = String::new();
            for run in &paragraph.runs {
                for item in &run.items {
                    self.inline(item, &mut line);
                }
            }
            let line = line.trim().to_string();
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines.join("<br>").replace("  \n", "<br>")
    }

    fn table(&mut self, table: &Table) -> String {
        let mut grid: BTreeMap<(i32, i32), String> = BTreeMap::new();
        for cell in &table.cells {
            let text = self.flat(&cell.paragraphs).replace('|', "\\|");
            grid.insert((cell.row, cell.col), text);
        }
        let rows: BTreeSet<i32> = grid.keys().map(|key| key.0).collect();
        let cols: BTreeSet<i32> = grid.keys().map(|key| key.1).collect();
        if rows.len().saturating_mul(cols.len()) > MAX_GRID_CELLS {
            return grid.into_values().collect::<Vec<_>>().join("\n\n");
        }
        let mut lines = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let cells: Vec<&str> = cols
                .iter()
                .map(|col| grid.get(&(*row, *col)).map_or("", String::as_str))
                .collect();
            lines.push(format!("| {} |", cells.join(" | ")));
            if index == 0 {
                lines.push(format!("|{}", " --- |".repeat(cols.len())));
            }
        }
        lines.join("\n")
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '&') {
            out.push('\\');
        }
        out.push(ch);
    }
    if out.starts_with(['#', '>', '-', '+', '|', '~', '=']) {
        out.insert(0, '\\');
    }
    out
}

#[cfg(test)]
mod tests {
    use hwp_core::{Cell, Note, Run, Section};

    use super::*;

    fn text(value: &str) -> Paragraph {
        Paragraph {
            runs: vec![Run {
                items: vec![RunItem::Text(value.to_string())],
                ..Run::default()
            }],
            ..Paragraph::default()
        }
    }

    fn with(item: RunItem) -> Paragraph {
        Paragraph {
            runs: vec![Run {
                items: vec![item],
                ..Run::default()
            }],
            ..Paragraph::default()
        }
    }

    fn document(paragraphs: Vec<Paragraph>) -> Document {
        Document {
            sections: vec![Section {
                paragraphs,
                ..Section::default()
            }],
            ..Document::default()
        }
    }

    fn all(document: &Document) -> String {
        let count = document.sections[0].paragraphs.len();
        write(document, &HashMap::from([(0, (0..count).collect())]))
    }

    #[test]
    fn outline_paragraphs_become_headings_and_text_is_escaped() {
        let mut title = text("제목*");
        title.para_pr_id = 1;
        let mut doc = document(vec![title, text("# 본문 [a]")]);
        doc.para_prs = vec![ParaPr {
            id: 1,
            heading_type: "OUTLINE".to_string(),
            heading_level: 1,
            ..ParaPr::default()
        }];
        assert_eq!(all(&doc), "## 제목\\*\n\n\\# 본문 \\[a\\]\n");
    }

    #[test]
    fn tables_become_grids_with_escaped_pipes() {
        let cell = |row: i32, col: i32, value: &str| Cell {
            row,
            col,
            paragraphs: vec![text(value)],
            ..Cell::default()
        };
        let table = Table {
            cells: vec![cell(0, 0, "가"), cell(0, 1, "나|다"), cell(1, 0, "라")],
            ..Table::default()
        };
        assert_eq!(
            all(&document(vec![with(RunItem::Table(table))])),
            "| 가 | 나\\|다 |\n| --- | --- |\n| 라 |  |\n"
        );
    }

    #[test]
    fn notes_move_to_the_end_and_unwanted_paragraphs_are_left_out() {
        let note = RunItem::Note(Note {
            paragraphs: vec![text("각주")],
            ..Note::default()
        });
        let doc = document(vec![text("앞"), with(note), text("뒤")]);
        let wanted = HashMap::from([(0, BTreeSet::from([1]))]);
        assert_eq!(write(&doc, &wanted), "[^1]\n\n[^1]: 각주\n");
    }

    #[test]
    fn oversized_grids_fall_back_to_paragraphs() {
        let cells = (0..1100)
            .map(|index| Cell {
                row: index,
                col: index,
                paragraphs: vec![text("칸")],
                ..Cell::default()
            })
            .collect();
        let table = Table {
            cells,
            ..Table::default()
        };
        let out = all(&document(vec![with(RunItem::Table(table))]));
        assert!(!out.contains('|'));
    }
}
