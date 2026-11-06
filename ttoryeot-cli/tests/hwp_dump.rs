use hwp_core::frame::{FRAME_MAGIC, FRAME_VERSION};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("hwp-dump-{}-{nanos}-{name}", std::process::id()))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn json_part(frame: &[u8]) -> &[u8] {
    let length = read_u32(frame, 8) as usize;
    &frame[12..12 + length]
}

#[test]
fn dump_not_hwp_fatal() {
    let path = temp_path("junk.bin");
    fs::write(&path, b"neither cfb nor zip").expect("write fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_hwp-dump"))
        .arg(&path)
        .output()
        .expect("run hwp-dump");
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        r#"{"schema":1,"ok":false,"fatal":{"kind":"NotHwp","detail":""}}"#
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("json response");
    assert_eq!(
        value,
        json!({
            "schema": 1,
            "ok": false,
            "fatal": {"kind": "NotHwp", "detail": ""}
        })
    );
}

#[test]
fn dump_not_hwp_is_reported_for_unimplemented_formats() {
    let path = temp_path("junk-text.bin");
    fs::write(&path, b"neither cfb nor zip").expect("write fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_hwp-dump"))
        .args(["--format", "text"])
        .arg(&path)
        .output()
        .expect("run hwp-dump");
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("json response");
    assert_eq!(value["fatal"]["kind"], "NotHwp");
}

#[test]
fn dump_rejects_missing_path() {
    let output = Command::new(env!("CARGO_BIN_EXE_hwp-dump"))
        .output()
        .expect("run hwp-dump");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn dump_frame_writes_fatal_frame() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("hwp-frame-{}-{nanos}.bin", std::process::id()));
    fs::write(&path, b"not a document").expect("write");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hwp-dump"))
        .args(["--format", "frame"])
        .arg(&path)
        .output()
        .expect("run hwp-dump");
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert_eq!(&output.stdout[..4], &FRAME_MAGIC);
    assert_eq!(read_u32(&output.stdout, 4), FRAME_VERSION);
    let json: Value = serde_json::from_slice(json_part(&output.stdout)).expect("json");
    assert_eq!(json["schema"], 1);
    assert_eq!(json["ok"], false);
    assert_eq!(json["fatal"]["kind"], "NotHwp");
    assert_eq!(
        read_u32(&output.stdout, 12 + json_part(&output.stdout).len()),
        0
    );
}
