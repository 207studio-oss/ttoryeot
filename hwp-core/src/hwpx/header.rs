use std::collections::BTreeMap;

use crate::hwp5::docinfo::{CHAR_HWPUNIT, SHAPE_LINE_TYPES};
use crate::model::{
    Border, BorderFill, Bullet, CharPr, Document, FontFace, Gradient, Numbering, NumberingHead,
    ParaHead, ParaPr, Style, TabPr, TabStop,
};

use super::dom::Node;

const LANGS: [&str; 7] = [
    "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
];
const NO_ID: i64 = 4_294_967_295;

pub fn parse_header(head: &Node, document: &mut Document, warnings: &mut Vec<crate::Warning>) {
    parse_font_faces(head, document);
    parse_border_fills(head, document);
    parse_char_prs(head, document);
    parse_tab_prs(head, document);
    parse_para_prs(head, document);
    parse_numberings(head, document, warnings);
    parse_bullets(head, document);
    parse_styles(head, document);
    // 한/글: flags 60인 문서는 변경을 빨갛게 표시하지 않고, 56인 문서는 표시한다.
    document.track_markup_hidden = head
        .find("trackchageConfig")
        .and_then(|config| config.int("flags"))
        .is_some_and(|flags| flags & 4 != 0);
}

fn parse_font_faces(head: &Node, document: &mut Document) {
    let mut fonts: BTreeMap<String, BTreeMap<i32, FontFace>> = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("fontface", &mut nodes);
    for face_list in nodes {
        let language = face_list.attr("lang").unwrap_or("HANGUL").to_lowercase();
        let entry = fonts.entry(language).or_default();
        for font in face_list.all("font") {
            let (Some(id), Some(name)) = (font.int("id"), font.attr("face")) else {
                continue;
            };
            let mut face = FontFace::new(name);
            face.id = narrow(id);
            face.face_type = font_kind(font.attr("type"));
            face.embedded = matches!(
                font.attr("isEmbedded")
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("1") | Some("true")
            );
            if let Some(reference) = font
                .attr("binaryItemIDRef")
                .filter(|value| !value.is_empty())
            {
                face.binary_ref = Some(reference.to_string());
            }
            if let Some(substitute) = font.first("substFont") {
                if let Some(alternative) = substitute.attr("face").filter(|value| !value.is_empty())
                {
                    face.alt_name = Some(alternative.to_string());
                    face.alt_type = font_kind(substitute.attr("type"));
                }
            }
            entry.insert(face.id, face);
        }
    }
    document.font_faces = fonts
        .into_iter()
        .map(|(language, faces)| (language, faces.into_values().collect()))
        .collect();
}

/// `<hc:gradation>` 하나(칸 배경과 도형 채우기가 같은 꼴을 쓴다).
pub(super) fn gradation_of(gradient: &Node) -> Gradient {
    Gradient {
        kind: gradient.attr("type").unwrap_or("LINEAR").to_string(),
        angle: gradient.num("angle").unwrap_or(0.0),
        center_x: gradient.num("centerX").unwrap_or(0.0),
        center_y: gradient.num("centerY").unwrap_or(0.0),
        step: narrow(gradient.int("step").unwrap_or(0)),
        step_center: narrow(gradient.int("stepCenter").unwrap_or(50)),
        colors: gradient
            .all("color")
            .map(|child| color(child.attr("value")).unwrap_or(0))
            .collect(),
        positions: Vec::new(),
    }
}

fn parse_border_fills(head: &Node, document: &mut Document) {
    let mut fills = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("borderFill", &mut nodes);
    for node in nodes {
        let mut fill = BorderFill {
            id: narrow(node.int("id").unwrap_or(0)),
            ..BorderFill::default()
        };
        fill.left = read_border(node.first("leftBorder"));
        fill.right = read_border(node.first("rightBorder"));
        fill.top = read_border(node.first("topBorder"));
        fill.bottom = read_border(node.first("bottomBorder"));
        fill.diagonal = read_border(node.first("diagonal"));
        fill.slash = node
            .first("slash")
            .and_then(|value| value.attr("type"))
            .unwrap_or("NONE")
            .to_string();
        fill.back_slash = node
            .first("backSlash")
            .and_then(|value| value.attr("type"))
            .unwrap_or("NONE")
            .to_string();
        if let Some(brush) = node.find("winBrush") {
            fill.fill_color = color(brush.attr("faceColor"));
        }
        if let Some(gradation) = node.find("gradation") {
            let gradient = gradation_of(gradation);
            if let Some(first) = gradient.colors.first().copied() {
                fill.fill_color = fill.fill_color.or(Some(first));
                fill.gradient = Some(gradient.colors.iter().map(|c| (*c, 0.0)).collect());
                fill.gradation = Some(gradient);
            }
        }
        fills.insert(fill.id, fill);
    }
    document.border_fills = fills.into_values().collect();
}

fn parse_char_prs(head: &Node, document: &mut Document) {
    let mut values = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("charPr", &mut nodes);
    for node in nodes {
        let mut value = CharPr {
            id: narrow(node.int("id").unwrap_or(0)),
            ..CharPr::default()
        };
        value.height = node.num("height").unwrap_or(1000.0);
        value.text_color = color(node.attr("textColor")).unwrap_or(0);
        value.shade_color = color(node.attr("shadeColor"));
        value.use_font_space = node.bool("useFontSpace");
        value.kerning = node.bool("useKerning");
        value.sym_mark = node.attr("symMark").unwrap_or("NONE").to_string();
        value.border_fill_id_ref = narrow(node.int("borderFillIDRef").unwrap_or(0));
        if let Some(reference) = node.first("fontRef") {
            for language in LANGS {
                if let Some(id) = reference.int(language) {
                    value.font_ref.insert(language.to_string(), narrow(id));
                }
            }
        }
        read_number_map(node, "ratio", &mut value.ratio);
        read_number_map(node, "spacing", &mut value.spacing);
        read_number_map(node, "relSz", &mut value.rel_sz);
        read_number_map(node, "offset", &mut value.offset);
        value.bold = node.first("bold").is_some();
        value.italic = node.first("italic").is_some();
        value.supscript = node.first("supscript").is_some();
        value.sub_script = node.first("subscript").is_some();
        value.emboss = node.first("emboss").is_some();
        value.engrave = node.first("engrave").is_some();
        value.outline = node
            .first("outline")
            .and_then(|outline| outline.attr("type"))
            .is_some_and(|kind| !matches!(kind, "NONE" | ""));
        if let Some(shadow) = node.first("shadow") {
            value.shadow_type = shadow.attr("type").unwrap_or("NONE").to_string();
            value.shadow = !matches!(value.shadow_type.as_str(), "NONE" | "");
            value.shadow_offset_x = shadow
                .num("offsetX")
                .filter(|value| value.is_finite())
                .unwrap_or(0.0);
            value.shadow_offset_y = shadow
                .num("offsetY")
                .filter(|value| value.is_finite())
                .unwrap_or(0.0);
            value.shadow_color = color(shadow.attr("color")).unwrap_or(0);
        }
        if let Some(underline) = node.first("underline") {
            value.underline = underline.attr("type").unwrap_or("NONE").to_string();
            value.underline_color = color(underline.attr("color")).unwrap_or(value.text_color);
            value.underline_shape = underline.attr("shape").unwrap_or("SOLID").to_string();
        }
        if let Some(strikeout) = node.first("strikeout") {
            let shape = strikeout.attr("shape").unwrap_or("NONE");
            let line_type = strikeout.attr("type").unwrap_or("SOLID");
            value.strikeout = !matches!(shape, "NONE" | "3D" | "") && line_type != "NONE";
            value.strikeout_color = color(strikeout.attr("color")).unwrap_or(0);
        }
        values.insert(value.id, value);
    }
    document.char_prs = values.into_values().collect();
}

/// `name` 자식과, 자식 switch마다 지원하는 case(없으면 default) 안의 `name` 자식을 차례로 준다.
/// 길이 값은 `unit_switched`가 고른다.
/// 개요 8~10 수준 문단의 heading도 case에만 OUTLINE으로 있고 default는 NONE이다.
fn switched<'a>(node: &'a Node, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
    node.children.iter().flat_map(move |child| {
        let holder = if child.name == "switch" {
            child
                .all("case")
                .find(|case| {
                    matches!(
                        case.attr("required-namespace"),
                        Some(
                            "http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"
                                | "http://www.hancom.co.kr/hwpml/2011/paragraph"
                                | "http://www.hancom.co.kr/hwpml/2016/paragraph"
                        )
                    )
                })
                .or_else(|| child.first("default"))
        } else {
            None
        };
        let direct = (child.name == name).then_some(child);
        direct
            .into_iter()
            .chain(holder.into_iter().flat_map(move |holder| holder.all(name)))
    })
}

const HWP_UNIT_CHAR: &str = "http://www.hancom.co.kr/hwpml/2016/HwpUnitChar";

/// 길이 값을 담은 `name` 자식과 그 값에 곱할 배율. HwpUnitChar case 안의 값은 HWPUNIT 그대로다.
/// switch default와, switch 없이 바로 적힌 값(한/글 2018 무렵 HWPX)은 HWP 5처럼 HWPUNIT의 두 배로 적혀 있어
/// 반으로 줄인다.
fn unit_switched<'a>(node: &'a Node, name: &'a str) -> Vec<(&'a Node, f64)> {
    let mut found = Vec::new();
    for child in &node.children {
        if child.name == name {
            found.push((child, 0.5));
        } else if child.name == "switch" {
            let case = child
                .all("case")
                .find(|case| case.attr("required-namespace") == Some(HWP_UNIT_CHAR));
            match case {
                Some(case) => found.extend(case.all(name).map(|item| (item, 1.0))),
                None => found.extend(
                    child
                        .first("default")
                        .into_iter()
                        .flat_map(|default| default.all(name))
                        .map(|item| (item, 0.5)),
                ),
            }
        }
    }
    found
}

fn parse_tab_prs(head: &Node, document: &mut Document) {
    let mut values = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("tabPr", &mut nodes);
    for node in nodes {
        let mut tab = TabPr {
            id: narrow(node.int("id").unwrap_or(0)),
            auto_left: node.bool("autoTabLeft"),
            auto_right: node.bool("autoTabRight"),
            stops: Vec::new(),
        };
        for (item, scale) in unit_switched(node, "tabItem") {
            let stop = TabStop {
                position: item.num("pos").unwrap_or(0.0) * scale,
                kind: tab_kind(item.attr("type")),
                leader: tab_leader(item.attr("leader")),
            };
            if stop.position.is_finite() && stop.position >= 0.0 {
                tab.stops.push(stop);
            }
        }
        values.insert(tab.id, tab);
    }
    document.tab_prs = values.into_values().collect();
}

fn parse_para_prs(head: &Node, document: &mut Document) {
    let mut values = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("paraPr", &mut nodes);
    for node in nodes {
        let mut value = ParaPr {
            id: narrow(node.int("id").unwrap_or(0)),
            ..ParaPr::default()
        };
        if let Some(align) = node.first("align") {
            value.align = align.attr("horizontal").unwrap_or("JUSTIFY").to_string();
            value.vert_align = align.attr("vertical").unwrap_or("BASELINE").to_string();
        }
        if let Some(heading) = switched(node, "heading").next() {
            value.heading_type = heading.attr("type").unwrap_or("NONE").to_string();
            value.heading_id_ref = narrow(heading.int("idRef").unwrap_or(0));
            value.heading_level = narrow(heading.int("level").unwrap_or(0));
        }
        value.tab_pr_id_ref = narrow(node.int("tabPrIDRef").unwrap_or(0));
        value.snap_to_grid = node
            .attr("snapToGrid")
            .map(|_| node.bool("snapToGrid"))
            .unwrap_or(true);
        value.condense = node
            .int("condense")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(0);
        value.font_line_height = node.bool("fontLineHeight");
        if let Some(breaks) = node.first("breakSetting") {
            value.break_latin_word = breaks
                .attr("breakLatinWord")
                .unwrap_or("KEEP_WORD")
                .to_string();
            value.break_non_latin_word = breaks
                .attr("breakNonLatinWord")
                .unwrap_or("BREAK_WORD")
                .to_string();
            value.line_wrap = breaks.attr("lineWrap").unwrap_or("BREAK").to_string();
            value.keep_with_next = breaks.bool("keepWithNext");
            value.page_break_before = breaks.bool("pageBreakBefore");
            value.widow_orphan = breaks.bool("widowOrphan");
            value.keep_lines = breaks.bool("keepLines");
        }
        if let Some(border) = node.first("border") {
            value.connect = border.bool("connect");
            value.ignore_margin = border.bool("ignoreMargin");
        }
        value.border_fill_id_ref = node
            .first("border")
            .and_then(|border| border.int("borderFillIDRef"))
            .map(narrow)
            .unwrap_or(0);
        if let Some(&(margin, scale)) = unit_switched(node, "margin").first() {
            value.indent = margin_value(margin, "intent", scale);
            value.margin_left = margin_value(margin, "left", scale);
            value.margin_right = margin_value(margin, "right", scale);
            value.prev = margin_value(margin, "prev", scale);
            value.next = margin_value(margin, "next", scale);
        }
        if let Some(spacing) = node.find("lineSpacing") {
            value.line_spacing_type = spacing.attr("type").unwrap_or("PERCENT").to_string();
            value.line_spacing = spacing.num("value").unwrap_or(160.0);
        }
        values.insert(value.id, value);
    }
    document.para_prs = values.into_values().collect();
}

fn parse_numberings(head: &Node, document: &mut Document, warnings: &mut Vec<crate::Warning>) {
    let mut values = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("numbering", &mut nodes);
    for node in nodes {
        let mut numbering = Numbering {
            id: narrow(node.int("id").unwrap_or(0)),
            start: narrow(node.int("start").unwrap_or(0)),
            heads: Vec::new(),
        };
        for head in node.all("paraHead") {
            let mut item = NumberingHead {
                level: narrow(head.int("level").unwrap_or(1)),
                format: crate::num::hwpx_format(
                    head.attr("numFormat").unwrap_or("DIGIT"),
                    warnings,
                ),
                text: head.all_text(),
                align: head.attr("align").unwrap_or("LEFT").to_string(),
                start: narrow(head.int("start").unwrap_or(1)),
                char_pr_id: None,
                para: read_para_head(head),
            };
            if let Some(id) = head.int("charPrIDRef").filter(|value| *value != NO_ID) {
                item.char_pr_id = Some(narrow(id));
            }
            numbering.heads.push(item);
        }
        values.insert(numbering.id, numbering);
    }
    document.numberings = values.into_values().collect();
}

fn parse_bullets(head: &Node, document: &mut Document) {
    let mut values = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("bullet", &mut nodes);
    for node in nodes {
        let mut bullet = Bullet {
            id: narrow(node.int("id").unwrap_or(0)),
            char: node.attr("char").unwrap_or("•").to_string(),
            char_pr_id: None,
            para: ParaHead::default(),
        };
        if let Some(head) = node.first("paraHead") {
            bullet.para = read_para_head(head);
            if let Some(id) = head.int("charPrIDRef").filter(|value| *value != NO_ID) {
                bullet.char_pr_id = Some(narrow(id));
            }
        }
        values.insert(bullet.id, bullet);
    }
    document.bullets = values.into_values().collect();
}

fn parse_styles(head: &Node, document: &mut Document) {
    let mut values = BTreeMap::new();
    let mut nodes = Vec::new();
    head.find_all("style", &mut nodes);
    for node in nodes {
        let Some(id) = node.int("id") else {
            continue;
        };
        values.insert(
            narrow(id),
            Style {
                id: narrow(id),
                para_pr_id: narrow(node.int("paraPrIDRef").unwrap_or(0)),
                char_pr_id: narrow(node.int("charPrIDRef").unwrap_or(0)),
                eng_name: node.attr("engName").unwrap_or_default().to_string(),
            },
        );
    }
    document.styles = values.into_values().collect();
}

fn read_border(node: Option<&Node>) -> Border {
    let Some(node) = node else {
        return Border::default();
    };
    Border {
        border_type: node.attr("type").unwrap_or("NONE").to_string(),
        width: millimetres(node.attr("width")),
        color: color(node.attr("color")).unwrap_or(0),
    }
}

fn read_number_map(node: &Node, name: &str, target: &mut BTreeMap<String, f64>) {
    let Some(values) = node.first(name) else {
        return;
    };
    for language in LANGS {
        if let Some(value) = values.num(language) {
            target.insert(language.to_string(), value);
        }
    }
}

fn read_para_head(node: &Node) -> ParaHead {
    ParaHead {
        auto_indent: node.bool("autoIndent"),
        use_inst_width: node.bool("useInstWidth"),
        width_adjust: node.num("widthAdjust").unwrap_or(0.0),
        text_offset_percent: node.attr("textOffsetType").unwrap_or("PERCENT") == "PERCENT",
        text_offset: node.num("textOffset").unwrap_or(0.0),
    }
}

/// `scale`은 HWPUNIT 값에만 곱한다(`unit_switched`). MM은 길이를 그대로 적은 값이다.
fn margin_value(margin: &Node, name: &str, scale: f64) -> f64 {
    let Some(value) = margin.first(name) else {
        return 0.0;
    };
    let number = value.num("value").unwrap_or(0.0);
    match value.attr("unit").unwrap_or("HWPUNIT") {
        "MM" => number * 7200.0 / 25.4,
        // 글자 단위는 1/100 글자로 적힌다.
        "CHAR" => number * CHAR_HWPUNIT / 100.0,
        _ => number * scale,
    }
}

fn font_kind(value: Option<&str>) -> i32 {
    match value.map(str::to_ascii_uppercase).as_deref() {
        Some("TTF") => 1,
        Some("HFT") => 2,
        _ => 0,
    }
}

pub(crate) fn tab_kind(value: Option<&str>) -> String {
    const KINDS: [&str; 4] = ["LEFT", "RIGHT", "CENTER", "DECIMAL"];
    let raw = value.unwrap_or("LEFT");
    if let Ok(index) = raw.parse::<usize>() {
        if (1..=4).contains(&index) {
            return KINDS[index - 1].to_string();
        }
        return "LEFT".to_string();
    }
    if KINDS.contains(&raw) {
        raw.to_string()
    } else {
        "LEFT".to_string()
    }
}

pub(crate) fn tab_leader(value: Option<&str>) -> String {
    let raw = value.unwrap_or("NONE");
    if let Ok(index) = raw.parse::<usize>() {
        return SHAPE_LINE_TYPES
            .get(index)
            .copied()
            .unwrap_or("NONE")
            .to_string();
    }
    if SHAPE_LINE_TYPES.contains(&raw) {
        raw.to_string()
    } else {
        "NONE".to_string()
    }
}

fn millimetres(value: Option<&str>) -> f64 {
    let Some(value) = value else {
        return 0.1;
    };
    let number = value.replace("mm", "");
    let number = number.trim_matches(|character: char| character == ' ' || character == '\t');
    number.parse::<f64>().unwrap_or(0.1)
}

pub(crate) fn color(value: Option<&str>) -> Option<u32> {
    let value = value?.trim_matches(|character: char| character == ' ' || character == '\t');
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return None;
    }
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(value, 16).ok()
}

fn narrow(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(flags: &str) -> bool {
        let xml =
            format!(r#"<hh:head xmlns:hh="h"><hh:trackchageConfig flags="{flags}"/></hh:head>"#);
        let head = super::super::dom::parse(xml.as_bytes()).unwrap();
        let mut document = Document::default();
        parse_header(&head, &mut document, &mut Vec::new());
        document.track_markup_hidden
    }

    #[test]
    fn track_config_flag_four_hides_change_markup() {
        assert!(hidden("60"));
        assert!(!hidden("56"));
    }
}
