// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeLibraries`]: cuTensorNet and the CUDA Runtime, simulated on the CPU
//! for the engine's tests.
//!
//! CI has no GPU, so the engine's real code (every call it makes, in the
//! order it makes them) runs against these fake libraries, which implement
//! `NativeCalls` as the loaded libraries do, one file per trait
//! (`fake_libraries/session_calls.rs`, `fake_libraries/memory_calls.rs`,
//! `fake_libraries/state_calls.rs`). It secures what a GPU run would
//! otherwise be the first to show: a call out of cuTensorNet's order,
//! memory freed while still in use, a read of a stale state. It is test
//! support, not a test: it sits beside the traits it implements and the
//! loaded libraries it stands in for, so the engine's modules stay private
//! to the engine, and it is compiled only with the tests, which live under
//! `src/tests`. This file holds what the three traits' files share: the
//! objects the fake keeps, the checks every call goes through, and what a
//! test sets and reads.
//!
//! It keeps what cuTensorNet would keep, by id: buffers, workspaces, states
//! with the operators applied to them, network operators and expectations. A
//! compute replays a state's operators on a state vector
//! (`fake_libraries/state_vector.rs`), reading each matrix from its buffer at
//! that moment, as the library reads it at compute time; reads use the computed
//! state. It is strict about the order the engine calls in, so a wrong order
//! fails a test instead of passing silently:
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
//! - a buffer attached to a live workspace as scratch is never freed: the
//!   descriptor would hold freed memory;
//! - a compute or an expectation needs the scratch its workspace asked
//!   for, attached, and no copy still running (`synchronize_copies`, the
//!   legacy stream), and the source of a copy still running is not freed;
//!   after a compute, nothing is freed or captured until the engine's
//!   stream is synchronized;
//! - a device-to-device copy of a whole computed tensor holds that tensor;
//!   a partial copy holds nothing;
//! - every object a call takes a handle for (a workspace, state, network
//!   operator or expectation) belongs to the handle it was created
//!   with, and is used only with that handle, which must be live; a handle
//!   is destroyed only once nothing of it is, so a state moved to a fresh
//!   handle is caught using the old one, and a handle released too early.
//!
//! It records the scratch buffers attached to workspaces, so a test can see
//! how many are allocated during a compute and after it, and asks for
//! [`WORKSPACE_BYTES`] of scratch unless told another size.
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
//! engine's discarded-weight cutoff drops. A call can be made to fail with
//! [`FakeLibraries::fail`], to test cleanup; and the device count, the
//! extents a compute reports and the values an expectation returns can be
//! set, to test what the engine does with what the library answers.

mod memory_calls;
mod session_calls;
mod state_calls;
mod state_vector;

use super::{
    OpaqueHandle,
    complex::Complex64Abi,
    state_calls::{StateF64Configuration, StateU32Configuration},
};
use crate::engine::EngineError;
use num_complex::Complex64;
use state_vector::StateVector;
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, MutexGuard},
};

/// The bytes `workspace_memory_size` asks for, unless
/// [`FakeLibraries::set_workspace_bytes`] says otherwise.
pub(crate) const WORKSPACE_BYTES: i64 = 1024;

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
    /// Whether the engine's stream may still be running a compute: set by
    /// `compute_state`, cleared by `synchronize_stream`.
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
    /// The scratch `workspace_size` asks for, if not [`WORKSPACE_BYTES`].
    workspace_bytes: Option<i64>,
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
    /// A workspace descriptor, and the scratch attached to it and the
    /// bytes it lends.
    Workspace {
        scratch: Option<(usize, i64)>,
    },
    State(FakeState),
    Operator(Vec<(Complex64, Vec<(usize, usize)>)>),
    /// Development only: a network operator made of one MPO, its sites in
    /// order and each site's tensor buffer and extents.
    Mpo {
        sites: Vec<usize>,
        tensors: Vec<(usize, Box<[i64]>)>,
    },
    Expectation {
        state: usize,
        operator: usize,
        prepared: bool,
    },
}

struct FakeState {
    sites: usize,
    /// The start of the next compute, once a compute is captured: the
    /// captured state and the output buffers it is read from.
    start: Option<Computed>,
    /// The operators applied since the start.
    operators: Vec<FakeOperator>,
    /// The native capacity's bonds, once finalized.
    bonds: Option<Vec<usize>>,
    configurations: usize,
    /// How many operators the state had at its last prepare.
    prepared_at: Option<usize>,
    /// The last compute, and how many operators the state had then.
    computed: Option<(Computed, usize)>,
}

/// An operator applied to a state, read when the state is computed.
#[derive(Clone)]
enum FakeOperator {
    /// A tensor operator: its sites and its buffer.
    Tensor(Vec<usize>, usize),
    /// Development only: a network operator made of one MPO.
    Mpo(usize),
}

/// A computed state and the output buffers the library wrote it to.
#[derive(Clone)]
struct Computed {
    state: StateVector,
    outputs: Vec<usize>,
}

fn failed(operation: &'static str, message: &str) -> EngineError {
    EngineError::NativeCallFailed {
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

fn to_usize(value: i64) -> Result<usize, EngineError> {
    usize::try_from(value).map_err(|_| failed("conversion", "negative value"))
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

    /// Makes every workspace ask for `bytes` of scratch from now on.
    pub(crate) fn set_workspace_bytes(&self, bytes: i64) {
        self.lock().workspace_bytes = Some(bytes);
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
    fn call(&self, operation: &'static str) -> Result<MutexGuard<'_, Inner>, EngineError> {
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
    /// `workspace` lending live scratch of at least the size it asked for.
    /// Returns the bytes of the scratch buffer.
    fn lent_scratch(
        &self,
        workspace: OpaqueHandle,
        operation: &'static str,
    ) -> Result<usize, EngineError> {
        if self.copies_running {
            return Err(failed(operation, "a copy may still be running"));
        }
        let Some(Object::Workspace {
            scratch: Some((scratch, lent)),
        }) = self.objects.get(&id(workspace))
        else {
            return Err(failed(operation, "no scratch attached"));
        };
        if *lent < self.workspace_bytes.unwrap_or(WORKSPACE_BYTES) {
            return Err(failed(operation, "less scratch than asked for"));
        }
        Ok(size_of_val(self.buffer(*scratch, operation)?))
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
    ) -> Result<OpaqueHandle, EngineError> {
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
    ) -> Result<(), EngineError> {
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
    ) -> Result<(), EngineError> {
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
    ) -> Result<(), EngineError> {
        match self.objects.get(&id(handle)) {
            Some(object) if is_kind(object) => Ok(()),
            _ => Err(failed(operation, "no such object")),
        }
    }

    fn buffer(&self, id: usize, operation: &'static str) -> Result<&[Complex64Abi], EngineError> {
        match self.objects.get(&id) {
            Some(Object::Buffer(buffer)) => Ok(buffer),
            _ => Err(failed(operation, "no such buffer")),
        }
    }

    fn state(
        &mut self,
        handle: OpaqueHandle,
        operation: &'static str,
    ) -> Result<&mut FakeState, EngineError> {
        match self.objects.get_mut(&id(handle)) {
            Some(Object::State(state)) => Ok(state),
            _ => Err(failed(operation, "no such state")),
        }
    }

    /// The state `state` was last computed as, if no operator was applied
    /// since and its output buffers are still allocated.
    fn computed(&self, state: usize, operation: &'static str) -> Result<&StateVector, EngineError> {
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
    ) -> Result<(), EngineError> {
        for &output in &computed.outputs {
            self.buffer(output, operation)
                .map_err(|_| failed(operation, "a computed MPS's output buffer was freed"))?;
        }
        Ok(())
    }

    /// `state` with `operator` applied, its tensors read now.
    fn apply(
        &self,
        state: &mut StateVector,
        operator: &FakeOperator,
        operation: &'static str,
    ) -> Result<(), EngineError> {
        match operator {
            FakeOperator::Tensor(sites, tensor) => {
                let elements = 1 << (2 * sites.len());
                let matrix: Vec<Complex64> = self
                    .buffer(*tensor, operation)?
                    .get(..elements)
                    .ok_or_else(|| failed(operation, "the operator buffer is too small"))?
                    .iter()
                    .copied()
                    .map(Complex64::from)
                    .collect();
                state.apply(sites, &matrix);
            }
            FakeOperator::Mpo(operator) => {
                let Some(Object::Mpo { sites, tensors }) = self.objects.get(operator) else {
                    return Err(failed(operation, "an applied MPO was destroyed"));
                };
                let matrix = self.contract_mpo(tensors, operation)?;
                state.apply(sites, &matrix);
            }
        }
        Ok(())
    }

    /// The dense matrix (row-major, the first site the most significant
    /// bit) of an open-boundary MPO whose tensors have cuTensorNet's modes
    /// `(previous bond, ket, next bond, bra)`, column-major, the first
    /// without its previous bond and the last without its next.
    fn contract_mpo(
        &self,
        tensors: &[(usize, Box<[i64]>)],
        operation: &'static str,
    ) -> Result<Vec<Complex64>, EngineError> {
        let last = tensors.len() - 1;
        // partial[(o, i, bond)] at o + dim·(i + dim·bond), dim = 2^sites.
        let (mut partial, mut dim, mut bond) = (vec![Complex64::ONE], 1, 1);
        for (at, (tensor, extents)) in tensors.iter().enumerate() {
            let extents = extents
                .iter()
                .map(|&extent| to_usize(extent))
                .collect::<Result<Vec<_>, _>>()?;
            let (left, ket, right, bra) = match (at, extents.as_slice()) {
                (0, &[ket, right, bra]) => (1, ket, right, bra),
                (at, &[left, ket, bra]) if at == last => (left, ket, 1, bra),
                (_, &[left, ket, right, bra]) => (left, ket, right, bra),
                _ => return Err(failed(operation, "an MPO tensor of the wrong rank")),
            };
            if left != bond || ket != 2 || bra != 2 {
                return Err(failed(operation, "an MPO tensor's extents do not chain"));
            }
            let elements = self.buffer(*tensor, operation)?;
            if elements.len() < left * 4 * right {
                return Err(failed(operation, "an MPO tensor's buffer is too small"));
            }
            let next_dim = 2 * dim;
            let mut next = vec![Complex64::ZERO; next_dim * next_dim * right];
            for (kl, kr, input, out) in (0..left).flat_map(|kl| {
                (0..right).flat_map(move |kr| {
                    (0..2).flat_map(move |input| (0..2).map(move |out| (kl, kr, input, out)))
                })
            }) {
                let w = Complex64::from(elements[kl + left * (input + 2 * (kr + right * out))]);
                for (o, i) in (0..dim).flat_map(|o| (0..dim).map(move |i| (o, i))) {
                    let to = (2 * o + out) + next_dim * ((2 * i + input) + next_dim * kr);
                    next[to] += partial[o + dim * (i + dim * kl)] * w;
                }
            }
            (partial, dim, bond) = (next, next_dim, right);
        }
        Ok((0..dim)
            .flat_map(|o| (0..dim).map(move |i| (o, i)))
            .map(|(o, i)| partial[o + dim * i])
            .collect())
    }
}
