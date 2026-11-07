//! 쪽마다 문서 구성 요소(문단·표·칸·개체·머리말 등)가 어디에 놓였는지 묻는 길.
//!
//! 배치 단계가 요소를 그릴 때 함께 적어 둔 것이라 픽셀을 보고 추측하지 않는다. 좌표는 [`scene::Page`]와 같다.
//! 단위는 pt, 원점은 쪽 왼쪽 위, y는 아래로 늘며, 쪽 배율(`scale`)을 이미 곱했으므로 그린 쪽 위의 자리와 맞는다.

use crate::scene::Rect;

/// 구성 요소의 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 쪽 전체.
    Page,
    /// 쪽의 본문 영역(여백 안쪽).
    Body,
    Header,
    Footer,
    MasterPage,
    PageNumber,
    Footnote,
    Paragraph,
    Table,
    Cell,
    Picture,
    Shape,
    Equation,
    Caption,
    /// 글 흐름 안에 있지만 자리가 없는 컨트롤(감추기, 새 번호, 필드 시작 등). 자리는 문단 왼쪽 바깥에 둔 표식이다.
    Control,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Page => "쪽",
            Kind::Body => "본문",
            Kind::Header => "머리말",
            Kind::Footer => "꼬리말",
            Kind::MasterPage => "바탕쪽",
            Kind::PageNumber => "쪽 번호",
            Kind::Footnote => "각주",
            Kind::Paragraph => "문단",
            Kind::Table => "표",
            Kind::Cell => "칸",
            Kind::Picture => "그림",
            Kind::Shape => "도형",
            Kind::Equation => "수식",
            Kind::Caption => "캡션",
            Kind::Control => "컨트롤",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Kind::Page => "page",
            Kind::Body => "body",
            Kind::Header => "header",
            Kind::Footer => "footer",
            Kind::MasterPage => "master-page",
            Kind::PageNumber => "page-number",
            Kind::Footnote => "footnote",
            Kind::Paragraph => "paragraph",
            Kind::Table => "table",
            Kind::Cell => "cell",
            Kind::Picture => "picture",
            Kind::Shape => "shape",
            Kind::Equation => "equation",
            Kind::Caption => "caption",
            Kind::Control => "control",
        }
    }

    /// 쪽에 놓이는 개체(표·그림·도형·수식).
    pub fn is_object(self) -> bool {
        matches!(
            self,
            Kind::Table | Kind::Picture | Kind::Shape | Kind::Equation
        )
    }

    /// 배치용 틀이라 눈에 보이는 것이 아닌 종류.
    pub fn is_frame(self) -> bool {
        matches!(self, Kind::Page | Kind::Body | Kind::Control)
    }
}

#[derive(Clone, Debug)]
pub struct Component {
    /// 이 쪽 목록에서의 번호. 부모는 항상 자식보다 앞선다.
    pub id: usize,
    pub parent: Option<usize>,
    /// 쪽에서 몇 단계 아래인가(쪽 0).
    pub depth: usize,
    pub kind: Kind,
    /// 보이는 이름. 칸은 `2행 3열`, 도형은 `사각형`처럼 종류보다 구체적이다.
    pub name: String,
    /// 문단의 첫 글, 수식 식, 병합 칸 표시 같은 덧붙임. 없으면 빈 문자열.
    pub text: String,
    pub bounds: Rect,
    /// 이 요소 자신이나 그 안의 내용이 눈에 보이는 것을 그렸는가.
    pub ink: bool,
    /// 글과 따로 놓인 개체(글자처럼 취급하지 않은 최상위 개체).
    pub floating: bool,
    /// 글 뒤에 깔린다.
    pub behind: bool,
    /// 감춰졌거나 쪽 밖에 놓여 보이지 않는다.
    pub hidden: bool,
}

impl Component {
    /// 사용자가 쉽게 놓치는 요소: 배치용 틀이거나, 감춰졌거나, 아무것도 그리지 않았거나, 글 뒤에 깔렸다.
    pub fn unseen(&self) -> bool {
        self.kind.is_frame() || self.hidden || !self.ink || self.behind
    }

    pub fn contains(&self, x: f32, y: f32, slop: f32) -> bool {
        let b = self.bounds;
        x >= b.x - slop && x <= b.right() + slop && y >= b.y - slop && y <= b.bottom() + slop
    }

    pub fn intersects(&self, rect: Rect) -> bool {
        let b = self.bounds;
        b.x <= rect.right() && rect.x <= b.right() && b.y <= rect.bottom() && rect.y <= b.bottom()
    }
}

/// 한 쪽의 구성 요소 목록.
#[derive(Clone, Debug, Default)]
pub struct PageComponents {
    pub width: f32,
    pub height: f32,
    pub components: Vec<Component>,
}

impl PageComponents {
    pub fn get(&self, id: usize) -> Option<&Component> {
        self.components.get(id)
    }

    /// 한 점(쪽 위 pt)에 놓인 가장 안쪽 요소. `slop`은 선처럼 얇은 요소를 집기 쉽게 사방으로 넓히는 값이다.
    /// 겹치면 글 뒤가 아닌 것, 그다음 더 작은 것, 그다음 나중에 그린 것을 고른다.
    pub fn at(&self, x: f32, y: f32, slop: f32) -> Option<&Component> {
        self.components
            .iter()
            .filter(|component| component.contains(x, y, slop))
            .min_by(|a, b| {
                let area = |c: &Component| c.bounds.width * c.bounds.height;
                (a.behind, area(a))
                    .partial_cmp(&(b.behind, area(b)))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.id.cmp(&a.id))
            })
    }

    /// 사각형(쪽 위 pt)과 겹치는 모든 요소를 목록 차례(부모가 먼저)로 돌려준다.
    pub fn in_rect(&self, rect: Rect) -> Vec<&Component> {
        self.components
            .iter()
            .filter(|component| component.intersects(rect))
            .collect()
    }

    /// 쪽·본문과, 개체를 닻으로 붙들 뿐인 문단을 뺀 조상 이름을 `>`로 이은 길. 예: `표 > 2행 3열 > 문단`.
    pub fn path(&self, id: usize) -> String {
        let mut names = Vec::new();
        let mut child: Option<Kind> = None;
        let mut next = self.get(id);
        while let Some(component) = next {
            let plumbing = matches!(component.kind, Kind::Page | Kind::Body);
            let anchor = component.kind == Kind::Paragraph && child.is_some_and(Kind::is_object);
            if component.id == id || !(plumbing || anchor) {
                names.push(component.name.as_str());
                child = Some(component.kind);
            }
            next = component.parent.and_then(|parent| self.get(parent));
        }
        names.reverse();
        names.join(" > ")
    }
}
