//! 쪽마다 그릴 것의 목록. 좌표는 pt, 원점은 쪽 왼쪽 위, y는 아래로 는다.

use crate::fonts::FaceId;
use std::ops::Range;
use std::sync::Arc;

pub struct Page {
    pub width: f32,
    pub height: f32,
    /// 쪽 왼쪽 위를 축으로 그릴 것 전체에 곱하는 배율. 보통 1이다.
    pub scale: f32,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PathOp {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Clone, Debug)]
pub struct Stroke {
    pub width: f32,
    pub color: u32,
    pub dash: Vec<f32>,
}

/// 같은 얼굴·크기·꾸밈으로 이어 그리는 글자들. x는 줄의 기준점에서 잰 각 글자의 왼쪽(pt, 장평 반영 뒤).
pub struct GlyphRun {
    pub face: FaceId,
    pub size: f32,
    pub origin: (f32, f32),
    /// 장평(가로 배율).
    pub scale_x: f32,
    pub italic: bool,
    /// 합성 굵게의 획 굵기(pt). 없으면 채우기만.
    pub bold_stroke: Option<f32>,
    pub color: u32,
    pub glyphs: Vec<(u32, f32)>,
    pub text: String,
    pub ranges: Vec<Range<usize>>,
}

pub enum ImageData {
    Png(Arc<Vec<u8>>),
    Jpeg(Arc<Vec<u8>>),
    Gif(Arc<Vec<u8>>),
    Webp(Arc<Vec<u8>>),
    Bmp(Arc<Vec<u8>>),
}

pub enum Item {
    Glyphs(GlyphRun),
    Fill {
        path: Vec<PathOp>,
        color: u32,
        alpha: f32,
    },
    Stroke {
        path: Vec<PathOp>,
        stroke: Stroke,
    },
    /// 사각형 `rect`를 `from`에서 `to`로 고르게 바뀌는 색으로 칠한다. `stops`는 (0–1 자리, 색)이다.
    LinearFill {
        rect: Rect,
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, u32)>,
    },
    Image {
        data: Arc<ImageData>,
        rect: Rect,
    },
    /// 이 사각형 밖은 그리지 않는다. `Pop`까지.
    Clip(Rect),
    /// 아핀 변환 [a b c d e f]: x' = a·x + c·y + e, y' = b·x + d·y + f. `Pop`까지.
    Transform([f32; 6]),
    Pop,
}

pub fn rect_path(rect: Rect) -> Vec<PathOp> {
    vec![
        PathOp::Move(rect.x, rect.y),
        PathOp::Line(rect.right(), rect.y),
        PathOp::Line(rect.right(), rect.bottom()),
        PathOp::Line(rect.x, rect.bottom()),
        PathOp::Close,
    ]
}
