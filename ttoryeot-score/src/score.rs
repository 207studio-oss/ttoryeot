//! 문서 하나의 채점과 묶음 평균.

use crate::golden::Kind;
use crate::metric::{
    Overlap, PRINT_SCALE_GAIN, PixelSums, SHIFT_RADIUS, SHIFT_SIDE, compare_pixels, print_scale,
    shift_hits, text_gaps, text_overlap,
};
use crate::scan::Page;
use crate::structure::{self, Diff};
use serde::Serialize;
use std::collections::BTreeMap;

pub struct DocScore {
    pub kind: Kind,
    pub source: String,
    pub set: String,
    pub reference_pages: usize,
    pub candidate_pages: usize,
    pub pixels: PixelSums,
    pub page_differences: Vec<f64>,
    /// 기준 쪽마다 빈 쪽을 냈을 때의 화소당 평균 차이. 후보가 이보다 작아야 빈 쪽보다 낫다.
    pub blank_page_differences: Vec<f64>,
    pub text: Overlap,
    /// 글자 종류별 [빠짐, 남음] 개수(`metric::text_gaps`).
    pub text_gaps: BTreeMap<&'static str, [usize; 2]>,
    pub page_text_f1: Vec<f64>,
    pub reference_glyphs: usize,
    pub reference_unicode_glyphs: usize,
    /// 두 쪽이 다 있는 쪽에서 후보 잉크를 옮겨 본 겹침(`metric::shift_hits`)의 합.
    pub shift_hits: Vec<u64>,
    /// 첫 쪽으로 잰 `metric::print_scale`: (배율 %, 그 잉크 F1, 100%의 잉크 F1). 첫 쪽이 한쪽에 없으면 100%.
    pub print_scale: (u32, f64, f64),
    /// 줄·쪽 나눔·괘선·그림 자리 비교(`structure::compare`).
    pub structure: Diff,
}

/// 쪽은 차례대로 맞춘다. 한쪽에만 있는 쪽은 다른 쪽을 같은 크기의 빈 쪽으로 보고 잰다.
pub fn score(
    kind: Kind,
    source: &str,
    set: &str,
    reference: &[Page],
    candidate: &[Page],
) -> DocScore {
    let pages = reference.len().max(candidate.len());
    let mut pixels = PixelSums::default();
    let mut page_differences = Vec::with_capacity(pages);
    let mut page_text_f1 = Vec::with_capacity(pages);
    let mut shifts = vec![0u64; SHIFT_SIDE * SHIFT_SIDE];
    for index in 0..pages {
        let (expected, actual) = match (reference.get(index), candidate.get(index)) {
            (Some(expected), Some(actual)) => {
                for (sum, hits) in shifts.iter_mut().zip(shift_hits(expected, actual)) {
                    *sum += hits;
                }
                (expected, actual)
            }
            (Some(expected), None) => (expected, &Page::blank(expected.width, expected.height)),
            (None, Some(actual)) => (&Page::blank(actual.width, actual.height), actual),
            (None, None) => unreachable!("index is below the longer page list"),
        };
        let sums = compare_pixels(expected, actual);
        page_differences.push(sums.mean_difference());
        pixels = pixels.add(sums);
        page_text_f1.push(text_overlap(&expected.text, &actual.text).f1());
    }
    let joined = |pages: &[Page]| {
        pages
            .iter()
            .map(|page| page.text.as_str())
            .collect::<String>()
    };
    let (reference_text, candidate_text) = (joined(reference), joined(candidate));
    DocScore {
        kind,
        source: source.to_string(),
        set: set.to_string(),
        reference_pages: reference.len(),
        candidate_pages: candidate.len(),
        pixels,
        page_differences,
        blank_page_differences: reference
            .iter()
            .map(|page| {
                compare_pixels(page, &Page::blank(page.width, page.height)).mean_difference()
            })
            .collect(),
        text: text_overlap(&reference_text, &candidate_text),
        text_gaps: text_gaps(&reference_text, &candidate_text),
        page_text_f1,
        reference_glyphs: reference.iter().map(|page| page.glyphs).sum(),
        reference_unicode_glyphs: reference.iter().map(|page| page.unicode_glyphs).sum(),
        shift_hits: shifts,
        print_scale: match (reference.first(), candidate.first()) {
            (Some(expected), Some(actual)) => print_scale(expected, actual),
            _ => (100, 0.0, 0.0),
        },
        structure: structure::compare(
            &reference
                .iter()
                .map(|page| &page.structure)
                .collect::<Vec<_>>(),
            &candidate
                .iter()
                .map(|page| &page.structure)
                .collect::<Vec<_>>(),
        ),
    }
}

impl DocScore {
    /// 쪽마다 화소당 평균 차이를 낸 뒤 쪽 평균, 백분율.
    pub fn pixel_difference_percent(&self) -> f64 {
        100.0 * mean(&self.page_differences)
    }

    pub fn blank_pixel_difference_percent(&self) -> f64 {
        100.0 * mean(&self.blank_page_differences)
    }

    /// 1 − Σ차이/Σ(빈 쪽을 냈을 때의 차이). 1이 같음, 0이 빈 쪽만큼, 0보다 작으면 빈 쪽보다 못함.
    pub fn pixel_score(&self) -> f64 {
        if self.pixels.reference_ink == 0 {
            if self.pixels.difference == 0 {
                1.0
            } else {
                0.0
            }
        } else {
            1.0 - self.pixels.difference as f64 / self.pixels.reference_ink as f64
        }
    }

    pub fn page_text_f1(&self) -> f64 {
        mean(&self.page_text_f1)
    }

    /// 기준 PDF가 줄여 찍혀(모아 찍기·축소 인쇄) 이 문서의 화소 수치가 한/글 화면과 견준 값이 아닌가.
    pub fn reference_scaled(&self) -> bool {
        let (percent, best, unscaled) = self.print_scale;
        percent != 100 && best >= PRINT_SCALE_GAIN * unscaled
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

#[derive(Serialize)]
pub struct DocRow {
    pub index: usize,
    pub format: &'static str,
    pub source: String,
    pub set: String,
    pub reference_pages: usize,
    pub candidate_pages: usize,
    pub pixel_difference_percent: f64,
    pub blank_pixel_difference_percent: f64,
    pub pixel_score: f64,
    pub ink_f1: f64,
    pub text_f1: f64,
    pub text_recall: f64,
    pub text_precision: f64,
    /// 글자 종류별 [빠짐, 남음] 개수. 글자 자체는 남기지 않는다.
    pub text_gaps: BTreeMap<&'static str, [usize; 2]>,
    pub page_text_f1: f64,
    /// 후보 잉크 화소 수 / 기준 잉크 화소 수. 1보다 훨씬 크면 덧그림, 1 근처인데 잉크 F1이 낮으면 자리 어긋남.
    pub ink_ratio: f64,
    /// 후보를 통째로 옮겨 기준 잉크와 가장 많이 겹치는 (dx, dy) 화소.
    pub best_shift: (i32, i32),
    /// 옮기지 않았을 때·가장 잘 옮겼을 때 기준 잉크에 겹친 후보 잉크의 몫.
    pub zero_shift_share: f64,
    pub best_shift_share: f64,
    /// 기준 PDF를 찍은 배율로 보이는 값(%)과 그 배율로 줄인 후보의 첫 쪽 잉크 F1.
    pub print_scale_percent: u32,
    pub print_scale_ink_f1: f64,
    pub reference_scaled: bool,
    /// 구조 불일치 수. 0이면 줄바꿈·쪽 나눔·줄 자리·괘선·그림 자리가 다 같다.
    pub structure_mismatches: usize,
    pub structure: Diff,
}

impl DocRow {
    pub fn new(index: usize, score: &DocScore) -> Self {
        let center = SHIFT_SIDE * SHIFT_SIDE / 2;
        let best = (0..score.shift_hits.len())
            .max_by_key(|at| {
                (
                    score.shift_hits[*at],
                    std::cmp::Reverse(at.abs_diff(center)),
                )
            })
            .unwrap_or(center);
        Self {
            index,
            format: score.kind.label(),
            source: score.source.clone(),
            set: score.set.clone(),
            reference_pages: score.reference_pages,
            candidate_pages: score.candidate_pages,
            pixel_difference_percent: round(score.pixel_difference_percent()),
            blank_pixel_difference_percent: round(score.blank_pixel_difference_percent()),
            pixel_score: round(score.pixel_score()),
            ink_f1: round(score.pixels.ink.f1()),
            text_f1: round(score.text.f1()),
            text_recall: round(score.text.recall()),
            text_precision: round(score.text.precision()),
            text_gaps: score.text_gaps.clone(),
            page_text_f1: round(score.page_text_f1()),
            ink_ratio: round(ratio(
                score.pixels.ink.candidate,
                score.pixels.ink.reference,
            )),
            best_shift: (
                (best % SHIFT_SIDE) as i32 - SHIFT_RADIUS,
                (best / SHIFT_SIDE) as i32 - SHIFT_RADIUS,
            ),
            zero_shift_share: round(ratio(score.shift_hits[center], score.pixels.ink.candidate)),
            best_shift_share: round(ratio(score.shift_hits[best], score.pixels.ink.candidate)),
            print_scale_percent: score.print_scale.0,
            print_scale_ink_f1: round(score.print_scale.1),
            reference_scaled: score.reference_scaled(),
            structure_mismatches: score.structure.total(),
            structure: score.structure.clone(),
        }
    }
}

/// 문서 평균. 모든 수치는 문서마다 낸 값의 산술 평균이다(쪽이 많은 문서가 더 무겁지 않다).
#[derive(Serialize, Default)]
pub struct Group {
    pub documents: usize,
    pub reference_pages: usize,
    pub page_count_match: f64,
    pub pixel_difference_percent: f64,
    /// 모든 문서에 빈 쪽을 냈을 때의 화소 차이(%)와, 그보다 화소 차이가 작은 문서의 몫.
    pub blank_pixel_difference_percent: f64,
    pub beats_blank: f64,
    pub pixel_score: f64,
    pub ink_f1: f64,
    pub text_f1: f64,
    pub text_recall: f64,
    pub text_precision: f64,
    pub page_text_f1: f64,
    /// 기준 PDF 글리프 중 유니코드로 풀리는 비율. 낮으면 텍스트 F1이 추출 실패를 재는 것이다.
    pub reference_unicode_coverage: f64,
}

pub fn group(scores: &[&DocScore]) -> Group {
    let n = scores.len();
    if n == 0 {
        return Group::default();
    }
    let average = |value: &dyn Fn(&DocScore) -> f64| {
        round(scores.iter().map(|score| value(score)).sum::<f64>() / n as f64)
    };
    let glyphs: usize = scores.iter().map(|score| score.reference_glyphs).sum();
    let unicode: usize = scores
        .iter()
        .map(|score| score.reference_unicode_glyphs)
        .sum();
    Group {
        documents: n,
        reference_pages: scores.iter().map(|score| score.reference_pages).sum(),
        page_count_match: average(&|score| {
            f64::from(u8::from(score.reference_pages == score.candidate_pages))
        }),
        pixel_difference_percent: average(&|score| score.pixel_difference_percent()),
        blank_pixel_difference_percent: average(&|score| score.blank_pixel_difference_percent()),
        beats_blank: average(&|score| {
            f64::from(u8::from(
                score.pixel_difference_percent() < score.blank_pixel_difference_percent(),
            ))
        }),
        pixel_score: average(&|score| score.pixel_score()),
        ink_f1: average(&|score| score.pixels.ink.f1()),
        text_f1: average(&|score| score.text.f1()),
        text_recall: average(&|score| score.text.recall()),
        text_precision: average(&|score| score.text.precision()),
        page_text_f1: average(&|score| score.page_text_f1()),
        reference_unicode_coverage: round(if glyphs == 0 {
            1.0
        } else {
            unicode as f64 / glyphs as f64
        }),
    }
}

/// 전체·형식별·기준 출처별·목록(`set:`)별 묶음. `100%:`로 시작하는 묶음은 기준 PDF가 줄여 찍힌 문서를 뺀 것이다.
/// 줄여 찍힘은 후보와 견줘 찾으니 빈 쪽·자기 자신 후보에서는 모두 100%로 나온다.
pub fn groups(scores: &[DocScore]) -> BTreeMap<String, Group> {
    let mut keys: BTreeMap<String, Vec<&DocScore>> = BTreeMap::new();
    for score in scores {
        keys.entry("all".to_string()).or_default().push(score);
        keys.entry(format!("format:{}", score.kind.label()))
            .or_default()
            .push(score);
        keys.entry(format!("source:{}", score.source))
            .or_default()
            .push(score);
        keys.entry(format!("set:{}", score.set))
            .or_default()
            .push(score);
        keys.entry(format!("set:{}:{}", score.set, score.kind.label()))
            .or_default()
            .push(score);
        if !score.reference_scaled() {
            keys.entry("100%:all".to_string()).or_default().push(score);
            keys.entry(format!("100%:format:{}", score.kind.label()))
                .or_default()
                .push(score);
        }
    }
    keys.into_iter()
        .map(|(key, members)| (key, group(&members)))
        .collect()
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn ratio(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}
