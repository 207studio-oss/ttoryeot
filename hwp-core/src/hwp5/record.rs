use crate::report::{Warning, WarningKind};

use super::reader::BinReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub tag: u16,
    pub level: u16,
    pub data: Vec<u8>,
    pub children: Vec<Record>,
}

impl Record {
    pub fn reader(&self) -> BinReader<'_> {
        BinReader::new(&self.data)
    }

    pub fn parse_checked(data: &[u8], section: Option<u32>) -> (Vec<Record>, Vec<Warning>) {
        crate::hwp5::record::parse_checked(data, section)
    }

    pub fn parse(data: &[u8]) -> Vec<Record> {
        crate::hwp5::record::parse_checked(data, None).0
    }
}

pub fn parse_checked(data: &[u8], section: Option<u32>) -> (Vec<Record>, Vec<Warning>) {
    let mut flat = Vec::new();
    let mut warnings = Vec::new();
    let mut reader = BinReader::new(data);
    while reader.remaining() > 0 {
        if reader.remaining() < 4 {
            warnings.push(Warning::new(
                WarningKind::RecordTruncated,
                section,
                format!("record header has {} trailing byte(s)", reader.remaining()),
            ));
            break;
        }
        let header = reader.u32();
        let tag = (header & 0x3FF) as u16;
        let level = ((header >> 10) & 0x3FF) as u16;
        let mut size = ((header >> 20) & 0xFFF) as usize;
        if size == 0xFFF {
            if reader.remaining() < 4 {
                warnings.push(Warning::new(
                    WarningKind::RecordTruncated,
                    section,
                    format!("record {tag} extended size is missing"),
                ));
                break;
            }
            size = reader.u32() as usize;
        }
        if size > reader.remaining() {
            let available = reader.remaining();
            flat.push(Record {
                tag,
                level,
                data: reader.bytes(available).to_vec(),
                children: Vec::new(),
            });
            warnings.push(Warning::new(
                WarningKind::RecordTruncated,
                section,
                format!("record {tag} needs {size} byte(s), found {available}"),
            ));
            break;
        }
        flat.push(Record {
            tag,
            level,
            data: reader.bytes(size).to_vec(),
            children: Vec::new(),
        });
    }
    (build_tree(flat), warnings)
}

fn build_tree(flat: Vec<Record>) -> Vec<Record> {
    let mut roots = Vec::new();
    let mut stack: Vec<Record> = Vec::new();
    for record in flat {
        while stack
            .last()
            .is_some_and(|parent| parent.level >= record.level)
        {
            if let Some(completed) = stack.pop() {
                attach(&mut roots, &mut stack, completed);
            }
        }
        stack.push(record);
    }
    while let Some(completed) = stack.pop() {
        attach(&mut roots, &mut stack, completed);
    }
    roots
}

fn attach(roots: &mut Vec<Record>, stack: &mut [Record], record: Record) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(record);
    } else {
        roots.push(record);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_checked;
    use crate::hwp5::tags;
    use crate::report::WarningKind;

    fn record(tag: u16, level: u16, data: &[u8]) -> Vec<u8> {
        let size = data.len() as u32;
        let mut bytes = (u32::from(tag) | (u32::from(level) << 10) | (size << 20))
            .to_le_bytes()
            .to_vec();
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn builds_parent_by_level_stack() {
        let mut bytes = record(66, 0, &[0]);
        bytes.extend(record(67, 1, &[1]));
        bytes.extend(record(72, 1, &[2]));
        bytes.extend(record(66, 2, &[3]));
        bytes.extend(record(66, 1, &[4]));
        let (roots, warnings) = parse_checked(&bytes, None);
        assert!(warnings.is_empty());
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].level, 0);
        assert_eq!(roots[0].children.len(), 3);
        assert_eq!(roots[0].children[2].tag, 66);
        assert_eq!(roots[0].children[1].children[0].tag, 66);
    }

    #[test]
    fn truncated_body_keeps_prefix() {
        let mut bytes = record(66, 0, &[1, 2, 3]);
        bytes.extend((u32::from(tags::T_PARA_TEXT) | (10 << 20)).to_le_bytes());
        bytes.extend([7, 8, 9]);
        let (records, warnings) = parse_checked(&bytes, Some(0));
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].tag, tags::T_PARA_TEXT);
        assert_eq!(records[1].data, [7, 8, 9]);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, WarningKind::RecordTruncated);
        assert_eq!(warnings[0].section, Some(0));
    }
}
