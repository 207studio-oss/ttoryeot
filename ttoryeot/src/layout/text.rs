//! 글자 흐름(atom), 글자 모양, 줄 하나의 글자 자리(탭·줄 맞춤), 그리기.

use super::object::DEVICE_UNIT;
use super::{Renderer, object_pos, pt};
use crate::fonts::{FaceId, GlyphInfo};
use crate::scene::{self, GlyphRun, Item, PathOp};
use hwp_core::{
    CharPr, Compose, FIXED_SPACE, FontFace, LineSeg, NO_BREAK_SPACE, Note, ParaPr, Paragraph,
    RunItem, TabPr,
};

/// 글자 흐름의 한 자리. 줄 배치(lineseg)의 textpos가 이 자리 번호를 센다.
pub(crate) struct Atom<'a> {
    pub ch: char,
    pub char_pr: i32,
    pub object: Option<&'a RunItem>,
    /// 컨트롤이 차지하는 자리를 맞추는 빈 자리. 그리지 않는다.
    pub filler: bool,
    /// 각주·미주 번호 글자(작게, 올려 그린다).
    pub note_ref: bool,
    /// 번호의 첫 글자에 단 각주·미주.
    pub note: Option<&'a Note>,
    pub tab_width: Option<f32>,
    pub tab_leader: &'a str,
    /// 변경 추적: 지운 글이면 `Some(false)`, 넣은 글이면 `Some(true)`.
    pub track: Option<bool>,
    /// 글자 겹치기: `ch`는 테두리 글자, 안쪽 글자는 그릴 때 얹는다.
    pub compose: Option<&'a Compose>,
    /// 문단 글(글 조각만 이은 것)에서 이 글자의 차례. 글 조각에서 온 글자만 가진다.
    pub source: Option<u32>,
}

impl<'a> Atom<'a> {
    pub fn plain(ch: char, char_pr: i32) -> Self {
        Self {
            ch,
            char_pr,
            object: None,
            filler: false,
            note_ref: false,
            note: None,
            tab_width: None,
            tab_leader: "NONE",
            track: None,
            compose: None,
            source: None,
        }
    }

    fn filler(char_pr: i32) -> Self {
        Self {
            filler: true,
            ..Self::plain(' ', char_pr)
        }
    }
}

/// 변경 추적 글자와 그 선의 색, 선 굵기(pt).
pub(crate) const TRACK_COLOR: u32 = 0xFF_00_00;
const TRACK_STROKE: f32 = 0.45;

/// 탭 채움 "DOT"에 찍는 글자.
const LEADER_DOT: char = '\u{B7}';
/// 탭 채움 점 칸의 최소 폭(em, 장평과 무관). 한/글 기준 PDF의 점 줄 칸 수가 탭 폭 / 이 값의 내림과 맞는다.
const LEADER_DOT_SLOT: f32 = 0.25;
/// 세로 정렬이 가운데인 글자의 기준선 비율. 한/글 기준 PDF에서 잰 값으로, 파일에 적힌 절반 값 대신 쓴다.
const CENTERED_ASCENT: f32 = 0.85;

/// 글자 모양·언어 칸 하나의 그리기 값. size는 pt(상대 크기·위 첨자 반영), 세로 치수도 pt.
#[derive(Clone, Copy)]
pub(crate) struct Style {
    pub face: Option<FaceId>,
    pub size: f32,
    pub ratio: f32,
    pub bold_stroke: Option<f32>,
    pub italic: bool,
    pub ascent: f32,
    pub descent: f32,
    pub x_height: f32,
    /// 한/글이 HFT로 그리는 영문 글꼴이면 그 폭 표. 모양은 `face`로 그리고 폭은 이 표로 잰다.
    pub hft: Option<usize>,
    /// 문서가 적은 글꼴(과 그 치환 후보)을 찾지 못해 계열 대체 글꼴로 그리는가.
    pub substituted: bool,
}

fn line_text(line: &Line<'_>) -> String {
    let mut text = String::new();
    for glyph in &line.glyphs {
        match glyph.kind {
            Kind::Text | Kind::Space => text.push(glyph.ch),
            Kind::Tab | Kind::FixedTab => text.push('\t'),
            _ => {}
        }
    }
    text.truncate(text.trim_end().len());
    text
}

/// 글자 폭을 재는 방식.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Advances {
    /// 글꼴 폭 그대로. 줄 나눔과 문단 머리·쪽 번호 폭은 이것으로 잰다(그리는 폭으로 재면 e02 등 줄 나눔이 어긋난다).
    Exact,
    /// 한/글 12.30.0(Mac)이 글자를 그리는 폭. [`hancom_advance`]를 본다.
    Hancom,
}

/// 한/글 12.30.0(Mac)이 글자 폭을 재는 단위(pt, 1/1800인치).
const GLYPH_UNIT: f32 = 72.0 / 1800.0;

/// 한/글이 그리는 글자 폭(pt). 글자 크기를 단위로 내림하고, 글꼴 폭(장평 포함)을 곱한 값을 단위로
/// 반올림한다. 자간은 그 뒤에 곱한다. 9·10·10.3·10.5·11·12·13·14·15pt 함초롬바탕·돋움 한 줄 28자의
/// 글자 간격이 모두 이 값과 맞는다(10.3pt: 257단위 × 0.97 → 249단위 = 9.96pt).
fn hancom_advance(em: f32, size: f32) -> f32 {
    let size_units = (size / GLYPH_UNIT + 1e-3).floor();
    (em * size_units + 0.5 + 1e-3).floor() * GLYPH_UNIT
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Space,
    Tab,
    FixedTab,
    Object,
}

struct Glyph<'a> {
    kind: Kind,
    ch: char,
    /// 줄 시작에서 잰 왼쪽(pt).
    x: f32,
    advance: f32,
    /// advance 가운데 자간이 더하거나 뺀 폭(pt).
    spacing: f32,
    face: Option<FaceId>,
    glyph: u32,
    size: f32,
    scale_x: f32,
    /// 문서 글꼴 대신 대체 글꼴로 그린 글자. 저장된 줄 폭은 원래 글꼴로 잰 값이다.
    substituted: bool,
    italic: bool,
    bold_stroke: Option<f32>,
    dy: f32,
    ascent: f32,
    descent: f32,
    x_height: f32,
    char_pr: &'a CharPr,
    leader: &'a str,
    object: Option<&'a RunItem>,
    track: Option<bool>,
    compose: Option<&'a Compose>,
    /// 문단 글에서 이 글자의 차례([`Atom::source`]).
    source: Option<u32>,
}

pub(crate) struct Placed<'a> {
    pub item: &'a RunItem,
    pub x: f32,
}

pub(crate) struct Line<'a> {
    glyphs: Vec<Glyph<'a>>,
    /// 끝 빈칸을 뺀 폭(pt).
    pub width: f32,
    /// 빈칸까지 넣은 폭(pt).
    pub full_width: f32,
    /// 끝 글자의 자간(pt). 한/글은 가운데·오른쪽 맞춤에서 이 값을 줄 폭에 넣지 않는다.
    last_spacing: f32,
    pub objects: Vec<Placed<'a>>,
}

impl Line<'_> {
    /// 변경 추적 글자가 있는 줄인가.
    pub(crate) fn tracked(&self) -> bool {
        self.glyphs
            .iter()
            .any(|glyph| glyph.track.is_some() && glyph.kind == Kind::Text)
    }

    /// 줄 글자 가운데 가장 깊은 글꼴 descent(pt).
    pub(crate) fn descent(&self) -> f32 {
        self.glyphs
            .iter()
            .map(|glyph| glyph.descent)
            .fold(0.0, f32::max)
    }

    /// 글자 세로 정렬이 위·가운데·아래인 줄에서 줄 위부터 기준선까지(pt)를 돌려준다.
    /// 한/글 12.30.0은 크기가 섞인 줄도 글자마다 따로 맞춘다. 위는 글꼴 ascent, 아래는 줄 높이에서 descent를 뺀 자리다.
    /// 가운데는 글자 크기의 85%를 기준선으로 삼아 남는 줄 높이의 절반만큼 내린다.
    /// 가장 큰 글자가 줄 기준선을 정하고, 나머지 글자는 그 차이를 dy에 더한다. 기준선 정렬이거나 글자가 없으면 None이다.
    pub(crate) fn align_vertically(&mut self, align: &str, height: f32) -> Option<f32> {
        let offset = |glyph: &Glyph| match align {
            "TOP" => Some(glyph.ascent),
            "CENTER" => Some((height - glyph.size).max(0.0) / 2.0 + glyph.size * CENTERED_ASCENT),
            "BOTTOM" => Some(height - glyph.descent),
            _ => None,
        };
        let tallest = self
            .glyphs
            .iter()
            .filter(|glyph| glyph.kind == Kind::Text)
            .max_by(|a, b| a.size.total_cmp(&b.size))?;
        let line = offset(tallest)?;
        for glyph in &mut self.glyphs {
            if glyph.kind == Kind::Text {
                glyph.dy += offset(glyph).unwrap_or(line) - line;
            }
        }
        Some(line)
    }

    fn measure(&mut self) {
        let mut pen = 0.0;
        for glyph in &mut self.glyphs {
            glyph.x = pen;
            pen += glyph.advance;
        }
        self.full_width = pen;
        let last = self
            .glyphs
            .iter()
            .rposition(|glyph| !matches!(glyph.kind, Kind::Space | Kind::Tab));
        self.width = last.map_or(0.0, |index| {
            self.glyphs[index].x + self.glyphs[index].advance
        });
        self.last_spacing = last.map_or(0.0, |index| self.glyphs[index].spacing);
        self.objects = self
            .glyphs
            .iter()
            .filter_map(|glyph| glyph.object.map(|item| Placed { item, x: glyph.x }))
            .collect();
    }
}

/// 글자의 언어 칸(글꼴·크기·장평을 고르는 칸).
pub fn lang_of(ch: char) -> &'static str {
    match u32::from(ch) {
        0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xD7B0..=0xD7FF => {
            "hangul"
        }
        0x0020..=0x007E | 0x00A0..=0x024F | 0x2005 | 0x2007 => "latin",
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x2E80..=0x2FDF => "hanja",
        0x3040..=0x30FF | 0x31F0..=0x31FF => "japanese",
        0x2000..=0x2BFF | 0x3000..=0x303F | 0xFF00..=0xFFEF => "symbol",
        _ => "other",
    }
}

/// 줄 첫머리에 올 수 없는 글자(닫는 문장 부호).
fn no_line_start(ch: char) -> bool {
    matches!(
        ch,
        '.' | ','
            | ')'
            | ']'
            | '}'
            | '!'
            | '?'
            | ':'
            | ';'
            | '\''
            | '"'
            | '%'
            | '~'
            | '\u{2019}'
            | '\u{201D}'
            | '\u{2026}'
            | '\u{00B7}'
            | '\u{3001}'
            | '\u{3002}'
            | '\u{3009}'
            | '\u{300B}'
            | '\u{300D}'
            | '\u{300F}'
            | '\u{3011}'
            | '\u{FF09}'
            | '\u{FF0C}'
            | '\u{FF0E}'
    )
}

/// 낱말 안에서도 줄을 끊을 수 있는 한글·한자·가나 글자인가.
fn wide(ch: char) -> bool {
    matches!(lang_of(ch), "hangul" | "hanja" | "japanese")
}

/// 그리지도 자리를 차지하지도 않는 글자.
/// 위 첨자 글자 크기 비율.
const SCRIPT_SCALE: f32 = 0.64;

pub(super) fn invisible(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{200B}'..='\u{200F}' | '\u{FEFF}' | '\u{FFFC}' | FIELD_START
        )
}

/// 누름틀 같은 필드의 시작 자리. 폭은 없지만 글자로 쳐서, 뒤따르는 빈칸이 줄 앞 빈칸이 되지 않는다(양쪽 정렬 때 함께 늘어난다).
const FIELD_START: char = '\u{FFF9}';

/// HWPX `hp:fieldBegin`, HWP 5 `%`로 시작하는 필드 컨트롤.
fn is_field_start(id: &str) -> bool {
    id == "hp:fieldBegin" || id.starts_with('%')
}

pub(crate) struct Head<'a> {
    pub line: Line<'a>,
    pub width: f32,
    pub auto_indent: bool,
}

impl<'a> Renderer<'a> {
    /// 문단의 글자 흐름. HWP 5와 HWPX는 컨트롤을 8자리로 세므로 보이는 글자 뒤를 빈 자리로 채워 번호를 맞춘다.
    pub(crate) fn atoms(&mut self, para: &'a Paragraph) -> Vec<Atom<'a>> {
        let wide = self.doc.wchar_positions;
        let mut out: Vec<Atom<'a>> = Vec::new();
        // 문단 글(글 조각만 이은 것)에서 몇 번째 글자인가. 고치는 쪽이 글자 자리를 찾는 데 쓴다.
        let mut source = 0u32;
        let pad = |out: &mut Vec<Atom<'a>>, width: usize, start: usize, char_pr: i32| {
            if wide {
                while out.len() - start < width {
                    out.push(Atom::filler(char_pr));
                }
            }
        };
        for run in &para.runs {
            let cp = run.char_pr_id;
            for item in &run.items {
                let start = out.len();
                match item {
                    RunItem::Text(text) => {
                        for ch in text.chars() {
                            let at = out.len();
                            out.push(Atom {
                                source: Some(source),
                                ..Atom::plain(ch, cp)
                            });
                            source += 1;
                            pad(&mut out, ch.len_utf16(), at, cp);
                        }
                    }
                    RunItem::Skip(count) => {
                        if wide {
                            for _ in 0..(*count).max(0) {
                                out.push(Atom::filler(cp));
                            }
                        }
                    }
                    RunItem::Tab => {
                        out.push(Atom::plain('\t', cp));
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::SizedTab(width) => {
                        out.push(Atom {
                            tab_width: Some(pt(*width)),
                            ..Atom::plain('\u{FFFC}', cp)
                        });
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::FormattedTab(tab) => {
                        let width = tab
                            .width
                            .filter(|width| width.is_finite() && *width > 0.0)
                            .map(pt);
                        out.push(Atom {
                            tab_width: width,
                            tab_leader: tab.leader.as_str(),
                            ..Atom::plain(if width.is_some() { '\u{FFFC}' } else { '\t' }, cp)
                        });
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::LineBreak => {
                        // 문단 글(`content`)에서 줄 바꿈은 `\n` 한 글자다.
                        out.push(Atom {
                            source: Some(source),
                            ..Atom::plain('\n', cp)
                        });
                        source += 1;
                    }
                    RunItem::Table(_) | RunItem::Picture(_) | RunItem::Shape(_) => {
                        out.push(Atom {
                            object: Some(item),
                            ..Atom::plain('\u{FFFC}', cp)
                        });
                        for _ in 0..7 {
                            out.push(Atom::filler(cp));
                        }
                    }
                    RunItem::AutoNum(kind) => {
                        for ch in self.auto_number(kind).chars() {
                            out.push(Atom::plain(ch, cp));
                        }
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::Control(id) => {
                        if is_field_start(id) {
                            out.push(Atom::plain(FIELD_START, cp));
                        }
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::Compose(compose) => {
                        out.push(Atom {
                            compose: Some(compose),
                            ..Atom::plain(compose_frame(&compose.shape).unwrap_or('\u{25CB}'), cp)
                        });
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::Note(note) => {
                        for (index, ch) in note_number_text(note).chars().enumerate() {
                            out.push(Atom {
                                note_ref: true,
                                note: (index == 0).then_some(note),
                                ..Atom::plain(ch, cp)
                            });
                        }
                        pad(&mut out, 8, start, cp);
                    }
                    RunItem::Header { .. }
                    | RunItem::Footer { .. }
                    | RunItem::PageHiding { .. }
                    | RunItem::NewNum(_)
                    | RunItem::PageNum { .. } => {
                        out.push(Atom {
                            object: Some(item),
                            filler: true,
                            ..Atom::plain('\u{200B}', cp)
                        });
                        pad(&mut out, 8, start, cp);
                    }
                }
            }
        }
        if wide && !self.doc.track_markup_hidden {
            for mark in &para.tracks {
                let end = (mark.end as usize).min(out.len());
                for atom in out.iter_mut().take(end).skip(mark.start as usize) {
                    atom.track = Some(mark.insert);
                }
            }
        }
        out
    }

    /// 자동 번호 필드의 글자열. 본문에서 전체 쪽수를 쓰면 확정 쪽수를 찍으려고 한 번 더 배치한다.
    fn auto_number(&mut self, kind: &str) -> String {
        match kind {
            "TOTAL_PAGE" => {
                if self.in_body && self.total_pages.is_none() {
                    self.saw_early_total = true;
                }
                let total = if self.in_body {
                    self.total_pages.unwrap_or(self.page_count())
                } else {
                    self.page_count()
                };
                total.to_string()
            }
            "FOOTNOTE" | "ENDNOTE" => self.current_note.clone().unwrap_or_default(),
            "PAGE" => self
                .display_page_number
                .unwrap_or(self.page.map_or(1, |page| page as i64 + 1))
                .to_string(),
            _ => String::new(),
        }
    }

    fn font_face(&self, lang: &str, id: i32) -> &'a FontFace {
        static DEFAULT: std::sync::OnceLock<FontFace> = std::sync::OnceLock::new();
        let doc = self.doc;
        let find = |lang: &str| {
            doc.font_faces.get(lang).and_then(|faces| {
                faces
                    .iter()
                    .find(|face| face.id == id)
                    .or_else(|| usize::try_from(id).ok().and_then(|index| faces.get(index)))
            })
        };
        find(lang)
            .or_else(|| find("hangul"))
            .unwrap_or_else(|| DEFAULT.get_or_init(|| FontFace::new("함초롬돋움".to_string())))
    }

    pub(crate) fn style(&mut self, char_pr: i32, lang: &'static str, note_ref: bool) -> Style {
        if let Some(style) = self.styles.get(&(char_pr, lang, note_ref)) {
            return *style;
        }
        let cp = self.char_pr(char_pr);
        let id = cp
            .font_ref
            .get(lang)
            .or_else(|| cp.font_ref.get("hangul"))
            .copied()
            .unwrap_or(0);
        let face = self.font_face(lang, id);
        let height = if note_ref {
            cp.height * 0.75
        } else {
            cp.height
        };
        let relative = cp.rel_sz.get(lang).copied().unwrap_or(100.0) as f32;
        let size = (pt(height) * relative / 100.0).max(1.0)
            * if cp.supscript || cp.sub_script {
                SCRIPT_SCALE
            } else {
                1.0
            };
        let ratio = (cp.ratio.get(lang).copied().unwrap_or(100.0) as f32 / 100.0).max(0.2);
        let resolved = self.rules.resolve(self.fonts, face, lang, cp.bold);
        let substituted = !self
            .rules
            .candidates(face, lang)
            .iter()
            .any(|name| self.fonts.find(name, cp.bold).is_some());
        let metrics = resolved.map(|(id, _)| self.fonts.metrics(id));
        let style = Style {
            face: resolved.map(|(id, _)| id),
            size,
            ratio,
            bold_stroke: (cp.bold && !resolved.is_some_and(|(_, bold)| bold))
                .then_some(size * 0.02),
            italic: cp.italic,
            ascent: metrics.map_or(0.85, |metrics| metrics.ascent) * size,
            descent: metrics.map_or(0.15, |metrics| metrics.descent) * size,
            x_height: metrics.map_or(0.5, |metrics| metrics.x_height) * size,
            hft: (lang == "latin" && face.face_type == 2)
                .then(|| self.fonts.hft_latin(&face.name, cp.bold, cp.italic))
                .flatten(),
            substituted,
        };
        self.styles.insert((char_pr, lang, note_ref), style);
        style
    }

    fn glyph_fallbacks(&mut self) -> Vec<FaceId> {
        if self.glyph_fallbacks.is_none() {
            let rules = self.rules;
            let mut faces: Vec<_> = rules
                .fallback_glyph
                .iter()
                .chain(&rules.fallback_gothic)
                .chain(&rules.fallback_serif)
                .filter_map(|name| self.fonts.find(name, false).map(|(id, _)| id))
                .collect();
            for id in self.fonts.embeddable_faces() {
                if !faces.contains(&id) {
                    faces.push(id);
                }
            }
            self.glyph_fallbacks = Some(faces);
        }
        self.glyph_fallbacks.clone().unwrap_or_default()
    }

    /// 글자를 그릴 얼굴과 글리프: 고른 얼굴에 없으면 대체 얼굴에서 찾는다.
    fn glyph_for(&mut self, face: Option<FaceId>, ch: char) -> Option<(FaceId, GlyphInfo)> {
        if let Some(face) = face
            && let Some(info) = self.fonts.glyph(face, ch).filter(|info| info.glyph != 0)
        {
            return Some((face, info));
        }
        self.glyph_fallbacks().into_iter().find_map(|fallback| {
            self.fonts
                .glyph(fallback, ch)
                .filter(|info| info.glyph != 0)
                .map(|info| (fallback, info))
        })
    }

    /// 줄 하나의 글자 자리. 탭은 탭 설정의 멈춤 자리(없으면 기본 간격의 배수)로 간다.
    pub(crate) fn shape(
        &mut self,
        atoms: &[Atom<'a>],
        tab_pr: Option<&'a TabPr>,
        advances: Advances,
    ) -> Line<'a> {
        let mut glyphs: Vec<Glyph<'a>> = Vec::new();
        for atom in atoms {
            let control = atom.filler && atom.ch == '\u{200B}' && atom.object.is_some();
            if atom.filler && !control {
                continue;
            }
            let cp = self.char_pr(atom.char_pr);
            let blank = Glyph {
                kind: Kind::Text,
                ch: atom.ch,
                x: 0.0,
                advance: 0.0,
                spacing: 0.0,
                face: None,
                glyph: 0,
                size: pt(cp.height),
                scale_x: 1.0,
                substituted: false,
                italic: false,
                bold_stroke: None,
                dy: 0.0,
                ascent: pt(cp.height) * 0.85,
                descent: pt(cp.height) * 0.15,
                x_height: pt(cp.height) * 0.5,
                char_pr: cp,
                leader: atom.tab_leader,
                object: None,
                track: atom.track,
                compose: atom.compose,
                source: atom.source,
            };
            if control {
                // 줄 앞 쪽 컨트롤 뒤의 빈칸은 한/글이 줄 안 빈칸으로 친다.
                glyphs.push(blank);
                continue;
            }
            if let Some(item) = atom.object {
                let (width, _) = self.object_size(item);
                let margins = super::object_margins(item);
                glyphs.push(Glyph {
                    kind: Kind::Object,
                    advance: if object_pos(item).treat_as_char {
                        width + pt(margins.left + margins.right)
                    } else {
                        0.0
                    },
                    object: Some(item),
                    ..blank
                });
                continue;
            }
            if atom.ch == FIELD_START {
                glyphs.push(blank);
                continue;
            }
            let fixed_tab = atom
                .tab_width
                .filter(|width| width.is_finite() && *width > 0.0);
            if fixed_tab.is_none() && (atom.ch == '\n' || (atom.ch != '\t' && invisible(atom.ch))) {
                continue;
            }
            let tab = fixed_tab.is_some() || atom.ch == '\t';
            // 탭은 채움 점을 찍을 글자 모양(점 글자의 언어 칸)으로 잡는다.
            let lang = lang_of(if tab { LEADER_DOT } else { atom.ch });
            let style = self.style(atom.char_pr, lang, atom.note_ref);
            let size = style.size;
            // 한/글은 자간을 글자 크기가 아니라 그 글자 폭(장평 반영)의 비율로 더하고 뺀다.
            let spacing_scale = 1.0 + cp.spacing.get(lang).copied().unwrap_or(0.0) as f32 / 100.0;
            let offset = cp.offset.get(lang).copied().unwrap_or(0.0) as f32 / 100.0 * size;
            let dy = if atom.note_ref {
                let base = self.style(atom.char_pr, lang, false).size;
                -0.85 * (base - size) + offset
            } else if cp.supscript {
                let relative = cp.rel_sz.get(lang).copied().unwrap_or(100.0) as f32;
                -0.44 * (pt(cp.height) * relative / 100.0).max(1.0) + offset
            } else if cp.sub_script {
                // 한/글: 아래 첨자는 줄이기 전 크기의 0.12만큼 내린다.
                let relative = cp.rel_sz.get(lang).copied().unwrap_or(100.0) as f32;
                0.12 * (pt(cp.height) * relative / 100.0).max(1.0) + offset
            } else {
                offset
            };
            let styled = Glyph {
                size,
                scale_x: style.ratio,
                substituted: style.substituted,
                italic: style.italic,
                bold_stroke: style.bold_stroke,
                dy,
                ascent: style.ascent,
                descent: style.descent,
                x_height: style.x_height,
                face: style.face,
                ..blank
            };
            if tab {
                let (face, glyph) = match self.glyph_for(style.face, LEADER_DOT) {
                    Some((face, info)) => (Some(face), info.glyph),
                    None => (style.face, 0),
                };
                glyphs.push(Glyph {
                    kind: if fixed_tab.is_some() {
                        Kind::FixedTab
                    } else {
                        Kind::Tab
                    },
                    advance: fixed_tab.unwrap_or(0.0),
                    face,
                    glyph,
                    ..styled
                });
                continue;
            }
            match atom.ch {
                ' ' | '\u{3000}' | '\u{00A0}' | FIXED_SPACE | NO_BREAK_SPACE => {
                    // 한/글: 고정폭 빈칸은 글꼴과 상관없이 1/4 em, 묶음 빈칸은 빈칸 폭에 자간을 넣지 않는다.
                    let em = match atom.ch {
                        FIXED_SPACE => 0.25,
                        ' ' | NO_BREAK_SPACE if !cp.use_font_space => 0.5,
                        NO_BREAK_SPACE => self
                            .glyph_for(style.face, ' ')
                            .map_or(0.5, |(_, info)| info.advance),
                        ch => self
                            .glyph_for(style.face, ch)
                            .map_or(if ch == '\u{3000}' { 1.0 } else { 0.5 }, |(_, info)| {
                                info.advance
                            }),
                    };
                    let spacing = if atom.ch == NO_BREAK_SPACE {
                        1.0
                    } else {
                        spacing_scale
                    };
                    // 한/글은 위·아래 첨자 안의 빈칸을 줄이지 않은 글자 크기로 잰다.
                    let size = if cp.supscript || cp.sub_script {
                        size / SCRIPT_SCALE
                    } else {
                        size
                    };
                    glyphs.push(Glyph {
                        kind: Kind::Space,
                        advance: em * size * style.ratio * spacing,
                        ..styled
                    });
                }
                ch => {
                    let (face, glyph, em) = match self.glyph_for(style.face, ch) {
                        Some((face, info)) => (Some(face), info.glyph, info.advance),
                        None => (style.face, 0, 0.5),
                    };
                    let em = style
                        .hft
                        .and_then(|table| self.fonts.hft_advance(table, ch))
                        .unwrap_or(em);
                    let advance = em * size * style.ratio;
                    let placed = match advances {
                        Advances::Exact => advance,
                        Advances::Hancom => hancom_advance(em * style.ratio, size),
                    } * spacing_scale;
                    glyphs.push(Glyph {
                        face,
                        glyph,
                        advance: placed,
                        spacing: placed - advance,
                        ..styled
                    });
                }
            }
        }
        self.place_tabs(&mut glyphs, tab_pr);
        let mut line = Line {
            glyphs,
            width: 0.0,
            full_width: 0.0,
            last_spacing: 0.0,
            objects: Vec::new(),
        };
        line.measure();
        line
    }

    fn place_tabs(&self, glyphs: &mut [Glyph<'a>], tab_pr: Option<&'a TabPr>) {
        if !glyphs.iter().any(|glyph| glyph.kind == Kind::Tab) {
            return;
        }
        let mut stops: Vec<(f32, &'a str, &'a str)> = tab_pr
            .map(|tab_pr| {
                tab_pr
                    .stops
                    .iter()
                    .map(|stop| (pt(stop.position), stop.kind.as_str(), stop.leader.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let interval = if self.tab_interval > 0.0 {
            self.tab_interval
        } else {
            40.0
        };
        let mut pen = 0.0f32;
        for index in 0..glyphs.len() {
            if glyphs[index].kind == Kind::Tab {
                let (target, kind, leader) = stops
                    .iter()
                    .find(|stop| stop.0 > pen + 0.01)
                    .copied()
                    .unwrap_or((
                        (pen / interval).floor() * interval + interval,
                        "LEFT",
                        "NONE",
                    ));
                let following = glyphs[index + 1..]
                    .iter()
                    .take_while(|glyph| !matches!(glyph.kind, Kind::Tab | Kind::FixedTab));
                let segment: f32 = following.clone().map(|glyph| glyph.advance).sum();
                let start = match kind {
                    "RIGHT" => target - segment,
                    "CENTER" => target - segment / 2.0,
                    "DECIMAL" => {
                        target
                            - following
                                .take_while(|glyph| glyph.ch != '.')
                                .map(|glyph| glyph.advance)
                                .sum::<f32>()
                    }
                    _ => target,
                };
                let glyph = &mut glyphs[index];
                glyph.advance = (start - pen).max(0.0);
                if glyph.leader == "NONE" {
                    glyph.leader = leader;
                }
            }
            pen += glyphs[index].advance;
        }
    }

    /// 줄의 글자를 쪽에 그린다: 음영 → 글자 → 밑줄·취소선·탭 채움 순.
    pub(crate) fn emit(&mut self, line: &Line<'a>, x: f32, baseline: f32) {
        for group in groups(&line.glyphs, |glyph| {
            glyph
                .char_pr
                .shade_color
                .filter(|_| glyph.kind != Kind::Object)
        }) {
            let (color, glyphs) = group;
            let (ascent, descent, left, right) = extent(glyphs);
            self.fill(
                scene::Rect::new(x + left, baseline - ascent, right - left, ascent + descent),
                color,
                1.0,
            );
        }
        if self.records_text {
            self.record_text(line_text(line));
        }
        if self.rec.on
            && let Some(page) = self.page
        {
            let sourced = || line.glyphs.iter().filter(|glyph| glyph.source.is_some());
            let ascent = sourced()
                .map(|glyph| glyph.ascent - glyph.dy)
                .fold(0.0, f32::max);
            let descent = sourced()
                .map(|glyph| glyph.descent + glyph.dy)
                .fold(0.0, f32::max);
            let boxes: Vec<(u32, scene::Rect)> = sourced()
                .filter_map(|glyph| {
                    let index = glyph.source?;
                    Some((
                        index,
                        scene::Rect::new(
                            x + glyph.x,
                            baseline - ascent,
                            glyph.advance.max(0.0),
                            ascent + descent,
                        ),
                    ))
                })
                .collect();
            self.rec.chars(page, &boxes);
        }
        let mut run: Option<GlyphRun> = None;
        for glyph in &line.glyphs {
            if glyph.kind != Kind::Text || glyph.glyph == 0 {
                continue;
            }
            let Some(face) = glyph.face else {
                continue;
            };
            let color = glyph
                .track
                .map_or(glyph.char_pr.text_color, |_| TRACK_COLOR);
            let same = run.as_ref().is_some_and(|run| {
                run.face == face
                    && run.size == glyph.size
                    && run.scale_x == glyph.scale_x
                    && run.italic == glyph.italic
                    && run.bold_stroke == glyph.bold_stroke
                    && run.color == color
                    && run.origin.1 == baseline + glyph.dy
            });
            if !same && let Some(done) = run.take() {
                self.add(Item::Glyphs(done));
            }
            let run = run.get_or_insert_with(|| GlyphRun {
                face,
                size: glyph.size,
                origin: (x, baseline + glyph.dy),
                scale_x: glyph.scale_x,
                italic: glyph.italic,
                bold_stroke: glyph.bold_stroke,
                color,
                glyphs: Vec::new(),
                text: String::new(),
                ranges: Vec::new(),
            });
            let start = run.text.len();
            run.text.push(glyph.ch);
            run.ranges.push(start..run.text.len());
            run.glyphs.push((glyph.glyph, glyph.x));
        }
        if let Some(done) = run {
            self.add(Item::Glyphs(done));
        }
        for glyph in &line.glyphs {
            if let Some(compose) = glyph.compose {
                self.compose_overlay(glyph, compose, x, baseline);
            }
        }
        let underline = |glyph: &Glyph<'a>| {
            (glyph.char_pr.underline != "NONE"
                && !glyph.char_pr.underline.is_empty()
                && glyph.kind != Kind::Object)
                .then(|| {
                    (
                        glyph.char_pr.underline_color,
                        glyph.char_pr.underline == "TOP",
                    )
                })
        };
        // 한/글은 줄 끝 빈칸에는 밑줄을 긋지 않는다.
        let content = line
            .glyphs
            .iter()
            .rposition(|glyph| glyph.kind != Kind::Space)
            .map_or(0, |last| last + 1);
        for ((color, top), glyphs) in groups(&line.glyphs[..content], underline) {
            let (ascent, descent, left, right) = extent(glyphs);
            let thickness = (ascent * 0.05).max(0.6);
            let y = if top {
                baseline - ascent - thickness
            } else {
                baseline + descent * 0.6
            };
            // 한/글은 밑줄을 다음 글자 자리보다 장치 단위 하나만큼 더 긋는다.
            self.fill(
                scene::Rect::new(x + left, y, right - left + DEVICE_UNIT, thickness),
                color,
                1.0,
            );
        }
        let strike = |glyph: &Glyph<'a>| {
            (glyph.char_pr.strikeout && glyph.kind != Kind::Object)
                .then_some(glyph.char_pr.strikeout_color)
        };
        for (color, glyphs) in groups(&line.glyphs, strike) {
            let (ascent, _, left, right) = extent(glyphs);
            self.fill(
                scene::Rect::new(
                    x + left,
                    baseline - ascent * 0.3,
                    right - left,
                    (ascent * 0.05).max(0.6),
                ),
                color,
                1.0,
            );
        }
        // 한/글: 지운 글은 기준선 위 0.359em에 취소선, 넣은 글은 기준선 아래 0.165em에 밑줄을 0.45pt로 긋는다.
        let tracked = |glyph: &Glyph<'a>| glyph.track.filter(|_| glyph.kind != Kind::Object);
        for (insert, glyphs) in groups(&line.glyphs, tracked) {
            // 한/글은 빈칸만 지운 자리에는 선을 긋지 않는다.
            if glyphs.iter().all(|glyph| glyph.kind != Kind::Text) {
                continue;
            }
            let (_, _, left, right) = extent(glyphs);
            let size = glyphs.iter().map(|glyph| glyph.size).fold(0.0, f32::max);
            let y = if insert {
                baseline + 0.165 * size
            } else {
                baseline - 0.359 * size
            };
            self.fill(
                scene::Rect::new(x + left, y - TRACK_STROKE / 2.0, right - left, TRACK_STROKE),
                TRACK_COLOR,
                1.0,
            );
        }
        for glyph in &line.glyphs {
            if matches!(glyph.kind, Kind::Tab | Kind::FixedTab)
                && glyph.leader != "NONE"
                && glyph.advance > 0.0
            {
                self.tab_leader(glyph, x, baseline);
            }
        }
    }

    /// 안쪽 글자: 글자 크기를 `char_size`pt만큼 줄여 테두리 글자 폭의 가운데에 나란히(겹치기면 한자리에) 놓고,
    /// 세로는 두 글자의 몸통 가운데를 맞춘다. 반전 테두리 안의 글자는 흰색이다.
    fn compose_overlay(&mut self, frame: &Glyph<'a>, compose: &'a Compose, x: f32, baseline: f32) {
        let char_pr = frame.char_pr.id;
        let mut inner = Vec::new();
        for ch in compose.text.chars() {
            let style = self.style(char_pr, lang_of(ch), false);
            let size = (style.size + compose.char_size as f32).max(1.0);
            let scale = size / style.size;
            let Some((face, info)) = self.glyph_for(style.face, ch) else {
                continue;
            };
            let middle = (style.ascent - style.descent) * scale / 2.0;
            inner.push((
                ch,
                face,
                info,
                size,
                style.ratio,
                info.advance * size * style.ratio,
                middle,
            ));
        }
        let frame_width = if frame.glyph == 0 {
            frame.advance
        } else {
            frame
                .face
                .and_then(|face| self.fonts.glyph(face, frame.ch))
                .map_or(frame.advance, |info| {
                    info.advance * frame.size * frame.scale_x
                })
        };
        let width: f32 = if compose.overlap {
            inner.iter().map(|item| item.5).fold(0.0, f32::max)
        } else {
            inner.iter().map(|item| item.5).sum()
        };
        let frame_middle = (frame.ascent - frame.descent) / 2.0;
        let color = if compose.shape.contains("REVERSAL") {
            0xFF_FF_FF
        } else {
            frame.char_pr.text_color
        };
        let mut pen = x + frame.x + (frame_width - width) / 2.0;
        for (ch, face, info, size, ratio, advance, middle) in inner {
            let left = if compose.overlap {
                x + frame.x + (frame_width - advance) / 2.0
            } else {
                pen
            };
            pen += advance;
            let mut text = String::new();
            text.push(ch);
            self.add(Item::Glyphs(GlyphRun {
                face,
                size,
                origin: (left, baseline + frame.dy - (frame_middle - middle)),
                scale_x: ratio,
                italic: frame.italic,
                bold_stroke: frame.bold_stroke,
                color,
                glyphs: vec![(info.glyph, 0.0)],
                ranges: std::iter::once(0..text.len()).collect(),
                text,
            }));
        }
    }

    fn tab_leader(&mut self, glyph: &Glyph<'a>, x: f32, baseline: f32) {
        let color = glyph.char_pr.text_color;
        let thickness = (glyph.size * 0.05).max(0.1);
        let y = baseline - glyph.x_height / 2.0;
        let left = x + glyph.x;
        let width = glyph.advance;
        if glyph.leader == "DOT"
            && glyph.glyph != 0
            && let Some(face) = glyph.face
        {
            // 한/글은 탭 폭을 고른 칸으로 나눠 첫 칸과 끝 두 칸을 비우고 나머지 칸 앞에 점 글자를 찍는다.
            let slots = (width / (glyph.size * LEADER_DOT_SLOT)).floor().min(4096.0);
            let step = width / slots;
            let count = (slots as usize).saturating_sub(2);
            let mut run = GlyphRun {
                face,
                size: glyph.size,
                origin: (left, baseline + glyph.dy),
                scale_x: glyph.scale_x,
                italic: glyph.italic,
                bold_stroke: glyph.bold_stroke,
                color,
                glyphs: Vec::with_capacity(count),
                text: String::with_capacity(count * LEADER_DOT.len_utf8()),
                ranges: Vec::with_capacity(count),
            };
            for index in 1..=count {
                let start = run.text.len();
                run.text.push(LEADER_DOT);
                run.ranges.push(start..run.text.len());
                run.glyphs.push((glyph.glyph, step * index as f32));
            }
            if count > 0 {
                self.add(Item::Glyphs(run));
            }
            return;
        }
        match glyph.leader {
            "DOT" | "CIRCLE" => {
                let step = glyph.size * 0.27 * glyph.scale_x.max(0.2);
                let diameter = if glyph.leader == "CIRCLE" {
                    thickness * 2.0
                } else {
                    thickness * 1.6
                };
                let count = ((width / step).floor() as usize).min(4096);
                let mut path = Vec::new();
                for index in 0..count {
                    let cx = left + step * (index as f32 + 0.5);
                    path.extend(ellipse(scene::Rect::new(
                        cx - diameter / 2.0,
                        y - diameter / 2.0,
                        diameter,
                        diameter,
                    )));
                }
                if !path.is_empty() {
                    self.add(Item::Fill {
                        path,
                        color,
                        alpha: 1.0,
                    });
                }
            }
            kind => {
                let border = hwp_core::Border {
                    border_type: kind.to_string(),
                    width: f64::from(thickness) * 25.4 / 72.0,
                    color,
                };
                self.border_line(&border, (left, y), (left + width, y));
            }
        }
    }

    /// 문단 머리(번호·글머리표). 폭 = 머리 글자열 폭 + 너비 보정 + 본문과의 거리.
    pub(crate) fn paragraph_head(
        &mut self,
        para: &'a Paragraph,
        pp: &'a ParaPr,
        first_char_pr: i32,
    ) -> Option<Head<'a>> {
        let text = para.head_text.as_deref()?;
        if text.is_empty() || text == "\0" {
            return None;
        }
        let (char_pr, head) = match pp.heading_type.as_str() {
            "BULLET" => {
                let bullet = self.bullets.get(&pp.heading_id_ref)?;
                (bullet.char_pr_id, &bullet.para)
            }
            "NUMBER" | "OUTLINE" => {
                let outline_numbering_id = self
                    .page
                    .and_then(|page| self.pages.get(page))
                    .and_then(|page| self.doc.sections.get(page.section))
                    .and_then(|section| section.outline_numbering_id);
                let numbering = self
                    .numberings
                    .get(&pp.numbering_id(outline_numbering_id))?;
                let level = pp.heading_level.saturating_add(1).max(1);
                let head = numbering
                    .heads
                    .iter()
                    .find(|head| head.level == level)
                    .or(numbering.heads.first())?;
                (head.char_pr_id, &head.para)
            }
            _ => return None,
        };
        let char_pr = char_pr
            .filter(|id| self.char_prs.contains_key(id))
            .unwrap_or(first_char_pr);
        let size = pt(self.char_pr(char_pr).height);
        let atoms: Vec<Atom<'a>> = text.chars().map(|ch| Atom::plain(ch, char_pr)).collect();
        let line = self.shape(&atoms, None, Advances::Exact);
        let distance = if head.text_offset_percent {
            head.text_offset as f32 / 100.0 * size
        } else {
            pt(head.text_offset)
        };
        // 한/글 12.30.0은 라틴 글머리표('-' 따위)를 글꼴 폭이나 자간과 상관없이 반 글자 폭으로 잰다.
        let text_width =
            if pp.heading_type == "BULLET" && text.chars().all(|ch| lang_of(ch) == "latin") {
                text.chars().count() as f32 * size / 2.0
            } else {
                line.full_width
            };
        let width = (text_width + pt(head.width_adjust) + distance).max(0.0);
        Some(Head {
            line,
            width,
            auto_indent: head.auto_indent,
        })
    }

    /// 줄 배치가 저장되지 않은 문단의 줄을 글자 폭으로 나눈다(HWPUNIT). 빈칸 뒤에서 끊고, 한 낱말이 넘치면 글자 사이에서 끊는다.
    pub(crate) fn flow_segs(
        &mut self,
        para: &'a Paragraph,
        width: f64,
        start: f64,
    ) -> Vec<LineSeg> {
        let pp = self.para_pr(para.para_pr_id);
        let atoms = self.atoms(para);
        let tab_pr = self.tab_prs.get(&pp.tab_pr_id_ref).copied();
        let first_char_pr = atoms
            .iter()
            .find(|atom| !atom.filler)
            .map(|atom| atom.char_pr)
            .or_else(|| para.runs.first().map(|run| run.char_pr_id))
            .unwrap_or(0);
        let head = if pp.heading_type == "NONE" {
            None
        } else {
            self.paragraph_head(para, pp, first_char_pr)
        };
        let head_width = head
            .as_ref()
            .map_or(0.0, |head| f64::from(head.width) * 100.0);
        let auto_indent = head.as_ref().is_some_and(|head| head.auto_indent);
        // 한/글은 한글 줄 나눔 기준이 글자면 HWPX에 KEEP_WORD로 쓴다(어절이면 BREAK_WORD).
        // 이때 한글·한자·가나 글자 사이 어디서나 줄을 끊는다.
        let split_words = pp.break_non_latin_word == "KEEP_WORD";
        // 한/글(e12): 문단 기준으로 위아래 비켜 놓인 그리기 개체와 겹치는 줄은 개체 바깥 아래로 내려간다.
        // 표는 문단의 줄 뒤에 놓인다(e05, e07).
        let bands: Vec<(f64, f64)> = atoms
            .iter()
            .filter_map(|atom| atom.object.filter(|_| !atom.filler))
            .filter(|item| !matches!(item, RunItem::Table(_)))
            .filter_map(|item| {
                let pos = object_pos(item);
                if pos.treat_as_char
                    || pos.vert_rel_to != "PARA"
                    || pos.text_wrap != "TOP_AND_BOTTOM"
                {
                    return None;
                }
                let (_, height) = self.object_size(item);
                let margins = super::object_margins(item);
                let top = start - pp.prev + pos.vert_offset;
                Some((
                    top,
                    top + f64::from(height) * 100.0 + margins.top + margins.bottom,
                ))
            })
            .collect();
        let mut segs = Vec::new();
        let mut start_index = 0;
        let mut vert = start;
        loop {
            let first = segs.is_empty();
            let left = pp.margin_left;
            let horzsize = (width - left - pp.margin_right).max(100.0);
            let usable = horzsize
                - indent(pp, first)
                - if first || auto_indent {
                    head_width
                } else {
                    0.0
                };
            let end = self.line_end(&atoms, start_index, usable / 100.0, tab_pr, split_words);
            let line = &atoms[start_index..end];
            let (textheight, object_height) = self.line_heights(line, first_char_pr);
            let vertsize = textheight.max(object_height);
            let spacing = match pp.line_spacing_type.as_str() {
                "FIXED" => (pp.line_spacing - vertsize).max(0.0),
                "BETWEEN_LINES" => pp.line_spacing,
                "AT_LEAST" => (pp.line_spacing - vertsize).max(0.0),
                _ => textheight * (pp.line_spacing - 100.0) / 100.0,
            };
            while let Some(&(_, bottom)) = bands
                .iter()
                .find(|(top, bottom)| vert < *bottom && vert + vertsize > *top)
            {
                vert = bottom;
            }
            segs.push(LineSeg {
                textpos: start_index as i32,
                vertpos: vert,
                vertsize,
                textheight,
                // 한/글은 글자처럼 놓인 개체가 줄을 키워도 기준선을 줄 높이의 85%에 둔다(e05, 저장된 줄 정보 모두).
                baseline: vertsize * 0.85,
                spacing,
                horzpos: left,
                horzsize,
                flags: 0,
            });
            vert += vertsize + spacing;
            if end >= atoms.len() {
                break;
            }
            start_index = end;
        }
        segs
    }

    /// 줄 높이(HWPUNIT): 글자 높이 중 가장 큰 값과 글자처럼 취급한 개체 높이 중 가장 큰 값.
    fn line_heights(&mut self, atoms: &[Atom<'a>], fallback: i32) -> (f64, f64) {
        let mut text = 0.0f64;
        let mut object = 0.0f64;
        for atom in atoms {
            if let Some(item) = atom.object {
                if !atom.filler && object_pos(item).treat_as_char {
                    let (_, height) = self.object_size(item);
                    let margins = super::object_margins(item);
                    object = object.max(f64::from(height) * 100.0 + margins.top + margins.bottom);
                }
                continue;
            }
            if atom.filler {
                continue;
            }
            let cp = self.char_pr(atom.char_pr);
            let relative = cp.rel_sz.get(lang_of(atom.ch)).copied().unwrap_or(100.0);
            text = text.max(cp.height * relative / 100.0);
        }
        if text <= 0.0 {
            text = self.char_pr(fallback).height;
        }
        (text, object)
    }

    /// `start`부터 폭 `limit`(pt)에 드는 줄의 끝 자리(다음 줄의 시작).
    fn line_end(
        &mut self,
        atoms: &[Atom<'a>],
        start: usize,
        limit: f64,
        tab_pr: Option<&'a TabPr>,
        split_words: bool,
    ) -> usize {
        let mut end = start;
        let mut last_break: Option<usize> = None;
        let mut prev_full = 0.0f32;
        while end < atoms.len() {
            let atom = &atoms[end];
            if !atom.filler && atom.ch == '\n' {
                return end + 1;
            }
            let next = end
                + 1
                + atoms[end + 1..]
                    .iter()
                    .take_while(|atom| atom.filler)
                    .count();
            let line = self.shape(&atoms[start..next], tab_pr, Advances::Exact);
            // 컨트롤 자리를 채운 빈 자리만 있는 줄은 줄이 아니다. 한/글은 넓은 개체도 그 줄에 둔다.
            // 한/글(e03, e15, e17): 낱말 가운데에서 끊는 글자는 줄 끝까지 0.75pt 넘게 남아야 그 줄에 둔다.
            // 낱말 끝 글자와 빈칸은 줄 끝에 딱 맞아도 둔다(e02). 음수 자간은 마지막 글자 것도 줄 폭에 넣는다.
            let mid_word = !atom.ch.is_whitespace()
                && atoms[next..]
                    .iter()
                    .find(|a| !a.filler)
                    .is_some_and(|a| !a.ch.is_whitespace());
            let room = if mid_word { MID_WORD_ROOM } else { -0.01 };
            if f64::from(line.width - line.last_spacing.max(0.0)) > limit - room
                && atoms[start..end].iter().any(|atom| !atom.filler)
            {
                if let Some(at) = last_break {
                    return at;
                }
                return end;
            }
            // 한/글: 빈칸이 이어지면 줄 끝을 넘어선 자리에서 시작하는 빈칸부터 다음 줄로 넘긴다.
            if !atom.filler
                && atom.ch == ' '
                && f64::from(prev_full) > limit + 0.01
                && atoms[start..end]
                    .iter()
                    .rev()
                    .find(|a| !a.filler)
                    .is_some_and(|a| a.ch == ' ')
                && atoms[next..]
                    .iter()
                    .any(|a| !a.filler && !a.ch.is_whitespace())
            {
                return end;
            }
            prev_full = line.full_width;
            let space = matches!(atom.ch, ' ' | '\t' | FIXED_SPACE);
            let inside_word = split_words
                && wide(atom.ch)
                && atoms.get(next).is_some_and(|after| {
                    !after.filler
                        && !after.ch.is_whitespace()
                        && after.ch != FIXED_SPACE
                        && after.ch != NO_BREAK_SPACE
                        && !no_line_start(after.ch)
                });
            if !atom.filler && (space || inside_word) {
                last_break = Some(next);
            }
            end = next;
        }
        atoms.len()
    }
}

/// 낱말 가운데에서 줄을 끊을 때 한/글이 줄 끝에 남기는 최소 여유(pt). e15·e17에서 0.68pt는 넘기고 0.87pt는 둔다.
const MID_WORD_ROOM: f64 = 0.75;

/// 첫 줄 들여쓰기·다음 줄 내어쓰기(HWPUNIT). 한/글은 줄 배치의 시작 자리·폭에 왼쪽 여백만 넣고 이 값은 그릴 때 더한다.
pub(crate) fn indent(pp: &ParaPr, first: bool) -> f64 {
    if first {
        pp.indent.max(0.0)
    } else {
        (-pp.indent).max(0.0)
    }
}

/// 줄 맞춤. 넘치는 줄은 글자 폭을 줄여 줄에 맞추고, 양쪽·배분·나눔 정렬은 남는 폭을 빈칸(없으면 글자 사이)에 나눈다.
/// 돌려주는 값은 줄을 그릴 왼쪽(pt).
pub(crate) fn align(line: &mut Line, pp: &ParaPr, x0: f32, avail: f32, last: bool) -> f32 {
    if line.width > avail + 0.5 && line.width > 0.0 {
        squeeze(line, avail, pp.line_wrap == "SQUEEZE", last);
    }
    let extra = avail - line.width;
    let mut x = x0;
    if extra > 0.0 && line.width > 0.0 {
        match pp.align.as_str() {
            "JUSTIFY" if !last => spread(line, extra, true),
            "DISTRIBUTE" => spread(line, extra, false),
            "DISTRIBUTE_SPACE" => spread(line, extra, true),
            "CENTER" => x += (extra + line.last_spacing) / 2.0,
            "RIGHT" => x += extra + line.last_spacing,
            _ => {}
        }
    }
    x
}

/// 한/글 12.30.0은 넘치는 줄의 빈칸을 먼저 80%까지 줄이고, 그래도 넘치는 만큼은
/// 빈칸이 아닌 글자마다 같은 폭씩 자간으로 뺀다. 글자 모양은 줄이지 않는다.
/// 줄 앞 빈칸은 '한 줄로 입력(SQUEEZE)' 문단에서만 줄인다.
/// 대체 글꼴 글자를 가로로 줄이는 한계. 이보다 더 넘치는 만큼은 한/글처럼 빈칸과 자간에서 뺀다.
const MIN_SUBSTITUTE_SCALE: f32 = 0.4;

fn squeeze(line: &mut Line, avail: f32, leading_spaces: bool, last_line: bool) {
    let Some(last) = line
        .glyphs
        .iter()
        .rposition(|glyph| !matches!(glyph.kind, Kind::Space | Kind::Tab))
    else {
        return;
    };
    let mut over = line.width - avail;
    // 저장된 줄 배치는 문서 글꼴로 잰 것이라, 더 넓은 대체 글꼴로 그리면 줄이 크게 넘친다. 이때 자간을
    // 빼면 글자가 서로 겹치므로, 대체 글꼴 글자를 장평처럼 가로로 줄여 줄에 맞춘다.
    let substituted: f32 = line.glyphs[..=last]
        .iter()
        .filter(|glyph| glyph.substituted && squeezable(glyph.kind))
        .map(|glyph| glyph.advance)
        .sum();
    if substituted > 0.0 {
        let factor = (1.0 - over / substituted).max(MIN_SUBSTITUTE_SCALE);
        for glyph in &mut line.glyphs[..=last] {
            if glyph.substituted && squeezable(glyph.kind) {
                glyph.advance *= factor;
                glyph.spacing *= factor;
                glyph.scale_x *= factor;
            }
        }
        over -= substituted * (1.0 - factor);
        if over <= 0.0 {
            line.measure();
            return;
        }
    }
    let space_sum = |first: usize| -> f32 {
        line.glyphs[first..last]
            .iter()
            .filter(|glyph| glyph.kind == Kind::Space)
            .map(|glyph| glyph.advance)
            .sum()
    };
    let mut first = if leading_spaces {
        0
    } else {
        line.glyphs
            .iter()
            .position(|glyph| glyph.kind != Kind::Space)
            .unwrap_or(0)
    };
    // 문단 끝 줄이 빈칸만 줄여서는 모자라면 한/글은 앞 빈칸도 함께 줄인다.
    if last_line && over > space_sum(first) * 0.2 {
        first = 0;
    }
    let spaces = space_sum(first);
    if spaces > 0.0 {
        let cut = over.min(spaces * 0.2);
        let factor = 1.0 - cut / spaces;
        for glyph in &mut line.glyphs[first..last] {
            if glyph.kind == Kind::Space {
                glyph.advance *= factor;
            }
        }
        over -= cut;
    }
    let letters = line.glyphs[..=last]
        .iter()
        .filter(|glyph| squeezable(glyph.kind))
        .count();
    if over > 0.0 && letters > 0 {
        // 마지막 글자 뒤 자간은 줄 폭에 들지 않아, 한/글은 남은 폭을 마지막 글자를 뺀 글자 수로 나눈다.
        let cut = over / letters.saturating_sub(1).max(1) as f32;
        for glyph in &mut line.glyphs[..=last] {
            if squeezable(glyph.kind) {
                let cut = cut.min(glyph.advance);
                glyph.advance -= cut;
                glyph.spacing -= cut;
            }
        }
    }
    line.measure();
}

fn squeezable(kind: Kind) -> bool {
    !matches!(
        kind,
        Kind::Space | Kind::Tab | Kind::FixedTab | Kind::Object
    )
}

fn spread(line: &mut Line, extra: f32, spaces_first: bool) {
    let Some(end) = line
        .glyphs
        .iter()
        .rposition(|glyph| !matches!(glyph.kind, Kind::Space | Kind::Tab))
    else {
        return;
    };
    let begin = line
        .glyphs
        .iter()
        .position(|glyph| glyph.kind != Kind::Space)
        .unwrap_or(0);
    // 한/글은 고정폭 빈칸을 늘리지 않는다.
    let spaces: Vec<usize> = (begin..end)
        .filter(|index| {
            let glyph = &line.glyphs[*index];
            glyph.kind == Kind::Space && glyph.ch != FIXED_SPACE
        })
        .collect();
    let targets: Vec<usize> = if spaces_first && !spaces.is_empty() {
        spaces
    } else {
        (begin..end).collect()
    };
    if targets.is_empty() {
        return;
    }
    let each = extra / targets.len() as f32;
    for index in targets {
        line.glyphs[index].advance += each;
    }
    line.measure();
}

/// 같은 값을 내는 이웃 글자끼리 묶는다. 값이 없는 글자는 묶음을 끊는다.
fn groups<'g, 'a, K: PartialEq + Copy>(
    glyphs: &'g [Glyph<'a>],
    key: impl Fn(&Glyph<'a>) -> Option<K>,
) -> Vec<(K, &'g [Glyph<'a>])> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let Some(value) = key(&glyphs[start]) else {
            start += 1;
            continue;
        };
        let mut end = start + 1;
        while end < glyphs.len() && key(&glyphs[end]) == Some(value) {
            end += 1;
        }
        out.push((value, &glyphs[start..end]));
        start = end;
    }
    out
}

/// 글자 묶음의 위·아래 치수와 왼쪽·오른쪽(pt, 줄 기준).
fn extent(glyphs: &[Glyph]) -> (f32, f32, f32, f32) {
    let ascent = glyphs.iter().map(|glyph| glyph.ascent).fold(0.0, f32::max);
    let descent = glyphs.iter().map(|glyph| glyph.descent).fold(0.0, f32::max);
    let left = glyphs.first().map_or(0.0, |glyph| glyph.x);
    let right = glyphs.last().map_or(0.0, |glyph| glyph.x + glyph.advance);
    (ascent, descent, left, right)
}

/// 타원을 베지어 네 조각으로.
pub(crate) fn ellipse(rect: scene::Rect) -> Vec<PathOp> {
    const K: f32 = 0.552_284_8;
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (rx, ry) = (rect.width / 2.0, rect.height / 2.0);
    vec![
        PathOp::Move(cx + rx, cy),
        PathOp::Cubic(cx + rx, cy + ry * K, cx + rx * K, cy + ry, cx, cy + ry),
        PathOp::Cubic(cx - rx * K, cy + ry, cx - rx, cy + ry * K, cx - rx, cy),
        PathOp::Cubic(cx - rx, cy - ry * K, cx - rx * K, cy - ry, cx, cy - ry),
        PathOp::Cubic(cx + rx * K, cy - ry, cx + rx, cy - ry * K, cx + rx, cy),
        PathOp::Close,
    ]
}

/// 각주·미주 번호 글자열: 앞 장식 + 번호 모양대로 쓴 번호 + 뒤 장식.
pub(crate) fn note_number_text(note: &Note) -> String {
    let number = i64::from(note.number);
    let format = match note.format {
        1 if (1..=20).contains(&note.number) => "CIRCLED_DIGIT",
        2 => "ROMAN_CAPITAL",
        3 => "ROMAN_SMALL",
        4 => "LATIN_CAPITAL",
        5 => "LATIN_SMALL",
        8 => "HANGUL_SYLLABLE",
        _ => "DIGIT",
    };
    format!(
        "{}{}{}",
        note.prefix,
        hwp_core::num::format_number(number, format),
        note.suffix
    )
}

/// 글자 겹치기 테두리 모양을 그릴 글자. 모르는 모양은 테두리 없이 안쪽 글자만 놓는다.
fn compose_frame(shape: &str) -> Option<char> {
    Some(match shape {
        "SHAPE_CIRCLE" => '\u{25CB}',
        "SHAPE_REVERSAL_CIRCLE" => '\u{25CF}',
        "SHAPE_RECTANGLE" => '\u{25A1}',
        "SHAPE_REVERSAL_RECTANGLE" => '\u{25A0}',
        "SHAPE_TRIANGLE" => '\u{25B3}',
        "SHAPE_REVERSAL_TRIANGLE" => '\u{25B2}',
        "SHAPE_LIGHT" => '\u{263C}',
        "SHAPE_RHOMBUS" => '\u{25C7}',
        "SHAPE_REVERSAL_RHOMBUS" => '\u{25C6}',
        "SHAPE_ROUNDED_RECT" => '\u{25A2}',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::hancom_advance;

    /// 한/글 12.30.0(Mac)이 함초롬바탕 한글(0.97em)을 크기마다 그린 글자 간격(pt). PDF의 0.12pt 단위로 잰 값이다.
    #[test]
    fn hangul_advance_matches_hancom_at_each_size() {
        for (size, expected) in [
            (9.0, 8.72),
            (10.0, 9.72),
            (10.3, 9.96),
            (10.5, 10.16),
            (11.0, 10.68),
            (12.0, 11.64),
            (13.0, 12.60),
            (14.0, 13.60),
            (15.0, 14.56),
        ] {
            let advance = hancom_advance(0.97, size);
            assert!(
                (advance - expected).abs() < 0.001,
                "{size}pt: {advance} vs {expected}"
            );
        }
    }
}
