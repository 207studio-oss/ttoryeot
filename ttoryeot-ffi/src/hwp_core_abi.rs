use std::panic::{AssertUnwindSafe, catch_unwind};
use std::slice;

use hwp_core::{FormatHint, frame};

pub const HWP_CORE_ABI_VERSION: u32 = 1;

#[repr(C)]
pub struct HwpCoreBuf {
    pub ptr: *mut u8,
    pub len: usize,
}

impl HwpCoreBuf {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
        }
    }
}

#[cfg(test)]
const PANIC_INPUT: &[u8] = b"\0hwp-core-test-inject-panic";

#[cfg(test)]
fn maybe_inject_panic(bytes: &[u8]) {
    if bytes == PANIC_INPUT {
        panic!("injected panic");
    }
}

fn hint(value: u32) -> Option<FormatHint> {
    match value {
        0 => Some(FormatHint::Auto),
        1 => Some(FormatHint::Hwp),
        2 => Some(FormatHint::Hwpx),
        _ => None,
    }
}

fn build_frame(bytes: &[u8], hint: FormatHint) -> Vec<u8> {
    #[cfg(test)]
    maybe_inject_panic(bytes);
    frame::parse_frame(bytes, hint)
}

#[unsafe(no_mangle)]
pub extern "C" fn hwp_core_abi_version() -> u32 {
    HWP_CORE_ABI_VERSION
}

/// # Safety
/// `bytes` must point to `len` readable bytes and `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hwp_core_parse(
    bytes: *const u8,
    len: usize,
    hint_value: u32,
    out: *mut HwpCoreBuf,
) -> i32 {
    if out.is_null() {
        return -1;
    }
    unsafe {
        out.write(HwpCoreBuf::empty());
    }
    if bytes.is_null() || len > isize::MAX as usize {
        return -1;
    }
    let Some(hint) = hint(hint_value) else {
        return -1;
    };
    let input = unsafe { slice::from_raw_parts(bytes, len) };
    let frame = catch_unwind(AssertUnwindSafe(|| build_frame(input, hint)))
        .unwrap_or_else(frame::panic_frame);
    let mut boxed = frame.into_boxed_slice();
    let ptr = boxed.as_mut_ptr();
    let len = boxed.len();
    std::mem::forget(boxed);
    unsafe {
        out.write(HwpCoreBuf { ptr, len });
    }
    0
}

/// # Safety
/// `buf` must come from `hwp_core_parse` and not have been freed already.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hwp_core_buf_free(buf: HwpCoreBuf) {
    if buf.ptr.is_null() {
        return;
    }
    let slice = std::ptr::slice_from_raw_parts_mut(buf.ptr, buf.len);
    drop(unsafe { Box::from_raw(slice) });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_json(buf: HwpCoreBuf) -> String {
        let bytes = unsafe { slice::from_raw_parts(buf.ptr, buf.len) };
        assert_eq!(&bytes[..4], b"HWPC");
        let json_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let json = String::from_utf8(bytes[12..12 + json_len].to_vec()).unwrap();
        unsafe { hwp_core_buf_free(buf) };
        json
    }

    #[test]
    fn ffi_panic_becomes_internal_fatal() {
        let input = PANIC_INPUT;
        let mut out = HwpCoreBuf::empty();
        let status = unsafe { hwp_core_parse(input.as_ptr(), input.len(), 0, &mut out) };
        assert_eq!(status, 0);
        let json = frame_json(out);
        assert!(json.contains(r#""kind":"Internal""#), "{json}");
        assert!(json.contains("injected panic"), "{json}");
    }
}
