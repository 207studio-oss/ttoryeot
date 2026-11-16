//! 데스크톱과 다른 플랫폼에서 공유하는 HWPX 문단 편집 규칙과 문단 선택.

use crate::components::{Component, Kind, PageComponents};
use hwp_core::ParagraphOrigin;
use hwp_core::hwpx::edit::{Check, EditError, Editor, TextMatch};

/// 비교할 문단 머리의 길이. 엔진이 알려 주는 문단 머리와 같은 길이다.
const HEAD_CHARS: usize = 40;

/// 고치기 전 글과 고친 글에서 실제로 바뀐 곳. 값은 바이트 위치다.
#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

/// 앞뒤로 같은 글을 빼고 바뀐 가운데만 남긴다. 같으면 `None`이다.
pub fn diff(old: &str, new: &str) -> Option<Change> {
    if old == new {
        return None;
    }
    let start = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or_else(|| old.len().min(new.len()), |((index, _), _)| index);
    let (old_rest, new_rest) = (&old[start..], &new[start..]);
    let suffix: usize = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix = suffix.min(old_rest.len()).min(new_rest.len());
    Some(Change {
        start,
        old_end: old.len() - suffix,
        new_end: new.len() - suffix,
    })
}

/// 엔진에 보낼 바꾸기 하나: `old[start..end]`를 `replacement`로.
#[derive(Debug, PartialEq, Eq)]
struct Attempt {
    start: usize,
    end: usize,
    replacement: String,
}

/// 바뀐 곳을 엔진의 바꾸기로 옮긴다. 글자를 넣기만 했으면 찾을 글이 비므로 앞이나 뒤 한 글자를 붙인다.
fn attempts(old: &str, new: &str, change: &Change) -> Vec<Attempt> {
    let inserted = &new[change.start..change.new_end];
    if change.old_end > change.start {
        return vec![Attempt {
            start: change.start,
            end: change.old_end,
            replacement: inserted.to_string(),
        }];
    }
    let mut out = Vec::new();
    if let Some(before) = old[..change.start].chars().next_back() {
        out.push(Attempt {
            start: change.start - before.len_utf8(),
            end: change.start,
            replacement: format!("{before}{inserted}"),
        });
    }
    if let Some(after) = old[change.start..].chars().next() {
        out.push(Attempt {
            start: change.start,
            end: change.start + after.len_utf8(),
            replacement: format!("{inserted}{after}"),
        });
    }
    out
}

/// 문단을 견줄 때 쓰는 머리. 공백과 제어 문자는 엔진과 배치가 다르게 셀 수 있어 뺀다.
fn head(text: &str) -> String {
    text.chars()
        .take(HEAD_CHARS)
        .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        .collect()
}

/// 이 문단의 이 자리에 해당하는 찾은 글인가.
fn same_place(found: &TextMatch, old: &str, attempt: &Attempt) -> bool {
    let (left, right) = (&old[..attempt.start], &old[attempt.end..]);
    let (ours, theirs) = (head(old), head(&found.paragraph));
    left.ends_with(&found.before)
        && right.starts_with(&found.after)
        && (ours.starts_with(&theirs) || theirs.starts_with(&ours))
}

/// 고친 글을 반영한다. 고치기 전 글이 비어 있으면 `origin`의 문단에 글을 넣는다.
/// `check`가 [`Check::Skip`]이면 고친 바이트를 다시 읽어 확인하지 않는다(미리 보기용, 저장하지 않는다).
pub fn apply_at(
    bytes: &[u8],
    origin: Option<ParagraphOrigin>,
    old: &str,
    new: &str,
    check: Check,
) -> Result<Vec<u8>, String> {
    if old == new {
        return Ok(bytes.to_vec());
    }
    let editor = Editor::open(bytes).map_err(|error| message(&error))?;
    if !old.is_empty() {
        return apply_with(&editor, old, new, check);
    }
    let origin = origin.ok_or("이 문단에는 아직 글을 넣을 수 없어요")?;
    editor
        .fill_empty_paragraph(origin.section, origin.ordinal, new, check)
        .map_err(|error| message(&error))
}

/// 고친 문단을 HWPX에 반영한 새 바이트를 돌려준다. `old`는 문단의 지금 글 전체, `new`는 고친 글이다.
/// 같은 글이 문서 다른 곳에 있어도 이 문단의 이 자리만 바꾼다.
#[cfg(test)]
pub fn apply(bytes: &[u8], old: &str, new: &str) -> Result<Vec<u8>, String> {
    if old == new {
        return Ok(bytes.to_vec());
    }
    let editor = Editor::open(bytes).map_err(|error| message(&error))?;
    apply_with(&editor, old, new, Check::Full)
}

/// 한 번 읽은 문서로 [`apply`]를 한다.
fn apply_with(editor: &Editor, old: &str, new: &str, check: Check) -> Result<Vec<u8>, String> {
    let Some(change) = diff(old, new) else {
        return Err("바뀐 것이 없어요".into());
    };
    let tries = attempts(old, new, &change);
    if tries.is_empty() {
        return Err("빈 문단은 아직 고칠 수 없어요".into());
    }
    let mut last = String::new();
    for attempt in &tries {
        let needle = &old[attempt.start..attempt.end];
        let found = match editor.find(needle) {
            Ok(found) => found,
            Err(error) => {
                last = message(&error);
                continue;
            }
        };
        let places: Vec<&TextMatch> = found
            .iter()
            .filter(|found| same_place(found, old, attempt))
            .collect();
        let occurrence = match places[..] {
            [only] => only.occurrence,
            [] => {
                last = "이 문단을 문서에서 찾지 못했어요. 문서를 다시 열어 보세요".into();
                continue;
            }
            _ => {
                last = "같은 글의 문단이 여러 군데 있어 이 자리만 고칠 수 없어요".into();
                continue;
            }
        };
        match editor.replace(needle, &attempt.replacement, Some(occurrence), check) {
            Ok(edited) => return Ok(edited),
            Err(EditError::SpansRuns) => last = message(&EditError::SpansRuns),
            Err(error) => return Err(message(&error)),
        }
    }
    Err(last)
}

fn message(error: &EditError) -> String {
    match error {
        EditError::SpansRuns => {
            "서식이 다른 글에 걸쳐 있어 한 번에 고칠 수 없어요. 서식이 같은 부분만 고쳐 주세요"
                .into()
        }
        EditError::InvalidReplacement => "탭은 넣을 수 없어요".into(),
        EditError::NotEmpty => "이 문단이 바뀌었어요. 문서를 다시 열어 보세요".into(),
        EditError::NotHwpx => "HWP 파일은 보기만 돼요. HWPX에서 글을 고칠 수 있어요".into(),
        EditError::NotFound => "이 문단을 문서에서 찾지 못했어요. 문서를 다시 열어 보세요".into(),
        other => other.to_string(),
    }
}

/// 한 점(쪽 위 pt)에 놓인 가장 안쪽 문단. 보이지 않는 문단은 고르지 않는다.
/// 점을 바로 품은 문단을 먼저 고르고, 없을 때만 `slop`만큼 넓혀 찾는다. 좁은 칸이 나란히 있으면
/// 넓힌 범위가 옆 칸 문단까지 닿기 때문이다.
pub fn paragraph_at(page: &PageComponents, x: f32, y: f32, slop: f32) -> Option<&Component> {
    let innermost = |slop: f32| {
        page.components
            .iter()
            .filter(|c| c.kind == Kind::Paragraph && !c.hidden && c.contains(x, y, slop))
            .min_by(|a, b| {
                let area = |c: &Component| c.bounds.width * c.bounds.height;
                area(a)
                    .partial_cmp(&area(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    };
    innermost(0.0).or_else(|| innermost(slop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwp_core::hwpx::edit;
    use std::path::PathBuf;

    fn fixture() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../hwp-core/tests/fixtures/synthetic.hwpx");
        std::fs::read(path).unwrap()
    }

    fn change(old: &str, new: &str) -> (String, String) {
        let change = diff(old, new).unwrap();
        (
            old[change.start..change.old_end].to_string(),
            new[change.start..change.new_end].to_string(),
        )
    }

    #[test]
    fn diff_keeps_only_the_changed_middle() {
        assert_eq!(diff("같다", "같다"), None);
        assert_eq!(
            change("09:30 시작", "10:00 시작"),
            ("09:3".into(), "10:0".into())
        );
        assert_eq!(
            change("새 학년을", "새로운 학년을"),
            ("".into(), "로운".into())
        );
        assert_eq!(change("가나다", "가나"), ("다".into(), "".into()));
        assert_eq!(change("aaa", "aaaa"), ("".into(), "a".into()));
        assert_eq!(change("가😀나", "가🙂나"), ("😀".into(), "🙂".into()));
    }

    #[test]
    fn insertions_borrow_a_neighbouring_letter_to_have_something_to_find() {
        let old = "새 학년을";
        let new = "새로운 학년을";
        let tries = attempts(old, new, &diff(old, new).unwrap());
        assert_eq!(&old[tries[0].start..tries[0].end], "새");
        assert_eq!(tries[0].replacement, "새로운");
        assert_eq!(&old[tries[1].start..tries[1].end], " ");
        assert_eq!(tries[1].replacement, "로운 ");
        assert!(attempts("", "가", &diff("", "가").unwrap()).is_empty());
    }

    #[test]
    fn changes_only_the_edited_paragraph_when_the_same_text_is_elsewhere() {
        let bytes = fixture();
        assert_eq!(edit::find_text(&bytes, "A").unwrap().len(), 3);
        let edited = apply(&bytes, "BETA", "BETX").unwrap();
        let left: Vec<_> = edit::find_text(&edited, "A")
            .unwrap()
            .into_iter()
            .map(|found| found.paragraph)
            .collect();
        assert_eq!(left.len(), 2);
        assert!(left.iter().all(|paragraph| paragraph.starts_with("ALP")));
        assert_eq!(edit::find_text(&edited, "BETX").unwrap().len(), 1);
    }

    #[test]
    fn picks_the_right_letter_inside_one_paragraph() {
        let bytes = fixture();
        let edited = apply(&bytes, "ALPHA", "ALPHO").unwrap();
        assert_eq!(edit::find_text(&edited, "ALPHO").unwrap().len(), 1);
        assert_eq!(edit::find_text(&edited, "BETA").unwrap().len(), 1);
    }

    #[test]
    fn unchanged_text_returns_the_same_bytes_and_errors_change_nothing() {
        let bytes = fixture();
        assert_eq!(apply(&bytes, "BETA", "BETA").unwrap(), bytes);
        assert!(
            apply(&bytes, "없는 문단", "고침")
                .unwrap_err()
                .contains("찾지 못했어요")
        );
        assert!(apply(&bytes, "BETA", "BE\tTA").unwrap_err().contains("탭"));
        assert!(apply(&bytes, "", "새 글").unwrap_err().contains("빈 문단"));
        assert!(
            apply(b"not a zip", "BETA", "BETX")
                .unwrap_err()
                .contains("HWP")
        );
    }
}
