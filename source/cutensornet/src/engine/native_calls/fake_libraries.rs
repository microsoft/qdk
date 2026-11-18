// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeLibraries`]: cuTensorNet and the CUDA Runtime, simulated on the CPU
//! for the tests of the native calls' callers (the device session and the
//! CUDA algebra).
//!
//! CI has no GPU, so the callers' real code (every call they make, in the
//! order they make them) runs against these fake libraries, which implement
//! `NativeCalls` as the loaded libraries do, one file per trait
//! (`fake_libraries/session_calls.rs`, `fake_libraries/memory_calls.rs`,
//! `fake_libraries/tensor_calls.rs`). It secures what a GPU run would
//! otherwise be the first to show: a call out of cuTensorNet's order,
//! memory freed while still in use, a read before the write is done. It is
//! test support, not a test: it sits beside the traits it implements and
//! the loaded libraries it stands in for, so its callers' tests reach it as
//! they reach the loaded libraries, and it is compiled only with the tests,
//! which live under `src/tests`. This file holds what the traits' files
//! share: the objects the fake keeps, the checks every call goes through,
//! and what a test sets and reads.
//!
//! It keeps what cuTensorNet and the CUDA Runtime would keep, by id:
//! streams, handles, buffers, workspaces, and the tensor API's objects,
//! whose rules are in `fake_libraries/tensor_calls.rs`. It is strict about
//! the order its callers call in, so a wrong order fails a test instead of
//! passing silently; the rules on memory and streams:
//!
//! - a buffer lent to a device slot of a live workspace is never freed:
//!   the descriptor would hold freed memory;
//! - a tensor call needs memory lent to every slot of its workspace that
//!   asks for some, at least what the slot asks, and no copy still running
//!   (`synchronize_copies`, the legacy stream), and the source of a copy
//!   still running is not freed;
//! - a tensor call runs on a live stream, and a stream is synchronized
//!   only while live;
//! - after a tensor call, no buffer is freed (`free`) until every stream
//!   recorded as the last to write a pending object (below) is
//!   synchronized, since the fake does not track what a stream's work
//!   reads. A stream whose writes another stream wrote again since is no
//!   longer recorded, so these waits do not cover its work;
//! - a buffer a tensor call wrote is copied (`copy_to_host`, and
//!   `copy_on_device` from it), copied into (`copy_to_device`,
//!   `copy_on_device` into it), and an SVD info reported
//!   (`svd_truncation`), only once the stream last queued to write it is
//!   synchronized: the copies run on CUDA's legacy stream, which a
//!   non-blocking stream does not wait for, and another stream's
//!   synchronization says nothing of it. An object written on two streams
//!   is released by the later one, and the fake does not flag the race.
//!   Other reads and writes of a pending write are not checked: a later
//!   call's inputs and outputs;
//! - a stream destroyed with writes pending keeps them, as its work may
//!   still run after `cudaStreamDestroy`: no buffer is freed while it is
//!   their last recorded writer;
//! - every object a call takes a handle for (a workspace, a descriptor, an
//!   SVD config or info, a network or an optimizer info) belongs to the
//!   handle it was created with, and is used only with that handle, which
//!   must be live; a handle is destroyed only once nothing of it is, so a
//!   handle released too early is caught.
//!
//! It records the buffers lent to workspaces as device scratch, so a test
//! can see how many are allocated. A workspace asks for [`WORKSPACE_BYTES`]
//! of device scratch and nothing in its other slots, unless told other
//! sizes. A call can be made to fail with [`FakeLibraries::fail`], to test
//! cleanup; and the device count, the free memory and the truncation a
//! gate split reports can be set, to test what the callers do with what
//! the library answers.

mod dense_tensor;
mod memory_calls;
mod session_calls;
mod tensor_calls;

use super::{OpaqueHandle, Stream, complex::Complex64Abi, memory_calls::WorkspaceSlot};
use crate::engine::CudaAlgebraError;
use std::{
    collections::BTreeMap,
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, MutexGuard},
};
use tensor_calls::{SizedFor, SvdConfig, TensorShape};

/// The bytes `workspace_size` asks for in the device scratch, unless
/// [`FakeLibraries::set_workspace_bytes`] or the setters beside it say
/// otherwise.
pub(crate) const WORKSPACE_BYTES: i64 = 1024;

/// Every slot of a workspace descriptor.
const WORKSPACE_SLOTS: [WorkspaceSlot; 4] = [
    WorkspaceSlot::DeviceScratch,
    WorkspaceSlot::DeviceCache,
    WorkspaceSlot::HostScratch,
    WorkspaceSlot::HostCache,
];

#[derive(Default)]
pub(crate) struct FakeLibraries {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next_id: usize,
    objects: BTreeMap<usize, Object>,
    events: Vec<&'static str>,
    calls: BTreeMap<&'static str, usize>,
    failures: Vec<(&'static str, usize)>,
    /// Every buffer ever attached to a workspace as scratch.
    scratch: Vec<usize>,
    /// Whether a copy may still be running on the legacy stream: set by a
    /// copy, cleared by `synchronize_copies`.
    copies_running: bool,
    /// What the work queued on a stream may still be writing (a tensor
    /// call's outputs and SVD info), by id, with the
    /// stream of the last call queued to write it: set by the call, cleared
    /// by `synchronize_stream` on that stream alone, and kept when the
    /// stream is destroyed (the rules on streams, in this file's header).
    pending_writes: BTreeMap<usize, usize>,
    /// The sources of the device-to-device copies that may still be
    /// running, which must not be freed.
    copy_sources: Vec<usize>,
    /// The free device memory `memory_info` reports, if not 1 TiB.
    free_memory: Option<usize>,
    /// The device scratch `workspace_size` asks for, if not
    /// [`WORKSPACE_BYTES`].
    workspace_bytes: Option<i64>,
    /// The device scratch the setting above takes when the next QR runs.
    workspace_bytes_after_qr: Option<i64>,
    /// The bytes `workspace_size` asks for in each other slot: the device
    /// cache and both host slots.
    other_slot_bytes: i64,
    /// The bytes `workspace_size` asks for in a slot, by its index, when
    /// set for that slot alone, whatever the settings above say.
    slot_bytes: [Option<i64>; 4],
    /// The handle each live object that takes one was created with.
    owners: BTreeMap<usize, usize>,
    /// The device count `device_count` reports, if not 1.
    device_count: Option<i32>,
    /// The truncation every gate split reports, instead of its own.
    misreported_truncation: Option<super::tensor_calls::SvdTruncation>,
    /// How many allocations are left before one returns the live buffer
    /// held here, by id, instead of a new one.
    aliased_allocation: Option<(usize, usize)>,
}

enum Object {
    Stream,
    Handle,
    Buffer(Vec<Complex64Abi>),
    /// A workspace descriptor: the memory lent to each slot, by
    /// [`slot_index`], and the bytes it lends; and the tensor call it was
    /// last sized for.
    Workspace {
        lent: [Option<(usize, i64)>; 4],
        sized_for: Option<SizedFor>,
    },
    TensorDescriptor(TensorShape),
    SvdConfig(SvdConfig),
    /// An SVD info, and what the last gate split given it truncated.
    SvdInfo(Option<super::tensor_calls::SvdTruncation>),
    /// A contraction network: the info whose path it was last optimized
    /// to, and, once prepared along that path, the device scratch bytes
    /// lent at the preparation.
    Network {
        inputs: [TensorShape; 2],
        output: TensorShape,
        path: Option<usize>,
        prepared: Option<i64>,
    },
    OptimizerInfo {
        network: usize,
        optimized: bool,
    },
}

fn failed(operation: &'static str, message: &str) -> CudaAlgebraError {
    CudaAlgebraError::NativeCallFailed {
        component: "fake libraries",
        operation,
        status: 1,
        message: message.to_string(),
    }
}

fn pointer(id: usize) -> OpaqueHandle {
    NonNull::new(std::ptr::without_provenance_mut::<c_void>(id)).unwrap_or(NonNull::dangling())
}

fn id(handle: OpaqueHandle) -> usize {
    handle.addr().get()
}

fn to_usize(value: i64) -> Result<usize, CudaAlgebraError> {
    usize::try_from(value).map_err(|_| failed("conversion", "negative value"))
}

/// Where a workspace's `lent` keeps `slot`.
fn slot_index(slot: WorkspaceSlot) -> usize {
    match slot {
        WorkspaceSlot::DeviceScratch => 0,
        WorkspaceSlot::DeviceCache => 1,
        WorkspaceSlot::HostScratch => 2,
        WorkspaceSlot::HostCache => 3,
    }
}

/// Whether `slot` takes device memory: a buffer of the fake.
fn is_device(slot: WorkspaceSlot) -> bool {
    matches!(
        slot,
        WorkspaceSlot::DeviceScratch | WorkspaceSlot::DeviceCache
    )
}

impl FakeLibraries {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Makes the `occurrence`-th call (from 1) to `operation` fail, beside
    /// the calls made to fail before.
    pub(crate) fn fail(&self, operation: &'static str, occurrence: usize) {
        self.lock().failures.push((operation, occurrence));
    }

    pub(crate) fn events(&self) -> Vec<&'static str> {
        self.lock().events.clone()
    }

    pub(crate) fn count(&self, operation: &'static str) -> usize {
        self.lock().calls.get(operation).copied().unwrap_or(0)
    }

    /// Every native object not yet released.
    pub(crate) fn live_objects(&self) -> usize {
        self.lock().objects.len()
    }

    /// How many cuTensorNet handles are live.
    pub(crate) fn live_handles(&self) -> usize {
        self.lock()
            .objects
            .values()
            .filter(|object| matches!(object, Object::Handle))
            .count()
    }

    /// How many scratch buffers are allocated now.
    pub(crate) fn live_scratch(&self) -> usize {
        self.lock().live_scratch()
    }

    /// How many buffers were ever attached to a workspace as scratch.
    pub(crate) fn scratch_buffers(&self) -> usize {
        self.lock().scratch.len()
    }

    /// Makes `memory_info` report `bytes` free.
    pub(crate) fn set_free_memory(&self, bytes: usize) {
        self.lock().free_memory = Some(bytes);
    }

    /// Makes `device_count` report `count` devices.
    pub(crate) fn set_device_count(&self, count: i32) {
        self.lock().device_count = Some(count);
    }

    /// Makes every gate split from now on report `truncation` in its SVD
    /// info, as a library that misreported it would; its outputs are
    /// unchanged.
    pub(crate) fn misreport_truncation(&self, truncation: super::tensor_calls::SvdTruncation) {
        self.lock().misreported_truncation = Some(truncation);
    }

    /// Makes the `occurrence`-th allocation from now on (from 1) return the
    /// buffer allocated last before this call, still live, as an allocator
    /// that handed out memory in use would.
    pub(crate) fn alias_allocation(&self, occurrence: usize) {
        let mut inner = self.lock();
        let last = inner
            .objects
            .iter()
            .rev()
            .find_map(|(&id, object)| matches!(object, Object::Buffer(_)).then_some(id))
            .expect("a live buffer to alias");
        inner.aliased_allocation = Some((occurrence, last));
    }

    /// Makes every workspace ask for `bytes` of device scratch from now on.
    pub(crate) fn set_workspace_bytes(&self, bytes: i64) {
        self.lock().workspace_bytes = Some(bytes);
    }

    /// Makes every workspace ask for `bytes` of device scratch once the
    /// next QR has run, as the setting above would then, so the
    /// contraction a move of the centre runs after its QR grows the
    /// scratch.
    pub(crate) fn set_workspace_bytes_after_the_next_qr(&self, bytes: i64) {
        self.lock().workspace_bytes_after_qr = Some(bytes);
    }

    /// Makes every workspace ask for `bytes` in each of its other slots
    /// (the device cache and both host slots) from now on, instead of
    /// none.
    pub(crate) fn set_other_workspace_slot_bytes(&self, bytes: i64) {
        self.lock().other_slot_bytes = bytes;
    }

    /// Makes every workspace ask for `bytes` in `slot` from now on,
    /// whatever the settings above say.
    pub(crate) fn set_workspace_slot_bytes(&self, slot: WorkspaceSlot, bytes: i64) {
        self.lock().slot_bytes[slot_index(slot)] = Some(bytes);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records a call to `operation` and fails it if asked to.
    fn call(&self, operation: &'static str) -> Result<MutexGuard<'_, Inner>, CudaAlgebraError> {
        let mut inner = self.lock();
        inner.events.push(operation);
        let count = inner.calls.entry(operation).or_insert(0);
        *count += 1;
        let count = *count;
        if inner.failures.contains(&(operation, count)) {
            return Err(failed(operation, "injected failure"));
        }
        Ok(inner)
    }
}

impl Inner {
    /// Fails unless the work about to run may: no copy still running, and
    /// `workspace` lending, in every slot that asks for memory, at least
    /// what it asks for, live in a device slot. Returns the bytes of the
    /// device scratch buffer, 0 if none is lent.
    fn lent_scratch(
        &self,
        workspace: OpaqueHandle,
        operation: &'static str,
    ) -> Result<usize, CudaAlgebraError> {
        if self.copies_running {
            return Err(failed(operation, "a copy may still be running"));
        }
        let Some(Object::Workspace { lent, .. }) = self.objects.get(&id(workspace)) else {
            return Err(failed(operation, "no such workspace"));
        };
        for slot in WORKSPACE_SLOTS {
            let asked = self.recommended_bytes(slot);
            if asked <= 0 {
                continue;
            }
            let Some((memory, bytes)) = lent[slot_index(slot)] else {
                return Err(failed(
                    operation,
                    "no memory lent to a slot that asks for it",
                ));
            };
            if bytes < asked {
                return Err(failed(operation, "less memory lent than a slot asks for"));
            }
            if is_device(slot) {
                self.buffer(memory, operation)?;
            }
        }
        match lent[slot_index(WorkspaceSlot::DeviceScratch)] {
            Some((scratch, _)) => Ok(size_of_val(self.buffer(scratch, operation)?)),
            None => Ok(0),
        }
    }

    /// Queues work on `stream` that writes `written`, by id, until
    /// `synchronize_stream(stream)`.
    fn queue_on(&mut self, stream: Stream, written: impl IntoIterator<Item = usize>) {
        for object in written {
            self.pending_writes.insert(object, id(stream));
        }
    }

    /// Whether work queued on a stream not synchronized since may still
    /// write `object`.
    fn write_pending(&self, object: usize) -> bool {
        self.pending_writes.contains_key(&object)
    }

    /// Whether any stream may still be running work, as far as the last
    /// writers of pending writes record it. `free` needs it, as the fake
    /// does not track what a stream's work reads (the rules on streams, in
    /// this file's header).
    fn any_stream_busy(&self) -> bool {
        !self.pending_writes.is_empty()
    }

    /// The bytes `workspace` lends its device scratch, 0 if none.
    fn lent_scratch_bytes(
        &self,
        workspace: OpaqueHandle,
        operation: &'static str,
    ) -> Result<i64, CudaAlgebraError> {
        match self.objects.get(&id(workspace)) {
            Some(Object::Workspace { lent, .. }) => {
                Ok(lent[slot_index(WorkspaceSlot::DeviceScratch)].map_or(0, |(_, bytes)| bytes))
            }
            _ => Err(failed(operation, "no such workspace")),
        }
    }

    /// The bytes every workspace asks for in `slot`.
    fn recommended_bytes(&self, slot: WorkspaceSlot) -> i64 {
        if let Some(bytes) = self.slot_bytes[slot_index(slot)] {
            return bytes;
        }
        match slot {
            WorkspaceSlot::DeviceScratch => self.workspace_bytes.unwrap_or(WORKSPACE_BYTES),
            _ => self.other_slot_bytes,
        }
    }

    fn live_scratch(&self) -> usize {
        self.scratch
            .iter()
            .filter(|scratch| self.objects.contains_key(scratch))
            .count()
    }

    fn create(&mut self, object: Object) -> OpaqueHandle {
        self.next_id += 1;
        let id = self.next_id * 256;
        self.objects.insert(id, object);
        pointer(id)
    }

    /// Creates `object` on the live `handle`, which it belongs to.
    fn create_on(
        &mut self,
        handle: OpaqueHandle,
        operation: &'static str,
        object: Object,
    ) -> Result<OpaqueHandle, CudaAlgebraError> {
        self.require(handle, operation, |o| matches!(o, Object::Handle))?;
        let created = self.create(object);
        self.owners.insert(id(created), id(handle));
        Ok(created)
    }

    /// Fails unless `handle` is live and `object` was created with it.
    fn owned(
        &self,
        handle: OpaqueHandle,
        object: OpaqueHandle,
        operation: &'static str,
    ) -> Result<(), CudaAlgebraError> {
        self.require(handle, operation, |o| matches!(o, Object::Handle))?;
        match self.owners.get(&id(object)) {
            Some(&owner) if owner == id(handle) => Ok(()),
            Some(_) => Err(failed(operation, "the object belongs to another handle")),
            None => Err(failed(operation, "no such object")),
        }
    }

    fn destroy(
        &mut self,
        handle: OpaqueHandle,
        operation: &'static str,
        is_kind: fn(&Object) -> bool,
    ) -> Result<(), CudaAlgebraError> {
        match self.objects.get(&id(handle)) {
            Some(object) if is_kind(object) => {
                self.objects.remove(&id(handle));
                self.owners.remove(&id(handle));
                Ok(())
            }
            _ => Err(failed(operation, "no such object")),
        }
    }

    fn require(
        &self,
        handle: OpaqueHandle,
        operation: &'static str,
        is_kind: fn(&Object) -> bool,
    ) -> Result<(), CudaAlgebraError> {
        match self.objects.get(&id(handle)) {
            Some(object) if is_kind(object) => Ok(()),
            _ => Err(failed(operation, "no such object")),
        }
    }

    fn buffer(
        &self,
        id: usize,
        operation: &'static str,
    ) -> Result<&[Complex64Abi], CudaAlgebraError> {
        match self.objects.get(&id) {
            Some(Object::Buffer(buffer)) => Ok(buffer),
            _ => Err(failed(operation, "no such buffer")),
        }
    }
}
