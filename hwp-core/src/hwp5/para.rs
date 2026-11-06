use crate::model::{LineSeg, Memo, Paragraph, RangeTag, Run, RunItem};
use crate::report::{Warning, WarningKind};
use crate::HwpUnit;

use super::columns::parse_columns;
use super::control::{
    auto_number_supscript, list_groups, parse_control, parse_nested, ParseContext,
};
use super::docinfo::SHAPE_LINE_TYPES;
use super::record::Record;
use super::tags::{
    T_CTRL_HEADER, T_MEMO_LIST, T_PARA_CHAR_SHAPE, T_PARA_LINE_SEG, T_PARA_RANGE_TAG, T_PARA_TEXT,
};

const TAB_KINDS: [&str; 4] = ["LEFT", "RIGHT", "CENTER", "DECIMAL"];

fn char_pr_at(shape_at: &[(u32, i32)], position: u32) -> i32 {
    let mut id = shape_at.first().map(|entry| entry.1).unwrap_or(0);
    for &(start, shape) in shape_at {
        if start <= position {
            id = shape;
        }
    }
    id
}

fn flush_text(buffer: &mut String, run: &mut Run) {
    if !buffer.is_empty() {
        run.items.push(RunItem::Text(std::mem::take(buffer)));
    }
}

fn append_control(item: RunItem, supscript: bool, current: &mut Run, runs: &mut Vec<Run>) {
    if supscript && matches!(item, RunItem::Text(_)) {
        let char_pr_id = current.char_pr_id;
        let finished = std::mem::replace(
            current,
            Run {
                char_pr_id,
                ..Run::default()
            },
        );
        if !finished.items.is_empty() {
            runs.push(finished);
        }
        runs.push(Run {
            char_pr_id,
            items: vec![item],
            supscript: true,
        });
    } else {
        current.items.push(item);
    }
}

fn warn_lone_surrogate(unit: u16, context: &mut ParseContext<'_>) {
    let section_index = context.section_index;
    let count = &mut context.state.lone_surrogate_count;
    *count += 1;
    if *count <= 100 {
        context.warnings.push(Warning::new(
            WarningKind::LoneSurrogate,
            section_index,
            format!("unpaired UTF-16 surrogate U+{unit:04X}"),
        ));
    } else if *count == 101 {
        context.warnings.push(Warning::new(
            WarningKind::LoneSurrogate,
            section_index,
            "more than 100 unpaired UTF-16 surrogates; further warnings suppressed".to_string(),
        ));
    }
}

fn tab_item(chars: &[u16], index: usize) -> RunItem {
    if index + 2 >= chars.len() {
        return RunItem::Tab;
    }
    let bits = u32::from(chars[index + 1]) | (u32::from(chars[index + 2]) << 16);
    let width = bits as i32;
    let info = if index + 7 < chars.len() {
        chars[index + 3]
    } else {
        0
    };
    let raw_leader = usize::from(info & 0xFF);
    let raw_kind = usize::from(info >> 8);
    let leader = SHAPE_LINE_TYPES.get(raw_leader).copied().unwrap_or("NONE");
    let kind = raw_kind
        .checked_sub(1)
        .and_then(|value| TAB_KINDS.get(value))
        .copied()
        .unwrap_or("LEFT");
    if leader != "NONE" || kind != "LEFT" {
        RunItem::FormattedTab(crate::model::Tab {
            width: (width > 0).then_some(HwpUnit::from(width)),
            kind: kind.to_string(),
            leader: leader.to_string(),
        })
    } else if width > 0 {
        RunItem::SizedTab(HwpUnit::from(width))
    } else {
        RunItem::Tab
    }
}

pub fn parse_paragraph(record: &Record, context: &mut ParseContext<'_>) -> Paragraph {
    let mut paragraph = Paragraph::default();
    let mut reader = record.reader();
    let _text_length = reader.u32() & 0x7FFF_FFFF;
    let _ = reader.u32();
    paragraph.para_pr_id = i32::from(reader.u16());
    paragraph.style_id = i32::from(reader.u8());
    let column_type = reader.u8();
    paragraph.page_break = column_type & 0x04 != 0;
    paragraph.column_break = column_type & 0x08 != 0;
    let _ = reader.u16();
    let _ = reader.u16();
    let _ = reader.u16();
    let _ = reader.u32();

    for ranges in record
        .children
        .iter()
        .filter(|child| child.tag == T_PARA_RANGE_TAG)
    {
        let mut reader = ranges.reader();
        while reader.remaining() >= 12 {
            paragraph.range_tags.push(RangeTag {
                start: reader.u32(),
                end: reader.u32(),
                tag: reader.u32(),
            });
        }
        if !reader.is_empty() {
            context.warnings.push(Warning::new(
                WarningKind::RecordTruncated,
                context.section_index,
                "PARA_RANGE_TAG has an incomplete entry",
            ));
        }
    }

    let mut shape_at = Vec::new();
    if let Some(char_shape) = record
        .children
        .iter()
        .find(|child| child.tag == T_PARA_CHAR_SHAPE)
    {
        let mut shape_reader = char_shape.reader();
        while shape_reader.remaining() >= 8 {
            let position = shape_reader.u32();
            let id = shape_reader.u32() as i32;
            shape_at.push((position, id));
        }
    }
    if shape_at.is_empty() {
        let id = context
            .document
            .styles
            .get(paragraph.style_id as usize)
            .map(|style| style.char_pr_id)
            .unwrap_or(0);
        shape_at.push((0, id));
    }

    let controls: Vec<&Record> = record
        .children
        .iter()
        .filter(|child| child.tag == T_CTRL_HEADER)
        .collect();
    if context.state.is_top_level() {
        paragraph.columns = controls.iter().find_map(|record| parse_columns(record));
    }
    let mut control_cursor = 0usize;

    let mut chars = Vec::new();
    if let Some(text) = record
        .children
        .iter()
        .find(|child| child.tag == T_PARA_TEXT)
    {
        let mut text_reader = text.reader();
        while text_reader.remaining() >= 2 {
            chars.push(text_reader.u16());
        }
    }

    let mut position = 0u32;
    let mut current = Run {
        char_pr_id: shape_at[0].1,
        ..Run::default()
    };
    let mut buffer = String::new();
    let mut index = 0usize;
    while index < chars.len() {
        let code = chars[index];
        let shape = char_pr_at(&shape_at, position);
        if shape != current.char_pr_id {
            flush_text(&mut buffer, &mut current);
            let finished = std::mem::replace(
                &mut current,
                Run {
                    char_pr_id: shape,
                    ..Run::default()
                },
            );
            if !finished.items.is_empty() {
                paragraph.runs.push(finished);
            }
        }
        match code {
            24 => {
                buffer.push('-');
                index += 1;
                position += 1;
            }
            30 => {
                buffer.push(crate::NO_BREAK_SPACE);
                index += 1;
                position += 1;
            }
            31 => {
                buffer.push(crate::FIXED_SPACE);
                index += 1;
                position += 1;
            }
            0 | 13 | 25..=29 => {
                if code == 13 {
                    index += 1;
                    position += 1;
                    continue;
                }
                flush_text(&mut buffer, &mut current);
                current.items.push(RunItem::Skip(1));
                index += 1;
                position += 1;
            }
            10 => {
                flush_text(&mut buffer, &mut current);
                current.items.push(RunItem::LineBreak);
                index += 1;
                position += 1;
            }
            9 => {
                flush_text(&mut buffer, &mut current);
                current.items.push(tab_item(&chars, index));
                index += 8;
                position += 8;
            }
            4..=8 | 19 | 20 => {
                flush_text(&mut buffer, &mut current);
                current.items.push(RunItem::Skip(8));
                index += 8;
                position += 8;
            }
            1..=3 | 11 | 12 | 14..=18 | 21..=23 => {
                flush_text(&mut buffer, &mut current);
                let (item, supscript) = if let Some(control) = controls.get(control_cursor) {
                    control_cursor += 1;
                    (
                        parse_control(control, context),
                        auto_number_supscript(control),
                    )
                } else {
                    (None, false)
                };
                match item {
                    Some(RunItem::Text(text)) => {
                        let units = text.encode_utf16().count();
                        append_control(
                            RunItem::Text(text),
                            supscript,
                            &mut current,
                            &mut paragraph.runs,
                        );
                        if units < 8 {
                            current.items.push(RunItem::Skip((8 - units) as i32));
                        }
                    }
                    Some(item) => current.items.push(item),
                    None => current.items.push(RunItem::Skip(8)),
                }
                index += 8;
                position += 8;
            }
            _ => {
                if (0xD800..=0xDBFF).contains(&code) {
                    if let Some(&low) = chars.get(index + 1) {
                        if (0xDC00..=0xDFFF).contains(&low) {
                            let value = 0x1_0000
                                + ((u32::from(code) - 0xD800) << 10)
                                + (u32::from(low) - 0xDC00);
                            if let Some(character) = char::from_u32(value) {
                                buffer.push(character);
                            } else {
                                warn_lone_surrogate(code, context);
                                buffer.push('\u{FFFD}');
                            }
                            index += 1;
                            position += 1;
                        } else {
                            warn_lone_surrogate(code, context);
                            buffer.push('\u{FFFD}');
                        }
                    } else {
                        warn_lone_surrogate(code, context);
                        buffer.push('\u{FFFD}');
                    }
                } else if (0xDC00..=0xDFFF).contains(&code) {
                    warn_lone_surrogate(code, context);
                    buffer.push('\u{FFFD}');
                } else {
                    buffer.push(char::from_u32(u32::from(code)).unwrap_or('\u{FFFD}'));
                }
                index += 1;
                position += 1;
            }
        }
    }
    flush_text(&mut buffer, &mut current);
    if current.items.is_empty() && paragraph.runs.last().is_some_and(|run| !run.supscript) {
        if let Some(previous) = paragraph.runs.pop() {
            current = previous;
        }
    }

    while control_cursor < controls.len() {
        let control = controls[control_cursor];
        control_cursor += 1;
        if let Some(item) = parse_control(control, context) {
            append_control(
                item,
                auto_number_supscript(control),
                &mut current,
                &mut paragraph.runs,
            );
        }
    }
    if !current.items.is_empty() || paragraph.runs.is_empty() {
        paragraph.runs.push(current);
    }

    if let Some(line_seg) = record
        .children
        .iter()
        .find(|child| child.tag == T_PARA_LINE_SEG)
    {
        let mut line_reader = line_seg.reader();
        while line_reader.remaining() >= 36 {
            paragraph.line_segs.push(LineSeg {
                textpos: line_reader.u32() as i32,
                vertpos: HwpUnit::from(line_reader.i32()),
                vertsize: HwpUnit::from(line_reader.i32()),
                textheight: HwpUnit::from(line_reader.i32()),
                baseline: HwpUnit::from(line_reader.i32()),
                spacing: HwpUnit::from(line_reader.i32()),
                horzpos: HwpUnit::from(line_reader.i32()),
                horzsize: HwpUnit::from(line_reader.i32()),
                flags: line_reader.u32(),
            });
        }
    }

    paragraph.memos = parse_memos(&record.children, context);
    paragraph
}

fn parse_memos(children: &[Record], context: &mut ParseContext<'_>) -> Vec<Memo> {
    let mut memos = Vec::new();
    let mut index = 0;
    while index < children.len() {
        if children[index].tag != T_MEMO_LIST {
            index += 1;
            continue;
        }
        let mut reader = children[index].reader();
        let shape_id = reader.u32() as i32;
        let end = children[index + 1..]
            .iter()
            .position(|child| child.tag == T_MEMO_LIST)
            .map_or(children.len(), |offset| index + 1 + offset);
        let paragraphs = list_groups(&children[index + 1..end])
            .first()
            .map_or_else(Vec::new, |group| parse_nested(&group.paragraphs, context));
        memos.push(Memo {
            shape_id,
            paragraphs,
        });
        index = end;
    }
    memos
}
