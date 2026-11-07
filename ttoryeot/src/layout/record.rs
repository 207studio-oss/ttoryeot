//! 배치하면서 구성 요소(문단·표·칸·개체·머리말 등)가 쪽의 어디에 놓였는지 적는다. 그리는 쪽과 한 번의 배치를 나눠 쓴다.
//!
//! 틀(frame)을 열고 닫는 차례가 요소의 포함 관계다. 틀은 쪽마다 자기 노드를 따로 가지며(쪽을 넘는 문단·조각난 표),
//! 노드는 그 쪽에서 처음 자리나 그린 것이 생길 때 만들어진다. 자리를 한 번도 얻지 못한 노드는 버린다.

use super::{Renderer, pt};
use crate::components::{Component, Kind, PageComponents};
use crate::scene::{Item, Rect};
use hwp_core::{Paragraph, RunItem, Shape};
use std::collections::HashMap;

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
const SNIPPET_CHARS: usize = 24;
const MARKER: f32 = 8.0;
const MARKER_LIMIT: usize = 12;
const WHITE: u32 = 0x00FF_FFFF;
const PAGE: usize = 0;
const BODY: usize = 1;

#[derive(Clone, Copy, Default)]
pub(crate) struct Flags {
    pub floating: bool,
    pub behind: bool,
    pub hidden: bool,
}

struct Frame {
    kind: Kind,
    name: String,
    text: String,
    flags: Flags,
    nodes: HashMap<usize, usize>,
}

struct Node {
    kind: Kind,
    name: String,
    text: String,
    parent: Option<usize>,
    bounds: Option<Rect>,
    derived: bool,
    ink: bool,
    flags: Flags,
}

struct PageNodes {
    width: f32,
    height: f32,
    nodes: Vec<Node>,
}

#[derive(Default)]
pub(crate) struct Recorder {
    pub(crate) on: bool,
    pages: Vec<PageNodes>,
    stack: Vec<Frame>,
    maps: Vec<[f32; 6]>,
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect::new(
        x,
        y,
        a.right().max(b.right()) - x,
        a.bottom().max(b.bottom()) - y,
    )
}

fn compose(outer: [f32; 6], inner: [f32; 6]) -> [f32; 6] {
    let [oa, ob, oc, od, oe, of] = outer;
    let [a, b, c, d, e, f] = inner;
    [
        oa * a + oc * b,
        ob * a + od * b,
        oa * c + oc * d,
        ob * c + od * d,
        oa * e + oc * f + oe,
        ob * e + od * f + of,
    ]
}

fn visible(item: &Item) -> bool {
    match item {
        Item::Glyphs(run) if run.text.is_empty() => !run.glyphs.is_empty(),
        Item::Glyphs(run) => !run.text.trim().is_empty(),
        Item::Fill { color, alpha, .. } => *alpha > 0.0 && *color != WHITE,
        Item::Stroke { stroke, .. } => stroke.width > 0.0 && stroke.color != WHITE,
        Item::LinearFill { .. } | Item::Image { .. } => true,
        Item::Clip(_) | Item::Transform(_) | Item::Pop => false,
    }
}

pub(crate) fn snippet(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(SNIPPET_CHARS + 1)
        .collect();
    if out.chars().count() > SNIPPET_CHARS {
        out = out.chars().take(SNIPPET_CHARS).collect::<String>() + "…";
    }
    out.trim().to_string()
}

pub(crate) fn paragraph_text(para: &Paragraph) -> String {
    let mut text = String::new();
    'runs: for run in &para.runs {
        for item in &run.items {
            if let RunItem::Text(piece) = item {
                text.push_str(piece);
                if text.chars().count() > SNIPPET_CHARS {
                    break 'runs;
                }
            }
        }
    }
    snippet(&text)
}

fn shape_name(shape: &Shape) -> String {
    match shape.kind.as_str() {
        "rect" if !shape.paragraphs.is_empty() => "글상자".to_string(),
        "rect" => "사각형".to_string(),
        "line" => "선".to_string(),
        "ellipse" => "타원".to_string(),
        "polygon" => "다각형".to_string(),
        "curve" => "곡선".to_string(),
        "arc" => "호".to_string(),
        "ole" => "OLE 개체".to_string(),
        "container" => "묶음 개체".to_string(),
        other => other.to_string(),
    }
}

/// 쪽에 틀을 따로 여는 개체의 (종류, 이름, 덧붙임). 표는 조각마다 `draw_table_piece`가 연다.
pub(crate) fn object_label(item: &RunItem) -> Option<(Kind, String, String)> {
    match item {
        RunItem::Picture(_) => Some((Kind::Picture, "그림".to_string(), String::new())),
        RunItem::Shape(shape) => Some(match &shape.equation {
            Some(equation) => (
                Kind::Equation,
                "수식".to_string(),
                snippet(&equation.script),
            ),
            None => (Kind::Shape, shape_name(shape), String::new()),
        }),
        _ => None,
    }
}

/// 글 흐름에 있으나 자리가 없는 컨트롤의 이름.
pub(crate) fn controls(para: &Paragraph) -> Vec<String> {
    let mut out = Vec::new();
    for item in para.runs.iter().flat_map(|run| &run.items) {
        let name = match item {
            RunItem::Control(id) if id == "hp:fieldBegin" || id.starts_with('%') => {
                "필드 시작".to_string()
            }
            RunItem::Control(id) => format!("컨트롤 {id}"),
            RunItem::PageHiding {
                header,
                footer,
                page_num,
            } => {
                let hidden: Vec<&str> = [
                    (*header, "머리말"),
                    (*footer, "꼬리말"),
                    (*page_num, "쪽 번호"),
                ]
                .into_iter()
                .filter_map(|(on, name)| on.then_some(name))
                .collect();
                format!("감추기 ({})", hidden.join(", "))
            }
            RunItem::NewNum(number) => format!("새 번호 {number}"),
            RunItem::PageNum { .. } => "쪽 번호 위치".to_string(),
            RunItem::Header { .. } => "머리말 컨트롤".to_string(),
            RunItem::Footer { .. } => "꼬리말 컨트롤".to_string(),
            _ => continue,
        };
        out.push(name);
    }
    out
}

impl Recorder {
    pub(crate) fn reset(&mut self) {
        self.pages.clear();
        self.stack.clear();
        self.maps.clear();
    }

    pub(crate) fn new_page(&mut self, index: usize, width: f32, height: f32, body: Rect) {
        if !self.on {
            return;
        }
        let page_flags = Flags::default();
        let node = |kind: Kind, name: String, parent: Option<usize>, bounds: Rect| Node {
            kind,
            name,
            text: String::new(),
            parent,
            bounds: Some(bounds),
            derived: false,
            ink: false,
            flags: page_flags,
        };
        let page = node(
            Kind::Page,
            format!("{}쪽", index + 1),
            None,
            Rect::new(0.0, 0.0, width, height),
        );
        let body = node(Kind::Body, "본문".to_string(), Some(PAGE), body);
        self.pages.push(PageNodes {
            width,
            height,
            nodes: vec![page, body],
        });
    }

    pub(crate) fn begin(&mut self, kind: Kind, name: &str, text: &str, flags: Flags) {
        if self.on {
            self.stack.push(Frame {
                kind,
                name: name.to_string(),
                text: text.to_string(),
                flags,
                nodes: HashMap::new(),
            });
        }
    }

    pub(crate) fn end(&mut self) {
        if self.on {
            self.stack.pop();
        }
    }

    pub(crate) fn inside_object(&self) -> bool {
        self.stack.iter().any(|frame| frame.kind.is_object())
    }

    pub(crate) fn observe(&mut self, item: &Item) {
        if !self.on {
            return;
        }
        match item {
            Item::Clip(_) => self.maps.push(self.current()),
            Item::Transform(matrix) => self.maps.push(compose(self.current(), *matrix)),
            Item::Pop => {
                self.maps.pop();
            }
            _ => {}
        }
    }

    fn current(&self) -> [f32; 6] {
        self.maps.last().copied().unwrap_or(IDENTITY)
    }

    fn mapped(&self, rect: Rect) -> Rect {
        let [a, b, c, d, e, f] = self.current();
        let corners = [
            (rect.x, rect.y),
            (rect.right(), rect.y),
            (rect.x, rect.bottom()),
            (rect.right(), rect.bottom()),
        ]
        .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
        let min = |pick: fn(&(f32, f32)) -> f32| corners.iter().map(pick).fold(f32::MAX, f32::min);
        let max = |pick: fn(&(f32, f32)) -> f32| corners.iter().map(pick).fold(f32::MIN, f32::max);
        let (x, y) = (min(|p| p.0), min(|p| p.1));
        Rect::new(x, y, max(|p| p.0) - x, max(|p| p.1) - y)
    }

    fn ensure(&mut self, page: usize, depth: usize) -> Option<usize> {
        if let Some(&id) = self.stack[depth].nodes.get(&page) {
            return Some(id);
        }
        let parent = if depth == 0 {
            if self.stack[0].kind.is_furniture() {
                PAGE
            } else {
                BODY
            }
        } else {
            self.ensure(page, depth - 1)?
        };
        let frame = &self.stack[depth];
        let node = Node {
            kind: frame.kind,
            name: frame.name.clone(),
            text: frame.text.clone(),
            parent: Some(parent),
            bounds: None,
            derived: false,
            ink: false,
            flags: frame.flags,
        };
        let nodes = &mut self.pages.get_mut(page)?.nodes;
        nodes.push(node);
        let id = nodes.len() - 1;
        self.stack[depth].nodes.insert(page, id);
        Some(id)
    }

    pub(crate) fn extend(&mut self, page: usize, rect: Rect) {
        if !self.on
            || self.stack.is_empty()
            || ![rect.x, rect.y, rect.width, rect.height]
                .iter()
                .all(|v| v.is_finite())
        {
            return;
        }
        let rect = self.mapped(rect);
        let top = self.stack.len() - 1;
        let Some(id) = self.ensure(page, top) else {
            return;
        };
        let node = &mut self.pages[page].nodes[id];
        node.bounds = Some(node.bounds.map_or(rect, |bounds| union(bounds, rect)));
        node.derived = false;
    }

    pub(crate) fn paint(&mut self, page: usize, item: &Item) {
        if visible(item) {
            self.ink(page);
        }
    }

    /// 지금 틀에 눈에 보이는 내용이 생겼다고 적는다. 안쪽 틀에서 바깥 틀로 올리되, 개체의 잉크는 그 개체를 품은 문단에 올리지 않는다.
    pub(crate) fn ink(&mut self, page: usize) {
        if !self.on || self.stack.is_empty() {
            return;
        }
        let mut depth = self.stack.len() - 1;
        loop {
            if let Some(id) = self.ensure(page, depth) {
                self.pages[page].nodes[id].ink = true;
            }
            if depth == 0
                || (self.stack[depth - 1].kind == Kind::Paragraph
                    && self.stack[depth].kind.is_object())
            {
                break;
            }
            depth -= 1;
        }
    }

    /// 지금 틀(문단)의 왼쪽 바깥에 자리 없는 컨트롤의 표식을 놓는다. 틀이 가장 먼저 놓인 쪽에 둔다.
    pub(crate) fn markers(&mut self, names: &[String]) {
        if !self.on || names.is_empty() {
            return;
        }
        let Some(frame) = self.stack.last() else {
            return;
        };
        let Some((&page, &id)) = frame.nodes.iter().min_by_key(|(page, _)| **page) else {
            return;
        };
        let Some(anchor) = self.pages[page].nodes[id].bounds else {
            return;
        };
        let shown = names.len().min(MARKER_LIMIT);
        for (index, name) in names.iter().take(shown).enumerate() {
            let name = if index + 1 == shown && names.len() > shown {
                format!("{name} 외 {}개", names.len() - shown)
            } else {
                name.clone()
            };
            self.pages[page].nodes.push(Node {
                kind: Kind::Control,
                name,
                text: String::new(),
                parent: Some(id),
                bounds: Some(Rect::new(
                    (anchor.x - MARKER - 2.0).max(0.0),
                    anchor.y + index as f32 * (MARKER + 1.0),
                    MARKER,
                    MARKER,
                )),
                derived: false,
                ink: false,
                flags: Flags {
                    hidden: true,
                    ..Flags::default()
                },
            });
        }
    }

    pub(crate) fn finish(&mut self, scales: &[f32]) -> Vec<PageComponents> {
        let pages = std::mem::take(&mut self.pages);
        pages
            .into_iter()
            .enumerate()
            .map(|(index, page)| finish_page(page, scales.get(index).copied().unwrap_or(1.0)))
            .collect()
    }
}

fn finish_page(page: PageNodes, scale: f32) -> PageComponents {
    let PageNodes {
        width,
        height,
        mut nodes,
    } = page;
    for index in (0..nodes.len()).rev() {
        let Some(bounds) = nodes[index].bounds else {
            continue;
        };
        if let Some(parent) = nodes[index].parent
            && (nodes[parent].bounds.is_none() || nodes[parent].derived)
        {
            let merged = nodes[parent]
                .bounds
                .map_or(bounds, |own| union(own, bounds));
            nodes[parent].bounds = Some(merged);
            nodes[parent].derived = true;
        }
    }
    let mut remap: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut components: Vec<Component> = Vec::new();
    for (index, node) in nodes.into_iter().enumerate() {
        let Some(mut bounds) = node.bounds else {
            continue;
        };
        if node.kind != Kind::Page {
            bounds = Rect::new(
                bounds.x * scale,
                bounds.y * scale,
                bounds.width * scale,
                bounds.height * scale,
            );
        }
        let parent = node.parent.and_then(|parent| remap[parent]);
        let depth = parent.map_or(0, |parent| components[parent].depth + 1);
        let off_page =
            bounds.right() < 0.0 || bounds.bottom() < 0.0 || bounds.x > width || bounds.y > height;
        remap[index] = Some(components.len());
        components.push(Component {
            id: components.len(),
            parent,
            depth,
            kind: node.kind,
            name: node.name,
            text: node.text,
            bounds,
            ink: node.ink,
            floating: node.flags.floating,
            behind: node.flags.behind,
            hidden: node.flags.hidden || off_page,
        });
    }
    PageComponents {
        width,
        height,
        components,
    }
}

impl Kind {
    fn is_furniture(self) -> bool {
        matches!(
            self,
            Kind::Header | Kind::Footer | Kind::MasterPage | Kind::PageNumber | Kind::Footnote
        )
    }
}

impl Renderer<'_> {
    pub(super) fn rec_extend(&mut self, rect: Rect) {
        if let Some(page) = self.page {
            self.rec.extend(page, rect);
        }
    }

    pub(super) fn rec_body_rect(section: &hwp_core::Section) -> Rect {
        let page = &section.page;
        Rect::new(
            pt(page.margin_left + page.gutter),
            pt(page.margin_top + page.header),
            pt(page.width - page.margin_left - page.margin_right - page.gutter).max(0.0),
            pt(page.height - page.margin_top - page.header - page.margin_bottom - page.footer)
                .max(0.0),
        )
    }
}
