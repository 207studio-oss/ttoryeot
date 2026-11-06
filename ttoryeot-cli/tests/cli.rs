//! 명령줄 도구가 실제 파일을 받아 PDF를 내는지 확인한다.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ttoryeot")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hwp-core/tests/fixtures")
        .join(name)
}

fn fonts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/fonts")
}

fn temp(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

fn run(arguments: &[&str]) -> Output {
    Command::new(binary())
        .args(arguments)
        .output()
        .expect("run ttoryeot")
}

fn run_with_password(arguments: &[&str], password: &str) -> Output {
    Command::new(binary())
        .args(arguments)
        .env("TTORYEOT_TEST_PASSWORD", password)
        .output()
        .expect("run ttoryeot")
}

#[test]
fn help_lists_both_commands() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("render"), "{text}");
    assert!(text.contains("fonts"), "{text}");
}

#[test]
fn renders_a_hwpx_document_to_pdf() {
    let input = fixture("synthetic.hwpx");
    let output = temp("cli-render.pdf");
    let _ = std::fs::remove_file(&output);
    let result = run(&[
        "render",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
        "--json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).expect("JSON report");
    assert!(report["pages"].as_u64().unwrap() >= 1, "{report}");
    assert!(report["bytes"].as_u64().unwrap() > 1_000, "{report}");
    assert_eq!(report["warnings"], 0);
    let pdf = std::fs::read(&output).expect("read pdf");
    assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
    let _ = std::fs::remove_file(&output);
}

#[test]
fn encrypt_version_0_opens_with_the_password_from_the_environment() {
    let input = fixture("synthetic-password-v0.hwp");
    let output = temp("cli-password-v0.pdf");
    let _ = std::fs::remove_file(&output);
    let result = run_with_password(
        &[
            "render",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--font",
            fonts().to_str().unwrap(),
            "--password-env",
            "TTORYEOT_TEST_PASSWORD",
            "--json",
        ],
        "12345",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).expect("JSON report");
    assert!(report["pages"].as_u64().unwrap() >= 1, "{report}");
    assert_eq!(report["warnings"], 0);
    let pdf = std::fs::read(&output).expect("read pdf");
    assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
    let _ = std::fs::remove_file(&output);
}

#[test]
fn encrypt_version_0_keeps_missing_and_wrong_password_distinct() {
    let input = fixture("synthetic-password-v0.hwp");
    let missing = run(&["render", input.to_str().unwrap()]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("암호가 필요한 문서"),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );

    let wrong = run_with_password(
        &[
            "render",
            input.to_str().unwrap(),
            "--password-env",
            "TTORYEOT_TEST_PASSWORD",
        ],
        "not-12345",
    );
    assert_eq!(wrong.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains("암호가 맞지 않습니다"),
        "{}",
        String::from_utf8_lossy(&wrong.stderr)
    );
}

#[test]
fn unsupported_encrypt_version_names_the_version() {
    let input = fixture("synthetic-unsupported-encryption-v3.hwp");
    let result = run_with_password(
        &[
            "render",
            input.to_str().unwrap(),
            "--password-env",
            "TTORYEOT_TEST_PASSWORD",
        ],
        "12345",
    );
    assert_eq!(result.status.code(), Some(1));
    let text = String::from_utf8_lossy(&result.stderr);
    assert!(text.contains("EncryptVersion 3"), "{text}");
    assert!(text.contains("지원하지 않습니다"), "{text}");
}

#[test]
fn unsupported_encrypt_version_is_named_without_a_password() {
    let input = fixture("synthetic-unsupported-encryption-v3.hwp");
    let result = run(&["render", input.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let text = String::from_utf8_lossy(&result.stderr);
    assert!(text.contains("EncryptVersion 3"), "{text}");
    assert!(text.contains("지원하지 않습니다"), "{text}");
    assert!(!text.contains("암호가 필요한 문서"), "{text}");
}

#[test]
fn missing_input_explains_the_failure() {
    let result = run(&["render", "없는-문서.hwp"]);
    assert_eq!(result.status.code(), Some(1));
    let text = String::from_utf8_lossy(&result.stderr);
    assert!(text.contains("읽지 못했습니다"), "{text}");
}

#[test]
fn fonts_reports_indexed_faces() {
    let result = run(&["fonts", "--font", fonts().to_str().unwrap(), "--json"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).expect("JSON report");
    assert!(report["faces"].as_u64().unwrap() >= 1, "{report}");
    assert_eq!(report["fonts"]["함초롬바탕"], false);
}

fn stdout_json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn info_reports_pages_fonts_and_encryption() {
    let input = fixture("synthetic.hwpx");
    let report = stdout_json(&run(&[
        "info",
        input.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
        "--json",
    ]));
    assert_eq!(report["format"], "hwpx");
    assert_eq!(report["encrypted"], false);
    assert_eq!(report["opened"], true);
    assert!(report["pages"].as_u64().unwrap() >= 1, "{report}");
    assert!(report["fonts"].is_array(), "{report}");
    assert!(report["metadata"].is_object(), "{report}");
}

#[test]
fn info_describes_a_locked_document_without_a_password() {
    let input = fixture("synthetic-password-v0.hwp");
    let report = stdout_json(&run(&["info", input.to_str().unwrap(), "--json"]));
    assert_eq!(report["encrypted"], true);
    assert_eq!(report["opened"], false);
}

#[test]
fn info_opens_a_locked_document_with_the_password_variable() {
    let input = fixture("synthetic-password-v0.hwp");
    let report = stdout_json(&run_with_password(
        &[
            "info",
            input.to_str().unwrap(),
            "--font",
            fonts().to_str().unwrap(),
            "--password-env",
            "TTORYEOT_TEST_PASSWORD",
            "--json",
        ],
        "12345",
    ));
    assert_eq!(report["encrypted"], true);
    assert_eq!(report["opened"], true, "{report}");
}

#[test]
fn password_cannot_be_given_on_the_command_line() {
    let input = fixture("synthetic-password-v0.hwp");
    let output = run(&["info", input.to_str().unwrap(), "--password", "secret"]);
    assert!(!output.status.success());
}

#[test]
fn text_prints_plain_text_of_the_chosen_page() {
    let input = fixture("synthetic.hwpx");
    let font = fonts();
    let all = stdout_json(&run(&[
        "text",
        input.to_str().unwrap(),
        "--font",
        font.to_str().unwrap(),
        "--json",
    ]));
    let total = all["page_count"].as_u64().unwrap();
    assert!(total >= 2, "{all}");
    assert!(all["text"].as_str().unwrap().contains('\u{C}'), "{all}");

    let first = stdout_json(&run(&[
        "text",
        input.to_str().unwrap(),
        "--font",
        font.to_str().unwrap(),
        "--pages",
        "1",
        "--json",
    ]));
    assert_eq!(first["pages"], serde_json::json!([1]));
    let text = first["text"].as_str().unwrap();
    assert!(!text.trim().is_empty(), "{first}");
    assert!(!text.contains('\u{C}'), "{first}");
    assert!(all["text"].as_str().unwrap().starts_with(text), "{all}");
}

#[test]
fn text_writes_markdown_to_a_file() {
    let input = fixture("synthetic.hwpx");
    let output = temp("cli-text.md");
    let _ = std::fs::remove_file(&output);
    let result = run(&[
        "text",
        input.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
        "--format",
        "markdown",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let markdown = std::fs::read_to_string(&output).expect("read markdown");
    assert!(!markdown.trim().is_empty());
    let _ = std::fs::remove_file(&output);
}

#[test]
fn a_page_range_outside_the_document_fails() {
    let input = fixture("synthetic.hwpx");
    let output = run(&[
        "text",
        input.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
        "--pages",
        "9999",
    ]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn png_writes_the_chosen_pages_at_the_chosen_dpi() {
    let input = fixture("synthetic.hwpx");
    let dir = fresh_dir("cli-png");
    let report = stdout_json(&run(&[
        "png",
        input.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
        "--output",
        dir.to_str().unwrap(),
        "--pages",
        "1-2",
        "--dpi",
        "72",
        "--json",
    ]));
    assert_eq!(report["files"].as_array().unwrap().len(), 2, "{report}");
    let first = &report["files"][0];
    let path = PathBuf::from(first["path"].as_str().unwrap());
    let bytes = std::fs::read(&path).expect("read png");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(u64::from(width), first["width"].as_u64().unwrap());
    assert!((500..700).contains(&width), "{width}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn png_rejects_an_impossible_dpi() {
    let input = fixture("synthetic.hwpx");
    let dir = fresh_dir("cli-png-dpi");
    let output = run(&[
        "png",
        input.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
        "--output",
        dir.to_str().unwrap(),
        "--dpi",
        "0",
    ]);
    assert!(!output.status.success());
}

#[test]
fn render_refuses_to_overwrite_its_input() {
    let copy = temp("cli-own-input.hwpx");
    std::fs::copy(fixture("synthetic.hwpx"), &copy).unwrap();
    let before = std::fs::read(&copy).unwrap();
    let output = run(&[
        "render",
        copy.to_str().unwrap(),
        "--output",
        copy.to_str().unwrap(),
        "--font",
        fonts().to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&copy).unwrap(), before);
    let _ = std::fs::remove_file(&copy);
}
