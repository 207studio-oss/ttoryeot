//! 그릴 것 목록을 krilla로 PDF에 쓴다.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::fonts::FontBook;
use crate::scene::{GlyphRun, ImageData, Item, Page, PathOp, Rect, Stroke};
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Size, Transform};
use krilla::image::{BitsPerComponent, CustomImage, Image, ImageColorspace};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{
    Fill, FillRule, LineCap, LinearGradient, SpreadMethod, Stop, Stroke as PdfStroke, StrokeDash,
};
use krilla::surface::Surface;
use krilla::text::{GlyphId, KrillaGlyph};

pub fn write(pages: Vec<Page>, fonts: &mut FontBook) -> Result<Vec<u8>, String> {
    let mut document = Document::new();
    let mut images = HashMap::new();
    let last_use: HashMap<*const ImageData, usize> = pages
        .iter()
        .enumerate()
        .flat_map(|(index, page)| {
            page.items.iter().filter_map(move |item| match item {
                Item::Image { data, .. } => Some((Arc::as_ptr(data), index)),
                _ => None,
            })
        })
        .collect();
    for (index, page) in pages.into_iter().enumerate() {
        let settings =
            PageSettings::from_wh(page.width.max(1.0), page.height.max(1.0)).ok_or("page size")?;
        let mut pdf_page = document.start_page_with(settings);
        let mut surface = pdf_page.surface();
        let scaled = page.scale.is_finite() && page.scale > 0.0 && page.scale != 1.0;
        if scaled {
            surface.push_transform(&Transform::from_scale(page.scale, page.scale));
        }
        for item in &page.items {
            draw(&mut surface, item, fonts, &mut images)?;
        }
        if scaled {
            surface.pop();
        }
        surface.finish();
        pdf_page.finish();
        images.retain(|data, _| last_use[data] > index);
    }
    document.finish().map_err(|error| format!("pdf: {error:?}"))
}

fn color(value: u32) -> rgb::Color {
    rgb::Color::new(
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    )
}

fn fill(value: u32, alpha: f32) -> Fill {
    Fill {
        paint: color(value).into(),
        opacity: NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
        rule: FillRule::NonZero,
    }
}

fn stroke(stroke: &Stroke) -> PdfStroke {
    PdfStroke {
        paint: color(stroke.color).into(),
        width: stroke.width.max(0.01),
        line_cap: LineCap::Butt,
        dash: (!stroke.dash.is_empty()).then(|| StrokeDash {
            array: stroke.dash.clone(),
            offset: 0.0,
        }),
        ..PdfStroke::default()
    }
}

fn path(ops: &[PathOp]) -> Option<Path> {
    let mut builder = PathBuilder::new();
    for op in ops {
        match *op {
            PathOp::Move(x, y) => builder.move_to(x, y),
            PathOp::Line(x, y) => builder.line_to(x, y),
            PathOp::Cubic(x1, y1, x2, y2, x, y) => builder.cubic_to(x1, y1, x2, y2, x, y),
            PathOp::Close => builder.close(),
        }
    }
    builder.finish()
}

fn rect(rect: Rect) -> Option<Path> {
    let mut builder = PathBuilder::new();
    builder.push_rect(krilla::geom::Rect::from_xywh(
        rect.x,
        rect.y,
        rect.width.max(0.0),
        rect.height.max(0.0),
    )?);
    builder.finish()
}

type Images = HashMap<*const ImageData, Option<Image>>;

fn draw(
    surface: &mut Surface,
    item: &Item,
    fonts: &mut FontBook,
    images: &mut Images,
) -> Result<(), String> {
    match item {
        Item::Glyphs(run) => glyphs(surface, run, fonts)?,
        Item::Fill {
            path: ops,
            color,
            alpha,
        } => {
            if let Some(path) = path(ops) {
                surface.set_stroke(None);
                surface.set_fill(Some(fill(*color, *alpha)));
                surface.draw_path(&path);
            }
        }
        Item::Stroke {
            path: ops,
            stroke: s,
        } => {
            if let Some(path) = path(ops) {
                surface.set_fill(None);
                surface.set_stroke(Some(stroke(s)));
                surface.draw_path(&path);
                surface.set_stroke(None);
            }
        }
        Item::Image { data, rect: area } => {
            let Some(image) = images
                .entry(Arc::as_ptr(data))
                .or_insert_with(|| image(data))
                .clone()
            else {
                return Ok(());
            };
            let Some(size) = Size::from_wh(area.width.max(0.01), area.height.max(0.01)) else {
                return Ok(());
            };
            surface.push_transform(&Transform::from_translate(area.x, area.y));
            surface.draw_image(image, size);
            surface.pop();
        }
        Item::LinearFill {
            rect: area,
            from,
            to,
            stops,
        } => {
            if let Some(path) = rect(*area) {
                let gradient = LinearGradient {
                    x1: from.0,
                    y1: from.1,
                    x2: to.0,
                    y2: to.1,
                    transform: Transform::identity(),
                    spread_method: SpreadMethod::Pad,
                    stops: stops
                        .iter()
                        .map(|(offset, value)| Stop {
                            offset: NormalizedF32::new(offset.clamp(0.0, 1.0))
                                .unwrap_or(NormalizedF32::ZERO),
                            color: color(*value).into(),
                            opacity: NormalizedF32::ONE,
                        })
                        .collect(),
                    anti_alias: false,
                };
                surface.set_stroke(None);
                surface.set_fill(Some(Fill {
                    paint: gradient.into(),
                    opacity: NormalizedF32::ONE,
                    rule: FillRule::NonZero,
                }));
                surface.draw_path(&path);
            }
        }
        Item::Clip(area) => match rect(*area) {
            Some(path) => surface.push_clip_path(&path, &FillRule::NonZero),
            None => surface.push_transform(&Transform::identity()),
        },
        Item::Transform([a, b, c, d, e, f]) => {
            surface.push_transform(&Transform::from_row(*a, *b, *c, *d, *e, *f));
        }
        Item::Pop => surface.pop(),
    }
    Ok(())
}

fn image(data: &ImageData) -> Option<Image> {
    match data {
        ImageData::Png(bytes) => Image::from_png(bytes.clone().into(), true).ok(),
        ImageData::Jpeg(bytes) => Image::from_jpeg(bytes.clone().into(), true).ok(),
        ImageData::Gif(bytes) => Image::from_gif(bytes.clone().into(), true).ok(),
        ImageData::Webp(bytes) => Image::from_webp(bytes.clone().into(), true).ok(),
        ImageData::Bmp(bytes) => Image::from_custom(Bitmap::decode(bytes)?, false).ok(),
    }
}

/// 푼 BMP 화소. krilla가 이 값을 해시하려고 복제하므로 화소는 Arc로 나눈다.
#[derive(Clone)]
struct Bitmap(Arc<BitmapPixels>);

struct BitmapPixels {
    source: Arc<Vec<u8>>,
    size: (u32, u32),
    color: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

impl Bitmap {
    fn decode(source: &Arc<Vec<u8>>) -> Option<Self> {
        let decoded = image::load_from_memory_with_format(source, image::ImageFormat::Bmp).ok()?;
        let size = (decoded.width(), decoded.height());
        let (color, alpha) = match decoded {
            image::DynamicImage::ImageRgb8(rgb) => (rgb.into_raw(), None),
            other => {
                let rgba = other.into_rgba8();
                let color = rgba.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
                let alpha: Vec<u8> = rgba.pixels().map(|p| p[3]).collect();
                (color, alpha.iter().any(|&a| a != 255).then_some(alpha))
            }
        };
        Some(Self(Arc::new(BitmapPixels {
            source: source.clone(),
            size,
            color,
            alpha,
        })))
    }
}

impl Hash for Bitmap {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.source.hash(state);
    }
}

impl CustomImage for Bitmap {
    fn color_channel(&self) -> &[u8] {
        &self.0.color
    }

    fn alpha_channel(&self) -> Option<&[u8]> {
        self.0.alpha.as_deref()
    }

    fn bits_per_component(&self) -> BitsPerComponent {
        BitsPerComponent::Eight
    }

    fn size(&self) -> (u32, u32) {
        self.0.size
    }

    fn icc_profile(&self) -> Option<&[u8]> {
        None
    }

    fn color_space(&self) -> ImageColorspace {
        ImageColorspace::Rgb
    }
}

fn glyphs(surface: &mut Surface, run: &GlyphRun, fonts: &mut FontBook) -> Result<(), String> {
    if run.glyphs.is_empty() || run.size <= 0.0 {
        return Ok(());
    }
    if fonts.embedding(run.face).is_none() {
        return Err(format!(
            "page uses font face {} that is no longer indexed; write the previous document's pages before rendering another",
            run.face
        ));
    }
    let scale = run.scale_x.max(0.05);
    let skew = if run.italic { -0.22 } else { 0.0 };
    surface.push_transform(&Transform::from_row(
        scale,
        0.0,
        skew,
        1.0,
        run.origin.0,
        run.origin.1,
    ));
    let bold = run.bold_stroke.map(|width| PdfStroke {
        paint: color(run.color).into(),
        width,
        ..PdfStroke::default()
    });
    if let Some(font) = fonts.pdf_font(run.face) {
        let glyphs: Vec<u32> = run.glyphs.iter().map(|(glyph, _)| *glyph).collect();
        surface.set_stroke(bold);
        draw_text(surface, run, scale, font, &glyphs, 1.0);
    } else if let Some((face, font)) = substitute(fonts, run) {
        // fsType이 포함을 막는 얼굴은 윤곽선도 남기지 않는다. 글꼴이 없을 때와 같은
        // 대체 사슬에서 고른 얼굴로 글자를 그려 모양과 텍스트 층을 함께 지킨다.
        let glyphs = substitute_glyphs(fonts, run, face);
        surface.set_stroke(bold);
        draw_text(surface, run, scale, font, &glyphs, 1.0);
    }
    surface.set_stroke(None);
    surface.pop();
    Ok(())
}

/// PDF에 넣을 수 없는 얼굴을 글꼴이 없을 때와 같은 대체 사슬로 바꾼다.
fn substitute(fonts: &mut FontBook, run: &GlyphRun) -> Option<(usize, krilla::text::Font)> {
    let (face, _) = fonts.fallback(run.face, run.bold_stroke.is_some())?;
    let font = fonts.pdf_font(face)?;
    Some((face, font))
}

/// 대체 얼굴에서 이 글자 묶음의 글리프 번호를 찾는다. 없는 글자는 `.notdef`로 둔다.
fn substitute_glyphs(fonts: &mut FontBook, run: &GlyphRun, face: usize) -> Vec<u32> {
    run.glyphs
        .iter()
        .enumerate()
        .map(|(index, _)| {
            run.ranges
                .get(index)
                .and_then(|range| run.text.get(range.clone()))
                .and_then(|cluster| cluster.chars().next())
                .and_then(|character| fonts.glyph(face, character))
                .map_or(0, |info| info.glyph)
        })
        .collect()
}

/// 글자들을 PDF 텍스트로 쓴다. `opacity`가 0이면 보이지 않지만 텍스트 층에는 남는다.
fn draw_text(
    surface: &mut Surface,
    run: &GlyphRun,
    scale: f32,
    font: krilla::text::Font,
    glyph_ids: &[u32],
    opacity: f32,
) {
    let local: Vec<f32> = run.glyphs.iter().map(|(_, x)| x / scale).collect();
    if local.is_empty() {
        return;
    }
    let glyphs: Vec<KrillaGlyph> = (0..local.len())
        .map(|index| {
            let advance = local
                .get(index + 1)
                .map_or(0.0, |next| (next - local[index]) / run.size);
            let range = run.ranges.get(index).cloned().unwrap_or(0..0);
            KrillaGlyph::new(
                GlyphId::new(glyph_ids.get(index).copied().unwrap_or(0)),
                advance,
                0.0,
                0.0,
                0.0,
                range,
                None,
            )
        })
        .collect();
    surface.set_fill(Some(fill(run.color, opacity)));
    surface.draw_glyphs(
        Point::from_xy(local[0], 0.0),
        &glyphs,
        font,
        &run.text,
        run.size,
        false,
    );
}
