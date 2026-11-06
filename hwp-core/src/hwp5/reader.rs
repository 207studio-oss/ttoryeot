#[derive(Debug, Clone, Copy)]
pub struct BinReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> BinReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    pub fn at(data: &'a [u8], position: usize) -> Self {
        Self { data, position }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn set_position(&mut self, position: usize) {
        self.position = position;
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn u8(&mut self) -> u8 {
        let Some(&value) = self.data.get(self.position) else {
            return 0;
        };
        self.position += 1;
        value
    }

    pub fn i8(&mut self) -> i8 {
        self.u8() as i8
    }

    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    pub fn i16(&mut self) -> i16 {
        self.u16() as i16
    }

    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }

    pub fn i32(&mut self) -> i32 {
        self.u32() as i32
    }

    pub fn f64(&mut self) -> f64 {
        let low = u64::from(self.u32());
        let high = u64::from(self.u32());
        f64::from_bits(low | (high << 32))
    }

    pub fn skip(&mut self, count: usize) {
        self.position += count.min(self.remaining());
    }

    pub fn bytes(&mut self, count: usize) -> &'a [u8] {
        let start = self.position;
        let end = start.saturating_add(count).min(self.data.len());
        if start >= self.data.len() || count == 0 {
            return &[];
        }
        self.position = end;
        &self.data[start..end]
    }

    pub fn utf16(&mut self, chars: usize) -> String {
        let Some(size) = chars.checked_mul(2) else {
            return String::new();
        };
        if size > self.remaining() {
            self.skip(self.remaining());
            return String::new();
        }
        let units: Vec<u16> = self
            .bytes(size)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16(&units).unwrap_or_default()
    }

    pub fn hwp_string(&mut self) -> String {
        let chars = usize::from(self.u16());
        self.utf16(chars)
    }
}

#[cfg(test)]
mod tests {
    use super::BinReader;

    #[test]
    fn reads_out_of_range_as_zero_and_empty() {
        let mut reader = BinReader::new(&[0x01, 0x02]);
        assert_eq!(reader.u32(), 0x0000_0201);
        assert_eq!(reader.remaining(), 0);
        assert_eq!(reader.u8(), 0);
        assert_eq!(reader.u32(), 0);
        assert!(reader.bytes(4).is_empty());
        assert_eq!(reader.utf16(2), "");
        assert_eq!(reader.position(), 2);
    }

    #[test]
    fn partial_bytes_advance_to_end() {
        let mut reader = BinReader::new(&[1, 2, 3]);
        assert_eq!(reader.bytes(8), &[1, 2, 3]);
        assert_eq!(reader.remaining(), 0);
    }
}
