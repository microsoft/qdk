// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One cuTensorNet state of the engine: its native objects, and the
//! computed MPS every read uses.
//!
//! The engine needs it because cuTensorNet does not hold an MPS the way the
//! contract reads one: a native state is a network, an initial MPS and the
//! operators applied to it, which the library factorizes into an MPS only
//! when asked (a compute). `NativeState` keeps one native state open
//! between calls, computes it only when a read needs it, and answers every
//! read from the computed MPS. The crate's `docs/engine.md` ("Native calls
//! of each phase") shows its native calls in order.
//!
//! It calls no library directly: every native call goes through
//! `device_session.calls()`, the traits in `native_calls`, which
//! `native_calls::loaded_libraries` implements with the cuTensorNet and CUDA
//! functions, and `native_calls::fake_libraries` with a host stand-in for the
//! tests.
//!
//! Its parts each add to `NativeState`:
//!
//! - `computed_mps`: how the operators applied become the computed MPS,
//!   captured after each compute and restarted periodically.
//! - `expectation`: how a read is answered, by one native expectation on the
//!   computed MPS.
//! - `scratch`: the one scratch buffer the state's native calls work in.
//!
//! Kept here: the methods the engine calls, and the rules every part keeps.
//!
//! - **A read** is one native expectation (`expectation`). A projection reuses
//!   the P(One) just read for its site, so a measurement costs one expectation,
//!   not two. A sample draws from the state's conditional probabilities, by
//!   `tensornet`'s shared sampler.
//! - **A fork copies the computed MPS** into buffers of its own and starts a
//!   new native state from them (`cutensornetStateInitializeMPS`). It never
//!   replays the operators: a replay truncates along another path, so a fork of
//!   a truncated state would read another state.
//! - **Two streams.** The engine's work runs on its own non-blocking CUDA
//!   stream, which does not wait for CUDA's legacy default stream, where
//!   `cudaMemcpy` copies operators, factors and a fork's tensors. So the engine
//!   waits for the copies before each compute and each expectation
//!   (`synchronize_copies`), and for its stream before it reads what a compute
//!   wrote or frees what the compute read.
//! - **Failures.** A native failure while the state is changed or computed
//!   leaves it unusable: every later call reports [`EngineError::Poisoned`]. A
//!   failed read releases what it created and leaves the state as it was.
//! - **Release.** Dropping the state releases its native objects and its device
//!   memory (`close`). A failed release is not reported, except on stderr in
//!   debug builds; `CuTensorNetState` says why.

use super::{
    EngineError,
    device_session::DeviceSession,
    error::{combine_execution_and_cleanup, first_failure, report_failed_release},
    native_calls::{
        NativeCalls, OpaqueHandle,
        complex::{Complex64Abi, complex_bytes},
        state_calls::{MpsLayout, NativeCapacity, StateI32Configuration, convert_layout},
    },
};
use expectation::{ProductTerm, diagonal_term, normalize_expectation, positive_squared_norm};
use num_complex::Complex64;
use std::{collections::BTreeMap, rc::Rc};
use tensornet::{MpsShape, PauliSum, Refusal, sampling};

mod computed_mps;
mod expectation;
mod scratch;

/// One native state of the engine, with everything it holds on the device
/// (the module's header says how it is kept).
pub(super) struct NativeState {
    device_session: Rc<DeviceSession<dyn NativeCalls>>,
    state: Option<OpaqueHandle>,
    workspace: Option<OpaqueHandle>,
    query_workspace: Option<OpaqueHandle>,
    network_operator: Option<OpaqueHandle>,
    expectation: Option<OpaqueHandle>,
    /// The state's scratch and its size in bytes, attached to every
    /// workspace descriptor the state uses.
    scratch: Option<(OpaqueHandle, usize)>,
    /// Scratch replaced by a larger buffer whose attachment failed: it may
    /// still be attached, so it is freed only after the descriptors.
    retired_scratch: Vec<OpaqueHandle>,
    /// Every device buffer of the state but its scratch and its operator
    /// tensors: the computed MPS, and a read's tensors while it runs.
    allocations: Vec<OpaqueHandle>,
    /// The device tensors of the operators applied to the native state,
    /// and their modes, released with it.
    operator_tensors: Vec<OpaqueHandle>,
    operator_modes: Vec<Box<[i32]>>,
    /// The outputs of the last compute, one per site, also in `allocations`.
    computed_buffers: Vec<OpaqueHandle>,
    /// The extents and strides the last compute wrote its outputs with,
    /// which a restart or a fork starts a new state from.
    computed_layout: Option<MpsLayout>,
    computes_since_restart: usize,
    /// The cuTensorNet handle the state moved to at its last restart: the
    /// state, its workspaces and its reads use it instead of the session's,
    /// and it is destroyed after them.
    own_handle: Option<OpaqueHandle>,
    state_extents: Box<[i64]>,
    capacity: NativeCapacity,
    finalized: bool,
    /// The chain of the last compute, while no operator has been applied
    /// since.
    realized: Option<MpsShape>,
    /// The site `probability_of_one` last read and its P(One), while no
    /// operator has been applied since, for `project` to reuse.
    last_probability_of_one: Option<(usize, f64)>,
    poisoned: Option<String>,
    closed: bool,
    /// Development only (`DevTuning::pooled_buffers`): freed device
    /// buffers by size, and the size of every buffer the pool may take
    /// back.
    pool: BTreeMap<usize, Vec<OpaqueHandle>>,
    pooled_bytes: BTreeMap<OpaqueHandle, usize>,
    /// Development only: the canonical centre the state is configured
    /// with, and the one its last compute was made with.
    canonical_centre: Option<usize>,
    computed_centre: Option<usize>,
}

impl NativeState {
    /// Creates the |0…0⟩ state of `requested`'s qubits on the session's
    /// handle, with the native capacity `requested` asks for
    /// (`NativeCapacity::new`).
    pub(super) fn new(
        device_session: &Rc<DeviceSession<dyn NativeCalls>>,
        requested: &MpsShape,
    ) -> Result<Self, EngineError> {
        let capacity = NativeCapacity::new(requested)?;
        let state_extents = vec![2; capacity.native_shape().site_count()].into_boxed_slice();
        let handle = device_session.handle();
        let state = device_session
            .calls()
            .create_state(handle, state_extents.as_ref())?;
        let mut native = Self {
            device_session: Rc::clone(device_session),
            state: Some(state),
            workspace: None,
            query_workspace: None,
            network_operator: None,
            expectation: None,
            scratch: None,
            retired_scratch: Vec::new(),
            allocations: Vec::new(),
            operator_tensors: Vec::new(),
            operator_modes: Vec::new(),
            computed_buffers: Vec::new(),
            computed_layout: None,
            computes_since_restart: 0,
            own_handle: None,
            state_extents,
            capacity,
            finalized: false,
            realized: None,
            last_probability_of_one: None,
            poisoned: None,
            closed: false,
            pool: BTreeMap::new(),
            pooled_bytes: BTreeMap::new(),
            canonical_centre: None,
            computed_centre: None,
        };
        match device_session.calls().create_workspace(handle) {
            Ok(workspace) => native.workspace = Some(workspace),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), native.close());
            }
        }
        Ok(native)
    }

    pub(super) fn device_session(&self) -> &Rc<DeviceSession<dyn NativeCalls>> {
        &self.device_session
    }

    /// How many sites the state's callers see, which the contract's checks
    /// bound the calls' sites by.
    pub(super) fn site_count(&self) -> usize {
        self.capacity.site_count()
    }

    /// Applies a unitary on one or two sites. cuTensorNet is told the
    /// operator is unitary, which the engine's check of the call
    /// (`checks::apply`) has established.
    pub(super) fn apply(
        &mut self,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        self.check_live()?;
        let modes = sites
            .iter()
            .map(|&site| mode(site))
            .collect::<Result<Box<[i32]>, _>>()?;
        let matrix: Box<[Complex64Abi]> = matrix.iter().copied().map(Complex64Abi::from).collect();
        self.poison_on_error(|native| native.apply_operator(modes, &matrix, true))
    }

    /// P(One) of `site`: ⟨Π₁⟩ / ⟨ψ|ψ⟩, one expectation on the computed
    /// state, remembered for a projection of the same site.
    pub(super) fn probability_of_one(&mut self, site: usize) -> Result<f64, EngineError> {
        self.check_live()?;
        self.ensure_computed()?;
        let probability = if self.device_session.dev_tuning().canonical_centre_reads {
            if self.computed_centre == Some(site) {
                self.canonical_probability_of_one(site)?
            } else {
                self.device_session
                    .record_dev_time(|times| times.canonical_fallbacks += 1);
                self.probability(site, true)?
            }
        } else {
            self.probability(site, true)?
        };
        self.last_probability_of_one = Some((site, probability));
        Ok(probability)
    }

    /// Development only: configures the canonical centre of the next
    /// computes (`CuTensorNetEngine::dev_set_canonical_centre`).
    pub(super) fn set_canonical_centre(&mut self, site: usize) -> Result<(), EngineError> {
        self.check_live()?;
        self.canonical_centre = Some(site);
        if self.finalized {
            let (handle, state) = (self.handle(), self.state()?);
            let centre = mode(site)?;
            self.poison_on_error(|native| {
                native.device_session.calls().configure_state_i32(
                    handle,
                    state,
                    StateI32Configuration::MpsCanonicalCentre(centre),
                )
            })?;
        }
        Ok(())
    }

    /// Development only: P(One) of `site`, the computed MPS's canonical
    /// centre, from its tensor alone: the tensors left of the centre are
    /// left-orthonormal and those right of it right-orthonormal, so
    /// P(One) = Σ|A[…, 1, …]|² / Σ|A|², the physical mode being the
    /// first of the first site's tensor and the second of the others'.
    fn canonical_probability_of_one(&mut self, site: usize) -> Result<f64, EngineError> {
        let started = std::time::Instant::now();
        let layout = self
            .computed_layout
            .as_ref()
            .expect("a computed state has the layout of its outputs");
        let extents = convert_layout("extent", &[layout.extents[site].clone()])?;
        let strides = convert_layout("stride", &[layout.strides[site].clone()])?;
        let (extents, strides) = (&extents[0], &strides[0]);
        let elements = 1 + extents
            .iter()
            .zip(strides)
            .map(|(extent, stride)| (extent - 1) * stride)
            .sum::<usize>();
        let mut tensor = vec![Complex64Abi::new(0.0, 0.0); elements];
        self.device_session
            .calls()
            .copy_to_host(&mut tensor, self.computed_buffers[site])?;
        let physical = usize::from(site != 0);
        let (mut one, mut total) = (0.0, 0.0);
        let mut index = vec![0usize; extents.len()];
        loop {
            let offset: usize = index.iter().zip(strides).map(|(i, s)| i * s).sum();
            let value = Complex64::from(tensor[offset]).norm_sqr();
            total += value;
            if index[physical] == 1 {
                one += value;
            }
            let mut mode = 0;
            loop {
                if mode == index.len() {
                    let probability = positive_squared_norm(Complex64::new(total, 0.0))
                        .map(|total| (one / total).clamp(0.0, 1.0));
                    self.device_session
                        .record_dev_time(|times| times.canonical_read.add_since(started));
                    return probability;
                }
                index[mode] += 1;
                if index[mode] < extents[mode] {
                    break;
                }
                index[mode] = 0;
                mode += 1;
            }
        }
    }

    /// P(One) of `site` given `given`: ⟨Π·Π₁⟩ / ⟨Π⟩ with Π the product of
    /// the given values' projectors, two expectations on the computed
    /// state, no copy of it, clamped to [0, 1] against rounding. A
    /// condition whose weight is zero or less, as rounding may leave it, is
    /// refused.
    pub(super) fn conditional_probability_of_one(
        &mut self,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, EngineError> {
        self.check_live()?;
        if given.is_empty() {
            return self.probability_of_one(site);
        }
        let mut condition: BTreeMap<i32, [f64; 2]> = BTreeMap::new();
        for &(at, one) in given {
            condition.entry(mode(at)?).or_insert([1.0, 1.0])[usize::from(!one)] = 0.0;
        }
        let mut numerator = condition.clone();
        numerator.entry(mode(site)?).or_insert([1.0, 1.0])[0] = 0.0;
        self.ensure_computed()?;
        let (value, squared_norm) = self.query(&[diagonal_term(&condition)])?;
        let weight = normalize_expectation(value, squared_norm)?.re;
        if weight <= 0.0 {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        let (value, squared_norm) = self.query(&[diagonal_term(&numerator)])?;
        Ok((normalize_expectation(value, squared_norm)?.re / weight).clamp(0.0, 1.0))
    }

    /// Projects `site` on One when `one` is true, on Zero when it is false,
    /// divides by √p and returns p, the probability of that value: a
    /// non-unitary one-site operator. p comes from the P(One)
    /// `probability_of_one` just read for the site, if no operator was
    /// applied since, or from one expectation otherwise. A value whose
    /// probability is zero or less, as rounding may leave it, is refused,
    /// the state unchanged.
    pub(super) fn project(&mut self, site: usize, one: bool) -> Result<f64, EngineError> {
        self.check_live()?;
        let probability = match self.last_probability_of_one {
            Some((read, p_one)) if read == site => {
                if one {
                    p_one
                } else {
                    1.0 - p_one
                }
            }
            _ => {
                self.ensure_computed()?;
                self.probability(site, one)?
            }
        };
        if probability <= 0.0 {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        let scale = Complex64Abi::new(1.0 / probability.sqrt(), 0.0);
        let zero = Complex64Abi::new(0.0, 0.0);
        let matrix = if one {
            [zero, zero, zero, scale]
        } else {
            [scale, zero, zero, zero]
        };
        let modes = Box::new([mode(site)?]);
        self.poison_on_error(|native| native.apply_operator(modes, &matrix, false))?;
        Ok(probability)
    }

    /// A new state, on the session's handle, that starts from a copy of
    /// this state's computed MPS, computing it first if an operator was
    /// applied since. The copies are the fork's own, so this state may
    /// change, compute, restart or be dropped without the fork seeing it.
    pub(super) fn fork(&mut self) -> Result<Self, EngineError> {
        self.check_live()?;
        self.ensure_computed()?;
        let layout = self
            .computed_layout
            .clone()
            .expect("a computed state has the layout of its outputs");
        let device_session = Rc::clone(&self.device_session);
        let calls = device_session.calls();
        let mut fork = Self::new(&device_session, self.capacity.requested())?;
        fork.canonical_centre = self.canonical_centre;
        let elements = self.capacity.native_shape().element_counts();
        let mut copies = Vec::with_capacity(elements.len());
        for (&source, &count) in self.computed_buffers.iter().zip(&elements) {
            let copy = fork.allocate_complex(count, "forked MPS tensor")?;
            calls.copy_on_device(copy, source, complex_bytes(count, "forked MPS tensor")?)?;
            copies.push(copy);
        }
        calls.synchronize_copies()?;
        let (handle, state) = (fork.handle(), fork.state()?);
        fork.start_from(handle, state, &layout, &copies)?;
        fork.finalized = true;
        fork.computed_buffers = copies;
        fork.computed_layout = Some(layout);
        Ok(fork)
    }

    /// `shots` draws of `sites`, by `tensornet`'s shared sampler over this
    /// state's conditional probabilities: each distinct prefix of values
    /// drawn costs two expectations, or one with no prefix.
    pub(super) fn sample(
        &mut self,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, EngineError> {
        self.check_live()?;
        sampling::sample_by_conditionals(sites, shots, seed, |site, given| {
            self.conditional_probability_of_one(site, given)
        })
    }

    /// `c_I + Σₖ cₖ⟨ψ|Pₖ|ψ⟩ / ⟨ψ|ψ⟩`: the identity coefficient on the host,
    /// the rest as one native expectation.
    pub(super) fn expectation(&mut self, observable: &PauliSum) -> Result<Complex64, EngineError> {
        self.check_live()?;
        if observable.terms().is_empty() {
            return Ok(observable.identity_coefficient());
        }
        let terms = observable
            .terms()
            .iter()
            .map(|term| {
                let factors = term
                    .factors()
                    .iter()
                    .map(|&(site, pauli)| Ok((mode(site)?, pauli.matrix().map(Complex64Abi::from))))
                    .collect::<Result<Vec<_>, EngineError>>()?;
                Ok(ProductTerm {
                    coefficient: term.coefficient(),
                    factors,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        self.ensure_computed()?;
        let (value, squared_norm) = self.query(&terms)?;
        Ok(observable.identity_coefficient() + normalize_expectation(value, squared_norm)?)
    }

    /// The chain of the computed state as the caller sees it, computing it
    /// first if an operator was applied since: the extents the library
    /// realized, without the padding site of a lone qubit.
    pub(super) fn shape(&mut self) -> Result<MpsShape, EngineError> {
        self.check_live()?;
        let realized = self.ensure_computed()?;
        self.capacity.logical(realized)
    }

    /// ⟨ψ|ψ⟩ of the computed state, as the library reports it beside an
    /// expectation: one expectation of the identity on the first site,
    /// whose squared norm is the answer. The norm is the retained weight
    /// because `configure` pins `SvdNormalizationNone`, so the kept
    /// singular values are never rescaled; the padding of a lone site stays
    /// |0⟩ and changes nothing.
    pub(super) fn retained_weight(&mut self) -> Result<f64, EngineError> {
        self.check_live()?;
        self.ensure_computed()?;
        let identity = BTreeMap::from([(0, [1.0, 1.0])]);
        let (_, squared_norm) = self.query(&[diagonal_term(&identity)])?;
        positive_squared_norm(squared_norm)
    }

    fn check_live(&self) -> Result<(), EngineError> {
        if let Some(reason) = &self.poisoned {
            return Err(EngineError::Poisoned {
                reason: reason.clone(),
            });
        }
        Ok(())
    }

    /// Runs `change`, and marks the state unusable if it fails: the native
    /// state may be half changed.
    fn poison_on_error<T>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let result = change(self);
        if let Err(error) = &result {
            self.poisoned = Some(error.to_string());
        }
        result
    }

    /// The handle the state is on: its own since a restart, the session's
    /// before.
    fn handle(&self) -> OpaqueHandle {
        self.own_handle
            .unwrap_or_else(|| self.device_session.handle())
    }

    fn state(&self) -> Result<OpaqueHandle, EngineError> {
        self.state.ok_or_else(|| EngineError::Poisoned {
            reason: "the state is closed".to_string(),
        })
    }

    fn workspace(&self) -> Result<OpaqueHandle, EngineError> {
        self.workspace.ok_or_else(|| EngineError::Poisoned {
            reason: "the state's workspace is closed".to_string(),
        })
    }

    fn allocate_complex(
        &mut self,
        elements: usize,
        resource: &'static str,
    ) -> Result<OpaqueHandle, EngineError> {
        let bytes = complex_bytes(elements, resource)?;
        let allocation = self.take_buffer(bytes)?;
        self.allocations.push(allocation);
        Ok(allocation)
    }

    /// A device buffer of `bytes`: a new one, or with
    /// `DevTuning::pooled_buffers` one of that size from the pool.
    fn take_buffer(&mut self, bytes: usize) -> Result<OpaqueHandle, EngineError> {
        let pooled = self.device_session.dev_tuning().pooled_buffers;
        if pooled && let Some(buffer) = self.pool.get_mut(&bytes).and_then(Vec::pop) {
            self.device_session
                .record_dev_time(|times| times.pool_hits += 1);
            return Ok(buffer);
        }
        let buffer = self.device_session.calls().allocate(bytes)?;
        self.device_session
            .record_dev_time(|times| times.allocations += 1);
        if pooled {
            self.pooled_bytes.insert(buffer, bytes);
        }
        Ok(buffer)
    }

    /// Frees a buffer `take_buffer` gave, or returns it to the pool.
    fn release_buffer(&mut self, buffer: OpaqueHandle) -> Result<(), EngineError> {
        match self.pooled_bytes.get(&buffer) {
            Some(&bytes) => {
                self.pool.entry(bytes).or_default().push(buffer);
                Ok(())
            }
            None => self.device_session.calls().free(buffer),
        }
    }

    /// Releases every native object and buffer of the state, the
    /// descriptors before the scratch they hold; attempts every release and
    /// returns the first failure. Later closes do nothing: `new` closes a
    /// state whose workspace it failed to create, and dropping that state
    /// closes it again.
    fn close(&mut self) -> Result<(), EngineError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let device_session = Rc::clone(&self.device_session);
        let calls = device_session.calls();
        // The engine's device, which another engine's call may have changed.
        let mut released = vec![
            device_session.bind_device(),
            calls.synchronize_stream(device_session.stream()),
            self.close_query_lifecycle(),
        ];
        if let Some(workspace) = self.workspace.take() {
            released.push(calls.destroy_workspace(workspace));
        }
        if let Some(state) = self.state.take() {
            released.push(calls.destroy_state(state));
        }
        self.computed_buffers.clear();
        self.operator_modes.clear();
        self.allocations.append(&mut self.operator_tensors);
        self.allocations.append(&mut self.retired_scratch);
        self.allocations
            .extend(self.scratch.take().map(|(scratch, _)| scratch));
        self.allocations
            .extend(std::mem::take(&mut self.pool).into_values().flatten());
        self.pooled_bytes.clear();
        while let Some(allocation) = self.allocations.pop() {
            released.push(calls.free(allocation));
        }
        if let Some(handle) = self.own_handle.take() {
            released.push(calls.destroy_handle(handle));
        }
        first_failure(released)
    }
}

impl Drop for NativeState {
    fn drop(&mut self) {
        report_failed_release("an MPS state", self.close());
    }
}

/// The native mode of one of the caller's sites, which the engine's checks
/// have bounded by the site count.
fn mode(site: usize) -> Result<i32, EngineError> {
    i32::try_from(site).map_err(|_| EngineError::ResourceSizeOverflow {
        resource: "native mode",
    })
}
