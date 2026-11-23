// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The memory the CUDA algebra lends cuTensorNet, and the singular values'
//! buffer it keeps across splits: grown only when a call asks for more,
//! the smaller buffer freed only once nothing uses it (within a move of
//! the centre too), lent in every slot a call asks memory for, bounded by
//! the device's free memory, refused, failing only its call, for a size it
//! cannot lend, and freed when the algebra closes.

use super::*;
use crate::engine::native_calls::fake_libraries::WORKSPACE_BYTES;
use crate::engine::native_calls::memory_calls::WorkspaceSlot;

/// A split the scratch tests repeat: any call that lends a workspace would
/// do.
fn split_once(algebra: &CudaAlgebra) -> Result<(), CudaAlgebraError> {
    let (a, b) = random_pair(1, 1, 1, 40);
    one_qubit_split(&a, &b).on(algebra).map(drop)
}

/// Splits of one shape that ask for the same scratch, then one that asks
/// for more, then one that asks for less: the algebra allocates one
/// scratch buffer and one singular values' buffer for the first, reuses
/// both, the singular values' at an exact fit, replaces the scratch by a
/// larger one for the fourth, and keeps that one for the last. Each host
/// slot, asked memory the same way, is allocated it once for the first
/// three, once more for the fourth, and not for the last.
#[test]
fn the_algebras_buffers_grow_only_when_a_call_asks_for_more() {
    let (algebra, libraries) = algebra();
    split_once(&algebra).expect("a split");
    let after_the_first = libraries.count("allocate");
    for _ in 0..2 {
        split_once(&algebra).expect("a split of the same shape");
    }
    // A repeated split allocates its pair and its two factors alone.
    assert_eq!(libraries.count("allocate") - after_the_first, 2 * 4);
    assert_eq!(libraries.scratch_buffers(), 1);
    libraries.set_workspace_slot_bytes(WorkspaceSlot::DeviceScratch, 4 * WORKSPACE_BYTES);
    split_once(&algebra).expect("the split asking for more");
    assert_eq!(libraries.scratch_buffers(), 2);
    assert_eq!(libraries.live_scratch(), 1);
    libraries.set_workspace_slot_bytes(WorkspaceSlot::DeviceScratch, WORKSPACE_BYTES);
    split_once(&algebra).expect("the split asking for less");
    assert_eq!(libraries.scratch_buffers(), 2);

    // Sizes no other test allocates; a host slot's memory is the size
    // asked plus its alignment padding, less than a boundary.
    let host_slots = [
        (WorkspaceSlot::HostScratch, 3_000_017),
        (WorkspaceSlot::HostCache, 5_000_011),
    ];
    for (slot, asked) in host_slots {
        let (algebra, libraries) = super::algebra();
        let grown_to = |bytes: usize, splits: usize| {
            let watched = bytes..bytes + super::misaligned_allocator::BOUNDARY;
            super::misaligned_allocator::count_allocations(watched, || {
                for _ in 0..splits {
                    split_once(&algebra).expect("a split asking host memory");
                }
            })
        };
        let set = |bytes: usize| {
            let bytes = i64::try_from(bytes).expect("a small size");
            libraries.set_workspace_slot_bytes(slot, bytes);
        };
        set(asked);
        assert_eq!(grown_to(asked, 3), 1, "{slot:?}: the same size");
        set(2 * asked);
        assert_eq!(grown_to(2 * asked, 1), 1, "{slot:?}: more");
        set(asked);
        assert_eq!(grown_to(asked, 1), 0, "{slot:?}: less");
    }
}

/// Calls that ask for more scratch than the workspace holds: a split
/// after a split, and the contraction of a move of the centre after its
/// QR, either side, with a gate or not. The smaller buffer is freed only
/// once nothing may use it: the larger is lent first, and the device has
/// finished the work that used the smaller. The fake refuses to free a
/// buffer a workspace holds, or one a stream may still use, so each call
/// succeeds.
#[test]
fn a_grown_scratch_frees_the_smaller_only_once_nothing_uses_it() {
    let (algebra, libraries) = algebra();
    split_once(&algebra).expect("a split");
    libraries.set_workspace_slot_bytes(WorkspaceSlot::DeviceScratch, 4 * WORKSPACE_BYTES);
    let grown = split_once(&algebra);
    assert!(grown.is_ok(), "a split after a split: {grown:?}");

    let (a, b) = random_pair(1, 1, 1, 41);
    let gates = [None, Some(one_qubit_gate(42))];
    for centre in [CentreSide::Left, CentreSide::Right] {
        for gate in gates {
            let (algebra, libraries) = super::algebra();
            libraries.set_workspace_slot_bytes(WorkspaceSlot::DeviceScratch, 64);
            libraries.set_workspace_slot_bytes_after_the_next_qr(
                WorkspaceSlot::DeviceScratch,
                WORKSPACE_BYTES,
            );
            let moved = Move {
                a: &a,
                b: &b,
                l: 1,
                m: 1,
                r: 1,
                gate,
                centre,
            };
            let grown = moved.on(&algebra).map(drop);
            assert!(
                grown.is_ok(),
                "{centre:?}, gate {}: {grown:?}",
                gate.is_some()
            );
            assert_eq!(
                libraries.scratch_buffers(),
                2,
                "{centre:?}, gate {}: grown within the move",
                gate.is_some()
            );
        }
    }
}

/// Calls whose workspace asks for memory in its other slots too (the
/// device cache and both host slots), then for more there, some asks just
/// above the last, within the bytes a host slot holds beyond it to align
/// what it lends, its memory starting just past a 256-byte boundary (the
/// tests' allocator), where it needs every one of them: every slot is lent
/// at least what it asks, which the fake checks at each call.
#[test]
fn every_slot_a_call_asks_memory_for_is_lent_it() {
    let boundary = super::misaligned_allocator::BOUNDARY;
    let memory = vec![0_u8; 512];
    assert_eq!(
        memory.as_ptr().align_offset(boundary),
        boundary - 1,
        "the test allocator starts a byte buffer just past a boundary, \
         where a host slot needs every byte it holds beyond what it lends"
    );
    let (algebra, libraries) = algebra();
    for bytes in [512, 768, 1000, 1256, 2000, 2256, 4096] {
        for slot in [
            WorkspaceSlot::DeviceCache,
            WorkspaceSlot::HostScratch,
            WorkspaceSlot::HostCache,
        ] {
            libraries.set_workspace_slot_bytes(slot, bytes);
        }
        let lent = split_once(&algebra);
        assert!(lent.is_ok(), "{bytes} bytes: {lent:?}");
    }
}

/// A call asking for more scratch than the device has free: it fails with
/// the bytes asked and the bytes free, before allocating any. A call
/// asking for exactly the free memory is lent it.
#[test]
fn scratch_is_bounded_by_the_free_memory() {
    let (algebra, libraries) = algebra();
    let asked = usize::try_from(WORKSPACE_BYTES).expect("a positive size");
    libraries.set_free_memory(asked - 1);
    let refused = split_once(&algebra);
    assert!(
        matches!(
            refused,
            Err(CudaAlgebraError::WorkspaceLimitExceeded { required, maximum })
                if required == asked && maximum == asked - 1
        ),
        "{refused:?}"
    );
    assert_eq!(libraries.scratch_buffers(), 0);
    libraries.set_free_memory(asked);
    split_once(&algebra).expect("a split asking for the free memory");
}

/// Calls whose library asks, in one slot, for a size the algebra cannot
/// lend: more host memory than the host's allocator gives (a `Vec` cannot
/// hold it, or the allocator returns none), or a negative size, in a
/// device or a host slot. Each fails with the error naming it
/// (`HostAllocationFailed` with the bytes asked, `InvalidNativeResult`),
/// not a panic or an abort, before its split runs, having released every
/// native object it made, and the algebra serves the next call.
#[test]
fn a_slot_size_the_algebra_cannot_lend_fails_only_its_call() {
    use WorkspaceSlot::{DeviceCache, DeviceScratch, HostCache, HostScratch};
    // Beyond `isize::MAX` once padded, and the most that fits, with the
    // padding of 255 bytes, in `isize::MAX`: more than any host has.
    let too_large = [i64::MAX, i64::MAX - 255];
    let rows = [HostScratch, HostCache]
        .into_iter()
        .flat_map(|slot| too_large.map(|bytes| (slot, bytes)))
        .chain([DeviceScratch, DeviceCache, HostScratch, HostCache].map(|slot| (slot, -1)));
    for (slot, bytes) in rows {
        let (algebra, libraries) = algebra();
        split_once(&algebra).expect("a split that makes the workspace");
        let live = libraries.live_objects();
        let splits = libraries.count("gate_split");
        libraries.set_workspace_slot_bytes(slot, bytes);
        let failed = split_once(&algebra);
        let named = match usize::try_from(bytes) {
            Ok(asked) => matches!(
                failed,
                Err(CudaAlgebraError::HostAllocationFailed { bytes }) if bytes == asked
            ),
            Err(_) => matches!(failed, Err(CudaAlgebraError::InvalidNativeResult { .. })),
        };
        assert!(named, "{slot:?} {bytes}: {failed:?}");
        assert_eq!(libraries.count("gate_split"), splits, "{slot:?} {bytes}");
        assert_eq!(libraries.live_objects(), live, "{slot:?} {bytes}");
        let usual = match slot {
            DeviceScratch => WORKSPACE_BYTES,
            _ => 0,
        };
        libraries.set_workspace_slot_bytes(slot, usual);
        split_once(&algebra).expect("the next call");
    }
}

/// A grown scratch the workspace failed to take: the call fails, and the
/// algebra, when dropped, frees both the new buffer and the one the
/// workspace may still hold.
#[test]
fn a_scratch_the_workspace_failed_to_take_is_freed_at_close() {
    let (algebra, libraries) = algebra();
    split_once(&algebra).expect("a split");
    libraries.set_workspace_slot_bytes(WorkspaceSlot::DeviceScratch, 4 * WORKSPACE_BYTES);
    libraries.fail("set_workspace", libraries.count("set_workspace") + 1);
    let failed = split_once(&algebra);
    assert!(
        matches!(
            failed,
            Err(CudaAlgebraError::NativeCallFailed {
                operation: "set_workspace",
                ..
            })
        ),
        "{failed:?}"
    );
    drop(algebra);
    assert_eq!(libraries.live_objects(), 0);
}
