use hwp_core::frame::{failure_frame, success_frame, FRAME_MAGIC, FRAME_VERSION};
use hwp_core::*;
use serde_json::Value;

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn json_part(frame: &[u8]) -> &[u8] {
    let length = read_u32(frame, 8) as usize;
    &frame[12..12 + length]
}

#[test]
fn frame_layout_v1() {
    let document = Document {
        wchar_positions: true,
        ..Document::default()
    };
    let warning = Warning::new(WarningKind::MissingBinData, Some(0), "bin3");
    let frame = success_frame(&document, std::slice::from_ref(&warning)).expect("frame");
    assert_eq!(&frame[..4], &FRAME_MAGIC);
    assert_eq!(read_u32(&frame, 4), FRAME_VERSION);
    let json: Value = serde_json::from_slice(json_part(&frame)).expect("json");
    assert_eq!(json["schema"], 1);
    assert_eq!(json["ok"], true);
    assert!(json["document"].is_object());
    assert_eq!(json["warnings"][0]["kind"], "MissingBinData");
    assert_eq!(json["warnings"][0]["section"], 0);
    assert_eq!(json["warnings"][0]["detail"], "bin3");
    assert!(!String::from_utf8_lossy(json_part(&frame)).contains("\"bins\""));
    let tail = 12 + json_part(&frame).len();
    assert_eq!(read_u32(&frame, tail), 0);
}

#[test]
fn blobs_are_raw_bytes() {
    let data = vec![0x00, 0x80, 0xFF, 0xFE, 0x7B, 0x22];
    let document = Document {
        bins: vec![Blob {
            id: "bin1".to_string(),
            kind: BlobKind::Picture,
            data: data.clone().into(),
        }],
        ..Document::default()
    };
    let frame = success_frame(&document, &[]).expect("frame");
    let tail = 12 + json_part(&frame).len();
    let count = read_u32(&frame, tail) as usize;
    assert_eq!(count, 1);
    let mut offset = tail + 4;
    let id_len = read_u32(&frame, offset) as usize;
    offset += 4;
    assert_eq!(&frame[offset..offset + id_len], b"bin1");
    offset += id_len;
    assert_eq!(frame[offset], 0);
    offset += 1;
    let data_len = read_u64(&frame, offset) as usize;
    offset += 8;
    assert_eq!(data_len, data.len());
    assert_eq!(&frame[offset..offset + data_len], data.as_slice());
    assert_eq!(offset + data_len, frame.len());
    let json = String::from_utf8_lossy(json_part(&frame));
    assert!(!json.contains("AID//ns="));
    assert!(!json.contains("bin1"));
}

#[test]
fn failure_frame_is_empty_blob_frame() {
    let frame = failure_frame(&Fatal::PasswordProtected);
    assert_eq!(&frame[..4], &FRAME_MAGIC);
    let json: Value = serde_json::from_slice(json_part(&frame)).expect("json");
    assert_eq!(json["fatal"]["kind"], "PasswordProtected");
    assert_eq!(read_u32(&frame, 12 + json_part(&frame).len()), 0);
}
