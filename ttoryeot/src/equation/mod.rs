//! 한글 수식 스크립트 배치. Swift `HWPEquationScript.swift`와 `HWPEquation.swift`를 옮겼다.
//!
//! 수식 글꼴(파일에 적힌 이름 또는 HyhwpEQ)이 있으면 그 글꼴의 사용자 영역 글리프를 쓰고,
//! 없으면 Times 계열과 유니코드 글자로 그린다. 글꼴이 하나도 없으면 아무것도 그리지 않는다.

mod layout;
mod script;

use crate::fonts::FontBook;
use crate::scene::{Item, Rect};
use crate::subst::Rules;
use hwp_core::Equation;
use layout::{Faces, Layout};
use script::{Glyphs, Parser, tokenize};

/// 기준선 비율(0…1). 파일 값이 0 이하이거나 100을 넘으면 0.85다.
fn baseline_fraction(equation: &Equation) -> f32 {
    if equation.baseline > 0.0 && equation.baseline <= 100.0 {
        equation.baseline as f32 / 100.0
    } else {
        0.85
    }
}

/// 수식 하나를 개체 사각형(pt) 안에 그리는 명령.
pub(crate) fn items(fonts: &mut FontBook, equation: &Equation, rect: Rect) -> Vec<Item> {
    let rules = Rules::builtin();
    let Some(faces) = Faces::resolve(fonts, rules, &equation.font) else {
        return Vec::new();
    };
    let size = crate::layout::pt(equation.size).max(1.0);
    let glyphs = Glyphs::new(faces.pua);
    let tokens = tokenize(&equation.script);
    let nodes = Parser::new(tokens, glyphs).parse_all();
    let mut layout = Layout::new(fonts, faces);
    let frame = layout.seq(&nodes, size);
    if frame.width <= 0.01 {
        return Vec::new();
    }
    layout.ops(&frame, rect, baseline_fraction(equation), equation.color)
}
