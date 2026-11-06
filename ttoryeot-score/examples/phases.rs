//! 문서 하나를 여러 번 읽어 단계별 시간(ms)과 메모리를 JSON 한 줄씩 찍는다.
//!
//! phases <문서> <글꼴> <회차> [pdf 출력]
//! - 글꼴이 `files:a.ttf:b.ttc`처럼 `files:`로 시작하면 FFI `ttoryeot_render`처럼 회차마다 새
//!   `FontBook`에 그 파일들만 `add_bytes`로 넣는다(읽기 시간 포함). 아니면 폴더들을 한 번 색인한다.
//! - 메모리는 phys_footprint(iOS가 앱을 끊는 기준)를 단계마다 잰다.
//! - `PHASES_FACES=1`이면 마지막 회차에서 글자에 쓴 글꼴 파일 이름과 그림 그리기 수(서로 다른 그림 수)를 찍는다.

use std::time::Instant;

use ttoryeot::fonts::FontBook;

fn ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1e6).round() / 1e3
}

/// (지금 phys_footprint, 프로세스 일생 최대 phys_footprint) MB. macOS 밖에서는 -1이다.
#[cfg(not(target_os = "macos"))]
fn footprint() -> (f64, f64) {
    (-1.0, -1.0)
}

/// (지금 phys_footprint, 프로세스 일생 최대 phys_footprint) MB.
#[cfg(target_os = "macos")]
fn footprint() -> (f64, f64) {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: the kernel fills the v4 struct for our own pid; we read it only on success.
    let ok = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    if ok != 0 {
        return (-1.0, -1.0);
    }
    let info = unsafe { info.assume_init() };
    let mb = |bytes: u64| (bytes as f64 / 1e5).round() / 10.0;
    (
        mb(info.ri_phys_footprint),
        mb(info.ri_lifetime_max_phys_footprint),
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, document, font_spec, runs, rest @ ..] = args.as_slice() else {
        eprintln!("usage: phases <document> <font dirs | files:a:b> <runs> [pdf out]");
        std::process::exit(2);
    };
    let runs: usize = runs.parse().expect("runs");
    let files = font_spec
        .strip_prefix("files:")
        .map(|list| std::env::split_paths(list).collect::<Vec<_>>());

    let mut shared = None;
    if files.is_none() {
        let started = Instant::now();
        let paths = std::env::split_paths(font_spec).collect::<Vec<_>>();
        let book = FontBook::scan(&paths);
        println!(
            "{{\"phase\":\"font_scan\",\"ms\":{},\"faces\":{},\"footprint_mb\":{}}}",
            ms(started),
            book.face_count(),
            footprint().0
        );
        shared = Some(book);
    }

    for run in 0..runs {
        let total = Instant::now();
        let started = Instant::now();
        let mut own = None;
        if let Some(files) = &files {
            let mut book = FontBook::default();
            for path in files {
                book.add_bytes(path.clone(), std::fs::read(path).expect("font"));
            }
            own = Some(book);
        }
        let fonts = own.as_mut().or(shared.as_mut()).expect("fonts");
        let font_ms = ms(started);
        let after_fonts = footprint().0;

        let started = Instant::now();
        let bytes = std::fs::read(document).expect("read");
        let parsed = hwp_core::parse(&bytes, &hwp_core::ParseOptions::default()).expect("parse");
        let parse = ms(started);
        let after_parse = footprint().0;

        let started = Instant::now();
        let pages = ttoryeot::render(&parsed.document, fonts);
        let layout = ms(started);
        let after_layout = footprint().0;

        let faces = (run + 1 == runs && std::env::var_os("PHASES_FACES").is_some()).then(|| {
            let mut faces = std::collections::BTreeSet::new();
            let mut images = std::collections::HashSet::new();
            let mut draws = 0;
            for page in &pages {
                for item in &page.items {
                    match item {
                        ttoryeot::scene::Item::Glyphs(run) => {
                            if let Some(name) = fonts.file_name(run.face) {
                                faces.insert(name);
                            }
                        }
                        ttoryeot::scene::Item::Image { data, .. } => {
                            draws += 1;
                            images.insert(std::sync::Arc::as_ptr(data));
                        }
                        _ => {}
                    }
                }
            }
            format!(
                "{{\"phase\":\"faces\",\"files\":[{}],\"image_draws\":{draws},\"images\":{}}}",
                faces
                    .iter()
                    .map(|name| format!("{name:?}"))
                    .collect::<Vec<_>>()
                    .join(","),
                images.len()
            )
        });
        let page_count = pages.len();

        let started = Instant::now();
        let pdf = ttoryeot::pdf::write(pages, fonts).expect("pdf");
        let pdf_ms = ms(started);
        let (after_pdf, peak) = footprint();

        if run == 0
            && let Some(out) = rest.first()
        {
            std::fs::write(out, &pdf).expect("write pdf");
        }
        if let Some(line) = faces {
            println!("{line}");
        }
        println!(
            "{{\"phase\":\"run\",\"run\":{run},\"pages\":{},\"pdf_bytes\":{},\"fonts\":{font_ms},\"parse\":{parse},\"layout\":{layout},\"pdf\":{pdf_ms},\"total\":{},\"mb_fonts\":{after_fonts},\"mb_parse\":{after_parse},\"mb_layout\":{after_layout},\"mb_pdf\":{after_pdf},\"mb_peak\":{peak}}}",
            page_count,
            pdf.len(),
            ms(total)
        );
    }
}
