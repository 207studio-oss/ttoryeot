use crate::report::{Warning, WarningKind};

type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Copy)]
enum Format {
    Wmf,
    Emf,
}

impl Format {
    fn label(self) -> &'static str {
        match self {
            Self::Wmf => "WMF",
            Self::Emf => "EMF",
        }
    }
}

pub(crate) fn picture_data(
    data: Vec<u8>,
    name: &str,
    id: &str,
    warnings: &mut Vec<Warning>,
) -> Vec<u8> {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    let format = if read32(&data, 0) == Ok(1) && read32(&data, 40) == Ok(0x464d_4520) {
        Format::Emf
    } else if read32(&data, 0) == Ok(0x9ac6_cdd7)
        || (matches!(read16(&data, 0), Ok(1 | 2))
            && read16(&data, 2) == Ok(9)
            && matches!(read16(&data, 4), Ok(0x100 | 0x300)))
        || extension.is_some_and(|ext| ext.eq_ignore_ascii_case("wmf"))
    {
        Format::Wmf
    } else if extension.is_some_and(|ext| ext.eq_ignore_ascii_case("emf")) {
        Format::Emf
    } else {
        return data;
    };
    let result = match format {
        Format::Wmf => extract_wmf(&data),
        Format::Emf => extract_emf(&data),
    }
    .and_then(|dib| dib.map(Dib::into_bmp).transpose());
    let reason = match result {
        Ok(Some(bmp)) => return bmp,
        Ok(None) => "vector-only",
        Err(reason) => reason,
    };
    warnings.push(Warning::new(
        WarningKind::UnsupportedObject,
        None,
        format!("{id}: {} {reason}", format.label()),
    ));
    data
}

fn extract_wmf(data: &[u8]) -> Result<Option<Dib<'_>>> {
    let start = if read32(data, 0)? == 0x9ac6_cdd7 {
        22
    } else {
        0
    };
    let data = data.get(start..).ok_or("truncated header")?;
    if !matches!(read16(data, 0)?, 1 | 2)
        || read16(data, 2)? != 9
        || !matches!(read16(data, 4)?, 0x100 | 0x300)
    {
        return Err("invalid header");
    }
    let size = (read32(data, 6)? as usize)
        .checked_mul(2)
        .ok_or("invalid size")?;
    let data = data.get(..size).ok_or("truncated metafile")?;
    let mut position = 18;
    let mut image = None;
    while position < data.len() {
        let words = read32(data, position)? as usize;
        let size = words.checked_mul(2).ok_or("invalid record size")?;
        if size < 6 {
            return Err("invalid record size");
        }
        let record = take(data, position, size)?;
        let function = read16(record, 4)?;
        position += size;
        if function == 0 {
            return if size == 6 && position == data.len() {
                Ok(image)
            } else {
                Err("invalid end record")
            };
        }
        let dib = match function & 0xff {
            0x43 => {
                if read16(record, 10)? != 0 {
                    return Err("palette-indexed DIB is unsupported");
                }
                record.get(28..).ok_or("truncated DIB record")?
            }
            0x41 if words != usize::from(function >> 8) + 3 => {
                record.get(26..).ok_or("truncated DIB record")?
            }
            0x22 | 0x23 | 0x33 | 0x40 | 0x42 | 0xf9 => {
                return Err("unsupported bitmap record");
            }
            _ => continue,
        };
        if image.is_some() {
            return Err("multiple DIB images are unsupported");
        }
        image = Some(Dib::parse(dib, None)?);
    }
    Err("missing end record")
}

fn extract_emf(data: &[u8]) -> Result<Option<Dib<'_>>> {
    if read32(data, 0)? != 1 || read32(data, 40)? != 0x464d_4520 {
        return Err("invalid header");
    }
    let header_size = read32(data, 4)? as usize;
    if header_size < 88 || !header_size.is_multiple_of(4) {
        return Err("invalid header size");
    }
    let data = take(data, 0, read32(data, 48)? as usize)?;
    take(data, 0, header_size)?;
    let record_count = read32(data, 52)?;
    let mut count = 1u64;
    let mut position = header_size;
    let mut image = None;
    while position < data.len() {
        let size = read32(data, position + 4)? as usize;
        if size < 8 || !size.is_multiple_of(4) {
            return Err("invalid record size");
        }
        let record = take(data, position, size)?;
        position += size;
        count += 1;
        match read32(record, 0)? {
            14 => {
                return if size >= 20 && position == data.len() && count == u64::from(record_count) {
                    Ok(image)
                } else {
                    Err("invalid end record")
                };
            }
            81 => {
                take(record, 0, 80)?;
                let info_offset = read32(record, 48)? as usize;
                let info_size = read32(record, 52)? as usize;
                let bits_offset = read32(record, 56)? as usize;
                let bits_size = read32(record, 60)? as usize;
                if info_size == 0 && bits_size == 0 {
                    continue;
                }
                if read32(record, 64)? != 0 {
                    return Err("palette-indexed DIB is unsupported");
                }
                if info_offset < 80 || bits_offset < 80 {
                    return Err("invalid DIB offsets");
                }
                let info = take(record, info_offset, info_size)?;
                let bits = take(record, bits_offset, bits_size)?;
                if info_offset < bits_offset + bits_size && bits_offset < info_offset + info_size {
                    return Err("overlapping DIB buffers");
                }
                if image.is_some() {
                    return Err("multiple DIB images are unsupported");
                }
                image = Some(Dib::parse(info, Some(bits))?);
            }
            76..=80 | 93 | 94 | 114 | 116 => return Err("unsupported bitmap record"),
            _ => {}
        }
    }
    Err("missing end record")
}

struct Dib<'a> {
    info: &'a [u8],
    bits: &'a [u8],
}

impl<'a> Dib<'a> {
    fn parse(data: &'a [u8], separate_bits: Option<&'a [u8]>) -> Result<Self> {
        let header_size = read32(data, 0)? as usize;
        let (width, height, bit_count, compression, image_size, colors, color_size, masks) =
            match header_size {
                12 => {
                    take(data, 0, 12)?;
                    if read16(data, 8)? != 1 {
                        return Err("invalid DIB planes");
                    }
                    let bit_count = read16(data, 10)?;
                    (
                        i32::from(read16(data, 4)?),
                        i32::from(read16(data, 6)?),
                        bit_count,
                        0,
                        0,
                        if bit_count <= 8 { 1u32 << bit_count } else { 0 },
                        3,
                        0,
                    )
                }
                40 | 52 | 56 | 108 | 124 => {
                    take(data, 0, header_size)?;
                    if read16(data, 12)? != 1 {
                        return Err("invalid DIB planes");
                    }
                    if header_size == 124 && read32(data, 116)? != 0 {
                        return Err("DIB color profile is unsupported");
                    }
                    let bit_count = read16(data, 14)?;
                    let compression = read32(data, 16)?;
                    let used = read32(data, 32)?;
                    let maximum = if bit_count <= 8 && bit_count != 0 {
                        1u32 << bit_count
                    } else {
                        0
                    };
                    if maximum != 0 && used > maximum {
                        return Err("invalid DIB palette size");
                    }
                    let masks = match (compression, header_size) {
                        (3, 40) => 12,
                        (6, 40) => 16,
                        (6, 52) => return Err("missing DIB alpha mask"),
                        _ => 0,
                    };
                    (
                        read32(data, 4)? as i32,
                        read32(data, 8)? as i32,
                        bit_count,
                        compression,
                        read32(data, 20)?,
                        if used == 0 { maximum } else { used },
                        4,
                        masks,
                    )
                }
                _ => return Err("unsupported DIB header"),
            };
        if width <= 0 || height == 0 {
            return Err("invalid DIB dimensions");
        }
        let uncompressed = match (compression, bit_count) {
            (0, 1 | 4 | 8 | 16 | 24 | 32) | (3 | 6, 16 | 32) => true,
            (1, 8) | (2, 4) | (4 | 5, 0) if height > 0 && image_size != 0 => false,
            _ => return Err("unsupported DIB encoding"),
        };
        let color_bytes = (colors as usize)
            .checked_mul(color_size)
            .ok_or("invalid DIB palette size")?;
        let info_size = header_size
            .checked_add(masks)
            .and_then(|size| size.checked_add(color_bytes))
            .ok_or("invalid DIB header size")?;
        take(data, 0, info_size)?;
        let (info, bits) = match separate_bits {
            Some(bits) => (data, bits),
            None => (
                take(data, 0, info_size)?,
                data.get(info_size..).ok_or("truncated DIB")?,
            ),
        };
        let minimum = if uncompressed {
            let stride = (u64::from(width as u32) * u64::from(bit_count)).div_ceil(32) * 4;
            stride
                .checked_mul(u64::from(height.unsigned_abs()))
                .ok_or("invalid DIB pixel size")?
        } else {
            u64::from(image_size)
        };
        let size = if image_size == 0 {
            minimum
        } else {
            u64::from(image_size)
        };
        if size < minimum {
            return Err("invalid DIB pixel size");
        }
        let size = usize::try_from(size).map_err(|_| "invalid DIB pixel size")?;
        let bits = take(bits, 0, size)?;
        Ok(Self { info, bits })
    }

    fn into_bmp(self) -> Result<Vec<u8>> {
        let offset = 14usize
            .checked_add(self.info.len())
            .ok_or("BMP too large")?;
        let size = offset.checked_add(self.bits.len()).ok_or("BMP too large")?;
        let file_size = u32::try_from(size).map_err(|_| "BMP too large")?;
        let bits_offset = u32::try_from(offset).map_err(|_| "BMP too large")?;
        let mut bmp = Vec::with_capacity(size);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&bits_offset.to_le_bytes());
        bmp.extend_from_slice(self.info);
        bmp.extend_from_slice(self.bits);
        Ok(bmp)
    }
}

fn take(data: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    let end = offset.checked_add(size).ok_or("invalid data size")?;
    data.get(offset..end).ok_or("truncated data")
}

fn read16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = take(data, offset, 2)?
        .try_into()
        .map_err(|_| "truncated word")?;
    Ok(u16::from_le_bytes(bytes))
}

fn read32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = take(data, offset, 4)?
        .try_into()
        .map_err(|_| "truncated dword")?;
    Ok(u32::from_le_bytes(bytes))
}
