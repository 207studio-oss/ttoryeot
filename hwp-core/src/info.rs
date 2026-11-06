//! 문서 형식과 파일 속성. 쪽 배치에는 쓰지 않는다.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentInfo {
    pub version: Option<String>,
    pub encrypted: bool,
    pub distribution: bool,
    pub metadata: Metadata,
}

pub(crate) fn non_empty(value: &str) -> Option<String> {
    let value = value.trim_matches(|ch: char| ch.is_whitespace() || ch == '\0');
    (!value.is_empty()).then(|| value.to_string())
}
