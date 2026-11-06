//! 쪽 구조 비교: 줄(같은 기준선의 글자 묶음), 괘선(가로·세로로 곧은 얇은 선), 그림 상자.
//! 화소 차이는 글꼴 모양 차이를 섞어 세지만, 이 비교는 자리만 본다. 그래서 "줄바꿈·쪽 나눔·
//! 표 행 높이·그림 자리가 같다"를 0/1로 말할 수 있다.
//!
//! 글은 비교에만 쓰고 어디에도 쓰지 않는다. 결과에는 쪽 번호·좌표·글자 수만 남는다.

use crate::metric::normalized;
use serde::Serialize;

/// 한 글자: 정규화한 글, 기준선 원점(pt, 쪽 왼쪽 위 기준), 글자 크기(pt).
#[derive(Clone, Debug)]
pub struct Glyph {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// 가로 또는 세로 괘선 한 토막. `at`은 선의 자리(가로선은 y), `from..to`는 길이 방향 구간.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub at: f64,
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box2 {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// 글꼴 단위로 그린 글자 윤곽 하나: 잉크 사각과 펜 자리(x). 「처럼 잉크가 글자 칸 한쪽에 몰린 글자는
/// 잉크만으로는 칸이 어디서 시작하는지 모른다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub ink: Box2,
    pub origin: f64,
}

impl From<Box2> for Outline {
    fn from(ink: Box2) -> Outline {
        Outline {
            ink,
            origin: ink.x0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Raw {
    pub glyphs: Vec<Glyph>,
    pub horizontal: Vec<Rule>,
    pub vertical: Vec<Rule>,
    /// 괘선보다 짧은 가로·세로 토막. 한/글은 점선을 점 하나하나로 긋는다.
    pub dots: [Vec<Rule>; 2],
    pub images: Vec<Box2>,
    /// 글꼴 단위로 그린 글자 윤곽 하나하나의 바깥 사각(`scan::outline_glyph`).
    pub outline_glyphs: Vec<Outline>,
}

/// 줄: 기준선이 같은 글자를 왼쪽부터 이은 것.
#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    pub y: f64,
    pub x: f64,
    /// 줄을 이룬 글자(겹친 사본은 하나). 윤곽 글자에 덮인 글자만 뺄 때 쓴다.
    pub glyphs: Vec<Glyph>,
}

impl Line {
    fn of(glyphs: Vec<Glyph>) -> Line {
        Line {
            text: glyphs
                .iter()
                .flat_map(|glyph| normalized(&glyph.text))
                .collect(),
            y: glyphs[0].y,
            x: glyphs[0].x,
            glyphs,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Structure {
    pub lines: Vec<Line>,
    pub horizontal: Vec<Rule>,
    pub vertical: Vec<Rule>,
    pub images: Vec<Box2>,
    /// 글자 윤곽이 덮은 자리. 한쪽이 글을 윤곽으로만 그렸으면 그 자리의 줄은 견줄 수 없다.
    pub outline_glyphs: Vec<Outline>,
}

impl Structure {
    /// 줄과 같은 기준선에서 줄 양끝에 글자 크기 안으로 이어 붙은 글자 윤곽 수. 윤곽이 다시 윤곽에
    /// 이어 붙으면 그것도 센다(낱말 전체를 윤곽으로 그린 줄).
    fn outlines_beside(&self, line: &Line) -> usize {
        let (first, last) = (&line.glyphs[0], &line.glyphs[line.glyphs.len() - 1]);
        let size = first.size.max(last.size);
        let (mut from, mut to) = (first.x, last.x + last.size);
        let mut left: Vec<&Outline> = self
            .outline_glyphs
            .iter()
            .filter(|b| on_baseline(&b.ink, line.y, size))
            .collect();
        let before = left.len();
        loop {
            // 칸은 펜 자리에서 시작한다. 잉크가 칸 오른쪽에 몰린 「도 앞 글자에 이어 붙은 것이다.
            let (near, far): (Vec<&Outline>, Vec<&Outline>) = left
                .into_iter()
                .partition(|b| b.ink.x1 > from - size && b.ink.x0.min(b.origin) < to + size);
            left = far;
            if near.is_empty() {
                break;
            }
            for b in near {
                from = from.min(b.ink.x0);
                to = to.max(b.ink.x1);
            }
        }
        before - left.len()
    }

    /// 글자 하나가 차지하는 가로 구간 `from..to`(제 기준점부터 다음 글자 기준점까지, 크기 이내)에 같은
    /// 기준선의 글자 윤곽이 절반 넘게 드는가. 기준점만 보면 윤곽 사이 틈에 놓인 글자가 남고, 윤곽 바로
    /// 뒤 글자가 빠진다. 구간 끝에 이웃 윤곽이 조금 걸친 것(양쪽 글자 자리의 작은 차이)은 덮임이 아니다.
    fn outlined(&self, glyph: &Glyph, from: f64, to: f64) -> bool {
        self.outline_glyphs.iter().any(|Outline { ink: b, .. }| {
            let overlap = to.min(b.x1) - from.max(b.x0);
            overlap > OUTLINE_OVERLAP
                && overlap * 2.0 > b.x1 - b.x0
                && on_baseline(b, glyph.y, glyph.size)
        })
    }
}

/// 윤곽이 기준선 `y`, 크기 `size`인 글자 줄에 놓였는가. 글자 잉크는 기준선부터 크기만큼 위로 선다
/// (작은따옴표처럼 위에만 있는 글자도 있다). 윗줄 잉크는 줄 간격 때문에 크기의 80%보다 위에 있다.
fn on_baseline(b: &Box2, y: f64, size: f64) -> bool {
    b.y1 > y - 0.8 * size && b.y0 < y + OUTLINE_SLACK
}

/// 글자 윤곽 사각과 글자 자리를 맞대는 여유(pt). 기준선 아래로 내려간 획과 글자 사이 띄움만큼.
const OUTLINE_SLACK: f64 = 3.0;
/// 글자 구간과 윤곽이 이보다 넓게 겹쳐야 덮였다고 본다(pt). 윤곽 잉크가 다음 글자 자리로 살짝
/// 넘어온 것(기울임·글자 사이 좁힘)은 덮임이 아니다.
const OUTLINE_OVERLAP: f64 = 0.3;

/// 같은 기준선으로 보는 거리: 글자 크기의 이 몫까지. 한/글은 한 줄의 글자를 한 기준선에 놓고,
/// 위·아래 첨자는 크기의 10% 넘게 옮긴다(한/글 기본 첨자 자리 이동은 크기의 1/3 안팎).
const SAME_BASELINE: f64 = 0.1;
/// 이보다 얇은 칠·선은 괘선이다(pt). 한/글 선 굵기 표는 0.1–5mm, 그중 표 괘선은 0.1–0.7mm(≈2pt).
const RULE_THICKNESS: f64 = 2.5;
/// 이보다 짧은 곧은 선은 괘선으로 보지 않는다(pt). 글자를 선으로 그린 조각을 거른다.
const RULE_LENGTH: f64 = 3.0;
/// 같은 괘선으로 이어 붙이는 거리(pt). 셀마다 따로 그린 테두리를 한 줄로 합친다.
const RULE_JOIN: f64 = 0.6;
/// 같은 자리의 괘선으로 묶는 거리(pt). 한/글 좌표는 0.12pt 격자에 있고 겹선 두 선은 0.6pt(격자 5칸)
/// 떨어지기도 한다. 경계를 격자 4칸과 5칸 사이 한가운데에 두어 f32 오차로 판정이 갈리지 않게 한다.
const RULE_SAME_PLACE: f64 = 0.54;
/// 점선의 점 사이로 보는 거리(pt). 한/글 점선은 0.48pt 점을 0.72pt 띄워 긋는다.
const DOT_GAP: f64 = 2.0;

/// 자리 허용 오차(pt). 72dpi 채점 래스터의 한 화소이고, 화소 비교의 잉크 허용 반경
/// (`metric::INK_RADIUS` 1화소)과 같다. 한/글은 가로 괘선 끝을 선 굵기의 반만큼 늘여 그리므로
/// (0.1–0.4mm 선에서 0.15–0.6pt) 이보다 좁히면 그리는 법의 차이를 자리 차이로 센다.
/// 허용 안의 어긋남도 숨기지 않도록 짝지은 것의 최대 편차를 따로 적는다(`Diff::max_*`).
pub const PLACE_TOLERANCE: f64 = 1.0;

impl Raw {
    pub fn structure(self) -> Structure {
        let [dots_h, dots_v] = self.dots;
        Structure {
            lines: lines(self.glyphs),
            horizontal: with_dotted(self.horizontal, dots_h),
            vertical: with_dotted(self.vertical, dots_v),
            images: self.images,
            outline_glyphs: self.outline_glyphs,
        }
    }

    /// 바깥 사각 `bounds`(장치 좌표)를 가진 칠·선 하나를 괘선이면 넣는다.
    pub fn add_rule(&mut self, bounds: Box2) {
        let (width, height) = (bounds.x1 - bounds.x0, bounds.y1 - bounds.y0);
        let horizontal = Rule {
            at: (bounds.y0 + bounds.y1) / 2.0,
            from: bounds.x0,
            to: bounds.x1,
        };
        let vertical = Rule {
            at: (bounds.x0 + bounds.x1) / 2.0,
            from: bounds.y0,
            to: bounds.y1,
        };
        if height <= RULE_THICKNESS && width >= RULE_LENGTH {
            self.horizontal.push(horizontal);
        } else if width <= RULE_THICKNESS && height >= RULE_LENGTH {
            self.vertical.push(vertical);
        } else if height <= width / 2.0 && width > 0.0 {
            self.dots[0].push(horizontal);
        } else if width <= height / 2.0 && height > 0.0 {
            self.dots[1].push(vertical);
        }
    }
}

fn lines(mut glyphs: Vec<Glyph>) -> Vec<Line> {
    glyphs.retain(|glyph| normalized(&glyph.text).next().is_some());
    glyphs.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut groups: Vec<Vec<Glyph>> = Vec::new();
    for glyph in glyphs {
        match groups.last_mut() {
            Some(group)
                if (glyph.y - group[0].y).abs()
                    <= SAME_BASELINE * glyph.size.max(group[0].size).max(1.0) =>
            {
                group.push(glyph)
            }
            _ => groups.push(vec![glyph]),
        }
    }
    groups
        .into_iter()
        .map(|mut group| {
            group.sort_by(|a, b| a.x.total_cmp(&b.x));
            // 그림자·외곽선은 같은 글자를 조금씩 비껴 여러 번 찍는다. 겹친 사본은 글자 하나로 본다.
            let mut kept: Vec<Glyph> = Vec::with_capacity(group.len());
            for glyph in group {
                let copy = kept.iter().rev().any(|other| {
                    other.text == glyph.text
                        && (glyph.x - other.x).abs()
                            <= SAME_BASELINE * glyph.size.max(other.size).max(1.0)
                });
                if !copy {
                    kept.push(glyph);
                }
            }
            Line::of(kept)
        })
        .collect()
}

/// 괘선에 점선을 더해 잇는다. 점은 `DOT_GAP` 안으로 늘어선 것만 이어서 괘선 길이가 되면 괘선이다.
fn with_dotted(mut rules: Vec<Rule>, dots: Vec<Rule>) -> Vec<Rule> {
    rules.extend(
        joined(dots, DOT_GAP)
            .into_iter()
            .filter(|rule| rule.to - rule.from >= RULE_LENGTH),
    );
    joined(rules, RULE_JOIN)
}

fn joined(mut rules: Vec<Rule>, gap: f64) -> Vec<Rule> {
    rules.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.from.total_cmp(&b.from)));
    let mut out: Vec<Rule> = Vec::new();
    // 같은 자리끼리 묶은 뒤 구간을 잇는다.
    let mut start = 0;
    while start < rules.len() {
        let mut end = start + 1;
        while end < rules.len() && rules[end].at - rules[start].at <= RULE_SAME_PLACE {
            end += 1;
        }
        let mut row = rules[start..end].to_vec();
        row.sort_by(|a, b| a.from.total_cmp(&b.from));
        let at = row.iter().map(|rule| rule.at).sum::<f64>() / row.len() as f64;
        let mut current: Option<Rule> = None;
        for rule in row {
            match &mut current {
                Some(open) if rule.from <= open.to + gap => open.to = open.to.max(rule.to),
                _ => {
                    out.extend(current.take());
                    current = Some(Rule { at, ..rule });
                }
            }
        }
        out.extend(current);
        start = end;
    }
    out
}

/// 문서 하나의 구조 차이. 모든 수는 개수다.
#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Diff {
    pub reference_lines: usize,
    pub candidate_lines: usize,
    /// 기준에만 있는 줄 + 후보에만 있는 줄. 줄바꿈 자리가 다르면 양쪽에 하나씩 생긴다.
    pub line_break: usize,
    /// 글은 같은데 다른 쪽에 놓인 줄(쪽 나눔·표 행 나눔이 다름).
    pub page_break: usize,
    /// 같은 쪽 같은 글인데 세로 자리가 허용 오차를 넘은 줄(줄 간격·문단 간격·행 높이).
    pub line_y: usize,
    /// 같은 쪽 같은 글인데 가로 시작 자리가 허용 오차를 넘은 줄(들여쓰기·정렬·셀 여백).
    pub line_x: usize,
    /// 짝이 없는 괘선(가로, 세로). 표 행 높이·셀 나눔이 다르면 생긴다.
    pub rules: usize,
    /// 짝이 없는 그림 상자.
    pub images: usize,
    /// 같은 쪽에서 짝지은 줄의 세로·가로 최대 편차(pt, 소수 둘째 자리).
    pub max_line_dy: f64,
    pub max_line_dx: f64,
    /// 짝지은 괘선의 자리(가로선은 y, 세로선은 x) 최대 편차(pt).
    pub max_rule_shift: f64,
    /// 한쪽이 글을 글자 윤곽으로만 그려 견주지 못한 줄(양쪽 합). 불일치에 세지 않고 따로 적는다.
    pub outlined_lines: usize,
    /// 쪽마다 [줄바꿈, 쪽 나눔, 세로, 가로, 괘선, 그림] 불일치 수(기준 쪽 번호, 후보에만 있는 쪽은 뒤에).
    pub pages: Vec<[usize; 6]>,
}

impl Diff {
    pub fn total(&self) -> usize {
        self.line_break + self.page_break + self.line_y + self.line_x + self.rules + self.images
    }
}

type PageLines = Vec<(usize, Line)>;

/// 양쪽의 줄을 쪽 번호와 함께 편다. 어느 한쪽이 같은 쪽 그 자리에 글자를 윤곽으로만 그렸으면
/// 그 글자는 견줄 짝이 PDF에 없으므로 줄에서 뺀다. 글자 절반 넘게 빠진 줄은 통째로 빼고 그 수를 돌려준다.
fn comparable_lines(
    reference: &[&Structure],
    candidate: &[&Structure],
) -> (PageLines, PageLines, usize) {
    let empty = Structure::default();
    let mut outlined = 0;
    let mut flat = |pages: &[&Structure], other: &[&Structure]| -> PageLines {
        let mut out = Vec::new();
        for (page, structure) in pages.iter().enumerate() {
            let facing = other.get(page).copied().unwrap_or(&empty);
            for line in &structure.lines {
                let glyphs = &line.glyphs;
                let seen: Vec<Glyph> = glyphs
                    .iter()
                    .enumerate()
                    .filter(|(index, glyph)| {
                        // 다음 글자가 빈칸 너머에 있으면 빈칸에 그린 윤곽(쉼표·낫표)까지 덮는다.
                        // 글자 폭은 크기를 넘지 않으므로 구간을 크기에서 자른다.
                        let to = glyphs
                            .get(index + 1)
                            .map_or(f64::INFINITY, |next| next.x)
                            .min(glyph.x + glyph.size);
                        // 다른 쪽 윤곽은 이 글자를 그린 것이다. 이 쪽 윤곽은 다른 글자이므로, 글자
                        // 앞 절반을 덮을 때만(같은 글자를 윤곽으로 겹쳐 그림) 뺀다. 자간을 좁히면
                        // 뒤 글자 윤곽이 이 글자 구간 끝에 걸친다.
                        !structure.outlined(glyph, glyph.x, glyph.x + glyph.size / 2.0)
                            && !facing.outlined(glyph, glyph.x, to)
                    })
                    .map(|(_, glyph)| glyph.clone())
                    .collect();
                // 글자 절반 넘게 윤곽인 줄은 남은 조각끼리 견줄 수 없다. 윤곽은 다른 쪽 글자를 덮은 것과
                // 이 쪽이 줄 곁(글자 크기 안)에 그린 것을 함께 센다.
                let hidden = glyphs.len() - seen.len() + structure.outlines_beside(line);
                if seen.len() < hidden {
                    outlined += 1;
                } else if seen.len() == line.glyphs.len() {
                    out.push((page, line.clone()));
                } else {
                    out.push((page, Line::of(seen)));
                }
            }
        }
        out
    };
    let expected = flat(reference, candidate);
    let actual = flat(candidate, reference);
    let split_expected = split_like(&expected, &actual);
    let split_actual = split_like(&actual, &expected);
    (split_expected, split_actual, outlined)
}

/// 기준선이 한 끗(글자 크기의 10%) 언저리로 다른 이웃 칸 글은 반올림 차이로 한쪽에서는 한 줄,
/// 다른 쪽에서는 두 줄로 묶인다. 한 줄의 글자가 다른 쪽 같은 쪽의 줄 여럿으로 빈틈없이 나뉘고
/// 조각마다 글이 같고 자리가 허용 안이면 그 조각들로 나눈다. 글이 같아야 하므로 줄바꿈 차이는 가려지지 않는다.
fn split_like(lines: &PageLines, other: &PageLines) -> PageLines {
    let mut out = Vec::with_capacity(lines.len());
    for (page, line) in lines {
        let facing: Vec<&Line> = other
            .iter()
            .filter(|(other_page, _)| other_page == page)
            .map(|(_, other)| other)
            .collect();
        match pieces(line, &facing) {
            Some(mut pieces) if pieces.len() > 1 => {
                // `lines`처럼 가장 위 글자의 기준선 차례로 놓아야 다른 쪽 줄 차례와 맞는다.
                let top = |piece: &Line| {
                    piece
                        .glyphs
                        .iter()
                        .map(|glyph| glyph.y)
                        .fold(f64::INFINITY, f64::min)
                };
                pieces.sort_by(|a, b| top(a).total_cmp(&top(b)).then(a.x.total_cmp(&b.x)));
                out.extend(pieces.into_iter().map(|piece| (*page, piece)))
            }
            _ => out.push((*page, line.clone())),
        }
    }
    out
}

/// `line`의 글자를 왼쪽부터 `facing` 줄과 글·자리가 맞는 조각으로 빈틈없이 나눈다. 못 나누면 `None`.
fn pieces(line: &Line, facing: &[&Line]) -> Option<Vec<Line>> {
    let glyphs = &line.glyphs;
    let mut out = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let first = &glyphs[start];
        let end = facing.iter().find_map(|other| {
            if (other.x - first.x).abs() > PLACE_TOLERANCE
                || (other.y - first.y).abs() > PLACE_TOLERANCE
            {
                return None;
            }
            let mut text = String::new();
            for (index, glyph) in glyphs.iter().enumerate().skip(start) {
                text.extend(normalized(&glyph.text));
                if text.len() >= other.text.len() {
                    return (text == other.text).then_some(index + 1);
                }
            }
            None
        })?;
        out.push(Line::of(glyphs[start..end].to_vec()));
        start = end;
    }
    Some(out)
}

pub fn compare(reference: &[&Structure], candidate: &[&Structure]) -> Diff {
    let pages = reference.len().max(candidate.len());
    let mut diff = Diff {
        pages: vec![[0; 6]; pages],
        ..Diff::default()
    };
    let (expected, actual, outlined) = comparable_lines(reference, candidate);
    diff.outlined_lines = outlined;
    diff.reference_lines = expected.len();
    diff.candidate_lines = actual.len();
    let pairs = lcs(&expected, &actual);
    let mut matched_expected = vec![false; expected.len()];
    let mut matched_actual = vec![false; actual.len()];
    for (left, right) in pairs {
        matched_expected[left] = true;
        matched_actual[right] = true;
        let ((page, line), (other_page, other)) = (&expected[left], &actual[right]);
        if page == other_page {
            diff.max_line_dy = diff.max_line_dy.max((line.y - other.y).abs());
            diff.max_line_dx = diff.max_line_dx.max((line.x - other.x).abs());
        }
        if page != other_page {
            diff.page_break += 1;
            diff.pages[*page][1] += 1;
        } else if (line.y - other.y).abs() > PLACE_TOLERANCE {
            diff.line_y += 1;
            diff.pages[*page][2] += 1;
        } else if (line.x - other.x).abs() > PLACE_TOLERANCE {
            diff.line_x += 1;
            diff.pages[*page][3] += 1;
        }
    }
    for (index, (page, _)) in expected.iter().enumerate() {
        if !matched_expected[index] {
            diff.line_break += 1;
            diff.pages[*page][0] += 1;
        }
    }
    for (index, (page, _)) in actual.iter().enumerate() {
        if !matched_actual[index] {
            diff.line_break += 1;
            diff.pages[*page][0] += 1;
        }
    }
    for page in 0..pages {
        let empty = Structure::default();
        let left = reference.get(page).copied().unwrap_or(&empty);
        let right = candidate.get(page).copied().unwrap_or(&empty);
        let (horizontal, shift_h) =
            unmatched(&left.horizontal, &right.horizontal, rule_close, rule_shift);
        let (vertical, shift_v) =
            unmatched(&left.vertical, &right.vertical, rule_close, rule_shift);
        let (images, _) = unmatched(&left.images, &right.images, box_close, |_, _| 0.0);
        let rules = horizontal + vertical;
        diff.max_rule_shift = diff.max_rule_shift.max(shift_h).max(shift_v);
        diff.rules += rules;
        diff.images += images;
        diff.pages[page][4] += rules;
        diff.pages[page][5] += images;
    }
    diff
}

/// 진단 글: 짝이 없는 줄과 괘선·그림을 쪽마다 적는다. 문서 글이 들어가므로 저장소 밖에만 쓴다.
/// 줄은 `R`(기준에만)·`C`(후보에만), 괘선은 `h`/`v`와 자리·구간(pt)이다.
pub fn report(reference: &[&Structure], candidate: &[&Structure]) -> String {
    use std::fmt::Write;
    let (expected, actual, _) = comparable_lines(reference, candidate);
    let pairs = lcs(&expected, &actual);
    let mut out = String::new();
    let mut seen_left = vec![false; expected.len()];
    let mut seen_right = vec![false; actual.len()];
    for &(left, right) in &pairs {
        seen_left[left] = true;
        seen_right[right] = true;
        let ((page, a), (other_page, b)) = (&expected[left], &actual[right]);
        if page != other_page
            || (a.y - b.y).abs() > PLACE_TOLERANCE
            || (a.x - b.x).abs() > PLACE_TOLERANCE
        {
            let _ = writeln!(
                out,
                "moved p{page}->{other_page} y {:.2}->{:.2} x {:.2}->{:.2} {}",
                a.y, b.y, a.x, b.x, a.text
            );
        }
    }
    for (side, lines, seen) in [("R", &expected, &seen_left), ("C", &actual, &seen_right)] {
        for (index, (page, line)) in lines.iter().enumerate() {
            if !seen[index] {
                let _ = writeln!(
                    out,
                    "{side} p{page} y {:.2} x {:.2} {}",
                    line.y, line.x, line.text
                );
            }
        }
    }
    let pages = reference.len().max(candidate.len());
    let empty = Structure::default();
    for page in 0..pages {
        let left = reference.get(page).copied().unwrap_or(&empty);
        let right = candidate.get(page).copied().unwrap_or(&empty);
        for (kind, l, r) in [
            ("h", &left.horizontal, &right.horizontal),
            ("v", &left.vertical, &right.vertical),
        ] {
            let (only_left, only_right) = if std::env::var_os("SCORE_DUMP_ALL_RULES").is_some() {
                (l.iter().collect(), r.iter().collect())
            } else {
                unmatched_items(l, r, rule_close)
            };
            for (side, items) in [("R", only_left), ("C", only_right)] {
                for rule in items {
                    let _ = writeln!(
                        out,
                        "{side} p{page} {kind} at {:.2} {:.2}..{:.2}",
                        rule.at, rule.from, rule.to
                    );
                }
            }
        }
        let (only_left, only_right) = unmatched_items(&left.images, &right.images, box_close);
        for (side, items) in [("R", only_left), ("C", only_right)] {
            for b in items {
                let _ = writeln!(
                    out,
                    "{side} p{page} img {:.2},{:.2} {:.2},{:.2}",
                    b.x0, b.y0, b.x1, b.y1
                );
            }
        }
    }
    out
}

fn unmatched_items<'a, T>(
    left: &'a [T],
    right: &'a [T],
    close: fn(&T, &T) -> bool,
) -> (Vec<&'a T>, Vec<&'a T>) {
    let mut used = vec![false; right.len()];
    let mut only_left = Vec::new();
    for item in left {
        match (0..right.len()).find(|&index| !used[index] && close(item, &right[index])) {
            Some(index) => used[index] = true,
            None => only_left.push(item),
        }
    }
    let only_right = right
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(item, _)| item)
        .collect();
    (only_left, only_right)
}

fn rule_close(a: &Rule, b: &Rule) -> bool {
    (a.at - b.at).abs() <= PLACE_TOLERANCE
        && (a.from - b.from).abs() <= PLACE_TOLERANCE
        && (a.to - b.to).abs() <= PLACE_TOLERANCE
}

fn rule_shift(a: &Rule, b: &Rule) -> f64 {
    (a.at - b.at).abs()
}

fn box_close(a: &Box2, b: &Box2) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|delta| delta.abs() <= PLACE_TOLERANCE)
}

/// 양쪽에서 짝을 못 찾은 것의 수와, 짝지은 것의 `shift` 최대값. 하나는 한 번만 짝짓는다.
fn unmatched<T>(
    left: &[T],
    right: &[T],
    close: fn(&T, &T) -> bool,
    shift: fn(&T, &T) -> f64,
) -> (usize, f64) {
    let mut used = vec![false; right.len()];
    let (mut missing, mut largest) = (0, 0.0f64);
    for item in left {
        match (0..right.len()).find(|&index| !used[index] && close(item, &right[index])) {
            Some(index) => {
                used[index] = true;
                largest = largest.max(shift(item, &right[index]));
            }
            None => missing += 1,
        }
    }
    (
        missing + used.iter().filter(|used| !**used).count(),
        largest,
    )
}

/// 글이 같은 줄의 최장 공통 부분열. 돌려주는 짝은 (기준 번호, 후보 번호).
fn lcs(left: &[(usize, Line)], right: &[(usize, Line)]) -> Vec<(usize, usize)> {
    let (n, m) = (left.len(), right.len());
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if left[i].1.text == right[j].1.text {
                table[at(i + 1, j + 1)] + 1
            } else {
                table[at(i + 1, j)].max(table[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < n && j < m {
        if left[i].1.text == right[j].1.text {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs(text: &str, x: f64, y: f64) -> Vec<Glyph> {
        text.chars()
            .enumerate()
            .map(|(index, character)| Glyph {
                text: character.to_string(),
                x: x + 10.0 * index as f64,
                y,
                size: 10.0,
            })
            .collect()
    }

    fn page(lines: &[(&str, f64)]) -> Structure {
        Raw {
            glyphs: lines
                .iter()
                .flat_map(|(text, y)| glyphs(text, 50.0, *y))
                .collect(),
            ..Raw::default()
        }
        .structure()
    }

    #[test]
    fn glyphs_on_one_baseline_make_one_line_in_x_order() {
        let mut raw = Raw::default();
        raw.glyphs.extend(glyphs("다라", 70.0, 100.0));
        raw.glyphs.extend(glyphs("가나", 50.0, 100.4));
        raw.glyphs.extend(glyphs("마", 50.0, 120.0));
        let lines = raw.structure().lines;
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["가나다라", "마"]
        );
    }

    #[test]
    fn identical_pages_have_no_structure_difference() {
        let a = page(&[("가나다", 100.0), ("라마", 120.0)]);
        assert_eq!(compare(&[&a], &[&a]).total(), 0);
    }

    #[test]
    fn moved_break_counts_on_both_sides() {
        let a = page(&[("가나다", 100.0), ("라마", 120.0)]);
        let b = page(&[("가나", 100.0), ("다라마", 120.0)]);
        let diff = compare(&[&a], &[&b]);
        assert_eq!(diff.line_break, 4);
        assert_eq!(diff.pages[0][0], 4);
    }

    #[test]
    fn line_pushed_to_next_page_is_a_page_break_difference() {
        let a = page(&[("가나", 100.0), ("다라", 120.0)]);
        let b1 = page(&[("가나", 100.0)]);
        let b2 = page(&[("다라", 100.0)]);
        let diff = compare(&[&a], &[&b1, &b2]);
        assert_eq!((diff.page_break, diff.line_break), (1, 0));
    }

    #[test]
    fn shifted_line_beyond_tolerance_is_a_vertical_difference() {
        let a = page(&[("가나", 100.0), ("다라", 120.0)]);
        let near = page(&[("가나", 100.8), ("다라", 120.0)]);
        let far = page(&[("가나", 100.0), ("다라", 121.5)]);
        let diff = compare(&[&a], &[&near]);
        assert_eq!(diff.total(), 0);
        assert!((diff.max_line_dy - 0.8).abs() < 1e-9);
        assert_eq!(compare(&[&a], &[&far]).line_y, 1);
    }

    #[test]
    fn per_cell_borders_join_into_one_rule() {
        let mut raw = Raw::default();
        for x in [0.0, 50.0, 100.0] {
            raw.add_rule(Box2 {
                x0: x,
                y0: 99.8,
                x1: x + 50.0,
                y1: 100.2,
            });
        }
        let mut whole = Raw::default();
        whole.add_rule(Box2 {
            x0: 0.0,
            y0: 99.9,
            x1: 150.0,
            y1: 100.1,
        });
        let (a, b) = (raw.structure(), whole.structure());
        assert_eq!(a.horizontal.len(), 1);
        assert_eq!(compare(&[&a], &[&b]).rules, 0);
    }

    #[test]
    fn double_line_strands_stay_apart_whatever_the_float_noise() {
        // 0.6pt 간격 겹선이 f32 오차로 한쪽은 0.5999756, 다른 쪽은 0.6000061만큼 떨어져 있었다.
        let strands = |gap: f64| {
            let mut raw = Raw::default();
            for y in [228.12, 228.12 + gap] {
                raw.add_rule(Box2 {
                    x0: 66.0,
                    y0: y - 0.12,
                    x1: 535.0,
                    y1: y + 0.12,
                });
            }
            raw.structure().horizontal.len()
        };
        assert_eq!(strands(0.599_975_6), 2);
        assert_eq!(strands(0.600_006_1), 2);
        assert_eq!(strands(0.48), 1);

        // 길이 방향으로 0.59pt 띄어 그린 칸 테두리(227.53..228.12)는 그대로 한 줄이다.
        let mut pieces = Raw::default();
        for (y0, y1) in [(193.73, 227.53), (228.12, 301.94)] {
            pieces.add_rule(Box2 {
                x0: 65.57,
                y0,
                x1: 66.19,
                y1,
            });
        }
        assert_eq!(pieces.structure().vertical.len(), 1);
    }

    #[test]
    fn double_lines_at_measured_positions_pair_up() {
        // 또렷과 한/글의 선 자리는 겹선 간격 0.6pt에서 1e-13에서 1e-5pt까지 어긋난다. 그 어긋남으로
        // 묶음이 양쪽에서 갈리면, 허용 오차 안에 짝이 있는 선도 짝 없는 선으로 세게 된다.
        let rules = |ats: &[f64], from: f64, to: f64| -> Vec<Rule> {
            ats.iter().map(|&at| Rule { at, from, to }).collect()
        };
        let pair = |left: Vec<Rule>, right: Vec<Rule>| {
            let reference = Structure {
                horizontal: with_dotted(left, Vec::new()),
                ..Structure::default()
            };
            let candidate = Structure {
                horizontal: with_dotted(right, Vec::new()),
                ..Structure::default()
            };
            compare(&[&reference], &[&candidate]).rules
        };
        assert_eq!(
            pair(
                rules(&[227.28, 227.76, 228.12, 228.72], 66.12, 535.2),
                rules(
                    &[
                        227.2800048828125,
                        227.8799948828125,
                        228.2399948828125,
                        228.8400048828125
                    ],
                    65.57,
                    535.46
                ),
            ),
            0
        );
        assert_eq!(
            pair(
                rules(&[535.3199999999999, 535.92], 193.32, 302.76),
                rules(&[535.32, 535.92], 193.3, 302.71),
            ),
            0
        );
    }

    #[test]
    fn shadow_copies_and_sub_point_offsets_stay_one_line() {
        // 한/글은 그림자를 0.12pt씩 비낀 사본으로 찍고, 세로 정렬이 다른 칸 글자를 0.1pt 남짓 어긋나게 놓는다.
        let mut text = glyphs("AB", 10.0, 50.0);
        text.extend(glyphs("AB", 10.12, 50.12));
        text.extend(glyphs("CD", 40.0, 50.4));
        let lines = lines(text);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "ABCD");
    }

    #[test]
    fn only_glyphs_under_the_other_sides_outline_are_left_out() {
        // 한/글은 ① 같은 글자를 윤곽으로만 그리고 뒤 글자는 글로 쓴다. 또렷은 ①도 글로 쓴다.
        let mut reference = page(&[("노인", 100.0)]);
        reference.lines[0] = Line::of(glyphs("노인", 72.0, 100.0));
        reference.outline_glyphs.push(
            Box2 {
                x0: 51.0,
                y0: 90.0,
                x1: 61.0,
                y1: 101.0,
            }
            .into(),
        );
        let mut candidate = Raw::default();
        candidate.glyphs.extend(glyphs("①", 50.0, 100.0));
        candidate.glyphs.extend(glyphs("노인", 72.0, 100.0));
        let candidate = candidate.structure();
        let diff = compare(&[&reference], &[&candidate]);
        assert_eq!((diff.total(), diff.outlined_lines), (0, 0));
        // 윤곽 바로 뒤에 이어 쓴 글자는 덮인 것으로 보지 않고, 기준점이 윤곽 앞에 있어도 구간이 윤곽을
        // 품으면 덮였다.
        let glyph = &glyphs("노", 61.0, 100.0)[0];
        assert!(!reference.outlined(glyph, 61.0, 71.0));
        assert!(reference.outlined(glyph, 48.5, 58.5));
    }

    #[test]
    fn glyph_before_a_space_keeps_clear_of_an_outlined_comma_after_it() {
        // 한/글은 빈칸 뒤 쉼표를 윤곽으로 그리고, 또렷은 글로 쓴다. 쉼표 앞 낱말 끝 글자는 남는다.
        let glyph = |text: &str, x: f64| Glyph {
            text: text.to_string(),
            x,
            y: 100.0,
            size: 10.0,
        };
        let reference = Raw {
            glyphs: vec![glyph("가", 50.0), glyph("나", 80.0)],
            outline_glyphs: vec![
                Box2 {
                    x0: 65.0,
                    y0: 98.0,
                    x1: 68.0,
                    y1: 102.0,
                }
                .into(),
            ],
            ..Raw::default()
        }
        .structure();
        let candidate = Raw {
            glyphs: vec![glyph("가", 50.0), glyph(",", 64.0), glyph("나", 80.0)],
            ..Raw::default()
        }
        .structure();
        assert_eq!(compare(&[&reference], &[&candidate]).total(), 0);
    }

    #[test]
    fn own_outlined_comma_under_tight_spacing_keeps_the_glyph_before_it() {
        // 자간을 좁힌 줄에서 한/글이 윤곽으로 그린 쉼표는 앞 글자 구간 끝에 걸친다. 앞 글자는 남는다.
        let glyph = |text: &str, x: f64| Glyph {
            text: text.to_string(),
            x,
            y: 100.0,
            size: 10.0,
        };
        let comma = Box2 {
            x0: 58.0,
            y0: 98.0,
            x1: 60.0,
            y1: 102.0,
        };
        let reference = Raw {
            glyphs: vec![glyph("가", 41.0), glyph("나", 50.0), glyph("다", 70.0)],
            outline_glyphs: vec![comma.into()],
            ..Raw::default()
        }
        .structure();
        let candidate = Raw {
            glyphs: vec![
                glyph("가", 41.0),
                glyph("나", 50.0),
                glyph(",", 58.0),
                glyph("다", 70.0),
            ],
            ..Raw::default()
        }
        .structure();
        assert_eq!(compare(&[&reference], &[&candidate]).total(), 0);
    }

    #[test]
    fn outline_above_the_baseline_counts_and_a_neighbours_edge_does_not() {
        let glyph = &glyphs("가", 50.0, 100.0)[0];
        let quote = Box2 {
            x0: 51.0,
            y0: 91.0,
            x1: 53.0,
            y1: 94.0,
        };
        let neighbour = Box2 {
            x0: 59.0,
            y0: 92.0,
            x1: 65.0,
            y1: 100.0,
        };
        let page = |b: Box2| Structure {
            outline_glyphs: vec![b.into()],
            ..Structure::default()
        };
        // 작은따옴표 윤곽은 기준선보다 6pt 위에 있어도 이 글자 자리다.
        assert!(page(quote).outlined(glyph, 50.0, 60.0));
        // 다음 글자 윤곽이 구간 끝 1pt에 걸친 것은 덮임이 아니다.
        assert!(!page(neighbour).outlined(glyph, 50.0, 60.0));
    }

    #[test]
    fn bullet_before_a_word_drawn_in_outlines_is_left_out_on_both_sides() {
        // 한/글은 "□ 개요"의 □만 글로 쓰고 "개요"는 윤곽으로 그린다. 또렷은 모두 글로 쓴다.
        let glyph = |text: &str, x: f64| Glyph {
            text: text.to_string(),
            x,
            y: 100.0,
            size: 10.0,
        };
        let outline = |x0: f64| Box2 {
            x0,
            y0: 91.0,
            x1: x0 + 10.0,
            y1: 100.0,
        };
        let reference = Raw {
            glyphs: vec![glyph("□", 50.0)],
            outline_glyphs: vec![outline(68.0).into(), outline(83.0).into()],
            ..Raw::default()
        }
        .structure();
        let candidate = Raw {
            glyphs: vec![glyph("□", 50.0), glyph("개", 67.0), glyph("요", 82.0)],
            ..Raw::default()
        }
        .structure();
        let diff = compare(&[&reference], &[&candidate]);
        assert_eq!((diff.total(), diff.outlined_lines), (0, 2));
    }

    #[test]
    fn bracket_inked_at_the_right_of_its_cell_still_joins_the_outlined_line() {
        // 한/글은 "○ 「법」"의 ○만 글로 쓰고 「법」은 윤곽으로 그린다. 「 잉크는 칸 오른쪽에 몰려
        // ○ 끝에서 글자 크기보다 멀리서 시작하지만, 펜 자리는 그 안이다.
        let glyph = |text: &str, x: f64| Glyph {
            text: text.to_string(),
            x,
            y: 100.0,
            size: 10.0,
        };
        let outline = |x0: f64, origin: f64| Outline {
            ink: Box2 {
                x0,
                y0: 91.0,
                x1: x0 + 3.0,
                y1: 100.0,
            },
            origin,
        };
        let reference = Raw {
            glyphs: vec![glyph("○", 50.0)],
            outline_glyphs: vec![outline(71.0, 64.0), outline(75.0, 74.0)],
            ..Raw::default()
        }
        .structure();
        let candidate = Raw {
            glyphs: vec![glyph("○", 50.0), glyph("「", 64.0), glyph("법", 74.0)],
            ..Raw::default()
        }
        .structure();
        let diff = compare(&[&reference], &[&candidate]);
        assert_eq!((diff.total(), diff.outlined_lines), (0, 2));
    }

    #[test]
    fn neighbouring_cells_on_either_side_of_the_baseline_edge_compare_piece_by_piece() {
        // 이웃 칸 기준선이 기준에서는 1.1pt(한 줄 한계 1pt 바깥), 또렷에서는 0.9pt(안) 다르다.
        let mut reference = glyphs("만풍배", 50.0, 100.0);
        reference.extend(glyphs("과수", 150.0, 98.9));
        let mut candidate = glyphs("만풍배", 50.0, 100.0);
        candidate.extend(glyphs("과수", 150.0, 99.1));
        let reference = Raw {
            glyphs: reference,
            ..Raw::default()
        }
        .structure();
        let candidate = Raw {
            glyphs: candidate,
            ..Raw::default()
        }
        .structure();
        assert_eq!((reference.lines.len(), candidate.lines.len()), (2, 1));
        assert_eq!(compare(&[&reference], &[&candidate]).total(), 0);
        // 글이 다르면 나누지 않고 줄바꿈 차이로 센다.
        let moved = page(&[("만풍배과", 100.0)]);
        assert!(compare(&[&reference], &[&moved]).line_break > 0);
    }

    #[test]
    fn dotted_line_drawn_dot_by_dot_is_one_rule() {
        // 한/글은 점선을 0.48pt 점마다 따로 긋는다. 한 번에 그은 점선과 같은 괘선이다.
        let mut dots = Raw::default();
        for step in 0..50 {
            let y = 100.0 + f64::from(step) * 1.2;
            dots.add_rule(Box2 {
                x0: 60.0,
                y0: y,
                x1: 60.0,
                y1: y + 0.48,
            });
        }
        let mut whole = Raw::default();
        whole.add_rule(Box2 {
            x0: 60.0,
            y0: 100.0,
            x1: 60.0,
            y1: 159.28,
        });
        let (a, b) = (dots.structure(), whole.structure());
        assert_eq!(a.vertical.len(), 1);
        assert_eq!(compare(&[&a], &[&b]).rules, 0);
        // 홀로 떨어진 짧은 토막은 괘선이 아니다.
        let mut lone = Raw::default();
        lone.add_rule(Box2 {
            x0: 10.0,
            y0: 10.0,
            x1: 12.0,
            y1: 10.0,
        });
        assert!(lone.structure().horizontal.is_empty());
    }

    #[test]
    fn taller_row_moves_a_rule() {
        let mut a = Raw::default();
        a.add_rule(Box2 {
            x0: 0.0,
            y0: 100.0,
            x1: 150.0,
            y1: 100.4,
        });
        let mut b = Raw::default();
        b.add_rule(Box2 {
            x0: 0.0,
            y0: 102.0,
            x1: 150.0,
            y1: 102.4,
        });
        assert_eq!(compare(&[&a.structure()], &[&b.structure()]).rules, 2);
    }
}
