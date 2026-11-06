use serde::Serialize;

use super::Margins;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageBorderFill {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "borderFillID")]
    pub border_fill_id: i32,
    pub text_border: String,
    pub header_inside: bool,
    pub footer_inside: bool,
    pub fill_area: String,
    pub offsets: Margins,
}

impl Default for PageBorderFill {
    fn default() -> Self {
        Self {
            kind: "BOTH".to_string(),
            border_fill_id: 0,
            text_border: "PAPER".to_string(),
            header_inside: false,
            footer_inside: false,
            fill_area: "PAPER".to_string(),
            offsets: Margins::default(),
        }
    }
}
