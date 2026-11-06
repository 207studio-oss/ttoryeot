use super::super::record::Record;
use super::super::tags::{
    T_SC_ARC, T_SC_CURVE, T_SC_ELLIPSE, T_SC_LINE, T_SC_OLE, T_SC_POLYGON, T_SC_RECT,
};
use super::ShapeComponent;
use crate::model::{Arc, HwpUnit, Point, Shape};
pub(in crate::hwp5) fn apply_geometry(
    shape: &mut Shape,
    component_record: &Record,
    component: &ShapeComponent,
    origin: Point,
) {
    shape.line_type = component.line_type.to_string();
    shape.line_color = component.line_color;
    shape.line_width = component.line_width;
    shape.fill_color = component.fill_color;
    shape.fill_alpha = component.fill_alpha;
    shape.arrow_start = component.arrow_start.clone();
    shape.arrow_end = component.arrow_end.clone();
    shape.gradient = component.gradient.clone();
    shape.image_fill = component.image_fill.clone();
    let local = |x: i32, y: i32| {
        let point = component.matrix.apply([f64::from(x), f64::from(y)]);
        [point[0] - origin[0], point[1] - origin[1]]
    };
    if let Some(line) = component_record
        .children
        .iter()
        .find(|child| child.tag == T_SC_LINE)
    {
        shape.kind = "line".to_string();
        let mut reader = line.reader();
        let x1 = reader.i32();
        let y1 = reader.i32();
        let x2 = reader.i32();
        let y2 = reader.i32();
        shape.points = vec![local(x1, y1), local(x2, y2)];
    } else if let Some(polygon) = component_record
        .children
        .iter()
        .find(|child| child.tag == T_SC_POLYGON || child.tag == T_SC_CURVE)
    {
        shape.kind = if polygon.tag == T_SC_POLYGON {
            "polygon".to_string()
        } else {
            "curve".to_string()
        };
        let mut reader = polygon.reader();
        let count = reader.i32();
        let available = polygon.data.len().saturating_sub(4) / 8;
        let count = if count <= 0 {
            0
        } else {
            (count as usize).min(available)
        };
        shape.points = (0..count)
            .map(|_| {
                let x = reader.i32();
                let y = reader.i32();
                local(x, y)
            })
            .collect();
        if polygon.tag == T_SC_CURVE {
            shape.segment_types = (0..count.saturating_sub(1))
                .map(|_| if reader.u8() == 1 { "CURVE" } else { "LINE" }.to_string())
                .collect();
        }
    } else if component_record
        .children
        .iter()
        .any(|child| child.tag == T_SC_ELLIPSE)
    {
        shape.kind = "ellipse".to_string();
    } else if let Some(arc) = component_record
        .children
        .iter()
        .find(|child| child.tag == T_SC_ARC)
    {
        shape.kind = "arc".to_string();
        if arc.data.len() >= 25 {
            let mut reader = arc.reader();
            let kind = match reader.u8() {
                1 => "PIE",
                2 => "CHORD",
                _ => "NORMAL",
            }
            .to_string();
            shape.arc = Some(Arc {
                kind,
                center: local(reader.i32(), reader.i32()),
                axis1: local(reader.i32(), reader.i32()),
                axis2: local(reader.i32(), reader.i32()),
            });
        }
    } else if component_record
        .children
        .iter()
        .any(|child| child.tag == T_SC_OLE)
    {
        shape.kind = "ole".to_string();
    } else {
        shape.kind = "rect".to_string();
        if let Some(rect) = component_record
            .children
            .iter()
            .find(|child| child.tag == T_SC_RECT)
        {
            let mut reader = rect.reader();
            shape.round_ratio = HwpUnit::from(reader.u8().min(50));
        }
    }
}
