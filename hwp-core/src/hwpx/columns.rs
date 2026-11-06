use crate::model::ColumnDef;

use super::dom::Node;
use super::header::color;
use super::narrow;

pub(super) fn parse_columns(node: &Node) -> ColumnDef {
    let mut columns = ColumnDef {
        count: narrow(node.int("colCount").unwrap_or(1)).max(1),
        gap: node.num("sameGap").unwrap_or(1134.0),
        same_width: node.attr("sameSz").is_none() || node.bool("sameSz"),
        direction: node.attr("layout").unwrap_or("LEFT").to_string(),
        ..ColumnDef::default()
    };
    if !columns.same_width {
        columns.widths = node
            .all("colSz")
            .map(|size| size.num("width").unwrap_or(0.0))
            .collect();
    }
    if let Some(line) = node.first("colLine") {
        columns.line_type = line.attr("type").unwrap_or("NONE").to_string();
        columns.line_width = line
            .attr("width")
            .and_then(|width| width.trim().trim_end_matches("mm").trim().parse().ok())
            .unwrap_or(0.1);
        columns.line_color = color(line.attr("color")).unwrap_or(0);
    }
    columns.fix_non_finite();
    columns
}
