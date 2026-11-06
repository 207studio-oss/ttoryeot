use hwp_core::{Document, Fatal, FormatHint, ParseOptions, SCHEMA_VERSION, Warning, parse};
use serde::Serialize;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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

#[derive(Serialize)]
struct TextSuccess {
    file: String,
    ok: bool,
    tables: usize,
    pictures: usize,
    paras: Vec<String>,
}

#[derive(Serialize)]
struct TextFailure<'a> {
    file: String,
    ok: bool,
    fatal: &'a Fatal,
}

#[derive(Serialize)]
struct LineSuccess<'a> {
    file: &'a str,
    ok: bool,
    #[serde(flatten)]
    line: &'a hwp_core::probe::ProbeLine,
}

#[derive(Serialize)]
struct LineFailure<'a> {
    file: &'a str,
    ok: bool,
    fatal: &'a Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Model,
    Frame,
    Text,
    Lines,
    NumberingRaw,
    ShapeFills,
}

#[derive(Debug)]
struct Args {
    format: OutputFormat,
    hint: FormatHint,
    paths: Vec<PathBuf>,
}

fn usage() -> &'static str {
    "usage: hwp-dump [--format model|frame|text|lines|numbering-raw|shape-fills] [--hint auto|hwp|hwpx] <file>..."
}

fn parse_format(value: &str) -> Option<OutputFormat> {
    match value {
        "model" => Some(OutputFormat::Model),
        "frame" => Some(OutputFormat::Frame),
        "text" => Some(OutputFormat::Text),
        "lines" => Some(OutputFormat::Lines),
        "numbering-raw" => Some(OutputFormat::NumberingRaw),
        "shape-fills" => Some(OutputFormat::ShapeFills),
        _ => None,
    }
}

fn parse_hint(value: &str) -> Option<FormatHint> {
    match value {
        "auto" => Some(FormatHint::Auto),
        "hwp" => Some(FormatHint::Hwp),
        "hwpx" => Some(FormatHint::Hwpx),
        _ => None,
    }
}

fn next_value<I>(args: &mut I, name: &str) -> Result<OsString, String>
where
    I: Iterator<Item = OsString>,
{
    args.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn parse_args<I>(mut args: I) -> Result<Args, String>
where
    I: Iterator<Item = OsString>,
{
    let mut format = OutputFormat::Model;
    let mut hint = FormatHint::Auto;
    let mut paths = Vec::new();
    while let Some(argument) = args.next() {
        let text = argument.to_string_lossy();
        match text.as_ref() {
            "-h" | "--help" => return Err(String::new()),
            "--format" => {
                let value = next_value(&mut args, "--format")?;
                format = parse_format(&value.to_string_lossy())
                    .ok_or_else(|| format!("unknown format: {}", value.to_string_lossy()))?;
            }
            "--hint" => {
                let value = next_value(&mut args, "--hint")?;
                hint = parse_hint(&value.to_string_lossy())
                    .ok_or_else(|| format!("unknown hint: {}", value.to_string_lossy()))?;
            }
            _ if text.starts_with("--format=") => {
                format = parse_format(&text["--format=".len()..])
                    .ok_or_else(|| format!("unknown format: {}", &text["--format=".len()..]))?;
            }
            _ if text.starts_with("--hint=") => {
                hint = parse_hint(&text["--hint=".len()..])
                    .ok_or_else(|| format!("unknown hint: {}", &text["--hint=".len()..]))?;
            }
            _ => paths.push(PathBuf::from(argument)),
        }
    }
    if paths.is_empty() {
        return Err("no input files".to_string());
    }
    Ok(Args {
        format,
        hint,
        paths,
    })
}

fn read_input(path: &Path) -> Result<Vec<u8>, Fatal> {
    if path == Path::new("-") {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|_| Fatal::Internal("stdin read failed".to_string()))?;
        return Ok(bytes);
    }
    std::fs::read(path).map_err(|_| Fatal::Internal("file read failed".to_string()))
}

enum Response {
    Json(String),
    Frame(Vec<u8>),
}

fn response(bytes: &[u8], format: OutputFormat, hint: FormatHint, file: &str) -> Response {
    if format == OutputFormat::NumberingRaw {
        return match hwp_core::hwp5::numbering::numbering_raw(bytes) {
            Ok(heads) => {
                json_response(serde_json::json!({"file": file, "ok": true, "heads": heads}))
            }
            Err(fatal) => json_response(LineFailure {
                file,
                ok: false,
                fatal: &fatal,
            }),
        };
    }
    let options = ParseOptions {
        hint,
        ..ParseOptions::default()
    };
    match parse(bytes, &options) {
        Ok(mut parsed) => {
            parsed.document.sanitize_non_finite();
            if format == OutputFormat::ShapeFills {
                return json_response(serde_json::json!({
                    "file": file, "ok": true,
                    "fills": hwp_core::probe::shape_fill_distribution(&parsed.document)
                }));
            }
            if format == OutputFormat::Frame {
                return Response::Frame(
                    hwp_core::frame::success_frame(&parsed.document, &parsed.warnings)
                        .unwrap_or_else(|_| {
                            hwp_core::frame::internal_frame("serialization failed")
                        }),
                );
            }
            if format == OutputFormat::Text {
                let summary = hwp_core::text::text_summary(&parsed.document);
                return json_response(TextSuccess {
                    file: file.to_string(),
                    ok: true,
                    tables: summary.tables,
                    pictures: summary.pictures,
                    paras: summary.paragraphs,
                });
            }
            if format == OutputFormat::Lines {
                let lines = hwp_core::probe::probe(&parsed.document);
                let mut output = String::new();
                for line in &lines {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(
                        &serde_json::to_string(&LineSuccess {
                            file,
                            ok: true,
                            line,
                        })
                        .unwrap_or_else(|_| fallback_line()),
                    );
                }
                return Response::Json(output);
            }
            if format != OutputFormat::Model {
                let fatal = Fatal::Internal("output format is not implemented".to_string());
                return json_response(Failure {
                    schema: SCHEMA_VERSION,
                    ok: false,
                    fatal: &fatal,
                });
            }
            json_response(Success {
                schema: SCHEMA_VERSION,
                ok: true,
                document: &parsed.document,
                warnings: &parsed.warnings,
            })
        }
        Err(fatal) => {
            if format == OutputFormat::Frame {
                Response::Frame(hwp_core::frame::failure_frame(&fatal))
            } else if format == OutputFormat::Text {
                json_response(TextFailure {
                    file: file.to_string(),
                    ok: false,
                    fatal: &fatal,
                })
            } else if format == OutputFormat::Lines {
                json_response(LineFailure {
                    file,
                    ok: false,
                    fatal: &fatal,
                })
            } else {
                json_response(Failure {
                    schema: SCHEMA_VERSION,
                    ok: false,
                    fatal: &fatal,
                })
            }
        }
    }
}

fn json_response<T: Serialize>(value: T) -> Response {
    Response::Json(serde_json::to_string(&value).unwrap_or_else(|_| fallback_line()))
}

fn fallback_line() -> String {
    format!(
        "{{\"schema\":{SCHEMA_VERSION},\"ok\":false,\"fatal\":{{\"kind\":\"Internal\",\"detail\":\"serialization failed\"}}}}"
    )
}

fn input_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn main() {
    let args = match parse_args(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("{message}");
            }
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    for path in &args.paths {
        let name = input_name(path);
        let response = match read_input(path) {
            Ok(bytes) => response(&bytes, args.format, args.hint, &name),
            Err(fatal) => {
                if args.format == OutputFormat::Frame {
                    Response::Frame(hwp_core::frame::failure_frame(&fatal))
                } else if args.format == OutputFormat::Text {
                    json_response(TextFailure {
                        file: name,
                        ok: false,
                        fatal: &fatal,
                    })
                } else if args.format == OutputFormat::Lines {
                    json_response(LineFailure {
                        file: &name,
                        ok: false,
                        fatal: &fatal,
                    })
                } else {
                    json_response(Failure {
                        schema: SCHEMA_VERSION,
                        ok: false,
                        fatal: &fatal,
                    })
                }
            }
        };
        let written = match response {
            Response::Json(line) if line.is_empty() => true,
            Response::Json(line) => writeln!(output, "{line}").is_ok(),
            Response::Frame(frame) => output.write_all(&frame).is_ok(),
        };
        if !written {
            std::process::exit(1);
        }
    }
}
