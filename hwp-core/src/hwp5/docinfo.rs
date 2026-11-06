use std::collections::BTreeMap;

use crate::model::{
    Border, BorderFill, Bullet, CharPr, Document, FontFace, MemoShape, Numbering, NumberingHead,
    ParaHead, ParaPr, Style, TabPr, TabStop,
};
use crate::report::{Warning, WarningKind};

use super::record::Record;
use super::tags::{
    T_BIN_DATA, T_BORDER_FILL, T_BULLET, T_CHAR_SHAPE, T_FACE_NAME, T_ID_MAPPINGS, T_MEMO_SHAPE,
    T_NUMBERING, T_PARA_SHAPE, T_STYLE, T_TAB_DEF, T_TRACK_CHANGE_CONTENT,
};

const LANGS: [&str; 7] = [
    "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
];
const BORDER_WIDTHS: [f64; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];
pub(crate) const SHAPE_LINE_TYPES: [&str; 12] = [
    "NONE",
    "SOLID",
    "DASH",
    "DOT",
    "DASH_DOT",
    "DASH_DOT_DOT",
    "LONG_DASH",
    "CIRCLE",
    "DOUBLE_SLIM",
    "SLIM_THICK",
    "THICK_SLIM",
    "SLIM_THICK_SLIM",
];
/// 글자 단위 여백 한 글자의 길이. 한/글 12.30.0은 글꼴 크기와 상관없이 500 HWPUNIT(5pt)으로 그린다.
pub(crate) const CHAR_HWPUNIT: f64 = 500.0;

/// 문단 여백·들여쓰기·문단 간격 한 칸을 HWPUNIT으로 읽는다.
/// 짝수 값은 HWPUNIT의 두 배다. 홀수 값은 맨 아래 비트가 글자 단위 표시이고, 나머지 절반이 1/100 글자다.
/// 601 → 3글자 = 1500 HWPUNIT. 한/글 PDF의 첫 줄 들여쓰기와 맞는다.
fn paragraph_length(raw: i32) -> f64 {
    let half = f64::from(raw / 2);
    if raw % 2 == 0 {
        half
    } else {
        half * CHAR_HWPUNIT / 100.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct BinDataStreams {
    pub names: BTreeMap<u32, String>,
    pub compression: BTreeMap<u32, u8>,
}

pub fn parse_doc_info(
    records: &[Record],
    version: u32,
    document: &mut Document,
    warnings: &mut Vec<Warning>,
) -> BinDataStreams {
    let mut all = Vec::new();
    flatten(records, &mut all);

    // HWPX의 insertBegin/deleteBegin처럼, 남은 변경이 있으면 한/글은 변경 추적 쪽 배율로 그린다.
    document.track_changes |= all
        .iter()
        .any(|record| record.tag == T_TRACK_CHANGE_CONTENT);

    let mut face_counts = [0usize; 7];
    if let Some(id_mappings) = all.iter().find(|record| record.tag == T_ID_MAPPINGS) {
        let mut reader = id_mappings.reader();
        let _ = reader.u32();
        for count in &mut face_counts {
            *count = reader.u32() as usize;
        }
    }

    let mut face_lang = 0usize;
    let mut lang_seen = 0usize;
    let mut bin_index = 1u32;
    let mut char_pr_index = 0i32;
    let mut para_pr_index = 0i32;
    let mut border_fill_index = 1i32;
    let mut numbering_index = 1i32;
    let mut bullet_index = 1i32;
    let mut style_index = 0i32;
    let mut tab_index = 0i32;
    let mut bins = BinDataStreams::default();

    for record in all {
        match record.tag {
            T_BIN_DATA => {
                let mut reader = record.reader();
                let attr = reader.u16();
                let kind = attr & 0xF;
                if kind == 0 {
                    let _ = reader.hwp_string();
                    let _ = reader.hwp_string();
                    warnings.push(Warning::new(
                        WarningKind::LinkedPicture,
                        None,
                        "BinData is linked and has no embedded stream",
                    ));
                }
                if kind == 1 || kind == 2 {
                    let id = reader.u16();
                    let extension = reader.hwp_string();
                    bins.names
                        .insert(bin_index, format!("BIN{id:04X}.{extension}"));
                    bins.compression.insert(bin_index, ((attr >> 4) & 3) as u8);
                }
                bin_index = bin_index.saturating_add(1);
            }
            T_FACE_NAME => {
                while face_lang < LANGS.len() && lang_seen >= face_counts[face_lang] {
                    face_lang += 1;
                    lang_seen = 0;
                }
                let mut reader = record.reader();
                let attr = reader.u8();
                let name = reader.hwp_string();
                let mut face = FontFace::new(name);
                face.face_type = i32::from(attr & 0x03);
                if attr & 0x80 != 0 {
                    face.alt_type = i32::from(reader.u8());
                    let alt = reader.hwp_string();
                    if !alt.is_empty() {
                        face.alt_name = Some(alt);
                    }
                }
                if attr & 0x40 != 0 {
                    reader.skip(10);
                }
                if attr & 0x20 != 0 {
                    let default = reader.hwp_string();
                    if !default.is_empty() {
                        face.default_name = Some(default);
                    }
                }
                if let Some(language) = LANGS.get(face_lang) {
                    face.id = lang_seen as i32;
                    document
                        .font_faces
                        .entry((*language).to_string())
                        .or_default()
                        .push(face);
                }
                lang_seen += 1;
            }
            T_BORDER_FILL => {
                let mut reader = record.reader();
                let attr = reader.u16();
                let mut border_fill = BorderFill {
                    id: border_fill_index,
                    ..BorderFill::default()
                };
                let mut sides: [Border; 4] = std::array::from_fn(|_| Border::default());
                for side in &mut sides {
                    let kind = reader.u8();
                    let width = reader.u8();
                    let color = reader.u32();
                    *side = Border {
                        border_type: border_type(kind).to_string(),
                        width: border_width(width),
                        color: color_ref(color),
                    };
                }
                border_fill.left = sides[0].clone();
                border_fill.right = sides[1].clone();
                border_fill.top = sides[2].clone();
                border_fill.bottom = sides[3].clone();
                let diagonal_kind = reader.u8();
                let diagonal_width = reader.u8();
                let diagonal_color = reader.u32();
                border_fill.diagonal = Border {
                    border_type: border_type(diagonal_kind).to_string(),
                    width: border_width(diagonal_width),
                    color: color_ref(diagonal_color),
                };
                if attr & 0x8 != 0 {
                    border_fill.slash = "SLASH".to_string();
                }
                if attr & 0x40 != 0 {
                    border_fill.back_slash = "BACK".to_string();
                }
                let fill_type = reader.u32();
                if fill_type & 1 != 0 {
                    let background = reader.u32();
                    let _ = reader.u32();
                    let _ = reader.i32();
                    border_fill.fill_color = Some(color_ref(background));
                }
                if fill_type & 4 != 0 {
                    let gradation = super::shape::fill::gradient(&mut reader);
                    if let Some((gradation, first)) =
                        gradation.and_then(|g| g.colors.first().copied().map(|first| (g, first)))
                    {
                        border_fill.fill_color = border_fill.fill_color.or(Some(first));
                        border_fill.gradient = Some(
                            gradation
                                .colors
                                .iter()
                                .enumerate()
                                .map(|(i, color)| {
                                    (*color, gradation.positions.get(i).copied().unwrap_or(0.0))
                                })
                                .collect(),
                        );
                        border_fill.gradation = Some(gradation);
                    }
                }
                document.border_fills.push(border_fill);
                border_fill_index = border_fill_index.saturating_add(1);
            }
            T_CHAR_SHAPE => {
                let mut reader = record.reader();
                let mut char_pr = CharPr {
                    id: char_pr_index,
                    ..CharPr::default()
                };
                for language in LANGS {
                    char_pr
                        .font_ref
                        .insert(language.to_string(), i32::from(reader.u16()));
                }
                for language in LANGS {
                    char_pr
                        .ratio
                        .insert(language.to_string(), f64::from(reader.u8()));
                }
                for language in LANGS {
                    char_pr
                        .spacing
                        .insert(language.to_string(), f64::from(reader.i8()));
                }
                for language in LANGS {
                    char_pr
                        .rel_sz
                        .insert(language.to_string(), f64::from(reader.u8()));
                }
                for language in LANGS {
                    char_pr
                        .offset
                        .insert(language.to_string(), f64::from(reader.i8()));
                }
                char_pr.height = f64::from(reader.i32());
                let attr = reader.u32();
                char_pr.italic = attr & 1 != 0;
                char_pr.bold = attr & 2 != 0;
                char_pr.underline = match (attr >> 2) & 3 {
                    1 => "BOTTOM",
                    3 => "TOP",
                    _ => "NONE",
                }
                .to_string();
                char_pr.underline_shape = char_line_type((attr >> 4) & 15).to_string();
                char_pr.sym_mark = match (attr >> 21) & 15 {
                    1 => "DOT_ABOVE",
                    2 => "RING_ABOVE",
                    3 => "CARON",
                    4 => "TILDE",
                    5 => "SIDE",
                    6 => "COLON",
                    _ => "NONE",
                }
                .to_string();
                char_pr.shadow_type = match (attr >> 11) & 3 {
                    1 => "DROP",
                    2 => "CONTINUOUS",
                    _ => "NONE",
                }
                .to_string();
                char_pr.outline = (attr >> 8) & 7 != 0;
                char_pr.shadow = (attr >> 11) & 3 != 0;
                char_pr.emboss = attr & (1 << 13) != 0;
                char_pr.engrave = attr & (1 << 14) != 0;
                char_pr.supscript = attr & (1 << 15) != 0;
                char_pr.sub_script = attr & (1 << 16) != 0;
                char_pr.use_font_space = attr & (1 << 25) != 0;
                char_pr.kerning = attr & (1 << 30) != 0;
                char_pr.strikeout = (attr >> 18) & 7 != 0 && (attr >> 26) & 0xF <= 12;
                char_pr.shadow_offset_x = f64::from(reader.i8());
                char_pr.shadow_offset_y = f64::from(reader.i8());
                char_pr.text_color = color_ref(reader.u32());
                char_pr.underline_color = color_ref(reader.u32());
                let shade = reader.u32();
                if shade != 0xFFFF_FFFF {
                    char_pr.shade_color = Some(color_ref(shade));
                }
                if reader.remaining() >= 4 {
                    char_pr.shadow_color = color_ref(reader.u32());
                    if version >= 0x0500_0201 && reader.remaining() >= 2 {
                        char_pr.border_fill_id_ref = i32::from(reader.u16());
                        if version >= 0x0500_0300 && reader.remaining() >= 4 {
                            char_pr.strikeout_color = color_ref(reader.u32());
                        }
                    }
                }
                document.char_prs.push(char_pr);
                char_pr_index = char_pr_index.saturating_add(1);
            }
            T_TAB_DEF => {
                let mut reader = record.reader();
                let mut tab_pr = TabPr {
                    id: tab_index,
                    ..TabPr::default()
                };
                if reader.remaining() >= 8 {
                    let flags = reader.u32();
                    tab_pr.auto_left = flags & 1 != 0;
                    tab_pr.auto_right = flags & 2 != 0;
                    let count = (reader.u32() as usize).min(reader.remaining() / 8);
                    // 탭 위치도 문단 여백처럼 HWPUNIT의 두 배로 저장된다(HWPX는 같은 값을 switch의 default에 적고
                    // HwpUnitChar case에 그 절반을 적는다).
                    for _ in 0..count {
                        let position = reader.i32();
                        let kind = reader.u8();
                        let leader = reader.u8();
                        let _ = reader.u16();
                        let stop = TabStop {
                            position: f64::from(position) / 2.0,
                            kind: match kind {
                                0 => "LEFT",
                                1 => "RIGHT",
                                2 => "CENTER",
                                3 => "DECIMAL",
                                _ => "LEFT",
                            }
                            .to_string(),
                            leader: line_type(leader).to_string(),
                        };
                        if position >= 0 {
                            tab_pr.stops.push(stop);
                        }
                    }
                }
                document.tab_prs.push(tab_pr);
                tab_index = tab_index.saturating_add(1);
            }
            T_PARA_SHAPE => {
                let mut reader = record.reader();
                let mut para_pr = ParaPr {
                    id: para_pr_index,
                    ..ParaPr::default()
                };
                let attr = reader.u32();
                para_pr.line_spacing_type = line_spacing_type(attr & 3).to_string();
                para_pr.break_latin_word = match (attr >> 5) & 3 {
                    1 => "HYPHENATION",
                    2 => "BREAK_WORD",
                    _ => "KEEP_WORD",
                }
                .to_string();
                para_pr.snap_to_grid = attr & (1 << 8) != 0;
                para_pr.condense = ((attr >> 9) & 127) as u8;
                para_pr.widow_orphan = attr & (1 << 16) != 0;
                para_pr.keep_with_next = attr & (1 << 17) != 0;
                para_pr.keep_lines = attr & (1 << 18) != 0;
                para_pr.page_break_before = attr & (1 << 19) != 0;
                para_pr.vert_align = match (attr >> 20) & 3 {
                    1 => "TOP",
                    2 => "CENTER",
                    3 => "BOTTOM",
                    _ => "BASELINE",
                }
                .to_string();
                para_pr.font_line_height = attr & (1 << 22) != 0;
                para_pr.connect = attr & (1 << 28) != 0;
                para_pr.ignore_margin = attr & (1 << 29) != 0;
                para_pr.tail_shape = attr & (1 << 30) != 0;
                // 7번 비트 1은 한글 글자 단위 끊기다. 한/글은 이 설정을 HWPX에 KEEP_WORD로 쓴다.
                para_pr.break_non_latin_word = if attr & (1 << 7) == 0 {
                    "BREAK_WORD"
                } else {
                    "KEEP_WORD"
                }
                .to_string();
                para_pr.align = match (attr >> 2) & 7 {
                    0 => "JUSTIFY",
                    1 => "LEFT",
                    2 => "RIGHT",
                    3 => "CENTER",
                    4 => "DISTRIBUTE",
                    _ => "DISTRIBUTE_SPACE",
                }
                .to_string();
                para_pr.heading_type = match (attr >> 23) & 3 {
                    0 => "NONE",
                    1 => "OUTLINE",
                    2 => "NUMBER",
                    _ => "BULLET",
                }
                .to_string();
                para_pr.heading_level = ((attr >> 25) & 7) as i32;
                // 문단 여백·들여쓰기·문단 간격은 HWPUNIT의 두 배로 저장된다: 첫 줄 시작 자리(LineSeg horzpos)와
                // 문단 사이 줄 간격이 이 값의 절반과 맞는다.
                para_pr.margin_left = paragraph_length(reader.i32());
                para_pr.margin_right = paragraph_length(reader.i32());
                para_pr.indent = paragraph_length(reader.i32());
                para_pr.prev = paragraph_length(reader.i32());
                para_pr.next = paragraph_length(reader.i32());
                para_pr.line_spacing = f64::from(reader.i32());
                para_pr.tab_pr_id_ref = i32::from(reader.u16());
                para_pr.heading_id_ref = i32::from(reader.u16());
                para_pr.border_fill_id_ref = i32::from(reader.u16());
                if version >= 0x0500_0205 && reader.remaining() >= 20 {
                    reader.skip(8);
                    para_pr.line_wrap = match reader.u32() & 3 {
                        1 => "SQUEEZE",
                        2 => "KEEP",
                        _ => "BREAK",
                    }
                    .to_string();
                    para_pr.line_spacing_type = line_spacing_type(reader.u32() & 31).to_string();
                    para_pr.line_spacing = f64::from(reader.u32());
                }
                document.para_prs.push(para_pr);
                para_pr_index = para_pr_index.saturating_add(1);
            }
            T_NUMBERING => {
                let mut reader = record.reader();
                let mut numbering = Numbering {
                    id: numbering_index,
                    ..Numbering::default()
                };
                for level in 1..=7 {
                    let head = reader.u32();
                    let width_adjust = reader.i16();
                    let text_offset = reader.i16();
                    let char_pr = reader.u32();
                    let text = reader.hwp_string();
                    let mut numbering_head = NumberingHead {
                        level,
                        text,
                        char_pr_id: (char_pr != 0xFFFF_FFFF).then_some(char_pr as i32),
                        para: para_head(head, width_adjust, text_offset),
                        ..NumberingHead::default()
                    };
                    numbering_head.format = crate::num::hwp5_format(head, warnings).to_string();
                    numbering.heads.push(numbering_head);
                    if reader.remaining() < 14 {
                        break;
                    }
                }
                if reader.remaining() >= 2 {
                    numbering.start = i32::from(reader.u16());
                }
                document.numberings.push(numbering);
                numbering_index = numbering_index.saturating_add(1);
            }
            T_BULLET => {
                let mut reader = record.reader();
                let attr = reader.u32();
                let width_adjust = reader.i16();
                let text_offset = reader.i16();
                let char_pr = reader.u32();
                let character = reader.u16();
                let bullet = Bullet {
                    id: bullet_index,
                    char: char::from_u32(u32::from(character))
                        .map(String::from)
                        .unwrap_or_else(|| "•".to_string()),
                    char_pr_id: (char_pr != 0xFFFF_FFFF).then_some(char_pr as i32),
                    para: para_head(attr, width_adjust, text_offset),
                };
                document.bullets.push(bullet);
                bullet_index = bullet_index.saturating_add(1);
            }
            T_STYLE => {
                let mut reader = record.reader();
                let _ = reader.hwp_string();
                let eng_name = reader.hwp_string();
                let _ = reader.u8();
                let _ = reader.u8();
                let _ = reader.u16();
                let para_pr = reader.u16();
                let char_pr = reader.u16();
                document.styles.push(Style {
                    id: style_index,
                    para_pr_id: i32::from(para_pr),
                    char_pr_id: i32::from(char_pr),
                    eng_name,
                });
                style_index = style_index.saturating_add(1);
            }
            T_MEMO_SHAPE => {
                let mut reader = record.reader();
                let memo_shape = MemoShape {
                    id: document.memo_shapes.len() as i32 + 1,
                    width: f64::from(reader.u32()),
                    line_type: i32::from(reader.u8()),
                    line_width: i32::from(reader.u8()),
                    line_color: color_ref(reader.u32()),
                    fill_color: color_ref(reader.u32()),
                    active_color: color_ref(reader.u32()),
                    memo_type: reader.u32() as i32,
                };
                document.memo_shapes.push(memo_shape);
            }
            _ => {}
        }
    }

    bins
}

fn flatten<'a>(records: &'a [Record], all: &mut Vec<&'a Record>) {
    let mut stack: Vec<&Record> = records.iter().rev().collect();
    while let Some(record) = stack.pop() {
        all.push(record);
        stack.extend(record.children.iter().rev());
    }
}

fn para_head(attr: u32, width_adjust: i16, text_offset: i16) -> ParaHead {
    ParaHead {
        auto_indent: attr & (1 << 3) != 0,
        use_inst_width: attr & (1 << 2) != 0,
        width_adjust: f64::from(width_adjust),
        text_offset_percent: attr & (1 << 4) == 0,
        text_offset: f64::from(text_offset),
    }
}

fn border_type(kind: u8) -> &'static str {
    SHAPE_LINE_TYPES
        .get(usize::from(kind))
        .copied()
        .unwrap_or("SOLID")
}

fn line_type(kind: u8) -> &'static str {
    SHAPE_LINE_TYPES
        .get(usize::from(kind))
        .copied()
        .unwrap_or("NONE")
}

fn border_width(kind: u8) -> f64 {
    BORDER_WIDTHS[usize::from(kind).min(BORDER_WIDTHS.len() - 1)]
}

fn color_ref(color: u32) -> u32 {
    ((color & 0xFF) << 16) | (color & 0xFF00) | ((color >> 16) & 0xFF)
}

fn line_spacing_type(kind: u32) -> &'static str {
    match kind {
        1 => "FIXED",
        2 => "BETWEEN_LINES",
        3 => "AT_LEAST",
        _ => "PERCENT",
    }
}

fn char_line_type(kind: u32) -> &'static str {
    match kind {
        1 => "DASH",
        2 => "DOT",
        3 => "DASH_DOT",
        4 => "DASH_DOT_DOT",
        5 => "LONG_DASH",
        6 => "CIRCLE",
        7 => "DOUBLE_SLIM",
        8 => "SLIM_THICK",
        9 => "THICK_SLIM",
        10 => "SLIM_THICK_SLIM",
        11 => "WAVE",
        12 => "DOUBLEWAVE",
        13 => "THICK3D",
        14 => "THICKREV3D",
        15 => "3D",
        _ => "SOLID",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(tag: u16) -> Record {
        Record {
            tag,
            level: 0,
            data: vec![0; 4],
            children: Vec::new(),
        }
    }

    #[test]
    fn odd_paragraph_length_is_counted_in_characters() {
        assert_eq!(paragraph_length(1134), 567.0);
        assert_eq!(paragraph_length(601), 1500.0);
        assert_eq!(paragraph_length(-601), -1500.0);
        assert_eq!(paragraph_length(1), 0.0);
    }

    #[test]
    fn track_change_content_turns_on_track_changes() {
        let mut document = Document::default();
        parse_doc_info(&[record(32)], 0x0500_0000, &mut document, &mut Vec::new());
        assert!(!document.track_changes);
        parse_doc_info(
            &[record(32), record(96)],
            0x0500_0000,
            &mut document,
            &mut Vec::new(),
        );
        assert!(document.track_changes);
    }
}
