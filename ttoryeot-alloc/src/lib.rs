//! macOS에서 큰 블록을 운영체제에 바로 돌려주는 전역 할당자.
//!
//! macOS 시스템 할당자는 놓인 큰 블록을 캐시에 쥐고 있어, 살아 있는 바이트가 그대로여도
//! 문서를 거듭 열면 phys_footprint가 늘어난다. `malloc_zone_pressure_relief`로도 돌아오지
//! 않고, 앱에 묶이는 라이브러리는 `MallocLargeCache` 환경 변수를 정할 수 없다.
//! 그래서 큰 블록만 할당자를 거치지 않고 직접 매핑한다.
//!
//! iOS 시뮬레이터의 시스템 할당자는 이렇게 쥐고 있지 않고 직접 매핑보다 최대 메모리도 낮아서,
//! macOS 밖에서는 이 타입이 `System`과 같다.
//!
//! 전역 할당자는 최종 산출물만 정할 수 있으므로 등록은 호출자가 한다.
//!
//! ```no_run
//! #[global_allocator]
//! static ALLOCATOR: ttoryeot_alloc::Allocator = ttoryeot_alloc::Allocator;
//! # fn main() {}
//! ```

#[cfg(not(target_os = "macos"))]
pub use std::alloc::System as Allocator;

#[cfg(target_os = "macos")]
pub use macos::Allocator;

#[cfg(target_os = "macos")]
mod macos {
    use std::alloc::{GlobalAlloc, Layout, System};

    /// 128 KiB·1 MiB와 견주어 고른 값이다. 더 낮추면 매핑 비용이 늘고 높이면 캐시에 남는 몫이 는다.
    const DIRECT_FROM: usize = 256 * 1024;
    /// Intel Mac까지 포함해 `mmap`이 보장하는 가장 작은 쪽 크기.
    const MAPPED_ALIGN: usize = 4096;

    pub struct Allocator;

    fn direct(layout: Layout) -> bool {
        layout.size() >= DIRECT_FROM && layout.align() <= MAPPED_ALIGN
    }

    fn map_near(hint: *mut u8, size: usize) -> *mut u8 {
        // SAFETY: MAP_FIXED 없는 익명 매핑이라 `hint`가 차 있으면 운영체제가 다른 주소를 고른다.
        let mapped = unsafe {
            libc::mmap(
                hint.cast(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if mapped == libc::MAP_FAILED {
            std::ptr::null_mut()
        } else {
            mapped.cast()
        }
    }

    fn map(size: usize) -> *mut u8 {
        map_near(std::ptr::null_mut(), size)
    }

    /// 복사 없이 크기를 바꾼다. 뒤쪽 주소가 차 있어 늘릴 수 없으면 false다.
    ///
    /// SAFETY: `ptr`은 `old_size` 길이의 직접 매핑이어야 한다.
    unsafe fn resize_in_place(ptr: *mut u8, old_size: usize, new_size: usize) -> bool {
        // SAFETY: sysconf에는 넘기는 메모리가 없다.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let old_end = old_size.next_multiple_of(page);
        let new_end = new_size.next_multiple_of(page);
        // SAFETY: 두 끝 모두 호출자가 보장한 매핑의 시작에서 잰 쪽 경계다.
        let (at_old, at_new) = unsafe { (ptr.add(old_end), ptr.add(new_end)) };
        if new_end <= old_end {
            if new_end < old_end {
                // SAFETY: 줄인 뒤쪽 쪽들은 이 블록만 쓰는 매핑이고 곧 layout에서도 빠진다.
                unsafe { libc::munmap(at_new.cast(), old_end - new_end) };
            }
            return true;
        }
        let extra = new_end - old_end;
        let mapped = map_near(at_old, extra);
        if mapped == at_old {
            return true;
        }
        if !mapped.is_null() {
            // SAFETY: 방금 받은 다른 주소의 매핑이라 아무도 쓰지 않는다.
            unsafe { libc::munmap(mapped.cast(), extra) };
        }
        false
    }

    // SAFETY: 큰 블록은 쪽 단위로 정렬된 새 매핑이고, `direct`는 Layout만 보므로 할당과 해제가
    // 언제나 같은 길로 간다. 나머지는 `System`에 그대로 맡긴다.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if direct(layout) {
                map(layout.size())
            } else {
                unsafe { System.alloc(layout) }
            }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if direct(layout) {
                map(layout.size())
            } else {
                unsafe { System.alloc_zeroed(layout) }
            }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if direct(layout) {
                unsafe { libc::munmap(ptr.cast(), layout.size()) };
            } else {
                unsafe { System.dealloc(ptr, layout) }
            }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: 호출자가 new_size와 정렬이 유효한 Layout을 이룬다고 보장한다.
            let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
            if !direct(layout) && !direct(new_layout) {
                return unsafe { System.realloc(ptr, layout, new_size) };
            }
            if direct(layout)
                && direct(new_layout)
                && unsafe { resize_in_place(ptr, layout.size(), new_size) }
            {
                return ptr;
            }
            let moved = unsafe { self.alloc(new_layout) };
            if !moved.is_null() {
                unsafe {
                    std::ptr::copy_nonoverlapping(ptr, moved, layout.size().min(new_size));
                    self.dealloc(ptr, layout);
                }
            }
            moved
        }
    }
}
