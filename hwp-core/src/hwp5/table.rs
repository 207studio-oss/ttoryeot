use crate::model::{Caption, Cell, Margins, Table};
use crate::HwpUnit;

use super::control::{list_attr, list_groups, parse_nested, Common, ParseContext};
use super::record::Record;
use super::tags::T_TABLE;

const PAGE_BREAK_MODES: [&str; 4] = ["NONE", "TABLE", "CELL", "CELL"];
const VERT_ALIGNS: [&str; 4] = ["TOP", "CENTER", "BOTTOM", "TOP"];

pub(super) fn parse_table(
    table_record: &Record,
    following: &[Record],
    common: &Common,
    caption: Option<Caption>,
    context: &mut ParseContext<'_>,
) -> Table {
    let mut reader = table_record.reader();
    let attr = reader.u32();
    let rows = i32::from(reader.u16());
    let cols = i32::from(reader.u16());
    let cell_spacing = HwpUnit::from(reader.u16());
    let in_margins = Margins {
        left: HwpUnit::from(reader.i16()),
        right: HwpUnit::from(reader.i16()),
        top: HwpUnit::from(reader.i16()),
        bottom: HwpUnit::from(reader.i16()),
    };
    for _ in 0..rows {
        let _ = reader.u16();
    }
    let border_fill_id = i32::from(reader.u16());
    let mut table = Table {
        rows,
        cols,
        width: common.width,
        height: common.height,
        pos: common.pos.clone(),
        out_margins: common.margins.clone(),
        in_margins,
        border_fill_id,
        cells: Vec::new(),
        cell_spacing,
        z_order: common.z_order,
        repeat_header: attr & 4 != 0,
        page_break_mode: PAGE_BREAK_MODES[(attr & 3) as usize].to_string(),
        caption,
    };

    for group in list_groups(following) {
        let attr = list_attr(group.header);
        let mut reader = group.header.reader();
        let _ = reader.i16();
        let _ = reader.u16();
        let _ = reader.u32();
        let col = i32::from(reader.u16());
        let row = i32::from(reader.u16());
        let col_span = i32::from(reader.u16());
        let row_span = i32::from(reader.u16());
        let width = HwpUnit::from(reader.u32());
        let height = HwpUnit::from(reader.u32());
        let margins = Margins {
            left: HwpUnit::from(reader.i16()),
            right: HwpUnit::from(reader.i16()),
            top: HwpUnit::from(reader.i16()),
            bottom: HwpUnit::from(reader.i16()),
        };
        let has_margin = attr & (1 << 16) != 0;
        let mut cell = Cell {
            col,
            row,
            col_span,
            row_span,
            width,
            height,
            border_fill_id: i32::from(reader.u16()),
            margins: if has_margin {
                margins
            } else {
                table.in_margins.clone()
            },
            has_margin,
            vert_align: VERT_ALIGNS[((attr >> 5) & 3) as usize].to_string(),
            header: attr & (1 << 18) != 0,
            ..Cell::default()
        };
        cell.paragraphs = parse_nested(&group.paragraphs, context);
        table.cells.push(cell);
    }
    normalize_grid(&mut table);
    table
}

pub(super) fn table_record(children: &[Record]) -> Option<usize> {
    children.iter().position(|child| child.tag == T_TABLE)
}

pub(crate) fn normalize_grid(table: &mut Table) {
    table
        .cells
        .retain(|cell| cell.col >= 0 && cell.row >= 0 && cell.col <= 4095 && cell.row <= 65535);
    for cell in &mut table.cells {
        cell.col_span = cell.col_span.clamp(1, 4096);
        cell.row_span = cell.row_span.clamp(1, 65536);
    }
    let max_col = table
        .cells
        .iter()
        .map(|cell| cell.col + cell.col_span)
        .max()
        .unwrap_or(0);
    let max_row = table
        .cells
        .iter()
        .map(|cell| cell.row + cell.row_span)
        .max()
        .unwrap_or(0);
    table.cols = table.cols.max(max_col).min(8192);
    table.rows = table.rows.max(max_row).min(131_072);
}

#[cfg(test)]
mod tests {
    use super::normalize_grid;
    use crate::model::{Cell, Table};

    #[test]
    fn normalize_grid_matches_swift_bounds() {
        let mut table = Table {
            rows: 1,
            cols: 1,
            cells: vec![
                Cell {
                    col: 3,
                    row: 4,
                    col_span: 0,
                    row_span: 0,
                    ..Cell::default()
                },
                Cell {
                    col: -1,
                    ..Cell::default()
                },
                Cell {
                    col: 5000,
                    ..Cell::default()
                },
            ],
            ..Table::default()
        };
        normalize_grid(&mut table);
        assert_eq!(table.cells.len(), 1);
        assert_eq!(table.cells[0].col_span, 1);
        assert_eq!(table.cells[0].row_span, 1);
        assert_eq!(table.cols, 4);
        assert_eq!(table.rows, 5);
    }
}
