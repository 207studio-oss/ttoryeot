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
        Self::start_with(password, &temp("notes-unused"))
    }

    fn start_with(password: Option<&str>, notes: &Path) -> Self {
        Self::start_env(password, notes, None)
    }

    fn start_env(password: Option<&str>, notes: &Path, viewer: Option<&Path>) -> Self {
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/fonts");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ttoryeot-mcp"));
        command
            .arg("--font")
            .arg(fonts)
            .env_remove("TTORYEOT_PASSWORD")
            .env("TTORYEOT_NOTES_DIR", notes)
            .env("TTORYEOT_VIEWER", viewer.unwrap_or(Path::new("")))
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
fn lists_exactly_the_ten_tools() {
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
            "find_components",
            "list_annotations",
            "list_fonts",
            "open_in_viewer",
            "read_document_info",
            "render_page_png",
            "render_pdf",
            "replace_text",
            "resolve_annotation",
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

fn fresh_dir(name: &str) -> PathBuf {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn finds_components_by_text_and_by_point() {
    let mut client = Client::start(None);
    let path = fixture("synthetic.hwpx");
    let found = json_block(
        &client.call(
            "find_components",
            json!({"path": path, "page": 1, "contains": "BETA"}),
        ),
        0,
    );
    let components = found["components"].as_array().unwrap();
    let paragraph = components
        .iter()
        .find(|c| c["kind"] == "paragraph")
        .expect("BETA paragraph");
    assert_eq!(paragraph["text"], "BETA");
    let [x, y, w, h] = [0, 1, 2, 3].map(|i| paragraph["bounds"][i].as_f64().unwrap());
    let at = json_block(
        &client.call(
            "find_components",
            json!({"path": path, "page": 1, "x": x + w / 2.0, "y": y + h / 2.0}),
        ),
        0,
    );
    assert!(
        at["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["text"] == "BETA"),
        "{at}"
    );
    let missing = client.call("find_components", json!({"path": path, "page": 99}));
    assert!(error_text(&missing).contains("99쪽이 없습니다"));
    let half = client.call(
        "find_components",
        json!({"path": path, "page": 1, "x": 1.0}),
    );
    assert!(error_text(&half).contains("함께"));
}

#[test]
fn reads_and_resolves_annotations_left_in_the_desktop_app() {
    let notes_dir = fresh_dir("notes-annotations");
    let docs = fresh_dir("notes-docs");
    let document = docs.join("synthetic.hwpx");
    std::fs::copy(fixture("synthetic.hwpx"), &document).unwrap();
    let bytes = std::fs::read(&document).unwrap();

    let mut client = Client::start_with(None, &notes_dir);
    let empty = json_block(
        &client.call("list_annotations", json!({"path": document})),
        0,
    );
    assert_eq!(empty["open"], 0);
    assert_eq!(empty["notes"].as_array().unwrap().len(), 0);

    let found = json_block(
        &client.call(
            "find_components",
            json!({"path": document, "page": 1, "contains": "BETA"}),
        ),
        0,
    );
    let target = found["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "paragraph")
        .unwrap()
        .clone();
    let bounds = [0, 1, 2, 3].map(|i| target["bounds"][i].as_f64().unwrap() as f32);
    let mut book = ttoryeot::notes::NoteBook::new(&document);
    book.add(
        ttoryeot::notes::Draft {
            page: 1,
            area: false,
            bounds: ttoryeot::scene::Rect::new(bounds[0], bounds[1], bounds[2], bounds[3]),
            targets: vec![ttoryeot::notes::Target {
                kind: "paragraph".into(),
                name: "문단".into(),
                path: target["path"].as_str().unwrap().into(),
                text: "BETA".into(),
                bounds,
            }],
            comment: "BETA를 GAMMA로 바꿔 주세요".into(),
            replacement: Some("GAMMA".into()),
        },
        ttoryeot::notes::fingerprint(&bytes),
        1,
    );
    ttoryeot::notes::save(&notes_dir, &book).unwrap();

    let listed = client.call("list_annotations", json!({"path": document}));
    let report = json_block(&listed, 0);
    assert_eq!(report["open"], 1);
    let note = &report["notes"][0];
    assert_eq!(note["id"], 1);
    assert_eq!(note["comment"], "BETA를 GAMMA로 바꿔 주세요");
    assert_eq!(note["replacement"], "GAMMA");
    assert_eq!(note["targets"][0]["text"], "BETA");
    assert_eq!(note["document_changed"], false);
    assert_eq!(text_block(&listed, 1), "주석 1의 자리");
    assert_eq!(listed["content"][2]["type"], "image");
    let no_images = client.call(
        "list_annotations",
        json!({"path": document, "images": false}),
    );
    assert_eq!(no_images["content"].as_array().unwrap().len(), 1);

    let resolved = json_block(
        &client.call(
            "resolve_annotation",
            json!({"path": document, "id": 1, "resolution": "GAMMA로 바꿈"}),
        ),
        0,
    );
    assert_eq!(resolved["open"], 0);
    let open_only = json_block(
        &client.call("list_annotations", json!({"path": document})),
        0,
    );
    assert_eq!(open_only["notes"].as_array().unwrap().len(), 0);
    let all = json_block(
        &client.call(
            "list_annotations",
            json!({"path": document, "include_resolved": true, "images": false}),
        ),
        0,
    );
    assert_eq!(all["notes"][0]["resolved"], true);
    assert_eq!(all["notes"][0]["resolution"], "GAMMA로 바꿈");
    let on_disk = ttoryeot::notes::load(&notes_dir, &document).unwrap();
    assert!(on_disk.notes[0].resolved);

    std::fs::write(&document, b"changed").unwrap();
    let changed = json_block(
        &client.call(
            "list_annotations",
            json!({"path": document, "include_resolved": true}),
        ),
        0,
    );
    assert_eq!(changed["notes"][0]["document_changed"], true);
    assert!(changed["images_skipped"].is_string(), "{changed}");

    let unknown = client.call("resolve_annotation", json!({"path": document, "id": 7}));
    assert!(error_text(&unknown).contains("주석 7"));
}

#[test]
fn edits_the_document_in_place_and_keeps_a_backup() {
    let notes = fresh_dir("mcp-inplace-notes");
    let work = fresh_dir("mcp-inplace-doc");
    let path = work.join("가정통신문.hwpx");
    std::fs::copy(fixture("synthetic.hwpx"), &path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut client = Client::start_with(None, &notes);

    let both = client.call(
        "replace_text",
        json!({"path": path, "output": work.join("x.hwpx"), "in_place": true, "search": "ALPHA", "replacement": "A"}),
    );
    assert!(error_text(&both).contains("output"));
    let neither = client.call(
        "replace_text",
        json!({"path": path, "search": "ALPHA", "replacement": "A"}),
    );
    assert!(error_text(&neither).contains("in_place"));
    assert_eq!(std::fs::read(&path).unwrap(), original);

    let result = client.call(
        "replace_text",
        json!({"path": path, "in_place": true, "search": "ALPHA", "replacement": "ALPHA-2"}),
    );
    let report = json_block(&result, 0);
    let backup = PathBuf::from(report["backup"].as_str().expect("backup path"));
    assert!(
        backup.starts_with(notes.canonicalize().unwrap_or(notes.clone()))
            || backup.starts_with(&notes),
        "{report}"
    );
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    let text = client.call("extract_text", json!({"path": path}));
    assert!(
        text_block(&text, 0).replace('\n', "").contains("ALPHA-2"),
        "{text}"
    );
    let leftovers: Vec<_> = std::fs::read_dir(&work).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no temporary files are left behind");
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn hands_a_document_to_the_running_viewer() {
    let notes = fresh_dir("mcp-viewer-running");
    std::fs::write(
        notes.join("viewer.json"),
        json!({"seen": now(), "process": 1}).to_string(),
    )
    .unwrap();
    // 떠 있는 앱 대신 요청 폴더를 비우며 받은 요청을 모은다.
    let inbox = notes.join("open");
    let taker = std::thread::spawn(move || {
        for _ in 0..100 {
            // 다 쓴 요청만 받는다. 쓰는 중인 임시 파일(.json.tmp)은 건드리지 않는다.
            let ready = std::fs::read_dir(&inbox).ok().and_then(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| path.extension().is_some_and(|ext| ext == "json"))
            });
            if let Some(path) = ready {
                let request: Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                std::fs::remove_file(&path).unwrap();
                return Some(request);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        None
    });
    let mut client = Client::start_with(None, &notes);
    let document = fixture("synthetic.hwpx");
    let result = client.call("open_in_viewer", json!({"path": document, "page": 2}));
    let report = json_block(&result, 0);
    assert_eq!(report["viewer"], "running", "{report}");
    let request = taker.join().unwrap().expect("request");
    assert_eq!(
        request["path"],
        document.canonicalize().unwrap().display().to_string()
    );
    assert_eq!(request["page"], 2);

    let not_a_document = client.call("open_in_viewer", json!({"path": notes.join("viewer.json")}));
    assert!(error_text(&not_a_document).contains(".hwp"));
}

#[cfg(unix)]
#[test]
fn launches_the_viewer_when_it_is_not_running() {
    use std::os::unix::fs::PermissionsExt;
    let notes = fresh_dir("mcp-viewer-launch");
    // 앱 대신 뜨는 스크립트: 받은 문서를 적고, 떠 있다고 알린다.
    let viewer = notes.join("fake-viewer.sh");
    std::fs::write(
        &viewer,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$TTORYEOT_NOTES_DIR/launched.txt\"\nprintf '{\"seen\":%s,\"process\":2}' \"$(date +%s)\" > \"$TTORYEOT_NOTES_DIR/viewer.json\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&viewer, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut client = Client::start_env(None, &notes, Some(&viewer));
    let document = fixture("synthetic.hwpx");
    let result = client.call("open_in_viewer", json!({"path": document}));
    let report = json_block(&result, 0);
    assert_eq!(report["viewer"], "launched", "{report}");
    assert_eq!(
        std::fs::read_to_string(notes.join("launched.txt")).unwrap(),
        document.canonicalize().unwrap().display().to_string()
    );

    let missing = fresh_dir("mcp-viewer-missing");
    let mut client = Client::start_env(None, &missing, Some(&missing.join("no-such-viewer")));
    let result = client.call("open_in_viewer", json!({"path": document}));
    assert!(
        error_text(&result).contains("띄우지 못했습니다"),
        "{result}"
    );
}
