// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeLibraries`]: cuTensorNet and the CUDA Runtime, simulated on the CPU
//! for the tests of the native calls' callers (the engine, and the tensor
//! API's caller).
//!
//! CI has no GPU, so the callers' real code (every call they make, in the
//! order they make them) runs against these fake libraries, which implement
//! `NativeCalls` as the loaded libraries do, one file per trait
//! (`fake_libraries/session_calls.rs`, `fake_libraries/memory_calls.rs`,
//! `fake_libraries/state_calls.rs`, `fake_libraries/tensor_calls.rs`). It
//! secures what a GPU run would
//! otherwise be the first to show: a call out of cuTensorNet's order,
//! memory freed while still in use, a read of a stale state. It is test
//! support, not a test: it sits beside the traits it implements and the
//! loaded libraries it stands in for, so its callers' tests reach it as
//! they reach the loaded libraries, and it is compiled only with the tests,
//! which live under `src/tests`. This file holds what the traits' files
//! share: the objects the fake keeps, the checks every call goes through,
//! and what a test sets and reads.
//!
//! It keeps what cuTensorNet would keep, by id: buffers, workspaces, states
//! with the operators applied to them, network operators and expectations,
//! and the tensor API's objects (`fake_libraries/tensor_calls.rs`). A
//! compute replays a state's operators on a state vector
//! (`fake_libraries/state_vector.rs`), reading each matrix from its buffer at
//! that moment, as the library reads it at compute time; reads use the computed
//! state. It is strict about the order its callers call in, so a wrong order
//! fails a test instead of passing silently; for the state API (the rule on
//! buffers lent to a workspace holds for the tensor API too):
//!
//! - `finalize_mps` once per state, before any configuration or prepare;
//! - a compute needs a prepare since the last operator, and scratch
//!   attached to the workspace;
//! - an expectation reads only a state computed since its last
//!   operator, whose output buffers are still allocated;
//! - `capture_mps` needs a state computed since its last operator; it makes
//!   the computed state the start of the next compute and deletes the
//!   operators, as cuTensorNet does, and the next compute reads that start
//!   from the captured compute's output buffers, which must still be
//!   allocated;
//! - `initialize_mps` takes buffers that hold a computed MPS, site by site,
//!   while they are allocated: a compute's outputs, as a restart uses them,
//!   or copies of them (`copy_on_device`), as a fork does. It makes that
//!   MPS the start of a state no operator has been applied to, with one
//!   extent and stride table per site. The next compute reads the start
//!   from those buffers, which must still be allocated;
//! - a buffer lent to a device slot of a live workspace is never freed:
//!   the descriptor would hold freed memory;
//! - a compute or an expectation needs memory lent to every slot of its
//!   workspace that asks for some, at least what the slot asks, and no
//!   copy still running (`synchronize_copies`, the legacy stream), and the
//!   source of a copy still running is not freed;
//!   after a compute, nothing is freed or captured until the session's
//!   stream is synchronized;
//! - a device-to-device copy of a whole computed tensor holds that tensor;
//!   a partial copy holds nothing;
//! - every object a call takes a handle for (a workspace, state, network
//!   operator or expectation) belongs to the handle it was created
//!   with, and is used only with that handle, which must be live; a handle
//!   is destroyed only once nothing of it is, so a state moved to a fresh
//!   handle is caught using the old one, and a handle released too early.
//!
//! It records the buffers lent to workspaces as device scratch, so a test
//! can see how many are allocated during a compute and after it. A
//! workspace asks for [`WORKSPACE_BYTES`] of device scratch and nothing in
//! its other slots, unless told other sizes. The tensor API's rules are in
//! `fake_libraries/tensor_calls.rs`.
//!
//! A compute truncates as an MPS of the native capacity's bonds must: at
//! each cut, left to right, whose Schmidt rank exceeds the capacity's
//! bond, it projects the sites left of the cut on the eigenvectors of their
//! reduced density matrix with the bond's largest eigenvalues, the best
//! truncation of that cut, and does not renormalize, as cuTensorNet under
//! `SvdNormalizationNone`. Its chain is then the truncated state's exact
//! one. So a truncated state's squared norm is the weight its truncations
//! kept, an expectation returns it beside the value, and a read that skips
//! the normalization is caught. Its truncation need not be the one
//! cuTensorNet's sweep finds when several cuts truncate at once; with one,
//! both are the best. Below the bond, it drops the singular values the
//! state's configured discarded-weight cutoff drops (none if unset, as
//! cuTensorNet's default is 0). A call can be made to fail with
//! [`FakeLibraries::fail`], to test cleanup; and the device count, the
//! extents a compute reports and the values an expectation returns can be
//! set, to test what the callers do with what the library answers.

mod dense_tensor;
mod memory_calls;
mod session_calls;
mod state_calls;
mod state_vector;
mod tensor_calls;

use super::{
    OpaqueHandle,
    complex::Complex64Abi,
    memory_calls::WorkspaceSlot,
    state_calls::{StateF64Configuration, StateU32Configuration},
};
use crate::native_calls::error::NativeError;
use num_complex::Complex64;
use state_vector::StateVector;
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, MutexGuard},
};
use tensor_calls::{SizedFor, SvdConfig, TensorShape};

/// The bytes `workspace_size` asks for in the device scratch, unless
/// [`FakeLibraries::set_workspace_bytes`] says otherwise.
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
    failure: Option<(&'static str, usize)>,
    /// Every operator applied, on any state: its modes and unitary flag.
    applied: Vec<(Vec<i32>, bool)>,
    /// Every buffer ever attached to a workspace as scratch.
    scratch: Vec<usize>,
    /// Whether a copy may still be running on the legacy stream: set by a
    /// copy, cleared by `synchronize_copies`.
    copies_running: bool,
    /// Whether the session's stream may still be running work: set by a
    /// compute or a tensor call, cleared by `synchronize_stream`.
    stream_busy: bool,
    /// The destination of every device-to-device copy.
    copy_destinations: Vec<usize>,
    /// The sources of the device-to-device copies that may still be
    /// running, which must not be freed.
    copy_sources: Vec<usize>,
    /// The squared norm every expectation reports, if not the state's.
    squared_norm: Option<Complex64>,
    /// For each state compute, how many scratch buffers were allocated and
    /// the size of the one attached.
    computes: Vec<(usize, usize)>,
    /// The free device memory `memory_info` reports, if not 1 TiB.
    free_memory: Option<usize>,
    /// The device scratch `workspace_size` asks for, if not
    /// [`WORKSPACE_BYTES`].
    workspace_bytes: Option<i64>,
    /// The bytes `workspace_size` asks for in each other slot: the device
    /// cache and both host slots.
    other_slot_bytes: i64,
    /// The computed MPS each buffer holds, as a compute wrote it or a copy
    /// copied it: the compute's number, and the site.
    mps_tensors: BTreeMap<usize, (usize, usize)>,
    /// The state each compute computed, by its number.
    computed_states: BTreeMap<usize, StateVector>,
    /// The limit each state prepare was given.
    prepare_limits: Vec<usize>,
    /// Each 32-bit configuration, in order, and the state it was set on.
    u32_configurations: Vec<(usize, StateU32Configuration)>,
    /// Each f64 configuration, in order.
    f64_configurations: Vec<StateF64Configuration>,
    /// The handle each live object that takes one was created with.
    owners: BTreeMap<usize, usize>,
    /// The device count `device_count` reports, if not 1.
    device_count: Option<i32>,
    /// The bonds each `finalize_mps` was given, in order.
    finalized_bonds: Vec<Vec<usize>>,
    /// The hyper-samples each expectation was configured with, in order.
    hyper_samples: Vec<i32>,
    /// The extents every compute reports for a site, by site, instead of
    /// the computed state's.
    misreported_extents: BTreeMap<usize, Vec<i64>>,
    /// The values the next expectations return, in order, instead of the
    /// computed state's.
    expectation_values: VecDeque<Complex64>,
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
    State(FakeState),
    Operator(Vec<(Complex64, Vec<(usize, usize)>)>),
    Expectation {
        state: usize,
        operator: usize,
        prepared: bool,
    },
    TensorDescriptor(TensorShape),
    SvdConfig(SvdConfig),
    /// An SVD info, and what the last gate split given it truncated.
    SvdInfo(Option<super::tensor_calls::SvdTruncation>),
    /// A contraction network: the info whose path it was last optimized
    /// to, and whether it is prepared along that path.
    Network {
        inputs: [TensorShape; 2],
        output: TensorShape,
        path: Option<usize>,
        prepared: bool,
    },
    OptimizerInfo {
        network: usize,
        optimized: bool,
    },
}

struct FakeState {
    sites: usize,
    /// The start of the next compute, once a compute is captured: the
    /// captured state and the output buffers it is read from.
    start: Option<Computed>,
    /// The operators applied since the start.
    operators: Vec<(Vec<usize>, usize)>,
    /// The native capacity's bonds, once finalized.
    bonds: Option<Vec<usize>>,
    configurations: usize,
    /// The discarded-weight cutoff each compute truncates at: the one the
    /// state was configured with, or cuTensorNet's default, 0.
    discarded_weight_cutoff: f64,
    /// How many operators the state had at its last prepare.
    prepared_at: Option<usize>,
    /// The last compute, and how many operators the state had then.
    computed: Option<(Computed, usize)>,
}

/// A computed state and the output buffers the library wrote it to.
#[derive(Clone)]
struct Computed {
    state: StateVector,
    outputs: Vec<usize>,
}

fn failed(operation: &'static str, message: &str) -> NativeError {
    NativeError::NativeCallFailed {
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

fn to_usize(value: i64) -> Result<usize, NativeError> {
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

    /// Makes the `occurrence`-th call (from 1) to `operation` fail.
    pub(crate) fn fail(&self, operation: &'static str, occurrence: usize) {
        self.lock().failure = Some((operation, occurrence));
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

    /// The modes and unitary flag of every operator applied.
    pub(crate) fn applied(&self) -> Vec<(Vec<i32>, bool)> {
        self.lock().applied.clone()
    }

    /// How many scratch buffers are allocated now.
    pub(crate) fn live_scratch(&self) -> usize {
        self.lock().live_scratch()
    }

    /// How many buffers were ever attached to a workspace as scratch.
    pub(crate) fn scratch_buffers(&self) -> usize {
        self.lock().scratch.len()
    }

    /// For each state compute, how many scratch buffers were allocated
    /// during it, and the bytes of the one attached.
    pub(crate) fn computes(&self) -> Vec<(usize, usize)> {
        self.lock().computes.clone()
    }

    /// Makes `memory_info` report `bytes` free.
    pub(crate) fn set_free_memory(&self, bytes: usize) {
        self.lock().free_memory = Some(bytes);
    }

    /// Makes every expectation report `norm` as the squared norm.
    pub(crate) fn set_squared_norm(&self, norm: Complex64) {
        self.lock().squared_norm = Some(norm);
    }

    /// How many device-to-device copies are still allocated.
    pub(crate) fn live_copies(&self) -> usize {
        let inner = self.lock();
        inner
            .copy_destinations
            .iter()
            .filter(|copy| inner.objects.contains_key(copy))
            .count()
    }

    /// Makes `device_count` report `count` devices.
    pub(crate) fn set_device_count(&self, count: i32) {
        self.lock().device_count = Some(count);
    }

    /// The bonds each `finalize_mps` was given, in order: a cut's bond for
    /// each cut.
    pub(crate) fn finalized_bonds(&self) -> Vec<Vec<usize>> {
        self.lock().finalized_bonds.clone()
    }

    /// The hyper-samples each expectation was configured with, in order.
    pub(crate) fn hyper_samples(&self) -> Vec<i32> {
        self.lock().hyper_samples.clone()
    }

    /// Makes every compute from now on report `extents` for `site`, as a
    /// library that realized another chain would; the strides follow them.
    pub(crate) fn misreport_extents(&self, site: usize, extents: &[i64]) {
        self.lock()
            .misreported_extents
            .insert(site, extents.to_vec());
    }

    /// Makes the next expectations return `values`, one each, in order, in
    /// place of the computed state's; the squared norm is unchanged.
    pub(crate) fn queue_expectation_values(&self, values: &[Complex64]) {
        self.lock().expectation_values.extend(values);
    }

    /// Makes every workspace ask for `bytes` of device scratch from now on.
    pub(crate) fn set_workspace_bytes(&self, bytes: i64) {
        self.lock().workspace_bytes = Some(bytes);
    }

    /// Makes every workspace ask for `bytes` in each of its other slots
    /// (the device cache and both host slots) from now on, instead of
    /// none.
    pub(crate) fn set_other_workspace_slot_bytes(&self, bytes: i64) {
        self.lock().other_slot_bytes = bytes;
    }

    /// The f64 configurations of every state, in order.
    pub(crate) fn f64_configurations(&self) -> Vec<StateF64Configuration> {
        self.lock().f64_configurations.clone()
    }

    /// The limit each state prepare was given, in order.
    pub(crate) fn prepare_limits(&self) -> Vec<usize> {
        self.lock().prepare_limits.clone()
    }

    /// The 32-bit configurations of each native state, in the order the
    /// states were first configured: a restarted state's come after its
    /// predecessor's.
    pub(crate) fn u32_configurations(&self) -> Vec<Vec<StateU32Configuration>> {
        let inner = self.lock();
        let mut states: Vec<(usize, Vec<StateU32Configuration>)> = Vec::new();
        for &(state, configuration) in &inner.u32_configurations {
            match states.iter_mut().find(|(id, _)| *id == state) {
                Some((_, configurations)) => configurations.push(configuration),
                None => states.push((state, vec![configuration])),
            }
        }
        states
            .into_iter()
            .map(|(_, configurations)| configurations)
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records a call to `operation` and fails it if asked to.
    fn call(&self, operation: &'static str) -> Result<MutexGuard<'_, Inner>, NativeError> {
        let mut inner = self.lock();
        inner.events.push(operation);
        let count = inner.calls.entry(operation).or_insert(0);
        *count += 1;
        let count = *count;
        if inner.failure == Some((operation, count)) {
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
    ) -> Result<usize, NativeError> {
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

    /// The bytes every workspace asks for in `slot`.
    fn recommended_bytes(&self, slot: WorkspaceSlot) -> i64 {
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
    ) -> Result<OpaqueHandle, NativeError> {
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
    ) -> Result<(), NativeError> {
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
    ) -> Result<(), NativeError> {
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
    ) -> Result<(), NativeError> {
        match self.objects.get(&id(handle)) {
            Some(object) if is_kind(object) => Ok(()),
            _ => Err(failed(operation, "no such object")),
        }
    }

    fn buffer(&self, id: usize, operation: &'static str) -> Result<&[Complex64Abi], NativeError> {
        match self.objects.get(&id) {
            Some(Object::Buffer(buffer)) => Ok(buffer),
            _ => Err(failed(operation, "no such buffer")),
        }
    }

    fn state(
        &mut self,
        handle: OpaqueHandle,
        operation: &'static str,
    ) -> Result<&mut FakeState, NativeError> {
        match self.objects.get_mut(&id(handle)) {
            Some(Object::State(state)) => Ok(state),
            _ => Err(failed(operation, "no such state")),
        }
    }

    /// The state `state` was last computed as, if no operator was applied
    /// since and its output buffers are still allocated.
    fn computed(&self, state: usize, operation: &'static str) -> Result<&StateVector, NativeError> {
        match self.objects.get(&state) {
            Some(Object::State(FakeState {
                operators,
                computed: Some((computed, at)),
                ..
            })) if *at == operators.len() => {
                self.outputs_live(computed, operation)?;
                Ok(&computed.state)
            }
            Some(Object::State(_)) => Err(failed(operation, "the state is not computed")),
            _ => Err(failed(operation, "no such state")),
        }
    }

    /// Fails unless every output buffer `computed` is read from is still
    /// allocated.
    fn outputs_live(
        &self,
        computed: &Computed,
        operation: &'static str,
    ) -> Result<(), NativeError> {
        for &output in &computed.outputs {
            self.buffer(output, operation)
                .map_err(|_| failed(operation, "a computed MPS's output buffer was freed"))?;
        }
        Ok(())
    }

    /// A copy of `state` with the matrix in buffer `tensor` applied to
    /// `sites`.
    fn apply(
        &self,
        state: &mut StateVector,
        sites: &[usize],
        tensor: usize,
        operation: &'static str,
    ) -> Result<(), NativeError> {
        let elements = 1 << (2 * sites.len());
        let matrix: Vec<Complex64> = self
            .buffer(tensor, operation)?
            .get(..elements)
            .ok_or_else(|| failed(operation, "the operator buffer is too small"))?
            .iter()
            .copied()
            .map(Complex64::from)
            .collect();
        state.apply(sites, &matrix);
        Ok(())
    }
}
