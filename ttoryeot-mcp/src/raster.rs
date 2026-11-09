//! PDF 쪽을 PNG로 그린다. CLI에 같은 코드가 있다. 플랫폼 모듈끼리는 의존하지 않는다.

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

pub const DEFAULT_DPI: u32 = 144;
pub const MAX_DPI: u32 = 1200;
/// 한 쪽 이미지의 최대 화소 수. 큰 용지와 높은 DPI가 메모리를 다 쓰지 않게 막는다.
const MAX_PIXELS: f64 = 100_000_000.0;
const POINTS_PER_INCH: f32 = 72.0;

pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

/// 쪽 위 사각형(pt, 왼쪽 위 원점) 하나를 잘라 그릴 요청. 쪽 번호는 0부터다.
pub struct Crop {
    pub page: usize,
    pub rect: [f32; 4],
}

/// `pdf`에서 요청한 사각형마다 그 부분만 PNG로 그린다. 같은 쪽은 한 번만 그린다.
pub fn crops(pdf: Vec<u8>, dpi: u32, requests: &[Crop]) -> Result<Vec<PageImage>, String> {
    if !(1..=MAX_DPI).contains(&dpi) {
        return Err(format!("DPI는 1에서 {MAX_DPI} 사이여야 합니다: {dpi}"));
    }
    let scale = dpi as f32 / POINTS_PER_INCH;
    let pdf = Pdf::new(pdf).map_err(|error| format!("PDF를 다시 읽지 못했습니다: {error:?}"))?;
    let pages = pdf.pages();
    let mut drawn: std::collections::HashMap<usize, (u32, u32, Vec<u8>)> = Default::default();
    let mut out = Vec::new();
    for request in requests {
        let Some(page) = pages.get(request.page) else {
            return Err(format!("{}쪽이 없습니다", request.page + 1));
        };
        if let std::collections::hash_map::Entry::Vacant(slot) = drawn.entry(request.page) {
            let (width, height) = page.render_dimensions();
            if f64::from(width * scale) * f64::from(height * scale) > MAX_PIXELS {
                return Err(format!(
                    "{}쪽은 너무 커서 그리지 못합니다",
                    request.page + 1
                ));
            }
            let pixmap = hayro::render(
                page,
                &RenderCache::new(),
                &InterpreterSettings::default(),
                &RenderSettings::default(),
                &PixmapSettings {
                    x_scale: scale,
                    y_scale: scale,
                    bg_color: WHITE,
                },
            );
            slot.insert((
                u32::from(pixmap.width()),
                u32::from(pixmap.height()),
                pixmap.data_as_u8_slice().to_vec(),
            ));
        }
        let (width, height, rgba) = &drawn[&request.page];
        let [x, y, w, h] = request.rect;
        let pixel = |value: f32| (value * scale).round().max(0.0) as u32;
        let (x0, y0) = (pixel(x).min(width - 1), pixel(y).min(height - 1));
        let x1 = pixel(x + w).clamp(x0 + 1, *width);
        let y1 = pixel(y + h).clamp(y0 + 1, *height);
        let (cw, ch) = (x1 - x0, y1 - y0);
        let mut cropped = Vec::with_capacity((cw * ch * 4) as usize);
        for row in y0..y0 + ch {
            let start = ((row * width + x0) * 4) as usize;
            cropped.extend_from_slice(&rgba[start..start + (cw * 4) as usize]);
        }
        out.push(PageImage {
            width: cw,
            height: ch,
            png: encode(&cropped, cw, ch)?,
        });
    }
    Ok(out)
}

/// `pdf`의 쪽을 차례로 그려 `each`에 넘긴다. 쪽 번호는 0부터다.
pub fn render_pages(
    pdf: Vec<u8>,
    dpi: u32,
    mut each: impl FnMut(usize, PageImage) -> Result<(), String>,
) -> Result<(), String> {
    if !(1..=MAX_DPI).contains(&dpi) {
        return Err(format!("DPI는 1에서 {MAX_DPI} 사이여야 합니다: {dpi}"));
    }
    let scale = dpi as f32 / POINTS_PER_INCH;
    let pdf = Pdf::new(pdf).map_err(|error| format!("PDF를 다시 읽지 못했습니다: {error:?}"))?;
    let pixmap_settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
    };
    for (index, page) in pdf.pages().iter().enumerate() {
        let (width, height) = page.render_dimensions();
        let pixels = f64::from(width * scale) * f64::from(height * scale);
        if pixels > MAX_PIXELS {
            return Err(format!(
                "{}쪽은 {dpi} DPI에서 너무 큽니다. DPI를 낮추세요",
                index + 1
            ));
        }
        let pixmap = hayro::render(
            page,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &pixmap_settings,
        );
        let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        let png = encode(pixmap.data_as_u8_slice(), width, height)?;
        each(index, PageImage { width, height, png })?;
    }
    Ok(())
}

fn encode(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("PNG를 쓰지 못했습니다: {error}"))?;
    writer
        .write_image_data(rgba)
        .map_err(|error| format!("PNG를 쓰지 못했습니다: {error}"))?;
    writer
        .finish()
        .map_err(|error| format!("PNG를 쓰지 못했습니다: {error}"))?;
    Ok(out)
}
