use crate::model::{HwpUnit, Margins, PageBorderFill};
use crate::report::{Warning, WarningKind};

use super::record::Record;
use super::tags::T_PAGE_BORDER_FILL;

pub(super) fn parse_fills(
    records: &[Record],
    section: Option<u32>,
    warnings: &mut Vec<Warning>,
) -> Vec<PageBorderFill> {
    records
        .iter()
        .filter(|record| record.tag == T_PAGE_BORDER_FILL)
        .enumerate()
        .filter_map(|(index, record)| {
            if record.data.len() < 14 {
                warnings.push(Warning::new(
                    WarningKind::RecordTruncated,
                    section,
                    format!(
                        "PAGE_BORDER_FILL {index} needs 14 bytes, found {}",
                        record.data.len()
                    ),
                ));
                return None;
            }
            let mut reader = record.reader();
            let flags = reader.u32();
            let offsets = Margins {
                left: HwpUnit::from(reader.i16()),
                right: HwpUnit::from(reader.i16()),
                top: HwpUnit::from(reader.i16()),
                bottom: HwpUnit::from(reader.i16()),
            };
            Some(PageBorderFill {
                kind: ["BOTH", "EVEN", "ODD"]
                    .get(index)
                    .unwrap_or(&"UNKNOWN")
                    .to_string(),
                border_fill_id: i32::from(reader.u16()),
                text_border: if flags & 1 == 0 { "CONTENT" } else { "PAPER" }.to_string(),
                header_inside: flags & 2 != 0,
                footer_inside: flags & 4 != 0,
                fill_area: match (flags >> 3) & 3 {
                    0 => "PAPER",
                    1 => "PAGE",
                    2 => "BORDER",
                    _ => "UNKNOWN",
                }
                .to_string(),
                offsets,
            })
        })
        .collect()
}
