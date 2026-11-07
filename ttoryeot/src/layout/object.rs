//! 개체(그림·도형)와 선·테두리 그리기.

use super::record::{self, Flags};
use super::{Renderer, mm, object_pos, pt};
use crate::components::Kind;
use crate::scene::{self, ImageData, Item, PathOp, Rect};
use hwp_core::{Border, BorderFill, Caption, ImageFill, Picture, RunItem, Section, Shape};
use std::sync::Arc;

/// 테두리 선 하나를 이루는 가는 선(겹선이면 둘 이상). offset은 굵기에 대한 비율, width·dash는 pt.
struct Rail {
    offset: f32,
    width: f32,
    dash: Vec<f32>,
}

/// 한/글 PDF는 선을 1/600인치 격자에 긋는다.
pub(crate) const DEVICE_UNIT: f32 = 72.0 / 600.0;

/// 테두리 파선(DASH)의 무늬. 한/글 PDF에서 잰 값: 굵기를 격자 w칸으로 반올림하면 켜짐 ⌊7w/5⌋칸, 꺼짐 ⌊11w/5⌋칸
/// (w 3 → 4·6, w 4 → 5·8, w 5 → 7·11).
fn hancom_dash(width: f32) -> Vec<f32> {
    let units = (width / DEVICE_UNIT).round().max(1.0) as u32;
    [units * 7 / 5, units * 11 / 5]
        .map(|count| count as f32 * DEVICE_UNIT)
        .to_vec()
}

fn rails(border: &Border) -> Vec<Rail> {
    if border.border_type == "NONE" || border.width <= 0.0 {
        return Vec::new();
    }
    let width = mm(border.width);
    let rail = |offset: f32, share: f32| Rail {
        offset,
        width: width * share,
        dash: Vec::new(),
    };
    match border.border_type.as_str() {
        "DOUBLE" | "DOUBLE_SLIM" => vec![rail(0.375, 0.25), rail(-0.375, 0.25)],
        // 한/글 PDF(k382 0.7mm: 4·9칸, 중심 10칸; k817 0.4mm: 2·5칸, 중심 5칸)에서 잰 자리와 굵기.
        "SLIM_THICK" => vec![rail(0.30, 0.25), rail(-0.31, 0.55)],
        "THICK_SLIM" => vec![rail(0.31, 0.55), rail(-0.30, 0.25)],
        "SLIM_THICK_SLIM" => vec![rail(0.425, 0.15), rail(0.0, 0.30), rail(-0.425, 0.15)],
        kind => {
            let w = width;
            let dash = match kind {
                "DASH" => hancom_dash(w),
                "DOT" | "CIRCLE" => vec![w, w * 2.0],
                "DASH_DOT" => vec![w * 4.0, w * 2.0, w, w * 2.0],
                "DASH_DOT_DOT" => vec![w * 4.0, w * 2.0, w, w * 2.0, w, w * 2.0],
                "LONG_DASH" => vec![w * 8.0, w * 2.0],
                _ => Vec::new(),
            };
            vec![Rail {
                offset: 0.0,
                width,
                dash,
            }]
        }
    }
}

fn stroke(rail: &Rail, color: u32, from: (f32, f32), to: (f32, f32), out: &mut Vec<Item>) {
    if rail.width <= 0.0 {
        return;
    }
    out.push(Item::Stroke {
        path: vec![PathOp::Move(from.0, from.1), PathOp::Line(to.0, to.1)],
        stroke: scene::Stroke {
            width: rail.width,
            color,
            dash: rail.dash.clone(),
        },
    });
}

/// 테두리 선 하나(가로 또는 세로). 겹선은 굵기에 비례해 선과 수직으로 벌린다.
pub(crate) fn border_items(border: &Border, from: (f32, f32), to: (f32, f32), out: &mut Vec<Item>) {
    let horizontal = from.1 == to.1;
    let scale = mm(border.width);
    for rail in rails(border) {
        let (dx, dy) = if horizontal {
            (0.0, scale * rail.offset)
        } else {
            (scale * rail.offset, 0.0)
        };
        stroke(
            &rail,
            border.color,
            (from.0 - dx, from.1 - dy),
            (to.0 - dx, to.1 - dy),
            out,
        );
    }
}

/// 표 선을 맞출 장치 격자(1/600인치).
#[derive(Clone, Copy)]
pub(crate) enum LineGrid {
    /// 쪽 본문 원점(왼쪽 여백+제본 여백, 위 여백+머리말). 한/글은 원점과 원점에서 변까지의 거리를 따로 반올림한다.
    Body(f32, f32),
    /// 변경 추적으로 줄여 찍는 쪽. 우리 배율이 한/글과 약 0.1% 달라 줄인 뒤의 격자를 맞힐 수 없으므로,
    /// 줄이기 전 좌표에서 변 자리만 반올림(반은 내림)한다.
    Scaled,
}

impl LineGrid {
    fn origin(self, vertical: bool) -> Option<f32> {
        match self {
            Self::Body(x, y) => Some(if vertical { x } else { y }),
            Self::Scaled => None,
        }
    }
}

/// 한 변의 선들(가운데 자리, 선), 칸 안쪽 선부터. `edge`는 변 자리, `origin`은 같은 축의 쪽 본문 원점(줄인 쪽은
/// 없음), `outward`는 칸 바깥 방향(±1).
fn rail_lines<'r>(
    border: &Border,
    rails: &'r [Rail],
    edge: f32,
    origin: Option<f32>,
    outward: f32,
) -> Vec<(f32, &'r Rail)> {
    let units = |at: f32| at / DEVICE_UNIT;
    let mut lines: Vec<(f32, &Rail)> = rails
        .iter()
        .map(|rail| {
            let offset = mm(border.width) * rail.offset / DEVICE_UNIT;
            let at = match origin {
                Some(origin) => {
                    // 칸의 정확히 절반은 f32 오차와 상관없이 올린다.
                    let round = |at: f32| (units(at) + 0.501).floor();
                    let snapped = round(origin) + round(edge - origin);
                    // 겹선 두 선은 격자에 맞춘 선 굵기 t칸의 1.5배씩 벌어진다(0.4mm ±3, 0.5mm -5·+4, 0.7mm ±6).
                    let shift = if matches!(border.border_type.as_str(), "DOUBLE" | "DOUBLE_SLIM") {
                        rail.offset.signum() * 1.5 * units(rail.width).round()
                    } else {
                        offset
                    };
                    (snapped - shift).floor()
                }
                None => ((units(edge) - 0.501).ceil() - offset).floor(),
            };
            (at * DEVICE_UNIT, rail)
        })
        .collect();
    lines.sort_by(|a, b| ((a.0 - edge) * outward).total_cmp(&((b.0 - edge) * outward)));
    lines
}

/// 칸 테두리의 위·아래 변을 그리는 방식.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    Hidden,
    Drawn,
    /// 그리되, 변 너머 행은 다른 조각에서 넘어온 행이다(되풀이한 머리행의 아랫변). 한/글은 이 변의 선을
    /// 표 바깥 세로 변의 바깥 가장자리까지 늘이고, 세로 겹선과 만나는 모서리는 너머로 이어지지 않는 것처럼 닫는다.
    Open,
}

/// 변 하나가 모서리에서 보는 모습. `outward`는 이 변을 따라 모서리 바깥 방향(±1), `across`는 맞닿은 변과
/// 그 선들, `along`은 이 변이 모서리 너머 이웃 칸으로 이어지는지, `beyond`는 맞닿은 변이 모서리 너머로
/// 이어지는지.
struct Corner<'c> {
    edge: f32,
    outward: f32,
    across: (&'c Border, &'c [(f32, &'c Rail)]),
    along: bool,
    beyond: bool,
    /// 모서리 위쪽에서 그리는 이웃 칸의 세로 변. 있으면 가로 변을 그 굵기로 늘인다.
    above: Option<&'c Border>,
    /// 이 변이 [`Edge::Open`]이다.
    open: bool,
}

impl Corner<'_> {
    /// 맞닿은 변이 겹선일 때 이 변의 `inner`번째 선(칸 안쪽부터, 모두 `count`개)이 끝나는 자리. 한/글 PDF에서
    /// 잰 규칙: 홑선은 겹선의 칸 쪽 선 바깥 가장자리에서 멈춘다. 겹선끼리는 안쪽 선이 맞닿은 변의 안쪽 선을
    /// 덮고 만난다. 바깥 선은 맞닿은 변이 모서리 너머로 이어지면(╬·╠) 그 안쪽 선에서, 이 변만 이어지면(╦)
    /// 모서리까지, 아무것도 이어지지 않으면(╗) 맞닿은 변의 바깥 선을 덮고 끝난다. 겹선이 아니면 `None`.
    fn double_end(&self, inner: usize, count: usize) -> Option<f32> {
        let lines = self.across.1;
        if lines.len() < 2 {
            return None;
        }
        let side =
            |(at, rail): (f32, &Rail), toward: f32| at + self.outward * toward * rail.width / 2.0;
        let (near, far) = (lines[0], lines[lines.len() - 1]);
        Some(if count < 2 && self.along {
            self.edge
        } else if count < 2 {
            side(near, -1.0)
        } else if inner == 0 || self.beyond {
            side(near, 1.0)
        } else if self.along {
            self.edge
        } else {
            side(far, 1.0)
        })
    }

    /// 가로 변 선의 끝. 맞닿은 세로 변이 겹선이 아니면 한/글처럼 그 굵기의 반만큼 바깥으로 늘인다(그리지 않는
    /// 변도 적힌 굵기를 쓴다). 이웃 칸으로 이어지지 않는 겹선은 그린 세로 변 안쪽에서 그만큼 멈춘다.
    fn horizontal_end(&self, inner: usize, count: usize) -> f32 {
        self.double_end(inner, count).unwrap_or_else(|| {
            let (border, lines) = self.across;
            let reach = if count > 1 && !self.along && !self.open && !lines.is_empty() {
                -0.5
            } else {
                0.5
            };
            let width = self.above.unwrap_or(border).width;
            self.edge + self.outward * reach * mm(width)
        })
    }
}

/// 칸·표 테두리 네 변. `grid`는 선을 맞출 격자, `top`·`bottom`은 그 변을 그리는 방식, `joins`는 위·아래 변이 왼쪽·오른쪽 이웃 칸의 같은
/// 선으로 이어지는지([[위 왼, 위 오], [아래 왼, 아래 오]]), `vertical_joins`는 왼쪽·오른쪽 변이 위·아래
/// 이웃 칸의 같은 선으로 이어지는지(같은 꼴). `above`는 위 왼·위 오 모서리 바로 위에서 그리는 이웃 칸의
/// 세로 변이다. 한/글은 행 사이 가로 변을 윗행 쪽 세로 변 굵기만큼 늘인다. `crossing`은 모서리([위·아래][왼·오른])를
/// 가로지르는 이웃 칸의 겹선이 이 칸 쪽 가로 변보다 굵을 때 그 겹선이다. 한/글은 홑선 세로 변을 그 겹선의 가까운
/// 선에서 멈춘다(0.5mm 겹선과 0.7mm 겹선이 만나는 모서리).
#[allow(clippy::too_many_arguments)]
pub(crate) fn frame_items(
    fill: &BorderFill,
    rect: Rect,
    grid: LineGrid,
    (top, bottom): (Edge, Edge),
    joins: [[bool; 2]; 2],
    vertical_joins: [[bool; 2]; 2],
    above: [Option<&Border>; 2],
    crossing: [[Option<&Border>; 2]; 2],
    out: &mut Vec<Item>,
) {
    let (min_x, min_y, max_x, max_y) = (rect.x, rect.y, rect.right(), rect.bottom());
    let all = [
        rails(&fill.left),
        rails(&fill.right),
        rails(&fill.top),
        rails(&fill.bottom),
    ];
    let (x0, y0) = (grid.origin(true), grid.origin(false));
    let left = rail_lines(&fill.left, &all[0], min_x, x0, -1.0);
    let right = rail_lines(&fill.right, &all[1], max_x, x0, 1.0);
    let top_lines = rail_lines(&fill.top, &all[2], min_y, y0, -1.0);
    let bottom_lines = rail_lines(&fill.bottom, &all[3], max_y, y0, 1.0);
    let sides = [(&fill.left, &left[..]), (&fill.right, &right[..])];
    let ends = [
        (&fill.top, &top_lines[..]),
        (&fill.bottom, &bottom_lines[..]),
    ];
    // row: 0 위·1 아래, col: 0 왼·1 오른.
    let drawn = [top != Edge::Hidden, bottom != Edge::Hidden];
    let open = [top == Edge::Open, bottom == Edge::Open];
    let horizontal_corner = |row: usize, col: usize| Corner {
        edge: [min_x, max_x][col],
        outward: [-1.0, 1.0][col],
        across: sides[col],
        along: joins[row][col],
        beyond: vertical_joins[row][col] && !open[row],
        above: if row == 0 { above[col] } else { None },
        open: open[row],
    };
    let vertical_corner = |row: usize, col: usize| Corner {
        edge: [min_y, max_y][row],
        outward: [-1.0, 1.0][row],
        across: ends[row],
        along: vertical_joins[row][col],
        beyond: joins[row][col],
        above: None,
        open: false,
    };
    for row in 0..2 {
        let (border, lines) = ends[row];
        if !drawn[row] {
            continue;
        }
        for (inner, &(y, rail)) in lines.iter().enumerate() {
            let x1 = horizontal_corner(row, 0).horizontal_end(inner, lines.len());
            let x2 = horizontal_corner(row, 1).horizontal_end(inner, lines.len());
            stroke(rail, border.color, (x1, y), (x2, y), out);
        }
    }
    for col in [1, 0] {
        let (border, lines) = sides[col];
        for (inner, &(x, rail)) in lines.iter().enumerate() {
            let end = |row: usize| {
                let edge = [min_y, max_y][row];
                let crossed = crossing[row][col].filter(|_| drawn[row] && lines.len() < 2);
                if let Some(cross) = crossed {
                    let cross_rails = rails(cross);
                    let cross_lines = rail_lines(cross, &cross_rails, edge, y0, [-1.0, 1.0][row]);
                    if let Some(&(at, _)) = cross_lines.first().filter(|_| cross_lines.len() > 1) {
                        return at;
                    }
                }
                if drawn[row] {
                    vertical_corner(row, col)
                        .double_end(inner, lines.len())
                        .unwrap_or(edge)
                } else {
                    edge
                }
            };
            stroke(rail, border.color, (x, end(0)), (x, end(1)), out);
        }
    }
}

/// 도형 선 종류와 굵기(pt) → 점선 무늬. 빈 목록이면 실선.
fn shape_dash(kind: &str, width: f32) -> Vec<f32> {
    if width <= 0.0 {
        Vec::new()
    } else if kind == "DASH" {
        vec![width * 8.2, width * 4.9]
    } else if kind.contains("DASH") {
        vec![width * 4.0, width * 2.0]
    } else if kind.contains("DOT") || kind == "CIRCLE" {
        vec![width, width * 2.0]
    } else {
        Vec::new()
    }
}

/// 그림 바이트의 형식과 화소 크기. 화소는 PDF를 쓸 때 한 장씩 푼다.
fn decode_image(bytes: &Arc<Vec<u8>>) -> Option<(ImageData, (u32, u32))> {
    let size = imagesize::blob_size(bytes).ok();
    let dims = size.map(|size| (size.width as u32, size.height as u32));
    let shared = || bytes.clone();
    match imagesize::image_type(bytes).ok()? {
        imagesize::ImageType::Png => Some((ImageData::Png(shared()), dims?)),
        imagesize::ImageType::Jpeg => Some((ImageData::Jpeg(shared()), dims?)),
        imagesize::ImageType::Gif => Some((ImageData::Gif(shared()), dims?)),
        imagesize::ImageType::Webp => Some((ImageData::Webp(shared()), dims?)),
        imagesize::ImageType::Bmp => {
            // 크기는 화소를 풀 디코더의 머리말 해석을 따르고, 머리말이 깨진 BMP는 그리지 않는다.
            let decoder =
                image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(bytes.as_slice())).ok()?;
            Some((
                ImageData::Bmp(shared()),
                image::ImageDecoder::dimensions(&decoder),
            ))
        }
        _ => None,
    }
}

fn caption_of(item: &RunItem) -> Option<&Caption> {
    let caption = match item {
        RunItem::Table(table) => table.caption.as_ref(),
        RunItem::Picture(picture) => picture.caption.as_ref(),
        RunItem::Shape(shape) => shape.caption.as_ref(),
        _ => None,
    }?;
    (!caption.paragraphs.is_empty()).then_some(caption)
}

impl<'a> Renderer<'a> {
    /// 개체가 차지하는 크기(pt).
    pub(crate) fn object_size(&mut self, item: &'a RunItem) -> (f32, f32) {
        match item {
            RunItem::Table(table) => {
                let (columns, rows) = self.grid(table);
                let spacing = table.cell_spacing.max(0.0);
                (
                    pt(columns.iter().sum::<f64>() + (columns.len() + 1) as f64 * spacing),
                    pt(rows.iter().sum::<f64>() + (rows.len() + 1) as f64 * spacing),
                )
            }
            RunItem::Picture(picture) => (pt(picture.width), pt(picture.height)),
            RunItem::Shape(shape) => {
                // 한/글(e12): 줄 정보 없는 글이 글상자보다 길면 글상자가 글 높이만큼 늘어난다.
                let mut height = shape.height;
                if shape
                    .paragraphs
                    .iter()
                    .any(|para| para.line_segs.is_empty())
                {
                    let margins = &shape.text_margins;
                    let width = shape.width - margins.left - margins.right;
                    let content = self.shape_text_height(shape, width);
                    height = height.max(content + margins.top + margins.bottom);
                }
                (pt(shape.width), pt(height))
            }
            _ => (0.0, 0.0),
        }
    }

    /// 글상자 글의 높이(HWPUNIT): 저장된 줄 높이로, 줄 정보가 없는 문단은 흘려 본 줄로 잰다.
    fn shape_text_height(&mut self, shape: &'a Shape, width: f64) -> f64 {
        let mut content = 0.0f64;
        let mut next_top = 0.0f64;
        for para in &shape.paragraphs {
            let flowed;
            let segs = if para.line_segs.is_empty() {
                let pp = self.para_pr(para.para_pr_id);
                flowed = self.flow_segs(para, width, next_top + pp.prev);
                if let Some(last) = flowed.last() {
                    next_top = last.vertpos + last.vertsize + last.spacing + pp.next;
                }
                &flowed
            } else {
                &para.line_segs
            };
            for seg in segs {
                content = content.max(seg.vertpos + seg.vertsize);
            }
        }
        content
    }

    /// 글자처럼 놓인 수식의 위(pt). 수식 엔진을 옮기기 전에는 줄 기준선에 수식 기준선(높이 × baseline%)을 맞춘다.
    pub(crate) fn inline_equation_top(
        &self,
        item: &RunItem,
        baseline: f32,
        height: f32,
    ) -> Option<f32> {
        let RunItem::Shape(shape) = item else {
            return None;
        };
        let equation = shape.equation.as_ref()?;
        let fraction = if equation.baseline > 0.0 && equation.baseline <= 100.0 {
            equation.baseline as f32 / 100.0
        } else {
            0.85
        };
        Some(baseline - height * fraction)
    }

    pub(crate) fn border_line(&mut self, border: &Border, from: (f32, f32), to: (f32, f32)) {
        let mut items = Vec::new();
        border_items(border, from, to, &mut items);
        for item in items {
            self.add(item);
        }
    }

    /// 캡션이 개체 둘레에 더하는 자리(pt): [왼, 위, 오른, 아래]. 한/글은 글자처럼 놓인 개체의 줄 높이에
    /// 캡션 높이와 간격을 더해 저장한다(표 23192 + 캡션 1300 + 간격 850 + 바깥 여백 280 = 25622).
    pub(crate) fn caption_space(&mut self, item: &'a RunItem, width: f32) -> [f32; 4] {
        let Some(caption) = caption_of(item) else {
            return [0.0; 4];
        };
        let gap = pt(caption.gap);
        match caption.side.as_str() {
            "LEFT" => [pt(caption.width) + gap, 0.0, 0.0, 0.0],
            "RIGHT" => [0.0, 0.0, pt(caption.width) + gap, 0.0],
            side => {
                let height = self.caption_height(caption, width) + gap;
                if side == "TOP" {
                    [0.0, height, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0, height]
                }
            }
        }
    }

    fn caption_height(&mut self, caption: &'a Caption, width: f32) -> f32 {
        let mut bottom = 0.0;
        for para in &caption.paragraphs {
            let segs = if para.line_segs.is_empty() {
                self.flow_segs(para, f64::from(width) * 100.0, bottom)
            } else {
                para.line_segs.clone()
            };
            if let Some(last) = segs.last() {
                bottom = last.vertpos + last.vertsize;
            }
        }
        pt(bottom)
    }

    /// 개체 사각형(rect, 바깥 여백 안쪽) 둘레에 캡션 문단을 놓는다.
    pub(crate) fn draw_caption(&mut self, item: &'a RunItem, rect: Rect, section: &'a Section) {
        let Some(caption) = caption_of(item) else {
            return;
        };
        let gap = pt(caption.gap);
        let side_width = pt(caption.width);
        let (origin, width) = match caption.side.as_str() {
            "LEFT" => ((rect.x - gap - side_width, rect.y), side_width),
            "RIGHT" => ((rect.right() + gap, rect.y), side_width),
            "TOP" => {
                let height = self.caption_height(caption, rect.width);
                ((rect.x, rect.y - gap - height), rect.width)
            }
            _ => ((rect.x, rect.bottom() + gap), rect.width),
        };
        let mut last_vert = -1.0;
        self.rec.begin(Kind::Caption, "캡션", "", Flags::default());
        for para in &caption.paragraphs {
            self.layout_paragraph(para, origin, width, section, false, &mut last_vert, None);
        }
        self.rec.end();
    }

    pub(crate) fn place(&mut self, item: &'a RunItem, rect: Rect, section: &'a Section) {
        let Some((kind, name, text)) = record::object_label(item).filter(|_| self.rec.on) else {
            self.place_object(item, rect, section);
            return;
        };
        let flags = Flags {
            floating: !object_pos(item).treat_as_char && !self.rec.inside_object(),
            behind: self.behind,
            hidden: false,
        };
        self.rec.begin(kind, &name, &text, flags);
        self.rec_extend(rect);
        self.place_object(item, rect, section);
        self.rec.end();
    }

    fn place_object(&mut self, item: &'a RunItem, rect: Rect, section: &'a Section) {
        match item {
            RunItem::Table(table) => self.layout_table(table, (rect.x, rect.y), section),
            RunItem::Picture(picture) => {
                self.place_picture(picture, rect);
                if let Some(border) = &picture.outline {
                    let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
                    self.border_line(border, (left, top), (right, top));
                    self.border_line(border, (left, bottom), (right, bottom));
                    self.border_line(border, (left, top), (left, bottom));
                    self.border_line(border, (right, top), (right, bottom));
                }
            }
            RunItem::Shape(shape) => self.layout_shape(shape, rect, section),
            _ => {}
        }
    }

    fn image(&mut self, id: &str) -> Option<super::Decoded> {
        if let Some(cached) = self.images.get(id) {
            return cached.clone();
        }
        let doc = self.doc;
        let decoded = doc
            .bins
            .iter()
            .find(|blob| blob.id == id)
            .and_then(|blob| decode_image(&blob.data))
            .map(|(data, size)| (Arc::new(data), size));
        self.images.insert(id.to_string(), decoded.clone());
        decoded
    }

    /// 도형의 그림 채우기. 가운데 배치는 그림 본디 크기(96dpi)로 두고 도형 밖은 오려 내며, 그 밖은 도형에 늘여 채운다.
    fn image_fill(&mut self, fill: &ImageFill, rect: Rect) {
        let Some((data, (px_w, px_h))) = self.image(&fill.binary_id) else {
            return;
        };
        if fill.mode == "CENTER" {
            let (w, h) = (pt(f64::from(px_w) * 75.0), pt(f64::from(px_h) * 75.0));
            let placed = Rect::new(
                rect.x + (rect.width - w) / 2.0,
                rect.y + (rect.height - h) / 2.0,
                w,
                h,
            );
            self.add(Item::Clip(rect));
            self.add(Item::Image { data, rect: placed });
            self.add(Item::Pop);
        } else {
            self.add(Item::Image { data, rect });
        }
    }

    /// 그림. 자르기는 그림 전체를 늘여 그리고 놓인 사각형으로 오려 낸다.
    fn place_picture(&mut self, picture: &Picture, rect: Rect) {
        let Some((data, (px_w, px_h))) = self.image(&picture.binary_id) else {
            return;
        };
        let (px_w, px_h) = (px_w as f64, px_h as f64);
        if px_w <= 0.0 || px_h <= 0.0 {
            return;
        }
        let mut crop: Option<(f64, f64, f64, f64)> = None;
        if let Some(clip) = picture.clip {
            let (x0, y0) = (clip[0][0].min(clip[1][0]), clip[0][1].min(clip[1][1]));
            let (x1, y1) = (clip[0][0].max(clip[1][0]), clip[0][1].max(clip[1][1]));
            let holds = |(w, h): (f64, f64)| w > 0.0 && h > 0.0 && x1 <= w * 1.01 && y1 <= h * 1.01;
            let stored = (picture.orig_width, picture.orig_height);
            let natural = (px_w * 75.0, px_h * 75.0);
            let reference = if holds(stored) {
                Some(stored)
            } else if holds(natural) {
                Some(natural)
            } else {
                None
            };
            if let Some((rw, rh)) = reference {
                let left = (x0 * px_w / rw).max(0.0).floor();
                let top = (y0 * px_h / rh).max(0.0).floor();
                let right = (x1 * px_w / rw).min(px_w).ceil();
                let bottom = (y1 * px_h / rh).min(px_h).ceil();
                let (w, h) = (right - left, bottom - top);
                if w > 0.0 && h > 0.0 && (w < px_w - 1.0 || h < px_h - 1.0) {
                    crop = Some((left, top, w, h));
                }
            }
        } else if picture.orig_width > 0.0
            && picture.orig_height > 0.0
            && picture.crop_left + picture.crop_right + picture.crop_top + picture.crop_bottom > 0.0
        {
            let sx = px_w / picture.orig_width;
            let sy = px_h / picture.orig_height;
            let left = (picture.crop_left * sx).floor();
            let top = (picture.crop_top * sy).floor();
            let w = ((picture.orig_width - picture.crop_left - picture.crop_right) * sx)
                .max(1.0)
                .ceil();
            let h = ((picture.orig_height - picture.crop_top - picture.crop_bottom) * sy)
                .max(1.0)
                .ceil();
            crop = Some((left, top, w, h));
        }
        match crop {
            Some((left, top, w, h)) => {
                let sx = f64::from(rect.width) / w;
                let sy = f64::from(rect.height) / h;
                let full = Rect::new(
                    rect.x - (left * sx) as f32,
                    rect.y - (top * sy) as f32,
                    (px_w * sx) as f32,
                    (px_h * sy) as f32,
                );
                self.add(Item::Clip(rect));
                self.add(Item::Image { data, rect: full });
                self.add(Item::Pop);
            }
            None => self.add(Item::Image { data, rect }),
        }
    }

    fn layout_shape(&mut self, shape: &'a Shape, rect: Rect, section: &'a Section) {
        if let Some(equation) = &shape.equation {
            if self.records_text {
                self.record_text(equation.script.clone());
            }
            for item in crate::equation::items(self.fonts, equation, rect) {
                self.add(item);
            }
            return;
        }
        let sx = if shape.width > 0.0 && rect.width > 0.0 {
            rect.width / pt(shape.width)
        } else {
            1.0
        };
        let sy = if shape.height > 0.0 && rect.height > 0.0 {
            rect.height / pt(shape.height)
        } else {
            1.0
        };
        let point = |p: [f64; 2]| (rect.x + pt(p[0]) * sx, rect.y + pt(p[1]) * sy);
        if shape.kind == "container" {
            for child in &shape.children {
                let size = match child.item.as_ref() {
                    RunItem::Picture(picture) => (picture.width, picture.height),
                    RunItem::Shape(shape) => (shape.width, shape.height),
                    _ => (0.0, 0.0),
                };
                if let Some(m) = child.transform
                    && size.0 > 0.0
                    && size.1 > 0.0
                {
                    // 자식 좌표(pt) → HWPUNIT → 묶음 변환 → 묶음 사각형 안의 pt.
                    let (a, b, c, d, e, f) = (m[0], m[1], m[2], m[3], m[4], m[5]);
                    let k = 0.01;
                    let transform = [
                        (a * f64::from(sx)) as f32,
                        (b * f64::from(sy)) as f32,
                        (c * f64::from(sx)) as f32,
                        (d * f64::from(sy)) as f32,
                        (e * k) as f32 * sx + rect.x,
                        (f * k) as f32 * sy + rect.y,
                    ];
                    self.add(Item::Transform(transform));
                    self.place(
                        &child.item,
                        Rect::new(0.0, 0.0, pt(size.0), pt(size.1)),
                        section,
                    );
                    self.add(Item::Pop);
                    continue;
                }
                // 자식 틀은 두 꼭짓점이 아니라 [왼쪽 위, 크기]다(HWP 5·HWPX 모두).
                let [origin, extent] = child.frame;
                self.place(
                    &child.item,
                    Rect::new(
                        rect.x + pt(origin[0]) * sx,
                        rect.y + pt(origin[1]) * sy,
                        pt(extent[0]) * sx,
                        pt(extent[1]) * sy,
                    ),
                    section,
                );
            }
            return;
        }
        let width = if shape.line_type == "NONE" {
            0.0
        } else {
            mm(shape.line_width)
        };
        let dash = shape_dash(&shape.line_type, width);
        let alpha = shape.fill_alpha as f32;
        match shape.kind.as_str() {
            "line" => {
                let (from, to) = if shape.points.len() >= 2 {
                    (point(shape.points[0]), point(shape.points[1]))
                } else {
                    ((rect.x, rect.y), (rect.right(), rect.bottom()))
                };
                self.line(from, to, width, shape.line_color, dash);
            }
            "polygon" | "curve" => {
                if shape.points.len() >= 2 {
                    let mut path: Vec<PathOp> = shape
                        .points
                        .iter()
                        .enumerate()
                        .map(|(index, p)| {
                            let (x, y) = point(*p);
                            if index == 0 {
                                PathOp::Move(x, y)
                            } else {
                                PathOp::Line(x, y)
                            }
                        })
                        .collect();
                    if shape.points.len() >= 3 && shape.points.first() == shape.points.last() {
                        path.push(PathOp::Close);
                    }
                    let fill = if shape.kind == "polygon" {
                        shape.fill_color
                    } else {
                        None
                    };
                    self.path(path, fill, alpha, shape.line_color, width, dash);
                }
            }
            "ellipse" => {
                self.path(
                    super::text::ellipse(rect),
                    shape.fill_color,
                    alpha,
                    shape.line_color,
                    width,
                    dash,
                );
            }
            "arc" | "ole" => {}
            _ => {
                let radius =
                    rect.width.min(rect.height) * (shape.round_ratio as f32).min(50.0) / 100.0;
                if radius > 0.0 {
                    self.path(
                        rounded(rect, radius),
                        shape.fill_color,
                        alpha,
                        shape.line_color,
                        width,
                        dash,
                    );
                } else {
                    if let Some(color) = shape.fill_color {
                        self.fill(rect, color, alpha);
                    }
                    if let Some(fill) = &shape.image_fill {
                        self.image_fill(fill, rect);
                    }
                    let (l, t, r, b) = (rect.x, rect.y, rect.right(), rect.bottom());
                    for (from, to) in [
                        ((l, t), (r, t)),
                        ((l, b), (r, b)),
                        ((l, t), (l, b)),
                        ((r, t), (r, b)),
                    ] {
                        self.line(from, to, width, shape.line_color, dash.clone());
                    }
                }
            }
        }
        if !shape.paragraphs.is_empty() {
            let margins = &shape.text_margins;
            let inner = rect.height - pt(margins.top + margins.bottom);
            // 한/글은 마지막 줄의 줄 간격을 빼고 저장된 줄 높이로 글을 가운데·아래에 맞춘다.
            // 줄 정보가 없는 문단은 흘려 본 줄로 높이를 잰다.
            let width = f64::from(rect.width - pt(margins.left + margins.right)) * 100.0;
            let content = self.shape_text_height(shape, width);
            let dy = match shape.text_vert_align.as_str() {
                "CENTER" => ((inner - pt(content)) / 2.0).max(0.0),
                "BOTTOM" => (inner - pt(content)).max(0.0),
                _ => 0.0,
            };
            let origin = (rect.x + pt(margins.left), rect.y + pt(margins.top) + dy);
            let saved = self.text_clip_bottom;
            let limit = rect.bottom() - pt(margins.bottom);
            self.text_clip_bottom = Some(saved.map_or(limit, |saved| saved.min(limit)));
            let avail = rect.width - pt(margins.left + margins.right);
            let mut last_vert = -1.0;
            for para in &shape.paragraphs {
                self.layout_paragraph(para, origin, avail, section, false, &mut last_vert, None);
            }
            self.text_clip_bottom = saved;
        }
    }

    fn path(
        &mut self,
        path: Vec<PathOp>,
        fill: Option<u32>,
        alpha: f32,
        color: u32,
        width: f32,
        dash: Vec<f32>,
    ) {
        if let Some(fill) = fill {
            self.add(Item::Fill {
                path: path.clone(),
                color: fill,
                alpha,
            });
        }
        if width > 0.0 {
            self.add(Item::Stroke {
                path,
                stroke: scene::Stroke { width, color, dash },
            });
        }
    }
}

/// 모서리가 둥근 사각형.
fn rounded(rect: Rect, radius: f32) -> Vec<PathOp> {
    const K: f32 = 0.552_284_8;
    let r = radius.min(rect.width / 2.0).min(rect.height / 2.0);
    let (l, t, rr, b) = (rect.x, rect.y, rect.right(), rect.bottom());
    let c = r * (1.0 - K);
    vec![
        PathOp::Move(l + r, t),
        PathOp::Line(rr - r, t),
        PathOp::Cubic(rr - c, t, rr, t + c, rr, t + r),
        PathOp::Line(rr, b - r),
        PathOp::Cubic(rr, b - c, rr - c, b, rr - r, b),
        PathOp::Line(l + r, b),
        PathOp::Cubic(l + c, b, l, b - c, l, b - r),
        PathOp::Line(l, t + r),
        PathOp::Cubic(l, t + c, l + c, t, l + r, t),
        PathOp::Close,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 위 변 선들의 왼쪽 끝(x), 위에서부터.
    fn top_starts(joins: [[bool; 2]; 2], vertical_joins: [[bool; 2]; 2]) -> Vec<f32> {
        let double = Border {
            border_type: "DOUBLE_SLIM".into(),
            width: 0.5,
            color: 0,
        };
        let fill = BorderFill {
            left: double.clone(),
            right: double.clone(),
            top: double.clone(),
            bottom: double,
            ..BorderFill::default()
        };
        let mut out = Vec::new();
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        frame_items(
            &fill,
            rect,
            LineGrid::Body(0.0, 0.0),
            (Edge::Drawn, Edge::Drawn),
            joins,
            vertical_joins,
            [None; 2],
            [[None; 2]; 2],
            &mut out,
        );
        let mut starts: Vec<(f32, f32)> = out
            .iter()
            .filter_map(|item| match item {
                Item::Stroke { path, .. } => match path[..] {
                    [PathOp::Move(x1, y1), PathOp::Line(_, y2)] if y1 == y2 && y1 < 25.0 => {
                        Some((y1, x1))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();
        starts.sort_by(|a, b| a.0.total_cmp(&b.0));
        starts.into_iter().map(|(_, x)| x).collect()
    }

    #[test]
    fn double_borders_close_each_quadrant_at_crossings() {
        // ╬: 위 변의 두 선 모두 왼쪽 세로 변의 안쪽 선에서 끝난다(칸마다 L자로 닫힘).
        let crossing = top_starts([[true; 2]; 2], [[true; 2]; 2]);
        assert_eq!(crossing.len(), 2);
        assert_eq!(crossing[0], crossing[1]);
        assert!(crossing[0] > 0.0);
        // ╔: 바깥 선은 세로 변의 바깥 선까지 간다.
        let corner = top_starts([[false; 2]; 2], [[false; 2]; 2]);
        assert!(corner[0] < 0.0 && corner[1] > 0.0);
    }

    fn grid_units(kind: &str, width: f64, hwpunit: f32, origin: Option<f32>) -> Vec<i32> {
        let border = Border {
            border_type: kind.into(),
            width,
            color: 0,
        };
        let rails = rails(&border);
        let mut units: Vec<i32> = rail_lines(
            &border,
            &rails,
            hwpunit / 100.0,
            origin.map(|origin| origin / 100.0),
            1.0,
        )
        .iter()
        .map(|(at, _)| (at / DEVICE_UNIT).round() as i32)
        .collect();
        units.sort_unstable();
        units
    }

    #[test]
    fn edge_grid_unit_depends_on_body_origin() {
        // 한/글 12.30.0에서 잰 자리. 왼쪽 여백 30mm(8504) 문서와 20mm(5669) 문서.
        for (edge, unit) in [(8504.0, 709), (20504.0, 1709), (34504.0, 2876)] {
            assert_eq!(grid_units("SOLID", 0.12, edge, Some(8504.0)), vec![unit]);
        }
        for (edge, unit) in [(5669.0, 472), (5674.0, 472), (5675.0, 473)] {
            assert_eq!(grid_units("SOLID", 0.12, edge, Some(5669.0)), vec![unit]);
        }
    }

    #[test]
    fn scaled_page_rounds_the_edge_alone() {
        // 본문 원점 20mm(5669)에서 원점과 거리를 따로 반올림하면 472칸이지만, 줄인 쪽은 변 자리만 반올림한다.
        assert_eq!(grid_units("SOLID", 0.12, 5674.0, Some(5669.0)), vec![472]);
        assert_eq!(grid_units("SOLID", 0.12, 5674.0, None), vec![473]);
    }

    #[test]
    fn double_strands_sit_at_hancom_measured_offsets() {
        for (width, low, high) in [(0.4, 997, 1003), (0.5, 995, 1004), (0.7, 994, 1006)] {
            assert_eq!(
                grid_units("DOUBLE_SLIM", width, 12000.0, Some(0.0)),
                vec![low, high],
                "{width}mm"
            );
        }
    }

    #[test]
    fn dash_matches_hancom_pdf_grid_counts() {
        // 한/글 PDF에서 잰 (굵기, 켜짐, 꺼짐) 격자 칸 수.
        for (width, on, off) in [(3.0, 4.0, 6.0), (4.0, 5.0, 8.0), (5.0, 7.0, 11.0)] {
            assert_eq!(
                hancom_dash(width * DEVICE_UNIT),
                vec![on * DEVICE_UNIT, off * DEVICE_UNIT]
            );
        }
    }

    #[test]
    fn single_side_stops_at_a_thicker_double_crossing_its_corner() {
        // 0.12mm 세로 변이 오른쪽 이웃 칸의 0.7mm 겹선(제 칸의 0.5mm 겹선보다 굵다)을 만나면
        // 한/글은 그 겹선의 위 선에서 멈춘다.
        let solid = Border {
            border_type: "SOLID".into(),
            width: 0.12,
            color: 0,
        };
        let cross = Border {
            border_type: "DOUBLE_SLIM".into(),
            width: 0.7,
            color: 0,
        };
        let fill = BorderFill {
            right: solid,
            ..BorderFill::default()
        };
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let mut out = Vec::new();
        frame_items(
            &fill,
            rect,
            LineGrid::Body(0.0, 0.0),
            (Edge::Drawn, Edge::Drawn),
            [[false; 2]; 2],
            [[true; 2]; 2],
            [None; 2],
            [[None, None], [None, Some(&cross)]],
            &mut out,
        );
        let cross_rails = rails(&cross);
        let top_line = rail_lines(&cross, &cross_rails, 50.0, Some(0.0), 1.0)[0].0;
        assert!(top_line < 50.0);
        let ends: Vec<f32> = out
            .iter()
            .filter_map(|item| match item {
                Item::Stroke { path, .. } => match path[..] {
                    [PathOp::Move(x0, _), PathOp::Line(x1, y1)] if x0 == x1 => Some(y1),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(ends, vec![top_line]);
    }
}
