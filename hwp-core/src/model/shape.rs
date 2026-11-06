use super::{
    fix, fix_point, Caption, Equation, HwpUnit, Margins, Paragraph, Point, Pos, ShapeChild,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrow {
    pub style: String,
    pub size: String,
    pub fill: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    pub kind: String,
    pub angle: HwpUnit,
    pub center_x: HwpUnit,
    pub center_y: HwpUnit,
    pub step: i32,
    pub step_center: i32,
    pub colors: Vec<u32>,
    pub positions: Vec<HwpUnit>,
}

impl Gradient {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.angle, 0.0);
        fix(&mut self.center_x, 0.0);
        fix(&mut self.center_y, 0.0);
        for position in &mut self.positions {
            fix(position, 0.0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFill {
    pub mode: String,
    #[serde(rename = "binaryID")]
    pub binary_id: String,
    pub bright: i32,
    pub contrast: i32,
    pub effect: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Arc {
    pub kind: String,
    pub center: Point,
    pub axis1: Point,
    pub axis2: Point,
}

impl Arc {
    pub(crate) fn fix_non_finite(&mut self) {
        fix_point(&mut self.center);
        fix_point(&mut self.axis1);
        fix_point(&mut self.axis2);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub kind: String,
    pub width: HwpUnit,
    pub height: HwpUnit,
    pub pos: Pos,
    pub out_margins: Margins,
    pub line_color: u32,
    pub line_width: HwpUnit,
    pub line_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_start: Option<Arrow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_end: Option<Arrow>,
    pub fill_color: Option<u32>,
    pub fill_alpha: HwpUnit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_fill: Option<ImageFill>,
    pub round_ratio: HwpUnit,
    pub z_order: i32,
    pub paragraphs: Vec<Paragraph>,
    pub text_margins: Margins,
    /// 글상자 글의 세로 정렬(TOP, CENTER, BOTTOM).
    #[serde(skip_serializing_if = "is_top")]
    pub text_vert_align: String,
    pub points: Vec<Point>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segment_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc: Option<Arc>,
    pub rotation: HwpUnit,
    pub equation: Option<Equation>,
    pub children: Vec<ShapeChild>,
    pub caption: Option<Caption>,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            kind: "rect".to_string(),
            width: 0.0,
            height: 0.0,
            pos: Pos::default(),
            out_margins: Margins::default(),
            line_color: 0,
            line_width: 0.12,
            line_type: "SOLID".to_string(),
            arrow_start: None,
            arrow_end: None,
            fill_color: None,
            fill_alpha: 1.0,
            gradient: None,
            image_fill: None,
            round_ratio: 0.0,
            z_order: 0,
            paragraphs: Vec::new(),
            text_margins: Margins::default(),
            text_vert_align: "TOP".to_string(),
            points: Vec::new(),
            segment_types: Vec::new(),
            arc: None,
            rotation: 0.0,
            equation: None,
            children: Vec::new(),
            caption: None,
        }
    }
}

impl Shape {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.width, 0.0);
        fix(&mut self.height, 0.0);
        fix(&mut self.line_width, 0.12);
        fix(&mut self.fill_alpha, 1.0);
        fix(&mut self.round_ratio, 0.0);
        fix(&mut self.rotation, 0.0);
        self.pos.fix_non_finite();
        self.out_margins.fix_non_finite();
        self.text_margins.fix_non_finite();
        for point in &mut self.points {
            fix_point(point);
        }
        if let Some(gradient) = &mut self.gradient {
            gradient.fix_non_finite();
        }
        if let Some(arc) = &mut self.arc {
            arc.fix_non_finite();
        }
        for paragraph in &mut self.paragraphs {
            paragraph.fix_non_finite();
        }
        if let Some(equation) = self.equation.as_mut() {
            equation.fix_non_finite();
        }
        for child in &mut self.children {
            child.fix_non_finite();
        }
        if let Some(caption) = self.caption.as_mut() {
            caption.fix_non_finite();
        }
    }
}

fn is_top(value: &str) -> bool {
    value == "TOP"
}
