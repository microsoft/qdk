// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The memory the CUDA algebra lends cuTensorNet (`Scratch`), lent
//! directly to a workspace on the fake libraries, as a call does before
//! its tensor work. Each slot's memory is:
//!
//! - grown only when a call asks for more, the larger lent before the
//!   smaller is freed;
//! - lent at least what its slot asks, from an aligned byte on the host;
//! - bounded by the device's free memory, on the device alone;
//! - refused for a size it cannot lend, the next call served;
//! - freed when released, a buffer the workspace failed to take included.
//!
//! The singular values' buffer is the call scope's
//! (`call_scope/tests.rs`), and the order of a move's QR and contraction
//! is the move's (`cuda_algebra/tests.rs`).

mod misaligned_allocator;

use super::*;
use crate::engine::cuda_algebra::device_session::DeviceSession;
use crate::engine::native_calls::fake_libraries::FakeLibraries;
use crate::engine::native_calls::memory_calls::{WorkspaceDescriptor, WorkspaceSlot};
use std::sync::Arc;

/// The device slots, whose memory is a buffer of the fake libraries.
const DEVICE_SLOTS: [WorkspaceSlot; 2] = [WorkspaceSlot::DeviceScratch, WorkspaceSlot::DeviceCache];

/// The host slots, whose memory is the scratch's own.
const HOST_SLOTS: [WorkspaceSlot; 2] = [WorkspaceSlot::HostScratch, WorkspaceSlot::HostCache];

/// Every slot of a workspace.
const SLOTS: [WorkspaceSlot; 4] = [
    WorkspaceSlot::DeviceScratch,
    WorkspaceSlot::DeviceCache,
    WorkspaceSlot::HostScratch,
    WorkspaceSlot::HostCache,
];

/// A workspace descriptor on the fake libraries and an empty scratch to
/// lend it: what an algebra holds before its first call.
struct WorkspaceLending {
    /// The fake libraries, which record what each slot is lent.
    libraries: Arc<FakeLibraries>,
    /// The device session the workspace belongs to.
    session: DeviceSession,
    /// The workspace the scratch lends its memory to.
    workspace: WorkspaceDescriptor,
    /// The scratch under test.
    scratch: Scratch,
}

impl WorkspaceLending {
    /// A workspace whose slots ask for the bytes `asked` gives, and for
    /// none elsewhere.
    fn asking(asked: &[(WorkspaceSlot, i64)]) -> Self {
        let libraries = Arc::new(FakeLibraries::new());
        for slot in SLOTS {
            libraries.set_workspace_slot_bytes(slot, 0);
        }
        let calls: Arc<dyn NativeCalls> = libraries.clone();
        let session = DeviceSession::new(calls, 0).expect("the fake has device 0");
        let workspace = session
            .calls()
            .create_workspace(session.handle())
            .expect("a workspace on the fake");
        let lending = Self {
            libraries,
            session,
            workspace,
            scratch: Scratch::default(),
        };
        lending.ask(asked);
        lending
    }

    /// Makes the workspace's slots ask for the bytes `asked` gives, as the
    /// next call's sizing would.
    fn ask(&self, asked: &[(WorkspaceSlot, i64)]) {
        for &(slot, bytes) in asked {
            self.libraries.set_workspace_slot_bytes(slot, bytes);
        }
    }

    /// Lends the workspace what its slots ask, as a call does before its
    /// tensor work.
    fn lend(&mut self) -> Result<(), CudaAlgebraError> {
        self.scratch
            .lend(&self.session, self.workspace, &|buffer| Ok(buffer))
    }

    /// The address of the memory `slot` is lent once it asks for `bytes`.
    fn lend_asking(&mut self, slot: WorkspaceSlot, bytes: i64) -> usize {
        self.ask(&[(slot, bytes)]);
        self.lend()
            .unwrap_or_else(|error| panic!("{slot:?} is lent {bytes} bytes: {error}"));
        self.lent(slot).0
    }

    /// The memory `slot` is lent, by address, and its bytes.
    fn lent(&self, slot: WorkspaceSlot) -> (usize, i64) {
        self.libraries
            .lent(self.workspace, slot)
            .unwrap_or_else(|| panic!("{slot:?} is lent memory"))
    }

    /// Destroys the workspace, then releases the scratch, as the algebra
    /// does when it closes, then the session. Returns the release's
    /// result, and the libraries, to check what is left live.
    fn close(mut self) -> (Result<(), CudaAlgebraError>, Arc<FakeLibraries>) {
        let calls = self.session.calls();
        calls
            .destroy_workspace(self.workspace)
            .expect("the workspace is destroyed");
        let released = self.scratch.release(calls);
        (released, self.libraries)
    }
}

/// A slot keeps its memory while a call asks no more than it holds, and
/// grows when a call asks for more: across calls asking for a size, the
/// same size, more, then less, each slot is lent one memory for the first
/// two, a new one for the third, and keeps that one for the last. A
/// device slot frees the smaller buffer as it grows, so it holds one at a
/// time, and frees it only once it has lent the larger: the fake refuses
/// to free a buffer a live workspace holds.
#[test]
fn a_slot_grows_only_when_a_call_asks_for_more() {
    for slot in SLOTS {
        let mut lending = WorkspaceLending::asking(&[]);
        let first = lending.lend_asking(slot, 1000);
        let live = lending.libraries.live_objects();
        assert_eq!(lending.lend_asking(slot, 1000), first, "{slot:?}: the same");
        let grown = lending.lend_asking(slot, 2000);
        assert_ne!(grown, first, "{slot:?}: more");
        assert_eq!(
            lending.libraries.live_objects(),
            live,
            "{slot:?}: the smaller memory is freed"
        );
        assert_eq!(lending.lend_asking(slot, 1000), grown, "{slot:?}: less");
    }
}

/// Each slot is lent at least what it asks for, and a host slot from a
/// 256-byte boundary, as cuTensorNet's own allocations are. The tests'
/// allocator starts a host slot's memory one byte past a boundary, where
/// the slot needs every byte it holds beyond what it lends. Each ask is
/// just above what the last left room for, so a slot that reused memory
/// too small, or skipped its alignment, is caught.
#[test]
fn every_slot_is_lent_at_least_what_it_asks() {
    let boundary = misaligned_allocator::BOUNDARY;
    let memory = vec![0_u8; 512];
    assert_eq!(
        memory.as_ptr().align_offset(boundary),
        boundary - 1,
        "the test allocator starts a byte buffer just past a boundary"
    );
    for slot in SLOTS {
        let mut lending = WorkspaceLending::asking(&[]);
        for asked in [512, 768, 1000, 1256, 2000, 2256, 4096] {
            let address = lending.lend_asking(slot, asked);
            let (_, bytes) = lending.lent(slot);
            assert!(bytes >= asked, "{slot:?}: {bytes} lent of {asked}");
            if HOST_SLOTS.contains(&slot) {
                assert_eq!(address % boundary, 0, "{slot:?}, {asked}: aligned");
            }
        }
    }
}

/// A device slot asking for more than the device has free is refused
/// with the bytes asked and the bytes free, before any allocation; one
/// asking for exactly the free memory is lent it. A host slot is not
/// bounded by it.
#[test]
fn a_device_slot_is_bounded_by_the_free_memory() {
    for slot in SLOTS {
        let mut lending = WorkspaceLending::asking(&[(slot, 1024)]);
        lending.libraries.set_free_memory(1023);
        let lent = lending.lend();
        if HOST_SLOTS.contains(&slot) {
            assert!(lent.is_ok(), "{slot:?}: {lent:?}");
            continue;
        }
        assert!(
            matches!(
                lent,
                Err(CudaAlgebraError::WorkspaceLimitExceeded {
                    required: 1024,
                    maximum: 1023
                })
            ),
            "{slot:?}: {lent:?}"
        );
        assert_eq!(lending.libraries.count("allocate"), 0, "{slot:?}");
        lending.libraries.set_free_memory(1024);
        let lent = lending.lend();
        assert!(lent.is_ok(), "{slot:?}, the free memory: {lent:?}");
    }
}

/// A slot asking for a size the scratch cannot lend fails its call with
/// the error naming it, not a panic or an abort, before any allocation:
/// more host memory than the allocator gives (`HostAllocationFailed`, with
/// the bytes asked), or a negative size, in any slot
/// (`InvalidNativeResult`). The slot is lent nothing, and the next call,
/// asking for a usual size, is lent it.
#[test]
fn a_slot_size_the_scratch_cannot_lend_fails_only_its_call() {
    // Beyond `isize::MAX` once padded, and the most that fits, with the
    // padding of 255 bytes, in `isize::MAX`: more than any host has.
    let too_large = [i64::MAX, i64::MAX - 255];
    let rows = HOST_SLOTS
        .into_iter()
        .flat_map(|slot| too_large.map(|bytes| (slot, bytes)))
        .chain(SLOTS.map(|slot| (slot, -1)));
    for (slot, asked) in rows {
        let mut lending = WorkspaceLending::asking(&[(slot, asked)]);
        let failed = lending.lend();
        let named = match usize::try_from(asked) {
            Ok(asked) => matches!(
                failed,
                Err(CudaAlgebraError::HostAllocationFailed { bytes }) if bytes == asked
            ),
            Err(_) => matches!(failed, Err(CudaAlgebraError::InvalidNativeResult { .. })),
        };
        assert!(named, "{slot:?} {asked}: {failed:?}");
        assert_eq!(lending.libraries.count("allocate"), 0, "{slot:?} {asked}");
        assert_eq!(
            lending.libraries.lent(lending.workspace, slot),
            None,
            "{slot:?} {asked}"
        );
        lending.lend_asking(slot, 1024);
    }
}

/// A device slot's grown buffer that the workspace failed to take: the
/// lend fails with that failure, and the release, once the workspace is
/// destroyed, frees both the grown buffer and the smaller one the
/// workspace may still hold.
#[test]
fn a_buffer_the_workspace_failed_to_take_is_freed_on_release() {
    for slot in DEVICE_SLOTS {
        let mut lending = WorkspaceLending::asking(&[]);
        lending.lend_asking(slot, 1024);
        lending.ask(&[(slot, 4096)]);
        let libraries = &lending.libraries;
        libraries.fail("set_workspace", libraries.count("set_workspace") + 1);
        let failed = lending.lend();
        assert!(
            matches!(
                failed,
                Err(CudaAlgebraError::NativeCallFailed {
                    operation: "set_workspace",
                    ..
                })
            ),
            "{slot:?}: {failed:?}"
        );
        let (released, libraries) = lending.close();
        assert!(released.is_ok(), "{slot:?}: {released:?}");
        assert_eq!(
            libraries.live_objects(),
            0,
            "{slot:?}: every buffer is freed"
        );
    }
}
