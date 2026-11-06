//! 앱처럼 `ttoryeot_render`를 한 번 불러 최대 메모리와 시간을 JSON 한 줄로 찍는다.
//!
//! ffi_mem <문서> <글꼴 a.ttf:b.ttc> <mapped|read> [pdf 출력]
//! - `mapped`는 Swift `Data(contentsOf:options: .alwaysMapped)`처럼 글꼴 파일을 읽기 전용으로 매핑한다.
//! - `read`는 `Data(contentsOf:)`처럼 글꼴을 통째로 읽는다.
//! - `mb_caller`는 부르기 직전 phys_footprint, `mb_peak`는 프로세스 일생 최대 phys_footprint(MB)이다.
//! - `FFI_MEM_CALLS=n`이면 한 프로세스에서 n번 불러 앱이 문서를 여러 개 여는 것처럼 잰다.
//!   `mb_after`는 결과를 놓은 뒤 phys_footprint로, 부를수록 늘면 렌더 사이에 남는 메모리가 있다.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("ffi_mem needs macOS phys_footprint");
}

#[cfg(target_os = "macos")]
fn main() {
    mac::main();
}

#[cfg(target_os = "macos")]
mod mac {
    use std::time::Instant;
    use ttoryeot_ffi::{
        TtoryeotBytes, TtoryeotResult, ttoryeot_render, ttoryeot_result_free, ttoryeot_result_pdf,
    };

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

    enum Font {
        Mapped(*const u8, usize),
        Read(Vec<u8>),
    }

    impl Font {
        fn open(path: &std::path::Path, mapped: bool) -> Self {
            if !mapped {
                return Self::Read(std::fs::read(path).expect("font"));
            }
            let file = std::fs::File::open(path).expect("font");
            let len = file.metadata().expect("font").len() as usize;
            // SAFETY: a private read-only mapping of a whole regular file; kept until the process exits.
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ,
                    libc::MAP_PRIVATE,
                    std::os::fd::AsRawFd::as_raw_fd(&file),
                    0,
                )
            };
            assert_ne!(ptr, libc::MAP_FAILED, "mmap");
            Self::Mapped(ptr.cast(), len)
        }

        fn range(&self) -> TtoryeotBytes {
            match self {
                Self::Mapped(ptr, len) => TtoryeotBytes {
                    ptr: *ptr,
                    len: *len,
                },
                Self::Read(bytes) => TtoryeotBytes {
                    ptr: bytes.as_ptr(),
                    len: bytes.len(),
                },
            }
        }
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        let [_, document, fonts, mode, rest @ ..] = args.as_slice() else {
            eprintln!("usage: ffi_mem <document> <a.ttf:b.ttc> <mapped|read> [pdf out]");
            std::process::exit(2);
        };
        let mapped = match mode.as_str() {
            "mapped" => true,
            "read" => false,
            _ => panic!("mode must be mapped or read"),
        };
        let fonts: Vec<Font> = std::env::split_paths(fonts)
            .map(|path| Font::open(&path, mapped))
            .collect();
        let ranges: Vec<TtoryeotBytes> = fonts.iter().map(Font::range).collect();
        let document = std::fs::read(document).expect("document");
        let caller = footprint().0;

        let calls: usize =
            std::env::var("FFI_MEM_CALLS").map_or(1, |n| n.parse().expect("FFI_MEM_CALLS"));
        for call in 0..calls {
            let started = Instant::now();
            let mut result: *mut TtoryeotResult = std::ptr::null_mut();
            let status = unsafe {
                ttoryeot_render(
                    TtoryeotBytes {
                        ptr: document.as_ptr(),
                        len: document.len(),
                    },
                    TtoryeotBytes {
                        ptr: std::ptr::null(),
                        len: 0,
                    },
                    ranges.as_ptr(),
                    ranges.len(),
                    &mut result,
                )
            };
            assert_eq!(status, 0);
            let bytes = unsafe { ttoryeot_result_pdf(result) };
            assert!(bytes.len > 0, "render failed");
            let pdf = unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) }.to_vec();
            unsafe { ttoryeot_result_free(result) };
            let ms = (started.elapsed().as_secs_f64() * 1e6).round() / 1e3;
            let (after, peak) = footprint();
            if let Some(out) = rest.first() {
                std::fs::write(out, &pdf).expect("write pdf");
            }
            println!(
                "{{\"phase\":\"run\",\"call\":{call},\"mode\":\"{mode}\",\"pdf_bytes\":{},\"total\":{ms},\"mb_caller\":{caller},\"mb_after\":{after},\"mb_peak\":{peak}}}",
                pdf.len()
            );
        }
    }
}
