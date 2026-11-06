use crate::model::{ColumnDef, HwpUnit};

use super::control::rgb;
use super::docinfo::SHAPE_LINE_TYPES;
use super::reader::BinReader;
use super::record::Record;

const BORDER_WIDTHS: [HwpUnit; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];

pub(super) fn parse_columns(record: &Record) -> Option<ColumnDef> {
    let mut reader = record.reader();
    if reader.u32() != u32::from_be_bytes(*b"cold") {
        return None;
    }
    read_columns(&mut reader)
}

pub(super) fn read_columns(reader: &mut BinReader<'_>) -> Option<ColumnDef> {
    if reader.remaining() < 4 {
        return None;
    }
    let attr = reader.u16();
    let count = (attr >> 2) & 0xFF;
    if count == 0 {
        return None;
    }
    let mut columns = ColumnDef {
        count: i32::from(count),
        gap: HwpUnit::from(reader.u16()),
        same_width: attr & 0x1000 != 0,
        direction: match (attr >> 10) & 3 {
            0 => "LEFT",
            1 => "RIGHT",
            2 => "MIRROR",
            _ => "UNKNOWN",
        }
        .to_string(),
        ..ColumnDef::default()
    };
    if !columns.same_width {
        if reader.remaining() < usize::from(count) * 2 + 8 {
            return Some(columns);
        }
        columns.widths = (0..count).map(|_| HwpUnit::from(reader.u16())).collect();
    }
    if reader.remaining() >= 8 {
        let _ = reader.u16();
        columns.line_type = SHAPE_LINE_TYPES
            .get(usize::from(reader.u8()))
            .unwrap_or(&"NONE")
            .to_string();
        columns.line_width = BORDER_WIDTHS[usize::from(reader.u8()).min(15)];
        columns.line_color = rgb(reader.u32());
    }
    Some(columns)
}
