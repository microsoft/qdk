// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NativeDouble`]: cuTensorNet and the CUDA Runtime, simulated on the CPU
//! for the engine's tests.
//!
//! CI has no GPU, so the engine's real code (every call it makes, in the
//! order it makes them) runs against this double, which implements the
//! same native traits as the loaded libraries. It secures what a GPU run
//! would otherwise be the first to show: a call out of cuTensorNet's order,
//! memory freed while still in use, a read of a stale state. It is test
//! support, not a test: it sits in the engine's module, beside what it
//! doubles, so the engine's modules stay private to the engine, and it is
//! compiled only with the tests, which live under `src/tests`.
//!
//! It keeps what cuTensorNet would keep, by id: buffers, workspaces, states
//! with the operators applied to them, network operators and expectations.
//! A compute replays a state's operators on a state vector, reading each
//! matrix from its buffer at that moment, as the library reads it at
//! compute time; reads use the computed state. It is strict about the order
//! the engine calls in, so a wrong order fails a test instead of passing
//! silently:
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
//! truncation of that
//! cut, and does not renormalize, as cuTensorNet under
//! `SvdNormalizationNone`. Its chain is then the truncated state's exact
//! one. So a truncated state's squared norm is the weight its truncations
//! kept, an expectation returns it beside the value, and a read that skips
//! the normalization is caught. Its truncation need not be the one
//! cuTensorNet's sweep finds when several cuts truncate at once; with one,
//! both are the best. Below the bond, it drops the singular values the
//! engine's discarded-weight cutoff drops. A call can be made to fail with
//! [`NativeDouble::fail`], to test cleanup.

use super::{
    EngineError, OpaqueHandle, Stream,
    ffi::Complex64Abi,
    memory_calls::MemoryCalls,
    state_calls::{MpsLayout, NativeCapacity, StateCalls},
};
use num_complex::Complex64;
use std::{
    collections::BTreeMap,
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, MutexGuard},
};
use tensornet::MpsShape;

// What the tests under `src/tests` name of the engine's private modules,
// which this module, inside the engine, can reach: the session, the
// configurations the engine sets, and the settings they check it keeps.
pub(crate) use super::{
    session::{DeviceSession, SessionCalls},
    state_calls::{StateF64Configuration, StateU32Configuration},
};
/// The engine's discarded-weight cutoff (`policy.rs`).
pub(crate) const DISCARDED_WEIGHT_CUTOFF: f64 = super::policy::DISCARDED_WEIGHT_CUTOFF;
/// The engine's restart period (`policy.rs`).
pub(crate) const RESTART_PERIOD: usize = super::policy::RESTART_PERIOD;

/// The bytes `workspace_memory_size` asks for, unless
/// [`NativeDouble::set_workspace_bytes`] says otherwise.
pub(crate) const WORKSPACE_BYTES: i64 = 1024;

#[derive(Default)]
pub(crate) struct NativeDouble {
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
    computed_states: BTreeMap<usize, Dense>,
    /// The limit each state prepare was given.
    prepare_limits: Vec<usize>,
    /// Each 32-bit configuration, in order, and the state it was set on.
    u32_configurations: Vec<(usize, StateU32Configuration)>,
    /// Each f64 configuration, in order.
    f64_configurations: Vec<StateF64Configuration>,
    /// The handle each live object that takes one was created with.
    owners: BTreeMap<usize, usize>,
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
    State(StateDouble),
    Operator(Vec<(Complex64, Vec<(usize, usize)>)>),
    Expectation {
        state: usize,
        operator: usize,
        prepared: bool,
    },
}

struct StateDouble {
    sites: usize,
    /// The start of the next compute, once a compute is captured: the
    /// captured state and the output buffers it is read from.
    start: Option<Computed>,
    /// The operators applied since the start.
    operators: Vec<(Vec<usize>, usize)>,
    /// The native capacity's bonds, once finalized.
    bonds: Option<Vec<usize>>,
    configurations: usize,
    /// How many operators the state had at its last prepare.
    prepared_at: Option<usize>,
    /// The last compute, and how many operators the state had then.
    computed: Option<(Computed, usize)>,
}

/// A computed state and the output buffers the library wrote it to.
#[derive(Clone)]
struct Computed {
    state: Dense,
    outputs: Vec<usize>,
}

/// A state vector of qubits, site `k` the bit `k` of the index, that the
/// double computes with. Unlike the contract's reference, it applies any
/// matrix: the projections, the truncations and the observables' factors.
#[derive(Clone)]
struct Dense {
    sites: usize,
    amplitudes: Vec<Complex64>,
}

impl Dense {
    fn zero(sites: usize) -> Self {
        let mut amplitudes = vec![Complex64::new(0.0, 0.0); 1 << sites];
        amplitudes[0] = Complex64::new(1.0, 0.0);
        Self { sites, amplitudes }
    }

    /// ψ ← (M ⊗ I)ψ for the row-major `matrix` on `sites`, `sites[0]` the
    /// most significant bit of its index, as `MpsEngine::apply` reads it.
    fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) {
        let (count, dim) = (sites.len(), 1 << sites.len());
        let mask: usize = sites.iter().map(|&site| 1 << site).sum();
        let index = |base: usize, local: usize| {
            sites.iter().enumerate().fold(base, |index, (at, &site)| {
                if (local >> (count - 1 - at)) & 1 == 1 {
                    index | (1 << site)
                } else {
                    index
                }
            })
        };
        for base in (0..self.amplitudes.len()).filter(|base| base & mask == 0) {
            let old: Vec<Complex64> = (0..dim).map(|l| self.amplitudes[index(base, l)]).collect();
            for out in 0..dim {
                self.amplitudes[index(base, out)] =
                    (0..dim).map(|l| matrix[out * dim + l] * old[l]).sum();
            }
        }
    }

    fn inner(&self, ket: &Self) -> Complex64 {
        self.amplitudes
            .iter()
            .zip(&ket.amplitudes)
            .map(|(bra, ket)| bra.conj() * ket)
            .sum()
    }

    /// The reduced density matrix of sites `0..=cut`, row-major:
    /// `ρ[a][b] = Σ_c ψ(a + c·2^(cut+1))·conj(ψ(b + c·2^(cut+1)))`.
    fn left_density(&self, cut: usize) -> (Vec<Complex64>, usize) {
        let left = 1_usize << (cut + 1);
        let right = 1_usize << (self.sites - cut - 1);
        let mut density = vec![Complex64::new(0.0, 0.0); left * left];
        for a in 0..left {
            for b in 0..left {
                density[a * left + b] = (0..right)
                    .map(|c| self.amplitudes[a + c * left] * self.amplitudes[b + c * left].conj())
                    .sum();
            }
        }
        (density, left)
    }

    /// How many squared Schmidt values at the cut after site `cut` exceed
    /// `fraction` of their total.
    fn rank_above(&self, cut: usize, fraction: f64) -> usize {
        let (density, left) = self.left_density(cut);
        let (values, _) = hermitian_eigen(density, left);
        let total: f64 = values.iter().sum();
        values
            .iter()
            .filter(|&&value| value > fraction * total)
            .count()
    }

    /// How many Schmidt values cuTensorNet keeps at the cut after site
    /// `cut`, before the bond: all but the smallest whose squares sum to at
    /// most the engine's `DISCARDED_WEIGHT_CUTOFF` of their total, which
    /// also drops the rounding a projection leaves.
    fn kept_rank(&self, cut: usize) -> usize {
        let (density, left) = self.left_density(cut);
        let (mut values, _) = hermitian_eigen(density, left);
        values.sort_by(f64::total_cmp);
        let total: f64 = values.iter().sum();
        let mut discarded = 0.0;
        let dropped = values
            .iter()
            .take_while(|&&value| {
                discarded += value;
                discarded <= DISCARDED_WEIGHT_CUTOFF * total
            })
            .count();
        (left - dropped).max(1)
    }
}

fn failed(operation: &'static str, message: &str) -> EngineError {
    EngineError::NativeCallFailed {
        component: "test double",
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

impl NativeDouble {
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
    ) -> Result<&mut StateDouble, EngineError> {
        match self.objects.get_mut(&id(handle)) {
            Some(Object::State(state)) => Ok(state),
            _ => Err(failed(operation, "no such state")),
        }
    }

    /// The state `state` was last computed as, if no operator was applied
    /// since and its output buffers are still allocated.
    fn computed(&self, state: usize, operation: &'static str) -> Result<&Dense, EngineError> {
        match self.objects.get(&state) {
            Some(Object::State(StateDouble {
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

    /// A copy of `state` with the matrix in buffer `tensor` applied to
    /// `sites`.
    fn apply(
        &self,
        state: &mut Dense,
        sites: &[usize],
        tensor: usize,
        operation: &'static str,
    ) -> Result<(), EngineError> {
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

/// Keeps the `bond` largest Schmidt values of `state` across the cut after
/// site `cut`: projects sites `0..=cut` on the eigenvectors of their reduced
/// density matrix `ρ[a][b] = Σ_c ψ(a + c·2^(cut+1))·conj(ψ(b + c·2^(cut+1)))`
/// with the `bond` largest eigenvalues, without renormalizing.
fn truncate(state: &mut Dense, cut: usize, bond: usize) {
    let (density, left) = state.left_density(cut);
    let (values, vectors) = hermitian_eigen(density, left);
    let mut order: Vec<usize> = (0..left).collect();
    order.sort_by(|&x, &y| values[y].total_cmp(&values[x]));
    let kept = &order[..bond.min(left)];
    let projector: Vec<Complex64> = (0..left * left)
        .map(|at| {
            let (a, b) = (at / left, at % left);
            kept.iter()
                .map(|&j| vectors[a * left + j] * vectors[b * left + j].conj())
                .sum()
        })
        .collect();
    // `apply` reads its first site as the most significant bit of the
    // local index, so site `k` is bit `k` with the sites from `cut` down.
    let sites: Vec<usize> = (0..=cut).rev().collect();
    state.apply(&sites, &projector);
}

/// The eigenvalues of the Hermitian `order`×`order` row-major `matrix` and its
/// eigenvectors, as the columns of a row-major matrix, by cyclic Jacobi
/// rotations: each rotation zeroes one off-diagonal element, and sweeps
/// repeat until the off-diagonal weight is rounding.
fn hermitian_eigen(mut matrix: Vec<Complex64>, order: usize) -> (Vec<f64>, Vec<Complex64>) {
    let zero = Complex64::new(0.0, 0.0);
    let mut vectors: Vec<Complex64> = (0..order * order)
        .map(|at| {
            if at / order == at % order {
                Complex64::new(1.0, 0.0)
            } else {
                zero
            }
        })
        .collect();
    let scale: f64 = matrix.iter().map(Complex64::norm_sqr).sum();
    for _ in 0..64 {
        let off: f64 = (0..order * order)
            .filter(|at| at / order != at % order)
            .map(|at| matrix[at].norm_sqr())
            .sum();
        if off <= 1e-30 * scale.max(f64::MIN_POSITIVE) {
            break;
        }
        for p in 0..order {
            for q in p + 1..order {
                let element = matrix[p * order + q];
                let magnitude = element.norm();
                if magnitude == 0.0 {
                    continue;
                }
                // G = diag(1, conj(phase)) · [[cos, sin], [−sin, cos]] on
                // (p, q) makes the element real, then zeroes it (Numerical
                // Recipes' rotation, ch. 11.1).
                let phase = element / magnitude;
                let tau = (matrix[q * order + q].re - matrix[p * order + p].re) / (2.0 * magnitude);
                let tangent = tau.signum() / (tau.abs() + tau.mul_add(tau, 1.0).sqrt());
                let cosine = 1.0 / tangent.mul_add(tangent, 1.0).sqrt();
                let sine = tangent * cosine;
                let (gpq, gqp, gqq) = (
                    Complex64::new(sine, 0.0),
                    -phase.conj() * sine,
                    phase.conj() * cosine,
                );
                for k in 0..order {
                    for m in [&mut matrix, &mut vectors] {
                        let (kp, kq) = (m[k * order + p], m[k * order + q]);
                        m[k * order + p] = kp * cosine + kq * gqp;
                        m[k * order + q] = kp * gpq + kq * gqq;
                    }
                }
                for k in 0..order {
                    let (pk, qk) = (matrix[p * order + k], matrix[q * order + k]);
                    matrix[p * order + k] = pk * cosine + qk * gqp.conj();
                    matrix[q * order + k] = pk * gpq.conj() + qk * gqq.conj();
                }
            }
        }
    }
    (
        (0..order).map(|i| matrix[i * order + i].re).collect(),
        vectors,
    )
}

impl SessionCalls for NativeDouble {
    fn device_count(&self) -> Result<i32, EngineError> {
        drop(self.call("device_count")?);
        Ok(1)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        drop(self.call("set_device")?);
        if ordinal == 0 {
            Ok(())
        } else {
            Err(failed("set_device", "no such device"))
        }
    }

    fn create_stream(&self) -> Result<Stream, EngineError> {
        Ok(self.call("create_stream")?.create(Object::Stream))
    }

    fn synchronize_stream(&self, stream: Stream) -> Result<(), EngineError> {
        let mut inner = self.call("synchronize_stream")?;
        inner.require(stream, "synchronize_stream", |o| {
            matches!(o, Object::Stream)
        })?;
        inner.stream_busy = false;
        Ok(())
    }

    fn destroy_stream(&self, stream: Stream) -> Result<(), EngineError> {
        self.call("destroy_stream")?
            .destroy(stream, "destroy_stream", |o| matches!(o, Object::Stream))
    }

    fn create_handle(&self) -> Result<OpaqueHandle, EngineError> {
        Ok(self.call("create_handle")?.create(Object::Handle))
    }

    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), EngineError> {
        let mut inner = self.call("destroy_handle")?;
        if inner.owners.values().any(|&owner| owner == id(handle)) {
            return Err(failed(
                "destroy_handle",
                "an object of the handle is still live",
            ));
        }
        inner.destroy(handle, "destroy_handle", |o| matches!(o, Object::Handle))
    }
}

impl MemoryCalls for NativeDouble {
    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        let inner = self.call("memory_info")?;
        Ok((inner.free_memory.unwrap_or(1 << 40), 1 << 40))
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, EngineError> {
        let elements = bytes.div_ceil(size_of::<Complex64Abi>());
        Ok(self
            .call("allocate")?
            .create(Object::Buffer(vec![Complex64Abi::default(); elements])))
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), EngineError> {
        let mut inner = self.call("free")?;
        if inner.stream_busy {
            return Err(failed("free", "the stream may still be using the buffer"));
        }
        if inner.copy_sources.contains(&id(allocation)) {
            return Err(failed("free", "a copy may still be reading the buffer"));
        }
        if inner.objects.values().any(|object| {
            matches!(
                object,
                Object::Workspace { scratch: Some((scratch, _)) } if *scratch == id(allocation)
            )
        }) {
            return Err(failed("free", "the buffer is attached to a live workspace"));
        }
        inner.mps_tensors.remove(&id(allocation));
        inner.destroy(allocation, "free", |o| matches!(o, Object::Buffer(_)))
    }

    fn copy_on_device(
        &self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("copy_on_device")?;
        let elements = bytes / size_of::<Complex64Abi>();
        let source_buffer = inner.buffer(id(source), "copy_on_device")?;
        let whole = elements == source_buffer.len();
        let copied = source_buffer
            .get(..elements)
            .ok_or_else(|| failed("copy_on_device", "the source is too small"))?
            .to_vec();
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= elements => {
                buffer[..elements].copy_from_slice(&copied);
            }
            _ => return Err(failed("copy_on_device", "no such buffer of that size")),
        }
        // Only a copy of a whole tensor holds the computed MPS's site.
        match inner.mps_tensors.get(&id(source)).copied() {
            Some(tag) if whole => inner.mps_tensors.insert(id(destination), tag),
            _ => inner.mps_tensors.remove(&id(destination)),
        };
        inner.copies_running = true;
        inner.copy_destinations.push(id(destination));
        inner.copy_sources.push(id(source));
        Ok(())
    }

    fn synchronize_copies(&self) -> Result<(), EngineError> {
        let mut inner = self.call("synchronize_copies")?;
        inner.copies_running = false;
        inner.copy_sources.clear();
        Ok(())
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), EngineError> {
        let mut inner = self.call("copy_to_device")?;
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= source.len() => {
                buffer[..source.len()].copy_from_slice(source);
            }
            _ => return Err(failed("copy_to_device", "no such buffer of that size")),
        }
        inner.copies_running = true;
        Ok(())
    }

    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError> {
        self.call("create_workspace")?.create_on(
            handle,
            "create_workspace",
            Object::Workspace { scratch: None },
        )
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_workspace")?
            .destroy(workspace, "destroy_workspace", |o| {
                matches!(o, Object::Workspace { .. })
            })
    }

    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<i64, EngineError> {
        let inner = self.call("workspace_size")?;
        inner.require(workspace, "workspace_size", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        inner.owned(handle, workspace, "workspace_size")?;
        Ok(inner.workspace_bytes.unwrap_or(WORKSPACE_BYTES))
    }

    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        scratch: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("set_workspace")?;
        inner.owned(handle, workspace, "set_workspace")?;
        let allocation = id(scratch);
        if size_of_val(inner.buffer(allocation, "set_workspace")?) < to_usize(bytes)? {
            return Err(failed(
                "set_workspace",
                "the scratch is smaller than its size",
            ));
        }
        match inner.objects.get_mut(&id(workspace)) {
            Some(Object::Workspace { scratch }) => {
                *scratch = Some((allocation, bytes));
            }
            _ => return Err(failed("set_workspace", "no such workspace")),
        }
        if !inner.scratch.contains(&allocation) {
            inner.scratch.push(allocation);
        }
        Ok(())
    }
}

impl StateCalls for NativeDouble {
    fn create_state(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_state")?;
        if mode_extents.iter().any(|&extent| extent != 2) {
            return Err(failed("create_state", "a mode is not a qubit"));
        }
        inner.create_on(
            handle,
            "create_state",
            Object::State(StateDouble {
                sites: mode_extents.len(),
                start: None,
                operators: Vec::new(),
                bonds: None,
                configurations: 0,
                prepared_at: None,
                computed: None,
            }),
        )
    }

    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_state")?
            .destroy(state, "destroy_state", |o| matches!(o, Object::State(_)))
    }

    fn apply_tensor_operator(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "apply_tensor_operator";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        inner.buffer(id(tensor), OPERATION)?;
        let sites = modes
            .iter()
            .map(|&mode| usize::try_from(mode).map_err(|_| failed(OPERATION, "negative mode")))
            .collect::<Result<Vec<_>, _>>()?;
        inner.applied.push((modes.to_vec(), unitary));
        let target = inner.state(state, OPERATION)?;
        if sites.iter().any(|&site| site >= target.sites) {
            return Err(failed(OPERATION, "a mode outside the state"));
        }
        target.operators.push((sites, id(tensor)));
        Ok(())
    }

    fn finalize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        capacity: &NativeCapacity,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("finalize_mps")?;
        inner.owned(handle, state, "finalize_mps")?;
        let state = inner.state(state, "finalize_mps")?;
        if state.bonds.is_some() {
            return Err(failed("finalize_mps", "the state is already finalized"));
        }
        let shape = capacity.native_shape();
        // As the library: MPS finalization needs at least two sites.
        if state.sites < 2 {
            return Err(failed("finalize_mps", "an MPS needs at least two sites"));
        }
        if shape.site_count() != state.sites {
            return Err(failed(
                "finalize_mps",
                "the capacity has another site count",
            ));
        }
        state.bonds = Some(
            (0..state.sites - 1)
                .map(|cut| shape.bond_dim(cut).unwrap_or(1))
                .collect(),
        );
        Ok(())
    }

    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateF64Configuration,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_f64")?;
        inner.owned(handle, state, "configure_state_f64")?;
        let state = inner.state(state, "configure_state_f64")?;
        if state.bonds.is_none() {
            return Err(failed("configure_state_f64", "configured before finalize"));
        }
        state.configurations += 1;
        inner.f64_configurations.push(configuration);
        Ok(())
    }

    fn configure_state_u32(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateU32Configuration,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_u32")?;
        inner.owned(handle, state, "configure_state_u32")?;
        let native = inner.state(state, "configure_state_u32")?;
        if native.bonds.is_none() {
            return Err(failed("configure_state_u32", "configured before finalize"));
        }
        native.configurations += 1;
        inner.u32_configurations.push((id(state), configuration));
        Ok(())
    }

    fn prepare_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_state")?;
        inner.owned(handle, state, "prepare_state")?;
        inner.owned(handle, workspace, "prepare_state")?;
        inner.prepare_limits.push(maximum_workspace_bytes);
        inner.require(workspace, "prepare_state", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        let state = inner.state(state, "prepare_state")?;
        if state.bonds.is_none() {
            return Err(failed("prepare_state", "prepared before finalize"));
        }
        state.prepared_at = Some(state.operators.len());
        Ok(())
    }

    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        layout: &mut MpsLayout,
        outputs: &mut [OpaqueHandle],
        _stream: Stream,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "compute_state";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        inner.owned(handle, workspace, OPERATION)?;
        let scratch_bytes = inner.lent_scratch(workspace, OPERATION)?;
        let live_scratch = inner.live_scratch();
        inner.computes.push((live_scratch, scratch_bytes));
        for output in outputs.iter() {
            inner.buffer(id(*output), OPERATION)?;
        }
        let (sites, start, operators, bonds) = {
            let state = inner.state(state, OPERATION)?;
            if state.prepared_at != Some(state.operators.len()) {
                return Err(failed(OPERATION, "not prepared since the last operator"));
            }
            let Some(bonds) = state.bonds.clone() else {
                return Err(failed(OPERATION, "not finalized"));
            };
            (
                state.sites,
                state.start.clone(),
                state.operators.clone(),
                bonds,
            )
        };
        if outputs.len() != sites || layout.extents.len() != sites {
            return Err(failed(OPERATION, "one output per site is needed"));
        }
        let mut computed = if let Some(start) = start {
            inner.outputs_live(&start, OPERATION)?;
            start.state
        } else {
            Dense::zero(sites)
        };
        for (sites, tensor) in &operators {
            inner.apply(&mut computed, sites, *tensor, OPERATION)?;
        }
        for (cut, &bond) in bonds.iter().enumerate() {
            // Only where a value is dropped: the eigenvectors of a nearly
            // rank-one density matrix lose an amplitude of 1e-100.
            let kept = computed.kept_rank(cut).min(bond);
            if kept < computed.rank_above(cut, 1e-20) {
                truncate(&mut computed, cut, kept);
            }
        }
        let bond = |cut: usize| -> Result<i64, EngineError> {
            let realized = computed.kept_rank(cut).min(bonds[cut]);
            i64::try_from(realized).map_err(|_| failed(OPERATION, "bond overflow"))
        };
        for site in 0..sites {
            let extents = if site == 0 {
                vec![2, bond(0)?]
            } else if site + 1 == sites {
                vec![bond(site - 1)?, 2]
            } else {
                vec![bond(site - 1)?, 2, bond(site)?]
            };
            if layout.extents[site].len() != extents.len()
                || layout.strides[site].len() != extents.len()
            {
                return Err(failed(OPERATION, "layout of the wrong rank"));
            }
            let mut stride = 1;
            for (axis, &extent) in extents.iter().enumerate() {
                layout.strides[site][axis] = stride;
                stride *= extent;
            }
            layout.extents[site].copy_from_slice(&extents);
        }
        let at = operators.len();
        let outputs: Vec<usize> = outputs.iter().map(|&output| id(output)).collect();
        let number = inner.computed_states.len();
        inner.computed_states.insert(number, computed.clone());
        for (site, &output) in outputs.iter().enumerate() {
            inner.mps_tensors.insert(output, (number, site));
        }
        inner.stream_busy = true;
        inner.state(state, OPERATION)?.computed = Some((
            Computed {
                state: computed,
                outputs,
            },
            at,
        ));
        Ok(())
    }

    fn initialize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &MpsLayout,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "initialize_mps";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        let tensors: Vec<usize> = tensors.iter().map(|&tensor| id(tensor)).collect();
        let Some(&(number, _)) = inner.mps_tensors.get(tensors.first().unwrap_or(&0)) else {
            return Err(failed(OPERATION, "the tensors are not a computed MPS"));
        };
        if tensors
            .iter()
            .enumerate()
            .any(|(site, tensor)| inner.mps_tensors.get(tensor) != Some(&(number, site)))
        {
            return Err(failed(OPERATION, "the tensors are not one computed MPS"));
        }
        let start = Computed {
            state: inner.computed_states[&number].clone(),
            outputs: tensors.clone(),
        };
        inner.outputs_live(&start, OPERATION)?;
        let sites = tensors.len();
        let ranks_fit = |tables: &[Box<[i64]>]| {
            tables.len() == sites
                && tables.iter().enumerate().all(|(site, table)| {
                    let rank = if site == 0 || site + 1 == sites { 2 } else { 3 };
                    table.len() == rank && table.iter().all(|&value| value > 0)
                })
        };
        if !ranks_fit(&layout.extents) || !ranks_fit(&layout.strides) {
            return Err(failed(OPERATION, "an extent or stride table does not fit"));
        }
        let target = inner.state(state, OPERATION)?;
        if target.sites != sites {
            return Err(failed(OPERATION, "the MPS has another site count"));
        }
        if !target.operators.is_empty() || target.computed.is_some() {
            return Err(failed(OPERATION, "the state already has operators"));
        }
        target.start = Some(start);
        Ok(())
    }

    fn capture_mps(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError> {
        const OPERATION: &str = "capture_mps";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        if inner.stream_busy {
            return Err(failed(OPERATION, "the compute may still be running"));
        }
        let target = inner.state(state, OPERATION)?;
        let computed = match &target.computed {
            Some((computed, at)) if *at == target.operators.len() => computed.clone(),
            _ => return Err(failed(OPERATION, "the state is not computed")),
        };
        target.start = Some(computed.clone());
        target.operators.clear();
        target.prepared_at = None;
        target.computed = Some((computed, 0));
        Ok(())
    }

    fn create_network_operator(
        &self,
        handle: OpaqueHandle,
        _mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        self.call("create_network_operator")?.create_on(
            handle,
            "create_network_operator",
            Object::Operator(Vec::new()),
        )
    }

    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_network_operator")?
            .destroy(operator, "destroy_network_operator", |o| {
                matches!(o, Object::Operator(_))
            })
    }

    fn append_product(
        &self,
        handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "append_product";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, operator, OPERATION)?;
        if factor_modes.len() != factor_tensors.len() {
            return Err(failed(OPERATION, "modes and tensors differ in number"));
        }
        let mut factors = Vec::with_capacity(factor_modes.len());
        for (modes, tensor) in factor_modes.iter().zip(factor_tensors) {
            inner.buffer(id(*tensor), OPERATION)?;
            let [mode] = **modes else {
                return Err(failed(OPERATION, "the double takes one-mode factors"));
            };
            let mode = usize::try_from(mode).map_err(|_| failed(OPERATION, "negative mode"))?;
            factors.push((mode, id(*tensor)));
        }
        match inner.objects.get_mut(&id(operator)) {
            Some(Object::Operator(products)) => {
                products.push((coefficient, factors));
                Ok(())
            }
            _ => Err(failed(OPERATION, "no such operator")),
        }
    }

    fn create_expectation(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_expectation")?;
        inner.owned(handle, state, "create_expectation")?;
        inner.owned(handle, operator, "create_expectation")?;
        inner.require(state, "create_expectation", |o| {
            matches!(o, Object::State(_))
        })?;
        inner.require(operator, "create_expectation", |o| {
            matches!(o, Object::Operator(_))
        })?;
        inner.create_on(
            handle,
            "create_expectation",
            Object::Expectation {
                state: id(state),
                operator: id(operator),
                prepared: false,
            },
        )
    }

    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_expectation")?
            .destroy(expectation, "destroy_expectation", |o| {
                matches!(o, Object::Expectation { .. })
            })
    }

    fn configure_expectation_hyper_samples(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError> {
        let inner = self.call("configure_expectation_hyper_samples")?;
        inner.owned(handle, expectation, "configure_expectation_hyper_samples")?;
        inner.require(expectation, "configure_expectation_hyper_samples", |o| {
            matches!(o, Object::Expectation { .. })
        })?;
        if hyper_samples > 0 {
            Ok(())
        } else {
            Err(failed(
                "configure_expectation_hyper_samples",
                "not positive",
            ))
        }
    }

    fn prepare_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        _maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_expectation")?;
        inner.owned(handle, expectation, "prepare_expectation")?;
        inner.owned(handle, workspace, "prepare_expectation")?;
        inner.require(workspace, "prepare_expectation", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        match inner.objects.get_mut(&id(expectation)) {
            Some(Object::Expectation { prepared, .. }) => {
                *prepared = true;
                Ok(())
            }
            _ => Err(failed("prepare_expectation", "no such expectation")),
        }
    }

    fn compute_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(Complex64, Complex64), EngineError> {
        const OPERATION: &str = "compute_expectation";
        let inner = self.call(OPERATION)?;
        inner.owned(handle, expectation, OPERATION)?;
        inner.owned(handle, workspace, OPERATION)?;
        inner.lent_scratch(workspace, OPERATION)?;
        let Some(Object::Expectation {
            state,
            operator,
            prepared: true,
        }) = inner.objects.get(&id(expectation))
        else {
            return Err(failed(OPERATION, "no prepared expectation"));
        };
        let Some(Object::Operator(products)) = inner.objects.get(operator) else {
            return Err(failed(OPERATION, "no such operator"));
        };
        let computed = inner.computed(*state, OPERATION)?;
        let mut value = Complex64::new(0.0, 0.0);
        for (coefficient, factors) in products {
            let mut ket = computed.clone();
            for &(mode, tensor) in factors {
                inner.apply(&mut ket, &[mode], tensor, OPERATION)?;
            }
            value += coefficient * computed.inner(&ket);
        }
        let squared_norm = inner
            .squared_norm
            .unwrap_or_else(|| computed.inner(computed));
        Ok((value, squared_norm))
    }
}

/// The capacity of `sites` qubits with every bond `bond`.
pub(crate) fn uniform_capacity(sites: usize, bond: usize) -> MpsShape {
    let shape = (0..sites)
        .map(|site| match site {
            0 => vec![2, bond],
            _ if site + 1 == sites => vec![bond, 2],
            _ => vec![bond, 2, bond],
        })
        .collect();
    MpsShape::new(shape).unwrap_or_else(|error| panic!("a valid capacity: {error}"))
}
