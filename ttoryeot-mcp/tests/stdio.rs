use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hwp-core/tests/fixtures")
        .join(name)
}

fn temp(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start(password: Option<&str>) -> Self {
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/fonts");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ttoryeot-mcp"));
        command
            .arg("--font")
            .arg(fonts)
            .env_remove("TTORYEOT_PASSWORD")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(password) = password {
            command.env("TTORYEOT_PASSWORD", password);
        }
        let mut child = command.spawn().expect("start server");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut client = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"},
            }),
        );
        assert_eq!(
            init["result"]["serverInfo"]["name"], "ttoryeot-mcp",
            "{init}"
        );
        client.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).unwrap();
            assert!(read > 0, "server closed stdout");
            let message: Value = serde_json::from_str(&line).expect("stdout holds only JSON-RPC");
            if message["id"] == id {
                return message;
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        response["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text_block(result: &Value, index: usize) -> &str {
    assert_ne!(result["isError"], true, "{result}");
    result["content"][index]["text"].as_str().expect("text")
}

fn json_block(result: &Value, index: usize) -> Value {
    serde_json::from_str(text_block(result, index)).expect("JSON block")
}

fn error_text(result: &Value) -> &str {
    assert_eq!(result["isError"], true, "{result}");
    result["content"][0]["text"].as_str().expect("message")
}

#[test]
fn lists_exactly_the_six_tools() {
    let mut client = Client::start(None);
    let response = client.request("tools/list", json!({}));
    let mut names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "extract_text",
            "list_fonts",
            "read_document_info",
            "render_page_png",
            "render_pdf",
            "replace_text",
        ]
    );
    let schemas = response.to_string();
    assert!(!schemas.contains("password"), "{schemas}");
}

#[test]
fn reads_document_info() {
    let mut client = Client::start(None);
    let result = client.call(
        "read_document_info",
        json!({"path": fixture("synthetic.hwpx")}),
    );
    let report = json_block(&result, 0);
    assert_eq!(report["format"], "hwpx");
    assert_eq!(report["encrypted"], false);
    assert!(report["pages"].as_u64().unwrap() >= 1, "{report}");
}

#[test]
fn password_comes_only_from_the_environment() {
    let path = fixture("synthetic-password-v0.hwp");

    let mut locked = Client::start(None);
    let report = json_block(&locked.call("read_document_info", json!({"path": path})), 0);
    assert_eq!(report["opened"], false);
    let refused = locked.call("extract_text", json!({"path": path}));
    assert!(
        error_text(&refused).contains("TTORYEOT_PASSWORD"),
        "{refused}"
    );

    let mut wrong = Client::start(Some("not-the-password"));
    let refused = wrong.call("extract_text", json!({"path": path}));
    let message = error_text(&refused);
    assert!(!message.contains("not-the-password"), "{message}");

    let mut open = Client::start(Some("12345"));
    let report = json_block(&open.call("read_document_info", json!({"path": path})), 0);
    assert_eq!(report["opened"], true, "{report}");
}

#[test]
fn extracts_plain_text_and_markdown_by_page() {
    let mut client = Client::start(None);
    let path = fixture("synthetic.hwpx");
    let plain = client.call("extract_text", json!({"path": path, "pages": "1"}));
    assert!(!text_block(&plain, 0).trim().is_empty(), "{plain}");
    let meta = json_block(&plain, 1);
    assert_eq!(meta["pages"], json!([1]));

    let markdown = client.call("extract_text", json!({"path": path, "format": "markdown"}));
    assert!(!text_block(&markdown, 0).trim().is_empty(), "{markdown}");

    let outside = client.call("extract_text", json!({"path": path, "pages": "9999"}));
    assert!(!error_text(&outside).is_empty());
}

#[test]
fn renders_a_new_pdf_and_never_overwrites() {
    let mut client = Client::start(None);
    let input = fixture("synthetic.hwpx");
    let output = temp("mcp-render.pdf");
    let _ = std::fs::remove_file(&output);
    let result = client.call(
        "render_pdf",
        json!({"path": input, "output": output, "pages": "1"}),
    );
    let report = json_block(&result, 0);
    assert_eq!(report["pages"], json!([1]));
    let pdf = std::fs::read(&output).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));

    let again = client.call("render_pdf", json!({"path": input, "output": output}));
    assert!(!error_text(&again).is_empty());
    assert_eq!(std::fs::read(&output).unwrap(), pdf);
    let _ = std::fs::remove_file(&output);

    let before = std::fs::read(&input).unwrap();
    let onto_input = client.call("render_pdf", json!({"path": input, "output": input}));
    assert!(!error_text(&onto_input).is_empty());
    assert_eq!(std::fs::read(&input).unwrap(), before);
}

#[test]
fn replaces_text_into_a_new_hwpx_and_never_overwrites() {
    let mut client = Client::start(None);
    let input = fixture("synthetic.hwpx");
    let original = std::fs::read(&input).unwrap();
    let output = temp("mcp-replace.hwpx");
    let _ = std::fs::remove_file(&output);

    let result = client.call(
        "replace_text",
        json!({"path": input, "output": output, "search": "ALPHA", "replacement": "ALPHA-2"}),
    );
    let report = json_block(&result, 0);
    assert_eq!(report["occurrence"], 1, "{report}");
    let edited = std::fs::read(&output).unwrap();
    assert!(edited.starts_with(b"PK"));
    let text = client.call("extract_text", json!({"path": output}));
    let flat = text_block(&text, 0).replace('\n', "");
    assert!(flat.contains("ALPHA-2") && flat.contains("BETA"), "{text}");
    assert_eq!(std::fs::read(&input).unwrap(), original);

    let again = client.call(
        "replace_text",
        json!({"path": input, "output": output, "search": "BETA", "replacement": "B"}),
    );
    assert!(!error_text(&again).is_empty());
    assert_eq!(std::fs::read(&output).unwrap(), edited);
    let _ = std::fs::remove_file(&output);

    let onto_input = client.call(
        "replace_text",
        json!({"path": input, "output": input, "search": "BETA", "replacement": "B"}),
    );
    assert!(!error_text(&onto_input).is_empty());
    assert_eq!(std::fs::read(&input).unwrap(), original);

    let pdf = temp("mcp-replace.pdf");
    let _ = std::fs::remove_file(&pdf);
    let wrong_extension = client.call(
        "replace_text",
        json!({"path": input, "output": pdf, "search": "BETA", "replacement": "B"}),
    );
    assert!(error_text(&wrong_extension).contains(".hwpx"));
    assert!(!pdf.exists());
}

#[test]
fn asks_which_occurrence_instead_of_replacing_every_match() {
    let mut client = Client::start(None);
    let input = fixture("synthetic.hwpx");
    let output = temp("mcp-replace-ambiguous.hwpx");
    let _ = std::fs::remove_file(&output);

    let ambiguous = client.call(
        "replace_text",
        json!({"path": input, "output": output, "search": "A", "replacement": "a"}),
    );
    let message = error_text(&ambiguous);
    assert!(
        message.contains("3군데") && message.contains("occurrence"),
        "{message}"
    );
    assert!(message.contains("[A]"), "{message}");
    assert!(!output.exists());

    let missing = client.call(
        "replace_text",
        json!({"path": input, "output": output, "search": "없는글", "replacement": "a"}),
    );
    assert!(!error_text(&missing).is_empty());
    assert!(!output.exists());

    let chosen = client.call(
        "replace_text",
        json!({"path": input, "output": output, "search": "A", "replacement": "a", "occurrence": 3}),
    );
    assert_eq!(json_block(&chosen, 0)["occurrence"], 3);
    let text = client.call("extract_text", json!({"path": output}));
    let flat = text_block(&text, 0).replace('\n', "");
    assert!(flat.contains("BETa") && flat.contains("ALPHA"), "{text}");
    let _ = std::fs::remove_file(&output);
}

#[test]
fn returns_a_page_as_a_png_image() {
    let mut client = Client::start(None);
    let path = fixture("synthetic.hwpx");
    let result = client.call(
        "render_page_png",
        json!({"path": path, "page": 1, "dpi": 72}),
    );
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    let data = result["content"][0]["data"].as_str().unwrap();
    assert!(data.starts_with("iVBORw0KGgo"), "not base64 PNG");
    let meta = json_block(&result, 1);
    assert_eq!(meta["page"], 1);
    assert!(
        (500..700).contains(&meta["width"].as_u64().unwrap()),
        "{meta}"
    );

    let outside = client.call("render_page_png", json!({"path": path, "page": 9999}));
    assert!(!error_text(&outside).is_empty());
    let too_fine = client.call(
        "render_page_png",
        json!({"path": path, "page": 1, "dpi": 0}),
    );
    assert!(!error_text(&too_fine).is_empty());
}

#[test]
fn lists_fonts_with_and_without_a_document() {
    let mut client = Client::start(None);
    let report = json_block(&client.call("list_fonts", json!({})), 0);
    assert!(report["faces"].as_u64().unwrap() >= 1, "{report}");

    let result = client.call("list_fonts", json!({"document": fixture("synthetic.hwpx")}));
    let report = json_block(&result, 0);
    assert!(report["fonts"].is_object(), "{report}");
}

#[test]
fn a_missing_file_is_a_tool_error_and_the_server_keeps_going() {
    let mut client = Client::start(None);
    let missing = client.call(
        "read_document_info",
        json!({"path": temp("mcp-missing.hwpx")}),
    );
    assert!(!error_text(&missing).is_empty());
    let folder = client.call("read_document_info", json!({"path": temp("")}));
    assert!(!error_text(&folder).is_empty());
    let report = client.call("list_fonts", json!({}));
    assert_ne!(report["isError"], true, "{report}");
}
