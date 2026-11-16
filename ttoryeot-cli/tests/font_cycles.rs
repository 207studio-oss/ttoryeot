#[cfg(unix)]
#[test]
fn scan_skips_directory_symlink_cycles_and_overlapping_roots() {
    use std::path::PathBuf;
    use ttoryeot::fonts::FontBook;

    let dir = std::env::temp_dir().join(format!("ttoryeot-font-cycle-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    let source = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    ));
    std::fs::copy(source, dir.join("nested/font.ttf")).unwrap();
    std::os::unix::fs::symlink(&dir, dir.join("nested/loop")).unwrap();
    let book = FontBook::scan(&[dir.clone(), dir.join("nested")]);
    assert_eq!(book.face_count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
