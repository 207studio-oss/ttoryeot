use serde::Serialize;

use crate::{Fatal, Warning};

use super::record::Record;
use super::tags::T_NUMBERING;
use super::{collect_streams, read_doc_info, DOC_INFO, FILE_HEADER};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberingRaw {
    pub numbering_id: u32,
    pub level: u8,
    pub attr: u32,
    pub code: u8,
}

pub fn numbering_raw(bytes: &[u8]) -> Result<Vec<NumberingRaw>, Fatal> {
    let mut file = super::container::open(bytes)?;
    let streams = collect_streams(&file);
    let header = streams
        .read(&mut file, FILE_HEADER)
        .filter(|stream| stream.data.len() >= 40)
        .ok_or(Fatal::NotHwp)?;
    let flags = super::reader::BinReader::at(&header.data, 36).u32();
    if flags & 2 != 0 {
        return Err(Fatal::PasswordProtected);
    }
    if !streams.lengths.contains_key(DOC_INFO) {
        return Err(Fatal::NoReadableSection);
    }
    let mut warnings: Vec<Warning> = Vec::new();
    let (records, truncated) = read_doc_info(&mut file, &streams, flags & 1 != 0, &mut warnings);
    if truncated {
        return Err(Fatal::CorruptDocument(
            "numbering DocInfo is not a whole DES block".into(),
        ));
    }
    if !warnings.is_empty() {
        return Err(Fatal::Internal(
            "numbering DocInfo is unreadable or truncated".into(),
        ));
    }
    let mut output = Vec::new();
    collect(&records, &mut 0, &mut output);
    Ok(output)
}

fn collect(records: &[Record], numbering_id: &mut u32, output: &mut Vec<NumberingRaw>) {
    for record in records {
        if record.tag == T_NUMBERING {
            *numbering_id = numbering_id.saturating_add(1);
            let mut reader = record.reader();
            for level in 1..=7 {
                if reader.remaining() < 14 {
                    break;
                }
                let attr = reader.u32();
                reader.skip(8);
                let count = usize::from(reader.u16());
                let size = count * 2;
                if reader.remaining() < size {
                    break;
                }
                reader.skip(size);
                output.push(NumberingRaw {
                    numbering_id: *numbering_id,
                    level,
                    attr,
                    code: ((attr >> 5) & 0x1F) as u8,
                });
            }
        }
        collect(&record.children, numbering_id, output);
    }
}
