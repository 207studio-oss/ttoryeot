use crate::model::{Markpen, RunItem, TrackMark};

use super::dom::Node;
use super::header::color;

#[derive(Default)]
pub(super) struct Markpens {
    position: u32,
    open: Vec<Option<(u32, u32)>>,
    ranges: Vec<Markpen>,
    malformed: bool,
    /// 열린 변경 추적 표시: (Id, 시작 자리, 넣은 글인가).
    open_tracks: Vec<(String, u32, bool)>,
    tracks: Vec<TrackMark>,
}

impl Markpens {
    pub fn marker(&mut self, node: &Node) -> bool {
        match node.name.as_str() {
            "markpenBegin" => {
                let color = color(node.attr("color"));
                self.malformed |= color.is_none();
                self.open.push(color.map(|color| (self.position, color)));
            }
            "markpenEnd" => match self.open.pop() {
                Some(Some((start, color))) => self.ranges.push(Markpen {
                    start,
                    end: self.position,
                    color,
                }),
                Some(None) => {}
                None => self.malformed = true,
            },
            "deleteBegin" | "insertBegin" => {
                let id = node.attr("Id").unwrap_or_default().to_string();
                let insert = node.name == "insertBegin";
                self.open_tracks.push((id, self.position, insert));
            }
            "deleteEnd" | "insertEnd" => {
                let id = node.attr("Id").unwrap_or_default();
                let insert = node.name == "insertEnd";
                // 앞 문단에서 시작한 표시는 이 문단 처음부터 이어진다.
                let start = match self
                    .open_tracks
                    .iter()
                    .rposition(|(open, _, kind)| open == id && *kind == insert)
                {
                    Some(index) => self.open_tracks.remove(index).1,
                    None => 0,
                };
                self.push_track(start, self.position, insert);
            }
            _ => return false,
        }
        true
    }

    pub fn advance(&mut self, items: &[RunItem]) {
        for item in items {
            let width = match item {
                RunItem::Text(text) => text.encode_utf16().count(),
                RunItem::LineBreak => 1,
                RunItem::Skip(width) => usize::try_from(*width).unwrap_or(0),
                RunItem::Tab
                | RunItem::SizedTab(_)
                | RunItem::FormattedTab(_)
                | RunItem::Table(_)
                | RunItem::Picture(_)
                | RunItem::Shape(_)
                | RunItem::AutoNum(_)
                | RunItem::Control(_)
                | RunItem::Compose(_)
                | RunItem::Note(_)
                | RunItem::Header { .. }
                | RunItem::Footer { .. }
                | RunItem::PageHiding { .. }
                | RunItem::NewNum(_)
                | RunItem::PageNum { .. } => 8,
            };
            self.position = self
                .position
                .saturating_add(u32::try_from(width).unwrap_or(u32::MAX));
        }
    }

    fn push_track(&mut self, start: u32, end: u32, insert: bool) {
        if end > start {
            self.tracks.push(TrackMark { start, end, insert });
        }
    }

    /// 형광펜 범위, 변경 추적 범위, 형광펜 표시가 어긋났는가. 문단 끝까지 닫히지 않은 변경 추적은 문단 끝까지 이어진다.
    pub fn finish(mut self) -> (Vec<Markpen>, Vec<TrackMark>, bool) {
        for (_, start, insert) in std::mem::take(&mut self.open_tracks) {
            self.push_track(start, self.position, insert);
        }
        (
            self.ranges,
            self.tracks,
            self.malformed || !self.open.is_empty(),
        )
    }
}
