use std::path::Path;

const LOCAL_MAGIC: u32 = 0x0403_4b50;
const CENTRAL_MAGIC: u32 = 0x0201_4b50;
const EOCD_MAGIC: u32 = 0x0605_4b50;

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
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

fn zip_stored(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in entries {
        let crc = crc32(data);
        let offset = output.len() as u32;
        let name = name.as_bytes();
        write_u32(&mut output, LOCAL_MAGIC);
        write_u16(&mut output, 20);
        write_u16(&mut output, 0);
        write_u16(&mut output, 0);
        write_u16(&mut output, 0);
        write_u16(&mut output, 0);
        write_u32(&mut output, crc);
        write_u32(&mut output, data.len() as u32);
        write_u32(&mut output, data.len() as u32);
        write_u16(&mut output, name.len() as u16);
        write_u16(&mut output, 0);
        output.extend_from_slice(name);
        output.extend_from_slice(data);

        write_u32(&mut directory, CENTRAL_MAGIC);
        write_u16(&mut directory, 20);
        write_u16(&mut directory, 20);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u16(&mut directory, 0);
        write_u32(&mut directory, crc);
        write_u32(&mut directory, data.len() as u32);
        write_u32(&mut directory, data.len() as u32);
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
    write_u32(&mut output, EOCD_MAGIC);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, entries.len() as u16);
    write_u16(&mut output, entries.len() as u16);
    write_u32(&mut output, directory.len() as u32);
    write_u32(&mut output, directory_offset);
    write_u16(&mut output, 0);
    output
}

fn build() -> Vec<u8> {
    let header = br#"<hh:head xmlns:hh="urn:hh" secCnt="1"/>"#;
    let section = br#"<hp:sec xmlns:hp="urn:hp" xmlns:hc="urn:hc">
<hp:p><hp:run><hp:t>body text</hp:t>
<hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:cellSz width="100" height="100"/><hp:subList><hp:p><hp:run><hp:t>cell text</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl>
</hp:run></hp:p>
</hp:sec>"#;
    zip_stored(&[
        ("Contents/header.xml", header),
        ("Contents/section0.xml", section),
    ])
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let directory = args.next().unwrap_or_else(|| {
        eprintln!("usage: parity_fixture <directory>");
        std::process::exit(2);
    });
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory).expect("create output directory");
    let bytes = build();
    for name in ["left.hwpx", "right.hwpx"] {
        std::fs::write(directory.join(name), &bytes).expect("write fixture");
    }
}
