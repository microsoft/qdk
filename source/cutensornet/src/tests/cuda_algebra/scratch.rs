// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The memory the CUDA algebra lends cuTensorNet, and the singular values'
//! buffer it keeps across splits: grown only when a call asks for more,
//! lent before the smaller buffer is freed, lent in every slot a call asks
//! memory for, bounded by the device's free memory, and freed when the
//! algebra closes.

use super::*;
use crate::native_calls::fake_libraries::WORKSPACE_BYTES;
use crate::native_calls::memory_calls::WorkspaceSlot;

/// A split the scratch tests repeat: any call that lends a workspace would
/// do.
fn split_once(algebra: &CudaAlgebra) -> Result<(), NativeError> {
    let (a, b) = random_pair(1, 1, 1, 40);
    one_qubit_split(&a, &b).on(algebra).map(drop)
}

/// Splits of one shape that ask for the same scratch, then one that asks
/// for more, then one that asks for less: the algebra allocates one
/// scratch buffer and one singular values' buffer for the first, reuses
/// both, the singular values' at an exact fit, replaces the scratch by a
/// larger one for the fourth, and keeps that one for the last.
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
    libraries.set_workspace_bytes(4 * WORKSPACE_BYTES);
    split_once(&algebra).expect("the split asking for more");
    assert_eq!(libraries.scratch_buffers(), 2);
    assert_eq!(libraries.live_scratch(), 1);
    libraries.set_workspace_bytes(WORKSPACE_BYTES);
    split_once(&algebra).expect("the split asking for less");
    assert_eq!(libraries.scratch_buffers(), 2);
}

/// A call that asks for more scratch than the workspace holds: the larger
/// buffer is lent before the smaller is freed, which the fake refuses while
/// a workspace holds it, so the call succeeds.
#[test]
fn a_grown_scratch_is_lent_before_the_smaller_is_freed() {
    let (algebra, libraries) = algebra();
    split_once(&algebra).expect("a split");
    libraries.set_workspace_bytes(4 * WORKSPACE_BYTES);
    let grown = split_once(&algebra);
    assert!(grown.is_ok(), "{grown:?}");
}

/// Calls whose workspace asks for memory in its other slots too (the
/// device cache and both host slots), then for more there, some asks just
/// above the last, within the bytes a host slot holds beyond it to align
/// what it lends, its memory starting just past a 256-byte boundary (the
/// tests' allocator), where it needs every one of them: every slot is lent
/// at least what it asks, which the fake checks at each call.
#[test]
fn every_slot_a_call_asks_memory_for_is_lent_it() {
    let boundary = crate::tests::misaligned_allocator::BOUNDARY;
    let memory = vec![0_u8; 512];
    assert_eq!(
        memory.as_ptr().align_offset(boundary),
        boundary - 1,
        "the test allocator starts a byte buffer just past a boundary, \
         where a host slot needs every byte it holds beyond what it lends"
    );
    let (algebra, libraries) = algebra();
    for bytes in [512, 768, 1000, 1256, 2000, 2256, 4096] {
        libraries.set_other_workspace_slot_bytes(bytes);
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
            Err(NativeError::WorkspaceLimitExceeded { required, maximum })
                if required == asked && maximum == asked - 1
        ),
        "{refused:?}"
    );
    assert_eq!(libraries.scratch_buffers(), 0);
    libraries.set_free_memory(asked);
    split_once(&algebra).expect("a split asking for the free memory");
}

/// Calls whose library asks for more host memory in one host slot than
/// the host's allocator gives, because a `Vec` cannot hold it or because
/// the allocator returns none: each fails with `HostAllocationFailed`
/// naming the bytes asked, not a panic or an abort, having released every
/// native object it made, and the algebra serves the next call.
#[test]
fn a_host_slot_the_allocator_refuses_fails_only_its_call() {
    // Beyond `isize::MAX` once padded, and the most that fits, with the
    // padding of 255 bytes, in `isize::MAX`: more than any host has.
    let sizes = [i64::MAX, i64::MAX - 255];
    let slots = [WorkspaceSlot::HostScratch, WorkspaceSlot::HostCache];
    for (slot, bytes) in slots
        .into_iter()
        .flat_map(|slot| sizes.map(|bytes| (slot, bytes)))
    {
        let (algebra, libraries) = algebra();
        split_once(&algebra).expect("a split that makes the workspace");
        let live = libraries.live_objects();
        libraries.set_workspace_slot_bytes(slot, bytes);
        let failed = split_once(&algebra);
        let asked = usize::try_from(bytes).expect("a 64-bit host");
        assert!(
            matches!(
                failed,
                Err(NativeError::HostAllocationFailed { bytes }) if bytes == asked
            ),
            "{slot:?} {bytes}: {failed:?}"
        );
        assert_eq!(libraries.live_objects(), live, "{slot:?} {bytes}");
        libraries.set_workspace_slot_bytes(slot, 0);
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
    libraries.set_workspace_bytes(4 * WORKSPACE_BYTES);
    libraries.fail("set_workspace", libraries.count("set_workspace") + 1);
    let failed = split_once(&algebra);
    assert!(
        matches!(
            failed,
            Err(NativeError::NativeCallFailed {
                operation: "set_workspace",
                ..
            })
        ),
        "{failed:?}"
    );
    drop(algebra);
    assert_eq!(libraries.live_objects(), 0);
}
