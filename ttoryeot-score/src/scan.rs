//! PDF 한 벌을 쪽마다 회색조 래스터와 글자열로 바꾼다. 기준 PDF와 후보 PDF가 같은 길을 탄다.

use std::collections::HashSet;

use crate::structure::{Box2, Glyph, Outline, Raw, Structure};
use hayro::hayro_interpret::Paint;
use hayro::kurbo::{PathSeg, Shape};

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{BezPath, Rect};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

/// 채점 해상도. 1pt가 1화소다.
pub const DPI: f32 = 72.0;
/// hayro는 글리프 변환에 글꼴 단위(1/1000 em)를 곱해 둔다. 글자 크기(pt)는 되돌려 잰다.
const HAYRO_UNITS_PER_EM: f64 = 1000.0;

pub struct Page {
    pub width: usize,
    pub height: usize,
    /// 0(검정)…255(흰색) 회색조, 줄 우선.
    pub gray: Vec<u8>,
    pub text: String,
    pub glyphs: usize,
    pub unicode_glyphs: usize,
    pub structure: Structure,
}

impl Page {
    pub fn blank(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            gray: vec![u8::MAX; width * height],
            text: String::new(),
            glyphs: 0,
            unicode_glyphs: 0,
            structure: Structure::default(),
        }
    }
}

pub fn scan(bytes: Vec<u8>) -> Result<Vec<Page>, String> {
    let pdf = Pdf::new(bytes).map_err(|error| format!("pdf: {error:?}"))?;
    let interpreter = InterpreterSettings::default();
    let cache = RenderCache::new();
    let scale = DPI / 72.0;
    let pixmap_settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
    };
    let mut pages = Vec::new();
    for page in pdf.pages().iter() {
        let pixmap = hayro::render(
            page,
            &cache,
            &interpreter,
            &RenderSettings::default(),
            &pixmap_settings,
        );
        let gray = pixmap
            .data_as_u8_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|[red, green, blue, _]| luminance(*red, *green, *blue))
            .collect();
        let mut text = TextDevice::default();
        let interpreter_cache = InterpreterCache::new();
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, f64::from(u16::MAX), f64::from(u16::MAX)),
            &interpreter_cache,
            page.xref(),
            interpreter.clone(),
        );
        interpret_page(page, &mut context, &mut text);
        pages.push(Page {
            width: usize::from(pixmap.width()),
            height: usize::from(pixmap.height()),
            gray,
            text: text.text,
            glyphs: text.glyphs,
            unicode_glyphs: text.unicode_glyphs,
            structure: text.raw.structure(),
        });
    }
    Ok(pages)
}

fn luminance(red: u8, green: u8, blue: u8) -> u8 {
    let value = 0.299 * f32::from(red) + 0.587 * f32::from(green) + 0.114 * f32::from(blue);
    value.round().clamp(0.0, 255.0) as u8
}

/// 그리기는 버리고 글자의 유니코드만 모은다.
/// 같은 쪽의 같은 자리에 같은 글자가 다시 오면 한 번만 센다. 칠하고 긋는 글자(`2 Tr`)는 hayro가
/// 칠하기와 긋기로 두 번 넘기는데, 화면에는 글자 하나다.
#[derive(Default)]
struct TextDevice {
    text: String,
    glyphs: usize,
    unicode_glyphs: usize,
    placed: HashSet<(String, [i64; 2])>,
    raw: Raw,
    /// 쌓인 오려 내기의 테두리(장치 좌표, 바깥부터 겹친 교집합).
    clips: Vec<Rect>,
}

/// 잉크로 보이는 칠인가(`metric::INK_BELOW`와 같은 밝기 기준). 무늬 칠은 잉크로 본다.
fn inked(paint: &Paint<'_>) -> bool {
    match paint {
        Paint::Color(color) => {
            let [red, green, blue, alpha] = color.to_rgba().to_rgba8();
            alpha > 0 && luminance(red, green, blue) < crate::metric::INK_BELOW
        }
        Paint::Pattern(_) => true,
    }
}

fn bounds(rect: Rect) -> Box2 {
    Box2 {
        x0: rect.x0,
        y0: rect.y0,
        x1: rect.x1,
        y1: rect.y1,
    }
}

/// 글꼴 단위(1000–2048 em)로 그린 글자 윤곽인가. 한/글은 어떤 글자를 글꼴로 넣지 않고 윤곽 경로로
/// 그린다(`0.0108 0 0 0.0108 x y cm` 뒤 닫힌 경로들을 `B`로 칠하고 긋는다). 그 획을 괘선으로 세면
/// 안 되고, 그 글은 PDF에 글자로 남지 않는다. 쪽 좌표(pt)로 그린 선·칠은 배율이 1 안팎이다.
fn outline_glyph(path: &BezPath, transform: &hayro::kurbo::Affine) -> bool {
    /// 이보다 작은 배율로 그린 경로를 글꼴 단위로 본다. 10pt 글자는 1000 em에서 0.01, 2048 em에서 0.005다.
    const GLYPH_SCALE: f64 = 0.1;
    /// 글자 하나의 장치 크기 상한(pt). 큰 글상자·도형을 글자로 보지 않는다.
    const GLYPH_EXTENT: f64 = 100.0;
    let scale = transform.determinant().abs().sqrt();
    if scale >= GLYPH_SCALE {
        return false;
    }
    let size = (*transform * path.clone()).bounding_box();
    if size.width() > GLYPH_EXTENT || size.height() > GLYPH_EXTENT {
        return false;
    }
    // 글자 윤곽은 닫힌 경로다.
    matches!(
        path.elements().last(),
        Some(hayro::kurbo::PathEl::ClosePath)
    )
}

/// 같은 자리로 보는 거리(pt). 한 번 그린 글자를 다시 그릴 때의 계산 끝수만 흡수한다.
const SAME_PLACE: f64 = 0.01;

/// 글자로 보는 스텐실 마스크의 높이(pt). 한 줄 글자 높이까지다. 폭은 묶인 글자 수만큼 길 수 있다.
const GLYPH_MASK_EXTENT: f64 = 36.0;

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        // 윤곽 글자는 색과 상관없이 글이다(어두운 머리행의 흰 글씨). 밝은 색 거르기는 괘선에만 쓴다.
        let outline = outline_glyph(path, &props.transform);
        if !outline && !inked(&props.paint) {
            return;
        }
        let path = props.transform * path.clone();
        if outline {
            if matches!(mode, DrawMode::Fill(_) | DrawMode::FillAndStroke(..)) {
                self.raw.outline_glyphs.push(Outline {
                    ink: bounds(path.bounding_box()),
                    origin: props.transform.translation().x,
                });
            }
            return;
        }
        match mode {
            // 선은 곧은 토막마다 본다(사각 테두리를 한 번에 그어도 네 괘선이다).
            DrawMode::Stroke(_) => {
                for segment in path.segments() {
                    if let PathSeg::Line(line) = segment {
                        self.raw.add_rule(bounds(line.bounding_box()));
                    }
                }
            }
            DrawMode::Fill(_) | DrawMode::FillAndStroke(..) => {
                self.raw.add_rule(bounds(path.bounding_box()))
            }
            DrawMode::Invisible => {}
        }
    }
    fn push_clip_path(&mut self, clip: &ClipPath) {
        let bounds = clip.path.bounding_box();
        let inner = self
            .clips
            .last()
            .map_or(bounds, |outer| outer.intersect(bounds));
        self.clips.push(inner);
    }
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            let unicode = glyph.as_unicode().map(|unicode| match unicode {
                BfString::Char(character) => character.to_string(),
                BfString::String(text) => text,
            });
            if let Some(text) = &unicode {
                let origin = (props.transform * glyph.transform()).translation();
                let place = [origin.x, origin.y].map(|value| (value / SAME_PLACE).round() as i64);
                if !self.placed.insert((text.clone(), place)) {
                    continue;
                }
                let full = props.transform * glyph.transform();
                let [_, _, c, d, _, _] = full.as_coeffs();
                self.raw.glyphs.push(Glyph {
                    text: text.clone(),
                    x: origin.x,
                    y: origin.y,
                    size: c.hypot(d) * HAYRO_UNITS_PER_EM,
                });
            }
            self.glyphs += 1;
            if let Some(text) = unicode {
                self.unicode_glyphs += 1;
                self.text.push_str(&text);
            }
        }
    }
    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        // hayro는 그림 변환을 화소 단위로 넘긴다(1/폭, 1/높이를 미리 곱함). 그림 전체는 화소 수만큼이다.
        let unit = Rect::new(
            0.0,
            0.0,
            f64::from(image.width()),
            f64::from(image.height()),
        )
        .to_path(0.1);
        // 오려 낸 그림은 보이는 데까지만 잰다. 한/글은 잘린 그림을 잘린 크기로 넣고, 또렷은 원본을 오려 낸다.
        let place = (props.transform * unit).bounding_box();
        let place = self
            .clips
            .last()
            .map_or(place, |clip| clip.intersect(place));
        if place.area() <= 0.0 {
            return;
        }
        // 한/글은 기호 글꼴의 글머리표(ㅇ 등)를 글자 크기 스텐실 마스크로 찍는다. 또렷은 같은 기호를 글자로
        // 그리므로 그림으로 견주지 않는다(화소 비교에는 남는다).
        if matches!(image, Image::Stencil(_)) && place.height() <= GLYPH_MASK_EXTENT {
            return;
        }
        self.raw.images.push(bounds(place));
    }
    fn pop_clip(&mut self) {
        self.clips.pop();
    }
    fn pop_transparency_group(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 표준 글꼴(Helvetica) 한 쪽짜리 PDF. `content`는 쪽 내용 흐름이다.
    fn pdf(content: &str) -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] \
             /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
                .to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn fill_and_stroke_text_counts_once() {
        let pages = scan(pdf("BT /F1 12 Tf 10 50 Td 2 Tr (AB) Tj ET")).expect("scan");
        assert_eq!(pages[0].text, "AB");
        assert_eq!(pages[0].glyphs, 2);
    }

    #[test]
    fn same_letter_elsewhere_still_counts() {
        let pages = scan(pdf(
            "BT /F1 12 Tf 10 50 Td (A) Tj ET BT /F1 12 Tf 10 30 Td (A) Tj ET",
        ))
        .expect("scan");
        assert_eq!(pages[0].text, "AA");
    }

    /// 한/글이 글자를 윤곽으로 그리는 꼴: 글꼴 단위 배율 아래 닫힌 경로를 칠하고 긋는다.
    #[test]
    fn outline_glyph_strokes_are_not_rules() {
        let pages = scan(pdf("0 0 0 RG 0 0 0 rg q 0.01 0 0 0.01 10 50 cm \
             100 0 m 900 0 l 900 60 l 100 60 l h 100 500 m 900 500 l 900 560 l 100 560 l h B Q"))
        .expect("scan");
        let structure = &pages[0].structure;
        assert!(
            structure.horizontal.is_empty(),
            "{:?}",
            structure.horizontal
        );
        assert_eq!(structure.outline_glyphs.len(), 1);
    }

    /// 어두운 칸 위 흰 글씨도 윤곽으로 그리면 글자다. 그 자리의 다른 쪽 글자를 가려야 한다.
    #[test]
    fn white_outline_glyph_is_still_a_glyph() {
        let pages = scan(pdf("1 1 1 RG 1 1 1 rg q 0.01 0 0 0.01 10 50 cm \
             100 0 m 900 0 l 900 600 l 100 600 l h B Q"))
        .expect("scan");
        assert_eq!(pages[0].structure.outline_glyphs.len(), 1);
    }

    #[test]
    fn image_box_covers_the_whole_image() {
        let pages = scan(pdf(
            "q 20 0 0 10 30 40 cm BI /W 2 /H 2 /BPC 8 /CS /G ID \x00\x00\x00\x00 EI Q",
        ))
        .expect("scan");
        let image = &pages[0].structure.images[0];
        assert!((image.x1 - image.x0 - 20.0).abs() < 0.01, "{image:?}");
        assert!((image.y1 - image.y0 - 10.0).abs() < 0.01, "{image:?}");
    }

    #[test]
    fn glyph_size_stencil_mask_is_not_an_image() {
        let pages = scan(pdf(
            "q 0 0 0 rg 17 0 0 19 30 40 cm BI /W 2 /H 2 /BPC 1 /IM true ID \x00\x00 EI Q",
        ))
        .expect("scan");
        assert!(pages[0].structure.images.is_empty());
    }

    #[test]
    fn structure_reads_lines_rules_and_size() {
        let pages = scan(pdf(
            "BT /F1 12 Tf 10 50 Td (AB) Tj ET BT /F1 12 Tf 40 50.5 Td (C) Tj ET 0 0 0 RG 1 w 5 20 m 95 20 l S",
        ))
        .expect("scan");
        let structure = &pages[0].structure;
        assert_eq!(structure.lines.len(), 1);
        assert_eq!(
            structure.lines[0].text, "ABC",
            "12pt 글자의 0.5pt 어긋남은 같은 줄"
        );
        assert!(
            (structure.lines[0].y - 50.0).abs() < 0.01,
            "{}",
            structure.lines[0].y
        );
        assert_eq!(structure.horizontal.len(), 1);
        assert!((structure.horizontal[0].at - 80.0).abs() < 0.01);
    }
}
