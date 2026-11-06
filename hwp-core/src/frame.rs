use serde::Serialize;

use crate::{
    on_parser_stack, panic_detail, parse_document, BlobKind, Document, Fatal, FormatHint,
    ParseOptions, Warning, SCHEMA_VERSION,
};

pub const FRAME_MAGIC: [u8; 4] = *b"HWPC";
pub const FRAME_VERSION: u32 = 1;

#[derive(Serialize)]
struct Success<'a> {
    schema: u32,
    ok: bool,
    document: &'a Document,
    warnings: &'a [Warning],
}

#[derive(Serialize)]
struct Failure<'a> {
    schema: u32,
    ok: bool,
    fatal: &'a Fatal,
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn encode(json: &[u8], blobs: &[crate::Blob]) -> Vec<u8> {
    let mut out = Vec::with_capacity(json.len() + 16);
    out.extend_from_slice(&FRAME_MAGIC);
    push_u32(&mut out, FRAME_VERSION);
    push_u32(&mut out, json.len() as u32);
    out.extend_from_slice(json);
    push_u32(&mut out, blobs.len() as u32);
    for blob in blobs {
        let id = blob.id.as_bytes();
        push_u32(&mut out, id.len() as u32);
        out.extend_from_slice(id);
        out.push(match blob.kind {
            BlobKind::Picture => 0,
            BlobKind::Font => 1,
        });
        push_u64(&mut out, blob.data.len() as u64);
        out.extend_from_slice(&blob.data);
    }
    out
}

/// 문서를 파서 스택에서 읽어 프레임으로 만든다. 직렬화도 깊이 재귀하므로 같은 스택에서 한다.
/// 읽기 실패와 파서 패닉도 프레임으로 돌려준다.
pub fn parse_frame(bytes: &[u8], hint: FormatHint) -> Vec<u8> {
    let frame = on_parser_stack(|| {
        let options = ParseOptions {
            hint,
            ..ParseOptions::default()
        };
        match parse_document(bytes, &options) {
            Ok(mut parsed) => {
                parsed.document.sanitize_non_finite();
                success_frame(&parsed.document, &parsed.warnings)
                    .unwrap_or_else(|_| internal_frame("serialization failed"))
            }
            Err(fatal) => failure_frame(&fatal),
        }
    });
    frame.unwrap_or_else(|fatal| failure_frame(&fatal))
}

/// `catch_unwind`가 잡은 패닉을 프레임으로 바꾼다.
pub fn panic_frame(payload: Box<dyn std::any::Any + Send>) -> Vec<u8> {
    internal_frame(&panic_detail(payload))
}

pub fn success_frame(
    document: &Document,
    warnings: &[Warning],
) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(&Success {
        schema: SCHEMA_VERSION,
        ok: true,
        document,
        warnings,
    })?;
    Ok(encode(&json, &document.bins))
}

pub fn failure_frame(fatal: &Fatal) -> Vec<u8> {
    let json = serde_json::to_vec(&Failure {
        schema: SCHEMA_VERSION,
        ok: false,
        fatal,
    })
    .unwrap_or_else(|_| {
        br#"{"schema":1,"ok":false,"fatal":{"kind":"Internal","detail":"serialization failed"}}"#
            .to_vec()
    });
    encode(&json, &[])
}

pub fn internal_frame(detail: &str) -> Vec<u8> {
    failure_frame(&Fatal::Internal(detail.to_string()))
}
