//! 문서 모델을 쪽마다의 그릴 것 목록으로 바꾼다. 줄 자리는 파일에 저장된 줄 배치(lineseg)를 그대로 쓰고,
//! 줄 배치가 없는 문단만 글자 폭으로 줄을 나눈다. 좌표는 pt, 원점은 쪽 왼쪽 위.

mod object;
mod record;
mod table;
pub(crate) mod text;

use crate::components::{Kind, PageComponents};
use crate::fonts::{FaceId, FontBook};
use crate::scene::{self, Item, PathOp};
use crate::subst::Rules;
use hwp_core::{
    Bullet, CharPr, Document, LineSeg, Note, Numbering, ParaPr, Paragraph, RunItem, Section, TabPr,
};
use record::Flags;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use text::{Atom, Style};

/// HWPUNIT(1/7200 인치) → pt.
pub(crate) fn pt(value: f64) -> f32 {
    (value / 100.0) as f32
}

/// 테두리·선 굵기(mm) → pt.
pub(crate) fn mm(value: f64) -> f32 {
    (value * 72.0 / 25.4) as f32
}

fn default_char_pr() -> &'static CharPr {
    static VALUE: OnceLock<CharPr> = OnceLock::new();
    VALUE.get_or_init(CharPr::default)
}

fn default_para_pr() -> &'static ParaPr {
    static VALUE: OnceLock<ParaPr> = OnceLock::new();
    VALUE.get_or_init(ParaPr::default)
}

struct PageOut {
    width: f32,
    height: f32,
    section: usize,
    /// 글 뒤에 놓는 개체. 쪽을 마칠 때 글보다 먼저 그린다.
    back: Vec<Item>,
    items: Vec<Item>,
    text: Vec<String>,
}

#[derive(Clone, Copy, Default)]
struct Hiding {
    header: bool,
    footer: bool,
    page_num: bool,
}

type HeaderEvent<'a> = (&'a str, &'a [Paragraph]);
/// 각주, 그 각주를 쌓을 단의 왼쪽(pt), 단 너비(pt).
type FootnoteSpot<'a> = (&'a Note, f32, f32);
/// 풀어 낸 그림과 그 픽셀 크기.
type Decoded = (Arc<scene::ImageData>, (u32, u32));

/// 쪽 배치 결과와, 최상위 문단이 놓인 쪽의 대응.
pub(crate) struct Layout {
    pub(crate) pages: Vec<scene::Page>,
    pub(crate) paragraphs: Vec<ParagraphSpan>,
    /// 쪽마다 그린 줄의 글자. [`Renderer::recording_text`]를 불렀을 때만 찬다.
    pub(crate) text: Vec<Vec<String>>,
    /// 쪽마다 배치한 구성 요소. [`Renderer::recording_components`]를 불렀을 때만 찬다.
    pub(crate) components: Vec<PageComponents>,
}

/// 최상위 문단 하나가 차지한 쪽 범위(0부터, 끝 포함).
pub(crate) struct ParagraphSpan {
    pub(crate) section: usize,
    pub(crate) paragraph: usize,
    pub(crate) first_page: usize,
    pub(crate) last_page: usize,
}

pub(crate) struct Renderer<'a> {
    doc: &'a Document,
    fonts: &'a mut FontBook,
    rules: &'static Rules,
    char_prs: HashMap<i32, &'a CharPr>,
    para_prs: HashMap<i32, &'a ParaPr>,
    tab_prs: HashMap<i32, &'a TabPr>,
    border_fills: HashMap<i32, &'a hwp_core::BorderFill>,
    numberings: HashMap<i32, &'a Numbering>,
    bullets: HashMap<i32, &'a Bullet>,
    pages: Vec<PageOut>,
    paragraph_spans: Vec<ParagraphSpan>,
    records_text: bool,
    rec: record::Recorder,
    page: Option<usize>,
    /// 지금 그리는 개체가 글 뒤에 놓이는가.
    behind: bool,
    object_bottom: f64,
    object_top: f64,
    /// 지금 쪽 본문에 놓인 개체의 바깥 여백까지 넓힌 자리(pt).
    floats: Vec<scene::Rect>,
    column: usize,
    column_count: usize,
    column_width: f32,
    column_step: f32,
    text_clip_bottom: Option<f32>,
    in_body: bool,
    total_pages: Option<usize>,
    saw_early_total: bool,
    display_page_number: Option<i64>,
    tab_interval: f32,
    page_hide: HashMap<usize, Hiding>,
    page_num_spec: HashMap<usize, (&'a str, &'a str, &'a str)>,
    page_new_num: HashMap<usize, i32>,
    page_headers: HashMap<usize, Vec<HeaderEvent<'a>>>,
    page_footers: HashMap<usize, Vec<HeaderEvent<'a>>>,
    page_footnotes: BTreeMap<usize, Vec<FootnoteSpot<'a>>>,
    placed_footnotes: HashSet<(usize, i32, i32)>,
    styles: HashMap<(i32, &'static str, bool), Style>,
    glyph_fallbacks: Option<Vec<FaceId>>,
    images: HashMap<String, Option<Decoded>>,
    /// 표 주소 → (열 너비, 행 높이). 칸 글 높이를 재므로 한 번만 구한다.
    grids: HashMap<usize, (Vec<f64>, Vec<f64>)>,
    /// 지금 그리는 각주·미주의 번호. 그 안의 자동 번호가 이 값을 찍는다.
    current_note: Option<String>,
    /// 본문에서 마지막 문단의 아래 + 문단 뒤 간격(HWPUNIT, 지금 단 위 기준). 줄 배치가 없는 문단이 여기서 시작한다.
    para_bottom: f64,
    /// 쪽 → 그 쪽에서 여러 단에 놓인 본문의 (위, 아래) pt. 단 구분선을 이 범위에 긋는다.
    column_extents: BTreeMap<usize, (f32, f32)>,
    /// 칸 글을 다시 쌓아 커진 표(`table::restacks`) 뒤에서, 지금 단의 본문 줄을 저장된 자리보다 내리는 값(HWPUNIT).
    /// 저장된 배치가 단·쪽을 넘기면 0으로 돌아간다.
    stack_shift: f64,
    /// `stack_shift`가 있을 때 마지막 본문 문단의 실제 아래 + 문단 뒤 간격(HWPUNIT). 한/글은 다음 문단을 저장된 자리가 아니라 여기에 쌓는다.
    stacked_bottom: Option<f64>,
}

impl<'a> Renderer<'a> {
    pub(crate) fn new(doc: &'a Document, fonts: &'a mut FontBook) -> Self {
        fn index<T>(values: &[T], id: impl Fn(&T) -> i32) -> HashMap<i32, &T> {
            let mut map = HashMap::new();
            for value in values {
                map.entry(id(value)).or_insert(value);
            }
            map
        }
        Self {
            doc,
            fonts,
            rules: Rules::builtin(),
            char_prs: index(&doc.char_prs, |value| value.id),
            para_prs: index(&doc.para_prs, |value| value.id),
            tab_prs: index(&doc.tab_prs, |value| value.id),
            border_fills: index(&doc.border_fills, |value| value.id),
            numberings: index(&doc.numberings, |value| value.id),
            bullets: index(&doc.bullets, |value| value.id),
            pages: Vec::new(),
            paragraph_spans: Vec::new(),
            records_text: false,
            rec: record::Recorder::default(),
            page: None,
            behind: false,
            object_bottom: 0.0,
            object_top: 0.0,
            floats: Vec::new(),
            column: 0,
            column_count: 1,
            column_width: 0.0,
            column_step: 0.0,
            text_clip_bottom: None,
            in_body: false,
            total_pages: None,
            saw_early_total: false,
            display_page_number: None,
            tab_interval: 40.0,
            page_hide: HashMap::new(),
            page_num_spec: HashMap::new(),
            page_new_num: HashMap::new(),
            page_headers: HashMap::new(),
            page_footers: HashMap::new(),
            page_footnotes: BTreeMap::new(),
            placed_footnotes: HashSet::new(),
            styles: HashMap::new(),
            glyph_fallbacks: None,
            images: HashMap::new(),
            grids: HashMap::new(),
            current_note: None,
            para_bottom: 0.0,
            column_extents: BTreeMap::new(),
            stack_shift: 0.0,
            stacked_bottom: None,
        }
    }

    fn char_pr(&self, id: i32) -> &'a CharPr {
        self.char_prs.get(&id).copied().unwrap_or(default_char_pr())
    }

    fn para_pr(&self, id: i32) -> &'a ParaPr {
        self.para_prs.get(&id).copied().unwrap_or(default_para_pr())
    }

    /// 쪽을 배치한다. 본문에 '전체 쪽수'가 앞쪽에서 쓰였으면 한 번 더 배치해 확정 쪽수를 찍는다.
    pub(crate) fn recording_text(mut self) -> Self {
        self.records_text = true;
        self
    }

    pub(crate) fn recording_components(mut self) -> Self {
        self.rec.on = true;
        self
    }

    pub(crate) fn layout(mut self) -> Layout {
        self.total_pages = None;
        self.saw_early_total = false;
        self.layout_pass();
        if self.saw_early_total {
            self.total_pages = Some(self.pages.len());
            self.layout_pass();
        }
        let sections = &self.doc.sections;
        let track_changes = self.doc.track_changes;
        let scales: Vec<f32> = self
            .pages
            .iter()
            .map(|page| {
                sections.get(page.section).map_or(1.0, |section| {
                    page_scale(track_changes, page.width, section)
                })
            })
            .collect();
        let components = self.rec.finish(&scales);
        let mut text = Vec::with_capacity(self.pages.len());
        let pages = self
            .pages
            .into_iter()
            .zip(scales)
            .map(|(page, scale)| {
                text.push(page.text);
                let mut items = page.back;
                items.extend(page.items);
                scene::Page {
                    width: page.width,
                    height: page.height,
                    scale,
                    items,
                }
            })
            .collect();
        Layout {
            pages,
            paragraphs: self.paragraph_spans,
            text,
            components,
        }
    }

    fn layout_pass(&mut self) {
        self.pages.clear();
        self.rec.reset();
        self.paragraph_spans.clear();
        self.page = None;
        self.object_bottom = 0.0;
        self.object_top = 0.0;
        self.stack_shift = 0.0;
        self.stacked_bottom = None;
        self.page_hide.clear();
        self.page_num_spec.clear();
        self.page_new_num.clear();
        self.page_headers.clear();
        self.page_footers.clear();
        self.page_footnotes.clear();
        self.placed_footnotes.clear();
        self.column_extents.clear();
        self.display_page_number = None;
        self.in_body = true;
        let doc = self.doc;
        for (index, section) in doc.sections.iter().enumerate() {
            self.new_page(section, index);
            let first_page = self.pages.len() - 1;
            self.object_bottom = 0.0;
            self.object_top = 0.0;
            self.stack_shift = 0.0;
            self.stacked_bottom = None;
            let page = &section.page;
            let origin = (
                pt(page.margin_left + page.gutter),
                pt(page.margin_top + page.header),
            );
            let body_width = pt(page.width - page.margin_left - page.margin_right - page.gutter);
            self.column_count = section.columns.max(1) as usize;
            self.column = 0;
            let gap = if self.column_count > 1 {
                pt(section.column_gap)
            } else {
                0.0
            };
            self.column_width =
                (body_width - gap * (self.column_count - 1) as f32) / self.column_count as f32;
            self.column_step = self.column_width + gap;
            let mut last_vert = -1.0;
            self.para_bottom = 0.0;
            let mut after_flowed = false;
            for (paragraph, para) in section.paragraphs.iter().enumerate() {
                // 줄 배치가 없어 글자 폭으로 나눈 문단(글자 수를 바꿔 줄 배치를 지운 문단 등) 뒤의 저장된 문단은
                // 저장된 자리가 아니라 그 문단의 실제 아래에 쌓는다. 그러지 않으면 늘어난 줄과 겹친다.
                if after_flowed && !para.line_segs.is_empty() {
                    self.stacked_bottom = Some(self.para_bottom + self.stack_shift);
                }
                after_flowed = para.line_segs.is_empty();
                let start = self.pages.len() - 1;
                let drawn = self.drawn_items(start);
                self.layout_paragraph(
                    para,
                    origin,
                    self.column_width,
                    section,
                    true,
                    &mut last_vert,
                    None,
                );
                let last_page = self.pages.len() - 1;
                // 첫 줄이 들어가지 않아 다음 쪽에서 시작한 문단은 앞쪽을 차지하지 않는다.
                let moved = last_page > start && self.drawn_items(start) == drawn;
                self.paragraph_spans.push(ParagraphSpan {
                    section: index,
                    paragraph,
                    first_page: start + usize::from(moved),
                    last_page,
                });
            }
            self.draw_column_lines(section, first_page, origin.0);
            self.column_count = 1;
            self.column = 0;
        }
        self.in_body = false;
        self.draw_footnotes();
        self.draw_page_furniture();
    }

    fn drawn_items(&self, page: usize) -> usize {
        self.pages[page].back.len() + self.pages[page].items.len()
    }

    fn new_page(&mut self, section: &Section, index: usize) {
        self.pages.push(PageOut {
            width: pt(section.page.width),
            height: pt(section.page.height),
            section: index,
            back: Vec::new(),
            items: Vec::new(),
            text: Vec::new(),
        });
        let index = self.pages.len() - 1;
        self.page = Some(index);
        self.floats.clear();
        let out = &self.pages[index];
        self.rec
            .new_page(index, out.width, out.height, Self::rec_body_rect(section));
    }

    /// 다음 단으로, `same_page`가 아니거나 남은 단이 없으면 새 쪽 첫 단으로 넘어간다.
    fn next_column(&mut self, section: &Section, same_page: bool) {
        if same_page && self.column + 1 < self.column_count {
            self.column += 1;
        } else {
            let index = self.page.map_or(0, |page| self.pages[page].section);
            self.new_page(section, index);
            self.column = 0;
        }
        self.object_bottom = 0.0;
        self.object_top = 0.0;
        self.stack_shift = 0.0;
        self.stacked_bottom = None;
    }

    fn record_text(&mut self, line: String) {
        if let Some(page) = self.page.and_then(|index| self.pages.get_mut(index)) {
            page.text.push(line);
        }
    }

    fn add(&mut self, item: Item) {
        let behind = self.behind;
        if let Some(page) = self.page {
            self.rec.observe(&item);
            self.rec.paint(page, &item);
        }
        if let Some(page) = self.page.and_then(|index| self.pages.get_mut(index)) {
            if behind {
                page.back.push(item);
            } else {
                page.items.push(item);
            }
        }
    }

    fn fill(&mut self, rect: scene::Rect, color: u32, alpha: f32) {
        if rect.width > 0.0 && rect.height > 0.0 {
            self.add(Item::Fill {
                path: scene::rect_path(rect),
                color,
                alpha,
            });
        }
    }

    /// 칸 배경을 그러데이션으로 칠한다. 원형(`RADIAL`)과 0도 줄(`LINEAR`)만 한/글처럼 그리고, 다른 꼴은 첫 색으로 칠한다.
    ///
    /// 한/글 12.30.0은 원형 그러데이션을 `step`개의 동심원으로 그린다. 중심은 칸 왼쪽 위에서
    /// `centerX`·`centerY` %만큼 간 곳이고, 가장 큰 원의 반지름은 중심에서 가장 먼 칸 모서리까지의
    /// 가로·세로 거리 중 큰 쪽이다. 바깥 원부터 끝 색으로 칠하고 안쪽으로 갈수록 첫 색에 다가간다.
    /// 칸 밖으로 넘친 원은 칸으로 오려 낸다.
    fn gradient_fill(
        &mut self,
        rect: scene::Rect,
        fallback: Option<u32>,
        gradient: &hwp_core::Gradient,
    ) {
        let steps = usize::try_from(gradient.step).unwrap_or(0);
        if gradient.kind == "LINEAR" && gradient.angle == 0.0 && gradient.colors.len() >= 2 {
            self.linear_fill(rect, gradient);
            return;
        }
        if gradient.kind != "RADIAL" || steps < 2 || gradient.colors.len() < 2 {
            if let Some(color) = fallback {
                self.fill(rect, color, 1.0);
            }
            return;
        }
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let cx = rect.x + rect.width * gradient.center_x as f32 / 100.0;
        let cy = rect.y + rect.height * gradient.center_y as f32 / 100.0;
        let radius = [rect.x, rect.right()]
            .iter()
            .map(|x| (x - cx).abs())
            .chain([rect.y, rect.bottom()].iter().map(|y| (y - cy).abs()))
            .fold(0.0f32, f32::max);
        self.add(Item::Clip(rect));
        for ring in 0..steps {
            let r = radius * (steps - ring) as f32 / steps as f32;
            let t = (steps - 1 - ring) as f32 / (steps - 1) as f32;
            let color = gradient_color(&gradient.colors, t);
            self.add(Item::Fill {
                path: text::ellipse(scene::Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r)),
                color,
                alpha: 1.0,
            });
        }
        self.add(Item::Pop);
    }

    /// 0도 줄 그러데이션. 한/글 12.30.0은 칸 위에서 `stepCenter` %만큼 내려간 줄에 첫 색을 두고
    /// 위·아래 가장자리로 갈수록 끝 색에 다가가게, 그 줄을 축으로 접어 그린다.
    fn linear_fill(&mut self, rect: scene::Rect, gradient: &hwp_core::Gradient) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let stops = linear_stops(&gradient.colors, gradient.step_center as f32 / 100.0);
        let x = rect.x + rect.width / 2.0;
        self.add(Item::LinearFill {
            rect,
            from: (x, rect.y),
            to: (x, rect.bottom()),
            stops,
        });
    }

    fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: u32, dash: Vec<f32>) {
        if width <= 0.0 {
            return;
        }
        self.add(Item::Stroke {
            path: vec![PathOp::Move(from.0, from.1), PathOp::Line(to.0, to.1)],
            stroke: scene::Stroke { width, color, dash },
        });
    }

    /// 여러 단으로 놓인 본문이 지금 쪽에서 차지한 세로 범위(pt)를 넓힌다.
    fn extend_columns(&mut self, top: f32, bottom: f32) {
        if self.column_count < 2 || !self.in_body {
            return;
        }
        if let Some(page) = self.page {
            let extent = self.column_extents.entry(page).or_insert((top, bottom));
            extent.0 = extent.0.min(top);
            extent.1 = extent.1.max(bottom);
        }
    }

    /// 단 구분선을 단 사이 간격 가운데에, 쪽마다 본문이 여러 단으로 놓인 범위만큼 위에서 아래로 긋는다.
    fn draw_column_lines(&mut self, section: &Section, first_page: usize, left: f32) {
        let Some(columns) = section
            .paragraphs
            .iter()
            .find_map(|para| para.columns.as_ref())
        else {
            return;
        };
        let border = hwp_core::Border {
            // 한/글 PDF는 단 구분선의 점선(DOT)을 파선(DASH)과 같은 무늬로 긋는다.
            border_type: match columns.line_type.as_str() {
                "DOT" => "DASH".to_string(),
                other => other.to_string(),
            },
            width: columns.line_width,
            color: columns.line_color,
        };
        let gap = self.column_step - self.column_width;
        let extents: Vec<(usize, (f32, f32))> = self
            .column_extents
            .range(first_page..)
            .map(|(page, extent)| (*page, *extent))
            .collect();
        let page = self.page;
        for (index, (top, bottom)) in extents {
            self.page = Some(index);
            for column in 1..self.column_count {
                let x = left + column as f32 * self.column_step - gap / 2.0;
                self.border_line(&border, (x, top), (x, bottom));
            }
        }
        self.page = page;
    }

    fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// 머리말·꼬리말·쪽 번호를 쪽마다 그린다. 쪽 번호는 새 번호 컨트롤을 따르고, 감추기는 그 쪽에만 적용한다.
    fn draw_page_furniture(&mut self) {
        let doc = self.doc;
        let mut number: i64 = 0;
        let mut current_page_num: Option<(&'a str, &'a str, &'a str)> = None;
        let mut running_headers: Vec<HeaderEvent<'a>> = Vec::new();
        let mut running_footers: Vec<HeaderEvent<'a>> = Vec::new();
        let mut running_section = None;
        let mut has_header_controls = false;
        let mut has_footer_controls = false;
        let mut nested_header_applies = None;
        let mut nested_footer_applies = None;
        for index in 0..self.pages.len() {
            let section_index = self.pages[index].section;
            let section = &doc.sections[section_index];
            if running_section != Some(section_index) {
                running_headers.clear();
                running_footers.clear();
                running_section = Some(section_index);
                has_header_controls =
                    section_has(section, |item| matches!(item, RunItem::Header { .. }));
                has_footer_controls =
                    section_has(section, |item| matches!(item, RunItem::Footer { .. }));
                nested_header_applies = first_applies(&section.paragraphs, true);
                nested_footer_applies = first_applies(&section.paragraphs, false);
            }
            running_headers.extend(self.page_headers.get(&index).into_iter().flatten().copied());
            running_footers.extend(self.page_footers.get(&index).into_iter().flatten().copied());
            let saved = self.page;
            self.page = Some(index);
            number = self
                .page_new_num
                .get(&index)
                .map_or(number.saturating_add(1), |value| i64::from(*value));
            if let Some(spec) = self.page_num_spec.get(&index) {
                current_page_num = Some(*spec);
            }
            let hide = self.page_hide.get(&index).copied().unwrap_or_default();
            self.display_page_number = Some(number);
            let odd = number.rem_euclid(2) == 1;
            let pick = |defs: &[HeaderEvent<'a>]| -> Option<&'a [Paragraph]> {
                defs.iter().rev().find_map(|(kind, paragraphs)| {
                    (*kind == "BOTH" || (*kind == "ODD" && odd) || (*kind == "EVEN" && !odd))
                        .then_some(*paragraphs)
                })
            };
            // 본문에 컨트롤이 없으면 구역의 머리말·꼬리말을 쓰되, 그것을 정한 컨트롤이 홀수·짝수 쪽 전용이면 그 쪽에만 둔다.
            let fallback = |applies: Option<&str>, paragraphs: &'a [Paragraph]| match applies {
                Some("ODD") if !odd => None,
                Some("EVEN") if odd => None,
                _ => Some(paragraphs),
            };
            let headers = if has_header_controls {
                pick(&running_headers)
            } else {
                fallback(nested_header_applies, &section.header)
            };
            let footers = if has_footer_controls {
                pick(&running_footers)
            } else {
                fallback(nested_footer_applies, &section.footer)
            };
            let page = &section.page;
            let left = pt(page.margin_left + page.gutter);
            let width = pt(page.width - page.margin_left - page.margin_right - page.gutter);
            // 바탕쪽은 본문 자리에 놓는다. 홀수·짝수 바탕쪽이 있으면 양쪽 바탕쪽보다 앞선다.
            let master = section
                .master_pages
                .iter()
                .rev()
                .find(|master| {
                    (master.applies == "ODD" && odd) || (master.applies == "EVEN" && !odd)
                })
                .or_else(|| {
                    section
                        .master_pages
                        .iter()
                        .find(|master| master.applies == "BOTH")
                });
            if let Some(master) = master {
                self.rec.begin(
                    Kind::MasterPage,
                    "바탕쪽",
                    "",
                    Flags {
                        behind: true,
                        ..Flags::default()
                    },
                );
                self.rec_extend(Self::rec_body_rect(section));
                let (items, back) = {
                    let out = &self.pages[index];
                    (out.items.len(), out.back.len())
                };
                let mut last_vert = -1.0;
                for para in &master.paragraphs {
                    self.layout_paragraph(
                        para,
                        (left, pt(page.margin_top + page.header)),
                        width,
                        section,
                        false,
                        &mut last_vert,
                        None,
                    );
                }
                // 바탕쪽은 글 뒤 개체보다도 뒤에 깔린다.
                let out = &mut self.pages[index];
                let mut under: Vec<Item> = out.back.drain(back..).collect();
                under.extend(out.items.drain(items..));
                out.back.splice(0..0, under);
                self.rec.end();
            }
            if let Some(headers) = headers {
                self.rec.begin(
                    Kind::Header,
                    "머리말",
                    "",
                    Flags {
                        hidden: hide.header,
                        ..Flags::default()
                    },
                );
                self.rec_extend(scene::Rect::new(
                    left,
                    pt(page.margin_top),
                    width,
                    pt(page.header),
                ));
                if !hide.header {
                    let mut last_vert = -1.0;
                    for para in headers {
                        self.layout_paragraph(
                            para,
                            (left, pt(page.margin_top)),
                            width,
                            section,
                            false,
                            &mut last_vert,
                            None,
                        );
                    }
                }
                self.rec.end();
            }
            if let Some(footers) = footers {
                // 한/글은 꼬리말 칸에 들어가는 내용을 칸 아래에 붙이고, 넘치는 내용은 칸 위에서부터 놓는다.
                let room = pt(page.footer);
                let area_top = pt(page.height - page.margin_bottom) - room;
                self.rec.begin(
                    Kind::Footer,
                    "꼬리말",
                    "",
                    Flags {
                        hidden: hide.footer,
                        ..Flags::default()
                    },
                );
                self.rec_extend(scene::Rect::new(left, area_top, width, room));
                if !hide.footer {
                    let content = self.footer_height(footers, section, left, width);
                    let top = area_top + (room - content).max(0.0);
                    let mut last_vert = -1.0;
                    for para in footers {
                        self.layout_paragraph(
                            para,
                            (left, top),
                            width,
                            section,
                            false,
                            &mut last_vert,
                            None,
                        );
                    }
                }
                self.rec.end();
            }
            if let Some(spec) = current_page_num {
                self.draw_page_number(spec, number, section, !hide.page_num);
            }
            self.display_page_number = None;
            self.page = saved;
        }
    }

    fn draw_page_number(
        &mut self,
        (pos, format, side): (&str, &str, &str),
        number: i64,
        section: &Section,
        visible: bool,
    ) {
        // 번호 위치 '없음'은 쪽 번호를 찍지 않는다.
        if pos == "NONE" {
            return;
        }
        let digits = hwp_core::num::format_number(number, format);
        let content = if side.is_empty() {
            digits
        } else {
            format!("{side} {digits} {side}")
        };
        let page = &section.page;
        // 한/글은 쪽 번호를 '쪽 번호' 스타일의 글자 모양으로 찍는다(없으면 글자 모양 0).
        let char_pr = self
            .doc
            .styles
            .iter()
            .find(|style| style.eng_name == "Page Number")
            .map_or(0, |style| style.char_pr_id);
        let height = pt(self.char_pr(char_pr).height);
        let atoms: Vec<Atom<'a>> = content.chars().map(|ch| Atom::plain(ch, char_pr)).collect();
        let line = self.shape(&atoms, None, text::Advances::Exact);
        let natural = line.width;
        // 아래 쪽 번호는 글꼴 descent만큼 띄워 본문 아래 여백선 위에 앉는다(12.30.0 기준 PDF에서 잼).
        let baseline = if pos.starts_with("TOP") {
            pt(page.margin_top) + height * 0.8
        } else {
            pt(page.height - page.margin_bottom) - line.descent()
        };
        let left = pt(page.margin_left + page.gutter);
        let width = pt(page.width - page.margin_left - page.margin_right - page.gutter);
        let x = if pos.ends_with("RIGHT") {
            left + width - natural
        } else if pos.ends_with("CENTER") {
            left + (width - natural) / 2.0
        } else {
            left
        };
        self.rec.begin(
            Kind::PageNumber,
            "쪽 번호",
            &record::snippet(&content),
            Flags {
                hidden: !visible,
                ..Flags::default()
            },
        );
        self.rec_extend(scene::Rect::new(
            x,
            baseline - height * 0.8,
            natural,
            height,
        ));
        if visible {
            self.emit(&line, x, baseline);
        }
        self.rec.end();
    }

    fn record_footnote(&mut self, note: &'a Note, section: &Section) {
        let Some(page) = self.page else {
            return;
        };
        if !self
            .placed_footnotes
            .insert((self.pages[page].section, note.seq, note.number))
        {
            return;
        }
        let body = &section.page;
        let x = pt(body.margin_left + body.gutter) + self.column as f32 * self.column_step;
        let width = if self.column_width > 0.0 {
            self.column_width
        } else {
            pt(body.width - body.margin_left - body.margin_right - body.gutter)
        };
        self.page_footnotes
            .entry(page)
            .or_default()
            .push((note, x, width));
    }

    /// 각주 하나의 높이(pt): 내용 문단 줄 배치의 가장 아래. 줄 배치가 없는 문단은 그릴 때처럼 흘려서 잰다.
    fn footnote_height(&mut self, note: &'a Note, width: f32) -> f32 {
        let mut last_vert = -1.0f64;
        let mut bottom: Option<f64> = None;
        for para in &note.paragraphs {
            let pp = self.para_pr(para.para_pr_id);
            let segs = if para.line_segs.is_empty() {
                self.flow_segs(para, f64::from(width) * 100.0, last_vert.max(0.0) + pp.prev)
            } else {
                para.line_segs.clone()
            };
            for seg in &segs {
                bottom = Some(bottom.map_or(seg.vertpos + seg.vertsize, |b: f64| {
                    b.max(seg.vertpos + seg.vertsize)
                }));
            }
            if let Some(last) = segs.last() {
                last_vert = last.vertpos + last.vertsize + last.spacing + pp.next;
            }
        }
        bottom.map_or(10.0, pt)
    }

    /// 각주들이 본문 아래에서 차지하는 높이(HwpUnit): 구분선 위·아래 여백, 선 굵기, 각주 높이와 각주 사이.
    fn footnote_reserve(&mut self, notes: &[&'a Note], section: &Section, width: f32) -> f64 {
        if notes.is_empty() {
            return 0.0;
        }
        let shape = &section.footnote_shape;
        let heights: f32 = notes
            .iter()
            .map(|note| self.footnote_height(note, width))
            .sum();
        f64::from(heights) * 100.0
            + shape.between_notes * (notes.len() - 1) as f64
            + shape.above_line
            + shape.below_line
            + f64::from(2.0 * mm(shape.line_width)) * 100.0
    }

    /// 쪽마다 각주를 본문 아래에 쌓는다. 마지막 각주의 아래가 본문 영역의 아래에 맞고, 구분선은 첫 각주 위에 긋는다.
    fn draw_footnotes(&mut self) {
        let saved = self.page;
        let doc = self.doc;
        let notes: Vec<(usize, Vec<FootnoteSpot<'a>>)> = std::mem::take(&mut self.page_footnotes)
            .into_iter()
            .collect();
        for (index, list) in &notes {
            self.page = Some(*index);
            let section = &doc.sections[self.pages[*index].section];
            let shape = &section.footnote_shape;
            let page = &section.page;
            let bottom = pt(page.height - page.margin_bottom - page.footer);
            let mut columns: Vec<f32> = Vec::new();
            for (_, x, _) in list {
                if !columns.iter().any(|seen| (seen - x).abs() < 0.5) {
                    columns.push(*x);
                }
            }
            for column_x in columns {
                let in_column: Vec<_> = list
                    .iter()
                    .filter(|(_, x, _)| (x - column_x).abs() < 0.5)
                    .collect();
                let width = in_column[0].2;
                let heights: Vec<f32> = in_column
                    .iter()
                    .map(|(note, _, width)| self.footnote_height(note, *width))
                    .collect();
                let between = pt(shape.between_notes);
                let mut y = bottom
                    - heights.iter().sum::<f32>()
                    - between * in_column.len().saturating_sub(1) as f32;
                if shape.line_type != 0 {
                    let line_width = mm(shape.line_width);
                    let length = width.min(if shape.line_length < 0.0 {
                        50.0 * 72.0 / 25.4
                    } else {
                        pt(shape.line_length)
                    });
                    let line_y = y - pt(shape.below_line) - 2.0 * line_width;
                    self.line(
                        (column_x, line_y),
                        (column_x + length, line_y),
                        line_width,
                        shape.line_color,
                        Vec::new(),
                    );
                }
                for ((note, _, _), height) in in_column.iter().zip(&heights) {
                    let mut last_vert = -1.0;
                    self.current_note = Some(text::note_number_text(note));
                    self.rec.begin(
                        Kind::Footnote,
                        "각주",
                        &record::snippet(&text::note_number_text(note)),
                        Flags::default(),
                    );
                    for para in &note.paragraphs {
                        self.layout_paragraph(
                            para,
                            (column_x, y),
                            width,
                            section,
                            false,
                            &mut last_vert,
                            None,
                        );
                    }
                    self.rec.end();
                    self.current_note = None;
                    y += height + between;
                }
            }
        }
        self.page = saved;
    }

    /// 문단 하나. 저장된 줄 배치가 없으면 글자 폭으로 줄을 만든다. 본문에서는 본문 높이를 넘는 줄부터
    /// 다음 단·쪽의 위로 옮겨, 저장된 줄처럼 세로 위치가 되돌아가는 곳에서 단·쪽을 넘긴다.
    #[allow(clippy::too_many_arguments)]
    fn layout_paragraph(
        &mut self,
        para: &'a Paragraph,
        origin: (f32, f32),
        avail_width: f32,
        section: &'a Section,
        breakable: bool,
        last_vert: &mut f64,
        lines: Option<&BTreeSet<usize>>,
    ) {
        if !self.rec.on {
            self.paragraph_lines(
                para,
                origin,
                avail_width,
                section,
                breakable,
                last_vert,
                lines,
            );
            return;
        }
        let text = record::paragraph_text(para);
        self.rec
            .begin(Kind::Paragraph, "문단", &text, Flags::default());
        self.rec
            .set_content(record::paragraph_content(para), para.origin);
        self.paragraph_lines(
            para,
            origin,
            avail_width,
            section,
            breakable,
            last_vert,
            lines,
        );
        self.rec.markers(&record::controls(para));
        self.rec.end();
    }

    #[allow(clippy::too_many_arguments)]
    fn paragraph_lines(
        &mut self,
        para: &'a Paragraph,
        origin: (f32, f32),
        avail_width: f32,
        section: &'a Section,
        breakable: bool,
        last_vert: &mut f64,
        lines: Option<&BTreeSet<usize>>,
    ) {
        if para.line_segs.is_empty() {
            let pp = self.para_pr(para.para_pr_id);
            let width = f64::from(avail_width) * 100.0;
            if !breakable {
                let segs = self.flow_segs(para, width, last_vert.max(0.0) + pp.prev);
                self.layout_lines(
                    para,
                    &segs,
                    false,
                    origin,
                    avail_width,
                    section,
                    false,
                    last_vert,
                    lines,
                );
                return;
            }
            let fresh = (para.page_break || para.column_break) && *last_vert >= 0.0;
            let start = if fresh { 0.0 } else { self.para_bottom };
            let mut segs = self.flow_segs(para, width, start + pp.prev);
            let page = &section.page;
            let body_height =
                page.height - page.margin_top - page.header - page.margin_bottom - page.footer;
            // 한/글(e08): 이 쪽에 놓일 각주 자리(구분선 위아래 여백과 각주 높이)는 본문이 쓰지 못한다.
            let stream = self.atoms(para);
            let mut notes: Vec<&'a Note> = self
                .page
                .and_then(|page| self.page_footnotes.get(&page))
                .map(|list| list.iter().map(|(note, _, _)| *note).collect())
                .unwrap_or_default();
            let mut offset = 0.0;
            for index in 0..segs.len() {
                let start = (segs[index].textpos.max(0) as usize).min(stream.len());
                let end = segs.get(index + 1).map_or(stream.len(), |next| {
                    (next.textpos.max(0) as usize).min(stream.len())
                });
                let line_notes: Vec<&'a Note> = stream[start..end.max(start)]
                    .iter()
                    .filter_map(|atom| atom.note.filter(|note| !note.endnote))
                    .collect();
                let mut with_line = notes.clone();
                with_line.extend(line_notes.iter().copied());
                let reserve = self.footnote_reserve(&with_line, section, avail_width);
                let seg = &mut segs[index];
                seg.vertpos -= offset;
                if seg.vertpos > 0.0 && seg.vertpos + seg.vertsize > body_height - reserve {
                    offset += seg.vertpos;
                    seg.vertpos = 0.0;
                    notes.clear();
                }
                notes.extend(line_notes);
            }
            self.layout_lines(
                para,
                &segs,
                true,
                origin,
                avail_width,
                section,
                true,
                last_vert,
                lines,
            );
        } else if let Some(width) = margin_blind_width(para) {
            let pp = self.para_pr(para.para_pr_id);
            let flowed = self.flow_segs(
                para,
                // 한/글은 여백을 뺀 폭보다 0.48pt 넘는 줄을 그대로 둔다. 그 차이만큼 넓혀 잰다.
                width + MARGIN_BLIND_ALLOWANCE + pp.margin_left + pp.margin_right,
                para.line_segs[0].vertpos,
            );
            let segs: Vec<LineSeg> = flowed
                .iter()
                .enumerate()
                .map(|(index, seg)| match para.line_segs.get(index) {
                    Some(stored) => LineSeg {
                        textpos: seg.textpos,
                        ..stored.clone()
                    },
                    None => seg.clone(),
                })
                .collect();
            self.layout_lines(
                para,
                &segs,
                true,
                origin,
                avail_width,
                section,
                breakable,
                last_vert,
                lines,
            );
        } else {
            self.layout_lines(
                para,
                &para.line_segs,
                true,
                origin,
                avail_width,
                section,
                breakable,
                last_vert,
                lines,
            );
        }
    }

    /// 줄마다 그 줄의 글자를 놓는다. 개체는 닻 글자가 든 줄에서 자리를 잡는다.
    #[allow(clippy::too_many_arguments)]
    fn layout_lines(
        &mut self,
        para: &'a Paragraph,
        segs: &[LineSeg],
        stored: bool,
        origin: (f32, f32),
        avail_width: f32,
        section: &'a Section,
        breakable: bool,
        last_vert: &mut f64,
        lines: Option<&BTreeSet<usize>>,
    ) {
        let pp = self.para_pr(para.para_pr_id);
        self.tab_interval = if section.tab_stop.is_finite() && section.tab_stop > 0.0 {
            pt(section.tab_stop)
        } else {
            40.0
        };
        let stream = self.atoms(para);
        if segs.is_empty() {
            return;
        }
        let ranges: Vec<std::ops::Range<usize>> = (0..segs.len())
            .map(|index| {
                let start = (segs[index].textpos.max(0) as usize).min(stream.len());
                let end = segs.get(index + 1).map_or(stream.len(), |next| {
                    (next.textpos.max(0) as usize).min(stream.len())
                });
                start..end.max(start)
            })
            .collect();
        // 한/글은 세로 기준이 문단인 개체를 문단 위 여백보다 위, 앞 문단이 끝난 자리에 둔다.
        // 위아래로 비켜 가는 개체에 밀린 첫 줄은 그 아래에 저장한다.
        let para_start = if breakable {
            self.para_bottom
        } else {
            last_vert.max(0.0)
        };
        let page = &section.page;
        let body_height =
            page.height - page.margin_top - page.header - page.margin_bottom - page.footer;
        let body_bottom = origin.1 + pt(body_height);
        let mut anchored_seen = HashSet::new();
        let mut fragment_top: Option<f32> = None;
        let mut moved_on = false;
        // 쪼개진 표가 다음 쪽으로 넘어갔는가. 줄 정보 없는 문단은 그 표 아래에서 다음 문단을 잇는다.
        let mut split_moved = false;
        // 줄 정보 없는 문단에서 위아래로 비켜 가는 문단 기준 개체의 바깥 아래(HWPUNIT).
        let mut wrap_bottom = 0.0f64;
        let first_char_pr = stream
            .iter()
            .find(|atom| !atom.filler)
            .map(|atom| atom.char_pr)
            .or_else(|| para.runs.first().map(|run| run.char_pr_id))
            .unwrap_or(0);
        let head = if pp.heading_type == "NONE" {
            None
        } else {
            self.paragraph_head(para, pp, first_char_pr)
        };
        let tab_pr = self.tab_prs.get(&pp.tab_pr_id_ref).copied();
        let mut line_end: Option<f64> = None;
        for (li, seg) in segs.iter().enumerate() {
            if lines.is_some_and(|lines| !lines.contains(&li)) {
                continue;
            }
            // 칸 글을 다시 쌓아 커진 표 뒤에서는 한/글처럼 줄을 실제 아래에 쌓는다. 앞 줄이 밀렸으니 저장된 배치가
            // 스스로 단·쪽을 넘긴 자리는 따르지 않고, 본문 아래를 넘는 줄만 다음 단·쪽 위로 옮긴다.
            let stacking = breakable && (self.stack_shift != 0.0 || self.stacked_bottom.is_some());
            let back_up = stored
                && !stacking
                && (seg.vertpos < *last_vert - 0.5
                    || (seg.vertpos + 0.5 < self.object_bottom
                        && seg.vertpos <= self.object_top + 0.5));
            let forced_page = li == 0 && para.page_break && *last_vert >= 0.0;
            let forced_column = li == 0 && para.column_break && *last_vert >= 0.0;
            let mut moved = breakable && (back_up || forced_page || forced_column);
            // 첫 줄만 다음 쪽에 저장됐으면 한/글은 문단 기준 개체를 이 쪽의 문단 시작 자리에 남긴다(들어갈 때).
            if moved && back_up && li == 0 {
                let shift = self.column as f32 * self.column_step;
                for index in ranges[0].clone() {
                    let atom = &stream[index];
                    let Some(item) = atom.object else {
                        continue;
                    };
                    let pos = object_pos(item);
                    if atom.filler
                        || pos.treat_as_char
                        || pos.vert_rel_to != "PARA"
                        || !takes_space(pos)
                        || anchored_seen.contains(&index)
                    {
                        continue;
                    }
                    let rect = self.floating_rect(
                        item,
                        section,
                        pp,
                        origin.0 + shift,
                        avail_width,
                        origin.1 + pt(para_start),
                    );
                    if outer(rect, &object_margins(item)).bottom() > body_bottom {
                        continue;
                    }
                    // 앞 개체에 밀려 이 쪽에 들어가지 않으면 문단을 따라 다음 쪽으로 간다.
                    let Some(rect) = self.below_floats(item, rect, body_bottom) else {
                        continue;
                    };
                    anchored_seen.insert(index);
                    self.place_float(item, rect, section, breakable);
                }
            }
            if moved {
                self.next_column(section, !forced_page && seg.flags & 1 == 0);
                fragment_top = None;
            }
            let mut stacked = false;
            if breakable && !moved {
                if li == 0
                    && let Some(bottom) = self.stacked_bottom
                {
                    self.stack_shift = bottom + pp.prev - seg.vertpos;
                    stacked = true;
                } else if stacking
                    && seg.vertpos < *last_vert - 0.5
                    && let Some(end) = line_end
                {
                    self.stack_shift = end - seg.vertpos;
                }
            }
            let line_atoms = &stream[ranges[li].clone()];
            let growth = if breakable {
                line_atoms
                    .iter()
                    .filter_map(|atom| match atom.object {
                        Some(RunItem::Table(table)) if !atom.filler && table.pos.treat_as_char => {
                            Some(table)
                        }
                        _ => None,
                    })
                    .map(|table| self.stacked_growth(table, seg.vertsize))
                    .fold(0.0, f64::max)
            } else {
                0.0
            };
            if breakable
                && (self.stack_shift != 0.0 || growth > 0.0)
                && seg.vertpos + self.stack_shift > 0.0
                && seg.vertpos + self.stack_shift + seg.vertsize + growth > body_height
            {
                self.next_column(section, true);
                self.stack_shift = -seg.vertpos;
                fragment_top = None;
                moved = true;
            }
            let lowered = if breakable { self.stack_shift } else { 0.0 };
            *last_vert = seg.vertpos;
            self.record_page_events(line_atoms, section, breakable);
            let line_top = origin.1 + pt(seg.vertpos + lowered);
            if self.text_clip_bottom.is_some_and(|limit| line_top >= limit) {
                continue;
            }
            let fragment = *fragment_top.get_or_insert(if li == 0 && !moved && !stacked {
                origin.1 + pt(para_start.min(seg.vertpos) + lowered)
            } else if li == 0 && moved && breakable {
                // 다음 쪽으로 넘어간 문단도 문단 위 여백 위에서 잰다.
                origin.1 + pt((seg.vertpos - pp.prev).max(0.0) + lowered)
            } else {
                line_top
            });
            // 한/글은 세로 정렬이 기준선이 아닌 줄의 저장된 기준선을 쓰지 않고 글자마다 다시 맞춘다.
            let mut line = self.shape(line_atoms, tab_pr, text::Advances::Hancom);
            let baseline = line_top
                + line
                    .align_vertically(&pp.vert_align, pt(seg.vertsize))
                    .unwrap_or(pt(seg.baseline));
            let is_first = li == 0;
            let is_last = li + 1 == segs.len();
            let shift = if breakable {
                self.column as f32 * self.column_step
            } else {
                0.0
            };
            let indent = pt(text::indent(pp, is_first));
            let mut x0 = origin.0 + shift + pt(seg.horzpos) + indent;
            let mut avail = pt(seg.horzsize);
            if avail <= 1.0 {
                avail = avail_width;
            }
            avail = (avail - indent).max(0.0);
            let line_head = head.as_ref().filter(|head| is_first || head.auto_indent);
            if let Some(head) = line_head {
                x0 += head.width;
                avail = (avail - head.width).max(0.0);
            }
            let ends_with_break = line_atoms.last().is_some_and(|atom| atom.ch == '\n');
            let x = text::align(&mut line, pp, x0, avail, is_last || ends_with_break);
            self.rec_extend(scene::Rect::new(
                origin.0 + shift + pt(seg.horzpos),
                line_top,
                if seg.horzsize > 1.0 {
                    pt(seg.horzsize)
                } else {
                    avail_width
                },
                pt(seg.vertsize),
            ));
            // 한/글은 문단 머리와 본문을 한 덩어리로 정렬하므로, 머리는 정렬된 본문 바로 앞에 붙는다.
            if let Some(head) = line_head
                && is_first
            {
                self.emit(&head.line, x - head.width, baseline);
            }
            self.emit(&line, x, baseline);
            if line.tracked() {
                // 한/글은 변경 추적 글자가 있는 줄 왼쪽, 쪽 왼쪽 여백 7.5pt 앞에 줄 높이만큼 세로 막대를 긋는다.
                let page = &section.page;
                let left = pt(page.margin_left + page.gutter) - 7.5;
                let height = pt(seg.vertsize + seg.spacing);
                self.fill(
                    scene::Rect::new(left - 0.375, line_top, 0.75, height),
                    text::TRACK_COLOR,
                    1.0,
                );
            }
            // 개체가 문서에 나타난 순서대로 자리를 차지해야 뒤의 inline 개체가 앞의 떠 있는 개체를 밀지 않는다.
            let mut bumped = Vec::new();
            let mut split_floats = Vec::new();
            let mut spans_separator = false;
            let has_text = (is_first && head.is_some())
                || line_atoms.iter().any(|atom| {
                    !atom.filler
                        && atom.object.is_none()
                        && !atom.ch.is_whitespace()
                        && !text::invisible(atom.ch)
                });
            for index in ranges[li].clone() {
                if !anchored_seen.insert(index) {
                    continue;
                }
                let atom = &stream[index];
                let Some(item) = atom.object else {
                    continue;
                };
                if atom.filler {
                    continue;
                }
                if !object_pos(item).treat_as_char {
                    // 떠 있는 개체끼리는 문서 순서대로 자리를 차지한다. 미뤄 둔 표는 글자처럼 놓인 개체만 앞지른다.
                    let floated = self.place_split_floats(
                        &mut split_floats,
                        origin,
                        section,
                        para.line_segs.is_empty(),
                    );
                    moved_on |= floated;
                    split_moved |= floated;
                    let mut rect = self.floating_rect(
                        item,
                        section,
                        pp,
                        origin.0 + shift,
                        avail_width,
                        if breakable { fragment } else { line_top },
                    );
                    let pos = object_pos(item);
                    if !breakable && pos.vert_rel_to == "PARA" && pos.vert_offset < 0.0 {
                        // 칸 안에서 한/글은 문단 위로 올린 개체를 닻 줄 위에 붙인다.
                        rect.y -= pt(pos.vert_offset);
                    }
                    if breakable
                        && pos.vert_rel_to == "PARA"
                        && !pos.allow_overlap
                        && takes_space(pos)
                        && outer(rect, &object_margins(item)).height <= pt(body_height)
                    {
                        match self.below_floats(item, rect, body_bottom) {
                            Some(below) => rect = below,
                            None => {
                                bumped.push(item);
                                continue;
                            }
                        }
                    }
                    if breakable && takes_space(pos) {
                        let gap = self.column_step - self.column_width;
                        spans_separator |= (1..self.column_count).any(|column| {
                            let separator = origin.0 + column as f32 * self.column_step - gap / 2.0;
                            rect.x < separator && rect.right() > separator
                        });
                    }
                    if breakable
                        && para.line_segs.is_empty()
                        && pos.vert_rel_to == "PARA"
                        && pos.text_wrap == "TOP_AND_BOTTOM"
                    {
                        let below = outer(rect, &object_margins(item)).bottom() - origin.1;
                        wrap_bottom = wrap_bottom.max(f64::from(below) * 100.0);
                    }
                    if breakable
                        && pos.vert_rel_to == "PARA"
                        && let RunItem::Table(table) = item
                    {
                        // 나뉘는 표는 다음 쪽으로 넘어가므로, 같은 줄의 다른 개체를 이 쪽에 다 놓은 뒤에 그린다.
                        split_floats.push((item, table, rect));
                    } else {
                        self.place_float(item, rect, section, breakable);
                    }
                    continue;
                }
                let Some(placed) = line
                    .objects
                    .iter()
                    .find(|placed| std::ptr::eq(placed.item, item))
                else {
                    continue;
                };
                let (w, h) = self.object_size(item);
                let margins = object_margins(item);
                let [caption_left, caption_top, _, caption_bottom] = self.caption_space(item, w);
                let boxed = h + caption_top + caption_bottom;
                let line_bottom = line_top + pt(seg.vertsize);
                // 글자가 있는 줄에서 개체는 기준선이 아니라 줄 아래(바깥 여백 안)에 맞춘다.
                // 개체만 있는 줄에서는 제 높이의 85% 자리를 기준선에 맞춘다. 한 줄에 높이가 다른 그림 둘
                // 이면 작은 그림의 아래가 큰 그림보다 위에 있다. 어느 쪽이든 줄 아래를 넘지 않고,
                // 줄보다 높으면 줄 위에서 시작한다.
                let bottom = if has_text {
                    line_bottom
                } else {
                    let outer_height = boxed + pt(margins.top + margins.bottom);
                    (baseline + outer_height * 0.15).min(line_bottom)
                };
                let top = self.inline_equation_top(item, baseline, h).unwrap_or(
                    (bottom - pt(margins.bottom) - boxed).max(line_top + pt(margins.top))
                        + caption_top,
                );
                let rect =
                    scene::Rect::new(x + placed.x + pt(margins.left) + caption_left, top, w, h);
                if breakable
                    && let RunItem::Table(table) = item
                    && let Some(bottom) = self.place_split_table(
                        table,
                        rect,
                        origin,
                        section,
                        para.line_segs.is_empty(),
                    )
                {
                    self.object_bottom = bottom;
                    self.object_top = 0.0;
                    moved_on = true;
                    split_moved = true;
                } else {
                    self.place(item, rect, section);
                    self.draw_caption(item, rect, section);
                    let bottom = seg.vertpos + f64::from(boxed) * 100.0;
                    if breakable {
                        // 한/글은 떠 있는 개체를 글자처럼 놓인 개체가 든 줄의 줄 간격 아래로 비킨다.
                        let mut taken = outer(rect, &margins);
                        let line_bottom = line_top + pt(seg.vertsize + seg.spacing);
                        taken.height = taken.height.max(line_bottom - taken.y);
                        self.floats.push(taken);
                    }
                    if breakable && bottom > self.object_bottom {
                        self.object_bottom = bottom;
                        self.object_top = seg.vertpos;
                    }
                }
            }
            let floated = self.place_split_floats(
                &mut split_floats,
                origin,
                section,
                para.line_segs.is_empty(),
            );
            moved_on |= floated;
            split_moved |= floated;
            if !bumped.is_empty() {
                self.next_column(section, true);
                for item in bumped {
                    let left = origin.0 + self.column as f32 * self.column_step;
                    let mut rect =
                        self.floating_rect(item, section, pp, left, avail_width, origin.1);
                    rect.y = origin.1 + pt(object_margins(item).top);
                    let rect = self.below_floats(item, rect, body_bottom).unwrap_or(rect);
                    if let RunItem::Table(table) = item
                        && let Some(bottom) = self.place_split_table(
                            table,
                            rect,
                            origin,
                            section,
                            para.line_segs.is_empty(),
                        )
                    {
                        self.object_bottom = bottom;
                    } else {
                        self.place_float(item, rect, section, breakable);
                    }
                }
                moved_on = true;
            }
            self.stack_shift += growth;
            line_end = Some(seg.vertpos + lowered + seg.vertsize + growth + seg.spacing);
            // 쪼개진 표는 조각마다 범위를 넓혔다. 본문 아래를 넘는 줄은 한/글 PDF에서 단 구분선이 그 줄 위에서 끝난다.
            if breakable && !moved_on && (!spans_separator || has_text) {
                let bottom = if seg.vertpos + lowered + seg.vertsize + growth > body_height {
                    line_top
                } else {
                    line_top + pt(seg.vertsize + growth)
                };
                self.extend_columns(line_top, bottom);
            }
            if moved_on {
                *last_vert = -1.0;
                fragment_top = None;
                moved_on = false;
            }
        }
        if breakable {
            if let Some(last) = segs.last() {
                self.para_bottom = last.vertpos + last.vertsize + last.spacing + pp.next;
                if split_moved && para.line_segs.is_empty() {
                    // 한/글(e07): 쪽을 넘어 나뉜 표 뒤 문단은 표 마지막 조각 아래에서 시작한다.
                    self.para_bottom = self.para_bottom.max(self.object_bottom);
                } else if para.line_segs.is_empty() {
                    // 한/글(e05): 위아래로 비켜 가는 개체 뒤 문단은 개체 아래에서 시작한다.
                    self.para_bottom = self.para_bottom.max(wrap_bottom);
                }
                self.stacked_bottom =
                    (self.stack_shift != 0.0).then_some(self.para_bottom + self.stack_shift);
            }
        } else if let Some(index) = (0..segs.len())
            .rev()
            .find(|index| lines.is_none_or(|lines| lines.contains(index)))
        {
            let last = &segs[index];
            *last_vert = last.vertpos + last.vertsize + last.spacing + pp.next;
        }
    }

    /// 꼬리말 내용의 높이(pt): 줄 아래와 문단 기준으로 떠 있는 개체의 바깥 여백 아래 중 가장 낮은 곳.
    fn footer_height(
        &mut self,
        paragraphs: &'a [Paragraph],
        section: &'a Section,
        left: f32,
        width: f32,
    ) -> f32 {
        let mut last_vert = -1.0f64;
        let mut height = 0.0f32;
        for para in paragraphs {
            let pp = self.para_pr(para.para_pr_id);
            let start = last_vert.max(0.0) + pp.prev;
            let segs = if para.line_segs.is_empty() {
                self.flow_segs(para, f64::from(width) * 100.0, start)
            } else {
                para.line_segs.clone()
            };
            let (Some(first), Some(last)) = (segs.first(), segs.last()) else {
                continue;
            };
            for seg in &segs {
                height = height.max(pt(seg.vertpos + seg.vertsize));
            }
            let para_top = pt(start.min(first.vertpos));
            for item in para.runs.iter().flat_map(|run| &run.items) {
                let pos = object_pos(item);
                if pos.treat_as_char || matches!(pos.vert_rel_to.as_str(), "PAGE" | "PAPER") {
                    continue;
                }
                let rect = self.floating_rect(item, section, pp, left, width, para_top);
                height = height.max(rect.y + rect.height + pt(object_margins(item).bottom));
            }
            last_vert = last.vertpos + last.vertsize + last.spacing + pp.next;
        }
        height
    }

    /// 미뤄 둔 문단 기준 떠 있는 표를 놓는다. 다른 쪽이나 단으로 넘어갔으면 true.
    fn place_split_floats(
        &mut self,
        pending: &mut Vec<(&'a RunItem, &'a hwp_core::Table, scene::Rect)>,
        origin: (f32, f32),
        section: &'a Section,
        reflow: bool,
    ) -> bool {
        let mut moved = false;
        for (item, table, rect) in pending.drain(..) {
            let before = (self.page, self.column);
            if let Some(bottom) = self.place_split_table(table, rect, origin, section, reflow) {
                self.object_bottom = bottom;
                self.object_top = 0.0;
                // 늘어나기만 하고 한 조각에 들어간 표는 이 쪽에 남는다. 다음 줄의 저장된 쪽 넘김을 그대로 따른다.
                moved |= (self.page, self.column) != before;
            } else {
                self.place_float(item, rect, section, true);
            }
        }
        moved
    }

    /// 떠 있는 개체를 놓는다. 본문에서 글과 자리를 나누는 개체는 지금 쪽의 다른 개체가 비켜 갈 자리로 적어 둔다.
    fn place_float(
        &mut self,
        item: &'a RunItem,
        rect: scene::Rect,
        section: &'a Section,
        breakable: bool,
    ) {
        let pos = object_pos(item);
        let behind = self.behind;
        self.behind = pos.text_wrap == "BEHIND_TEXT";
        self.place(item, rect, section);
        self.draw_caption(item, rect, section);
        self.behind = behind;
        if breakable && takes_space(pos) {
            self.floats.push(outer(rect, &object_margins(item)));
        }
    }

    /// 이 쪽에 놓인 개체와 바깥 여백이 겹치지 않을 때까지 아래로 내린 자리. 본문 아래(pt)를 넘으면 None.
    fn below_floats(
        &self,
        item: &RunItem,
        mut rect: scene::Rect,
        body_bottom: f32,
    ) -> Option<scene::Rect> {
        let margins = object_margins(item);
        let height = match item {
            RunItem::Table(table) if table.height > 0.0 => pt(table.height),
            _ => rect.height,
        };
        loop {
            let bounds = outer(rect, &margins);
            let Some(bottom) = self
                .floats
                .iter()
                .filter(|float| overlaps(**float, bounds))
                .map(scene::Rect::bottom)
                .reduce(f32::max)
            else {
                return Some(rect);
            };
            rect.y = bottom + pt(margins.top);
            if rect.y + height + pt(margins.bottom) > body_bottom {
                return None;
            }
        }
    }

    /// 떠 있는 개체의 자리(pt). 세로 기준 PARA는 이 문단이 지금 쪽·단에서 시작한 줄의 위다.
    fn floating_rect(
        &mut self,
        item: &'a RunItem,
        section: &Section,
        pp: &ParaPr,
        column_left: f32,
        column_width: f32,
        fragment_top: f32,
    ) -> scene::Rect {
        let (w, h) = self.object_size(item);
        let pos = object_pos(item);
        let margins = object_margins(item);
        let page = &section.page;
        let body_left = pt(page.margin_left + page.gutter);
        let body_width = pt(page.width - page.margin_left - page.margin_right - page.gutter);
        let oy = match pos.vert_rel_to.as_str() {
            "PAGE" => pt(page.margin_top + page.header) + pt(pos.vert_offset),
            "PAPER" => pt(pos.vert_offset),
            _ => fragment_top + pt(pos.vert_offset),
        };
        let (mut ox, base_width) = match pos.horz_rel_to.as_str() {
            "PAGE" => (body_left, body_width),
            "PAPER" => (0.0, pt(page.width)),
            "PARA" => (
                column_left + pt(pp.margin_left),
                (column_width - pt(pp.margin_left + pp.margin_right)).max(0.0),
            ),
            _ => (column_left, column_width),
        };
        // 가운데 맞춤은 바깥 여백까지 친 상자를 가운데에 둔다.
        let outer_width = w + pt(margins.left + margins.right);
        match pos.horz_align.as_str() {
            "CENTER" => ox += (base_width - outer_width) / 2.0 + pt(pos.horz_offset),
            "RIGHT" => ox += base_width - w - pt(pos.horz_offset),
            _ => ox += pt(pos.horz_offset),
        }
        scene::Rect::new(ox + pt(margins.left), oy + pt(margins.top), w, h)
    }

    /// 쪽 컨트롤(머리말·꼬리말·감추기·새 번호·쪽 번호 자리)과 각주를 이 줄이 떨어진 쪽에 적어 둔다.
    fn record_page_events(&mut self, atoms: &[Atom<'a>], section: &Section, breakable: bool) {
        let Some(page) = self.page else {
            return;
        };
        for atom in atoms {
            if self.in_body
                && let Some(note) = atom.note
                && !note.endnote
            {
                self.record_footnote(note, section);
            }
            let Some(item) = atom.object else {
                continue;
            };
            match item {
                RunItem::Header {
                    paragraphs,
                    applies,
                } if self.in_body && breakable => {
                    self.page_headers
                        .entry(page)
                        .or_default()
                        .push((applies.as_str(), paragraphs.as_slice()));
                }
                RunItem::Footer {
                    paragraphs,
                    applies,
                } if self.in_body && breakable => {
                    self.page_footers
                        .entry(page)
                        .or_default()
                        .push((applies.as_str(), paragraphs.as_slice()));
                }
                RunItem::PageHiding {
                    header,
                    footer,
                    page_num,
                } => {
                    let state = self.page_hide.entry(page).or_default();
                    state.header |= *header;
                    state.footer |= *footer;
                    state.page_num |= *page_num;
                }
                RunItem::NewNum(number) => {
                    self.page_new_num.insert(page, *number);
                }
                RunItem::PageNum { pos, format, side } => {
                    self.page_num_spec
                        .insert(page, (pos.as_str(), format.as_str(), side.as_str()));
                }
                _ => {}
            }
        }
    }
}

/// 접은 줄 그러데이션의 (자리, 색). `center`(0–1) 자리에 첫 색, 양 끝에 끝 색을 둔다.
fn linear_stops(colors: &[u32], center: f32) -> Vec<(f32, u32)> {
    let center = center.clamp(0.0, 1.0);
    let spans = colors.len().saturating_sub(1).max(1) as f32;
    let above = colors
        .iter()
        .enumerate()
        .rev()
        .map(|(index, &color)| (center * (1.0 - index as f32 / spans), color));
    let below = colors
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, &color)| (center + (1.0 - center) * index as f32 / spans, color));
    above.chain(below).collect()
}

/// 그러데이션 색 띠 `colors`를 고르게 나눈 위의 `t`(0은 첫 색, 1은 끝 색) 자리 색. 한/글처럼 채널마다 버림한다.
fn gradient_color(colors: &[u32], t: f32) -> u32 {
    let spans = colors.len().saturating_sub(1);
    if spans == 0 {
        return colors.first().copied().unwrap_or(0);
    }
    let at = t.clamp(0.0, 1.0) * spans as f32;
    let index = (at.floor() as usize).min(spans - 1);
    let local = at - index as f32;
    let (from, to) = (colors[index], colors[index + 1]);
    [16, 8, 0].iter().fold(0, |out, shift| {
        let a = ((from >> shift) & 0xFF) as f32;
        let b = ((to >> shift) & 0xFF) as f32;
        out | (((a + (b - a) * local + 1e-3).floor() as u32) << shift)
    })
}

fn page_scale(track_changes: bool, width: f32, section: &Section) -> f32 {
    if track_changes {
        track_change_scale(width, pt(section.page.margin_right))
    } else {
        1.0
    }
}

/// 변경 추적 표시가 있는 문서를 한/글 12.30.0은 쪽 오른쪽에 풍선 칸을 남기고 왼쪽 위를 축으로 줄여 찍는다.
/// 줄인 쪽의 오른쪽 여백 끝에서 풍선 칸 너비(약 232pt)가 원래 쪽 너비에 맞는다. 왼쪽 여백·제본 여백과는 무관하다.
fn track_change_scale(width: f32, margin_right: f32) -> f32 {
    const BALLOON: f32 = 232.1;
    let room = width - margin_right + BALLOON;
    if width > 0.0 && room > width {
        width / room
    } else {
        1.0
    }
}

fn section_has(section: &Section, test: impl Fn(&RunItem) -> bool) -> bool {
    section
        .paragraphs
        .iter()
        .flat_map(|para| &para.runs)
        .flat_map(|run| &run.items)
        .any(test)
}

/// 표 칸·글상자·각주 속까지 문서 차례로 훑어 처음 나오는, 내용이 있는 머리말(`header`) 또는 꼬리말 컨트롤이 놓일 쪽.
/// 구역의 머리말·꼬리말은 이 컨트롤의 내용이다.
fn first_applies(paragraphs: &[Paragraph], header: bool) -> Option<&str> {
    paragraphs
        .iter()
        .flat_map(|para| &para.runs)
        .flat_map(|run| &run.items)
        .find_map(|item| item_applies(item, header))
}

fn item_applies(item: &RunItem, header: bool) -> Option<&str> {
    match item {
        RunItem::Header {
            paragraphs,
            applies,
        } if header && !paragraphs.is_empty() => Some(applies),
        RunItem::Footer {
            paragraphs,
            applies,
        } if !header && !paragraphs.is_empty() => Some(applies),
        RunItem::Table(table) => table
            .cells
            .iter()
            .find_map(|cell| first_applies(&cell.paragraphs, header)),
        RunItem::Shape(shape) => first_applies(&shape.paragraphs, header).or_else(|| {
            shape
                .children
                .iter()
                .find_map(|child| item_applies(&child.item, header))
        }),
        RunItem::Note(note) => first_applies(&note.paragraphs, header),
        _ => None,
    }
}

pub(crate) fn object_pos(item: &RunItem) -> &hwp_core::Pos {
    static DEFAULT: OnceLock<hwp_core::Pos> = OnceLock::new();
    match item {
        RunItem::Table(table) => &table.pos,
        RunItem::Picture(picture) => &picture.pos,
        RunItem::Shape(shape) => &shape.pos,
        _ => DEFAULT.get_or_init(hwp_core::Pos::default),
    }
}

/// 글이 개체 옆이나 위아래로 비켜 가는가(어울림·자리 차지).
fn takes_space(pos: &hwp_core::Pos) -> bool {
    matches!(pos.text_wrap.as_str(), "SQUARE" | "TOP_AND_BOTTOM")
}

/// 개체 자리를 바깥 여백까지 넓힌다.
fn outer(rect: scene::Rect, margins: &hwp_core::Margins) -> scene::Rect {
    scene::Rect::new(
        rect.x - pt(margins.left),
        rect.y - pt(margins.top),
        rect.width + pt(margins.left + margins.right),
        rect.height + pt(margins.top + margins.bottom),
    )
}

/// 두 자리가 0.01pt보다 넓게 겹치는가.
fn overlaps(a: scene::Rect, b: scene::Rect) -> bool {
    a.x < b.right() - 0.01
        && b.x < a.right() - 0.01
        && a.y < b.bottom() - 0.01
        && b.y < a.bottom() - 0.01
}

pub(crate) fn object_margins(item: &RunItem) -> hwp_core::Margins {
    match item {
        RunItem::Table(table) => table.out_margins.clone(),
        RunItem::Picture(picture) => picture.out_margins.clone(),
        RunItem::Shape(shape) => shape.out_margins.clone(),
        _ => hwp_core::Margins::default(),
    }
}

/// 여백을 뺀 폭으로 다시 나눌 때 더 받아 주는 폭(HWPUNIT).
const MARGIN_BLIND_ALLOWANCE: f64 = 50.0;

/// 저장된 줄 폭이 어울림 개체의 바깥 여백을 빼지 않았으면 여백을 뺀 폭(HWPUNIT). 한/글은 이런 문단의 줄을 다시 나눈다.
fn margin_blind_width(para: &Paragraph) -> Option<f64> {
    let first = para.line_segs.first()?;
    para.runs
        .iter()
        .flat_map(|run| &run.items)
        .find_map(|item| {
            let pos = object_pos(item);
            let margins = object_margins(item);
            let blind = !pos.treat_as_char
                && pos.text_wrap == "SQUARE"
                && pos.horz_align == "LEFT"
                && pos.horz_offset > 0.0
                && margins.left > 0.0
                && first.horzpos == 0.0
                && first.horzsize == pos.horz_offset;
            blind.then_some(pos.horz_offset - margins.left)
        })
}

#[cfg(test)]
mod tests {
    use super::{gradient_color, linear_stops, track_change_scale};

    #[test]
    fn track_change_scale_matches_hwp_exports() {
        // 한/글 12.30.0이 변경 추적 문서를 줄인 배율(오린 영역 너비 / 쪽 너비).
        for (width, margin_right, expected) in [
            (595.28, 85.04, 0.80171),
            (595.28, 28.34, 0.74506),
            (595.28, 56.69, 0.7726),
            (841.89, 85.04, 0.85152),
        ] {
            let scale = track_change_scale(width, margin_right);
            assert!(
                (scale - expected).abs() < 0.0015,
                "{width} {margin_right}: {scale}"
            );
        }
    }

    #[test]
    fn linear_gradient_folds_around_its_center() {
        // 칸 배경: 한/글은 가운데 흰색에서 위·아래 끝 #C1D5ED로 가는 두 축 음영을 그린다.
        assert_eq!(
            linear_stops(&[0xFFFFFF, 0xC1D5ED], 0.5),
            vec![(0.0, 0xC1D5ED), (0.5, 0xFFFFFF), (1.0, 0xC1D5ED)]
        );
    }

    #[test]
    fn gradient_steps_match_hwp_radial_rings() {
        // 칸 배경(#0074A5 → #72A8BD, 50단계)을 한/글이 칠한 바깥 둘째 원과 안쪽 둘째 원의 색.
        let colors = [0x0074A5, 0x72A8BD];
        assert_eq!(gradient_color(&colors, 0.0), 0x0074A5);
        assert_eq!(gradient_color(&colors, 1.0 / 49.0), 0x0275A5);
        assert_eq!(gradient_color(&colors, 48.0 / 49.0), 0x6FA6BC);
        assert_eq!(gradient_color(&colors, 1.0), 0x72A8BD);
    }
}
