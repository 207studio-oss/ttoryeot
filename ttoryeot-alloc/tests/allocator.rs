use std::alloc::{GlobalAlloc, Layout};

use ttoryeot_alloc::Allocator;

const SMALL: usize = 1024;
const LARGE: usize = 1024 * 1024;

fn fill(ptr: *mut u8, size: usize) {
    for offset in 0..size {
        unsafe { ptr.add(offset).write(offset as u8) };
    }
}

fn assert_filled(ptr: *mut u8, size: usize) {
    for offset in 0..size {
        assert_eq!(unsafe { ptr.add(offset).read() }, offset as u8, "{offset}");
    }
}

#[test]
fn realloc_keeps_bytes_across_the_size_boundary() {
    let sizes = [
        (SMALL, LARGE),
        (LARGE, 4 * LARGE),
        (4 * LARGE, LARGE),
        (LARGE, SMALL),
    ];
    for (from, to) in sizes {
        let layout = Layout::from_size_align(from, 16).unwrap();
        let ptr = unsafe { Allocator.alloc(layout) };
        assert!(!ptr.is_null());
        fill(ptr, from);
        let moved = unsafe { Allocator.realloc(ptr, layout, to) };
        assert!(!moved.is_null());
        assert_filled(moved, from.min(to));
        unsafe { Allocator.dealloc(moved, Layout::from_size_align(to, 16).unwrap()) };
    }
}

#[test]
fn zeroed_blocks_are_zero_after_a_dirty_block_of_the_same_size_is_freed() {
    for size in [SMALL, LARGE] {
        let layout = Layout::from_size_align(size, 16).unwrap();
        let dirty = unsafe { Allocator.alloc(layout) };
        unsafe { dirty.write_bytes(0xAB, size) };
        unsafe { Allocator.dealloc(dirty, layout) };
        let zeroed = unsafe { Allocator.alloc_zeroed(layout) };
        assert!((0..size).all(|offset| unsafe { zeroed.add(offset).read() } == 0));
        unsafe { Allocator.dealloc(zeroed, layout) };
    }
}

#[test]
fn large_blocks_honor_every_alignment() {
    for align in [16, 4096, 16 * 1024, 64 * 1024] {
        let layout = Layout::from_size_align(LARGE, align).unwrap();
        let ptr = unsafe { Allocator.alloc(layout) };
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % align, 0, "{align}");
        fill(ptr, LARGE);
        assert_filled(ptr, LARGE);
        unsafe { Allocator.dealloc(ptr, layout) };
    }
}
