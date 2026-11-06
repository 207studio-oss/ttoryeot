use std::io::Write;

use flate2::write::DeflateEncoder;
use flate2::Compression;

pub struct Entry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    pub deflate: bool,
}

pub fn build(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut directory = Vec::new();
    for entry in entries {
        let data = if entry.deflate {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(entry.data).expect("deflate");
            encoder.finish().expect("finish deflate")
        } else {
            entry.data.to_vec()
        };
        let crc = crc32(entry.data);
        let method = if entry.deflate { 8u16 } else { 0u16 };
        let offset = output.len() as u32;
        let name = entry.name.as_bytes();
        write_u32(&mut output, 0x0403_4b50);
        write_u16(&mut output, 20);
        write_u16(&mut output, 0);
        write_u16(&mut output, method);
        write_u16(&mut output, 0);
        write_u16(&mut output, 0);
        write_u32(&mut output, crc);
        write_u32(&mut output, data.len() as u32);
        write_u32(&mut output, entry.data.len() as u32);
        write_u16(&mut output, name.len() as u16);
        write_u16(&mut output, 0);
        output.extend_from_slice(name);
        output.extend_from_slice(&data);

        write_u32(&mut directory, 0x0201_4b50);
        write_u16(&mut directory, 20);
        write_u16(&mut directory, 20);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, method);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u32(&mut directory, crc);
        write_u32(&mut directory, data.len() as u32);
        write_u32(&mut directory, entry.data.len() as u32);
        write_u16(&mut directory, name.len() as u16);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u32(&mut directory, 0);
        write_u32(&mut directory, offset);
        directory.extend_from_slice(name);
    }
    let directory_offset = output.len() as u32;
    output.extend_from_slice(&directory);
    write_u32(&mut output, 0x0605_4b50);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, entries.len() as u16);
    write_u16(&mut output, entries.len() as u16);
    write_u32(&mut output, directory.len() as u32);
    write_u32(&mut output, directory_offset);
    write_u16(&mut output, 0);
    output
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
