//! One cuTensorNet state of the engine: its native objects, the operators
//! applied to it, and the computed MPS every read uses.
//!
//! `MpsExecution` is ④'s (`library/simulation/mps_execution.rs` in M0),
//! moved without the circuit: it takes matrices instead of gates, and keeps
//! its state open between calls instead of running one circuit to the end.
//!
//! ```text
//! create_state ─ apply* ─┬─ finalize_mps + configure (first compute only)
//!                        └─ prepare_state ─ compute_state ─ capture_mps ─► realized MpsShape
//!                              │
//!         ┌────────────────────┼─────────────────────┐
//!   expectation query    sampler on the state   projection: apply a scaled
//!   (P(One), ⟨O⟩, P(One │ given))                 projector, compute again
//! ```
//!
//! - Every read needs a computed state. After an operator is applied, the
//!   next read computes the state again, as ④'s branch qualification did
//!   after its projection; `finalize_mps` is called once, before the first
//!   compute.
//! - Each compute is captured (`capture_mps`): the computed MPS becomes the
//!   state's start, and the library forgets the operators applied so far, so
//!   the next compute applies only the operators applied since. Without it
//!   cuTensorNet computes from |0…0⟩ with every operator, and a read after
//!   each of a shot's `M` measurements costs `O(M·G)` for `G` gates instead
//!   of `O(G)`. Reads follow the capture, as in ④'s branch qualification
//!   (capture, then the branch masses). The policy can turn capture off,
//!   only to measure that cost. The engine keeps its own log of the
//!   operators, which a fork replays from |0…0⟩; nothing reads the
//!   library's operator ids.
//! - A read is one native expectation or sampler whose descriptors and
//!   buffers are released before it returns, as ④'s Query.
//! - A fork is a new native state with every logged operator applied again
//!   (cuTensorNet 2.13 has no state copy in the loader manifest), so it
//!   costs a recompute from |0…0⟩ (D21). It is not a copy of the parent's
//!   captured MPS: under truncation the parent, computed in steps, and the
//!   fork, computed once, may differ within the truncation error (the A100
//!   qualification's `fork_cost` measures by how much). Nothing in M1 forks
//!   (see [`MpsEngine::fork`](tensornet::MpsEngine::fork) on the engine): a
//!   strategy that branches shots from a shared prefix is the case to
//!   revisit (DECISIONS G8).
//! - Operator tensors stay allocated as long as the native state they were
//!   applied to, as ④'s did: `cutensornetStateApplyTensorOperator` says the
//!   tensor "must stay valid for the whole lifetime of the tensor network
//!   state", and `cutensornetStateCaptureMPS` deletes the operators but says
//!   nothing of their tensors. They are kept apart from the other buffers, so
//!   the per-compute bookkeeping does not scan them. A restart ends the
//!   state, and frees them.
//! - A compute's scratch is freed as soon as the stream has finished the
//!   compute: scratch is only "drawn … and released back once done" by the
//!   call that uses it (`cutensornet.h`, on scratch workspaces), so holding
//!   it until the next compute, as the engine did, doubled the peak to the
//!   last two computes' scratch. It stays attached to the state's workspace
//!   descriptor, as the sampler's does; the next compute or sample attaches
//!   its own before using it. A compute's output buffers are freed when the
//!   next compute has succeeded and been captured, since the captured MPS
//!   may be read from them until then, and a read's scratch when the read
//!   returns, so a long-lived state does not grow with its reads (④ computed
//!   and sampled a bounded number of times, and kept them all).
//! - The prepare is told the workspace it may use is the policy's budget or
//!   the device's free memory, whichever is less, as CUDA-Q tells it the size
//!   of its scratch pad, and each compute asks for the policy's
//!   `compute_workspace` size, recommended or minimum.
//! - With a `restart_period` of `K` (the engine's policy has one), the
//!   native state is replaced every `K` computes, before the next operator,
//!   by a new one whose start is the last computed MPS
//!   (`cutensornetStateInitializeMPS`, with the extents and strides that
//!   compute reported), finalized and configured as the first one was.
//!   Capture is documented as "equivalent to creating a `cutensornetState_t`
//!   with the previous output MPS state as the initial MPS state", so a
//!   restart changes no value. The new state reads the MPS from the last
//!   compute's outputs, which are freed, as before, after its first compute
//!   and capture. With `fresh_handle_at_restart`, as in the engine's policy,
//!   the new state is created on a cuTensorNet handle of its own, with its
//!   own workspace descriptor, and the previous own handle is destroyed
//!   after the old state; the session's handle never is. The A100 measured
//!   the recommended workspace growing by about 16 MiB per compute on a
//!   long-lived handle, restarts on it included, and flat with a fresh
//!   handle every 10 computes (`history_growth`; the policy's
//!   `RESTART_PERIOD`). A fork starts on the session's handle, as any new
//!   state, and moves at its own restarts, so its parent's restarts never
//!   destroy a handle it is on.
//! - A native failure while the state is changed or computed leaves it
//!   unusable: every later call reports [`EngineError::Poisoned`].

#[cfg(any(test, feature = "qualification"))]
use super::profile::{NativeHandle, WorkspaceSizes};
use super::{
    EngineError, OpaqueHandle, Session,
    error::combine_execution_and_cleanup,
    execution::{
        MpsTarget, OutputMetadata, StateF64Attribute, StateU32Configuration, convert_layout,
        validate_realized_chain,
    },
    ffi::Complex64Abi,
    memory_workspace::WorkspacePreference,
    profile::StateProfile,
    sampler::{PreparedSampler, SamplerContext, SamplingRequest},
};
use num_complex::Complex64;
use std::{collections::BTreeMap, mem::size_of, rc::Rc, time::Instant};
use tensornet::{MpsShape, PauliSum, SampleSeed};

/// Hyper-samples of each expectation's contraction path search, ④'s B2
/// value.
const EXPECTATION_HYPER_SAMPLES: i32 = 8;

/// Hyper-samples of the sampler's contraction path search, ④'s value.
const SAMPLER_HYPER_SAMPLES: i32 = 8;

/// How far past 1 a probability given to `project` may be, as
/// `tensornet::reference::ExactReference` allows.
const PROBABILITY_ROUNDING: f64 = 1e-9;

/// The weight of a condition below which the ratio ⟨Π·P₁⟩/⟨Π⟩ is refused.
/// It is needed because on a GPU an impossible condition comes back as
/// rounding noise, not zero, and a ratio of two such values is no
/// probability. Below it, a rare condition can't be told from an impossible
/// one: a legitimate condition on t random values weighs about 2⁻ᵗ, so one
/// on more than about 40 is refused too. The value is a guess, not measured
/// on a GPU. Only a batch with several measurements conditions on earlier
/// values; M1–M3's walk ends each batch at its measurement, so they never
/// do (DECISIONS E16).
const IMPOSSIBLE_CONDITION: f64 = 1e-12;

/// How far `M†M` may be from the identity, element by element, for `M` to
/// be passed to cuTensorNet as unitary.
const UNITARY_TOLERANCE: f64 = 1e-12;

/// One operator applied to the state, kept to replay it in a fork. Its
/// modes are the ones handed to the native call, so they live as long as
/// the state.
#[derive(Clone, Debug, PartialEq)]
struct Operator {
    modes: Box<[i32]>,
    matrix: Box<[Complex64Abi]>,
    unitary: bool,
}

/// One product of one-site factors in an expectation's operator.
struct Product {
    coefficient: Complex64,
    factors: Vec<(i32, [Complex64Abi; 4])>,
}

pub(super) struct MpsExecution {
    session: Rc<Session>,
    state: Option<OpaqueHandle>,
    workspace: Option<OpaqueHandle>,
    query_workspace: Option<OpaqueHandle>,
    network_operator: Option<OpaqueHandle>,
    expectation: Option<OpaqueHandle>,
    /// Every device buffer of the state but its operator tensors.
    allocations: Vec<OpaqueHandle>,
    /// The device tensors of the operators applied to the native state,
    /// released with it.
    operator_tensors: Vec<OpaqueHandle>,
    /// The outputs of the last compute, one per site, also in `allocations`.
    computed_buffers: Vec<OpaqueHandle>,
    /// The extents and strides the last compute wrote its outputs with,
    /// which a restart initializes the new state from.
    computed_layout: Option<OutputMetadata>,
    computes_since_restart: usize,
    /// The cuTensorNet handle the state moved to at its last restart, if
    /// the policy gives each restart a fresh one: the state, its workspace
    /// and its reads use it instead of the session's, and it is destroyed
    /// after them.
    own_handle: Option<OpaqueHandle>,
    /// The computes done on `own_handle`.
    own_handle_computes: usize,
    profile: StateProfile,
    state_extents: Box<[i64]>,
    operators: Vec<Operator>,
    target: MpsTarget,
    finalized: bool,
    /// The chain of the last compute, while no operator has been applied
    /// since.
    realized: Option<MpsShape>,
    poisoned: Option<String>,
    closed: bool,
}

impl MpsExecution {
    /// Creates the |0…0⟩ state of `capacity`'s qubits.
    pub(super) fn new(session: &Rc<Session>, capacity: &MpsShape) -> Result<Self, EngineError> {
        let target = MpsTarget::from_capacity(capacity)?;
        let state_extents = vec![2; target.shape().site_count()].into_boxed_slice();
        let handle = session.resources.handle();
        let state = session
            .resources
            .api()
            .create_state(handle, state_extents.as_ref())?;
        let mut execution = Self {
            session: Rc::clone(session),
            state: Some(state),
            workspace: None,
            query_workspace: None,
            network_operator: None,
            expectation: None,
            allocations: Vec::new(),
            operator_tensors: Vec::new(),
            computed_buffers: Vec::new(),
            computed_layout: None,
            computes_since_restart: 0,
            own_handle: None,
            own_handle_computes: 0,
            profile: StateProfile::default(),
            state_extents,
            operators: Vec::new(),
            target,
            finalized: false,
            realized: None,
            poisoned: None,
            closed: false,
        };
        match session.resources.api().create_workspace(handle) {
            Ok(workspace) => execution.workspace = Some(workspace),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), execution.close());
            }
        }
        Ok(execution)
    }

    pub(super) fn session(&self) -> &Rc<Session> {
        &self.session
    }

    /// What the state's native calls have cost so far, and what it holds.
    #[cfg_attr(
        not(any(test, feature = "qualification")),
        allow(dead_code, reason = "read by the qualification only")
    )]
    pub(super) fn profile(&self) -> StateProfile {
        StateProfile {
            live_allocations: self.allocations.len() + self.operator_tensors.len(),
            operator_tensors: self.operator_tensors.len(),
            handle_computes: if self.own_handle.is_some() {
                self.own_handle_computes
            } else {
                self.session.handle_computes.get()
            },
            ..self.profile.clone()
        }
    }

    pub(super) fn apply(
        &mut self,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        self.check_live()?;
        let operator = self.operator(sites, matrix)?;
        self.poison_on_error(|execution| execution.register(operator))
    }

    /// The operator `apply` would apply, if it takes it.
    fn operator(&self, sites: &[usize], matrix: &[Complex64]) -> Result<Operator, EngineError> {
        if !(1..=2).contains(&sites.len()) {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "the cuTensorNet engine applies operators on one or two sites, not {}",
                    sites.len()
                ),
            });
        }
        let modes = self.modes(sites)?;
        let expected = 1_usize << (2 * sites.len());
        if matrix.len() != expected {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "an operator on {} sites has {expected} elements, not {}",
                    sites.len(),
                    matrix.len()
                ),
            });
        }
        if matrix
            .iter()
            .any(|z| !(z.re.is_finite() && z.im.is_finite()))
        {
            return Err(EngineError::InvalidRequest {
                reason: "an operator has a non-finite element".to_string(),
            });
        }
        Ok(Operator {
            modes: modes.into_boxed_slice(),
            matrix: matrix.iter().copied().map(Complex64Abi::from).collect(),
            unitary: is_unitary(matrix, 1 << sites.len()),
        })
    }

    pub(super) fn probability_of_one(&mut self, site: usize) -> Result<f64, EngineError> {
        self.conditional_probability_of_one(site, &[])
    }

    /// P(One) of `site` given `given`: ⟨Π·P₁⟩ / ⟨Π⟩ with Π the product of
    /// the given values' projectors, two expectations on the computed state,
    /// no copy of it. With nothing given, it is ⟨P₁⟩ / ⟨ψ|ψ⟩. A condition
    /// of weight at most [`IMPOSSIBLE_CONDITION`] is refused.
    pub(super) fn conditional_probability_of_one(
        &mut self,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, EngineError> {
        self.check_live()?;
        let mode = self.mode(site)?;
        let mut condition: BTreeMap<i32, [f64; 2]> = BTreeMap::new();
        for &(at, one) in given {
            let diagonal = condition.entry(self.mode(at)?).or_insert([1.0, 1.0]);
            diagonal[usize::from(!one)] = 0.0;
        }
        let mut numerator = condition.clone();
        numerator.entry(mode).or_insert([1.0, 1.0])[0] = 0.0;
        self.ensure_computed()?;
        let probability = if given.is_empty() {
            let (value, squared_norm) = self.query(&[diagonal_product(&numerator)])?;
            normalize_expectation(value, squared_norm)?.re
        } else {
            let (value, squared_norm) = self.query(&[diagonal_product(&condition)])?;
            let condition = normalize_expectation(value, squared_norm)?.re;
            if condition <= IMPOSSIBLE_CONDITION {
                return Err(EngineError::InvalidRequest {
                    reason: format!(
                        "the condition's probability {condition} is below the ratio's precision \
                         ({IMPOSSIBLE_CONDITION}): it is impossible, or only rare"
                    ),
                });
            }
            let (value, squared_norm) = self.query(&[diagonal_product(&numerator)])?;
            normalize_expectation(value, squared_norm)?.re / condition
        };
        Ok(probability.clamp(0.0, 1.0))
    }

    /// Projects `site` on `one` and divides by √`probability`: a non-unitary
    /// one-site operator, as ④'s `apply_projection`.
    pub(super) fn project(
        &mut self,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), EngineError> {
        self.check_live()?;
        let mode = self.mode(site)?;
        if !(probability > 0.0 && probability <= 1.0 + PROBABILITY_ROUNDING) {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "{probability} is not the probability of a value that can be drawn"
                ),
            });
        }
        let scale = 1.0 / probability.sqrt();
        let zero = Complex64Abi::new(0.0, 0.0);
        let on = Complex64Abi::new(scale, 0.0);
        let matrix = if one {
            [zero, zero, zero, on]
        } else {
            [on, zero, zero, zero]
        };
        let operator = Operator {
            modes: vec![mode].into_boxed_slice(),
            matrix: matrix.into(),
            unitary: false,
        };
        self.poison_on_error(|execution| execution.register(operator))
    }

    /// A new native state with every operator of this one applied again,
    /// in order.
    pub(super) fn fork(&self) -> Result<Self, EngineError> {
        self.check_live()?;
        let mut copy = Self::new(&self.session, self.target.capacity())?;
        for operator in &self.operators {
            copy.register(operator.clone())?;
        }
        Ok(copy)
    }

    pub(super) fn sample(
        &mut self,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, EngineError> {
        self.check_live()?;
        let modes = self.modes(sites)?;
        if shots == 0 || modes.is_empty() {
            return Ok(Vec::new());
        }
        let seed =
            i32::try_from(seed.get()).map_err(|_| EngineError::InvalidSamplerConfiguration {
                reason: "sampler seed does not fit the native seed",
            })?;
        let request = SamplingRequest::new(shots, SAMPLER_HYPER_SAMPLES, Some(seed), seed)?;
        self.ensure_computed()?;
        let output = self.sample_modes(&modes, request)?;
        if output.len() != shots * modes.len() {
            return Err(EngineError::InvalidNativeResult {
                reason: "sampler output shape does not match the sampled modes".to_string(),
            });
        }
        output
            .iter()
            .map(|&value| match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(EngineError::InvalidNativeResult {
                    reason: "sampler returned a non-bit value for a qubit mode".to_string(),
                }),
            })
            .collect()
    }

    /// `c_I + Σₖ cₖ⟨ψ|Pₖ|ψ⟩ / ⟨ψ|ψ⟩`: the identity coefficient on the host,
    /// the rest as one native expectation, as ④'s `evaluate_observables`.
    pub(super) fn expectation(&mut self, observable: &PauliSum) -> Result<Complex64, EngineError> {
        self.check_live()?;
        if let Some(site) = observable.max_site() {
            self.mode(site)?;
        }
        if observable.terms().is_empty() {
            return Ok(observable.identity());
        }
        let products = observable
            .terms()
            .iter()
            .map(|term| {
                let factors = term
                    .factors()
                    .iter()
                    .map(|&(site, pauli)| {
                        Ok((self.mode(site)?, pauli.matrix().map(Complex64Abi::from)))
                    })
                    .collect::<Result<Vec<_>, EngineError>>()?;
                Ok(Product {
                    coefficient: term.coefficient(),
                    factors,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        self.ensure_computed()?;
        let (value, squared_norm) = self.query(&products)?;
        Ok(observable.identity() + normalize_expectation(value, squared_norm)?)
    }

    pub(super) fn shape(&mut self) -> Result<MpsShape, EngineError> {
        self.check_live()?;
        let realized = self.ensure_computed()?;
        self.target.logical(realized)
    }

    /// ⟨ψ|ψ⟩ of the computed state, as the library reports it beside an
    /// expectation: one Query of the identity on the first site, whose
    /// squared norm is the answer. The norm is the retained weight because
    /// `configure` pins `SvdNormalizationNone`; the padding of a lone site
    /// stays |0⟩ and changes nothing.
    pub(super) fn retained_weight(&mut self) -> Result<f64, EngineError> {
        self.check_live()?;
        let first = self.mode(0)?;
        self.ensure_computed()?;
        let (_, squared_norm) =
            self.query(&[diagonal_product(&BTreeMap::from([(first, [1.0, 1.0])]))])?;
        real_squared_norm(squared_norm)
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

    /// The mode of one of the caller's sites. The bound is the capacity's
    /// site count, not the native state's, so a padding site is out of
    /// range like any other.
    fn mode(&self, site: usize) -> Result<i32, EngineError> {
        if site >= self.target.site_count() {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "site {site} is outside the {}-site state",
                    self.target.site_count()
                ),
            });
        }
        i32::try_from(site).map_err(|_| EngineError::InvalidRequest {
            reason: format!("site {site} does not fit the native mode identifier"),
        })
    }

    /// The modes of distinct `sites` of the state.
    fn modes(&self, sites: &[usize]) -> Result<Vec<i32>, EngineError> {
        if let Some((at, _)) = sites
            .iter()
            .enumerate()
            .find(|&(at, site)| sites[..at].contains(site))
        {
            return Err(EngineError::InvalidRequest {
                reason: format!("site {} is named twice", sites[at]),
            });
        }
        sites.iter().map(|&site| self.mode(site)).collect()
    }

    /// Uploads `operator` and applies it to the state. The matrix is copied
    /// verbatim: cuTensorNet reads the row-major `M[out][in]` layout with
    /// default strides on the modes in order, first most significant, which
    /// is `MpsEngine::apply`'s convention (④'s `fixture_operator`).
    ///
    /// If the policy's restart period has passed since the last restart,
    /// the native state is restarted first.
    fn register(&mut self, operator: Operator) -> Result<(), EngineError> {
        if let Some(period) = self.session.policy.restart_period
            && self.realized.is_some()
            && self.computes_since_restart >= period.get()
        {
            self.restart()?;
        }
        let started = Instant::now();
        let session = Rc::clone(&self.session);
        let api = session.resources.api();
        let bytes = operator
            .matrix
            .len()
            .checked_mul(size_of::<Complex64Abi>())
            .ok_or(EngineError::ResourceSizeOverflow {
                resource: "operator tensor",
            })?;
        let tensor = api.allocate(bytes)?;
        self.operator_tensors.push(tensor);
        api.copy_to_device(tensor, &operator.matrix)?;
        self.realized = None;
        api.apply_tensor_operator(
            self.handle(),
            self.state()?,
            &operator.modes,
            tensor,
            operator.unitary,
        )?;
        self.operators.push(operator);
        self.profile.apply += started.elapsed();
        Ok(())
    }

    /// Replaces the native state, which must have been computed and
    /// captured since its last operator, by a new one whose start is that
    /// computed MPS, finalized and configured as the first one, then
    /// releases the old state and its operator tensors. The new state reads
    /// the MPS from `computed_buffers`, which the next compute frees once it
    /// has captured its own.
    ///
    /// If the policy asks for a fresh handle at restart, the new state, and
    /// a new workspace descriptor for it, are created on a new handle; the
    /// old workspace, then the old handle, are destroyed after the old
    /// state, unless the old handle is the session's.
    fn restart(&mut self) -> Result<(), EngineError> {
        let started = Instant::now();
        let session = Rc::clone(&self.session);
        let api = session.resources.api();
        let fresh_handle = if session.policy.fresh_handle_at_restart {
            Some(api.create_handle()?)
        } else {
            None
        };
        let handle = fresh_handle.unwrap_or_else(|| self.handle());
        let started_state = self.start_from_computed(handle).and_then(|fresh| {
            if fresh_handle.is_none() {
                return Ok((fresh, None));
            }
            match api.create_workspace(handle) {
                Ok(workspace) => Ok((fresh, Some(workspace))),
                Err(error) => combine_execution_and_cleanup(Err(error), api.destroy_state(fresh)),
            }
        });
        let (fresh, fresh_workspace) = match started_state {
            Ok(started_state) => started_state,
            Err(error) => {
                let cleanup = fresh_handle.map_or(Ok(()), |handle| api.destroy_handle(handle));
                return combine_execution_and_cleanup(Err(error), cleanup);
            }
        };
        let previous = self.state.replace(fresh);
        let mut first_error = previous.and_then(|state| api.destroy_state(state).err());
        if let Some(workspace) = fresh_workspace
            && let Some(previous) = self.workspace.replace(workspace)
            && let Err(error) = api.destroy_workspace(previous)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        for tensor in std::mem::take(&mut self.operator_tensors) {
            if let Err(error) = api.free(tensor)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Some(handle) = fresh_handle {
            self.own_handle_computes = 0;
            self.profile.fresh_handles += 1;
            if let Some(previous) = self.own_handle.replace(handle)
                && let Err(error) = api.destroy_handle(previous)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        self.computes_since_restart = 0;
        self.profile.restarts += 1;
        self.profile.restart += started.elapsed();
        first_error.map_or(Ok(()), Err)
    }

    /// A new native state on `handle` whose start is the last computed MPS,
    /// finalized and configured as the first one; on failure it is
    /// destroyed.
    fn start_from_computed(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError> {
        let api = self.session.resources.api();
        let Some(layout) = &self.computed_layout else {
            return Err(EngineError::InvalidRequest {
                reason: "a restart needs a computed MPS".to_string(),
            });
        };
        let fresh = api.create_state(handle, self.state_extents.as_ref())?;
        let initialized = api
            .initialize_mps(handle, fresh, layout, &self.computed_buffers)
            .and_then(|()| api.finalize_mps(handle, fresh, &self.target))
            .and_then(|()| self.configure(handle, fresh));
        match initialized {
            Ok(()) => Ok(fresh),
            Err(error) => combine_execution_and_cleanup(Err(error), api.destroy_state(fresh)),
        }
    }

    /// The scratch cuTensorNet asks for to compute a new state on `handle`
    /// whose start is the last computed MPS and whose only operators are
    /// `operators`, after one prepare; everything the probe creates is
    /// released before it returns, and the state is not changed.
    #[cfg(any(test, feature = "qualification"))]
    pub(super) fn fresh_state_workspace(
        &self,
        handle: NativeHandle,
        operators: &[(&[usize], &[Complex64])],
    ) -> Result<WorkspaceSizes, EngineError> {
        self.check_live()?;
        if self.computed_layout.is_none() {
            return Err(EngineError::InvalidRequest {
                reason: "the workspace probe needs a computed state".to_string(),
            });
        }
        let operators = operators
            .iter()
            .map(|(sites, matrix)| self.operator(sites, matrix))
            .collect::<Result<Vec<_>, _>>()?;
        let session = Rc::clone(&self.session);
        let (api, stream) = (session.resources.api(), session.resources.stream());
        let (probe_handle, created_handle, handle_computes) = match handle {
            NativeHandle::Engine => (
                session.resources.handle(),
                None,
                session.handle_computes.get(),
            ),
            NativeHandle::Fresh => {
                let created = api.create_handle()?;
                (created, Some(created), 0)
            }
        };
        let mut state = None;
        let mut workspace = None;
        let mut tensors = Vec::new();
        let measured = (|| {
            let probe = self.start_from_computed(probe_handle)?;
            state = Some(probe);
            for operator in &operators {
                let bytes = operator
                    .matrix
                    .len()
                    .checked_mul(size_of::<Complex64Abi>())
                    .ok_or(EngineError::ResourceSizeOverflow {
                        resource: "operator tensor",
                    })?;
                let tensor = api.allocate(bytes)?;
                tensors.push(tensor);
                api.copy_to_device(tensor, &operator.matrix)?;
                api.apply_tensor_operator(
                    probe_handle,
                    probe,
                    &operator.modes,
                    tensor,
                    operator.unitary,
                )?;
            }
            let probe_workspace = api.create_workspace(probe_handle)?;
            workspace = Some(probe_workspace);
            let (free_bytes, _) = api.memory_info()?;
            api.prepare_state(
                probe_handle,
                probe,
                session.policy.maximum_workspace_bytes.min(free_bytes),
                probe_workspace,
                stream,
            )?;
            Ok(WorkspaceSizes {
                minimum_bytes: api.workspace_size(
                    probe_handle,
                    probe_workspace,
                    WorkspacePreference::Min,
                )?,
                recommended_bytes: api.workspace_size(
                    probe_handle,
                    probe_workspace,
                    WorkspacePreference::Recommended,
                )?,
                handle_computes,
            })
        })();
        let mut first_error = api.synchronize_stream(stream).err();
        let mut keep = |result: Result<(), EngineError>| {
            if let Err(error) = result
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        };
        if let Some(workspace) = workspace {
            keep(api.destroy_workspace(workspace));
        }
        if let Some(state) = state {
            keep(api.destroy_state(state));
        }
        for tensor in tensors {
            keep(api.free(tensor));
        }
        if let Some(handle) = created_handle {
            keep(api.destroy_handle(handle));
        }
        combine_execution_and_cleanup(measured, first_error.map_or(Ok(()), Err))
    }

    /// The handle the state is on: its own since a restart with a fresh
    /// handle, the session's otherwise.
    fn handle(&self) -> OpaqueHandle {
        self.own_handle
            .unwrap_or_else(|| self.session.resources.handle())
    }

    /// The chain of the computed state, computing it first if an operator
    /// was applied since the last compute.
    fn ensure_computed(&mut self) -> Result<MpsShape, EngineError> {
        if let Some(realized) = &self.realized {
            return Ok(realized.clone());
        }
        self.poison_on_error(|execution| {
            if !execution.finalized {
                execution.finalize()?;
                execution.finalized = true;
            }
            let realized = execution.materialize()?;
            execution.realized = Some(realized.clone());
            Ok(realized)
        })
    }

    /// Finalizes the state as a bond-capped MPS and applies the truncation
    /// configuration.
    fn finalize(&self) -> Result<(), EngineError> {
        let state = self.state()?;
        let handle = self.handle();
        self.session
            .resources
            .api()
            .finalize_mps(handle, state, &self.target)?;
        self.configure(handle, state)
    }

    fn configure(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError> {
        let api = self.session.resources.api();
        let policy = self.session.policy;
        api.configure_state_f64(
            handle,
            state,
            StateF64Attribute::SvdAbsoluteCutoff,
            policy.absolute_cutoff,
        )?;
        api.configure_state_f64(
            handle,
            state,
            StateF64Attribute::SvdRelativeCutoff,
            policy.relative_cutoff,
        )?;
        // No normalization is the library default, pinned so the norm stays
        // the retained weight whichever default a future release picks.
        api.configure_state_u32(handle, state, StateU32Configuration::SvdNormalizationNone)?;
        api.configure_state_u32(handle, state, StateU32Configuration::SvdAlgorithmGesvd)?;
        api.configure_state_u32(handle, state, StateU32Configuration::MpsGaugeSimple)
    }

    /// Computes the state into new output buffers and returns the chain the
    /// library realized, checked against the target.
    fn materialize(&mut self) -> Result<MpsShape, EngineError> {
        let session = Rc::clone(&self.session);
        let (api, handle, stream) = (
            session.resources.api(),
            self.handle(),
            session.resources.stream(),
        );
        let maximum_workspace_bytes = session.policy.maximum_workspace_bytes;
        let state = self.state()?;
        let mark = self.allocations.len();
        let started = Instant::now();
        let output_elements = self.target.shape().element_counts();
        let mut outputs = Vec::with_capacity(output_elements.len());
        for elements in &output_elements {
            outputs.push(self.allocate_complex(*elements, "MPS output")?);
        }
        let (free_before_bytes, _) = api.memory_info()?;
        self.profile.allocate += started.elapsed();
        let workspace = self.workspace()?;
        let started = Instant::now();
        api.prepare_state(
            handle,
            state,
            maximum_workspace_bytes.min(free_before_bytes),
            workspace,
            stream,
        )?;
        self.profile.prepare += started.elapsed();
        let started = Instant::now();
        let minimum_bytes = api.workspace_size(handle, workspace, WorkspacePreference::Min)?;
        let recommended_bytes =
            api.workspace_size(handle, workspace, WorkspacePreference::Recommended)?;
        self.profile.workspace_query += started.elapsed();
        self.profile.minimum_workspace_bytes = minimum_bytes;
        self.profile.recommended_workspace_bytes = recommended_bytes;
        let workspace_bytes = match session.policy.compute_workspace {
            WorkspacePreference::Min => minimum_bytes,
            WorkspacePreference::Recommended => recommended_bytes,
        };
        if workspace_bytes <= 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("the compute's workspace size is {workspace_bytes}"),
            });
        }
        let workspace_size =
            usize::try_from(workspace_bytes).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "device workspace",
            })?;
        validate_workspace_size(workspace_size, maximum_workspace_bytes, free_before_bytes)?;
        let started = Instant::now();
        let scratch_mark = self.allocations.len();
        let scratch = self.allocate_scratch(workspace_size, "device workspace")?;
        self.profile.allocate += started.elapsed();
        api.set_workspace(handle, workspace, scratch, workspace_bytes)?;
        let mut metadata = OutputMetadata::new(&self.target);
        let started = Instant::now();
        api.compute_state(
            handle,
            state,
            workspace,
            &mut metadata,
            &mut outputs,
            stream,
        )?;
        api.synchronize_stream(stream)?;
        self.profile.compute += started.elapsed();
        // The stream has finished with the scratch; it stays attached to the
        // workspace descriptor, which the next compute or sample attaches
        // its own scratch to before using.
        let started = Instant::now();
        self.free_since(scratch_mark)?;
        self.profile.free += started.elapsed();
        // The library reports the shape it produced and, separately, the
        // strides it used to write it. Only the shape is part of the state.
        let realized =
            MpsShape::new(convert_layout("extent", &metadata.extents)?).map_err(|error| {
                EngineError::InvalidNativeResult {
                    reason: format!("realized MPS is not a valid chain: {error}"),
                }
            })?;
        validate_realized_chain(&realized, self.target.shape())?;
        if session.policy.capture_after_compute {
            let started = Instant::now();
            api.capture_mps(handle, state)?;
            self.profile.capture += started.elapsed();
        }
        self.count_compute();
        // The state now refers to the new buffers, as its computed MPS and,
        // once captured, as its start; the stream has finished with the old
        // ones, and a restart, whose state may have read them, has computed
        // and captured since.
        let started = Instant::now();
        let fresh = self.allocations[mark..].to_vec();
        let previous = std::mem::replace(&mut self.computed_buffers, fresh);
        self.computed_layout = Some(metadata);
        self.allocations
            .retain(|allocation| !previous.contains(allocation));
        let mut first_error = None;
        for allocation in previous {
            if let Err(error) = api.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        self.profile.free += started.elapsed();
        first_error.map_or(Ok(realized), Err)
    }

    /// Counts a compute: the state's, since its restart, and its handle's.
    fn count_compute(&mut self) {
        self.profile.computes += 1;
        self.computes_since_restart += 1;
        if self.own_handle.is_some() {
            self.own_handle_computes += 1;
        } else {
            let computes = &self.session.handle_computes;
            computes.set(computes.get() + 1);
        }
    }

    /// Evaluates `Σₖ cₖ⟨ψ|Pₖ|ψ⟩` and `⟨ψ|ψ⟩` on the computed state in one
    /// Query lifecycle, released before returning, as ④'s
    /// `execute_pauli_query`.
    ///
    /// Each factor is a one-mode tensor. For one mode, `AppendProduct`'s
    /// forward mode order and `ApplyTensorOperator`'s reversed order coincide,
    /// so the textbook row-major matrix is read as `⟨i|P|j⟩`; this matters
    /// for Y, the only non-symmetric Pauli. (④'s note, kept.)
    fn query(&mut self, products: &[Product]) -> Result<(Complex64, Complex64), EngineError> {
        if self.network_operator.is_some()
            || self.expectation.is_some()
            || self.query_workspace.is_some()
        {
            return Err(EngineError::InvalidRequest {
                reason: "this state already owns a Query lifecycle".to_string(),
            });
        }
        let started = Instant::now();
        let mark = self.allocations.len();
        let query = self.open_query(products);
        let release = self.release_query(mark);
        self.profile.reads += started.elapsed();
        combine_execution_and_cleanup(query, release)
    }

    fn open_query(&mut self, products: &[Product]) -> Result<(Complex64, Complex64), EngineError> {
        let session = Rc::clone(&self.session);
        let (api, handle, stream) = (
            session.resources.api(),
            self.handle(),
            session.resources.stream(),
        );
        let maximum_workspace_bytes = session.policy.maximum_workspace_bytes;
        let operator = api.create_network_operator(handle, self.state_extents.as_ref())?;
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
                    let tensor = self.allocate_complex(matrix.len(), "Query factor tensor")?;
                    api.copy_to_device(tensor, matrix)?;
                    tensors.push((*matrix, tensor));
                    tensor
                };
                factor_modes.push(vec![*mode].into_boxed_slice());
                factor_tensors.push(tensor);
            }
            api.append_product(
                handle,
                operator,
                product.coefficient,
                &factor_modes,
                &factor_tensors,
            )?;
        }
        let expectation = api.create_expectation(handle, self.state()?, operator)?;
        self.expectation = Some(expectation);
        api.configure_expectation_hyper_samples(handle, expectation, EXPECTATION_HYPER_SAMPLES)?;
        let query_workspace = api.create_workspace(handle)?;
        self.query_workspace = Some(query_workspace);
        let (free_before_bytes, _) = api.memory_info()?;
        api.prepare_expectation(
            handle,
            expectation,
            maximum_workspace_bytes,
            query_workspace,
            stream,
        )?;
        let workspace_bytes =
            api.workspace_size(handle, query_workspace, WorkspacePreference::Recommended)?;
        if workspace_bytes < 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("recommended Query workspace size is {workspace_bytes}"),
            });
        }
        let workspace_size =
            usize::try_from(workspace_bytes).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "Query device workspace",
            })?;
        validate_workspace_size(workspace_size, maximum_workspace_bytes, free_before_bytes)?;
        if workspace_size > 0 {
            let scratch = self.allocate_scratch(workspace_size, "Query device workspace")?;
            api.set_workspace(handle, query_workspace, scratch, workspace_bytes)?;
        }
        let values = api.compute_expectation(handle, expectation, query_workspace, stream)?;
        api.synchronize_stream(stream)?;
        Ok(values)
    }

    /// Samples `modes` of the computed state, as ④'s `sample_qubits`.
    fn sample_modes(
        &mut self,
        modes: &[i32],
        request: SamplingRequest,
    ) -> Result<Box<[i64]>, EngineError> {
        let session = Rc::clone(&self.session);
        let (api, handle, stream) = (
            session.resources.api(),
            self.handle(),
            session.resources.stream(),
        );
        let maximum_workspace_bytes = session.policy.maximum_workspace_bytes;
        let started = Instant::now();
        let (free_before_bytes, _) = api.memory_info()?;
        let workspace = self.workspace()?;
        let mark = self.allocations.len();
        let mut sampler = PreparedSampler::new(
            api,
            SamplerContext {
                handle,
                state: self.state()?,
                workspace,
                stream,
                maximum_workspace_bytes,
            },
            modes,
            &request,
        )?;
        let execution = (|| {
            let workspace_bytes =
                api.workspace_size(handle, workspace, WorkspacePreference::Recommended)?;
            if workspace_bytes <= 0 {
                return Err(EngineError::InvalidNativeResult {
                    reason: format!("recommended sampler workspace size is {workspace_bytes}"),
                });
            }
            let workspace_size = usize::try_from(workspace_bytes).map_err(|_| {
                EngineError::ResourceSizeOverflow {
                    resource: "sampler device workspace",
                }
            })?;
            validate_workspace_size(workspace_size, maximum_workspace_bytes, free_before_bytes)?;
            let scratch = self.allocate_scratch(workspace_size, "sampler device workspace")?;
            api.set_workspace(handle, workspace, scratch, workspace_bytes)?;
            sampler.sample(&request, workspace, stream)
        })();
        let execution = combine_execution_and_cleanup(execution, sampler.close());
        // The scratch is still attached to the state's workspace; the next
        // compute or sample attaches its own before using it.
        let result = combine_execution_and_cleanup(execution, self.free_since(mark));
        self.profile.reads += started.elapsed();
        result
    }

    /// Closes the Query lifecycle, then frees the allocations made since
    /// `mark`.
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
        let api = session.resources.api();
        let mut first_error = api.synchronize_stream(session.resources.stream()).err();
        while self.allocations.len() > mark {
            let Some(allocation) = self.allocations.pop() else {
                break;
            };
            if let Err(error) = api.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn close_query_lifecycle(&mut self) -> Result<(), EngineError> {
        let api = self.session.resources.api();
        let mut first_error = None;
        if let Some(workspace) = self.query_workspace.take()
            && let Err(error) = api.destroy_workspace(workspace)
        {
            first_error = Some(error);
        }
        if let Some(expectation) = self.expectation.take()
            && let Err(error) = api.destroy_expectation(expectation)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(operator) = self.network_operator.take()
            && let Err(error) = api.destroy_network_operator(operator)
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
        let bytes = elements
            .checked_mul(size_of::<Complex64Abi>())
            .ok_or(EngineError::ResourceSizeOverflow { resource })?;
        self.allocate_bytes(bytes, resource)
    }

    /// Workspace scratch, which cuTensorNet needs aligned to 256 bytes.
    fn allocate_scratch(
        &mut self,
        bytes: usize,
        resource: &'static str,
    ) -> Result<OpaqueHandle, EngineError> {
        let scratch = self.allocate_bytes(bytes, resource)?;
        if scratch.addr().get().is_multiple_of(256) {
            Ok(scratch)
        } else {
            Err(EngineError::InvalidNativeResult {
                reason: format!("cudaMalloc returned {resource} below 256-byte alignment"),
            })
        }
    }

    fn allocate_bytes(
        &mut self,
        bytes: usize,
        resource: &'static str,
    ) -> Result<OpaqueHandle, EngineError> {
        if bytes == 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("{resource} requires zero bytes"),
            });
        }
        let allocation = self.session.resources.api().allocate(bytes)?;
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

    /// Attempts all releases and returns the first failure; subsequent closes
    /// are no-ops, including after a native destruction failure.
    pub(super) fn close(&mut self) -> Result<(), EngineError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let session = Rc::clone(&self.session);
        let api = session.resources.api();
        // The engine's device, which another engine's call may have changed.
        let mut first_error = session.resources.bind_device().err();
        if let Err(error) = api.synchronize_stream(session.resources.stream())
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Err(error) = self.close_query_lifecycle()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(workspace) = self.workspace.take()
            && let Err(error) = api.destroy_workspace(workspace)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(state) = self.state.take()
            && let Err(error) = api.destroy_state(state)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        self.computed_buffers.clear();
        self.allocations.append(&mut self.operator_tensors);
        while let Some(allocation) = self.allocations.pop() {
            if let Err(error) = api.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Some(handle) = self.own_handle.take()
            && let Err(error) = api.destroy_handle(handle)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for MpsExecution {
    fn drop(&mut self) {
        let _ = self.close();
    }
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

/// The squared norm the library reported, which must be finite and real
/// to rounding. It may be zero: a state whose truncations discarded all of
/// its weight reports that, rather than an error.
fn real_squared_norm(squared_norm: Complex64) -> Result<f64, EngineError> {
    if !(squared_norm.re.is_finite() && squared_norm.im.is_finite()) {
        return Err(EngineError::InvalidNativeResult {
            reason: "Query returned a non-finite squared norm".to_string(),
        });
    }
    if squared_norm.im.abs() > 1.0e-12 * squared_norm.re.abs().max(1.0) {
        return Err(EngineError::InvalidNativeResult {
            reason: "Query squared norm has a material imaginary component".to_string(),
        });
    }
    Ok(squared_norm.re)
}

/// `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`, as ④'s `normalize_expectation`: a truncated MPS is
/// not normalized, so every read divides by the norm the library reports
/// beside it, and rejects a non-finite value or a nonpositive or complex
/// norm.
fn normalize_expectation(
    raw_expectation: Complex64,
    squared_norm: Complex64,
) -> Result<Complex64, EngineError> {
    let finite = |value: Complex64| value.re.is_finite() && value.im.is_finite();
    let invalid = |reason: &str| EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    };
    if !finite(raw_expectation) || !finite(squared_norm) {
        return Err(invalid("Query returned a non-finite value"));
    }
    if squared_norm.re <= 0.0 {
        return Err(invalid("Query returned a nonpositive squared norm"));
    }
    let imaginary_tolerance = 1.0e-12 * squared_norm.re.abs().max(1.0);
    if squared_norm.im.abs() > imaginary_tolerance {
        return Err(invalid(
            "Query squared norm has a material imaginary component",
        ));
    }
    let normalized = raw_expectation / squared_norm;
    if finite(normalized) {
        Ok(normalized)
    } else {
        Err(invalid("normalized Query value is non-finite"))
    }
}

/// Whether the row-major `dimension`×`dimension` `matrix` is unitary, so
/// that cuTensorNet may treat it as one.
fn is_unitary(matrix: &[Complex64], dimension: usize) -> bool {
    (0..dimension).all(|i| {
        (0..dimension).all(|j| {
            let product: Complex64 = (0..dimension)
                .map(|k| matrix[k * dimension + i].conj() * matrix[k * dimension + j])
                .sum();
            let identity = if i == j { 1.0 } else { 0.0 };
            (product - identity).norm() <= UNITARY_TOLERANCE
        })
    })
}

fn validate_workspace_size(
    required: usize,
    policy_maximum: usize,
    free_bytes: usize,
) -> Result<(), EngineError> {
    if required > policy_maximum {
        return Err(EngineError::WorkspaceLimitExceeded {
            required,
            maximum: policy_maximum,
        });
    }
    if required > free_bytes {
        return Err(EngineError::WorkspaceLimitExceeded {
            required,
            maximum: free_bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Complex64, EngineError, real_squared_norm};

    #[test]
    fn a_squared_norm_of_zero_or_rounding_below_it_is_returned_as_reported() {
        for norm in [1.0, 0.25, 0.0, -1.0e-18] {
            assert_eq!(
                real_squared_norm(Complex64::new(norm, 1.0e-14)).ok(),
                Some(norm)
            );
        }
    }

    #[test]
    fn a_non_finite_or_complex_squared_norm_is_an_error() {
        for norm in [
            Complex64::new(f64::NAN, 0.0),
            Complex64::new(f64::INFINITY, 0.0),
            Complex64::new(1.0, f64::NAN),
            Complex64::new(1.0, 1.0e-6),
            Complex64::new(0.0, 1.0e-9),
        ] {
            assert!(
                matches!(
                    real_squared_norm(norm),
                    Err(EngineError::InvalidNativeResult { .. })
                ),
                "{norm}"
            );
        }
    }
}
