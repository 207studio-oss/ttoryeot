use std::cell::Cell;
use std::io::Read;

const EOCD_MAGIC: u32 = 0x0605_4b50;
const CENTRAL_MAGIC: u32 = 0x0201_4b50;
const LOCAL_MAGIC: u32 = 0x0403_4b50;
const EOCD_MIN_SIZE: usize = 22;
const MAX_SEARCH: usize = 66_000;
const MAX_ENTRY_SIZE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_DECODE: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    method: u16,
    comp_size: usize,
    uncomp_size: usize,
    local_offset: usize,
    invalid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipError {
    Malformed,
    TooLarge,
}

#[derive(Debug, Clone)]
pub struct MiniZip<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
    decoded: Cell<u64>,
    budget: u64,
}

impl<'a> MiniZip<'a> {
    pub fn new(data: &'a [u8]) -> Option<Self> {
        Self::with_budget(data, MAX_TOTAL_DECODE)
    }

    pub fn with_budget(data: &'a [u8], budget: u64) -> Option<Self> {
        if data.len() <= EOCD_MIN_SIZE {
            return None;
        }
        let lowest = data.len().saturating_sub(MAX_SEARCH);
        let mut eocd = None;
        for offset in (lowest..=data.len() - EOCD_MIN_SIZE).rev() {
            if read_u32(data, offset) == Some(EOCD_MAGIC) {
                eocd = Some(offset);
                break;
            }
        }
        let eocd = eocd?;
        let count = usize::from(read_u16(data, eocd + 10)?);
        if read_u16(data, eocd + 8)? != count as u16 {
            return None;
        }
        let directory_offset = read_u32(data, eocd + 16)? as usize;
        let directory_size = read_u32(data, eocd + 12)? as usize;
        let directory_end = directory_offset.checked_add(directory_size)?;
        if directory_end > eocd || directory_end > data.len() {
            return None;
        }

        let mut entries = Vec::with_capacity(count.min(4096));
        let mut position = directory_offset;
        for _ in 0..count {
            if position.checked_add(46)? > directory_end {
                return None;
            }
            if read_u32(data, position)? != CENTRAL_MAGIC {
                return None;
            }
            let method = read_u16(data, position + 10)?;
            let comp_size = read_u32(data, position + 20)? as usize;
            let uncomp_size = read_u32(data, position + 24)? as usize;
            let name_length = usize::from(read_u16(data, position + 28)?);
            let extra_length = usize::from(read_u16(data, position + 30)?);
            let comment_length = usize::from(read_u16(data, position + 32)?);
            let local_offset = read_u32(data, position + 42)? as usize;
            let name_start = position.checked_add(46)?;
            let name_end = name_start.checked_add(name_length)?;
            let next = name_end
                .checked_add(extra_length)?
                .checked_add(comment_length)?;
            if next > directory_end {
                return None;
            }
            let name = std::str::from_utf8(&data[name_start..name_end])
                .unwrap_or("")
                .to_string();
            entries.push(Entry {
                name,
                method,
                comp_size,
                uncomp_size,
                local_offset,
                invalid: false,
            });
            position = next;
        }
        if position != directory_end {
            return None;
        }
        mark_invalid_entries(data, &mut entries);
        Some(Self {
            data,
            entries,
            decoded: Cell::new(0),
            budget,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .and_then(|entry| self.read_entry(entry).ok())
    }

    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>, ZipError> {
        if entry.invalid {
            return Err(ZipError::Malformed);
        }
        if entry.uncomp_size as u64 > MAX_ENTRY_SIZE {
            return Err(ZipError::TooLarge);
        }
        if entry.method == 8 && entry.uncomp_size == 0 {
            return Err(ZipError::Malformed);
        }
        let remaining = self.budget.saturating_sub(self.decoded.get());
        if remaining == 0 || entry.uncomp_size as u64 > remaining {
            return Err(ZipError::TooLarge);
        }
        let offset = entry.local_offset;
        let local_end = offset.checked_add(30).ok_or(ZipError::Malformed)?;
        if local_end > self.data.len() {
            return Err(ZipError::Malformed);
        }
        if read_u32(self.data, offset) != Some(LOCAL_MAGIC) {
            return Err(ZipError::Malformed);
        }
        if read_u16(self.data, offset + 8) != Some(entry.method) {
            return Err(ZipError::Malformed);
        }
        let name_length = usize::from(read_u16(self.data, offset + 26).ok_or(ZipError::Malformed)?);
        let extra_length =
            usize::from(read_u16(self.data, offset + 28).ok_or(ZipError::Malformed)?);
        let start = offset
            .checked_add(30)
            .and_then(|value| value.checked_add(name_length))
            .and_then(|value| value.checked_add(extra_length))
            .ok_or(ZipError::Malformed)?;
        let end = start
            .checked_add(entry.comp_size)
            .ok_or(ZipError::Malformed)?;
        if end > self.data.len() {
            return Err(ZipError::Malformed);
        }
        let compressed = &self.data[start..end];
        let decoded = match entry.method {
            0 => {
                if compressed.len() != entry.uncomp_size {
                    return Err(ZipError::Malformed);
                }
                compressed.to_vec()
            }
            8 => inflate_raw(compressed, entry.uncomp_size, remaining)?,
            _ => return Err(ZipError::Malformed),
        };
        if decoded.len() != entry.uncomp_size || decoded.len() as u64 > remaining {
            return Err(ZipError::Malformed);
        }
        self.decoded
            .set(self.decoded.get().saturating_add(decoded.len() as u64));
        Ok(decoded)
    }
}

fn entry_range(data: &[u8], entry: &Entry) -> Option<(usize, usize)> {
    let offset = entry.local_offset;
    if offset.checked_add(30)? > data.len() || read_u32(data, offset) != Some(LOCAL_MAGIC) {
        return None;
    }
    if read_u16(data, offset + 8)? != entry.method {
        return None;
    }
    let name_length = usize::from(read_u16(data, offset + 26)?);
    let extra_length = usize::from(read_u16(data, offset + 28)?);
    let start = offset
        .checked_add(30)?
        .checked_add(name_length)?
        .checked_add(extra_length)?;
    let end = start.checked_add(entry.comp_size)?;
    if end > data.len() {
        return None;
    }
    Some((start, end))
}

fn mark_invalid_entries(data: &[u8], entries: &mut [Entry]) {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for entry in entries {
        let Some((start, end)) = entry_range(data, entry) else {
            entry.invalid = true;
            continue;
        };
        if ranges
            .iter()
            .any(|(earlier_start, earlier_end)| start < *earlier_end && *earlier_start < end)
        {
            entry.invalid = true;
            continue;
        }
        ranges.push((start, end));
    }
}

fn inflate_raw(compressed: &[u8], expected: usize, remaining: u64) -> Result<Vec<u8>, ZipError> {
    if compressed.is_empty() {
        return Err(ZipError::Malformed);
    }
    let decoder = flate2::read::DeflateDecoder::new(compressed);
    let per_entry = if expected > 0 {
        (expected as u64).saturating_add(1)
    } else {
        MAX_ENTRY_SIZE.saturating_add(1)
    };
    let limit = per_entry.min(remaining.saturating_add(1));
    let mut decoder = decoder.take(limit);
    let mut output = Vec::new();
    let read = decoder.read_to_end(&mut output);
    if read.is_err() && output.is_empty() {
        return Err(ZipError::Malformed);
    }
    if output.len() as u64 > remaining {
        return Err(ZipError::TooLarge);
    }
    if expected > 0 && output.len() > expected {
        output.truncate(expected);
    }
    if output.is_empty() {
        return Err(ZipError::Malformed);
    }
    Ok(output)
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let bytes = data.get(offset..end)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes = data.get(offset..end)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
