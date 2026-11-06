use crate::model::{Arc, Paragraph, Point, Rect, RunItem, Shape, ShapeChild, Transform};

#[path = "shape_fill.rs"]
mod fill;

use super::dom::Node;
use super::header::color;
use super::section::{parse_caption, parse_margins, parse_picture, parse_pos};

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
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn from_node(node: &Node) -> Self {
        Self {
            a: node.num("e1").unwrap_or(1.0),
            b: node.num("e4").unwrap_or(0.0),
            c: node.num("e2").unwrap_or(0.0),
            d: node.num("e5").unwrap_or(1.0),
            tx: node.num("e3").unwrap_or(0.0),
            ty: node.num("e6").unwrap_or(0.0),
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

    fn to_model(self) -> Transform {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
    }
}

pub(super) trait ShapeContext {
    fn paragraph(&mut self, node: &Node) -> Paragraph;
    fn enter(&mut self) -> bool;
    fn leave(&mut self);
}

pub(super) fn parse_shape<C>(node: &Node, context: &mut C) -> Shape
where
    C: ShapeContext,
{
    if !context.enter() {
        return Shape::default();
    }
    let shape = parse_shape_inner(node, context);
    context.leave();
    shape
}

fn parse_shape_inner<C>(node: &Node, context: &mut C) -> Shape
where
    C: ShapeContext,
{
    let mut shape = Shape {
        kind: match node.name.as_str() {
            "ole" | "chart" => "ole".to_string(),
            "connectLine" => "line".to_string(),
            _ => node.name.clone(),
        },
        ..Shape::default()
    };
    if let Some(size) = node.first("sz") {
        shape.width = size.num("width").unwrap_or(0.0);
        shape.height = size.num("height").unwrap_or(0.0);
    }
    // 한글은 글이 넘쳐 늘어난 글상자처럼 지정 크기(sz)와 현재 크기(curSz)가 다르면 현재 크기로 그린다(HWP 5와 같다).
    // 한 방향만 늘면 다른 쪽은 0으로 남긴다(`curSz width="0" height="6366"`). 0인 쪽은 지정 크기를 쓴다.
    if let Some(current) = node.first("curSz") {
        let width = current.num("width").unwrap_or(0.0);
        let height = current.num("height").unwrap_or(0.0);
        if width > 0.0 && width.is_finite() {
            shape.width = width;
        }
        if height > 0.0 && height.is_finite() {
            shape.height = height;
        }
    }
    shape.pos = parse_pos(node);
    shape.out_margins = parse_margins(node.first("outMargin"));
    shape.z_order = crate::hwpx::narrow(node.int("zOrder").unwrap_or(0));
    if let Some(text_wrap) = node.attr("textWrap") {
        shape.pos.text_wrap = text_wrap.to_string();
    }
    if let Some(line) = node.first("lineShape") {
        shape.line_color = color(line.attr("color")).unwrap_or(0);
        shape.line_width = line
            .num("width")
            .map(|value| value / 7200.0 * 25.4)
            .unwrap_or(0.12);
        shape.line_type = line.attr("style").unwrap_or("SOLID").to_string();
        shape.arrow_start = fill::arrow(line, "headStyle", "headSz", "headfill");
        shape.arrow_end = fill::arrow(line, "tailStyle", "tailSz", "tailfill");
    }
    if let Some(brush) = node.find("winBrush") {
        shape.fill_color = color(brush.attr("faceColor"));
    }
    fill::read(node, &mut shape);
    if let Some(rotation) = node.first("rotationInfo") {
        shape.rotation = rotation.num("angle").unwrap_or(0.0);
    }
    if let Some(draw_text) = node.first("drawText") {
        if let Some(list) = draw_text.first("subList") {
            shape.text_margins = parse_margins(draw_text.first("textMargin"));
            shape.text_vert_align = list.attr("vertAlign").unwrap_or("TOP").to_string();
            shape.paragraphs = list
                .all("p")
                .map(|paragraph| context.paragraph(paragraph))
                .collect();
        }
    }
    shape.caption = parse_caption(node.first("caption"), &mut |paragraph| {
        context.paragraph(paragraph)
    });

    let matrix = rendering_matrix(node).unwrap_or(Affine::IDENTITY);
    let point =
        |node: &Node| matrix.apply([node.num("x").unwrap_or(0.0), node.num("y").unwrap_or(0.0)]);
    let parts: Vec<&Node> = node
        .children
        .iter()
        .filter(|child| matches!(child.name.as_str(), "startPt" | "endPt" | "pt"))
        .collect();
    match node.name.as_str() {
        "line" | "connectLine" => {
            let start = parts.iter().copied().find(|part| part.name == "startPt");
            let end = parts.iter().copied().find(|part| part.name == "endPt");
            if let (Some(start), Some(end)) = (start, end) {
                shape.points = vec![point(start), point(end)];
            }
        }
        "polygon" => {
            shape.points = parts
                .iter()
                .copied()
                .filter(|part| part.name == "pt")
                .map(point)
                .collect();
        }
        "curve" => {
            let segments: Vec<&Node> = node
                .children
                .iter()
                .filter(|child| child.name == "seg")
                .collect();
            if let Some(first) = segments.first() {
                shape.points = std::iter::once([
                    first.num("x1").unwrap_or(0.0),
                    first.num("y1").unwrap_or(0.0),
                ])
                .chain(segments.iter().map(|segment| {
                    [
                        segment.num("x2").unwrap_or(0.0),
                        segment.num("y2").unwrap_or(0.0),
                    ]
                }))
                .map(|point| matrix.apply(point))
                .collect();
                shape.segment_types = segments
                    .iter()
                    .map(|segment| segment.attr("type").unwrap_or("LINE").to_string())
                    .collect();
            }
        }
        "arc" => {
            if let (Some(center), Some(axis1), Some(axis2)) =
                (node.first("center"), node.first("ax1"), node.first("ax2"))
            {
                shape.arc = Some(Arc {
                    kind: node.attr("type").unwrap_or("NORMAL").to_string(),
                    center: point(center),
                    axis1: point(axis1),
                    axis2: point(axis2),
                });
            }
        }
        _ => {}
    }
    if node.name == "container" {
        shape.line_type = "NONE".to_string();
        shape.children = container_children(node, context, [0.0, 0.0]);
    }
    shape
}

fn container_children<C>(node: &Node, context: &mut C, origin: Point) -> Vec<ShapeChild>
where
    C: ShapeContext,
{
    let mut children = Vec::new();
    for child in &node.children {
        match child.name.as_str() {
            "container" => {
                let offset = child.first("offset");
                let inner = [
                    origin[0] + offset.map_or(0.0, |node| node.num("x").unwrap_or(0.0)),
                    origin[1] + offset.map_or(0.0, |node| node.num("y").unwrap_or(0.0)),
                ];
                if !context.enter() {
                    continue;
                }
                children.extend(container_children(child, context, inner));
                context.leave();
            }
            "pic" => {
                let Some(frame) = child_frame(child, origin) else {
                    continue;
                };
                let mut picture =
                    parse_picture(child, &mut |paragraph| context.paragraph(paragraph));
                if let Some((matrix, size)) = turning_matrix(child) {
                    picture.width = size[0];
                    picture.height = size[1];
                    children.push(ShapeChild {
                        item: Box::new(RunItem::Picture(picture)),
                        frame,
                        transform: Some(matrix.to_model()),
                    });
                    continue;
                }
                picture.width = frame[1][0];
                picture.height = frame[1][1];
                children.push(ShapeChild {
                    item: Box::new(RunItem::Picture(picture)),
                    frame,
                    transform: None,
                });
            }
            "rect" | "ellipse" | "textart" | "ole" | "chart" => {
                let Some(frame) = child_frame(child, origin) else {
                    continue;
                };
                let mut shape = parse_shape(child, context);
                if let Some((matrix, size)) = turning_matrix(child) {
                    shape.width = size[0];
                    shape.height = size[1];
                    children.push(ShapeChild {
                        item: Box::new(RunItem::Shape(shape)),
                        frame,
                        transform: Some(matrix.to_model()),
                    });
                    continue;
                }
                shape.width = frame[1][0];
                shape.height = frame[1][1];
                children.push(ShapeChild {
                    item: Box::new(RunItem::Shape(shape)),
                    frame,
                    transform: None,
                });
            }
            "line" | "connectLine" | "polygon" | "curve" | "arc" => {
                let Some(frame) = child_frame(child, origin) else {
                    continue;
                };
                let mut shape = parse_shape(child, context);
                shape.width = frame[1][0];
                shape.height = frame[1][1];
                if rendering_matrix(child).is_some() {
                    shape.points = shape
                        .points
                        .into_iter()
                        .map(|point| [point[0] - frame[0][0], point[1] - frame[0][1]])
                        .collect();
                    if let Some(arc) = &mut shape.arc {
                        for point in [&mut arc.center, &mut arc.axis1, &mut arc.axis2] {
                            point[0] -= frame[0][0];
                            point[1] -= frame[0][1];
                        }
                    }
                }
                children.push(ShapeChild {
                    item: Box::new(RunItem::Shape(shape)),
                    frame,
                    transform: None,
                });
            }
            _ => {}
        }
    }
    children
}

fn turning_matrix(node: &Node) -> Option<(Affine, Point)> {
    let matrix = rendering_matrix(node)?;
    let original = node.first("orgSz")?;
    let size = [
        original.num("width").unwrap_or(0.0),
        original.num("height").unwrap_or(0.0),
    ];
    let keeps_axes = matrix.b == 0.0 && matrix.c == 0.0 && matrix.a > 0.0 && matrix.d > 0.0;
    if keeps_axes || size[0] <= 0.0 || size[1] <= 0.0 {
        return None;
    }
    if ![matrix.a, matrix.b, matrix.c, matrix.d, matrix.tx, matrix.ty]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    Some((matrix, size))
}

fn child_frame(node: &Node, origin: Point) -> Option<Rect> {
    let frame = match (rendering_matrix(node), node.first("orgSz")) {
        (Some(matrix), Some(original)) => transformed_frame(
            original.num("width").unwrap_or(0.0),
            original.num("height").unwrap_or(0.0),
            matrix,
        ),
        _ => {
            let offset = node.first("offset");
            let current = node.first("curSz").or_else(|| match node.name.as_str() {
                "ole" | "chart" | "connectLine" => node.first("sz"),
                _ => None,
            });
            [
                [
                    origin[0] + offset.map_or(0.0, |node| node.num("x").unwrap_or(0.0)),
                    origin[1] + offset.map_or(0.0, |node| node.num("y").unwrap_or(0.0)),
                ],
                [
                    current.map_or(0.0, |node| node.num("width").unwrap_or(0.0)),
                    current.map_or(0.0, |node| node.num("height").unwrap_or(0.0)),
                ],
            ]
        }
    };
    if frame.iter().flatten().all(|value| value.is_finite()) {
        Some(frame)
    } else {
        None
    }
}

fn transformed_frame(width: f64, height: f64, matrix: Affine) -> Rect {
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

fn rendering_matrix(node: &Node) -> Option<Affine> {
    let info = node.first("renderingInfo")?;
    let mut transform: Option<Affine> = None;
    let mut scale: Option<Affine> = None;
    for child in &info.children {
        match child.name.as_str() {
            "transMatrix" => transform = Some(Affine::from_node(child)),
            "scaMatrix" => scale = Some(Affine::from_node(child)),
            "rotMatrix" => {
                if let (Some(base), Some(scale)) = (transform, scale) {
                    transform = Some(
                        scale
                            .concatenating(Affine::from_node(child))
                            .concatenating(base),
                    );
                }
                scale = None;
            }
            _ => {}
        }
    }
    transform.filter(|matrix| matrix.is_usable())
}
