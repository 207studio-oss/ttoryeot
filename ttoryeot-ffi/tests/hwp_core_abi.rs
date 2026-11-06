use ttoryeot_ffi::hwp_core_abi::{HwpCoreBuf, hwp_core_buf_free, hwp_core_parse};

#[test]
fn ffi_rejects_null_args() {
    let input = b"not a document";
    let mut out = HwpCoreBuf::empty();
    let status = unsafe { hwp_core_parse(std::ptr::null(), 0, 0, &mut out) };
    assert_eq!(status, -1);
    assert!(out.ptr.is_null());
    assert_eq!(out.len, 0);

    let status = unsafe { hwp_core_parse(input.as_ptr(), input.len(), 0, std::ptr::null_mut()) };
    assert_eq!(status, -1);

    let status = unsafe { hwp_core_parse(input.as_ptr(), input.len(), 99, &mut out) };
    assert_eq!(status, -1);
    assert!(out.ptr.is_null());
    assert_eq!(out.len, 0);

    let status = unsafe { hwp_core_parse(input.as_ptr(), input.len(), 0, &mut out) };
    assert_eq!(status, 0);
    assert!(!out.ptr.is_null());
    assert!(&unsafe { std::slice::from_raw_parts(out.ptr, out.len) }[..4] == b"HWPC");
    unsafe { hwp_core_buf_free(out) };
}
