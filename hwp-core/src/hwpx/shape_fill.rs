use crate::model::{Arrow, ImageFill, Shape};

use super::super::dom::Node;
use super::super::header::gradation_of;

pub(super) fn arrow(line: &Node, style: &str, size: &str, fill: &str) -> Option<Arrow> {
    let style = line.attr(style).unwrap_or("NORMAL");
    (style != "NORMAL").then(|| Arrow {
        style: style.to_string(),
        size: line.attr(size).unwrap_or("MEDIUM_MEDIUM").to_string(),
        fill: line.attr(fill) == Some("1"),
    })
}

pub(super) fn read(node: &Node, shape: &mut Shape) {
    let Some(brush) = node.first("fillBrush") else {
        return;
    };
    if let Some(gradient) = brush.first("gradation") {
        shape.gradient = Some(gradation_of(gradient));
    }
    if let Some(image) = brush.first("imgBrush") {
        shape.image_fill = image.first("img").and_then(|img| {
            let binary_id = img.attr("binaryItemIDRef").filter(|id| !id.is_empty())?;
            Some(ImageFill {
                mode: image.attr("mode").unwrap_or("TOTAL").to_string(),
                binary_id: binary_id.to_string(),
                bright: crate::hwpx::narrow(img.int("bright").unwrap_or(0)),
                contrast: crate::hwpx::narrow(img.int("contrast").unwrap_or(0)),
                effect: img.attr("effect").unwrap_or("REAL_PIC").to_string(),
            })
        });
    }
}
