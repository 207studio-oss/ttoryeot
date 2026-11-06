//! 수식 배치: 구문 나무를 글자 조각·가로줄·선으로 놓는다. Swift `HWPEquation.swift`를 옮겼다.
//!
//! 길이는 pt, 세로 좌표는 기준선에서 아래가 +다. 비율 상수는 그 자리의 글자 크기(em)의 배수이며,
//! 한/글 기준 PDF에서 재어 맞춘 값이다.

use super::script::{Cls, Face, FaceKind, Glyphs, Node, rows_of};
use crate::fonts::{FaceId, FontBook, Ink};
use crate::scene::{self, GlyphRun, Item, PathOp, Rect, Stroke};
use crate::subst::Rules;
use hwp_core::FontFace;

const ASCENT: f32 = 0.8;
const DESCENT: f32 = 0.2;
const AXIS: f32 = 0.3;
const SCRIPT_SCALE: f32 = 0.673;
const SUP_RISE: f32 = 0.44;
const SUB_DROP: f32 = 0.35;
const BAR_THICKNESS: f32 = 0.045;
const FRAC_GAP: f32 = 0.135;
const FRAC_PAD: f32 = 0.13;
const FRAC_MARGIN: f32 = 0.12;
const ROW_GAP: f32 = 0.078;
const BIN_SPACE: f32 = 0.12;
const REL_SPACE: f32 = 0.16;

/// 배치한 글자 조각·가로줄·선. 기준선 원점, y는 아래가 +다.
#[derive(Clone)]
struct Run {
    text: String,
    face: Face,
    size: f32,
    x: f32,
    y: f32,
    hs: f32,
    vs: f32,
}

#[derive(Clone, Copy)]
struct Rule {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy)]
struct Seg {
    a: (f32, f32),
    b: (f32, f32),
    width: f32,
}

#[derive(Default, Clone)]
pub(super) struct Frame {
    pub(super) width: f32,
    asc: f32,
    desc: f32,
    runs: Vec<Run>,
    rules: Vec<Rule>,
    segs: Vec<Seg>,
}

impl Frame {
    fn put(&mut self, other: &Frame, x: f32, y: f32) {
        for run in &other.runs {
            self.runs.push(Run {
                x: run.x + x,
                y: run.y + y,
                ..run.clone()
            });
        }
        for rule in &other.rules {
            self.rules.push(Rule {
                x: rule.x + x,
                y: rule.y + y,
                ..*rule
            });
        }
        for seg in &other.segs {
            self.segs.push(Seg {
                a: (seg.a.0 + x, seg.a.1 + y),
                b: (seg.b.0 + x, seg.b.1 + y),
                ..*seg
            });
        }
        self.asc = self.asc.max(other.asc - y);
        self.desc = self.desc.max(other.desc + y);
    }
}

/// 수식 하나를 그리는 동안 쓰는 글꼴·글리프.
pub(super) struct Faces {
    pub(super) pua: bool,
    math: (FaceId, bool),
    math_bold: (FaceId, bool),
    italic: Option<(FaceId, bool)>,
    italic_bold: Option<(FaceId, bool)>,
    text: (FaceId, bool),
    text_bold: (FaceId, bool),
}

impl Faces {
    pub(super) fn resolve(fonts: &FontBook, rules: &Rules, name: &str) -> Option<Self> {
        let pick = |names: &[&str], bold: bool| -> Option<(FaceId, bool)> {
            names
                .iter()
                .find_map(|candidate| fonts.find(candidate, bold))
        };
        let anything = || fonts.any_face().map(|id| (id, false));
        let equation = (!name.is_empty())
            .then(|| fonts.find(name, false))
            .flatten()
            .or_else(|| fonts.find("HyhwpEQ", false));
        let text_face = |bold: bool| {
            rules
                .resolve(
                    fonts,
                    &FontFace::new("한컴바탕".to_string()),
                    "hangul",
                    bold,
                )
                .or_else(|| {
                    pick(
                        &[
                            "Batang",
                            "바탕",
                            "NanumMyeongjo",
                            "AppleMyungjo",
                            "Liberation Serif",
                        ],
                        bold,
                    )
                })
                .or_else(anything)
        };
        let math = equation
            .or_else(|| {
                pick(
                    &[
                        "Times New Roman",
                        "Times",
                        "Liberation Serif",
                        "Nimbus Roman",
                    ],
                    false,
                )
            })
            .or_else(|| text_face(false))?;
        let math_bold = equation
            .map(|(id, _)| (id, false))
            .or_else(|| {
                pick(
                    &[
                        "Times New Roman Bold",
                        "Liberation Serif Bold",
                        "Nimbus Roman Bold",
                    ],
                    true,
                )
            })
            .unwrap_or(math);
        let italic = equation.or_else(|| {
            pick(
                &[
                    "Times New Roman Italic",
                    "Liberation Serif Italic",
                    "Nimbus Roman Italic",
                ],
                false,
            )
        });
        let italic_bold = equation.or_else(|| {
            pick(
                &[
                    "Times New Roman Bold Italic",
                    "Liberation Serif Bold Italic",
                    "Nimbus Roman Bold Italic",
                ],
                false,
            )
        });
        let text = text_face(false).unwrap_or(math);
        let text_bold = text_face(true).unwrap_or(text);
        Some(Self {
            math,
            math_bold,
            italic,
            italic_bold,
            text,
            text_bold,
            pua: equation.is_some(),
        })
    }

    /// (얼굴, 굵은 얼굴인가, 기울임을 흉내 내는가).
    fn of(&self, face: Face) -> (FaceId, bool, bool) {
        let (slot, synthetic) = match (face.kind, face.bold) {
            (FaceKind::Math, false) => (self.math, false),
            (FaceKind::Math, true) => (self.math_bold, false),
            (FaceKind::Italic, false) => match self.italic {
                Some(slot) => (slot, false),
                None => (self.math, true),
            },
            (FaceKind::Italic, true) => match self.italic_bold {
                Some(slot) => (slot, false),
                None => match self.italic {
                    Some(slot) => (slot, true),
                    None => (self.math_bold, true),
                },
            },
            (FaceKind::Text, false) => (self.text, false),
            (FaceKind::Text, true) => (self.text_bold, false),
        };
        (slot.0, slot.1, synthetic)
    }
}

pub(super) struct Layout<'a> {
    fonts: &'a mut FontBook,
    faces: Faces,
    glyphs: Glyphs,
    fallbacks: Vec<FaceId>,
}

/// 파일에 적힌 개체 크기가 스크립트와 맞지 않는가.
fn is_stale(frame: &Frame, rect: Rect) -> bool {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return true;
    }
    let height_ratio = (frame.asc + frame.desc) / rect.height;
    let width_ratio = frame.width / rect.width;
    height_ratio > 1.2 || height_ratio < 0.7 || width_ratio > 1.45 || width_ratio < 0.55
}

impl<'a> Layout<'a> {
    pub(super) fn new(fonts: &'a mut FontBook, faces: Faces) -> Self {
        let rules = Rules::builtin();
        let fallbacks = rules
            .fallback_glyph
            .iter()
            .chain(&rules.fallback_serif)
            .chain(&rules.fallback_gothic)
            .filter_map(|name| fonts.find(name, false).map(|(id, _)| id))
            .collect();
        let glyphs = Glyphs::new(faces.pua);
        Self {
            fonts,
            faces,
            glyphs,
            fallbacks,
        }
    }

    /// 이 글자들을 그릴 얼굴. 모자라는 글자가 있으면 대신할 글꼴을 찾는다.
    fn select(&mut self, text: &str, face: Face) -> (FaceId, bool, bool) {
        let (id, bold, synthetic) = self.faces.of(face);
        if text
            .chars()
            .all(|character| self.fonts.glyph(id, character).is_some())
        {
            return (id, bold, synthetic);
        }
        for fallback in &self.fallbacks {
            let fallback = *fallback;
            if text
                .chars()
                .all(|character| self.fonts.glyph(fallback, character).is_some())
            {
                return (fallback, false, false);
            }
        }
        (id, bold, synthetic)
    }

    fn width(&mut self, text: &str, face: Face, size: f32) -> f32 {
        let (id, _, _) = self.faces.of(face);
        let mut width = 0.0;
        for character in text.chars() {
            if let Some(info) = self.fonts.glyph(id, character) {
                width += info.advance;
            }
        }
        width * size
    }

    fn ink(&mut self, text: &str, face: Face, size: f32) -> Ink {
        let (id, _, _) = self.faces.of(face);
        let ink = self.fonts.ink(id, text);
        Ink {
            min_x: ink.min_x * size,
            max_x: ink.max_x * size,
            min_y: ink.min_y * size,
            max_y: ink.max_y * size,
        }
    }

    fn atom(&mut self, text: &str, face: Face, size: f32) -> Frame {
        let width = self.width(text, face, size);
        Frame {
            width,
            asc: ASCENT * size,
            desc: DESCENT * size,
            runs: vec![Run {
                text: text.to_string(),
                face,
                size,
                x: 0.0,
                y: 0.0,
                hs: 1.0,
                vs: 1.0,
            }],
            rules: Vec::new(),
            segs: Vec::new(),
        }
    }

    fn frame(&mut self, node: &Node, size: f32) -> (Frame, Cls) {
        match node {
            Node::Atom(text, face, class) => (self.atom(text, *face, size), *class),
            Node::Space(amount) => (
                Frame {
                    width: amount * size,
                    ..Frame::default()
                },
                Cls::Ord,
            ),
            Node::Group(list) => (self.seq(list, size), Cls::Ord),
            Node::Amp | Node::RowSep => (Frame::default(), Cls::Ord),
            Node::Script(base, sub, sup) => {
                let (boxed, class) = self.frame(base, size);
                (
                    self.scripts(&boxed, sub.as_deref(), sup.as_deref(), size),
                    class,
                )
            }
            Node::Frac(numerator, denominator, bar) => {
                (self.frac(numerator, denominator, *bar, size), Cls::Ord)
            }
            Node::Root(content, index) => (self.root(content, index.as_deref(), size), Cls::Ord),
            Node::Fence(left, inner, right) => (
                self.fence(left.as_deref(), inner, right.as_deref(), size),
                Cls::Ord,
            ),
            Node::Grid(kind, cells) => (self.grid(kind, cells, size), Cls::Ord),
            Node::BigOp(name, sub, sup) => (
                self.bigop(name, sub.as_deref(), sup.as_deref(), size),
                Cls::Ord,
            ),
            Node::Accent(kind, inner) => (self.accent(kind, inner, size), Cls::Ord),
        }
    }

    fn boxed(&mut self, node: &Node, size: f32) -> Frame {
        self.frame(node, size).0
    }

    /// 가로로 잇는다. 행 표지가 있으면 왼쪽 맞춤 세로 쌓기다.
    pub(super) fn seq(&mut self, nodes: &[Node], size: f32) -> Frame {
        if nodes.iter().any(|node| matches!(node, Node::RowSep)) {
            return self.grid("lpile", &rows_of(nodes), size);
        }
        let mut out = Frame {
            asc: ASCENT * size,
            desc: DESCENT * size,
            ..Frame::default()
        };
        let mut previous: Option<Cls> = None;
        let mut x = 0.0;
        for node in nodes {
            match node {
                Node::Amp => continue,
                Node::Space(amount) => {
                    x += amount * size;
                    continue;
                }
                _ => {}
            }
            let (placed, mut class) = self.frame(node, size);
            if class == Cls::Bin
                && (previous.is_none()
                    || matches!(previous, Some(Cls::Bin | Cls::Rel | Cls::Open | Cls::Punct)))
            {
                class = Cls::Ord;
            }
            if let Some(before) = previous {
                x += spacing(before, class) * size;
            }
            out.put(&placed, x, 0.0);
            x += placed.width;
            previous = Some(class);
        }
        out.width = x;
        out
    }

    fn scripts(
        &mut self,
        base: &Frame,
        sub: Option<&Node>,
        sup: Option<&Node>,
        size: f32,
    ) -> Frame {
        let mut out = Frame::default();
        out.put(base, 0.0, 0.0);
        let script_size = size * SCRIPT_SCALE;
        let mut width = 0.0f32;
        if let Some(node) = sup {
            let placed = self.boxed(node, script_size);
            out.put(
                &placed,
                base.width,
                -(SUP_RISE * size).max(base.asc - 0.36 * size),
            );
            width = width.max(placed.width);
        }
        if let Some(node) = sub {
            let placed = self.boxed(node, script_size);
            out.put(
                &placed,
                base.width,
                (SUB_DROP * size).max(base.desc + 0.15 * size),
            );
            width = width.max(placed.width);
        }
        out.width = base.width + width;
        out
    }

    fn frac(&mut self, numerator: &Node, denominator: &Node, bar: bool, size: f32) -> Frame {
        let top = self.boxed(numerator, size);
        let bottom = self.boxed(denominator, size);
        let pad = FRAC_PAD * size;
        let thickness = BAR_THICKNESS * size;
        let gap = FRAC_GAP * size;
        let axis = AXIS * size;
        let margin = FRAC_MARGIN * size;
        let width = top.width.max(bottom.width) + 2.0 * pad;
        let mut out = Frame::default();
        out.put(
            &top,
            margin + (width - top.width) / 2.0,
            -(axis + thickness / 2.0 + gap + top.desc),
        );
        out.put(
            &bottom,
            margin + (width - bottom.width) / 2.0,
            -axis + thickness / 2.0 + gap + bottom.asc,
        );
        if bar {
            out.rules.push(Rule {
                x: margin,
                y: -axis - thickness / 2.0,
                width,
                height: thickness,
            });
        }
        out.width = width + 2.0 * margin;
        out
    }

    /// 근호: 근호 글리프·내용 위 가로줄·왼쪽 위 지수.
    fn root(&mut self, content: &Node, index: Option<&Node>, size: f32) -> Frame {
        let inner = self.boxed(content, size);
        let thickness = BAR_THICKNESS * size;
        let radical_size = size * 1.04;
        let radical = self.glyphs.radical();
        let ink = self.ink(&radical, Face::math(), radical_size);
        let bar_top = -(inner.asc + 0.2 * size);
        let needed = inner.desc - bar_top;
        let stretch = if ink.height() > 0.0 {
            1.0f32.max(needed / ink.height())
        } else {
            1.0
        };
        let y = bar_top + ink.max_y * stretch;
        let mut out = Frame::default();
        let mut shift = 0.0f32;
        if let Some(index) = index {
            let placed = self.boxed(index, size * SCRIPT_SCALE * SCRIPT_SCALE);
            let right = ink.min_x + 0.35 * size;
            shift = (placed.width - right).max(0.0);
            out.put(&placed, shift + right - placed.width, -0.47 * size);
        }
        let content_x = shift + (ink.max_x - 0.1 * size).max(ink.min_x + 0.3 * size);
        let bar_x = (shift + ink.max_x - 0.03 * size).min(content_x);
        out.runs.push(Run {
            text: radical,
            face: Face::math(),
            size: radical_size,
            x: shift,
            y,
            hs: 1.0,
            vs: stretch,
        });
        out.rules.push(Rule {
            x: bar_x,
            y: bar_top,
            width: content_x + inner.width + 0.1 * size - bar_x,
            height: thickness,
        });
        out.put(&inner, content_x, 0.0);
        out.asc = out.asc.max(-bar_top);
        out.desc = out.desc.max(y - ink.min_y * stretch);
        out.width = content_x + inner.width + 0.16 * size;
        out
    }

    /// 내용 옆에 놓을 구분 기호. 잉크 높이가 내용 높이보다 5% 넘게 낮으면 세로로 늘인다.
    /// 가로는 full이면 그대로, 아니면 1 + 0.4 × (늘인 배율 − 1)배다.
    fn delim(&mut self, delimiter: &str, content: &Frame, size: f32, full: bool) -> Frame {
        let (character, face) = self.glyphs.delimiter(delimiter);
        let ink = self.ink(&character, face, size);
        let needed = content.asc + content.desc;
        let mut ratio = 1.0f32;
        if ink.height() > 0.0 && needed > ink.height() * 1.05 {
            ratio = needed / ink.height();
        }
        let hs = if full { 1.0 } else { 1.0 + 0.4 * (ratio - 1.0) };
        let scaled = size * hs;
        let vs = ratio / hs;
        let center = (content.asc - content.desc) / 2.0;
        let y = ink.mid_y() * ratio - center;
        let width = self.width(&character, face, scaled);
        let mut out = Frame {
            width,
            ..Frame::default()
        };
        out.runs.push(Run {
            text: character,
            face,
            size: scaled,
            x: 0.0,
            y,
            hs: 1.0,
            vs,
        });
        out.asc = (ink.max_y * ratio - y).max(0.0);
        out.desc = (y - ink.min_y * ratio).max(0.0);
        out
    }

    fn fence(&mut self, left: Option<&str>, inner: &Node, right: Option<&str>, size: f32) -> Frame {
        let content = self.boxed(inner, size);
        let mut out = Frame {
            asc: content.asc,
            desc: content.desc,
            ..Frame::default()
        };
        let mut x = 0.0;
        if let Some(left) = left {
            let placed = self.delim(left, &content, size, false);
            out.put(&placed, 0.0, 0.0);
            x = placed.width;
        }
        out.put(&content, x, 0.0);
        x += content.width;
        if let Some(right) = right {
            let placed = self.delim(right, &content, size, false);
            out.put(&placed, x, 0.0);
            x += placed.width;
        }
        out.width = x;
        out
    }

    fn grid(&mut self, kind: &str, cells: &[Vec<Node>], size: f32) -> Frame {
        let mut placed: Vec<Vec<Frame>> = Vec::new();
        for row in cells {
            let mut line = Vec::new();
            for cell in row {
                line.push(self.boxed(cell, size));
            }
            placed.push(line);
        }
        let columns = placed.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0.0f32; columns];
        for row in &placed {
            for (index, cell) in row.iter().enumerate() {
                widths[index] = widths[index].max(cell.width);
            }
        }
        if kind == "cases" {
            for width in widths.iter_mut() {
                if *width == 0.0 {
                    *width = 0.46 * size;
                }
            }
        }
        let column_gap = if kind == "eqalign" {
            0.0
        } else if kind.ends_with("matrix") {
            0.8 * size
        } else {
            0.2 * size
        };
        let row_asc: Vec<f32> = placed
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.asc)
                    .fold(ASCENT * size, f32::max)
            })
            .collect();
        let row_desc: Vec<f32> = placed
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.desc)
                    .fold(DESCENT * size, f32::max)
            })
            .collect();
        let gap = ROW_GAP * size;
        let height: f32 = row_asc
            .iter()
            .zip(&row_desc)
            .map(|(asc, desc)| asc + desc)
            .sum::<f32>()
            + gap * (placed.len().saturating_sub(1)) as f32;
        let top = -(AXIS * size + height / 2.0);
        let mut body = Frame::default();
        let mut y = top;
        for (row_index, row) in placed.iter().enumerate() {
            y += row_asc[row_index];
            let mut x = 0.0;
            for (column, width) in widths.iter().enumerate() {
                if let Some(cell) = row.get(column) {
                    let slack = width - cell.width;
                    let dx = match kind {
                        "cases" | "lpile" => 0.0,
                        "rpile" => slack,
                        "eqalign" => {
                            if column % 2 == 0 {
                                slack
                            } else {
                                0.0
                            }
                        }
                        _ => slack / 2.0,
                    };
                    body.put(cell, x + dx, y);
                }
                x += widths[column]
                    + if column + 1 < columns {
                        column_gap
                    } else {
                        0.0
                    };
            }
            y += row_desc[row_index] + gap;
        }
        body.width = widths.iter().sum::<f32>() + column_gap * columns.saturating_sub(1) as f32;
        body.asc = body.asc.max(-top);
        body.desc = body.desc.max(top + height);
        let fences = match kind {
            "cases" => (Some("{"), None),
            "pmatrix" => (Some("("), Some(")")),
            "bmatrix" => (Some("["), Some("]")),
            "dmatrix" => (Some("|"), Some("|")),
            _ => (None, None),
        };
        if fences.0.is_none() && fences.1.is_none() {
            return body;
        }
        let mut out = Frame::default();
        let mut x = 0.0;
        if let Some(left) = fences.0 {
            let placed = self.delim(left, &body, size, true);
            out.put(&placed, 0.0, 0.0);
            x = placed.width + if kind == "cases" { 0.0 } else { 0.1 * size };
        }
        out.put(&body, x, 0.0);
        x += body.width;
        if let Some(right) = fences.1 {
            x += 0.1 * size;
            let placed = self.delim(right, &body, size, true);
            out.put(&placed, x, 0.0);
            x += placed.width;
        }
        out.width = x;
        out
    }

    fn bigop(&mut self, name: &str, sub: Option<&Node>, sup: Option<&Node>, size: f32) -> Frame {
        let script_size = size * SCRIPT_SCALE;
        let lower = sub.map(|node| self.boxed(node, script_size));
        let upper = sup.map(|node| self.boxed(node, script_size));
        let mut out = Frame::default();
        if name == "lim" {
            let operator = self.atom("lim", Face::math(), size * 1.2);
            let drop = 0.06 * size;
            let width = operator
                .width
                .max(lower.as_ref().map_or(0.0, |frame| frame.width))
                .max(upper.as_ref().map_or(0.0, |frame| frame.width));
            out.put(&operator, (width - operator.width) / 2.0, drop);
            if let Some(lower) = &lower {
                out.put(
                    lower,
                    (width - lower.width) / 2.0,
                    drop + operator.desc + lower.asc,
                );
            }
            if let Some(upper) = &upper {
                out.put(
                    upper,
                    (width - upper.width) / 2.0,
                    drop - operator.asc - upper.desc,
                );
            }
            out.width = width;
            return out;
        }
        let (glyph, scale, limits) = self.glyphs.bigop(name);
        let glyph_size = size * scale;
        let ink = self.ink(&glyph, Face::math(), glyph_size);
        let y = ink.mid_y() - AXIS * size;
        let operator_width = self.width(&glyph, Face::math(), glyph_size);
        let mut operator = Frame {
            width: operator_width,
            ..Frame::default()
        };
        operator.runs.push(Run {
            text: glyph,
            face: Face::math(),
            size: glyph_size,
            x: 0.0,
            y,
            hs: 1.0,
            vs: 1.0,
        });
        operator.asc = (ink.max_y - y).max(0.0);
        operator.desc = (y - ink.min_y).max(0.0);
        let ink_top = y - ink.max_y;
        let ink_bottom = y - ink.min_y;
        if limits {
            let width = operator
                .width
                .max(lower.as_ref().map_or(0.0, |frame| frame.width))
                .max(upper.as_ref().map_or(0.0, |frame| frame.width));
            out.put(&operator, (width - operator.width) / 2.0, 0.0);
            if let Some(lower) = &lower {
                out.put(
                    lower,
                    (width - lower.width) / 2.0,
                    ink_bottom + 0.05 * size + lower.asc,
                );
            }
            if let Some(upper) = &upper {
                out.put(
                    upper,
                    (width - upper.width) / 2.0,
                    ink_top - 0.05 * size - upper.desc,
                );
            }
            out.width = width;
        } else {
            out.put(&operator, 0.0, 0.0);
            let mut width = operator.width;
            if let Some(lower) = &lower {
                let x = operator.width * 0.59;
                out.put(lower, x, ink_bottom + 0.21 * size);
                width = width.max(x + lower.width);
            }
            if let Some(upper) = &upper {
                let x = operator.width * 1.04;
                out.put(upper, x, ink_top + 0.35 * size);
                width = width.max(x + upper.width);
            }
            out.width = width;
        }
        out
    }

    fn accent(&mut self, kind: &str, inner: &Node, size: f32) -> Frame {
        let content = self.boxed(inner, size);
        let mut out = Frame::default();
        out.put(&content, 0.0, 0.0);
        out.width = content.width;
        let thickness = BAR_THICKNESS * size;
        match kind {
            "bar" | "overline" => {
                let y = -(content.asc + 0.18 * size);
                out.rules.push(Rule {
                    x: 0.0,
                    y: y - thickness,
                    width: content.width,
                    height: thickness,
                });
                out.asc = out.asc.max(-y + thickness);
            }
            "under" | "underline" => {
                let y = content.desc + 0.06 * size;
                out.rules.push(Rule {
                    x: 0.0,
                    y,
                    width: content.width,
                    height: thickness,
                });
                out.desc = out.desc.max(y + thickness);
            }
            "vec" => {
                let y = -(content.asc + 0.2 * size);
                let head = 0.18 * size;
                let width = 0.045 * size;
                out.segs.push(Seg {
                    a: (0.0, y),
                    b: (content.width, y),
                    width,
                });
                out.segs.push(Seg {
                    a: (content.width, y),
                    b: (content.width - head, y - head / 2.0),
                    width,
                });
                out.segs.push(Seg {
                    a: (content.width, y),
                    b: (content.width - head, y + head / 2.0),
                    width,
                });
                out.asc = out.asc.max(-y + head / 2.0);
            }
            _ => {
                let glyph = self.glyphs.accent(kind);
                if !glyph.is_empty() {
                    let ink = self.ink(&glyph, Face::math(), size);
                    let y = -(content.asc + 0.04 * size) + ink.min_y;
                    let width = self.width(&glyph, Face::math(), size);
                    out.runs.push(Run {
                        text: glyph,
                        face: Face::math(),
                        size,
                        x: (content.width - width) / 2.0,
                        y,
                        hs: 1.0,
                        vs: 1.0,
                    });
                    out.asc = out.asc.max(ink.max_y - y);
                }
            }
        }
        out
    }

    /// 배치한 수식을 개체 사각형 안에 그리는 명령으로 바꾼다. 배치 폭이 0이면 빈 목록이다.
    pub(super) fn ops(
        &mut self,
        frame: &Frame,
        rect: Rect,
        scale_y_baseline: f32,
        color: u32,
    ) -> Vec<Item> {
        let stale = is_stale(frame, rect);
        let (scale, left, base) = if stale {
            (1.0, rect.x, rect.y + frame.asc)
        } else {
            let scale = if rect.width > 0.0 {
                (rect.width / frame.width).clamp(0.8, 1.25)
            } else {
                1.0
            };
            (
                scale,
                rect.x + ((rect.width - frame.width * scale) / 2.0).max(0.0),
                rect.y + rect.height * scale_y_baseline,
            )
        };
        let mut items = Vec::new();
        for run in &frame.runs {
            if run.text.is_empty() {
                continue;
            }
            let (face, real_bold, synthetic_italic) = self.select(&run.text, run.face);
            let mut glyphs = Vec::new();
            let mut text = String::new();
            let mut ranges = Vec::new();
            let mut x = 0.0f32;
            for character in run.text.chars() {
                let Some(info) = self.fonts.glyph(face, character) else {
                    continue;
                };
                let start = text.len();
                text.push(character);
                ranges.push(start..text.len());
                glyphs.push((info.glyph, x));
                x += info.advance * run.size;
            }
            if glyphs.is_empty() {
                continue;
            }
            let baseline = base + run.y;
            let stroke = (run.face.bold && !real_bold).then_some(run.size * 0.02);
            if (run.vs - 1.0).abs() > 1e-4 {
                items.push(Item::Transform([
                    1.0,
                    0.0,
                    0.0,
                    run.vs,
                    0.0,
                    baseline * (1.0 - run.vs),
                ]));
            }
            items.push(Item::Glyphs(GlyphRun {
                face,
                size: run.size,
                origin: (left + run.x * scale, baseline),
                scale_x: scale * run.hs,
                italic: synthetic_italic,
                bold_stroke: stroke,
                color,
                glyphs,
                text,
                ranges,
            }));
            if (run.vs - 1.0).abs() > 1e-4 {
                items.push(Item::Pop);
            }
        }
        for rule in &frame.rules {
            if rule.width <= 0.0 || rule.height <= 0.0 {
                continue;
            }
            let area = Rect::new(
                left + rule.x * scale,
                base + rule.y,
                rule.width * scale,
                rule.height,
            );
            items.push(Item::Fill {
                path: scene::rect_path(area),
                color,
                alpha: 1.0,
            });
        }
        for seg in &frame.segs {
            let a = (left + seg.a.0 * scale, base + seg.a.1);
            let b = (left + seg.b.0 * scale, base + seg.b.1);
            items.push(Item::Stroke {
                path: vec![PathOp::Move(a.0, a.1), PathOp::Line(b.0, b.1)],
                stroke: Stroke {
                    width: seg.width,
                    color,
                    dash: Vec::new(),
                },
            });
        }
        items
    }
}

fn spacing(before: Cls, after: Cls) -> f32 {
    if before == Cls::Rel || after == Cls::Rel {
        return if before == after { 0.0 } else { REL_SPACE };
    }
    if before == Cls::Bin || after == Cls::Bin {
        return BIN_SPACE;
    }
    0.0
}
