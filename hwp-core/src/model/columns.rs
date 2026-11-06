use serde::Serialize;

use super::{fix, HwpUnit};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    pub count: i32,
    pub gap: HwpUnit,
    pub same_width: bool,
    pub widths: Vec<HwpUnit>,
    pub line_type: String,
    pub line_width: HwpUnit,
    pub line_color: u32,
    pub direction: String,
}

impl Default for ColumnDef {
    fn default() -> Self {
        Self {
            count: 1,
            gap: 1134.0,
            same_width: true,
            widths: Vec::new(),
            line_type: "NONE".to_string(),
            line_width: 0.1,
            line_color: 0,
            direction: "LEFT".to_string(),
        }
    }
}

impl ColumnDef {
    pub(crate) fn fix_non_finite(&mut self) {
        fix(&mut self.gap, 1134.0);
        fix(&mut self.line_width, 0.1);
        for width in &mut self.widths {
            fix(width, 0.0);
        }
    }
}
