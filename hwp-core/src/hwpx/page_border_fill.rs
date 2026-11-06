use crate::model::{Margins, PageBorderFill};

use super::dom::Node;
use super::narrow;

pub(super) fn parse_fill(node: &Node) -> PageBorderFill {
    let mut offsets = node
        .first("offset")
        .map(|offset| Margins {
            left: offset.num("left").unwrap_or(0.0),
            right: offset.num("right").unwrap_or(0.0),
            top: offset.num("top").unwrap_or(0.0),
            bottom: offset.num("bottom").unwrap_or(0.0),
        })
        .unwrap_or_default();
    offsets.fix_non_finite();
    PageBorderFill {
        kind: node.attr("type").unwrap_or("BOTH").to_string(),
        border_fill_id: narrow(node.int("borderFillIDRef").unwrap_or(0)),
        text_border: node.attr("textBorder").unwrap_or("PAPER").to_string(),
        header_inside: node.bool("headerInside"),
        footer_inside: node.bool("footerInside"),
        fill_area: node.attr("fillArea").unwrap_or("PAPER").to_string(),
        offsets,
    }
}
