//! 문서에 내장된 글꼴을 디스크에 쓰지 않고 문서별 메모리 색인으로 그리는지 확인한다.

use hwp_core::{Blob, BlobKind, CharPr, Document, FontFace, Paragraph, Run, RunItem, Section};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use ttoryeot::fonts::{Embedding, FontBook};
use ttoryeot::scene::{GlyphRun, Item, Page};

mod common;
use common::rename_font;

fn font_path() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    )
}

fn font_bytes() -> Vec<u8> {
    std::fs::read(font_path()).expect("read test font")
}

fn one_line(face: FontFace) -> Document {
    Document {
        font_faces: BTreeMap::from([("hangul".to_string(), vec![face])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("hangul".to_string(), 0)]),
            ..CharPr::default()
        }],
        sections: vec![Section {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    char_pr_id: 0,
                    items: vec![RunItem::Text("Embedded font".to_string())],
                    supscript: false,
                }],
                ..Paragraph::default()
            }],
            ..Section::default()
        }],
        ..Document::default()
    }
}

fn with_embedded_font(mut document: Document, data: Vec<u8>) -> Document {
    let mut face = FontFace::new("Liberation Sans");
    face.embedded = true;
    face.binary_ref = Some("bin1".to_string());
    document.font_faces.insert("hangul".to_string(), vec![face]);
    document.bins.push(Blob {
        id: "bin1".to_string(),
        kind: BlobKind::Font,
        data: data.into(),
    });
    document
}

fn glyphs(pages: &[ttoryeot::scene::Page]) -> usize {
    pages
        .iter()
        .flat_map(|page| &page.items)
        .map(|item| match item {
            ttoryeot::scene::Item::Glyphs(run) => run.glyphs.len(),
            _ => 0,
        })
        .sum()
}

/// OS/2 표의 위치.
fn os2_offset(bytes: &[u8]) -> usize {
    let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    for index in 0..tables {
        let record = 12 + index * 16;
        if &bytes[record..record + 4] == b"OS/2" {
            return u32::from_be_bytes(
                bytes[record + 8..record + 12]
                    .try_into()
                    .expect("OS/2 offset"),
            ) as usize;
        }
    }
    panic!("test font has no OS/2 table");
}

/// OS/2 표의 `fsType`만 바꾼 글꼴 사본.
fn with_fs_type(data: &[u8], fs_type: u16) -> Vec<u8> {
    let mut bytes = data.to_vec();
    let offset = os2_offset(&bytes);
    bytes[offset + 8..offset + 10].copy_from_slice(&fs_type.to_be_bytes());
    bytes
}

/// OS/2 표의 `version`과 `fsType`을 바꾼 글꼴 사본.
fn with_os2(data: &[u8], version: u16, fs_type: u16) -> Vec<u8> {
    let mut bytes = with_fs_type(data, fs_type);
    let offset = os2_offset(&bytes);
    bytes[offset..offset + 2].copy_from_slice(&version.to_be_bytes());
    bytes
}

/// OS/2 표가 없는 글꼴 사본. 표 이름표만 바꾸므로 표 자체는 읽히지 않는다.
fn without_os2(data: &[u8]) -> Vec<u8> {
    let mut bytes = data.to_vec();
    let mut offset = os2_offset(&bytes);
    let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    for index in 0..tables {
        let record = 12 + index * 16;
        if &bytes[record..record + 4] == b"OS/2" {
            offset = record;
            break;
        }
    }
    bytes[offset..offset + 4].copy_from_slice(b"OS/3");
    bytes
}

/// OS/2 표의 `usWeightClass`만 바꾼 글꼴 사본.
fn with_weight(data: &[u8], weight: u16) -> Vec<u8> {
    let mut bytes = data.to_vec();
    let offset = os2_offset(&bytes);
    bytes[offset + 4..offset + 6].copy_from_slice(&weight.to_be_bytes());
    bytes
}

/// 정책 시험에 쓰는 원본 글꼴. 이름을 `Policy Sans`로 바꿔 대체 사슬의 HCR Dotum과 구별한다.
fn policy_font(fs_type: u16) -> Vec<u8> {
    let family = rename_font(&font_bytes(), "Liberation Sans", "Policy Sans");
    let family = rename_font(&family, "LiberationSans", "PolicySans");
    with_fs_type(&family, fs_type)
}

/// 시험용 HCR Dotum. 실제 운영체제 글꼴이 없어도 대체 사슬을 재현한다.
fn fallback_font(bold: bool) -> Vec<u8> {
    let family = rename_font(&font_bytes(), "Liberation Sans", "HCR Dotum");
    let postscript = if bold { "HCRDotum-Bold" } else { "HCRDotum" };
    let family = rename_font(&family, "LiberationSans", postscript);
    let family = if bold {
        with_weight(&family, 700)
    } else {
        family
    };
    with_fs_type(&family, 0x0008)
}

/// 대체 사슬{HCR Dotum 보통, HCR Dotum 굵게}이 들어 있는 색인.
fn policy_fonts() -> FontBook {
    let mut fonts = FontBook::scan(&[]);
    fonts.add_bytes("hcr-dotum.ttf".into(), fallback_font(false));
    fonts.add_bytes("hcr-dotum-bold.ttf".into(), fallback_font(true));
    fonts
}

const POLICY_TEXT: &str = "Policy-fsType-012";

/// `Policy Sans` 한 줄짜리 문서. `embedded`면 글꼴을 문서에 넣는다.
fn policy_document(embedded: bool, bold: bool, data: Vec<u8>) -> Document {
    let mut face = FontFace::new("Policy Sans");
    let mut document = Document {
        font_faces: BTreeMap::from([("hangul".to_string(), vec![face.clone()])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("hangul".to_string(), 0)]),
            bold,
            ..CharPr::default()
        }],
        sections: vec![Section {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    char_pr_id: 0,
                    items: vec![RunItem::Text(POLICY_TEXT.to_string())],
                    supscript: false,
                }],
                ..Paragraph::default()
            }],
            ..Section::default()
        }],
        ..Document::default()
    };
    if embedded {
        face.embedded = true;
        face.binary_ref = Some("bin1".to_string());
        document.font_faces.insert("hangul".to_string(), vec![face]);
        document.bins.push(Blob {
            id: "bin1".to_string(),
            kind: BlobKind::Font,
            data: data.into(),
        });
    }
    document
}

/// 이 얼굴로 글자 하나짜리 줄을 만든다. 배치를 거치지 않고 PDF 쓰기만 시험할 때 쓴다.
fn text_page(fonts: &mut FontBook, face: usize, text: &str) -> Page {
    let size = 12.0;
    let mut glyphs = Vec::new();
    let mut ranges = Vec::new();
    let mut x = 0.0;
    for (start, character) in text.char_indices() {
        let info = fonts
            .glyph(face, character)
            .unwrap_or_else(|| panic!("face {face} has no glyph for {character:?}"));
        glyphs.push((info.glyph, x));
        ranges.push(start..start + character.len_utf8());
        x += info.advance * size;
    }
    Page {
        width: 300.0,
        height: 100.0,
        scale: 1.0,
        items: vec![Item::Glyphs(GlyphRun {
            face,
            size,
            origin: (20.0, 50.0),
            scale_x: 1.0,
            italic: false,
            bold_stroke: None,
            color: 0,
            glyphs,
            text: text.to_string(),
            ranges,
        })],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// 글꼴 프로그램을 부분 집합으로 넣는다.
    Embed,
    /// 글꼴 자료를 넣지 않고 글꼴이 없을 때와 같은 사슬로 대체한다.
    Fallback,
}

fn run_faces(pages: &[Page]) -> Vec<usize> {
    pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some(run.face),
            _ => None,
        })
        .collect()
}

/// 한 `fsType` 값의 설치 글꼴과 문서 내장 글꼴을 모두 확인한다.
fn assert_policy(fs_type: u16, outcome: Outcome) {
    for embedded in [false, true] {
        let tag = format!(
            "fsType {fs_type:#06x}, {} font",
            if embedded {
                "document-embedded"
            } else {
                "installed"
            }
        );
        assert_policy_case(
            policy_font(fs_type),
            Embedding::from_fs_type(fs_type),
            outcome,
            false,
            embedded,
            &tag,
        );
    }
}

fn assert_policy_case(
    source: Vec<u8>,
    expected_embedding: Embedding,
    outcome: Outcome,
    bold: bool,
    embedded: bool,
    tag: &str,
) {
    let mut fonts = policy_fonts();
    if !embedded {
        assert_eq!(fonts.face_count(), 2, "{tag}: fallback faces");
        fonts.add_bytes("policy.ttf".into(), source.clone());
    }
    let document = policy_document(embedded, bold, source);
    let pages = ttoryeot::render(&document, &mut fonts);
    let faces = run_faces(&pages);
    assert!(!faces.is_empty(), "{tag}: no glyph run");
    assert!(
        faces.iter().all(|face| *face == faces[0]),
        "{tag}: one document font became mixed faces {faces:?}"
    );
    assert_eq!(
        fonts.embedding(2),
        Some(expected_embedding),
        "{tag}: source face"
    );

    let pdf = ttoryeot::pdf::write(pages, &mut fonts).expect("write pdf");
    let content = page_content(&pdf);
    let names = base_font_names(&pdf);
    assert!(text_operators(&content) > 0, "{tag}: no text operators");
    assert_eq!(extract_text(&pdf), POLICY_TEXT, "{tag}: extracted text");

    match outcome {
        Outcome::Embed => {
            assert!(expected_embedding.can_embed(), "{tag}: embedding denied");
            assert_eq!(faces[0], 2, "{tag}: source face was not used");
            assert!(
                has_subset_font(&pdf, "PolicySans"),
                "{tag}: source font was not subset; BaseFont {names:?}"
            );
            assert!(
                !names.iter().any(|name| name.contains("HCRDotum")),
                "{tag}: fallback was embedded too; BaseFont {names:?}"
            );
        }
        Outcome::Fallback => {
            assert!(!expected_embedding.can_embed(), "{tag}: embedding allowed");
            let expected_face = if bold { 1 } else { 0 };
            assert_eq!(
                faces[0], expected_face,
                "{tag}: restricted font was not replaced by the HCR Dotum chain"
            );
            assert!(
                !has_subset_font(&pdf, "PolicySans"),
                "{tag}: restricted font was subset; BaseFont {names:?}"
            );
            let expected_name = if bold { "+HCRDotum-Bold" } else { "+HCRDotum" };
            assert!(
                names.iter().any(|name| name.ends_with(expected_name)),
                "{tag}: missing-font fallback not embedded; BaseFont {names:?}"
            );
            assert_eq!(
                path_operators(&content),
                0,
                "{tag}: restricted font was drawn as vector outlines"
            );
        }
    }
}

/// PDF의 스트림을 풀어 ASCII인 것만 모은다. 글꼴 프로그램 같은 이진 스트림은 뺀다.
fn page_content(pdf: &[u8]) -> String {
    let mut content = String::new();
    let mut search = 0;
    while let Some(index) = find_from(pdf, b"stream", search) {
        if index > 0 && pdf[index - 1].is_ascii_alphanumeric() {
            search = index + b"stream".len();
            continue;
        }
        let dictionary_start = index.saturating_sub(512);
        let length = stream_length(&pdf[dictionary_start..index]);
        let mut data_start = index + b"stream".len();
        if pdf.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if pdf.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        let data = length
            .and_then(|length| pdf.get(data_start..data_start + length))
            .unwrap_or_else(|| {
                let end = find_from(pdf, b"endstream", data_start).unwrap_or(pdf.len());
                &pdf[data_start..end]
            });
        let mut decoded = Vec::new();
        if flate2::read::ZlibDecoder::new(data)
            .read_to_end(&mut decoded)
            .is_ok()
            && decoded.iter().all(u8::is_ascii)
        {
            content.push_str(&String::from_utf8_lossy(&decoded));
            content.push('\n');
        }
        search = data_start + data.len();
    }
    content
}

fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from)
}

/// `/Length` 뒤의 숫자. 스트림 사전이 512바이트 안에 있다고 본다.
fn stream_length(dictionary: &[u8]) -> Option<usize> {
    let position = dictionary
        .windows(b"/Length".len())
        .rposition(|window| window == b"/Length")?;
    let mut index = position + b"/Length".len();
    while dictionary.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    let start = index;
    while dictionary.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    std::str::from_utf8(&dictionary[start..index])
        .ok()?
        .parse()
        .ok()
}

fn token_count(content: &str, token: &str) -> usize {
    content
        .split_whitespace()
        .filter(|word| *word == token)
        .count()
}

fn text_operators(content: &str) -> usize {
    token_count(content, "BT") + token_count(content, "Tj") + token_count(content, "TJ")
}

fn path_operators(content: &str) -> usize {
    token_count(content, "m")
        + token_count(content, "l")
        + token_count(content, "c")
        + token_count(content, "f")
}

/// PDF의 모든 스트림을 풀어 놓는다.
fn decoded_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();
    let mut search = 0;
    while let Some(index) = find_from(pdf, b"stream", search) {
        if index > 0 && pdf[index - 1].is_ascii_alphanumeric() {
            search = index + b"stream".len();
            continue;
        }
        let dictionary_start = index.saturating_sub(512);
        let length = stream_length(&pdf[dictionary_start..index]);
        let mut data_start = index + b"stream".len();
        if pdf.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if pdf.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        let data = length
            .and_then(|length| pdf.get(data_start..data_start + length))
            .unwrap_or_else(|| {
                let end = find_from(pdf, b"endstream", data_start).unwrap_or(pdf.len());
                &pdf[data_start..end]
            });
        let mut decoded = Vec::new();
        if flate2::read::ZlibDecoder::new(data)
            .read_to_end(&mut decoded)
            .is_ok()
        {
            streams.push(decoded);
        }
        search = data_start + data.len();
    }
    streams
}

/// PDF의 `/BaseFont` 이름들(부분 집합 접두사 포함).
fn base_font_names(pdf: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut search = 0;
    while let Some(index) = find_from(pdf, b"/BaseFont", search) {
        let mut cursor = index + b"/BaseFont".len();
        while pdf.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if pdf.get(cursor) == Some(&b'/') {
            cursor += 1;
            let start = cursor;
            while let Some(&byte) = pdf.get(cursor) {
                if byte.is_ascii_whitespace()
                    || matches!(byte, b'/' | b'<' | b'>' | b'[' | b']' | b'(' | b')')
                {
                    break;
                }
                cursor += 1;
            }
            names.push(String::from_utf8_lossy(&pdf[start..cursor]).into_owned());
        }
        search = cursor.max(index + 1);
    }
    names
}

/// 이 이름의 글꼴이 부분 집합 접두사(`ABCDEF+`)를 달고 PDF에 들어갔는지.
fn has_subset_font(pdf: &[u8], name: &str) -> bool {
    let needle = format!("+{name}");
    base_font_names(pdf)
        .iter()
        .any(|font| font.ends_with(&needle))
}

fn hex_u16(token: &str) -> Option<u16> {
    let value = token.trim().trim_start_matches('<').trim_end_matches('>');
    (value.len() == 4)
        .then(|| u16::from_str_radix(value, 16).ok())
        .flatten()
}

fn hex_text(token: &str) -> Option<String> {
    let value = token.trim().trim_start_matches('<').trim_end_matches('>');
    if value.is_empty() || !value.len().is_multiple_of(4) {
        return None;
    }
    let mut text = String::new();
    for index in (0..value.len()).step_by(4) {
        let code = u16::from_str_radix(&value[index..index + 4], 16).ok()?;
        text.push(char::from_u32(u32::from(code))?);
    }
    Some(text)
}

/// ToUnicode CMap의 글리프 → 글자 표를 읽는다.
fn parse_cmap(stream: &str, map: &mut HashMap<u16, String>) {
    let mut mode = "";
    for line in stream.lines() {
        let line = line.trim();
        if line.ends_with("beginbfchar") {
            mode = "char";
            continue;
        }
        if line.ends_with("endbfchar") {
            mode = "";
            continue;
        }
        if line.ends_with("beginbfrange") {
            mode = "range";
            continue;
        }
        if line.ends_with("endbfrange") {
            mode = "";
            continue;
        }
        let mut fields = line.split_whitespace();
        match mode {
            "char" => {
                let (Some(source), Some(target)) = (fields.next(), fields.next()) else {
                    continue;
                };
                if let (Some(source), Some(target)) = (hex_u16(source), hex_text(target)) {
                    map.insert(source, target);
                }
            }
            "range" => {
                let (Some(start), Some(end), Some(target)) =
                    (fields.next(), fields.next(), fields.next())
                else {
                    continue;
                };
                let (Some(start), Some(end), Some(target)) =
                    (hex_u16(start), hex_u16(end), hex_text(target))
                else {
                    continue;
                };
                for code in start..=end {
                    map.insert(code, target.clone());
                }
            }
            _ => {}
        }
    }
}

/// PDF 내용 스트림의 문자열 하나를 PDF 이스케이프까지 풀어 읽는다.
fn pdf_literal_strings(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'(' {
            cursor += 1;
            continue;
        }
        cursor += 1;
        let mut string = Vec::new();
        while let Some(&byte) = bytes.get(cursor) {
            match byte {
                b')' => {
                    cursor += 1;
                    break;
                }
                b'\\' => {
                    cursor += 1;
                    let Some(&escaped) = bytes.get(cursor) else {
                        break;
                    };
                    match escaped {
                        b'0'..=b'7' => {
                            let mut value = 0u8;
                            let mut count = 0;
                            while count < 3
                                && bytes
                                    .get(cursor)
                                    .is_some_and(|byte| (b'0'..=b'7').contains(byte))
                            {
                                value = value * 8 + (bytes[cursor] - b'0');
                                cursor += 1;
                                count += 1;
                            }
                            string.push(value);
                            continue;
                        }
                        b'n' => string.push(b'\n'),
                        b'r' => string.push(b'\r'),
                        b't' => string.push(b'\t'),
                        b'b' => string.push(8),
                        b'f' => string.push(12),
                        b'\r' => {
                            if bytes.get(cursor + 1) == Some(&b'\n') {
                                cursor += 1;
                            }
                        }
                        b'\n' => {}
                        other => string.push(other),
                    }
                    cursor += 1;
                }
                other => {
                    string.push(other);
                    cursor += 1;
                }
            }
        }
        out.push(string);
    }
    out
}

/// PDF에서 글자를 뽑는다. ToUnicode CMap과 내용 스트림의 글리프 번호를 잇는다.
fn extract_text(pdf: &[u8]) -> String {
    let streams = decoded_streams(pdf);
    let mut map = HashMap::new();
    for stream in &streams {
        let text = String::from_utf8_lossy(stream);
        if text.contains("beginbfchar") || text.contains("beginbfrange") {
            parse_cmap(&text, &mut map);
        }
    }
    let mut extracted = String::new();
    for stream in &streams {
        let has_text_font = stream.windows(2).any(|window| window == b"Tf");
        let has_text = stream.windows(2).any(|window| window == b"TJ")
            || stream.windows(2).any(|window| window == b"Tj");
        if !has_text_font || !has_text {
            continue;
        }
        for string in pdf_literal_strings(stream) {
            for pair in string.as_chunks::<2>().0 {
                let code = u16::from_be_bytes([pair[0], pair[1]]);
                if let Some(text) = map.get(&code) {
                    extracted.push_str(text);
                }
            }
        }
    }
    extracted
}

#[test]
fn fs_type_decides_whether_a_face_may_be_written_to_the_pdf() {
    for (fs_type, embedding) in [
        (0x0000, Embedding::Installable),
        (0x0001, Embedding::Unknown),
        (0x0002, Embedding::Restricted),
        (0x0003, Embedding::Unknown),
        (0x0004, Embedding::PreviewPrint),
        (0x0005, Embedding::Unknown),
        (0x0006, Embedding::PreviewPrint),
        (0x0007, Embedding::Unknown),
        (0x0008, Embedding::Editable),
        (0x0009, Embedding::Unknown),
        (0x000A, Embedding::Editable),
        (0x000B, Embedding::Unknown),
        (0x000C, Embedding::Editable),
        (0x000D, Embedding::Unknown),
        (0x000E, Embedding::Editable),
        (0x000F, Embedding::Unknown),
        (0x0010, Embedding::Unknown),
        (0x0100, Embedding::NoSubsetting),
        (0x0104, Embedding::NoSubsetting),
        (0x0200, Embedding::BitmapOnly),
        (0x0204, Embedding::BitmapOnly),
    ] {
        assert_eq!(
            Embedding::from_fs_type(fs_type),
            embedding,
            "{fs_type:#06x}"
        );
    }
    assert!(Embedding::Installable.can_embed());
    assert!(Embedding::PreviewPrint.can_embed());
    assert!(Embedding::Editable.can_embed());
    assert!(!Embedding::Restricted.can_embed());
    assert!(!Embedding::NoSubsetting.can_embed());
    assert!(!Embedding::BitmapOnly.can_embed());
    assert!(!Embedding::Unknown.can_embed());
}

#[test]
fn os2_version_decides_which_fs_type_bits_are_read() {
    assert_eq!(Embedding::from_os2(0, 0x0104), Embedding::PreviewPrint);
    assert_eq!(Embedding::from_os2(1, 0x0204), Embedding::PreviewPrint);
    assert_eq!(Embedding::from_os2(2, 0x0104), Embedding::NoSubsetting);
    assert_eq!(Embedding::from_os2(2, 0x0204), Embedding::BitmapOnly);
    assert_eq!(Embedding::from_os2(0, 0x0010), Embedding::Installable);
    assert_eq!(Embedding::from_os2(2, 0x0010), Embedding::Unknown);
}

#[test]
fn os2_version_is_used_when_indexing_a_face() {
    for (version, fs_type, expected) in [
        (0, 0x0104, Embedding::PreviewPrint),
        (1, 0x0204, Embedding::PreviewPrint),
        (2, 0x0104, Embedding::NoSubsetting),
        (2, 0x0204, Embedding::BitmapOnly),
        (2, 0x0009, Embedding::Unknown),
        (2, 0x000B, Embedding::Unknown),
        (2, 0x000D, Embedding::Unknown),
        (2, 0x000F, Embedding::Unknown),
    ] {
        let mut fonts = FontBook::scan(&[]);
        fonts.add_bytes(
            font_path().into(),
            with_os2(&font_bytes(), version, fs_type),
        );
        assert_eq!(fonts.embedding(0), Some(expected), "version {version}");
    }
}

#[test]
fn hwpx_embedded_font_is_rendered_from_memory() {
    let font = font_bytes();
    let bytes = embedded_hwpx(&font);
    let mut fonts = FontBook::scan(&[]);
    assert_eq!(fonts.face_count(), 0);

    let output = ttoryeot::to_pdf(&bytes, None, &mut fonts).expect("render embedded font");
    assert!(output.pages >= 1);
    assert_eq!(fonts.document_face_count(), 1);
    assert!(
        output
            .pdf
            .windows(b"/FontFile2".len())
            .any(|window| window == b"/FontFile2"),
        "the PDF should contain the embedded TrueType font"
    );
}

#[test]
fn fs_type_0000_embeds_installed_and_document_fonts_as_subsets() {
    assert_policy(0x0000, Outcome::Embed);
}

#[test]
fn fs_type_0002_restricts_installed_and_document_fonts_without_outlines() {
    assert_policy(0x0002, Outcome::Fallback);
}

#[test]
fn fs_type_0004_preview_and_print_embeds_installed_and_document_fonts() {
    assert_policy(0x0004, Outcome::Embed);
}

#[test]
fn fs_type_0008_editable_embeds_installed_and_document_fonts_as_subsets() {
    assert_policy(0x0008, Outcome::Embed);
}

#[test]
fn fs_type_0100_no_subsetting_is_never_subset() {
    assert_policy(0x0100, Outcome::Fallback);
}

#[test]
fn fs_type_0200_bitmap_only_has_no_outlines_and_uses_the_fallback() {
    assert_policy(0x0200, Outcome::Fallback);
}

#[test]
fn restricted_bold_uses_the_bold_missing_font_fallback() {
    for fs_type in [0x0002, 0x0100, 0x0200] {
        for embedded in [false, true] {
            let tag = format!(
                "bold fsType {fs_type:#06x}, {} font",
                if embedded {
                    "document-embedded"
                } else {
                    "installed"
                }
            );
            assert_policy_case(
                policy_font(fs_type),
                Embedding::from_fs_type(fs_type),
                Outcome::Fallback,
                true,
                embedded,
                &tag,
            );
        }
    }
}

#[test]
fn missing_os2_is_unknown_and_uses_the_missing_font_fallback() {
    let source = without_os2(&policy_font(0x0000));
    for embedded in [false, true] {
        let tag = format!(
            "missing OS/2, {} font",
            if embedded {
                "document-embedded"
            } else {
                "installed"
            }
        );
        assert_policy_case(
            source.clone(),
            Embedding::Unknown,
            Outcome::Fallback,
            false,
            embedded,
            &tag,
        );
    }
}

#[test]
fn restricted_page_written_directly_uses_the_missing_font_fallback() {
    let mut fonts = policy_fonts();
    fonts.add_bytes("policy.ttf".into(), policy_font(0x0002));
    let page = text_page(&mut fonts, 2, "Direct");
    let pdf = ttoryeot::pdf::write(vec![page], &mut fonts).expect("write pdf");
    assert!(has_subset_font(&pdf, "HCRDotum"));
    assert!(!has_subset_font(&pdf, "PolicySans"));
    assert_eq!(path_operators(&page_content(&pdf)), 0);
    assert_eq!(extract_text(&pdf), "Direct");
}

#[test]
fn next_document_clears_the_previous_documents_embedded_fonts() {
    let document = with_embedded_font(one_line(FontFace::new("Liberation Sans")), font_bytes());
    let mut fonts = FontBook::scan(&[]);
    ttoryeot::render(&document, &mut fonts);
    assert_eq!(fonts.document_face_count(), 1);

    let plain = one_line(FontFace::new("함초롬바탕"));
    ttoryeot::render(&plain, &mut fonts);
    assert_eq!(fonts.document_face_count(), 0);
    assert_eq!(fonts.face_count(), 0);
    assert!(fonts.find("Liberation Sans", false).is_none());
}

#[test]
fn document_without_embedded_fonts_keeps_the_index_and_output() {
    let document = one_line(FontFace::new("Liberation Sans"));
    let mut fonts = FontBook::scan(&[]);
    fonts.add_bytes(font_path().into(), font_bytes());
    let before = fonts.face_count();
    let selected = fonts.find("Liberation Sans", false).unwrap().0;

    let pages = ttoryeot::render(&document, &mut fonts);
    assert_eq!(fonts.face_count(), before);
    assert_eq!(fonts.document_face_count(), 0);
    assert_eq!(fonts.find("Liberation Sans", false).unwrap().0, selected);
    assert!(glyphs(&pages) > 0);
    let pdf = ttoryeot::pdf::write(pages, &mut fonts).expect("write pdf");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn add_bytes_after_a_document_keeps_the_document_boundary() {
    let document = with_embedded_font(one_line(FontFace::new("Liberation Sans")), font_bytes());
    let mut fonts = FontBook::scan(&[]);
    ttoryeot::render(&document, &mut fonts);
    assert_eq!(fonts.document_face_count(), 1);
    assert_eq!(fonts.face_count(), 1);

    fonts.add_bytes("second.ttf".into(), font_bytes());
    assert_eq!(
        fonts.document_face_count(),
        0,
        "the previous document's embedded face must not become installed"
    );
    assert_eq!(fonts.face_count(), 1);
}

#[test]
fn previous_pages_must_be_written_before_rendering_another_document() {
    let document = with_embedded_font(one_line(FontFace::new("Liberation Sans")), font_bytes());
    let mut fonts = FontBook::scan(&[]);
    let pages = ttoryeot::render(&document, &mut fonts);
    assert!(ttoryeot::pdf::write(pages, &mut fonts).is_ok());
    let pages = ttoryeot::render(&document, &mut fonts);

    let next = one_line(FontFace::new("함초롬바탕"));
    ttoryeot::render(&next, &mut fonts);
    assert!(
        ttoryeot::pdf::write(pages, &mut fonts).is_err(),
        "writing stale pages must fail instead of silently dropping glyphs"
    );
}

#[test]
fn previous_pages_are_not_redrawn_with_the_next_documents_embedded_face() {
    let first = with_embedded_font(one_line(FontFace::new("Liberation Sans")), font_bytes());
    let second = with_embedded_font(one_line(FontFace::new("Liberation Sans")), font_bytes());
    let mut fonts = FontBook::scan(&[]);
    let pages = ttoryeot::render(&first, &mut fonts);
    assert_eq!(pages[0].items.len(), 1);

    ttoryeot::render(&second, &mut fonts);
    assert!(
        ttoryeot::pdf::write(pages, &mut fonts).is_err(),
        "the next document's embedded face must not reuse the previous page's FaceId"
    );
}

#[test]
fn add_bytes_does_not_reuse_a_previous_pages_embedded_face_id() {
    let document = with_embedded_font(one_line(FontFace::new("Liberation Sans")), font_bytes());
    let mut fonts = FontBook::scan(&[]);
    let pages = ttoryeot::render(&document, &mut fonts);

    fonts.add_bytes("second.ttf".into(), font_bytes());
    assert!(
        ttoryeot::pdf::write(pages, &mut fonts).is_err(),
        "add_bytes must not let the stale page draw with the new installed face"
    );
}

#[test]
fn hwpx_non_embedded_font_ref_is_not_indexed() {
    let bytes = hwpx_with_font(&font_bytes(), false);
    let mut fonts = FontBook::scan(&[]);
    let output = ttoryeot::to_pdf(&bytes, None, &mut fonts).expect("render");
    assert!(output.pages >= 1);
    assert_eq!(fonts.document_face_count(), 0);
    assert_eq!(fonts.face_count(), 0);
}

/// HWPX 한 부를 메모리에 만든다. 글꼴은 `BinData/font.ttf`에 넣고 `isEmbedded`로 가리킨다.
fn embedded_hwpx(font: &[u8]) -> Vec<u8> {
    hwpx_with_font(font, true)
}

fn hwpx_with_font(font: &[u8], embedded: bool) -> Vec<u8> {
    let flag = if embedded { "1" } else { "0" };
    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" secCnt="1">
  <hh:fontfaces>
    <hh:fontface lang="HANGUL">
      <hh:font id="0" face="Liberation Sans" type="TTF" isEmbedded="{flag}" binaryItemIDRef="bin1"/>
    </hh:fontface>
  </hh:fontfaces>
  <hh:charProperties><hh:charPr id="0" height="1000"><hh:fontRef hangul="0"/></hh:charPr></hh:charProperties>
</hh:head>"#
    );
    let section = r#"<?xml version="1.0" encoding="UTF-8"?>
<hs:sec xmlns:hs="urn:hs"><hs:p><hs:run charPrIDRef="0"><hs:t>Embedded font</hs:t></hs:run></hs:p></hs:sec>"#;
    let content = br#"<?xml version="1.0" encoding="UTF-8"?>
<opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest>
  <opf:item id="section0" href="Contents/section0.xml"/>
  <opf:item id="bin1" href="BinData/font.ttf" media-type="application/x-font-ttf"/>
</opf:manifest></opf:package>"#;
    zip(&[
        ("mimetype", b"application/hwp+zip"),
        ("Contents/header.xml", header.as_bytes()),
        ("Contents/section0.xml", section.as_bytes()),
        ("Contents/content.hpf", content),
        ("BinData/font.ttf", font),
    ])
}

/// 압축하지 않는 ZIP. `MiniZip`은 CRC를 보지 않으므로 0으로 둔다.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in entries {
        let offset = output.len() as u32;
        let name = name.as_bytes();
        push_u32(&mut output, 0x0403_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, data.len() as u32);
        push_u32(&mut output, data.len() as u32);
        push_u16(&mut output, name.len() as u16);
        push_u16(&mut output, 0);
        output.extend_from_slice(name);
        output.extend_from_slice(data);

        push_u32(&mut directory, 0x0201_4b50);
        push_u16(&mut directory, 20);
        push_u16(&mut directory, 20);
        push_u16(&mut directory, 0);
        push_u16(&mut directory, 0);
        push_u16(&mut directory, 0);
        push_u16(&mut directory, 0);
        push_u32(&mut directory, 0);
        push_u32(&mut directory, data.len() as u32);
        push_u32(&mut directory, data.len() as u32);
        push_u16(&mut directory, name.len() as u16);
        push_u16(&mut directory, 0);
        push_u16(&mut directory, 0);
        push_u16(&mut directory, 0);
        push_u16(&mut directory, 0);
        push_u32(&mut directory, 0);
        push_u32(&mut directory, offset);
        directory.extend_from_slice(name);
    }
    let directory_offset = output.len() as u32;
    output.extend_from_slice(&directory);
    push_u32(&mut output, 0x0605_4b50);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(&mut output, entries.len() as u16);
    push_u16(&mut output, entries.len() as u16);
    push_u32(&mut output, directory.len() as u32);
    push_u32(&mut output, directory_offset);
    push_u16(&mut output, 0);
    output
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
