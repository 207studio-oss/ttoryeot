#[path = "support/mod.rs"]
mod support;

use hwp_core::{parse, BlobKind, ParseOptions, Parsed, WarningKind};
use support::{cfbw, rec, zipw};

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn dib() -> Vec<u8> {
    let mut bytes = vec![0; 40];
    put32(&mut bytes, 0, 40);
    put32(&mut bytes, 4, 2);
    put32(&mut bytes, 8, 1);
    put16(&mut bytes, 12, 1);
    put16(&mut bytes, 14, 24);
    bytes.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
    bytes
}

fn wmf_record(function: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 6];
    put32(&mut bytes, 0, ((6 + data.len()).div_ceil(2)) as u32);
    put16(&mut bytes, 4, function);
    bytes.extend_from_slice(data);
    bytes.resize(bytes.len().next_multiple_of(2), 0);
    bytes
}

fn wmf(records: &[Vec<u8>], placeable: bool) -> Vec<u8> {
    let mut bytes = vec![0; 18];
    put16(&mut bytes, 0, 1);
    put16(&mut bytes, 2, 9);
    put16(&mut bytes, 4, 0x300);
    let mut maximum = 3;
    for record in records {
        maximum = maximum.max((record.len() / 2) as u32);
        bytes.extend_from_slice(record);
    }
    bytes.extend(wmf_record(0, &[]));
    let words = (bytes.len() / 2) as u32;
    put32(&mut bytes, 6, words);
    put32(&mut bytes, 12, maximum);
    if placeable {
        let mut header = vec![0; 22];
        put32(&mut header, 0, 0x9ac6_cdd7);
        put16(&mut header, 10, 2);
        put16(&mut header, 12, 1);
        put16(&mut header, 14, 1440);
        let checksum = header[..20]
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(u16::from_le_bytes)
            .fold(0, |sum, word| sum ^ word);
        put16(&mut header, 20, checksum);
        header.extend(bytes);
        header
    } else {
        bytes
    }
}

fn stretch_dib(function: u16, dib: &[u8]) -> Vec<u8> {
    let mut parameters = vec![0; if function == 0x0f43 { 22 } else { 20 }];
    put32(&mut parameters, 0, 0x00cc_0020);
    let dimensions = if function == 0x0f43 { 6 } else { 4 };
    put16(&mut parameters, dimensions, 1);
    put16(&mut parameters, dimensions + 2, 2);
    put16(&mut parameters, dimensions + 8, 1);
    put16(&mut parameters, dimensions + 10, 2);
    parameters.extend_from_slice(dib);
    wmf_record(function, &parameters)
}

fn emf_record(kind: u32, payload: &[u8]) -> Vec<u8> {
    let size = (8 + payload.len()).next_multiple_of(4);
    let mut bytes = vec![0; 8];
    put32(&mut bytes, 0, kind);
    put32(&mut bytes, 4, size as u32);
    bytes.extend_from_slice(payload);
    bytes.resize(size, 0);
    bytes
}

fn emf(dib: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 88];
    put32(&mut bytes, 0, 1);
    put32(&mut bytes, 4, 88);
    put32(&mut bytes, 40, 0x464d_4520);
    put32(&mut bytes, 44, 0x0001_0000);
    put32(&mut bytes, 52, 3);
    put16(&mut bytes, 56, 1);
    let mut record = vec![0; 92];
    put32(&mut record, 0, 81);
    put32(&mut record, 4, 148);
    put32(&mut record, 40, 2);
    put32(&mut record, 44, 1);
    put32(&mut record, 48, 92);
    put32(&mut record, 52, 40);
    put32(&mut record, 56, 140);
    put32(&mut record, 60, (dib.len() - 40) as u32);
    put32(&mut record, 68, 0x00cc_0020);
    put32(&mut record, 72, 2);
    put32(&mut record, 76, 1);
    record.extend_from_slice(&dib[..40]);
    record.resize(140, 0xee);
    record.extend_from_slice(&dib[40..]);
    let size = record.len() as u32;
    put32(&mut record, 4, size);
    bytes.extend(record);
    let mut eof = vec![0; 12];
    put32(&mut eof, 8, 20);
    bytes.extend(emf_record(14, &eof));
    let size = bytes.len() as u32;
    put32(&mut bytes, 48, size);
    bytes
}

fn hwp5_picture(data: &[u8], extension: &str, compressed: bool) -> Parsed {
    let mut header = vec![0; 40];
    header[..17].copy_from_slice(b"HWP Document File");
    put32(&mut header, 32, 0x0500_0500);
    put32(&mut header, 36, u32::from(compressed));
    let mut descriptor = vec![0; 6];
    put16(&mut descriptor, 0, 1);
    put16(&mut descriptor, 2, 7);
    put16(&mut descriptor, 4, extension.encode_utf16().count() as u16);
    for unit in extension.encode_utf16() {
        descriptor.extend_from_slice(&unit.to_le_bytes());
    }
    let mut docinfo = rec::document_properties(1);
    docinfo.extend(rec::record(18, 0, &descriptor));
    let section = rec::paragraph_header();
    let encode = |bytes: &[u8]| {
        if compressed {
            cfbw::compress(bytes)
        } else {
            bytes.to_vec()
        }
    };
    let file = cfbw::build(&[
        ("FileHeader".into(), header),
        ("DocInfo".into(), encode(&docinfo)),
        ("BodyText/Section0".into(), encode(&section)),
        (format!("BinData/BIN0007.{extension}"), encode(data)),
    ]);
    parse(&file, &ParseOptions::default()).expect("parse synthetic HWP5")
}

fn hwpx_picture(data: &[u8], extension: &str, manifest: bool) -> Parsed {
    let path = format!("BinData/picture.{extension}");
    let content =
        format!("<package><manifest><item id='picture' href='{path}'/></manifest></package>");
    let mut entries = vec![
        zipw::Entry {
            name: "Contents/header.xml",
            data: b"<head secCnt='1'/>",
            deflate: false,
        },
        zipw::Entry {
            name: "Contents/section0.xml",
            data: b"<sec><p><run><pic><img binaryItemIDRef='picture'/></pic></run></p></sec>",
            deflate: false,
        },
        zipw::Entry {
            name: &path,
            data,
            deflate: !data.is_empty(),
        },
    ];
    if manifest {
        entries.push(zipw::Entry {
            name: "Contents/content.hpf",
            data: content.as_bytes(),
            deflate: false,
        });
    }
    parse(&zipw::build(&entries), &ParseOptions::default()).expect("parse synthetic HWPX")
}

fn assert_bmp(bytes: &[u8], dib: &[u8], pixel_offset: u32) {
    assert_eq!(&bytes[..2], b"BM");
    assert_eq!(
        u32::from_le_bytes(bytes[2..6].try_into().unwrap()) as usize,
        bytes.len()
    );
    assert_eq!(&bytes[6..10], &[0; 4]);
    assert_eq!(
        u32::from_le_bytes(bytes[10..14].try_into().unwrap()),
        pixel_offset
    );
    assert_eq!(&bytes[14..], dib);
}

#[test]
fn wmf_stretchdib_to_bmp() {
    let dib = dib();
    for placeable in [false, true] {
        for compressed in [false, true] {
            let parsed = hwp5_picture(
                &wmf(&[stretch_dib(0x0f43, &dib)], placeable),
                "wmf",
                compressed,
            );
            assert_eq!(parsed.document.bins.len(), 1);
            let blob = &parsed.document.bins[0];
            assert_eq!(blob.id, "bin1");
            assert_eq!(blob.kind, BlobKind::Picture);
            assert_bmp(&blob.data, &dib, 54);
            assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        }
    }
}

#[test]
fn wmf_dibstretchblt_to_bmp() {
    let dib = dib();
    let parsed = hwp5_picture(&wmf(&[stretch_dib(0x0b41, &dib)], false), "WMF", false);
    assert_bmp(&parsed.document.bins[0].data, &dib, 54);
    assert!(parsed.warnings.is_empty());
}

#[test]
fn emf_stretchdibits_to_bmp() {
    let dib = dib();
    for manifest in [false, true] {
        let parsed = hwpx_picture(&emf(&dib), "emf", manifest);
        assert_eq!(parsed.document.bins.len(), 1);
        let blob = &parsed.document.bins[0];
        assert_eq!(blob.id, "picture");
        assert_bmp(&blob.data, &dib, 54);
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }
    let parsed = hwp5_picture(&emf(&dib), "emf", true);
    assert_bmp(&parsed.document.bins[0].data, &dib, 54);
}

#[test]
fn vector_only_wmf_warns() {
    let bytes = wmf(&[wmf_record(0x0214, &[0, 0, 0, 0])], true);
    let parsed = hwp5_picture(&bytes, "wmf", true);
    assert_eq!(parsed.document.bins[0].id, "bin1");
    assert_eq!(*parsed.document.bins[0].data, bytes);
    assert_eq!(parsed.warnings.len(), 1);
    let warning = &parsed.warnings[0];
    assert_eq!(warning.kind, WarningKind::UnsupportedObject);
    assert_eq!(warning.section, None);
    assert_eq!(warning.detail, "bin1: WMF vector-only");
}

#[test]
fn hwpx_wmf_preserves_picture_reference() {
    let dib = dib();
    for manifest in [false, true] {
        let parsed = hwpx_picture(&wmf(&[stretch_dib(0x0f43, &dib)], true), "wmf", manifest);
        assert_bmp(&parsed.document.bins[0].data, &dib, 54);
        let hwp_core::RunItem::Picture(picture) =
            &parsed.document.sections[0].paragraphs[0].runs[0].items[0]
        else {
            panic!("picture reference was lost");
        };
        assert_eq!(picture.binary_id, "picture");
        assert_eq!(parsed.document.bins[0].id, picture.binary_id);
    }
}

#[test]
fn dib_palette_core_header_and_masks_set_bmp_offset() {
    let mut indexed = dib();
    put16(&mut indexed, 14, 1);
    indexed.truncate(40);
    indexed.extend_from_slice(&[0, 0, 0, 0, 255, 255, 255, 0, 0x80, 0, 0, 0]);
    let mut core = vec![0; 12];
    put32(&mut core, 0, 12);
    put16(&mut core, 4, 2);
    put16(&mut core, 6, 1);
    put16(&mut core, 8, 1);
    put16(&mut core, 10, 1);
    core.extend_from_slice(&[0, 0, 0, 255, 255, 255, 0x80, 0, 0, 0]);
    let mut bitfields = dib();
    bitfields.truncate(40);
    put16(&mut bitfields, 14, 16);
    put32(&mut bitfields, 16, 3);
    bitfields.extend_from_slice(&[0, 0xf8, 0, 0, 0xe0, 7, 0, 0, 0x1f, 0, 0, 0]);
    bitfields.extend_from_slice(&[0, 0xf8, 0xe0, 7]);
    let mut top_down = dib();
    put32(&mut top_down, 8, (-1i32) as u32);
    for (dib, offset) in [(indexed, 62), (core, 32), (bitfields, 66), (top_down, 54)] {
        let parsed = hwp5_picture(&wmf(&[stretch_dib(0x0f43, &dib)], false), "wmf", false);
        assert_bmp(&parsed.document.bins[0].data, &dib, offset);
        assert!(parsed.warnings.is_empty());
    }
}

#[test]
fn other_picture_bytes_are_unchanged() {
    for (extension, data) in [
        ("png", &b"\x89PNG\r\n\x1a\noriginal"[..]),
        ("jpg", &b"\xff\xd8\xfforiginal"[..]),
        ("gif", &b"GIF89aoriginal"[..]),
        ("bmp", &b"BMoriginal"[..]),
        ("tmp", &b"original"[..]),
    ] {
        for parsed in [
            hwp5_picture(data, extension, true),
            hwpx_picture(data, extension, true),
            hwpx_picture(data, extension, false),
        ] {
            assert_eq!(*parsed.document.bins[0].data, data);
            assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        }
    }
}

fn assert_preserved_with_warning(data: &[u8], extension: &str) {
    let parsed = hwpx_picture(data, extension, true);
    assert_eq!(*parsed.document.bins[0].data, data);
    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].kind, WarningKind::UnsupportedObject);
}

#[test]
fn truncated_metafiles_never_produce_bmp() {
    for (bytes, extension) in [
        (wmf(&[stretch_dib(0x0f43, &dib())], false), "wmf"),
        (wmf(&[stretch_dib(0x0b41, &dib())], true), "wmf"),
        (emf(&dib()), "emf"),
    ] {
        for length in 0..bytes.len() {
            assert_preserved_with_warning(&bytes[..length], extension);
        }
    }
}

#[test]
fn malformed_lengths_offsets_and_pixel_sizes_warn() {
    let original_wmf = wmf(&[stretch_dib(0x0f43, &dib())], false);
    for (offset, value) in [
        (6, u32::MAX),
        (18, 0),
        (18, 2),
        (18, u32::MAX),
        (46, u32::MAX),
        (50, i32::MAX as u32),
        (54, i32::MIN as u32),
        (66, 1),
        (66, u32::MAX),
        (78, u32::MAX),
    ] {
        let mut bytes = original_wmf.clone();
        put32(&mut bytes, offset, value);
        assert_preserved_with_warning(&bytes, "wmf");
    }
    let original_emf = emf(&dib());
    for (offset, value) in [
        (4, u32::MAX),
        (48, u32::MAX),
        (52, 1),
        (92, 0),
        (92, 81),
        (92, u32::MAX),
        (136, 0),
        (136, u32::MAX),
        (140, u32::MAX),
        (144, 92),
        (144, u32::MAX),
        (148, 0),
        (148, u32::MAX),
    ] {
        let mut bytes = original_emf.clone();
        put32(&mut bytes, offset, value);
        assert_preserved_with_warning(&bytes, "emf");
    }
}

#[test]
fn multiple_dibs_and_palette_indices_warn() {
    let record = stretch_dib(0x0f43, &dib());
    assert_preserved_with_warning(&wmf(&[record.clone(), record.clone()], false), "wmf");
    let mut palette = record;
    put16(&mut palette, 10, 1);
    assert_preserved_with_warning(&wmf(&[palette], false), "wmf");
    let mut bytes = emf(&dib());
    put32(&mut bytes, 152, 1);
    assert_preserved_with_warning(&bytes, "emf");
}

#[test]
fn vector_only_emf_and_source_free_blits_warn() {
    let mut bytes = emf(&dib());
    bytes.drain(88..bytes.len() - 20);
    let size = bytes.len() as u32;
    put32(&mut bytes, 48, size);
    put32(&mut bytes, 52, 2);
    let parsed = hwpx_picture(&bytes, "emf", true);
    assert_eq!(*parsed.document.bins[0].data, bytes);
    assert_eq!(parsed.warnings[0].detail, "picture: EMF vector-only");
    let bytes = wmf(&[wmf_record(0x0b41, &[0; 22])], false);
    let parsed = hwp5_picture(&bytes, "wmf", false);
    assert_eq!(*parsed.document.bins[0].data, bytes);
    assert_eq!(parsed.warnings[0].detail, "bin1: WMF vector-only");
}

#[test]
fn metafile_signatures_work_without_filename_extension() {
    let dib = dib();
    for bytes in [wmf(&[stretch_dib(0x0f43, &dib)], false), emf(&dib)] {
        let parsed = hwpx_picture(&bytes, "tmp", false);
        assert_bmp(&parsed.document.bins[0].data, &dib, 54);
        assert!(parsed.warnings.is_empty());
    }
}

#[test]
fn unsupported_bitmap_records_are_not_reported_as_vector_only() {
    let bytes = wmf(&[wmf_record(0x0940, &[0; 16])], false);
    let parsed = hwp5_picture(&bytes, "wmf", false);
    assert_eq!(*parsed.document.bins[0].data, bytes);
    assert_eq!(
        parsed.warnings[0].detail,
        "bin1: WMF unsupported bitmap record"
    );
    let mut bytes = emf(&dib());
    put32(&mut bytes, 88, 80);
    let parsed = hwpx_picture(&bytes, "emf", true);
    assert_eq!(*parsed.document.bins[0].data, bytes);
    assert_eq!(
        parsed.warnings[0].detail,
        "picture: EMF unsupported bitmap record"
    );
}

#[test]
fn metafile_mutations_do_not_panic_or_expand_from_size_fields() {
    for (original, extension) in [
        (wmf(&[stretch_dib(0x0f43, &dib())], true), "wmf"),
        (emf(&dib()), "emf"),
    ] {
        let mut state = 0x71c3_2845u32;
        for iteration in 0..500 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut bytes = original.clone();
            let offset = state as usize % bytes.len();
            match iteration % 3 {
                0 => bytes.truncate(offset),
                1 => bytes[offset] ^= (state >> 24) as u8,
                _ => {
                    let length = (bytes.len() - offset).min(4);
                    bytes[offset..offset + length].fill(0xff);
                }
            }
            let parsed = hwpx_picture(&bytes, extension, true);
            let blob = &parsed.document.bins[0];
            assert!(blob.data.len() <= bytes.len() + 14);
            if blob.data.starts_with(b"BM") {
                assert_eq!(
                    u32::from_le_bytes(blob.data[2..6].try_into().unwrap()) as usize,
                    blob.data.len()
                );
                let offset = u32::from_le_bytes(blob.data[10..14].try_into().unwrap()) as usize;
                assert!(offset >= 26 && offset < blob.data.len());
            } else {
                assert_eq!(*blob.data, bytes);
                assert_eq!(parsed.warnings.len(), 1);
            }
        }
    }
}
