use hwp_core::{parse, parse_on_current_thread, ParseOptions};

#[test]
fn current_thread_matches_the_guarded_entry_for_documents_and_errors() {
    let options = ParseOptions::default();
    for bytes in [
        include_bytes!("fixtures/synthetic.hwpx").as_slice(),
        include_bytes!("fixtures/synthetic-password-v0.hwp").as_slice(),
        b"not a document".as_slice(),
    ] {
        let threaded = parse(bytes, &options);
        let current = parse_on_current_thread(bytes, &options);
        match (threaded, current) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
            (a, b) => panic!("different results: {a:?} and {b:?}"),
        }
    }
}
