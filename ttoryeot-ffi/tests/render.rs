use serde_json::Value;
use ttoryeot_ffi::{
    TtoryeotBytes, TtoryeotResult, ttoryeot_render, ttoryeot_result_free, ttoryeot_result_metadata,
    ttoryeot_result_pdf,
};

const HWPX: &[u8] = include_bytes!("../../hwp-core/tests/fixtures/synthetic.hwpx");
const HWP: &[u8] = include_bytes!("../../hwp-core/tests/fixtures/synthetic-password-v0.hwp");
const UNSUPPORTED: &[u8] =
    include_bytes!("../../hwp-core/tests/fixtures/synthetic-unsupported-encryption-v3.hwp");
const FONT: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");

fn range(bytes: &[u8]) -> TtoryeotBytes {
    TtoryeotBytes {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }
}

fn absent() -> TtoryeotBytes {
    TtoryeotBytes {
        ptr: std::ptr::null(),
        len: 0,
    }
}

fn convert(document: &[u8], password: Option<&[u8]>, fonts: &[&[u8]]) -> (Vec<u8>, Value) {
    let fonts: Vec<_> = fonts.iter().map(|font| range(font)).collect();
    let mut result: *mut TtoryeotResult = std::ptr::null_mut();
    let status = unsafe {
        ttoryeot_render(
            range(document),
            password.map_or_else(absent, range),
            fonts.as_ptr(),
            fonts.len(),
            &mut result,
        )
    };
    assert_eq!(status, 0);
    assert!(!result.is_null());
    let pdf = unsafe { ttoryeot_result_pdf(result) };
    let metadata = unsafe { ttoryeot_result_metadata(result) };
    let pdf = if pdf.len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(pdf.ptr, pdf.len) }.to_vec()
    };
    let metadata = unsafe { std::slice::from_raw_parts(metadata.ptr, metadata.len) };
    let metadata = serde_json::from_slice(metadata).expect("metadata JSON");
    unsafe { ttoryeot_result_free(result) };
    (pdf, metadata)
}

#[test]
fn synthetic_hwpx_returns_pdf_pages_warnings_and_font_counts() {
    let (pdf, metadata) = convert(HWPX, None, &[FONT]);
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(metadata["schema"], 1);
    assert_eq!(metadata["ok"], true);
    assert!(metadata["pages"].as_u64().unwrap() > 0);
    assert!(metadata["warnings"].is_array());
    assert_eq!(metadata["fontFaces"]["provided"], 1);
    assert_eq!(metadata["fontFaces"]["documentEmbedded"], 0);
    assert_eq!(metadata["fontFaces"]["hasEmbeddableFace"], true);
}

#[test]
fn font_count_reports_an_empty_or_invalid_font_set() {
    let (_, empty) = convert(HWPX, None, &[]);
    let (_, invalid) = convert(HWPX, None, &[b"not a font"]);
    assert_eq!(empty["fontFaces"]["provided"], 0);
    assert_eq!(invalid["fontFaces"]["provided"], 0);
    assert_eq!(empty["fontFaces"]["hasEmbeddableFace"], false);
    assert_eq!(invalid["fontFaces"]["hasEmbeddableFace"], false);
}

#[test]
fn encrypted_hwp_distinguishes_missing_wrong_and_correct_passwords() {
    let (pdf, missing) = convert(HWP, None, &[FONT]);
    assert!(pdf.is_empty());
    assert_eq!(missing["error"]["kind"], "PasswordProtected");
    let (_, wrong) = convert(HWP, Some(b"wrong"), &[FONT]);
    assert_eq!(wrong["error"]["kind"], "WrongPassword");
    let (_, empty) = convert(HWP, Some(b""), &[FONT]);
    assert_eq!(empty["error"]["kind"], "WrongPassword");
    let (pdf, correct) = convert(HWP, Some(b"12345"), &[FONT]);
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(correct["pages"], 1);
    let (_, unsupported) = convert(UNSUPPORTED, None, &[FONT]);
    assert_eq!(unsupported["error"]["kind"], "UnsupportedEncryption");
    assert!(
        unsupported["error"]["detail"]
            .as_str()
            .unwrap()
            .contains('3')
    );
}

#[test]
fn invalid_arguments_do_not_return_a_result() {
    let mut result = std::ptr::dangling_mut::<TtoryeotResult>();
    let invalid = TtoryeotBytes {
        ptr: std::ptr::null(),
        len: 1,
    };
    let status = unsafe { ttoryeot_render(invalid, absent(), std::ptr::null(), 0, &mut result) };
    assert_eq!(status, -1);
    assert!(result.is_null());
    let status = unsafe {
        ttoryeot_render(
            range(HWPX),
            range(&[0xff]),
            std::ptr::null(),
            0,
            &mut result,
        )
    };
    assert_eq!(status, -1);
    assert!(result.is_null());
    let status =
        unsafe { ttoryeot_render(range(HWPX), absent(), std::ptr::null(), 1, &mut result) };
    assert_eq!(status, -1);
    assert!(result.is_null());
}

#[test]
fn result_stays_valid_after_the_caller_reuses_its_font_buffer() {
    let (expected, _) = convert(HWPX, None, &[FONT]);
    let mut font = FONT.to_vec();
    let fonts = [range(&font)];
    let mut result: *mut TtoryeotResult = std::ptr::null_mut();
    let status = unsafe { ttoryeot_render(range(HWPX), absent(), fonts.as_ptr(), 1, &mut result) };
    assert_eq!(status, 0);
    font.fill(0);
    drop(font);
    let pdf = unsafe { ttoryeot_result_pdf(result) };
    let pdf = unsafe { std::slice::from_raw_parts(pdf.ptr, pdf.len) }.to_vec();
    unsafe { ttoryeot_result_free(result) };
    assert_eq!(pdf, expected);
}
