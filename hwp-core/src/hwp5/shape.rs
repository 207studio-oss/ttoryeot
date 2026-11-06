use crate::model::{
    Arrow, Equation, Gradient, HwpUnit, ImageFill, Margins, Picture, Point, Pos, Rect, RunItem,
    Shape, ShapeChild,
};

#[path = "shape_fill.rs"]
pub(super) mod fill;
#[path = "shape_geometry.rs"]
mod geometry;
pub(super) use geometry::apply_geometry;

use super::control::{list_groups, parse_nested, rgb, ListGroup, ParseContext};
use super::docinfo::SHAPE_LINE_TYPES;
use super::reader::BinReader;
use super::record::Record;
use super::tags::{T_SC_CONTAINER, T_SC_PICTURE, T_SHAPE_COMPONENT};

const VERT_REL_TO: [&str; 4] = ["PAPER", "PAGE", "PARA", "PARA"];
const VERT_ALIGNS: [&str; 8] = [
    "TOP", "CENTER", "BOTTOM", "INSIDE", "OUTSIDE", "TOP", "TOP", "TOP",
];
const HORZ_REL_TO: [&str; 4] = ["PAPER", "PAGE", "COLUMN", "PARA"];
const HORZ_ALIGNS: [&str; 8] = [
    "LEFT", "CENTER", "RIGHT", "INSIDE", "OUTSIDE", "LEFT", "LEFT", "LEFT",
];
const WIDTH_REL_TO: [&str; 8] = [
    "PAPER", "PAGE", "COLUMN", "PARA", "ABSOLUTE", "ABSOLUTE", "ABSOLUTE", "ABSOLUTE",
];
const HEIGHT_REL_TO: [&str; 4] = ["PAPER", "PAGE", "ABSOLUTE", "ABSOLUTE"];
const TEXT_WRAPS: [&str; 8] = [
    "SQUARE",
    "TOP_AND_BOTTOM",
    "BEHIND_TEXT",
    "IN_FRONT_OF_TEXT",
    "SQUARE",
    "SQUARE",
    "SQUARE",
    "SQUARE",
];

#[derive(Clone)]
pub(super) struct Common {
    pub pos: Pos,
    pub width: HwpUnit,
    pub height: HwpUnit,
    pub margins: Margins,
    pub z_order: i32,
}

#[derive(Clone, Copy)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl Affine {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            b: 0.0,
            c: 0.0,
            d: y,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn concatenating(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            tx: self.tx * other.a + self.ty * other.c + other.tx,
            ty: self.tx * other.b + self.ty * other.d + other.ty,
        }
    }

    fn apply(self, point: Point) -> Point {
        [
            self.a * point[0] + self.c * point[1] + self.tx,
            self.b * point[0] + self.d * point[1] + self.ty,
        ]
    }

    fn is_usable(self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|value| value.is_finite() && value.abs() < 1e9)
    }
}

#[derive(Clone)]
pub(super) struct ShapeComponent {
    orig_w: HwpUnit,
    orig_h: HwpUnit,
    pub(super) cur_w: HwpUnit,
    pub(super) cur_h: HwpUnit,
    matrix: Affine,
    line_type: &'static str,
    line_color: u32,
    line_width: HwpUnit,
    arrow_start: Option<Arrow>,
    arrow_end: Option<Arrow>,
    fill_color: Option<u32>,
    fill_alpha: HwpUnit,
    gradient: Option<Gradient>,
    image_fill: Option<ImageFill>,
}

impl Default for ShapeComponent {
    fn default() -> Self {
        Self {
            orig_w: 0.0,
            orig_h: 0.0,
            cur_w: 0.0,
            cur_h: 0.0,
            matrix: Affine::identity(),
            line_type: "NONE",
            line_color: 0,
            line_width: 0.0,
            arrow_start: None,
            arrow_end: None,
            fill_color: None,
            fill_alpha: 1.0,
            gradient: None,
            image_fill: None,
        }
    }
}

pub(super) fn parse_common(reader: &mut BinReader<'_>) -> Common {
    let attr = reader.u32();
    let pos = Pos {
        treat_as_char: attr & 1 != 0,
        affect_l_spacing: attr & 4 != 0,
        vert_rel_to: VERT_REL_TO[((attr >> 3) & 3) as usize].to_string(),
        vert_align: VERT_ALIGNS[((attr >> 5) & 7) as usize].to_string(),
        horz_rel_to: HORZ_REL_TO[((attr >> 8) & 3) as usize].to_string(),
        horz_align: HORZ_ALIGNS[((attr >> 10) & 7) as usize].to_string(),
        width_rel_to: WIDTH_REL_TO[((attr >> 15) & 7) as usize].to_string(),
        height_rel_to: HEIGHT_REL_TO[((attr >> 18) & 3) as usize].to_string(),
        text_wrap: TEXT_WRAPS[((attr >> 21) & 7) as usize].to_string(),
        flow_with_text: attr & (1 << 13) != 0,
        allow_overlap: attr & (1 << 14) != 0,
        vert_offset: HwpUnit::from(reader.i32()),
        horz_offset: HwpUnit::from(reader.i32()),
    };
    let width = HwpUnit::from(reader.u32());
    let height = HwpUnit::from(reader.u32());
    let z_order = reader.i32();
    let margins = Margins {
        left: HwpUnit::from(reader.i16()),
        right: HwpUnit::from(reader.i16()),
        top: HwpUnit::from(reader.i16()),
        bottom: HwpUnit::from(reader.i16()),
    };
    let _ = reader.u32();
    Common {
        pos,
        width,
        height,
        margins,
        z_order,
    }
}

fn read_matrix(reader: &mut BinReader<'_>) -> Affine {
    let e: [f64; 6] = std::array::from_fn(|_| reader.f64());
    Affine {
        a: e[0],
        b: e[3],
        c: e[1],
        d: e[4],
        tx: e[2],
        ty: e[5],
    }
}

pub(super) fn shape_component(data: &[u8], container: bool) -> ShapeComponent {
    let mut component = ShapeComponent::default();
    let mut reader = BinReader::new(data);
    let id = reader.u32();
    if data.len() >= 8 {
        let mut twin = BinReader::at(data, 4);
        if twin.u32() == id {
            reader.skip(4);
        }
    }
    reader.skip(12);
    component.orig_w = HwpUnit::from(reader.u32());
    component.orig_h = HwpUnit::from(reader.u32());
    component.cur_w = HwpUnit::from(reader.u32());
    component.cur_h = HwpUnit::from(reader.u32());
    reader.skip(14);
    let pairs = usize::from(reader.u16());
    let mut matrix = None;
    if reader.remaining() >= 48 {
        let mut combined = read_matrix(&mut reader);
        for _ in 0..pairs {
            if reader.remaining() < 96 {
                break;
            }
            let scale = read_matrix(&mut reader);
            let rotation = read_matrix(&mut reader);
            combined = scale.concatenating(rotation).concatenating(combined);
        }
        if combined.is_usable() {
            matrix = Some(combined);
        }
    }
    component.matrix = matrix.unwrap_or_else(|| {
        Affine::scale(
            if component.orig_w > 0.0 && component.cur_w > 0.0 {
                component.cur_w / component.orig_w
            } else {
                1.0
            },
            if component.orig_h > 0.0 && component.cur_h > 0.0 {
                component.cur_h / component.orig_h
            } else {
                1.0
            },
        )
    });
    if container || reader.remaining() < 13 {
        return component;
    }
    let color = reader.u32();
    let width = reader.i32();
    let attr = reader.u32();
    let _ = reader.u8();
    let line_index = usize::from((attr & 0x3F) as u8);
    component.line_type = SHAPE_LINE_TYPES.get(line_index).copied().unwrap_or("SOLID");
    component.line_color = rgb(color);
    component.line_width = f64::from(width.max(0)) / 7200.0 * 25.4;
    component.arrow_start = fill::arrow(
        (attr >> 10) & 0x3F,
        (attr >> 22) & 0xF,
        attr & (1 << 30) != 0,
    );
    component.arrow_end = fill::arrow(
        (attr >> 16) & 0x3F,
        (attr >> 26) & 0xF,
        attr & (1 << 31) != 0,
    );
    if reader.remaining() < 4 {
        return component;
    }
    let fill_type = reader.u32();
    if fill_type == 0 {
        return component;
    }
    let mut solid = None;
    if fill_type & 1 != 0 {
        let background = reader.u32();
        let _ = reader.u32();
        solid = Some((background, reader.i32()));
    }
    if fill_type & 4 != 0 {
        component.gradient = fill::gradient(&mut reader);
    }
    if fill_type & 2 != 0 {
        component.image_fill = fill::image(&mut reader);
    }
    let extra = reader.u32() as usize;
    let extra_data = reader.bytes(extra);
    if let (Some(gradient), Some(step_center)) = (&mut component.gradient, extra_data.first()) {
        gradient.step_center = i32::from(*step_center);
    }
    let alpha = if fill_type & 1 != 0 { reader.u8() } else { 0 };
    if let Some((background, pattern)) = solid {
        if pattern <= 0 && background >> 24 == 0 {
            component.fill_color = Some(rgb(background));
            component.fill_alpha = 1.0 - f64::from(alpha) / 255.0;
        }
    }
    component
}

fn picture_info(data: &[u8], base: usize, max_bin: usize) -> (usize, Option<(HwpUnit, HwpUnit)>) {
    let mut reader = BinReader::at(data, base.saturating_add(3));
    let mut bin = 0usize;
    if reader.remaining() >= 2 {
        let value = usize::from(reader.u16());
        if value >= 1 && value <= max_bin {
            bin = value;
        }
    }
    let mut size_reader = BinReader::at(data, base.saturating_add(14));
    let size = (size_reader.remaining() >= 8).then(|| {
        (
            HwpUnit::from(size_reader.u32()),
            HwpUnit::from(size_reader.u32()),
        )
    });
    (bin, size)
}

pub(super) fn read_picture(picture: &mut Picture, record: &Record, max_bin: usize) {
    let mut reader = record.reader();
    let _ = reader.u32();
    let _ = reader.i32();
    let _ = reader.u32();
    let mut xs = [0i32; 4];
    let mut ys = [0i32; 4];
    for index in 0..4 {
        xs[index] = reader.i32();
        ys[index] = reader.i32();
    }
    let min_x = *xs.iter().min().unwrap_or(&0);
    let max_x = *xs.iter().max().unwrap_or(&0);
    let min_y = *ys.iter().min().unwrap_or(&0);
    let max_y = *ys.iter().max().unwrap_or(&0);
    picture.orig_width = f64::from(max_x) - f64::from(min_x);
    picture.orig_height = f64::from(max_y) - f64::from(min_y);
    let clip_left = f64::from(reader.i32());
    let clip_top = f64::from(reader.i32());
    let clip_right = f64::from(reader.i32());
    let clip_bottom = f64::from(reader.i32());
    if clip_right > clip_left && clip_bottom > clip_top {
        picture.clip = Some([
            [clip_left, clip_top],
            [clip_right - clip_left, clip_bottom - clip_top],
        ]);
    }
    for _ in 0..4 {
        let _ = reader.i16();
    }
    let info = picture_info(record.data.as_slice(), reader.position(), max_bin);
    picture.binary_id = format!("bin{}", info.0);
    let mut dimensions = Vec::with_capacity(3);
    if let Some(size) = info.1 {
        dimensions.push(size);
    }
    for back in [8usize, 9] {
        if record.data.len() >= back {
            let mut tail = BinReader::at(record.data.as_slice(), record.data.len() - back);
            dimensions.push((HwpUnit::from(tail.u32()), HwpUnit::from(tail.u32())));
        }
    }
    if let Some(clip) = picture.clip {
        let max_clip_x = clip[0][0] + clip[1][0];
        let max_clip_y = clip[0][1] + clip[1][1];
        if let Some((width, height)) = dimensions.into_iter().find(|(width, height)| {
            *width > 0.0
                && *height > 0.0
                && *width < 10_000_000.0
                && *height < 10_000_000.0
                && max_clip_x <= width * 1.01
                && max_clip_y <= height * 1.01
        }) {
            picture.orig_width = width;
            picture.orig_height = height;
        }
    }
}

fn transformed_frame(width: HwpUnit, height: HwpUnit, matrix: Affine) -> Rect {
    let corners =
        [[0.0, 0.0], [width, 0.0], [0.0, height], [width, height]].map(|point| matrix.apply(point));
    let min_x = corners
        .iter()
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);
    let min_y = corners
        .iter()
        .map(|point| point[1])
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = corners
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max);
    [[min_x, min_y], [max_x - min_x, max_y - min_y]]
}

fn point_is_finite(point: Point) -> bool {
    point[0].is_finite() && point[1].is_finite()
}

fn rect_is_finite(rect: Rect) -> bool {
    rect.iter().all(|point| point_is_finite(*point))
}

pub(super) fn read_text_box(
    shape: &mut Shape,
    group: &ListGroup<'_>,
    context: &mut ParseContext<'_>,
) {
    let mut reader = group.header.reader();
    let _ = reader.i16();
    let _ = reader.u16();
    let attr = reader.u32();
    shape.text_vert_align =
        ["TOP", "CENTER", "BOTTOM", "TOP"][((attr >> 5) & 3) as usize].to_string();
    if reader.remaining() >= 8 {
        let left = HwpUnit::from(reader.u16());
        let right = HwpUnit::from(reader.u16());
        let top = HwpUnit::from(reader.u16());
        let bottom = HwpUnit::from(reader.u16());
        if left.max(right).max(top).max(bottom) < 20000.0 {
            shape.text_margins = Margins {
                left,
                right,
                top,
                bottom,
            };
        }
    }
    shape.paragraphs = parse_nested(&group.paragraphs, context);
}

pub(super) fn is_container(component_record: &Record) -> bool {
    component_record
        .children
        .iter()
        .any(|child| child.tag == T_SC_CONTAINER || child.tag == T_SHAPE_COMPONENT)
}

pub(super) fn group_children(
    component_record: &Record,
    context: &mut ParseContext<'_>,
) -> Vec<ShapeChild> {
    let section = context.section_index;
    if !context.state.enter(context.warnings, section) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for child in component_record
        .children
        .iter()
        .filter(|child| child.tag == T_SHAPE_COMPONENT)
    {
        let container = is_container(child);
        let component = shape_component(&child.data, container);
        if container {
            out.extend(group_children(child, context));
            continue;
        }
        let frame = transformed_frame(component.orig_w, component.orig_h, component.matrix);
        if !rect_is_finite(frame) {
            continue;
        }
        if let Some(picture_record) = child
            .children
            .iter()
            .find(|record| record.tag == T_SC_PICTURE)
        {
            let mut picture = Picture {
                width: frame[1][0],
                height: frame[1][1],
                ..Picture::default()
            };
            read_picture(&mut picture, picture_record, context.state.max_bin);
            out.push(ShapeChild {
                item: Box::new(RunItem::Picture(picture)),
                frame,
                transform: None,
            });
            continue;
        }
        let mut shape = Shape {
            width: frame[1][0],
            height: frame[1][1],
            ..Shape::default()
        };
        apply_geometry(&mut shape, child, &component, frame[0]);
        if let Some(group) = list_groups(&child.children).into_iter().next() {
            read_text_box(&mut shape, &group, context);
        }
        out.push(ShapeChild {
            item: Box::new(RunItem::Shape(shape)),
            frame,
            transform: None,
        });
    }
    context.state.leave();
    out
}

pub(super) fn equation_info(data: &[u8]) -> Option<Equation> {
    let mut reader = BinReader::new(data);
    if reader.remaining() < 6 {
        return None;
    }
    let _ = reader.u32();
    let length = usize::from(reader.u16());
    if reader.remaining() < length.saturating_mul(2) {
        return None;
    }
    let mut equation = Equation {
        script: reader.utf16(length),
        ..Equation::default()
    };
    if reader.remaining() >= 4 {
        equation.size = HwpUnit::from(reader.u32());
    }
    if reader.remaining() >= 4 {
        equation.color = rgb(reader.u32());
    }
    if reader.remaining() >= 2 {
        equation.baseline = HwpUnit::from(reader.i16());
    }
    if reader.remaining() >= 2 {
        let version_length = usize::from(reader.u16());
        if reader.remaining() >= version_length.saturating_mul(2) {
            let _ = reader.utf16(version_length);
        }
    }
    if reader.remaining() >= 2 {
        let font_length = usize::from(reader.u16());
        if reader.remaining() >= font_length.saturating_mul(2) {
            equation.font = reader.utf16(font_length);
        }
    }
    if equation.size <= 0.0 || equation.size >= 100_000.0 {
        equation.size = 1000.0;
    }
    Some(equation)
}
