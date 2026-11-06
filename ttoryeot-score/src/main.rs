//! 또렷 채점기. 골든셋 문서를 열어 보고, 한/글 기준 PDF와 비교한 수치만 쓴다.
//! 문서 이름·내용·그림은 어디에도 쓰지 않는다(`need-refs`가 쓰는 목록 파일만 예외로 상대경로를 적는다).
//! `bench`는 저장소 안 문서의 작업 트리 상대 경로만 JSON에 적고, 저장소 밖 문서는 거부한다.

mod bench;
mod golden;
mod metric;
mod opens;
mod scan;
mod score;
mod structure;

use golden::Kind;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
ttoryeot-score opens --documents <dir> --out <json> [--password-env <VAR>]
ttoryeot-score need-refs --documents <dir> --manifest <lists> --root <dir> --out <txt> [--scores <score json>]
ttoryeot-score score --documents <dir> --manifest <lists> --candidate <self|blank|ttoryeot> --out <json>
    [--fonts <font dirs or files, path-list separated>] [--source-fonts <manifest source text>=<paths>]
    [--password-env <VAR>] [--drop glyphs,fill,stroke,image]
    [--detail <document indexes, comma separated, or all>] [--dump-structure <dir outside repo>]
ttoryeot-score text-baseline --documents <dir> --manifest <lists> --out <json>
ttoryeot-score bench --document <hwp/hwpx> --fonts <font dirs or files> [--runs <n>] [--warmup <n>]
    [--password-env <env>]
<lists>: path-list separated [<set name>=]<manifest .json or tab-separated list>; later lists replace earlier PDFs";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or("no command")?;
    let options = Options::parse(args)?;
    match command.as_str() {
        "opens" => opens_command(&options),
        "need-refs" => need_refs_command(&options),
        "score" => score_command(&options),
        "text-baseline" => text_baseline_command(&options),
        "bench" => bench_command(&options),
        other => Err(format!("unknown command {other}")),
    }
}

struct Options(BTreeMap<String, String>);

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut args = args.peekable();
        while let Some(flag) = args.next() {
            let name = flag
                .strip_prefix("--")
                .ok_or(format!("expected --flag, got {flag}"))?;
            let value = args.next().ok_or(format!("--{name} needs a value"))?;
            values.insert(name.to_string(), value);
        }
        Ok(Self(values))
    }

    fn path(&self, name: &str) -> Result<PathBuf, String> {
        self.0
            .get(name)
            .map(PathBuf::from)
            .ok_or(format!("--{name} is required"))
    }

    fn text(&self, name: &str) -> Result<String, String> {
        self.0
            .get(name)
            .cloned()
            .ok_or(format!("--{name} is required"))
    }

    fn number(&self, name: &str) -> Result<Option<usize>, String> {
        self.0
            .get(name)
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("--{name} needs a number"))
            })
            .transpose()
    }

    /// 암호는 인자로 받지 않는다(셸 기록에 남는다). 이름을 받은 환경 변수에서 읽는다.
    fn password(&self) -> Result<Option<String>, String> {
        match self.0.get("password-env") {
            None => Ok(None),
            Some(variable) => std::env::var(variable)
                .map(Some)
                .map_err(|_| format!("environment variable {variable} is not set")),
        }
    }
}

fn opens_command(options: &Options) -> Result<(), String> {
    let documents = golden::documents(&options.path("documents")?).map_err(|e| e.to_string())?;
    let password = options.password()?;
    let mut report = opens::Report::default();
    for path in &documents {
        let kind = Kind::of(path).ok_or("not hwp/hwpx")?;
        report.add(&opens::open(path, kind, password.as_deref()));
    }
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    write_outside_repo(&options.path("out")?, json.as_bytes())?;
    println!("{json}");
    Ok(())
}

fn need_refs_command(options: &Options) -> Result<(), String> {
    let documents_dir = options.path("documents")?;
    let root = options.path("root")?;
    let documents = golden::documents(&documents_dir).map_err(|e| e.to_string())?;
    let references = references(options, &documents_dir)?;
    let referenced: BTreeSet<&PathBuf> = references
        .iter()
        .map(|reference| &reference.document)
        .collect();
    let relative_to_root = |path: &Path| -> Result<String, String> {
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| "--documents must be inside --root".to_string())?;
        Ok(relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"))
    };
    let mut needed: Vec<(Kind, String, String)> = Vec::new();
    for path in documents.iter().filter(|path| !referenced.contains(path)) {
        let kind = Kind::of(path).ok_or("not hwp/hwpx")?;
        let relative = relative_to_root(path)?;
        let opened = opens::open(path, kind, None);
        let note = match (&opened.protection, &opened.result) {
            (opens::Protection::Password, _) => {
                "\t# 암호 문서: 암호를 알아야 만들 수 있다".to_string()
            }
            (_, Err(fatal)) => format!("\t# hwp-core가 열지 못함: {}", fatal.kind()),
            _ => String::new(),
        };
        needed.push((kind, relative, note));
    }
    // 기준 PDF가 적은 HWPX를 먼저 적는다.
    needed.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let hwpx = needed.iter().filter(|item| item.0 == Kind::Hwpx).count();
    let mut text = format!(
        "# 한/글 기준 PDF가 없는 문서 {}개(HWPX {hwpx}, HWP {}).\n\
         # 경로는 {} 기준 상대경로다. 한/글에서 'PDF로 저장하기'로 만들어 기준 PDF 폴더에 두고 manifest에 더한다.\n\
         # HWPX를 먼저 적었다. 줄 끝 '#' 뒤는 설명이다.\n",
        needed.len(),
        needed.len() - hwpx,
        root.display(),
    );
    for (_, relative, note) in &needed {
        text.push_str(relative);
        text.push_str(note);
        text.push('\n');
    }
    if let Some(scores) = options.0.get("scores") {
        let scaled = scaled_references(Path::new(scores))?;
        text.push_str(
            "\n# 기준 PDF가 줄여 찍힌 것으로 보이는 문서(채점기가 첫 쪽 배율로 찾았다). \
             인쇄 배율 100%, 모아 찍기·메모 함께 인쇄 없이 'PDF로 저장하기'로 다시 만들어 기존 기준 PDF를 바꾼다.\n",
        );
        for (index, percent) in &scaled {
            let reference = references
                .get(*index)
                .ok_or("--scores does not match --manifest")?;
            text.push_str(&format!(
                "{}\t# 채점기 문서 {index}: 기준 PDF가 약 {percent}%로 줄여 찍힘\n",
                relative_to_root(&reference.document)?
            ));
        }
    }
    write_outside_repo(&options.path("out")?, text.as_bytes())?;
    println!(
        "need-refs: {} documents (hwpx {hwpx}, hwp {})",
        needed.len(),
        needed.len() - hwpx
    );
    Ok(())
}

/// `score`가 쓴 JSON에서 기준 PDF가 줄여 찍힌 것으로 보이는 문서: (manifest 순번, 배율 %).
fn scaled_references(scores: &Path) -> Result<Vec<(usize, u32)>, String> {
    #[derive(serde::Deserialize)]
    struct Scores {
        documents: Vec<Scored>,
    }
    #[derive(serde::Deserialize)]
    struct Scored {
        index: usize,
        print_scale_percent: u32,
        reference_scaled: bool,
    }
    let json = std::fs::read(scores).map_err(|e| format!("--scores: {e}"))?;
    let scores: Scores = serde_json::from_slice(&json).map_err(|e| format!("--scores: {e}"))?;
    Ok(scores
        .documents
        .into_iter()
        .filter(|doc| doc.reference_scaled)
        .map(|doc| (doc.index, doc.print_scale_percent))
        .collect())
}

#[derive(serde::Serialize)]
struct Settings {
    dpi: f32,
    ink_below: u8,
    ink_radius_px: usize,
    text_normalization: &'static str,
    candidate: String,
}

#[derive(serde::Serialize)]
struct ScoreReport {
    settings: Settings,
    /// 후보를 만들지 못한 문서 수(단계:종류별). 그 문서는 빈 쪽으로 채점했다.
    failures: BTreeMap<String, usize>,
    groups: BTreeMap<String, score::Group>,
    documents: Vec<score::DocRow>,
}

/// `--manifest`의 목록들을 합치고 목록마다 더한 수와 앞 목록의 기준 PDF를 바꾼 수만 찍는다.
fn references(options: &Options, documents_dir: &Path) -> Result<Vec<golden::Reference>, String> {
    let (references, counts) = golden::references(&options.text("manifest")?, documents_dir)?;
    for count in &counts {
        println!(
            "references {}: added {}, replaced earlier pdf {}",
            count.set, count.added, count.replaced
        );
    }
    Ok(references)
}

fn score_command(options: &Options) -> Result<(), String> {
    let references = references(options, &options.path("documents")?)?;
    let candidate = options.text("candidate")?;
    let password = options.password()?;
    // 기준 PDF를 만든 컴퓨터의 글꼴이 다르면 결과도 다르다(없는 글꼴은 한/글이 대신한다).
    // 그래서 manifest의 출처 글에 이 글이 든 기준과 견줄 때는 --source-fonts의 글꼴만 쓴다.
    let mut books: Vec<(Option<String>, ttoryeot::fonts::FontBook)> = match candidate.as_str() {
        "ttoryeot" => {
            let mut books = vec![(None, font_book(&options.text("fonts")?, "fonts")?)];
            if let Some(spec) = options.0.get("source-fonts") {
                let (source, paths) = spec
                    .split_once('=')
                    .ok_or("--source-fonts needs <source text>=<paths>")?;
                books.push((Some(source.to_string()), font_book(paths, "source-fonts")?));
            }
            books
        }
        "self" | "blank" => Vec::new(),
        other => return Err(format!("unknown candidate {other}")),
    };
    // 진단용: 또렷이 그린 것 중 이 종류(glyphs, fill, stroke, image)를 빼고 채점한다.
    let drop: BTreeSet<String> = options
        .0
        .get("drop")
        .map(|list| list.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let detail: BTreeSet<usize> = match options.0.get("detail").map(String::as_str) {
        Some("all") => (0..references.len()).collect(),
        Some(list) => list
            .split(',')
            .map(|index| index.parse().map_err(|_| "--detail needs numbers or all"))
            .collect::<Result<_, _>>()?,
        None => BTreeSet::new(),
    };
    // 패닉 메시지에는 문서 글이 섞일 수 있다. 위치만 남긴다.
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|at| format!("{}:{}", at.file(), at.line()));
        eprintln!("panic at {}", location.unwrap_or_default());
    }));
    let mut failures: BTreeMap<String, usize> = BTreeMap::new();
    let mut scores = Vec::new();
    let mut sizes: BTreeMap<(&str, usize, usize), usize> = BTreeMap::new();
    let mut gaps = BTreeMap::new();
    let mut fonts_used = FontsUsed::default();
    for (index, reference) in references.iter().enumerate() {
        if detail.contains(&index) {
            println!("doc {index}");
        }
        let expected = read_pdf(&reference.pdf)?;
        let blank = || {
            expected
                .iter()
                .map(|page| scan::Page::blank(page.width, page.height))
                .collect()
        };
        let book = books
            .iter()
            .position(|(source, _)| {
                source
                    .as_ref()
                    .is_some_and(|text| reference.source.contains(text.as_str()))
            })
            .or((!books.is_empty()).then_some(0));
        let actual = match (candidate.as_str(), book) {
            ("self", _) => read_pdf(&reference.pdf)?,
            (_, Some(book)) => {
                match render(
                    &reference.document,
                    password.as_deref(),
                    &mut books[book].1,
                    &drop,
                    detail.contains(&index),
                    &mut fonts_used,
                ) {
                    Ok(pages) => pages,
                    Err(kind) => {
                        *failures.entry(kind).or_default() += 1;
                        blank()
                    }
                }
            }
            _ => blank(),
        };
        if detail.contains(&index) {
            for (side, pages) in [("reference", &expected), ("candidate", &actual)] {
                for (number, page) in pages.iter().enumerate() {
                    println!("  {side} page {number}: {}", page_profile(page));
                }
            }
        }
        for (side, pages) in [("reference", &expected), ("candidate", &actual)] {
            for page in pages {
                *sizes.entry((side, page.width, page.height)).or_default() += 1;
            }
        }
        let score = score::score(
            reference.kind,
            &reference.source,
            &reference.set,
            &expected,
            &actual,
        );
        if let Some(dir) = options.0.get("dump-structure") {
            fn side(pages: &[scan::Page]) -> Vec<&structure::Structure> {
                pages.iter().map(|page| &page.structure).collect()
            }
            let text = structure::report(&side(&expected), &side(&actual));
            write_outside_repo(
                &Path::new(dir).join(format!("{index}.txt")),
                text.as_bytes(),
            )?;
        }
        add_gaps(&mut gaps, &score.text_gaps);
        scores.push(score);
    }
    let report = ScoreReport {
        settings: Settings {
            dpi: scan::DPI,
            ink_below: metric::INK_BELOW,
            ink_radius_px: metric::INK_RADIUS,
            text_normalization: "NFKC, drop whitespace/control/U+FFFD, character multiset",
            candidate,
        },
        failures,
        groups: score::groups(&scores),
        documents: scores
            .iter()
            .enumerate()
            .map(|(index, score)| score::DocRow::new(index, score))
            .collect(),
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    write_outside_repo(&options.path("out")?, json.as_bytes())?;
    println!("failures {:?}", report.failures);
    println!("page sizes (side, width, height): count {sizes:?}");
    println!("text gaps by class [missing, extra]: {gaps:?}");
    fonts_used.print();
    println!("group                  docs  diff% blank% beats   pixel  ink_f1 text_f1 pages");
    for (key, group) in &report.groups {
        if key.starts_with("source:") {
            continue;
        }
        println!(
            "{key:<22} {:>4} {:>6.3} {:>6.3} {:>5.2} {:>7.3} {:>6.3} {:>7.3} {:>5.2}",
            group.documents,
            group.pixel_difference_percent,
            group.blank_pixel_difference_percent,
            group.beats_blank,
            group.pixel_score,
            group.ink_f1,
            group.text_f1,
            group.page_count_match,
        );
    }
    println!(
        "doc  fmt  set      pages  diff% blank%  ink_f1 text_f1 ink_ratio shift     zero  best  scale"
    );
    for row in &report.documents {
        println!(
            "{:>3} {:<5} {:<8} {:>2}/{:<2} {:>6.3} {:>6.3} {:>6.3} {:>7.3} {:>9.3} {:>4},{:<4} {:>5.3} {:>5.3} {:>3}%{}",
            row.index,
            row.format,
            row.set,
            row.reference_pages,
            row.candidate_pages,
            row.pixel_difference_percent,
            row.blank_pixel_difference_percent,
            row.ink_f1,
            row.text_f1,
            row.ink_ratio,
            row.best_shift.0,
            row.best_shift.1,
            row.zero_shift_share,
            row.best_shift_share,
            row.print_scale_percent,
            if row.reference_scaled { " scaled" } else { "" },
        );
    }
    println!(
        "structure (tolerance {} pt): doc lines ref/cand  break  page  y  x  rules  images  total  max dy dx rule",
        structure::PLACE_TOLERANCE
    );
    for row in &report.documents {
        let s = &row.structure;
        println!(
            "{:>3} {:>5}/{:<5} {:>5} {:>5} {:>4} {:>3} {:>5} {:>6} {:>6}  {:>5.2} {:>5.2} {:>5.2}",
            row.index,
            s.reference_lines,
            s.candidate_lines,
            s.line_break,
            s.page_break,
            s.line_y,
            s.line_x,
            s.rules,
            s.images,
            row.structure_mismatches,
            s.max_line_dy,
            s.max_line_dx,
            s.max_rule_shift,
        );
    }
    Ok(())
}

/// 또렷으로 문서를 PDF로 그려 메모리에서 바로 읽는다(그린 PDF는 디스크에 쓰지 않는다).
/// 실패하면 단계와 종류만 돌려준다.
fn render(
    document: &Path,
    password: Option<&str>,
    fonts: &mut ttoryeot::fonts::FontBook,
    drop: &BTreeSet<String>,
    detail: bool,
    fonts_used: &mut FontsUsed,
) -> Result<Vec<scan::Page>, String> {
    let bytes = std::fs::read(document).map_err(|e| format!("read:{:?}", e.kind()))?;
    let options = hwp_core::ParseOptions {
        password: password.map(str::to_string),
        ..hwp_core::ParseOptions::default()
    };
    let parsed =
        hwp_core::parse(&bytes, &options).map_err(|fatal| format!("parse:{}", fatal.kind()))?;
    if detail {
        for section in &parsed.document.sections {
            println!(
                "  section page {:?} columns {}",
                section.page, section.columns
            );
            for columns in section
                .paragraphs
                .iter()
                .filter_map(|para| para.columns.as_ref())
            {
                println!("    column definition {columns:?}");
            }
        }
    }
    for face in parsed
        .document
        .font_faces
        .get("hangul")
        .into_iter()
        .flatten()
    {
        let resolved = ttoryeot::subst::Rules::builtin()
            .resolve(fonts, face, "hangul", false)
            .and_then(|(id, _)| fonts.file_name(id))
            .unwrap_or_default();
        if detail {
            println!("  hangul face {} -> {resolved}", face.name);
        }
        *fonts_used
            .hangul_faces
            .entry(format!("{} -> {resolved}", face.name))
            .or_default() += 1;
    }
    let pdf = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut pages = ttoryeot::render(&parsed.document, fonts);
        if detail {
            scene_detail(&pages);
            let mut runs: BTreeMap<(String, String, bool, String), usize> = BTreeMap::new();
            for item in pages.iter().flat_map(|page| &page.items) {
                if let ttoryeot::scene::Item::Glyphs(run) = item {
                    let key = (
                        fonts.file_name(run.face).unwrap_or_default(),
                        format!("{:.1}", run.size),
                        run.bold_stroke.is_some(),
                        format!("{:.2}", run.scale_x),
                    );
                    *runs.entry(key).or_default() += run.glyphs.len();
                }
            }
            println!("  glyphs by (file, size, synthetic bold, width ratio): {runs:?}");
        }
        for item in pages.iter().flat_map(|page| &page.items) {
            if let ttoryeot::scene::Item::Glyphs(run) = item {
                let name = fonts.file_name(run.face).unwrap_or_default();
                *fonts_used.glyphs.entry(name).or_default() += run.glyphs.len();
            }
        }
        for page in &mut pages {
            page.items.retain(|item| !drop.contains(item_kind(item)));
        }
        ttoryeot::pdf::write(pages, fonts)
    }))
    .map_err(|_| "render:panic".to_string())?
    .map_err(|_| "pdf:error".to_string())?;
    scan::scan(pdf).map_err(|_| "scan:error".to_string())
}

/// `--drop`에 쓰는 그릴 것 종류 이름. 잘라 그리기·변환은 짝이 맞아야 하므로 빼지 않는다.
fn item_kind(item: &ttoryeot::scene::Item) -> &'static str {
    use ttoryeot::scene::Item;
    match item {
        Item::Glyphs(_) => "glyphs",
        Item::Fill { .. } | Item::LinearFill { .. } => "fill",
        Item::Stroke { .. } => "stroke",
        Item::Image { .. } => "image",
        Item::Clip(_) | Item::Transform(_) | Item::Pop => "state",
    }
}

#[derive(serde::Serialize, Default)]
struct TextGroup {
    documents: usize,
    text_f1: f64,
    text_recall: f64,
    text_precision: f64,
}

/// 렌더러 없이 hwp-core 모델 글과 기준 PDF 글을 비교한다. 글자 추출기를 검증하는 기준선이다.
fn text_baseline_command(options: &Options) -> Result<(), String> {
    let references = references(options, &options.path("documents")?)?;
    let mut groups: BTreeMap<String, Vec<metric::Overlap>> = BTreeMap::new();
    let mut gaps = BTreeMap::new();
    for reference in &references {
        let pdf_text = joined_text(&read_pdf(&reference.pdf)?);
        let bytes =
            std::fs::read(&reference.document).map_err(|e| format!("read: {}", e.kind()))?;
        let model_text = match hwp_core::parse(&bytes, &hwp_core::ParseOptions::default()) {
            Ok(parsed) => hwp_core::text::text_summary(&parsed.document)
                .paragraphs
                .concat(),
            Err(_) => String::new(),
        };
        let overlap = metric::text_overlap(&pdf_text, &model_text);
        add_gaps(&mut gaps, &metric::text_gaps(&pdf_text, &model_text));
        for key in [
            "all".to_string(),
            format!("format:{}", reference.kind.label()),
            format!("source:{}", reference.source),
            format!("set:{}", reference.set),
        ] {
            groups.entry(key).or_default().push(overlap);
        }
    }
    let summary: BTreeMap<String, TextGroup> = groups
        .into_iter()
        .map(|(key, overlaps)| {
            let n = overlaps.len() as f64;
            let average = |value: fn(metric::Overlap) -> f64| {
                (overlaps.iter().map(|overlap| value(*overlap)).sum::<f64>() / n * 10_000.0).round()
                    / 10_000.0
            };
            let group = TextGroup {
                documents: overlaps.len(),
                text_f1: average(metric::Overlap::f1),
                text_recall: average(metric::Overlap::recall),
                text_precision: average(metric::Overlap::precision),
            };
            (key, group)
        })
        .collect();
    let json = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    write_outside_repo(&options.path("out")?, json.as_bytes())?;
    println!("{json}");
    println!("text gaps by class [missing, extra]: {gaps:?}");
    Ok(())
}

fn bench_command(options: &Options) -> Result<(), String> {
    let runs = options.number("runs")?.unwrap_or(20);
    let warmup = options.number("warmup")?.unwrap_or(3);
    let password = options.password()?;
    bench::run(
        &options.path("document")?,
        &options.text("fonts")?,
        runs,
        warmup,
        password.as_deref(),
    )
}

fn joined_text(pages: &[scan::Page]) -> String {
    pages.iter().map(|page| page.text.as_str()).collect()
}

/// 진단용: 글 차이를 글자 종류별 [빠짐, 남음] 개수로만 모은다(글자 자체는 남기지 않는다).
fn add_gaps(
    total: &mut BTreeMap<&'static str, [usize; 2]>,
    gaps: &BTreeMap<&'static str, [usize; 2]>,
) {
    for (class, [missing, extra]) in gaps {
        let sum = total.entry(*class).or_default();
        sum[0] += missing;
        sum[1] += extra;
    }
}

fn read_pdf(path: &Path) -> Result<Vec<scan::Page>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read pdf: {}", e.kind()))?;
    scan::scan(bytes)
}

/// 결과를 git 작업 트리 밖에만 쓴다. 실제 문서에서 나온 결과가 커밋되지 않게 한다.
fn write_outside_repo(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| format!("out dir: {e}"))?;
    let parent = parent.canonicalize().map_err(|e| format!("out dir: {e}"))?;
    if parent.ancestors().any(|dir| dir.join(".git").exists()) {
        return Err("refusing to write inside a git work tree".to_string());
    }
    std::fs::write(path, bytes).map_err(|e| format!("write: {e}"))
}

/// `--detail <문서 번호>` 진단: 그 문서의 쪽마다 그릴 것의 수와 크기만 적는다(글·그림은 적지 않는다).
fn scene_detail(pages: &[ttoryeot::scene::Page]) {
    use ttoryeot::scene::{Item, PathOp};
    for (number, page) in pages.iter().enumerate() {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let (mut min_size, mut max_size) = (f32::MAX, 0.0f32);
        let mut largest_fill = (0.0f32, 0u32);
        let mut image_area = 0.0f32;
        let mut outside = 0usize;
        let mut stroke_widths: BTreeMap<String, usize> = BTreeMap::new();
        let mut stroke_paths: BTreeMap<String, usize> = BTreeMap::new();
        let mut long_dashed = Vec::new();
        for item in &page.items {
            *counts.entry(item_kind(item)).or_default() += 1;
            match item {
                Item::Stroke { path, stroke } => {
                    let dashed = if stroke.dash.is_empty() {
                        String::new()
                    } else {
                        format!(" dash {:.2?}", stroke.dash)
                    };
                    *stroke_widths
                        .entry(format!(
                            "{:.2}pt {:06x}{dashed}",
                            stroke.width, stroke.color
                        ))
                        .or_default() += 1;
                    *stroke_paths
                        .entry(format!("{path:.1?} {:.2}", stroke.width))
                        .or_default() += 1;
                    if let [PathOp::Move(x0, y0), PathOp::Line(x1, y1)] = path.as_slice()
                        && x0 == x1
                        && (y1 - y0).abs() > 20.0
                        && !stroke.dash.is_empty()
                    {
                        long_dashed.push(format!(
                            "x {x0:.2} {y0:.2}..{y1:.2} {:.2}pt{dashed}",
                            stroke.width
                        ));
                    }
                }
                Item::Glyphs(run) => {
                    min_size = min_size.min(run.size);
                    max_size = max_size.max(run.size);
                    let (x, y) = run.origin;
                    if x < 0.0 || y < 0.0 || x > page.width || y > page.height {
                        outside += 1;
                    }
                }
                Item::Fill { path, color, .. } => {
                    let points = path.iter().filter_map(|op| match op {
                        PathOp::Move(x, y) | PathOp::Line(x, y) => Some((*x, *y)),
                        _ => None,
                    });
                    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                    for (x, y) in points {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                    }
                    let area = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
                    if area > largest_fill.0 {
                        largest_fill = (area, *color);
                    }
                }
                Item::Image { rect, .. } => image_area += rect.width * rect.height,
                _ => {}
            }
        }
        let page_area = page.width * page.height;
        println!(
            "  page {number}: {:.0}x{:.0} {counts:?} glyph size {:.1}..{:.1} outside {outside} \
             largest fill {:.3} of page color {:06x} images {:.3} of page",
            page.width,
            page.height,
            if max_size > 0.0 { min_size } else { 0.0 },
            max_size,
            largest_fill.0 / page_area,
            largest_fill.1,
            image_area / page_area,
        );
        if !long_dashed.is_empty() {
            println!("    long dashed vertical lines {long_dashed:?}");
        }
        if !stroke_widths.is_empty() {
            let repeated: usize = stroke_paths.values().map(|count| count - 1).sum();
            println!(
                "    strokes by (width, color): {:?}; drawn again on the same path {repeated}",
                &ranked(&stroke_widths)[..stroke_widths.len().min(8)]
            );
        }
    }
}

fn font_book(paths: &str, flag: &str) -> Result<ttoryeot::fonts::FontBook, String> {
    let book = ttoryeot::fonts::FontBook::scan(&std::env::split_paths(paths).collect::<Vec<_>>());
    if book.face_count() == 0 {
        return Err(format!("--{flag}: no font faces found"));
    }
    Ok(book)
}

/// 진단용 글꼴 집계: 문서가 적은 한글 글꼴 이름이 어느 파일로 풀렸는지(문서 수)와 파일별로 그린 글자 수.
#[derive(Default)]
struct FontsUsed {
    hangul_faces: BTreeMap<String, usize>,
    glyphs: BTreeMap<String, usize>,
}

impl FontsUsed {
    fn print(&self) {
        println!("glyphs by font file: {:?}", ranked(&self.glyphs));
        println!(
            "hangul faces (name -> file): documents {:?}",
            ranked(&self.hangul_faces)
        );
    }
}

fn ranked(counts: &BTreeMap<String, usize>) -> Vec<(&String, &usize)> {
    let mut ranked: Vec<_> = counts.iter().collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(*entry.1));
    ranked
}

/// `--detail` 진단: 래스터 한 쪽의 잉크 양·범위·줄 띠(잉크가 있는 가로줄이 이어진 묶음) 수와 높이·잉크 평균 밝기.
fn page_profile(page: &scan::Page) -> String {
    let inked = |x: usize, y: usize| page.gray[y * page.width + x] < metric::INK_BELOW;
    let (mut count, mut darkness) = (0usize, 0u64);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    let mut bands: Vec<usize> = Vec::new();
    let mut run = 0;
    for y in 0..page.height {
        let mut row = false;
        for x in 0..page.width {
            if inked(x, y) {
                row = true;
                count += 1;
                darkness += u64::from(page.gray[y * page.width + x]);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
        if row {
            run += 1;
        } else if run > 0 {
            bands.push(run);
            run = 0;
        }
    }
    if run > 0 {
        bands.push(run);
    }
    bands.sort_unstable();
    format!(
        "{}x{} glyphs {} characters {} ink {:.3} mean gray {} box {:?} bands {} median band {}",
        page.width,
        page.height,
        page.glyphs,
        metric::normalized(&page.text).count(),
        count as f64 / page.gray.len().max(1) as f64,
        darkness / count.max(1) as u64,
        (count > 0).then_some((x0, y0, x1, y1)),
        bands.len(),
        bands.get(bands.len() / 2).copied().unwrap_or(0),
    )
}
