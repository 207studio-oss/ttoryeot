//! HWPX를 HWPX로 둔 채 짧은 글 한 군데를 바꾼다.
//!
//! 구역 XML을 다시 쓰지 않고 바꿀 글자의 바이트 범위만 갈아 끼운다. 글자 모양·문단 모양·줄 간격을
//! 정하는 속성과 나머지 항목은 그대로 남는다. 글자 수가 달라지면 그 문단에 저장된 줄 배치(`linesegarray`,
//! 줄마다 시작 글자 위치와 세로 자리)가 글과 맞지 않으므로 그 문단의 줄 배치만 지운다. 한/글과 또렷은 줄 배치가
//! 없는 문단을 열 때 글자 폭으로 다시 나눈다.
//!
//! 글 안의 줄 바꿈(`hp:lineBreak`)은 글자 `\n`으로 다룬다. 찾을 글과 바꿀 글에 `\n`을 넣어 줄 바꿈을 찾고, 넣고, 지운다.
//! 글이 없는 빈 문단은 찾을 글이 없으므로 구역과 문단 차례([`fill_empty_paragraph`])로 가리켜 글을 넣는다.

use std::collections::BTreeMap;
use std::fmt;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::dom;
use super::zip::MiniZip;
use super::{crypt, section_entries, Package};
use crate::report::Fatal;
use crate::ParseOptions;

const MAX_DEPTH: usize = 1024;
const CONTEXT_CHARS: usize = 12;
const PARAGRAPH_CHARS: usize = 40;

/// 글을 바꾸지 못한 까닭.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// HWPX 꾸러미가 아니거나 읽을 수 있는 구역이 없다.
    NotHwpx,
    /// 암호 문서는 이 길로 고치지 않는다.
    Encrypted,
    /// 변경 추적 표시가 있는 문서는 삭제된 글과 섞일 수 있어 고치지 않는다.
    TrackedChanges,
    EmptySearch,
    /// 바꿀 글에 제어 문자나 XML에 쓸 수 없는 문자가 있다. 줄 바꿈(`\n`)은 넣을 수 있고, 탭은 개체라 이 길로 넣지 않는다.
    InvalidReplacement,
    /// 글을 넣으려는 문단에 이미 글이 있다. 글이 있는 문단은 [`replace_text`]로 고친다.
    NotEmpty,
    NotFound,
    /// 찾는 글이 서로 다른 서식 조각이나 탭·줄 바꿈에 걸쳐 있다.
    SpansRuns,
    /// 한 조각 안에 하나 있고, 서식이 나뉜 곳에도 같은 글이 더 있다. 하나만 바꾸면 나머지를 놓친다.
    AlsoSpansRuns,
    /// 같은 글이 여러 군데 있다. `occurrence`로 몇 번째인지 정해야 한다.
    Ambiguous {
        count: usize,
    },
    OccurrenceOutOfRange {
        occurrence: usize,
        count: usize,
    },
    /// 새 꾸러미를 만들지 못했거나 만든 꾸러미가 HWPX로 읽히지 않는다.
    Write(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NotHwpx => write!(formatter, "HWPX 문서가 아니거나 읽을 수 있는 구역이 없습니다"),
            EditError::Encrypted => write!(formatter, "암호 문서는 글을 바꿀 수 없습니다"),
            EditError::TrackedChanges => {
                write!(formatter, "변경 추적 표시가 있는 문서는 글을 바꿀 수 없습니다")
            }
            EditError::EmptySearch => write!(formatter, "찾을 글이 비어 있습니다"),
            EditError::NotEmpty => write!(formatter, "이 문단에는 이미 글이 있습니다"),
            EditError::InvalidReplacement => {
                write!(formatter, "바꿀 글에 탭 같은 제어 문자가 있습니다")
            }
            EditError::NotFound => write!(formatter, "찾을 글이 문서에 없습니다"),
            EditError::SpansRuns => write!(
                formatter,
                "찾을 글이 서식이 다른 조각이나 탭·줄 바꿈에 걸쳐 있습니다. 한 조각 안의 더 짧은 글로 찾아 주세요"
            ),
            EditError::AlsoSpansRuns => write!(
                formatter,
                "같은 글이 서식이 나뉜 곳에도 더 있어 하나만 바꾸면 나머지를 놓칩니다. 앞뒤를 붙인 더 긴 글로 좁히거나 occurrence를 정해 주세요"
            ),
            EditError::Ambiguous { count } => write!(
                formatter,
                "같은 글이 {count}군데 있습니다. 몇 번째인지 정해 주세요 (occurrence 1~{count})"
            ),
            EditError::OccurrenceOutOfRange { occurrence, count } => write!(
                formatter,
                "{occurrence}번째 글은 없습니다. 같은 글은 {count}군데입니다"
            ),
            EditError::Write(detail) => write!(formatter, "고친 문서를 만들지 못했습니다: {detail}"),
        }
    }
}

impl std::error::Error for EditError {}

/// 찾은 글 한 군데.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub section: u32,
    /// 문서 차례(구역 순서, 그 안에서 위에서 아래)로 센 번호. 1부터 시작한다.
    pub occurrence: usize,
    /// 같은 서식 조각 안에서 찾은 글 앞뒤의 글. 각각 12자까지다.
    pub before: String,
    pub after: String,
    /// 찾은 글이 든 문단의 첫 40자.
    pub paragraph: String,
}

struct Segment {
    start: usize,
    end: usize,
    text: String,
    cdata: bool,
}

struct Unit {
    segments: Vec<Segment>,
    text: String,
    paragraph: usize,
    /// 글(`t`) 요소 이름의 앞붙이(예: `hp:`). 줄 바꿈 요소를 같은 앞붙이로 쓴다.
    prefix: String,
}

/// 문단의 마지막 `run`: 빈 문단에 글을 넣을 자리.
#[derive(Clone, Copy)]
enum RunSlot {
    /// `<hp:run …/>`. 태그의 바이트 범위.
    Empty { start: usize, end: usize },
    /// `<hp:run …>…</hp:run>`. 닫는 태그가 시작하는 바이트 위치.
    Open { close: usize },
}

#[derive(Default)]
struct Paragraph {
    text: String,
    /// 이 문단의 `linesegarray` 요소가 차지하는 바이트 범위. 없으면 `None`.
    layout: Option<(usize, usize)>,
    /// 이 문단 바로 아래의 마지막 `run`.
    run: Option<RunSlot>,
    /// `</hp:p>`가 시작하는 바이트 위치. 빈 요소 `<hp:p/>`면 `None`.
    close: Option<usize>,
    /// 문단(`p`) 요소 이름의 앞붙이(예: `hp:`).
    prefix: String,
}

struct SectionScan {
    index: u32,
    name: String,
    data: Vec<u8>,
    units: Vec<Unit>,
    paragraphs: Vec<Paragraph>,
    tracked: bool,
}

struct Hit {
    scan: usize,
    unit: usize,
    offset: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Paragraph,
    Run,
    Text,
    /// 문단의 `linesegarray`. 값은 시작 바이트 위치다.
    Layout(usize),
    Other,
}

/// 고친 바이트를 다시 읽어 확인할지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// 고친 구역 XML과 문서 전체를 다시 읽어 본다. 저장할 바이트는 이렇게 만든다.
    Full,
    /// 확인하지 않는다. 바로 다시 읽어 그리는 미리 보기처럼 부르는 쪽이 어차피 읽을 때만 쓴다.
    /// 구역 XML이 잘못돼도 파서는 그 구역을 건너뛰고 경고만 남기므로, 이렇게 만든 바이트는 저장하지 않는다.
    Skip,
}

/// 한 번 읽은 HWPX. 찾기와 바꾸기를 같은 읽기로 해, 큰 구역 XML을 여러 번 훑지 않는다.
pub struct Editor<'a> {
    zip: MiniZip<'a>,
    scans: Vec<SectionScan>,
}

impl<'a> Editor<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, EditError> {
        let (zip, scans) = open(bytes)?;
        Ok(Self { zip, scans })
    }

    /// 문서에서 `needle`이 있는 곳을 모두 문서 차례로 돌려준다.
    pub fn find(&self, needle: &str) -> Result<Vec<TextMatch>, EditError> {
        if needle.is_empty() {
            return Err(EditError::EmptySearch);
        }
        let hits = collect(&self.scans, needle);
        if hits.is_empty() {
            return Err(missing(&self.scans, needle));
        }
        Ok(hits
            .iter()
            .enumerate()
            .map(|(index, hit)| describe(&self.scans, hit, needle, index + 1))
            .collect())
    }

    /// [`replace_text`]와 같다. `check`로 다시 읽어 확인할지 정한다.
    pub fn replace(
        &self,
        needle: &str,
        replacement: &str,
        occurrence: Option<usize>,
        check: Check,
    ) -> Result<Vec<u8>, EditError> {
        if needle.is_empty() {
            return Err(EditError::EmptySearch);
        }
        check_replacement(replacement)?;
        let scans = &self.scans;
        if scans.iter().any(|scan| scan.tracked) {
            return Err(EditError::TrackedChanges);
        }
        let hits = collect(scans, needle);
        let count = hits.len();
        if count == 0 {
            return Err(missing(scans, needle));
        }
        let chosen = match occurrence {
            None if count == 1 && spanning_matches(scans, needle) > count => {
                return Err(EditError::AlsoSpansRuns);
            }
            None if count == 1 => 0,
            None => return Err(EditError::Ambiguous { count }),
            Some(number) if (1..=count).contains(&number) => number - 1,
            Some(number) => {
                return Err(EditError::OccurrenceOutOfRange {
                    occurrence: number,
                    count,
                })
            }
        };
        let hit = &hits[chosen];
        let scan = &scans[hit.scan];
        let patched = patch_section(scan, hit, needle, replacement);
        finish(&self.zip, scan, patched, check)
    }

    /// [`fill_empty_paragraph`]와 같다. `check`로 다시 읽어 확인할지 정한다.
    pub fn fill_empty_paragraph(
        &self,
        section: u32,
        ordinal: u32,
        text: &str,
        check: Check,
    ) -> Result<Vec<u8>, EditError> {
        check_replacement(text)?;
        let scans = &self.scans;
        if scans.iter().any(|scan| scan.tracked) {
            return Err(EditError::TrackedChanges);
        }
        let scan = scans
            .iter()
            .find(|scan| scan.index == section)
            .ok_or(EditError::NotFound)?;
        let index = ordinal as usize;
        let paragraph = scan.paragraphs.get(index).ok_or(EditError::NotFound)?;
        if !paragraph.text.is_empty() || scan.units.iter().any(|unit| unit.paragraph == index) {
            return Err(EditError::NotEmpty);
        }
        let p = &paragraph.prefix;
        let t = format!("<{p}t>{}</{p}t>", escape_text(text, p));
        let mut patches: Vec<(usize, usize, Vec<u8>)> = Vec::new();
        match (paragraph.run, paragraph.layout, paragraph.close) {
            (Some(RunSlot::Open { close }), layout, _) => {
                patches.push((close, close, t.into_bytes()));
                patches.extend(layout.map(|(from, to)| (from, to, Vec::new())));
            }
            (Some(RunSlot::Empty { start, end }), layout, _) => {
                let tag =
                    std::str::from_utf8(&scan.data[start..end]).map_err(|_| EditError::NotHwpx)?;
                let open = tag
                    .trim_end()
                    .strip_suffix("/>")
                    .ok_or(EditError::NotHwpx)?
                    .trim_end();
                patches.push((start, end, format!("{open}>{t}</{p}run>").into_bytes()));
                patches.extend(layout.map(|(from, to)| (from, to, Vec::new())));
            }
            // run이 없으면 줄 배치 자리(없으면 문단 끝)에 기본 글자 모양으로 하나 만든다.
            (None, Some((from, to)), _) => {
                patches.push((
                    from,
                    to,
                    format!("<{p}run charPrIDRef=\"0\">{t}</{p}run>").into_bytes(),
                ));
            }
            (None, None, Some(close)) => {
                patches.push((
                    close,
                    close,
                    format!("<{p}run charPrIDRef=\"0\">{t}</{p}run>").into_bytes(),
                ));
            }
            (None, None, None) => return Err(EditError::NotFound),
        }
        let patched = apply_patches(&scan.data, patches);
        finish(&self.zip, scan, patched, check)
    }
}

/// 문서에서 `needle`이 있는 곳을 모두 문서 차례로 돌려준다. 문서는 바꾸지 않는다.
pub fn find_text(bytes: &[u8], needle: &str) -> Result<Vec<TextMatch>, EditError> {
    if needle.is_empty() {
        return Err(EditError::EmptySearch);
    }
    Editor::open(bytes)?.find(needle)
}

/// `needle`이 든 곳 하나를 `replacement`로 바꾼 새 HWPX 바이트를 돌려준다.
///
/// 같은 글이 여러 군데면 `occurrence`(1부터)로 정해야 하고, 정하지 않으면 [`EditError::Ambiguous`]다.
/// 바꾸는 글자 말고 바뀌는 바이트는, 글자 수가 달라질 때 그 문단의 줄 배치(`linesegarray`)를 지운 것뿐이다.
pub fn replace_text(
    bytes: &[u8],
    needle: &str,
    replacement: &str,
    occurrence: Option<usize>,
) -> Result<Vec<u8>, EditError> {
    if needle.is_empty() {
        return Err(EditError::EmptySearch);
    }
    check_replacement(replacement)?;
    Editor::open(bytes)?.replace(needle, replacement, occurrence, Check::Full)
}

/// 글이 없는 빈 문단에 글을 넣은 새 HWPX 바이트를 돌려준다. 문단은 구역 번호(`section0.xml`이면 0)와 그 구역 XML에서
/// 문단(`p`) 요소가 나오는 차례(0부터, 표 칸 안 문단도 센다)로 가리킨다. 파서가 문단마다 적는
/// [`ParagraphOrigin`](crate::ParagraphOrigin)과 같은 값이다. 글은 문단의 마지막 `run`에 넣고, 그 문단의 줄 배치는
/// 지워 다시 나누게 한다. `text`의 `\n`은 줄 바꿈이 된다.
pub fn fill_empty_paragraph(
    bytes: &[u8],
    section: u32,
    ordinal: u32,
    text: &str,
) -> Result<Vec<u8>, EditError> {
    if text.is_empty() {
        return Ok(bytes.to_vec());
    }
    check_replacement(text)?;
    Editor::open(bytes)?.fill_empty_paragraph(section, ordinal, text, Check::Full)
}

/// 바꿀 글에 쓸 수 없는 문자가 있는지 본다. 줄 바꿈(`\n`)은 줄 바꿈 요소로 넣는다.
fn check_replacement(text: &str) -> Result<(), EditError> {
    if text
        .chars()
        .any(|ch| (ch.is_control() && ch != '\n') || matches!(ch, '\u{FFFE}' | '\u{FFFF}'))
    {
        return Err(EditError::InvalidReplacement);
    }
    Ok(())
}

/// 고친 구역 XML을 꾸러미에 넣고, XML과 문서로 읽히는지 확인한 새 바이트를 돌려준다.
fn finish(
    zip: &MiniZip<'_>,
    scan: &SectionScan,
    patched: Vec<u8>,
    check: Check,
) -> Result<Vec<u8>, EditError> {
    if check == Check::Full && dom::parse(&patched).is_none() {
        return Err(EditError::Write(
            "고친 구역이 XML로 읽히지 않습니다".to_string(),
        ));
    }
    let mut replaced = BTreeMap::new();
    replaced.insert(scan.name.clone(), patched);
    let output = zip
        .rewrite(&replaced)
        .map_err(|error| EditError::Write(format!("{error:?}")))?;
    if check == Check::Full {
        super::parse(&output, &ParseOptions::default()).map_err(|error| {
            EditError::Write(format!("고친 문서를 다시 읽지 못했습니다: {error:?}"))
        })?;
    }
    Ok(output)
}

/// 바이트 범위를 뒤에서부터 갈아 끼운다. 범위는 겹치지 않아야 한다.
fn apply_patches(data: &[u8], mut patches: Vec<(usize, usize, Vec<u8>)>) -> Vec<u8> {
    patches.sort_by_key(|patch| std::cmp::Reverse(patch.0));
    let mut output = data.to_vec();
    for (from, to, content) in patches {
        output.splice(from..to, content);
    }
    output
}

fn open(bytes: &[u8]) -> Result<(MiniZip<'_>, Vec<SectionScan>), EditError> {
    let zip = MiniZip::new(bytes).ok_or(EditError::NotHwpx)?;
    match crypt::unlock(&zip, None, &mut Vec::new()) {
        Ok(_) => {}
        Err(Fatal::CorruptDocument(_)) => return Err(EditError::NotHwpx),
        Err(_) => return Err(EditError::Encrypted),
    }
    let package = Package {
        zip,
        opened: BTreeMap::new(),
    };
    let sections = section_entries(&package);
    if sections.is_empty() {
        return Err(EditError::NotHwpx);
    }
    let mut scans = Vec::new();
    for (index, name) in sections {
        let data = package.read(&name).ok_or(EditError::NotHwpx)?;
        scans.push(scan_section(index, name, data)?);
    }
    Ok((package.zip, scans))
}

fn collect(scans: &[SectionScan], needle: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (scan_index, scan) in scans.iter().enumerate() {
        for (unit_index, unit) in scan.units.iter().enumerate() {
            for (offset, _) in unit.text.match_indices(needle) {
                hits.push(Hit {
                    scan: scan_index,
                    unit: unit_index,
                    offset,
                });
            }
        }
    }
    hits
}

fn spanning_matches(scans: &[SectionScan], needle: &str) -> usize {
    scans
        .iter()
        .flat_map(|scan| &scan.paragraphs)
        .map(|paragraph| paragraph.text.matches(needle).count())
        .sum()
}

fn missing(scans: &[SectionScan], needle: &str) -> EditError {
    let spanning = scans
        .iter()
        .flat_map(|scan| &scan.paragraphs)
        .any(|paragraph| paragraph.text.contains(needle));
    if spanning {
        EditError::SpansRuns
    } else {
        EditError::NotFound
    }
}

fn describe(scans: &[SectionScan], hit: &Hit, needle: &str, occurrence: usize) -> TextMatch {
    let scan = &scans[hit.scan];
    let unit = &scan.units[hit.unit];
    let before: Vec<char> = unit.text[..hit.offset].chars().collect();
    let after = &unit.text[hit.offset + needle.len()..];
    TextMatch {
        section: scan.index,
        occurrence,
        before: before[before.len().saturating_sub(CONTEXT_CHARS)..]
            .iter()
            .collect(),
        after: after.chars().take(CONTEXT_CHARS).collect(),
        paragraph: scan.paragraphs[unit.paragraph]
            .text
            .chars()
            .take(PARAGRAPH_CHARS)
            .collect(),
    }
}

fn patch_section(scan: &SectionScan, hit: &Hit, needle: &str, replacement: &str) -> Vec<u8> {
    let unit = &scan.units[hit.unit];
    let mut patches: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    patches.push(text_patch(
        scan,
        unit,
        hit.offset,
        needle.len(),
        replacement,
    ));
    // 글자 수가 바뀌면 저장된 줄 나눔은 옛 글의 것이다. 시작 위치만 옮기면 한 줄에 글이 넘치거나 모자란다.
    // 그 문단의 줄 배치를 지워, 여는 쪽이 글자 폭으로 다시 나누게 한다. 다른 문단의 줄 배치는 그대로 둔다.
    // 줄 바꿈을 넣거나 지워도 줄이 달라진다.
    let breaks = needle.contains('\n') || replacement.contains('\n');
    if breaks || utf16_len(replacement) != utf16_len(needle) {
        if let Some((from, to)) = scan.paragraphs[unit.paragraph].layout {
            patches.push((from, to, Vec::new()));
        }
    }
    apply_patches(&scan.data, patches)
}

fn text_patch(
    scan: &SectionScan,
    unit: &Unit,
    offset: usize,
    length: usize,
    replacement: &str,
) -> (usize, usize, Vec<u8>) {
    let end = offset + length;
    let mut cursor = 0;
    let mut first = None;
    let mut last = None;
    for (index, segment) in unit.segments.iter().enumerate() {
        let next = cursor + segment.text.len();
        if first.is_none() && offset < next {
            first = Some((index, cursor));
        }
        if end <= next {
            last = Some((index, cursor));
            break;
        }
        cursor = next;
    }
    let (first_index, first_cursor) = first.expect("match inside unit");
    let (last_index, last_cursor) = last.expect("match inside unit");
    let head = &unit.segments[first_index];
    let tail = &unit.segments[last_index];

    let mut content = Vec::new();
    let prefix = &head.text[..offset - first_cursor];
    if head.cdata {
        content.extend_from_slice(escape(prefix).as_bytes());
    } else {
        content.extend_from_slice(&scan.data[head.start..head.start + prefix.len()]);
    }
    content.extend_from_slice(escape_text(replacement, &unit.prefix).as_bytes());
    let suffix = &tail.text[end - last_cursor..];
    if tail.cdata {
        content.extend_from_slice(escape(suffix).as_bytes());
    } else {
        content.extend_from_slice(&scan.data[tail.end - suffix.len()..tail.end]);
    }
    (head.start, tail.end, content)
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// 넣을 글을 XML로 쓴다. `\n`은 같은 앞붙이의 줄 바꿈 요소가 된다.
fn escape_text(text: &str, prefix: &str) -> String {
    escape(text).replace('\n', &format!("<{prefix}lineBreak/>"))
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn scan_section(index: u32, name: String, data: Vec<u8>) -> Result<SectionScan, EditError> {
    let malformed = || EditError::NotHwpx;
    let mut reader = Reader::from_reader(data.as_slice());
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    // 열려 있는 문단의 번호. 표 칸 안의 문단은 바깥 문단 위에 쌓인다.
    let mut open_paragraphs: Vec<usize> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut tracked = false;
    // 지금 글(`t`) 요소 이름의 앞붙이.
    let mut text_prefix = String::new();

    loop {
        let before = usize::try_from(reader.buffer_position()).map_err(|_| malformed())?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| malformed())?;
        let after = usize::try_from(reader.buffer_position()).map_err(|_| malformed())?;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                let local = local_name(element.name().local_name().into_inner().as_bytes())?;
                let qualified = element.name().as_ref().to_string();
                let prefix = qualified[..qualified.len() - local.len()].to_string();
                let starts = matches!(event, Event::Start(_));
                if matches!(local.as_str(), "deleteBegin" | "insertBegin") {
                    tracked = true;
                }
                // 글 안의 줄 바꿈은 글자 `\n` 하나로 이어 둔다. 바꿀 글이 줄 바꿈을 넘어도 한 조각이다.
                if !starts && local == "lineBreak" && frames.last() == Some(&Frame::Text) {
                    segments.push(Segment {
                        start: before,
                        end: after,
                        text: "\n".to_string(),
                        cdata: false,
                    });
                    buffer.clear();
                    continue;
                }
                if frames.last() == Some(&Frame::Text) {
                    finish_unit(
                        &mut segments,
                        &mut units,
                        &mut paragraphs,
                        &open_paragraphs,
                        &text_prefix,
                    );
                }
                if !starts && local == "p" {
                    // 빈 요소 문단도 파서처럼 센다.
                    paragraphs.push(Paragraph {
                        prefix: prefix.clone(),
                        ..Paragraph::default()
                    });
                }
                if !starts && local == "run" && frames.last() == Some(&Frame::Paragraph) {
                    if let Some(&paragraph) = open_paragraphs.last() {
                        paragraphs[paragraph].run = Some(RunSlot::Empty {
                            start: before,
                            end: after,
                        });
                    }
                }
                let layout = local == "linesegarray" && frames.last() == Some(&Frame::Paragraph);
                if layout && !starts {
                    if let Some(&paragraph) = open_paragraphs.last() {
                        paragraphs[paragraph].layout = Some((before, after));
                    }
                }
                if starts {
                    if frames.len() >= MAX_DEPTH {
                        return Err(malformed());
                    }
                    let in_paragraph = !open_paragraphs.is_empty();
                    let frame = match local.as_str() {
                        "p" => {
                            paragraphs.push(Paragraph {
                                prefix: prefix.clone(),
                                ..Paragraph::default()
                            });
                            open_paragraphs.push(paragraphs.len() - 1);
                            Frame::Paragraph
                        }
                        "run" => Frame::Run,
                        "t" if in_paragraph && frames.last() == Some(&Frame::Run) => {
                            text_prefix = prefix.clone();
                            Frame::Text
                        }
                        _ if layout => Frame::Layout(before),
                        _ => Frame::Other,
                    };
                    frames.push(frame);
                }
            }
            Event::End(_) => match frames.pop().ok_or_else(malformed)? {
                Frame::Text => finish_unit(
                    &mut segments,
                    &mut units,
                    &mut paragraphs,
                    &open_paragraphs,
                    &text_prefix,
                ),
                Frame::Paragraph => {
                    if let Some(paragraph) = open_paragraphs.pop() {
                        paragraphs[paragraph].close = Some(before);
                    }
                }
                Frame::Run if frames.last() == Some(&Frame::Paragraph) => {
                    if let Some(&paragraph) = open_paragraphs.last() {
                        paragraphs[paragraph].run = Some(RunSlot::Open { close: before });
                    }
                }
                Frame::Layout(start) => {
                    if let Some(&paragraph) = open_paragraphs.last() {
                        paragraphs[paragraph].layout = Some((start, after));
                    }
                }
                _ => {}
            },
            Event::Text(text) if frames.last() == Some(&Frame::Text) => {
                let decoded: &str = text.as_ref();
                segments.push(Segment {
                    start: before,
                    end: after,
                    text: decoded.to_string(),
                    cdata: false,
                });
            }
            Event::CData(text) if frames.last() == Some(&Frame::Text) => {
                let decoded: &str = text.as_ref();
                segments.push(Segment {
                    start: before,
                    end: after,
                    text: decoded.to_string(),
                    cdata: true,
                });
            }
            Event::GeneralRef(reference) if frames.last() == Some(&Frame::Text) => {
                let name: &str = reference.as_ref();
                let value = dom::resolve_reference(name).ok_or_else(malformed)?;
                segments.push(Segment {
                    start: before,
                    end: after,
                    text: value,
                    cdata: false,
                });
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if !frames.is_empty() {
        return Err(malformed());
    }
    Ok(SectionScan {
        index,
        name,
        data,
        units,
        paragraphs,
        tracked,
    })
}

fn finish_unit(
    segments: &mut Vec<Segment>,
    units: &mut Vec<Unit>,
    paragraphs: &mut [Paragraph],
    open_paragraphs: &[usize],
    prefix: &str,
) {
    if segments.is_empty() {
        return;
    }
    let Some(&paragraph) = open_paragraphs.last() else {
        segments.clear();
        return;
    };
    let segments = std::mem::take(segments);
    let text: String = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    paragraphs[paragraph].text.push_str(&text);
    units.push(Unit {
        segments,
        text,
        paragraph,
        prefix: prefix.to_string(),
    });
}

fn local_name(raw: &[u8]) -> Result<String, EditError> {
    std::str::from_utf8(raw)
        .map(str::to_string)
        .map_err(|_| EditError::NotHwpx)
}
