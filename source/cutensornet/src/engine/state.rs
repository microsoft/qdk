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
//! read from the computed MPS. The crate's `README.md` ("The MPS engine")
//! shows its native calls in order.
//!
//! - **Computed on demand.** After an operator is applied, the next read
//!   computes the state again; `finalize_mps` (the native capacity's bond
//!   at each cut) is called once, before the first compute.
//! - **Captured after each compute** (`capture_mps`): the computed MPS
//!   becomes the state's start, and the library forgets the operators
//!   applied so far, so the next compute applies only the operators applied
//!   since. Without it cuTensorNet computes from |0…0⟩ with every operator,
//!   and a read after each of a shot's `M` measurements costs `O(M·G)` for
//!   `G` gates instead of `O(G)` (H₂: `M` ≈ 1,404, `G` ≈ 48,000).
//! - **A read** is one native expectation, whose objects are released
//!   before it returns. A projection reuses the P(One) just read for its
//!   site, so a measurement costs one expectation, not two. A sample draws
//!   from the state's conditional probabilities, by `tensornet`'s shared
//!   sampler.
//! - **A fork copies the computed MPS** into buffers of its own and starts
//!   a new native state from them (`cutensornetStateInitializeMPS`). It
//!   never replays the operators: a replay truncates along another path, so
//!   a fork of a truncated state would read another state.
//! - **Operator tensors and modes** stay allocated as long as the native
//!   state they were applied to: `cutensornetStateApplyTensorOperator` says
//!   the tensor "must stay valid for the whole lifetime of the tensor
//!   network state", and `cutensornetStateCaptureMPS` deletes the operators
//!   but says nothing of their tensors. A restart ends the state and frees
//!   them.
//! - **One scratch buffer per state.** cuTensorNet works in scratch memory
//!   its caller attaches to a workspace descriptor. The state holds one
//!   buffer, attached to every descriptor it uses, and grows it, attaching
//!   the larger buffer before freeing the smaller, when a call needs more;
//!   so no descriptor ever holds freed memory. At rest a state holds one
//!   buffer, its largest call's; while it grows, the old and the new. A
//!   compute's output buffers are freed once the next compute has been
//!   captured, since the captured MPS is read from them until then.
//! - **Two streams.** The engine's work runs on its own non-blocking CUDA
//!   stream, which does not wait for CUDA's legacy default stream, where
//!   `cudaMemcpy` copies operators, factors and a fork's tensors. So the
//!   engine waits for the copies before each compute and each expectation
//!   (`synchronize_copies`), and for its stream before it reads what a
//!   compute wrote or frees what the compute read.
//! - **Restarts.** Every `RESTART_PERIOD` computes, before the next
//!   operator, the native state is replaced by a new one on a cuTensorNet
//!   handle of its own, whose start is the last computed MPS, finalized and
//!   configured as the first one was; the old state, its operator tensors
//!   and its handle are released (never the session's). Capture is
//!   documented as "equivalent to creating a `cutensornetState_t` with the
//!   previous output MPS state as the initial MPS state", so a restart
//!   changes no value; it bounds the scratch a compute asks for over a long
//!   shot (`policy.rs`).
//! - **Failures.** A native failure while the state is changed or computed
//!   leaves it unusable: every later call reports
//!   [`EngineError::Poisoned`]. A failed read releases what it created and
//!   leaves the state as it was.
//! - **Release.** Dropping the state releases its native objects and its
//!   device memory (`close`). A failed release is not reported, since a drop
//!   has no caller to report to, except on stderr in debug builds.

use super::{
    EngineError, NativeCalls, OpaqueHandle,
    error::{combine_execution_and_cleanup, report_failed_release},
    ffi::Complex64Abi,
    policy::{DISCARDED_WEIGHT_CUTOFF, MAXIMUM_WORKSPACE_BYTES, RESTART_PERIOD},
    session::DeviceSession,
    state_calls::{
        MpsLayout, NativeCapacity, StateF64Configuration, StateU32Configuration, convert_layout,
        validate_realized_chain,
    },
};
use num_complex::Complex64;
use std::{collections::BTreeMap, mem::size_of, rc::Rc};
use tensornet::{MpsShape, PauliSum, Refusal, checks, sampling};

/// Hyper-samples of each expectation's contraction path search.
const EXPECTATION_HYPER_SAMPLES: i32 = 8;

/// One product of one-site factors in an expectation's operator.
struct Product {
    coefficient: Complex64,
    factors: Vec<(i32, [Complex64Abi; 4])>,
}

pub(super) struct NativeState {
    session: Rc<DeviceSession<dyn NativeCalls>>,
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
}

impl NativeState {
    /// Creates the |0…0⟩ state of `capacity`'s qubits on the session's
    /// handle.
    pub(super) fn new(
        session: &Rc<DeviceSession<dyn NativeCalls>>,
        requested: &MpsShape,
    ) -> Result<Self, EngineError> {
        let capacity = NativeCapacity::new(requested)?;
        let state_extents = vec![2; capacity.native_shape().site_count()].into_boxed_slice();
        let handle = session.handle();
        let state = session
            .calls()
            .create_state(handle, state_extents.as_ref())?;
        let mut native = Self {
            session: Rc::clone(session),
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
        };
        match session.calls().create_workspace(handle) {
            Ok(workspace) => native.workspace = Some(workspace),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), native.close());
            }
        }
        Ok(native)
    }

    pub(super) fn session(&self) -> &Rc<DeviceSession<dyn NativeCalls>> {
        &self.session
    }

    /// Applies a unitary on one or two sites, once the contract's checks
    /// pass. cuTensorNet is told the operator is unitary, which the check
    /// has established.
    pub(super) fn apply(
        &mut self,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        self.check_live()?;
        checks::apply(self.capacity.site_count(), sites, matrix)?;
        let modes = sites
            .iter()
            .map(|&site| mode(site))
            .collect::<Result<Box<[i32]>, _>>()?;
        let matrix: Box<[Complex64Abi]> = matrix.iter().copied().map(Complex64Abi::from).collect();
        self.poison_on_error(|native| native.register(modes, &matrix, true))
    }

    /// P(One) of `site`: ⟨Π₁⟩ / ⟨ψ|ψ⟩, one expectation on the computed
    /// state, remembered for a projection of the same site.
    pub(super) fn probability_of_one(&mut self, site: usize) -> Result<f64, EngineError> {
        self.check_live()?;
        checks::probability_of_one(self.capacity.site_count(), site)?;
        self.ensure_computed()?;
        let probability = self.probability(site, true)?;
        self.last_probability_of_one = Some((site, probability));
        Ok(probability)
    }

    /// P(One) of `site` given `given`: ⟨Π·Π₁⟩ / ⟨Π⟩ with Π the product of
    /// the given values' projectors, two expectations on the computed
    /// state, no copy of it. A condition whose weight is zero or less, as
    /// rounding may leave it, or NaN is refused.
    pub(super) fn conditional_probability_of_one(
        &mut self,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, EngineError> {
        self.check_live()?;
        checks::conditional_probability_of_one(self.capacity.site_count(), site, given)?;
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
        let (value, squared_norm) = self.query(&[diagonal_product(&condition)])?;
        let weight = normalize_expectation(value, squared_norm)?.re;
        if weight.is_nan() || weight <= 0.0 {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        let (value, squared_norm) = self.query(&[diagonal_product(&numerator)])?;
        Ok((normalize_expectation(value, squared_norm)?.re / weight).clamp(0.0, 1.0))
    }

    /// Projects `site` on `one`, divides by √p and returns p: a non-unitary
    /// one-site operator. p is the P(One) `probability_of_one` just read for
    /// the site, if no operator was applied since, or one expectation
    /// otherwise. A value whose probability is zero or less, as rounding
    /// may leave it, or NaN is refused, the state unchanged.
    pub(super) fn project(&mut self, site: usize, one: bool) -> Result<f64, EngineError> {
        self.check_live()?;
        checks::project(self.capacity.site_count(), site)?;
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
        if probability.is_nan() || probability <= 0.0 {
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
        self.poison_on_error(|native| native.register(modes, &matrix, false))?;
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
        let session = Rc::clone(&self.session);
        let calls = session.calls();
        let mut fork = Self::new(&session, self.capacity.requested())?;
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
        checks::sample(self.capacity.site_count(), sites)?;
        sampling::sample_by_conditionals(sites, shots, seed, |site, given| {
            self.conditional_probability_of_one(site, given)
        })
    }

    /// `c_I + Σₖ cₖ⟨ψ|Pₖ|ψ⟩ / ⟨ψ|ψ⟩`: the identity coefficient on the host,
    /// the rest as one native expectation.
    pub(super) fn expectation(&mut self, observable: &PauliSum) -> Result<Complex64, EngineError> {
        self.check_live()?;
        checks::expectation(self.capacity.site_count(), observable)?;
        if observable.terms().is_empty() {
            return Ok(observable.identity_coefficient());
        }
        let products = observable
            .terms()
            .iter()
            .map(|term| {
                let factors = term
                    .factors()
                    .iter()
                    .map(|&(site, pauli)| Ok((mode(site)?, pauli.matrix().map(Complex64Abi::from))))
                    .collect::<Result<Vec<_>, EngineError>>()?;
                Ok(Product {
                    coefficient: term.coefficient(),
                    factors,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        self.ensure_computed()?;
        let (value, squared_norm) = self.query(&products)?;
        Ok(observable.identity_coefficient() + normalize_expectation(value, squared_norm)?)
    }

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
        let (_, squared_norm) = self.query(&[diagonal_product(&identity)])?;
        positive_squared_norm(squared_norm)
    }

    fn check_live(&self) -> Result<(), EngineError> {
        if let Some(reason) = &self.poisoned {
            return Err(EngineError::Poisoned {
                reason: reason.clone(),
            });
        }
        if self.closed {
            return Err(EngineError::Poisoned {
                reason: "the state is closed".to_string(),
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

    /// The probability of `one` at `site` in the computed state, clamped to
    /// [0, 1] against rounding.
    fn probability(&mut self, site: usize, one: bool) -> Result<f64, EngineError> {
        let mut diagonal = [0.0, 0.0];
        diagonal[usize::from(one)] = 1.0;
        let projector = BTreeMap::from([(mode(site)?, diagonal)]);
        let (value, squared_norm) = self.query(&[diagonal_product(&projector)])?;
        Ok(normalize_expectation(value, squared_norm)?
            .re
            .clamp(0.0, 1.0))
    }

    /// Uploads `matrix` and applies it to `modes` of the state. The matrix
    /// is copied verbatim: cuTensorNet reads the row-major `M[out][in]`
    /// layout with default strides on the modes in order, first most
    /// significant, which is `MpsEngine::apply`'s convention.
    ///
    /// If `RESTART_PERIOD` computes have passed since the last restart, the
    /// native state is restarted first.
    fn register(
        &mut self,
        modes: Box<[i32]>,
        matrix: &[Complex64Abi],
        unitary: bool,
    ) -> Result<(), EngineError> {
        if self.realized.is_some() && self.computes_since_restart >= RESTART_PERIOD {
            self.restart()?;
        }
        let session = Rc::clone(&self.session);
        let calls = session.calls();
        let bytes = complex_bytes(matrix.len(), "operator tensor")?;
        let tensor = calls.allocate(bytes)?;
        self.operator_tensors.push(tensor);
        calls.copy_to_device(tensor, matrix)?;
        self.realized = None;
        self.last_probability_of_one = None;
        calls.apply_tensor_operator(self.handle(), self.state()?, &modes, tensor, unitary)?;
        self.operator_modes.push(modes);
        Ok(())
    }

    /// Replaces the native state, which must have been computed and
    /// captured since its last operator, by a new one on a new handle
    /// whose start is that computed MPS, finalized and configured as the
    /// first one, with a new workspace descriptor; then releases the old
    /// state, its workspace, its operator tensors and, unless it is the
    /// session's, its handle. The new state reads the MPS from
    /// `computed_buffers`, which the next compute frees once it has
    /// captured its own.
    fn restart(&mut self) -> Result<(), EngineError> {
        let session = Rc::clone(&self.session);
        let calls = session.calls();
        let layout = self
            .computed_layout
            .as_ref()
            .expect("a restart follows a compute, which leaves its layout");
        let handle = calls.create_handle()?;
        let started = calls
            .create_state(handle, self.state_extents.as_ref())
            .and_then(|fresh| {
                let initialized = self
                    .start_from(handle, fresh, layout, &self.computed_buffers)
                    .and_then(|()| calls.create_workspace(handle));
                match initialized {
                    Ok(workspace) => Ok((fresh, workspace)),
                    Err(error) => {
                        combine_execution_and_cleanup(Err(error), calls.destroy_state(fresh))
                    }
                }
            });
        let (fresh, workspace) = match started {
            Ok(started) => started,
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), calls.destroy_handle(handle));
            }
        };
        let mut released = Vec::new();
        if let Some(previous) = self.state.replace(fresh) {
            released.push(calls.destroy_state(previous));
        }
        if let Some(previous) = self.workspace.replace(workspace) {
            released.push(calls.destroy_workspace(previous));
        }
        self.operator_modes.clear();
        for tensor in std::mem::take(&mut self.operator_tensors) {
            released.push(calls.free(tensor));
        }
        if let Some(previous) = self.own_handle.replace(handle) {
            released.push(calls.destroy_handle(previous));
        }
        self.computes_since_restart = 0;
        released.into_iter().collect()
    }

    /// Makes the MPS in `tensors`, laid out as `layout` says, the start of
    /// the native `state` on `handle`, then finalizes and configures it as
    /// every state of the engine.
    fn start_from(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &MpsLayout,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        let calls = self.session.calls();
        calls.initialize_mps(handle, state, layout, tensors)?;
        calls.finalize_mps(handle, state, &self.capacity)?;
        self.configure(handle, state)
    }

    /// The handle the state is on: its own since a restart, the session's
    /// before.
    fn handle(&self) -> OpaqueHandle {
        self.own_handle.unwrap_or_else(|| self.session.handle())
    }

    /// The chain of the computed state, computing it first if an operator
    /// was applied since the last compute.
    fn ensure_computed(&mut self) -> Result<MpsShape, EngineError> {
        if let Some(realized) = &self.realized {
            return Ok(realized.clone());
        }
        self.poison_on_error(|native| {
            if !native.finalized {
                let (handle, state) = (native.handle(), native.state()?);
                native
                    .session
                    .calls()
                    .finalize_mps(handle, state, &native.capacity)?;
                native.configure(handle, state)?;
                native.finalized = true;
            }
            let realized = native.materialize()?;
            native.realized = Some(realized.clone());
            Ok(realized)
        })
    }

    /// The truncation and factorization every state of the engine is
    /// computed with (`policy.rs`).
    fn configure(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError> {
        let calls = self.session.calls();
        calls.configure_state_f64(
            handle,
            state,
            StateF64Configuration::SvdDiscardedWeightCutoff(DISCARDED_WEIGHT_CUTOFF),
        )?;
        // No normalization is the library default, pinned so the norm stays
        // the retained weight whichever default a future release picks.
        calls.configure_state_u32(handle, state, StateU32Configuration::SvdNormalizationNone)?;
        calls.configure_state_u32(handle, state, StateU32Configuration::SvdAlgorithmGesvd)?;
        calls.configure_state_u32(handle, state, StateU32Configuration::MpsGaugeSimple)
    }

    /// Computes the state into new output buffers, captures it, and returns
    /// the chain the library realized, checked against the native capacity.
    fn materialize(&mut self) -> Result<MpsShape, EngineError> {
        let session = Rc::clone(&self.session);
        let (calls, handle, stream) = (session.calls(), self.handle(), session.stream());
        let state = self.state()?;
        let mark = self.allocations.len();
        let mut outputs = Vec::new();
        for elements in self.capacity.native_shape().element_counts() {
            outputs.push(self.allocate_complex(elements, "MPS output")?);
        }
        let workspace = self.workspace()?;
        let available = self.available_workspace_bytes()?;
        calls.prepare_state(handle, state, available, workspace, stream)?;
        let bytes = calls.workspace_size(handle, workspace)?;
        if bytes <= 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("the compute's workspace size is {bytes}"),
            });
        }
        self.attach_scratch(workspace, bytes, "device workspace")?;
        calls.synchronize_copies()?;
        let mut layout = MpsLayout::new(&self.capacity);
        calls.compute_state(handle, state, workspace, &mut layout, &mut outputs, stream)?;
        calls.synchronize_stream(stream)?;
        // The library reports the shape it produced and, separately, the
        // strides it used to write it. Only the shape is part of the state.
        let realized =
            MpsShape::new(convert_layout("extent", &layout.extents)?).map_err(|error| {
                EngineError::InvalidNativeResult {
                    reason: format!("realized MPS is not a valid chain: {error}"),
                }
            })?;
        validate_realized_chain(&realized, self.capacity.native_shape())?;
        calls.capture_mps(handle, state)?;
        self.computes_since_restart += 1;
        // The state now refers to the new buffers, as its computed MPS and,
        // captured, as its start; the stream has finished with the old
        // ones, and a restart or fork whose state read them has computed
        // and captured since.
        let fresh = self.allocations[mark..].to_vec();
        let previous = std::mem::replace(&mut self.computed_buffers, fresh);
        self.computed_layout = Some(layout);
        self.allocations
            .retain(|allocation| !previous.contains(allocation));
        previous
            .into_iter()
            .try_for_each(|old| calls.free(old))
            .map(|()| realized)
    }

    /// The scratch a call may take: the budget, or the larger of the
    /// device's free memory and the scratch the state holds, whichever is
    /// less. Not their sum: a larger buffer is allocated before the held
    /// one is freed, so a growth must fit in the free memory alone.
    fn available_workspace_bytes(&self) -> Result<usize, EngineError> {
        let (free, _) = self.session.calls().memory_info()?;
        let held = self.scratch.map_or(0, |(_, size)| size);
        Ok(MAXIMUM_WORKSPACE_BYTES.min(free.max(held)))
    }

    /// Attaches the state's scratch to `workspace`, first growing it to
    /// `bytes` if it is smaller. A larger buffer is attached to the state's
    /// own workspace descriptor before the smaller one is freed, so no
    /// descriptor holds freed memory.
    fn attach_scratch(
        &mut self,
        workspace: OpaqueHandle,
        bytes: i64,
        resource: &'static str,
    ) -> Result<(), EngineError> {
        let session = Rc::clone(&self.session);
        let (calls, handle) = (session.calls(), self.handle());
        let needed =
            usize::try_from(bytes).map_err(|_| EngineError::ResourceSizeOverflow { resource })?;
        let held = self.scratch.map_or(0, |(_, size)| size);
        if needed > held {
            let (free, _) = calls.memory_info()?;
            validate_workspace_size(needed, MAXIMUM_WORKSPACE_BYTES, free)?;
            let grown = calls.allocate(needed)?;
            let previous = self.scratch.replace((grown, needed));
            // Only the state's own descriptor can hold the previous buffer:
            // a read's descriptor is new, and attached after this.
            let attached = self
                .workspace()
                .and_then(|own| calls.set_workspace(handle, own, grown, size_i64(needed)?));
            if let Some((old, _)) = previous {
                match attached {
                    Ok(()) => calls.free(old)?,
                    Err(error) => {
                        self.retired_scratch.push(old);
                        return Err(error);
                    }
                }
            } else {
                attached?;
            }
        }
        let (scratch, size) = self
            .scratch
            .expect("the scratch was grown to the call's size");
        calls.set_workspace(handle, workspace, scratch, size_i64(size)?)
    }

    /// Evaluates `Σₖ cₖ⟨ψ|Pₖ|ψ⟩` and `⟨ψ|ψ⟩` on the computed state in one
    /// expectation, whose objects are released before it returns.
    ///
    /// Each factor is a one-mode tensor. For one mode, `AppendProduct`'s
    /// forward mode order and `ApplyTensorOperator`'s reversed order
    /// coincide, so the textbook row-major matrix is read as `⟨i|P|j⟩`;
    /// this matters for Y, the only non-symmetric Pauli.
    fn query(&mut self, products: &[Product]) -> Result<(Complex64, Complex64), EngineError> {
        let mark = self.allocations.len();
        let query = self.open_query(products);
        let release = self.release_query(mark);
        combine_execution_and_cleanup(query, release)
    }

    fn open_query(&mut self, products: &[Product]) -> Result<(Complex64, Complex64), EngineError> {
        let session = Rc::clone(&self.session);
        let (calls, handle, stream) = (session.calls(), self.handle(), session.stream());
        let operator = calls.create_network_operator(handle, self.state_extents.as_ref())?;
        self.network_operator = Some(operator);
        // One device tensor per distinct factor matrix, shared by every
        // factor that uses it.
        let mut tensors: Vec<([Complex64Abi; 4], OpaqueHandle)> = Vec::new();
        for product in products {
            let mut factor_modes = Vec::with_capacity(product.factors.len());
            let mut factor_tensors = Vec::with_capacity(product.factors.len());
            for (mode, matrix) in &product.factors {
                let tensor = if let Some((_, tensor)) = tensors.iter().find(|(m, _)| m == matrix) {
                    *tensor
                } else {
                    let tensor = self.allocate_complex(matrix.len(), "expectation factor")?;
                    calls.copy_to_device(tensor, matrix)?;
                    tensors.push((*matrix, tensor));
                    tensor
                };
                factor_modes.push(vec![*mode].into_boxed_slice());
                factor_tensors.push(tensor);
            }
            calls.append_product(
                handle,
                operator,
                product.coefficient,
                &factor_modes,
                &factor_tensors,
            )?;
        }
        let expectation = calls.create_expectation(handle, self.state()?, operator)?;
        self.expectation = Some(expectation);
        calls.configure_expectation_hyper_samples(
            handle,
            expectation,
            EXPECTATION_HYPER_SAMPLES,
        )?;
        let query_workspace = calls.create_workspace(handle)?;
        self.query_workspace = Some(query_workspace);
        let available = self.available_workspace_bytes()?;
        calls.prepare_expectation(handle, expectation, available, query_workspace, stream)?;
        let bytes = calls.workspace_size(handle, query_workspace)?;
        if bytes < 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("the expectation's workspace size is {bytes}"),
            });
        }
        if bytes > 0 {
            self.attach_scratch(query_workspace, bytes, "expectation workspace")?;
        }
        calls.synchronize_copies()?;
        let values = calls.compute_expectation(handle, expectation, query_workspace, stream)?;
        calls.synchronize_stream(stream)?;
        Ok(values)
    }

    /// Closes the expectation's objects, then frees the buffers allocated
    /// since `mark`.
    fn release_query(&mut self, mark: usize) -> Result<(), EngineError> {
        let closed = self.close_query_lifecycle();
        let freed = self.free_since(mark);
        closed.and(freed)
    }

    /// Frees the allocations made since `mark`, newest first, once the
    /// stream has finished with them; attempts every free and returns the
    /// first failure.
    fn free_since(&mut self, mark: usize) -> Result<(), EngineError> {
        let session = Rc::clone(&self.session);
        let calls = session.calls();
        let mut first_error = calls.synchronize_stream(session.stream()).err();
        while self.allocations.len() > mark {
            let Some(allocation) = self.allocations.pop() else {
                break;
            };
            if let Err(error) = calls.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn close_query_lifecycle(&mut self) -> Result<(), EngineError> {
        let calls = self.session.calls();
        let mut first_error = None;
        if let Some(workspace) = self.query_workspace.take()
            && let Err(error) = calls.destroy_workspace(workspace)
        {
            first_error = Some(error);
        }
        if let Some(expectation) = self.expectation.take()
            && let Err(error) = calls.destroy_expectation(expectation)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(operator) = self.network_operator.take()
            && let Err(error) = calls.destroy_network_operator(operator)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }

    fn allocate_complex(
        &mut self,
        elements: usize,
        resource: &'static str,
    ) -> Result<OpaqueHandle, EngineError> {
        let bytes = complex_bytes(elements, resource)?;
        let allocation = self.session.calls().allocate(bytes)?;
        self.allocations.push(allocation);
        Ok(allocation)
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

    /// Releases every native object and buffer of the state, the
    /// descriptors before the scratch they hold; attempts every release and
    /// returns the first failure. Later closes do nothing.
    pub(super) fn close(&mut self) -> Result<(), EngineError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let session = Rc::clone(&self.session);
        let calls = session.calls();
        // The engine's device, which another engine's call may have changed.
        let mut released = vec![
            session.bind_device(),
            calls.synchronize_stream(session.stream()),
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
        while let Some(allocation) = self.allocations.pop() {
            released.push(calls.free(allocation));
        }
        if let Some(handle) = self.own_handle.take() {
            released.push(calls.destroy_handle(handle));
        }
        released.into_iter().collect()
    }
}

impl Drop for NativeState {
    fn drop(&mut self) {
        report_failed_release("an MPS state", self.close());
    }
}

/// The native mode of one of the caller's sites, which the contract's
/// checks have bounded by the site count.
fn mode(site: usize) -> Result<i32, EngineError> {
    i32::try_from(site).map_err(|_| EngineError::ResourceSizeOverflow {
        resource: "native mode",
    })
}

fn complex_bytes(elements: usize, resource: &'static str) -> Result<usize, EngineError> {
    elements
        .checked_mul(size_of::<Complex64Abi>())
        .ok_or(EngineError::ResourceSizeOverflow { resource })
}

fn size_i64(bytes: usize) -> Result<i64, EngineError> {
    i64::try_from(bytes).map_err(|_| EngineError::ResourceSizeOverflow {
        resource: "device workspace",
    })
}

/// One product of the one-site diagonal operators `diagonals`.
fn diagonal_product(diagonals: &BTreeMap<i32, [f64; 2]>) -> Product {
    let zero = Complex64Abi::new(0.0, 0.0);
    Product {
        coefficient: Complex64::new(1.0, 0.0),
        factors: diagonals
            .iter()
            .map(|(&mode, &[d0, d1])| {
                (
                    mode,
                    [
                        Complex64Abi::new(d0, 0.0),
                        zero,
                        zero,
                        Complex64Abi::new(d1, 0.0),
                    ],
                )
            })
            .collect(),
    }
}

/// The squared norm the library reported beside an expectation, which must
/// be finite, real to rounding and positive: every read divides by it, and
/// it is the retained weight, which the contract keeps above zero.
fn positive_squared_norm(squared_norm: Complex64) -> Result<f64, EngineError> {
    let invalid = |reason: &str| EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    };
    if !(squared_norm.re.is_finite() && squared_norm.im.is_finite()) {
        return Err(invalid(
            "the expectation returned a non-finite squared norm",
        ));
    }
    if squared_norm.im.abs() > 1.0e-12 * squared_norm.re.abs().max(1.0) {
        return Err(invalid(
            "the expectation's squared norm has a material imaginary part",
        ));
    }
    if squared_norm.re <= 0.0 {
        return Err(invalid(
            "the expectation returned a nonpositive squared norm",
        ));
    }
    Ok(squared_norm.re)
}

/// `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`: a truncated MPS is not normalized, so every read
/// divides by the norm the library reports beside it, and rejects a
/// non-finite value or a norm `positive_squared_norm` rejects.
fn normalize_expectation(
    raw_expectation: Complex64,
    squared_norm: Complex64,
) -> Result<Complex64, EngineError> {
    let finite = |value: Complex64| value.re.is_finite() && value.im.is_finite();
    let invalid = |reason: &str| EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    };
    if !finite(raw_expectation) {
        return Err(invalid("the expectation returned a non-finite value"));
    }
    let normalized = raw_expectation / positive_squared_norm(squared_norm)?;
    if finite(normalized) {
        Ok(normalized)
    } else {
        Err(invalid("the normalized expectation is non-finite"))
    }
}

fn validate_workspace_size(
    required: usize,
    policy_maximum: usize,
    available: usize,
) -> Result<(), EngineError> {
    let maximum = policy_maximum.min(available);
    if required > maximum {
        return Err(EngineError::WorkspaceLimitExceeded { required, maximum });
    }
    Ok(())
}
