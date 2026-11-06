//! 채점 수치의 정의. 바꾸면 이전 수치와 비교할 수 없으니 정의를 바꿀 때는 PR에 적는다.

use crate::scan::Page;
use std::collections::{BTreeMap, HashMap};
use unicode_normalization::UnicodeNormalization;

/// 이 값보다 어두운 화소를 잉크로 본다(0…255). 72dpi에서 0.1mm 선도 잡히게 밝게 잡았다.
pub const INK_BELOW: u8 = 192;
/// 잉크 F1에서 이만큼(화소, 체비셰프 거리) 떨어진 잉크까지 맞은 것으로 본다.
pub const INK_RADIUS: usize = 1;

/// 글자 비교 전 정규화: NFKC, 공백·제어 문자·U+FFFD를 빼고 줄표 무리를 하나로 본다.
///
/// 한컴 글꼴은 줄표 여럿을 한 글리프에 묶는다(H2HDRM: U+2013과 U+00AD가 글리프 135).
/// 한/글 PDF의 ToUnicode는 그 글리프를 U+00AD로 되읽으므로 줄표 차이는 글꼴 몫이다.
pub fn normalized(text: &str) -> impl Iterator<Item = char> + '_ {
    text.nfkc()
        .filter(|character| {
            !character.is_whitespace() && !character.is_control() && *character != '\u{FFFD}'
        })
        .map(|character| match character {
            '\u{AD}' | '\u{2010}'..='\u{2015}' => '\u{2010}',
            other => other,
        })
}

#[derive(Default, Clone, Copy)]
pub struct Overlap {
    pub reference: usize,
    pub candidate: usize,
    pub matched: usize,
}

impl Overlap {
    pub fn f1(self) -> f64 {
        let total = self.reference + self.candidate;
        if total == 0 {
            1.0
        } else {
            2.0 * self.matched as f64 / total as f64
        }
    }

    pub fn recall(self) -> f64 {
        ratio(self.matched as u64, self.reference as u64)
    }

    pub fn precision(self) -> f64 {
        ratio(self.matched as u64, self.candidate as u64)
    }
}

fn ratio(matched: u64, total: u64) -> f64 {
    if total == 0 {
        1.0
    } else {
        matched as f64 / total as f64
    }
}

/// 글자 다중집합의 겹침. 차례는 보지 않는다.
pub fn text_overlap(reference: &str, candidate: &str) -> Overlap {
    let mut counts: HashMap<char, isize> = HashMap::new();
    let mut overlap = Overlap::default();
    for character in normalized(reference) {
        overlap.reference += 1;
        *counts.entry(character).or_default() += 1;
    }
    for character in normalized(candidate) {
        overlap.candidate += 1;
        let count = counts.entry(character).or_default();
        if *count > 0 {
            overlap.matched += 1;
        }
        *count -= 1;
    }
    overlap
}

/// 진단용 글자 종류. 정규화한 뒤의 글자로 가른다(NFKC는 ①을 1로, ㉠을 ᄀ으로 바꾼다).
pub fn char_class(character: char) -> &'static str {
    match character {
        '가'..='힣' => "hangul",
        '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' => "jamo",
        '0'..='9' => "digit",
        'a'..='z' | 'A'..='Z' => "latin",
        '!'..='~' => "ascii-punct",
        '\u{A0}'..='\u{FF}' => "latin1-symbol",
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' => "hanja",
        '\u{2000}'..='\u{206F}' | '\u{3000}'..='\u{303F}' => "punct",
        '\u{2190}'..='\u{23FF}' => "math-arrow",
        '\u{2460}'..='\u{24FF}' | '\u{25A0}'..='\u{27BF}' | '\u{3200}'..='\u{32FF}' => "symbol",
        '\u{E000}'..='\u{F8FF}' => "private-use",
        _ => "other",
    }
}

/// 기준에만 있는 글자(빠짐)와 후보에만 있는 글자(남음)를 `char_class`별로 센다.
pub fn text_gaps(reference: &str, candidate: &str) -> BTreeMap<&'static str, [usize; 2]> {
    let mut counts: HashMap<char, isize> = HashMap::new();
    for character in normalized(reference) {
        *counts.entry(character).or_default() += 1;
    }
    for character in normalized(candidate) {
        *counts.entry(character).or_default() -= 1;
    }
    let mut gaps: BTreeMap<&'static str, [usize; 2]> = BTreeMap::new();
    for (character, count) in counts {
        let side = usize::from(count < 0);
        gaps.entry(char_class(character)).or_default()[side] += count.unsigned_abs();
    }
    gaps.retain(|_, [missing, extra]| *missing + *extra > 0);
    gaps
}

/// 한 쪽의 화소 비교. 두 래스터를 왼쪽 위에 맞추고, 한쪽에만 있는 영역은 최대 차이(255)로 센다.
/// 단, 크기 차이가 `SAME_PAPER` 이내면 같은 종이로 보고 모자란 곳을 흰 바탕으로 센다.
#[derive(Default, Clone, Copy)]
pub struct PixelSums {
    /// Σ|기준 − 후보|(0…255 단위).
    pub difference: u64,
    /// Σ(255 − 기준): 빈 쪽을 냈을 때의 차이.
    pub reference_ink: u64,
    pub pixels: u64,
    pub ink: InkSums,
}

impl PixelSums {
    pub fn add(self, other: PixelSums) -> PixelSums {
        PixelSums {
            difference: self.difference + other.difference,
            reference_ink: self.reference_ink + other.reference_ink,
            pixels: self.pixels + other.pixels,
            ink: self.ink.add(other.ink),
        }
    }

    /// 화소당 평균 차이, 0(같음)…1(흑백이 정반대).
    pub fn mean_difference(self) -> f64 {
        if self.pixels == 0 {
            0.0
        } else {
            self.difference as f64 / (255.0 * self.pixels as f64)
        }
    }
}

/// 한/글 PDF는 A4(595.28×841.89pt)를 정수 pt로 적는다: 윈도 한/글은 595×841, 맥 한/글은 595×842.
/// 이만큼의 화소 차이는 종이가 다른 것이 아니라 크기를 적을 때 생긴 끝수다.
const SAME_PAPER: usize = 1;

pub fn compare_pixels(reference: &Page, candidate: &Page) -> PixelSums {
    let width = reference.width.max(candidate.width);
    let height = reference.height.max(candidate.height);
    let same_paper = reference.width.abs_diff(candidate.width) <= SAME_PAPER
        && reference.height.abs_diff(candidate.height) <= SAME_PAPER;
    let outside = same_paper.then_some(u8::MAX);
    let sample = |page: &Page, x: usize, y: usize| -> Option<u8> {
        if x < page.width && y < page.height {
            Some(page.gray[y * page.width + x])
        } else {
            outside
        }
    };
    let mut sums = PixelSums {
        pixels: (width * height) as u64,
        ..PixelSums::default()
    };
    for y in 0..height {
        for x in 0..width {
            let expected = sample(reference, x, y);
            let actual = sample(candidate, x, y);
            sums.difference += match (expected, actual) {
                (Some(a), Some(b)) => u64::from(a.abs_diff(b)),
                _ => 255,
            };
            sums.reference_ink += u64::from(u8::MAX - expected.unwrap_or(u8::MAX));
        }
    }
    sums.ink = ink_overlap(reference, candidate, width, height);
    sums
}

/// 잉크 화소의 겹침. 기준 잉크 반경 안에 후보 잉크가 있으면 재현, 그 반대면 정밀이다.
#[derive(Default, Clone, Copy)]
pub struct InkSums {
    pub reference: u64,
    pub candidate: u64,
    pub recalled: u64,
    pub precise: u64,
}

impl InkSums {
    pub fn add(self, other: InkSums) -> InkSums {
        InkSums {
            reference: self.reference + other.reference,
            candidate: self.candidate + other.candidate,
            recalled: self.recalled + other.recalled,
            precise: self.precise + other.precise,
        }
    }

    pub fn f1(self) -> f64 {
        if self.reference + self.candidate == 0 {
            return 1.0;
        }
        let precision = ratio(self.precise, self.candidate);
        let recall = ratio(self.recalled, self.reference);
        if precision + recall == 0.0 {
            0.0
        } else {
            2.0 * precision * recall / (precision + recall)
        }
    }
}

fn ink_overlap(reference: &Page, candidate: &Page, width: usize, height: usize) -> InkSums {
    let reference_mask = mask(reference, width, height);
    let candidate_mask = mask(candidate, width, height);
    let reference_near = dilate(&reference_mask, width, height);
    let candidate_near = dilate(&candidate_mask, width, height);
    let count = |mask: &[bool]| mask.iter().filter(|value| **value).count() as u64;
    let hits = |mask: &[bool], near: &[bool]| {
        mask.iter()
            .zip(near)
            .filter(|(inked, close)| **inked && **close)
            .count() as u64
    };
    InkSums {
        reference: count(&reference_mask),
        candidate: count(&candidate_mask),
        recalled: hits(&reference_mask, &candidate_near),
        precise: hits(&candidate_mask, &reference_near),
    }
}

/// 진단용으로 후보를 통째로 옮겨 볼 거리(화소). 이 범위의 모든 (dx, dy)를 센다.
pub const SHIFT_RADIUS: i32 = 10;
pub const SHIFT_SIDE: usize = (2 * SHIFT_RADIUS + 1) as usize;

/// 후보 잉크를 (dx, dy)만큼 옮겼을 때 기준 잉크에 그대로 겹치는 화소 수. 줄 우선으로 SHIFT_SIDE² 칸.
/// 겹침이 (0, 0)이 아닌 곳에서 가장 크면 원점·기준선이 어긋난 것이다.
pub fn shift_hits(reference: &Page, candidate: &Page) -> Vec<u64> {
    let mut hits = vec![0u64; SHIFT_SIDE * SHIFT_SIDE];
    let inked = |page: &Page, x: i32, y: i32| {
        x >= 0
            && y >= 0
            && (x as usize) < page.width
            && (y as usize) < page.height
            && page.gray[y as usize * page.width + x as usize] < INK_BELOW
    };
    for y in 0..candidate.height {
        for x in 0..candidate.width {
            if candidate.gray[y * candidate.width + x] >= INK_BELOW {
                continue;
            }
            for dy in -SHIFT_RADIUS..=SHIFT_RADIUS {
                for dx in -SHIFT_RADIUS..=SHIFT_RADIUS {
                    if inked(reference, x as i32 + dx, y as i32 + dy) {
                        hits[((dy + SHIFT_RADIUS) as usize) * SHIFT_SIDE
                            + (dx + SHIFT_RADIUS) as usize] += 1;
                    }
                }
            }
        }
    }
    hits
}

/// 축소 인쇄를 찾을 배율 범위(%). 한/글 모아 찍기 2쪽은 약 71%(1/√2)다.
pub const PRINT_SCALES: std::ops::RangeInclusive<u32> = 50..=100;
/// 가장 잘 맞는 배율의 잉크 F1이 100%일 때의 이 배 이상이면 기준 PDF가 축소 인쇄된 것으로 본다.
pub const PRINT_SCALE_GAIN: f64 = 1.5;

/// 기준 쪽이 원래 쪽을 왼쪽 위 기준으로 줄여 찍은 것인지 잰다(모아 찍기·축소 인쇄).
/// 후보 쪽을 `PRINT_SCALES`의 배율마다 줄여 잉크 F1이 가장 높은 배율과 그 F1, 100%일 때의 F1을 낸다.
pub fn print_scale(reference: &Page, candidate: &Page) -> (u32, f64, f64) {
    let f1_at = |percent: u32| {
        let scaled = scaled_page(candidate, percent as f32 / 100.0, reference);
        compare_pixels(reference, &scaled).ink.f1()
    };
    let unscaled = f1_at(100);
    let (best, best_f1) = PRINT_SCALES.map(|percent| (percent, f1_at(percent))).fold(
        (100, unscaled),
        |best, next| if next.1 > best.1 { next } else { best },
    );
    (best, best_f1, unscaled)
}

/// `page`를 왼쪽 위 기준으로 `scale`배 줄인 래스터. 크기는 `like`와 같다.
fn scaled_page(page: &Page, scale: f32, like: &Page) -> Page {
    let mut out = Page::blank(like.width, like.height);
    for y in 0..like.height {
        for x in 0..like.width {
            let (sx, sy) = ((x as f32 / scale) as usize, (y as f32 / scale) as usize);
            if sx < page.width && sy < page.height {
                out.gray[y * like.width + x] = page.gray[sy * page.width + sx];
            }
        }
    }
    out
}

fn mask(page: &Page, width: usize, height: usize) -> Vec<bool> {
    let mut out = vec![false; width * height];
    for y in 0..page.height.min(height) {
        for x in 0..page.width.min(width) {
            out[y * width + x] = page.gray[y * page.width + x] < INK_BELOW;
        }
    }
    out
}

fn dilate(mask: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            if !mask[y * width + x] {
                continue;
            }
            for ny in y.saturating_sub(INK_RADIUS)..=(y + INK_RADIUS).min(height - 1) {
                for nx in x.saturating_sub(INK_RADIUS)..=(x + INK_RADIUS).min(width - 1) {
                    out[ny * width + nx] = true;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(width: usize, height: usize, ink: &[(usize, usize)]) -> Page {
        let mut page = Page::blank(width, height);
        for (x, y) in ink {
            page.gray[y * width + x] = 0;
        }
        page
    }

    #[test]
    fn soft_hyphen_read_back_from_a_shared_glyph_is_the_same_dash() {
        let reference: String = normalized("수산물\u{AD}삼치").collect();
        let candidate: String = normalized("수산물 \u{2013} 삼치").collect();
        assert_eq!(reference, candidate);
    }

    #[test]
    fn identical_pages_have_no_difference() {
        let a = page(10, 10, &[(1, 1), (5, 5)]);
        let sums = compare_pixels(&a, &a);
        assert_eq!(sums.difference, 0);
        assert_eq!(sums.ink.f1(), 1.0);
    }

    #[test]
    fn blank_candidate_costs_the_reference_ink() {
        let a = page(10, 10, &[(1, 1), (5, 5)]);
        let sums = compare_pixels(&a, &Page::blank(10, 10));
        assert_eq!(sums.difference, sums.reference_ink);
        assert_eq!(sums.ink.f1(), 0.0);
    }

    #[test]
    fn size_mismatch_counts_as_full_difference() {
        let sums = compare_pixels(&Page::blank(10, 10), &Page::blank(10, 8));
        assert_eq!(sums.difference, 2 * 10 * 255);
    }

    #[test]
    fn one_pixel_short_page_is_the_same_paper_with_a_white_edge() {
        let reference = page(10, 10, &[(1, 1), (4, 9)]);
        let candidate = page(10, 9, &[(1, 1)]);
        assert_eq!(compare_pixels(&reference, &candidate).difference, 255);
        assert_eq!(compare_pixels(&candidate, &reference).difference, 255);
        let same = page(10, 9, &[(1, 1)]);
        assert_eq!(
            compare_pixels(&page(10, 10, &[(1, 1)]), &same).difference,
            0
        );
    }

    #[test]
    fn one_pixel_shift_is_inside_ink_radius() {
        let sums = compare_pixels(&page(10, 10, &[(4, 4)]), &page(10, 10, &[(5, 4)]));
        assert_eq!(sums.ink.f1(), 1.0);
        let far = compare_pixels(&page(10, 10, &[(1, 1)]), &page(10, 10, &[(8, 8)]));
        assert_eq!(far.ink.f1(), 0.0);
    }

    #[test]
    fn shift_hits_peak_at_the_offset_that_aligns_ink() {
        let hits = shift_hits(&page(30, 30, &[(12, 9)]), &page(30, 30, &[(15, 7)]));
        let peak = hits.iter().position(|count| *count == 1).unwrap();
        let (dy, dx) = (peak / SHIFT_SIDE, peak % SHIFT_SIDE);
        assert_eq!(
            (dx as i32 - SHIFT_RADIUS, dy as i32 - SHIFT_RADIUS),
            (-3, 2)
        );
        assert_eq!(hits.iter().sum::<u64>(), 1);
    }

    #[test]
    fn print_scale_finds_a_shrunk_reference() {
        let ink: Vec<(usize, usize)> = (0..40)
            .map(|step| (10 + step * 37 % 180, 10 + step * 53 % 260))
            .collect();
        let original = page(200, 280, &ink);
        let shrunk = scaled_page(&original, 0.75, &original);
        let (percent, best, unscaled) = print_scale(&shrunk, &original);
        assert!((74..=76).contains(&percent), "{percent}");
        assert!(best >= PRINT_SCALE_GAIN * unscaled);
        assert_eq!(print_scale(&original, &original), (100, 1.0, 1.0));
    }

    #[test]
    fn text_overlap_ignores_order_and_spacing() {
        let overlap = text_overlap("가 나\n다", "다나가");
        assert_eq!(overlap.f1(), 1.0);
        let half = text_overlap("가나", "가라");
        assert_eq!(half.matched, 1);
        assert_eq!(half.f1(), 0.5);
        assert_eq!(text_overlap("", "").f1(), 1.0);
    }

    #[test]
    fn text_gaps_count_missing_and_extra_by_class() {
        let gaps = text_gaps("가가1①ab", "가1x");
        assert_eq!(gaps.get("hangul"), Some(&[1, 0]));
        assert_eq!(gaps.get("digit"), Some(&[1, 0]));
        assert_eq!(gaps.get("latin"), Some(&[2, 1]));
        assert!(text_gaps("가 나", "나가").is_empty());
    }

    #[test]
    fn nfkc_folds_compatibility_forms() {
        assert_eq!(text_overlap("ﬁ１", "fi1").f1(), 1.0);
    }
}
