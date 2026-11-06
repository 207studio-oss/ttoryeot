use std::io::{Read, Write};
use std::sync::Arc;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use ttoryeot::fonts::FontBook;
use ttoryeot::pdf;
use ttoryeot::scene::{ImageData, Item, Page, Rect};

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(body);
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&crc.sum().to_be_bytes());
    out
}

fn header(depth: u8, color_type: u8, interlace: u8) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&2u32.to_be_bytes());
    body.extend_from_slice(&2u32.to_be_bytes());
    body.extend_from_slice(&[depth, color_type, 0, 0, interlace]);
    chunk(b"IHDR", &body)
}

fn zlib(raw: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(9));
    encoder.write_all(raw).unwrap();
    encoder.finish().unwrap()
}

fn png(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    chunks.iter().for_each(|part| out.extend_from_slice(part));
    out.extend_from_slice(&chunk(b"IEND", b""));
    out
}

const RGB_ROWS: &[u8] = &[
    0, 255, 0, 0, 0, 255, 0, //
    1, 0, 0, 255, 255, 1, 2,
];

fn write_pdf(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let page = Page {
        width: 100.0,
        height: 100.0,
        scale: 1.0,
        items: vec![Item::Image {
            data: Arc::new(ImageData::Png(Arc::new(bytes))),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
        }],
    };
    pdf::write(vec![page], &mut FontBook::scan(&[]))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}

#[test]
fn rgb_png_keeps_its_compressed_rows_across_idat_chunks() {
    let stream = zlib(RGB_ROWS);
    let (first, second) = stream.split_at(stream.len() / 2);
    let pdf = write_pdf(png(&[
        header(8, 2, 0),
        chunk(b"IDAT", first),
        chunk(b"IDAT", second),
    ]))
    .unwrap();

    assert!(
        contains(&pdf, &stream),
        "PNG zlib stream embedded unchanged"
    );
    assert!(contains(
        &pdf,
        b"/DecodeParms<</Predictor 15/Colors 3/BitsPerComponent 8/Columns 2>>"
    ));
}

#[test]
fn gray_png_declares_one_color() {
    let stream = zlib(&[0, 10, 20, 2, 1, 2]);
    let pdf = write_pdf(png(&[header(8, 0, 0), chunk(b"IDAT", &stream)])).unwrap();

    assert!(contains(&pdf, &stream));
    assert!(contains(&pdf, b"/Colors 1/"));
}

/// Samples behind each `/Predictor 15` stream in file order, read the way a PDF reader does.
fn predicted_samples(pdf: &[u8]) -> Vec<Vec<u8>> {
    const KEY: &[u8] = b"/Predictor 15/Colors ";
    let mut found = Vec::new();
    for at in (0..pdf.len()).filter(|&at| pdf[at..].starts_with(KEY)) {
        let dict = &pdf[at + KEY.len()..];
        let colors = number_at(dict);
        let columns_at = position(dict, b"/Columns ") + b"/Columns ".len();
        let row_len = colors * number_at(&dict[columns_at..]);
        let start = position(dict, b"stream\n") + b"stream\n".len();
        let end = start + position(&dict[start..], b"\nendstream");

        let mut rows = Vec::new();
        ZlibDecoder::new(&dict[start..end])
            .read_to_end(&mut rows)
            .unwrap();
        let mut samples: Vec<u8> = Vec::new();
        for row in rows.chunks_exact(1 + row_len) {
            assert_eq!(row[0], 2, "only Up rows are written here");
            let above = samples.len().checked_sub(row_len);
            for (i, delta) in row[1..].iter().enumerate() {
                let up = above.map_or(0, |above| samples[above + i]);
                samples.push(delta.wrapping_add(up));
            }
        }
        found.push(samples);
    }
    found
}

fn position(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|part| part == needle)
        .unwrap()
}

fn number_at(text: &[u8]) -> usize {
    text.iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0, |value, byte| value * 10 + usize::from(byte - b'0'))
}

const RGBA_ROWS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, //
    2, 8, 7, 6, 5, 3, 3, 3, 0,
];
const RGBA_COLOR: &[u8] = &[1, 2, 3, 5, 6, 7, 9, 9, 9, 8, 9, 10];
const RGBA_ALPHA: &[u8] = &[4, 8, 9, 8];

#[test]
fn rgba_png_splits_into_color_and_mask_rows() {
    let pdf = write_pdf(png(&[header(8, 6, 0), chunk(b"IDAT", &zlib(RGBA_ROWS))])).unwrap();

    assert!(contains(&pdf, b"/SMask "));
    let mut images = predicted_samples(&pdf);
    images.sort_by_key(Vec::len);
    assert_eq!(images, [RGBA_ALPHA, RGBA_COLOR]);
}

#[test]
fn interlaced_rgba_png_gets_the_same_rows() {
    let rows: &[u8] = &[
        0, 1, 2, 3, 4, //
        0, 5, 6, 7, 8, //
        0, 9, 9, 9, 9, 8, 9, 10, 8,
    ];
    let pdf = write_pdf(png(&[header(8, 6, 1), chunk(b"IDAT", &zlib(rows))])).unwrap();

    let mut images = predicted_samples(&pdf);
    images.sort_by_key(Vec::len);
    assert_eq!(images, [RGBA_ALPHA, RGBA_COLOR]);
}

#[test]
fn opaque_rgba_png_has_no_mask() {
    let rows: &[u8] = &[
        0, 1, 2, 3, 255, 5, 6, 7, 255, 0, 9, 9, 9, 255, 8, 9, 10, 255,
    ];
    let pdf = write_pdf(png(&[header(8, 6, 0), chunk(b"IDAT", &zlib(rows))])).unwrap();

    assert!(!contains(&pdf, b"/SMask "));
    assert_eq!(predicted_samples(&pdf), [RGBA_COLOR]);
}

#[test]
fn gray_alpha_png_splits_too() {
    let rows: &[u8] = &[0, 10, 1, 20, 2, 0, 30, 3, 40, 4];
    let pdf = write_pdf(png(&[header(8, 4, 0), chunk(b"IDAT", &zlib(rows))])).unwrap();

    let mut images = predicted_samples(&pdf);
    images.sort();
    assert_eq!(images, [[1, 2, 3, 4], [10, 20, 30, 40]]);
}

#[test]
fn transparent_color_png_becomes_a_mask() {
    let rows: &[u8] = &[0, 255, 0, 255, 1, 2, 3, 0, 4, 5, 6, 7, 8, 9];
    let pdf = write_pdf(png(&[
        header(8, 2, 0),
        chunk(b"tRNS", &[0, 255, 0, 0, 0, 255]),
        chunk(b"IDAT", &zlib(rows)),
    ]))
    .unwrap();

    let mut images = predicted_samples(&pdf);
    images.sort_by_key(Vec::len);
    assert_eq!(
        images,
        [
            vec![0, 255, 255, 255],
            vec![255, 0, 255, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        ]
    );
}

#[test]
fn palette_png_expands_to_rgb_rows() {
    let pdf = write_pdf(png(&[
        header(8, 3, 0),
        chunk(b"PLTE", &[1, 2, 3]),
        chunk(b"IDAT", &zlib(&[0, 0, 0, 0, 0, 0])),
    ]))
    .unwrap();

    assert_eq!(predicted_samples(&pdf), [[1, 2, 3].repeat(4)]);
}

#[test]
fn sixteen_bit_png_stays_raw() {
    let pdf = write_pdf(png(&[header(16, 2, 0), chunk(b"IDAT", &zlib(&[0; 26]))])).unwrap();
    assert!(!contains(&pdf, b"/Predictor"));
}

/// Rows of an RGBA PNG, its pixels from `pixel(x, y)`, stored without PNG filters.
fn rgba_png(side: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> (Vec<u8>, Vec<u8>) {
    let mut rows = Vec::new();
    let mut color = Vec::new();
    for y in 0..side {
        rows.push(0);
        for x in 0..side {
            let value = pixel(x, y);
            rows.extend_from_slice(&value);
            color.extend_from_slice(&value[..3]);
        }
    }
    let mut body = Vec::new();
    body.extend_from_slice(&side.to_be_bytes());
    body.extend_from_slice(&side.to_be_bytes());
    body.extend_from_slice(&[8, 6, 0, 0, 0]);
    let file = png(&[chunk(b"IHDR", &body), chunk(b"IDAT", &zlib(&rows))]);
    (file, color)
}

#[test]
fn gradient_png_stays_about_its_own_size() {
    let (file, color) = rgba_png(300, |x, y| {
        let shade = (x * 3 + y * 5) / 4;
        [shade as u8, (shade + 80) as u8, (shade + 160) as u8, 200]
    });
    let pdf = write_pdf(file.clone()).unwrap();

    assert!(
        pdf.len() < 2 * file.len() + 4096,
        "{} {}",
        pdf.len(),
        file.len()
    );
    let images = predicted_samples(&pdf);
    assert!(images.contains(&color));
}

#[test]
fn noisy_png_keeps_its_pixels() {
    let noise = |x: u32, y: u32, salt: u32| {
        let mut value = (x + 1).wrapping_mul(2_654_435_761) ^ (y + 1).wrapping_mul(40_503) ^ salt;
        value ^= value >> 13;
        value.wrapping_mul(1_274_126_177).to_be_bytes()[0]
    };
    let (file, color) = rgba_png(64, |x, y| {
        [
            noise(x, y, 1),
            noise(x, y, 2),
            noise(x, y, 3),
            noise(x, y, 4),
        ]
    });
    let pdf = write_pdf(file).unwrap();

    assert!(predicted_samples(&pdf).contains(&color));
}

#[test]
fn png_failing_a_checksum_still_fails_the_pdf() {
    let mut damaged = chunk(b"IDAT", &zlib(RGB_ROWS));
    damaged[9] ^= 0xff;
    let error = write_pdf(png(&[header(8, 2, 0), damaged])).unwrap_err();
    assert!(error.contains("Image"), "{error}");
}

#[test]
fn broken_zlib_behind_valid_checksums_does_not_fail_the_pdf() {
    let stream = zlib(RGB_ROWS);
    let broken = &stream[..stream.len() / 2];
    let pdf = write_pdf(png(&[header(8, 2, 0), chunk(b"IDAT", broken)])).unwrap();
    assert!(contains(&pdf, b"/Predictor 15"));
}
