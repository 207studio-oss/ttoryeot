use crate::model::{Arrow, Gradient, HwpUnit, ImageFill};

use super::super::control::rgb;
use super::super::reader::BinReader;

pub(super) fn arrow(style: u32, size: u32, fill: bool) -> Option<Arrow> {
    let style = match style {
        1 => "ARROW",
        2 => "SPEAR",
        3 => "CONCAVE_ARROW",
        4 => "EMPTY_DIAMOND",
        5 => "EMPTY_CIRCLE",
        6 => "EMPTY_BOX",
        _ => return None,
    };
    let sizes = [
        "SMALL_SMALL",
        "SMALL_MEDIUM",
        "SMALL_LARGE",
        "MEDIUM_SMALL",
        "MEDIUM_MEDIUM",
        "MEDIUM_LARGE",
        "LARGE_SMALL",
        "LARGE_MEDIUM",
        "LARGE_LARGE",
    ];
    Some(Arrow {
        style: style.to_string(),
        size: sizes
            .get(size as usize)
            .unwrap_or(&"MEDIUM_MEDIUM")
            .to_string(),
        fill,
    })
}

pub(in crate::hwp5) fn gradient(reader: &mut BinReader<'_>) -> Option<Gradient> {
    if reader.remaining() < 21 {
        reader.skip(reader.remaining());
        return None;
    }
    let kind = match reader.u8() {
        2 => "RADIAL",
        3 => "CONICAL",
        4 => "SQUARE",
        _ => "LINEAR",
    }
    .to_string();
    let angle = HwpUnit::from(reader.i32());
    let center_x = HwpUnit::from(reader.i32());
    let center_y = HwpUnit::from(reader.i32());
    let step = reader.i32();
    let count = reader.i32();
    let stride = if count > 2 { 8 } else { 4 };
    if count < 0 || count as usize > reader.remaining() / stride {
        reader.skip(reader.remaining());
        return None;
    }
    let positions = if count > 2 {
        (0..count).map(|_| HwpUnit::from(reader.i32())).collect()
    } else {
        Vec::new()
    };
    let colors = (0..count).map(|_| rgb(reader.u32())).collect();
    Some(Gradient {
        kind,
        angle,
        center_x,
        center_y,
        step,
        step_center: 50,
        colors,
        positions,
    })
}

pub(super) fn image(reader: &mut BinReader<'_>) -> Option<ImageFill> {
    if reader.remaining() < 6 {
        reader.skip(reader.remaining());
        return None;
    }
    let modes = [
        "TILE",
        "TILE_HORZ_TOP",
        "TILE_HORZ_BOTTOM",
        "TILE_VERT_LEFT",
        "TILE_VERT_RIGHT",
        "TOTAL",
        "CENTER",
        "CENTER_TOP",
        "CENTER_BOTTOM",
        "LEFT_CENTER",
        "LEFT_TOP",
        "LEFT_BOTTOM",
        "RIGHT_CENTER",
        "RIGHT_TOP",
        "RIGHT_BOTTOM",
        "ZOOM",
    ];
    let mode = modes
        .get(usize::from(reader.u8()))
        .unwrap_or(&"TOTAL")
        .to_string();
    let bright = i32::from(reader.i8());
    let contrast = i32::from(reader.i8());
    let effect = match reader.u8() {
        1 => "GRAY_SCALE",
        2 => "BLACK_WHITE",
        3 => "PATTERN8x8",
        _ => "REAL_PIC",
    }
    .to_string();
    let bin = reader.u16();
    (bin != 0).then(|| ImageFill {
        mode,
        binary_id: format!("bin{bin}"),
        bright,
        contrast,
        effect,
    })
}
