//! HWPX를 HWPX로 둔 채 짧은 글 한 군데를 바꾼다.
//!
//! 구역 XML을 다시 쓰지 않고 바꿀 글자의 바이트 범위만 갈아 끼운다. 글자 모양·문단 모양·줄 간격을
//! 정하는 속성과 나머지 항목은 그대로 남는다. 글자 수가 달라져 줄 시작 위치(`textpos`)가 밀려야 하면
//! 위치를 정확히 셀 수 있을 때만 옮기고, 셀 수 없으면 서식을 건드리지 않고 오류로 멈춘다.

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
    /// 바꿀 글에 제어 문자나 XML에 쓸 수 없는 문자가 있다. 탭과 줄 바꿈은 글자가 아니라 개체라 이 길로 넣지 않는다.
    InvalidReplacement,
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
    /// 글자 수가 달라지는데 줄 시작 위치를 맞출 수 없다. 같은 길이로 바꾸면 고칠 수 있다.
    LayoutBound {
        section: u32,
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
            EditError::InvalidReplacement => {
                write!(formatter, "바꿀 글에 탭이나 줄 바꿈 같은 제어 문자가 있습니다")
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
            EditError::LayoutBound { section } => write!(
                formatter,
                "구역 {section}의 이 문단은 줄 나눔 위치가 글자 수에 묶여 있어 글자 수를 바꿀 수 없습니다. 글자 수가 같게 바꿔 주세요"
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
    prefix: Option<usize>,
}

#[derive(Default)]
struct Paragraph {
    text: String,
    linesegs: Vec<(String, (usize, usize))>,
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
    Other,
}

struct Open {
    paragraph: usize,
    pure: bool,
    position: usize,
}

/// 문서에서 `needle`이 있는 곳을 모두 문서 차례로 돌려준다. 문서는 바꾸지 않는다.
pub fn find_text(bytes: &[u8], needle: &str) -> Result<Vec<TextMatch>, EditError> {
    if needle.is_empty() {
        return Err(EditError::EmptySearch);
    }
    let (_, scans) = open(bytes)?;
    let hits = collect(&scans, needle);
    if hits.is_empty() {
        return Err(missing(&scans, needle));
    }
    Ok(hits
        .iter()
        .enumerate()
        .map(|(index, hit)| describe(&scans, hit, needle, index + 1))
        .collect())
}

/// `needle`이 든 곳 하나를 `replacement`로 바꾼 새 HWPX 바이트를 돌려준다.
///
/// 같은 글이 여러 군데면 `occurrence`(1부터)로 정해야 하고, 정하지 않으면 [`EditError::Ambiguous`]다.
/// 바꾸는 글자 말고 바뀌는 바이트는 줄 시작 위치 값뿐이며, 그것도 글자 수가 달라질 때만이다.
pub fn replace_text(
    bytes: &[u8],
    needle: &str,
    replacement: &str,
    occurrence: Option<usize>,
) -> Result<Vec<u8>, EditError> {
    if needle.is_empty() {
        return Err(EditError::EmptySearch);
    }
    if replacement
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '\u{FFFE}' | '\u{FFFF}'))
    {
        return Err(EditError::InvalidReplacement);
    }
    let (zip, scans) = open(bytes)?;
    if scans.iter().any(|scan| scan.tracked) {
        return Err(EditError::TrackedChanges);
    }
    let hits = collect(&scans, needle);
    let count = hits.len();
    if count == 0 {
        return Err(missing(&scans, needle));
    }
    let chosen = match occurrence {
        None if count == 1 && spanning_matches(&scans, needle) > count => {
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
    let patched = patch_section(scan, hit, needle, replacement)?;
    if dom::parse(&patched).is_none() {
        return Err(EditError::Write(
            "고친 구역이 XML로 읽히지 않습니다".to_string(),
        ));
    }
    let mut replaced = BTreeMap::new();
    replaced.insert(scan.name.clone(), patched);
    let output = zip
        .rewrite(&replaced)
        .map_err(|error| EditError::Write(format!("{error:?}")))?;
    super::parse(&output, &ParseOptions::default()).map_err(|error| {
        EditError::Write(format!("고친 문서를 다시 읽지 못했습니다: {error:?}"))
    })?;
    Ok(output)
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

fn patch_section(
    scan: &SectionScan,
    hit: &Hit,
    needle: &str,
    replacement: &str,
) -> Result<Vec<u8>, EditError> {
    let unit = &scan.units[hit.unit];
    let mut patches: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    patches.push(text_patch(
        scan,
        unit,
        hit.offset,
        needle.len(),
        replacement,
    ));

    let delta = utf16_len(replacement) as i64 - utf16_len(needle) as i64;
    let paragraph = &scan.paragraphs[unit.paragraph];
    if delta != 0 && paragraph.linesegs.len() > 1 {
        let bound = EditError::LayoutBound {
            section: scan.index,
        };
        let start = unit.prefix.ok_or_else(|| bound.clone())? + utf16_len(&unit.text[..hit.offset]);
        let end = start + utf16_len(needle);
        for (value, (from, to)) in &paragraph.linesegs {
            let position: usize = value.parse().map_err(|_| bound.clone())?;
            if position <= start {
                continue;
            }
            if position < end {
                return Err(bound);
            }
            let shifted = i64::try_from(position).map_err(|_| bound.clone())? + delta;
            let shifted = u32::try_from(shifted).map_err(|_| bound.clone())?;
            patches.push((*from, *to, shifted.to_string().into_bytes()));
        }
    }

    patches.sort_by_key(|patch| std::cmp::Reverse(patch.0));
    let mut output = scan.data.clone();
    for (from, to, content) in patches {
        output.splice(from..to, content);
    }
    Ok(output)
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
    content.extend_from_slice(escape(replacement).as_bytes());
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

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn scan_section(index: u32, name: String, data: Vec<u8>) -> Result<SectionScan, EditError> {
    let malformed = || EditError::NotHwpx;
    let mut reader = Reader::from_reader(data.as_slice());
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut open_paragraphs: Vec<Open> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut tracked = false;

    loop {
        let before = usize::try_from(reader.buffer_position()).map_err(|_| malformed())?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| malformed())?;
        let after = usize::try_from(reader.buffer_position()).map_err(|_| malformed())?;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                let local = local_name(element.name().local_name().into_inner().as_bytes())?;
                let starts = matches!(event, Event::Start(_));
                if matches!(local.as_str(), "deleteBegin" | "insertBegin") {
                    tracked = true;
                }
                if frames.last() == Some(&Frame::Text) {
                    finish_unit(
                        &mut segments,
                        &mut units,
                        &mut paragraphs,
                        &mut open_paragraphs,
                    );
                    if let Some(open) = open_paragraphs.last_mut() {
                        open.pure = false;
                    }
                } else if frames.len() >= 2
                    && frames[frames.len() - 1] == Frame::Run
                    && frames[frames.len() - 2] == Frame::Paragraph
                    && local != "t"
                {
                    if let Some(open) = open_paragraphs.last_mut() {
                        open.pure = false;
                    }
                }
                if local == "lineseg" {
                    if let Some(open) = open_paragraphs.last() {
                        if let Some(range) = attribute_range(&data[before..after], "textpos") {
                            let value =
                                std::str::from_utf8(&data[before + range.0..before + range.1])
                                    .unwrap_or("")
                                    .to_string();
                            paragraphs[open.paragraph]
                                .linesegs
                                .push((value, (before + range.0, before + range.1)));
                        }
                    }
                }
                if starts {
                    if frames.len() >= MAX_DEPTH {
                        return Err(malformed());
                    }
                    let in_paragraph = !open_paragraphs.is_empty();
                    let frame = match local.as_str() {
                        "p" => {
                            paragraphs.push(Paragraph::default());
                            open_paragraphs.push(Open {
                                paragraph: paragraphs.len() - 1,
                                pure: true,
                                position: 0,
                            });
                            Frame::Paragraph
                        }
                        "run" => Frame::Run,
                        "t" if in_paragraph && frames.last() == Some(&Frame::Run) => Frame::Text,
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
                    &mut open_paragraphs,
                ),
                Frame::Paragraph => {
                    open_paragraphs.pop();
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
    open_paragraphs: &mut [Open],
) {
    if segments.is_empty() {
        return;
    }
    let Some(open) = open_paragraphs.last_mut() else {
        segments.clear();
        return;
    };
    let segments = std::mem::take(segments);
    let text: String = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    let prefix = open.pure.then_some(open.position);
    open.position += utf16_len(&text);
    paragraphs[open.paragraph].text.push_str(&text);
    units.push(Unit {
        segments,
        text,
        paragraph: open.paragraph,
        prefix,
    });
}

fn local_name(raw: &[u8]) -> Result<String, EditError> {
    std::str::from_utf8(raw)
        .map(str::to_string)
        .map_err(|_| EditError::NotHwpx)
}

fn attribute_range(tag: &[u8], name: &str) -> Option<(usize, usize)> {
    let key = name.as_bytes();
    let mut from = 0;
    while from + key.len() <= tag.len() {
        let found = tag[from..]
            .windows(key.len())
            .position(|window| window == key)?;
        let at = from + found;
        from = at + key.len();
        if at == 0 || !tag[at - 1].is_ascii_whitespace() {
            continue;
        }
        let mut cursor = from;
        while tag.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if tag.get(cursor) != Some(&b'=') {
            continue;
        }
        cursor += 1;
        while tag.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let quote = *tag.get(cursor)?;
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        let start = cursor + 1;
        let length = tag[start..].iter().position(|byte| *byte == quote)?;
        return Some((start, start + length));
    }
    None
}
