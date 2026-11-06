//! 성능 기준선: 저장소 안의 합성·공개 문서를 정해진 횟수만큼 파싱·배치·PDF 쓰기·래스터화해
//! 쪽/초와 최대 작업 집합을 잰다. 실제 말뭉치는 쓰지 않는다.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

#[derive(Serialize)]
struct Report {
    /// 작업 트리 기준 상대 경로. 저장소 밖 경로는 싣지 않는다.
    document: String,
    runs: usize,
    warmup: usize,
    /// 회차 평균 쪽 수. 회차마다 쪽 수가 다를 수 있어 소수점이 붙을 수 있다.
    pages_per_run: f64,
    total_pages: usize,
    seconds: f64,
    pages_per_second: f64,
    median_pages_per_second: f64,
    peak_memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct Measurement {
    pages: usize,
    seconds: f64,
}

#[derive(Debug, PartialEq)]
struct Summary {
    pages_per_run: f64,
    total_pages: usize,
    total_seconds: f64,
    pages_per_second: f64,
    median_pages_per_second: f64,
}

/// `runs`번(예열 `warmup`번 제외) 파싱부터 PDF 래스터화까지 돌려 결과를 JSON으로 찍는다.
pub fn run(
    document: &Path,
    font_paths: &str,
    runs: usize,
    warmup: usize,
    password: Option<&str>,
) -> Result<(), String> {
    if runs == 0 {
        return Err("--runs must be at least 1".to_string());
    }
    let current = std::env::current_dir().map_err(|e| format!("cwd: {}", e.kind()))?;
    let root = work_tree_root(&current).ok_or("bench must run inside a git work tree")?;
    let root = std::fs::canonicalize(root).map_err(|e| format!("work tree: {}", e.kind()))?;
    let (document_path, document) = resolve_document(&current, &root, document)?;

    let bytes = std::fs::read(&document_path).map_err(|e| format!("read: {}", e.kind()))?;
    let paths = std::env::split_paths(font_paths).collect::<Vec<_>>();
    let mut fonts = ttoryeot::fonts::FontBook::scan(&paths);
    if fonts.face_count() == 0 {
        return Err("fonts: no font faces found".to_string());
    }
    let parse_options = hwp_core::ParseOptions {
        password: password.map(str::to_string),
        ..hwp_core::ParseOptions::default()
    };

    for _ in 0..warmup {
        measure_once(&bytes, &parse_options, &mut fonts)?;
    }

    let mut measurements = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        let pages = measure_once(&bytes, &parse_options, &mut fonts)?;
        measurements.push(Measurement {
            pages,
            seconds: started.elapsed().as_secs_f64(),
        });
    }
    let summary = summarize(&measurements);

    let report = Report {
        document,
        runs,
        warmup,
        pages_per_run: summary.pages_per_run,
        total_pages: summary.total_pages,
        seconds: round_to(summary.total_seconds, 1_000_000.0),
        pages_per_second: round(summary.pages_per_second),
        median_pages_per_second: round(summary.median_pages_per_second),
        peak_memory_bytes: peak_memory_bytes(),
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}

/// 회차마다 잰 (쪽, 초)를 모아 전체와 회차별 비율을 낸다. 회차 쪽 수가 달라도 서로 맞는다.
fn summarize(measurements: &[Measurement]) -> Summary {
    let total_pages: usize = measurements.iter().map(|run| run.pages).sum();
    let total_seconds: f64 = measurements.iter().map(|run| run.seconds).sum();
    let mut per_run = measurements
        .iter()
        .map(|run| run.pages as f64 / run.seconds)
        .collect::<Vec<_>>();
    per_run.sort_by(f64::total_cmp);
    let median = if per_run.len() % 2 == 1 {
        per_run[per_run.len() / 2]
    } else {
        (per_run[per_run.len() / 2 - 1] + per_run[per_run.len() / 2]) / 2.0
    };
    Summary {
        pages_per_run: total_pages as f64 / measurements.len() as f64,
        total_pages,
        total_seconds,
        pages_per_second: total_pages as f64 / total_seconds,
        median_pages_per_second: median,
    }
}

/// `cwd`에서 위로 올라가며 처음 만난 `.git`이 있는 디렉터리(작업 트리 루트)를 찾는다.
fn work_tree_root(start: &Path) -> Option<&Path> {
    start
        .ancestors()
        .find(|directory| directory.join(".git").exists())
}

/// `current`에서 부른 상대 `document`를 절대 경로로 풀고, JSON에 실을 작업 트리 상대 이름을 낸다.
/// 읽을 때는 cwd와 무관한 절대 경로를 쓴다.
fn resolve_document(
    current: &Path,
    root: &Path,
    document: &Path,
) -> Result<(PathBuf, String), String> {
    let path = if document.is_absolute() {
        document.to_path_buf()
    } else {
        current.join(document)
    };
    let path = std::fs::canonicalize(path).map_err(|e| format!("document: {}", e.kind()))?;
    let label = document_label(root, &path)?;
    Ok((path, label))
}

/// 절대 문서 경로를 작업 트리 기준 상대 경로로 바꾼다. 저장소 밖 경로는 이름도 싣지 않고 거부한다.
fn document_label(root: &Path, document: &Path) -> Result<String, String> {
    let relative = document.strip_prefix(root).map_err(|_| {
        "--document must be inside the current git work tree; pass a repository document"
            .to_string()
    })?;
    let label = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if label.is_empty() {
        return Err("--document must name a file, not the work tree root".to_string());
    }
    Ok(label)
}

/// 한 번의 채점 후보 쪽 작업: 파싱, 배치, PDF 쓰기, PDF 래스터화. 쪽 수를 돌려준다.
fn measure_once(
    bytes: &[u8],
    options: &hwp_core::ParseOptions,
    fonts: &mut ttoryeot::fonts::FontBook,
) -> Result<usize, String> {
    let parsed =
        hwp_core::parse(bytes, options).map_err(|fatal| format!("parse:{}", fatal.kind()))?;
    let pages = ttoryeot::render(&parsed.document, fonts);
    let pdf = ttoryeot::pdf::write(pages, fonts).map_err(|_| "pdf:error".to_string())?;
    let scanned = crate::scan::scan(pdf)?;
    Ok(scanned.len())
}

fn round(value: f64) -> f64 {
    round_to(value, 100.0)
}

fn round_to(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

/// 프로세스가 시작한 뒤 지금까지 쓴 최고 실제 메모리(작업 집합). 글꼴 색인·예열·첫 렌더가
/// 모두 포함되며, 타이밍 구간만의 값이 아니다. 운영체제가 지원하지 않으면 `None`.
#[cfg(windows)]
fn peak_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: the struct is zeroed, its `cb` is set to its own size, and the call only writes it.
    let mut counters = unsafe { std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (ok != 0).then_some(counters.PeakWorkingSetSize as u64)
}

#[cfg(unix)]
fn peak_memory_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `getrusage` initializes the struct on success; we only read it when it returns 0.
    let ok = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if ok != 0 {
        return None;
    }
    // SAFETY: the successful call above initialized `usage`.
    let max_rss = u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).ok()?;
    // Linux reports kilobytes; macOS reports bytes.
    Some(if cfg!(any(target_os = "macos", target_os = "ios")) {
        max_rss
    } else {
        max_rss.saturating_mul(1024)
    })
}

#[cfg(not(any(windows, unix)))]
fn peak_memory_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ttoryeot-bench-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn measurement(pages: usize, seconds: f64) -> Measurement {
        Measurement { pages, seconds }
    }

    #[test]
    fn zero_runs_is_rejected_before_touching_the_document() {
        let error = run(Path::new("does-not-exist.hwpx"), "", 0, 0, None).unwrap_err();
        assert!(error.contains("--runs must be at least 1"), "{error}");
    }

    #[test]
    fn document_label_is_relative_to_the_work_tree() {
        let root = TempDir::new("label");
        fs::create_dir(root.path().join(".git")).unwrap();
        let document = root.path().join("hwp-core/tests/fixtures/synthetic.hwpx");
        fs::create_dir_all(document.parent().unwrap()).unwrap();
        fs::write(&document, b"fixture").unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        let document = fs::canonicalize(document).unwrap();
        assert_eq!(
            document_label(&canonical_root, &document).unwrap(),
            "hwp-core/tests/fixtures/synthetic.hwpx"
        );
    }

    #[test]
    fn document_outside_the_work_tree_is_rejected_without_its_path() {
        let root = TempDir::new("inside");
        let outside = TempDir::new("outside");
        let document = outside.path().join("private.hwpx");
        fs::write(&document, b"fixture").unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        let document = fs::canonicalize(document).unwrap();
        let error = document_label(&canonical_root, &document).unwrap_err();
        assert!(error.contains("--document must be inside"), "{error}");
        assert!(!error.contains("private.hwpx"), "{error}");
    }

    #[test]
    fn relative_document_resolves_from_the_invocation_directory() {
        // 하위 디렉터리에서 부른 `../...` 경로는 그 디렉터리 기준으로 풀려야 하고,
        // 읽을 때는 작업 트리 상대 이름이 아니라 절대 경로를 써야 한다.
        let root = TempDir::new("resolve");
        fs::create_dir(root.path().join(".git")).unwrap();
        let subdir = root.path().join("ttoryeot-score");
        fs::create_dir_all(&subdir).unwrap();
        let document = root.path().join("hwp-core/tests/fixtures/synthetic.hwpx");
        fs::create_dir_all(document.parent().unwrap()).unwrap();
        fs::write(&document, b"fixture").unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        let (path, label) = resolve_document(
            &subdir,
            &canonical_root,
            Path::new("../hwp-core/tests/fixtures/synthetic.hwpx"),
        )
        .unwrap();
        assert!(path.is_absolute(), "{}", path.display());
        assert_eq!(fs::read(&path).unwrap(), b"fixture");
        assert_eq!(label, "hwp-core/tests/fixtures/synthetic.hwpx");
    }

    #[test]
    fn median_of_an_odd_number_of_runs() {
        let summary = summarize(&[
            measurement(10, 1.0),
            measurement(20, 1.0),
            measurement(30, 1.0),
        ]);
        assert_eq!(summary.median_pages_per_second, 20.0);
    }

    #[test]
    fn median_of_an_even_number_of_runs() {
        let summary = summarize(&[measurement(10, 1.0), measurement(20, 1.0)]);
        assert_eq!(summary.median_pages_per_second, 15.0);
    }

    #[test]
    fn each_run_uses_its_own_page_count() {
        // 마지막 회차(4쪽)를 모든 회차에 쓰면 중앙값이 3.0, 전체가 2.667이 되어 틀린다.
        let summary = summarize(&[measurement(2, 1.0), measurement(4, 2.0)]);
        assert_eq!(summary.pages_per_run, 3.0);
        assert_eq!(summary.total_pages, 6);
        assert_eq!(summary.total_seconds, 3.0);
        assert_eq!(summary.pages_per_second, 2.0);
        assert_eq!(summary.median_pages_per_second, 2.0);
    }

    #[test]
    fn pages_per_run_keeps_the_fractional_average() {
        // 정수 나눗셈이면 5 / 2 = 2로 잘린다. 평균은 2.5여야 한다.
        let summary = summarize(&[measurement(2, 1.0), measurement(3, 1.0)]);
        assert_eq!(summary.pages_per_run, 2.5);
        assert_eq!(summary.total_pages, 5);
    }

    #[test]
    fn rounding_keeps_two_decimals_for_rates_and_six_for_seconds() {
        assert_eq!(round(920.216), 920.22);
        assert_eq!(round(0.005), 0.01);
        assert_eq!(round_to(1.195_371_4, 1_000_000.0), 1.195_371);
    }

    #[test]
    fn displayed_seconds_reproduce_the_reported_rate() {
        let summary = summarize(&[measurement(1_100, 1.195_371_4)]);
        let seconds = round_to(summary.total_seconds, 1_000_000.0);
        assert_eq!(round(summary.pages_per_second), round(1_100.0 / seconds));
    }
}
