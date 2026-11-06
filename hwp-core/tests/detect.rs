use hwp_core::{detect, parse, Fatal, Format, ParseOptions};

#[test]
fn detect_cfb_zip_hwp3_hwpml() {
    let cfb = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0];
    assert_eq!(detect(&cfb), Format::Cfb);
    assert_eq!(detect(b"PK\x03\x04rest"), Format::Zip);
    assert_eq!(detect(b"HWP Document File V3.00"), Format::Hwp3);
    assert_eq!(
        detect(b"<?xml version=\"1.0\"?><HWPML><HEAD/></HWPML>"),
        Format::Hwpml
    );
    assert_eq!(
        detect(b"\xEF\xBB\xBF \n<?xml version=\"1.0\"?><HWPML/>"),
        Format::Hwpml
    );
    let mut long_declaration = b"<?xml version=\"1.0\"?>".to_vec();
    long_declaration.extend(std::iter::repeat_n(b' ', 8192));
    long_declaration.extend_from_slice(b"<HWPML><HEAD/></HWPML>");
    assert_eq!(detect(&long_declaration), Format::Hwpml);
    assert_eq!(detect(b"<?xml version=\"1.0\"?><root/>"), Format::Unknown);
    assert_eq!(
        detect(b"<?xml version=\"1.0\"?><root>HWPML</root>"),
        Format::Unknown
    );
    assert_eq!(detect(b""), Format::Unknown);
    assert_eq!(detect(b"not a document"), Format::Unknown);
}

#[test]
fn unsupported_formats_are_specific_fatals() {
    let options = ParseOptions::default();
    assert_eq!(parse(b"", &options), Err(Fatal::NotHwp));
    assert_eq!(
        parse(b"HWP Document File V3.00", &options),
        Err(Fatal::Hwp3Unsupported)
    );
    assert_eq!(
        parse(b"<?xml version=\"1.0\"?><HWPML/>", &options),
        Err(Fatal::HwpmlUnsupported)
    );
    assert_eq!(parse(b"PK\x03\x04rest", &options), Err(Fatal::NotHwp));
}
