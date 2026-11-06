use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::slice;

use serde_json::{Value, json};
use ttoryeot::fonts::FontBook;

pub mod hwp_core_abi;

#[global_allocator]
static ALLOCATOR: ttoryeot_alloc::Allocator = ttoryeot_alloc::Allocator;

pub const TTORYEOT_ABI_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TtoryeotBytes {
    pub ptr: *const u8,
    pub len: usize,
}

pub struct TtoryeotResult {
    pdf: Vec<u8>,
    metadata: Vec<u8>,
}

impl TtoryeotResult {
    fn new(pdf: Vec<u8>, metadata: Value) -> Self {
        Self {
            pdf,
            metadata: serde_json::to_vec(&metadata).expect("JSON serialization"),
        }
    }

    fn failure(kind: &str, detail: &str) -> Self {
        Self::new(
            Vec::new(),
            json!({"schema": 1, "ok": false, "error": {"kind": kind, "detail": detail}}),
        )
    }
}

/// 앱이 넘긴 글꼴 범위를 복사하지 않고 보는 값.
struct CallerBytes {
    ptr: *const u8,
    len: usize,
}

// SAFETY: 이 값은 `convert` 안의 `FontBook`과 그 글꼴을 쓰는 krilla 문서에만 들어가고,
// 둘 다 `convert`가 돌아가거나 panic으로 풀리기 전에 버려진다. 렌더러와 krilla에는 글꼴
// 바이트를 붙드는 전역 저장소나 스레드가 없다. 앱은 이 호출 동안 범위를 읽을 수 있게 둔다.
unsafe impl Send for CallerBytes {}
unsafe impl Sync for CallerBytes {}

impl AsRef<[u8]> for CallerBytes {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            bytes(TtoryeotBytes {
                ptr: self.ptr,
                len: self.len,
            })
        }
    }
}

fn convert(document: &[u8], password: Option<&str>, font_data: &[&[u8]]) -> TtoryeotResult {
    let mut fonts = FontBook::default();
    for (index, bytes) in font_data.iter().enumerate() {
        fonts.add_bytes(
            PathBuf::from(format!("provided-font-{index}")),
            CallerBytes {
                ptr: bytes.as_ptr(),
                len: bytes.len(),
            },
        );
    }
    let provided = fonts.face_count();
    match ttoryeot::to_pdf(document, password, &mut fonts) {
        Ok(output) => TtoryeotResult::new(
            output.pdf,
            json!({
                "schema": 1,
                "ok": true,
                "pages": output.pages,
                "warnings": output.warnings,
                "fontFaces": {
                    "provided": provided,
                    "documentEmbedded": fonts.document_face_count(),
                    "hasEmbeddableFace": fonts.any_face().is_some()
                }
            }),
        ),
        Err(ttoryeot::Error::Parse(error)) => TtoryeotResult::failure(error.kind(), error.detail()),
        Err(ttoryeot::Error::Pdf(detail)) => TtoryeotResult::failure("Pdf", &detail),
    }
}

fn valid_bytes(value: TtoryeotBytes) -> bool {
    value.len <= isize::MAX as usize && (value.len == 0 || !value.ptr.is_null())
}

// The caller must supply a readable range when len is nonzero.
unsafe fn bytes<'a>(value: TtoryeotBytes) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.ptr, value.len) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ttoryeot_abi_version() -> u32 {
    TTORYEOT_ABI_VERSION
}

/// # Safety
/// Nonempty byte ranges and the font array must be readable for this call. `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttoryeot_render(
    document: TtoryeotBytes,
    password: TtoryeotBytes,
    font_data: *const TtoryeotBytes,
    font_count: usize,
    out: *mut *mut TtoryeotResult,
) -> i32 {
    if out.is_null() {
        return -1;
    }
    unsafe { out.write(std::ptr::null_mut()) };
    if !valid_bytes(document)
        || !valid_bytes(password)
        || font_count > isize::MAX as usize / std::mem::size_of::<TtoryeotBytes>()
        || (font_count != 0 && font_data.is_null())
    {
        return -1;
    }
    let font_ranges = if font_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(font_data, font_count) }
    };
    if font_ranges.iter().any(|range| !valid_bytes(*range)) {
        return -1;
    }
    let document = unsafe { bytes(document) };
    let password = if password.ptr.is_null() {
        None
    } else {
        match std::str::from_utf8(unsafe { bytes(password) }) {
            Ok(password) => Some(password),
            Err(_) => return -1,
        }
    };
    let fonts: Vec<&[u8]> = font_ranges
        .iter()
        .map(|range| unsafe { bytes(*range) })
        .collect();
    let result = catch_unwind(AssertUnwindSafe(|| convert(document, password, &fonts)))
        .unwrap_or_else(|_| TtoryeotResult::failure("Internal", "renderer panicked"));
    unsafe { out.write(Box::into_raw(Box::new(result))) };
    0
}

/// # Safety
/// `result` must be null or a live result returned by `ttoryeot_render`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttoryeot_result_pdf(result: *const TtoryeotResult) -> TtoryeotBytes {
    unsafe { result.as_ref() }.map_or(
        TtoryeotBytes {
            ptr: std::ptr::null(),
            len: 0,
        },
        |result| TtoryeotBytes {
            ptr: result.pdf.as_ptr(),
            len: result.pdf.len(),
        },
    )
}

/// # Safety
/// `result` must be null or a live result returned by `ttoryeot_render`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttoryeot_result_metadata(result: *const TtoryeotResult) -> TtoryeotBytes {
    unsafe { result.as_ref() }.map_or(
        TtoryeotBytes {
            ptr: std::ptr::null(),
            len: 0,
        },
        |result| TtoryeotBytes {
            ptr: result.metadata.as_ptr(),
            len: result.metadata.len(),
        },
    )
}

/// # Safety
/// `result` must be null or a live result returned by `ttoryeot_render`, freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttoryeot_result_free(result: *mut TtoryeotResult) {
    if !result.is_null() {
        drop(unsafe { Box::from_raw(result) });
    }
}
