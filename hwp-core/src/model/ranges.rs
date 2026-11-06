use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RangeTag {
    pub start: u32,
    pub end: u32,
    pub tag: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Markpen {
    pub start: u32,
    pub end: u32,
    pub color: u32,
}

/// 변경 추적으로 지운 글(`insert`가 거짓) 또는 넣은 글의 글자 자리 범위.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrackMark {
    pub start: u32,
    pub end: u32,
    pub insert: bool,
}
