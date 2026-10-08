//! Wire lengths must not cause allocations unrelated to the available input.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use yrs::encoding::read::Cursor;
use yrs::sync::AwarenessUpdate;
use yrs::updates::decoder::Decode;
use yrs::{Any, IdSet, Snapshot, StateVector, Update};

struct TrackingAllocator;
thread_local! {
    static ALLOCATED: Cell<Option<usize>> = const { Cell::new(None) };
}
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        track(layout.size());
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        track(layout.size());
        System.alloc_zeroed(layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        track(size);
        System.realloc(ptr, layout, size)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}
fn track(size: usize) {
    let _ = ALLOCATED.try_with(|total| {
        if let Some(n) = total.get() {
            total.set(Some(n.saturating_add(size)));
        }
    });
}
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn bounded_error<T>(decode: impl FnOnce() -> Result<T, yrs::encoding::read::Error>) {
    ALLOCATED.with(|total| total.set(Some(0)));
    let result = decode();
    let allocated = ALLOCATED.with(|total| total.replace(None).unwrap());
    assert!(result.is_err());
    // Deterministic even under a loaded test runner, unlike a wall-clock deadline.
    assert!(
        allocated < 64 * 1024,
        "decoder requested {} bytes",
        allocated
    );
}

fn v2(rest: &[u8]) -> Vec<u8> {
    // Feature byte, nine empty streams (the string stream contains an empty string).
    let mut bytes = vec![0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    bytes.extend_from_slice(rest);
    bytes
}

#[test]
fn state_vectors_and_snapshots_bound_client_counts() {
    let hostile = [0xff, 0xff, 0xff, 0x7f, 1, 1];
    bounded_error(|| StateVector::decode_v1(&hostile));
    bounded_error(|| StateVector::decode_v2(&v2(&hostile)));
    let mut snapshot = vec![0]; // Empty delete set, followed by state vector.
    snapshot.extend_from_slice(&hostile);
    bounded_error(|| Snapshot::decode_v1(&snapshot));
    bounded_error(|| Snapshot::decode_v2(&v2(&snapshot)));
}

#[test]
fn updates_bound_client_and_block_counts() {
    let clients = [0xff, 0xff, 0xff, 0x7f, 1, 1, 0, 0];
    bounded_error(|| Update::decode_v1(&clients));
    bounded_error(|| Update::decode_v2(&v2(&clients)));
    let blocks = [1, 0xff, 0xff, 0xff, 0x7f, 1, 0];
    bounded_error(|| Update::decode_v1(&blocks));
    // v2 client stream holds client 1; rest holds count, block count and clock.
    let v2_blocks = [
        0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0x7f, 0,
    ];
    bounded_error(|| Update::decode_v2(&v2_blocks));
}

#[test]
fn delete_sets_bound_range_counts() {
    let ranges = [1, 1, 0xff, 0xff, 0xff, 0x7f];
    bounded_error(|| IdSet::decode_v1(&ranges));
    bounded_error(|| IdSet::decode_v2(&v2(&ranges)));
    bounded_error(|| Snapshot::decode_v1(&ranges));
    bounded_error(|| Snapshot::decode_v2(&v2(&ranges)));
}

#[test]
fn any_bounds_map_and_array_counts() {
    for tag in [117, 118] {
        let bytes = [tag, 0xff, 0xff, 0xff, 0x7f];
        bounded_error(|| Any::decode(&mut Cursor::new(&bytes)));
    }
}

#[test]
fn item_content_bounds_json_and_any_counts() {
    for tag in [2, 8] {
        // One item under root "t", with a hostile content count.
        let bytes = [1, 1, 1, 0, tag, 1, 1, b't', 0xff, 0xff, 0xff, 0x7f];
        bounded_error(|| Update::decode_v1(&bytes));
        // v2 info/parent/length streams contain the same values.
        let bytes_v2 = [
            0, 0, 1, 1, 0, 0, 1, tag, 3, 1, b't', 1, 1, 1, 0, 5, 0xbf, 0xff, 0xff, 0xff, 1, 1, 1, 0,
        ];
        bounded_error(|| Update::decode_v2(&bytes_v2));
    }
}

#[test]
fn awareness_bounds_client_counts() {
    let bytes = [0xff, 0xff, 0xff, 0x7f];
    bounded_error(|| AwarenessUpdate::decode_v1(&bytes));
}
