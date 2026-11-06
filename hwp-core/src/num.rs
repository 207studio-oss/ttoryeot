use std::collections::BTreeMap;

use crate::model::{Bullet, Caption, Document, Numbering, ParaPr, Paragraph, RunItem};
use crate::report::{Warning, WarningKind};

pub const FORMAT_NAMES: [&str; 17] = [
    "DIGIT",
    "CIRCLED_DIGIT",
    "ROMAN_CAPITAL",
    "ROMAN_SMALL",
    "LATIN_CAPITAL",
    "LATIN_SMALL",
    "CIRCLED_LATIN_CAPITAL",
    "CIRCLED_LATIN_SMALL",
    "HANGUL_SYLLABLE",
    "CIRCLED_HANGUL_SYLLABLE",
    "HANGUL_JAMO",
    "CIRCLED_HANGUL_JAMO",
    "HANGUL_PHONETIC",
    "IDEOGRAPH",
    "CIRCLED_IDEOGRAPH",
    "DECAGON_CIRCLE",
    "DECAGON_CIRCLE_HANJA",
];

pub fn format_name(code: u8) -> Option<&'static str> {
    match code {
        0x80 => Some("SYMBOL"),
        0x81 => Some("USER_CHAR"),
        _ => FORMAT_NAMES.get(usize::from(code)).copied(),
    }
}

pub fn format_code(name: &str) -> Option<u8> {
    match name {
        "SYMBOL" => Some(0x80),
        "USER_CHAR" => Some(0x81),
        _ => FORMAT_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .map(|n| n as u8),
    }
}

pub(crate) fn hwp5_format(attr: u32, warnings: &mut Vec<Warning>) -> &'static str {
    let code = ((attr >> 5) & 0x1F) as u8;
    format_name(code).unwrap_or_else(|| {
        warnings.push(Warning::new(
            WarningKind::UnknownNumberFormat,
            None,
            format!("number format code {code}"),
        ));
        "DIGIT"
    })
}

pub(crate) fn hwpx_format(name: &str, warnings: &mut Vec<Warning>) -> String {
    if format_code(name).is_some() {
        name.to_string()
    } else {
        warnings.push(Warning::new(
            WarningKind::UnknownNumberFormat,
            None,
            "unknown numbering numFormat",
        ));
        "DIGIT".to_string()
    }
}

pub fn format_number(number: i64, format: &str) -> String {
    if matches!(format, "SYMBOL" | "USER_CHAR") {
        return String::new();
    }
    if number <= 0 {
        return number.to_string();
    }
    let glyph = match format {
        "CIRCLED_DIGIT" if number <= 20 => range_char(number, 0x2460, 20),
        "CIRCLED_DIGIT" if number <= 35 => range_char(number - 20, 0x3251, 15),
        "CIRCLED_DIGIT" => range_char(number - 35, 0x32B1, 15),
        "ROMAN_CAPITAL" | "ROMAN_SMALL" => roman(number).map(|text| {
            if format == "ROMAN_SMALL" {
                text.to_lowercase()
            } else {
                text
            }
        }),
        "LATIN_CAPITAL" => range_char(number, 0x41, 26),
        "LATIN_SMALL" => range_char(number, 0x61, 26),
        "CIRCLED_LATIN_CAPITAL" => range_char(number, 0x24B6, 26),
        "CIRCLED_LATIN_SMALL" => range_char(number, 0x24D0, 26),
        "HANGUL_SYLLABLE" => hangul_syllable(number),
        "CIRCLED_HANGUL_SYLLABLE" => range_char(number, 0x326E, 14),
        "HANGUL_JAMO" => sequence(number, "ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎ"),
        "CIRCLED_HANGUL_JAMO" => range_char(number, 0x3260, 14),
        "HANGUL_PHONETIC" => positional(number, "일이삼사오육칠팔구", "십백천"),
        "IDEOGRAPH" => positional(number, "一二三四五六七八九", "十百千"),
        "CIRCLED_IDEOGRAPH" => range_char(number, 0x3280, 10),
        "DECAGON_CIRCLE" => sequence(number, "갑을병정무기경신임계"),
        "DECAGON_CIRCLE_HANJA" => sequence(number, "甲乙丙丁戊己庚辛壬癸"),
        _ => None,
    };
    glyph.unwrap_or_else(|| number.to_string())
}

fn range_char(number: i64, first: u32, count: i64) -> Option<String> {
    (1..=count)
        .contains(&number)
        .then_some(number)
        .and_then(|n| char::from_u32(first + n as u32 - 1))
        .map(String::from)
}

fn sequence(number: i64, chars: &str) -> Option<String> {
    usize::try_from(number - 1)
        .ok()
        .and_then(|n| chars.chars().nth(n))
        .map(String::from)
}

fn hangul_syllable(number: i64) -> Option<String> {
    const INITIALS: [u32; 14] = [0, 2, 3, 5, 6, 7, 9, 11, 12, 14, 15, 16, 17, 18];
    const VOWELS: [u32; 6] = [0, 4, 8, 13, 18, 20];
    let index = usize::try_from(number - 1).ok()?;
    let initial = INITIALS.get(index % 14)?;
    let vowel = VOWELS.get(index / 14)?;
    char::from_u32(0xAC00 + initial * 588 + vowel * 28).map(String::from)
}

fn roman(mut number: i64) -> Option<String> {
    if number > 3999 {
        return None;
    }
    let mut text = String::new();
    for (value, symbol) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while number >= value {
            text.push_str(symbol);
            number -= value;
        }
    }
    Some(text)
}

fn positional(number: i64, digits: &str, units: &str) -> Option<String> {
    if number > 9999 {
        return None;
    }
    let mut text = String::new();
    for (index, divisor) in [1, 10, 100, 1000].into_iter().enumerate().rev() {
        let digit = (number / divisor) % 10;
        if digit == 0 {
            continue;
        }
        if digit != 1 || index == 0 {
            text.push_str(&sequence(digit, digits)?);
        }
        if index > 0 {
            text.push_str(&sequence(index as i64, units)?);
        }
    }
    Some(text)
}

pub fn resolve_heads(document: &mut Document) {
    let mut resolver = HeadResolver {
        para_prs: document.para_prs.iter().map(|p| (p.id, p)).collect(),
        numberings: document.numberings.iter().map(|n| (n.id, n)).collect(),
        bullets: document.bullets.iter().map(|b| (b.id, b)).collect(),
        counters: BTreeMap::new(),
        outline_numbering_id: None,
        header: None,
        footer: None,
    };
    for section in &mut document.sections {
        resolver.outline_numbering_id = section.outline_numbering_id;
        resolver.header = None;
        resolver.footer = None;
        resolver.paragraphs(&mut section.paragraphs);
        if let Some(header) = resolver.header.take() {
            section.header = header;
        } else {
            resolver.paragraphs(&mut section.header);
        }
        if let Some(footer) = resolver.footer.take() {
            section.footer = footer;
        } else {
            resolver.paragraphs(&mut section.footer);
        }
    }
}

struct HeadResolver<'a> {
    para_prs: BTreeMap<i32, &'a ParaPr>,
    numberings: BTreeMap<i32, &'a Numbering>,
    bullets: BTreeMap<i32, &'a Bullet>,
    counters: BTreeMap<i32, [i64; 7]>,
    outline_numbering_id: Option<i32>,
    header: Option<Vec<Paragraph>>,
    footer: Option<Vec<Paragraph>>,
}

impl HeadResolver<'_> {
    fn head(&mut self, paragraph: &Paragraph) -> Option<String> {
        let para_pr = self.para_prs.get(&paragraph.para_pr_id)?;
        if para_pr.heading_type == "BULLET" {
            let bullet = self.bullets.get(&para_pr.heading_id_ref)?;
            return Some(bullet.char.replace('\u{00AD}', "-"));
        }
        if !matches!(para_pr.heading_type.as_str(), "NUMBER" | "OUTLINE") {
            return None;
        }
        let level = usize::try_from(para_pr.heading_level)
            .ok()
            .filter(|n| *n < 7)?;
        let numbering = self
            .numberings
            .get(&para_pr.numbering_id(self.outline_numbering_id))?;
        let head = numbering
            .heads
            .iter()
            .find(|h| h.level == level as i32 + 1)?;
        let starts = std::array::from_fn(|index| {
            numbering
                .heads
                .iter()
                .find(|h| h.level == index as i32 + 1)
                .map(|h| i64::from(h.start) - 1)
                .unwrap_or(0)
        });
        let counters = self.counters.entry(numbering.id).or_insert(starts);
        if let Some(counter) = counters.get_mut(level) {
            *counter = counter.saturating_add(1);
        }
        for (counter, start) in counters.iter_mut().zip(starts).skip(level + 1) {
            *counter = start;
        }
        let mut text = String::new();
        let mut chars = head.text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '^' {
                if let Some(index) = chars
                    .peek()
                    .and_then(|c| c.to_digit(10))
                    .filter(|n| (1..=7).contains(n))
                {
                    chars.next();
                    let value = counters.get(index as usize - 1).copied().unwrap_or(0);
                    let format = numbering
                        .heads
                        .iter()
                        .find(|h| h.level == index as i32)
                        .map(|h| h.format.as_str())
                        .unwrap_or("DIGIT");
                    text.push_str(&format_number(value, format));
                    continue;
                }
            }
            text.push(ch);
        }
        Some(text)
    }

    fn paragraphs(&mut self, paragraphs: &mut [Paragraph]) {
        for paragraph in paragraphs {
            paragraph.head_text = self.head(paragraph);
            for run in &mut paragraph.runs {
                for item in &mut run.items {
                    self.item(item);
                }
            }
            for memo in &mut paragraph.memos {
                self.paragraphs(&mut memo.paragraphs);
            }
        }
    }

    fn caption(&mut self, caption: &mut Option<Caption>) {
        if let Some(caption) = caption {
            self.paragraphs(&mut caption.paragraphs);
        }
    }

    fn item(&mut self, item: &mut RunItem) {
        match item {
            RunItem::Table(table) => {
                self.caption(&mut table.caption);
                for cell in &mut table.cells {
                    self.paragraphs(&mut cell.paragraphs);
                }
            }
            RunItem::Picture(picture) => self.caption(&mut picture.caption),
            RunItem::Shape(shape) => {
                self.caption(&mut shape.caption);
                self.paragraphs(&mut shape.paragraphs);
                for child in &mut shape.children {
                    self.item(&mut child.item);
                }
            }
            RunItem::Note(note) => self.paragraphs(&mut note.paragraphs),
            RunItem::Header { paragraphs, .. } => {
                self.paragraphs(paragraphs);
                if self.header.is_none() && !paragraphs.is_empty() {
                    self.header = Some(paragraphs.clone());
                }
            }
            RunItem::Footer { paragraphs, .. } => {
                self.paragraphs(paragraphs);
                if self.footer.is_none() && !paragraphs.is_empty() {
                    self.footer = Some(paragraphs.clone());
                }
            }
            RunItem::Text(_)
            | RunItem::Tab
            | RunItem::SizedTab(_)
            | RunItem::FormattedTab(_)
            | RunItem::LineBreak
            | RunItem::AutoNum(_)
            | RunItem::Control(_)
            | RunItem::Compose(_)
            | RunItem::Skip(_)
            | RunItem::PageHiding { .. }
            | RunItem::NewNum(_)
            | RunItem::PageNum { .. } => {}
        }
    }
}
