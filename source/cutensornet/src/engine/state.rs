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
//! - Operator tensors stay allocated until the state closes, as ④'s did:
//!   without capture every compute reads them again; with it the library
//!   no longer reads them after the next capture, but whether it still
//!   needs them is not documented, and nothing has measured what they cost.
//!   At about 49,400 small tensors per H₂ shot (48,000 gates and 1,404
//!   projections), freeing them after the capture is a follow-up once the
//!   qualification's `device_mib` rows show it pays. A compute's output buffers and
//!   scratch are freed when the next compute has succeeded and been
//!   captured, since the captured MPS may be read from them until then, and
//!   a read's scratch when the read returns, so a long-lived state does not
//!   grow with its reads (④ computed and sampled a bounded number of times,
//!   and kept them all).
//! - A native failure while the state is changed or computed leaves it
//!   unusable: every later call reports [`EngineError::Poisoned`].

use super::{
    EngineError, OpaqueHandle, Session,
    error::combine_execution_and_cleanup,
    execution::{
        MpsTarget, OutputMetadata, StateF64Attribute, StateU32Configuration, convert_layout,
        validate_realized_chain,
    },
    ffi::Complex64Abi,
    sampler::{PreparedSampler, SamplerContext, SamplingRequest},
};
use num_complex::Complex64;
use std::{collections::BTreeMap, mem::size_of, rc::Rc};
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
    allocations: Vec<OpaqueHandle>,
    /// The outputs and scratch of the last compute, also in `allocations`.
    computed_buffers: Vec<OpaqueHandle>,
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
            computed_buffers: Vec::new(),
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

    pub(super) fn apply(
        &mut self,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        self.check_live()?;
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
        let operator = Operator {
            modes: modes.into_boxed_slice(),
            matrix: matrix.iter().copied().map(Complex64Abi::from).collect(),
            unitary: is_unitary(matrix, 1 << sites.len()),
        };
        self.poison_on_error(|execution| execution.register(operator))
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
        let mut copy = Self::new(&self.session, self.target.shape())?;
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
        self.ensure_computed()
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

    fn mode(&self, site: usize) -> Result<i32, EngineError> {
        if site >= self.state_extents.len() {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "site {site} is outside the {}-site state",
                    self.state_extents.len()
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
    fn register(&mut self, operator: Operator) -> Result<(), EngineError> {
        let session = Rc::clone(&self.session);
        let api = session.resources.api();
        let tensor = self.allocate_complex(operator.matrix.len(), "operator tensor")?;
        api.copy_to_device(tensor, &operator.matrix)?;
        self.realized = None;
        api.apply_tensor_operator(
            session.resources.handle(),
            self.state()?,
            &operator.modes,
            tensor,
            operator.unitary,
        )?;
        self.operators.push(operator);
        Ok(())
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
        let resources = &self.session.resources;
        resources
            .api()
            .finalize_mps(resources.handle(), state, &self.target)?;
        self.configure(state)
    }

    fn configure(&self, state: OpaqueHandle) -> Result<(), EngineError> {
        let resources = &self.session.resources;
        let (api, handle) = (resources.api(), resources.handle());
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
            session.resources.handle(),
            session.resources.stream(),
        );
        let maximum_workspace_bytes = session.policy.maximum_workspace_bytes;
        let state = self.state()?;
        let mark = self.allocations.len();
        let output_elements = self.target.shape().element_counts();
        let mut outputs = Vec::with_capacity(output_elements.len());
        for elements in &output_elements {
            outputs.push(self.allocate_complex(*elements, "MPS output")?);
        }
        let (free_before_bytes, _) = api.memory_info()?;
        let workspace = self.workspace()?;
        api.prepare_state(handle, state, maximum_workspace_bytes, workspace, stream)?;
        let workspace_bytes = api.workspace_size(handle, workspace)?;
        if workspace_bytes <= 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("recommended workspace size is {workspace_bytes}"),
            });
        }
        let workspace_size =
            usize::try_from(workspace_bytes).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "device workspace",
            })?;
        validate_workspace_size(workspace_size, maximum_workspace_bytes, free_before_bytes)?;
        let scratch = self.allocate_scratch(workspace_size, "device workspace")?;
        api.set_workspace(handle, workspace, scratch, workspace_bytes)?;
        let mut metadata = OutputMetadata::new(&self.target);
        api.compute_state(
            handle,
            state,
            workspace,
            &mut metadata,
            &mut outputs,
            stream,
        )?;
        api.synchronize_stream(stream)?;
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
            api.capture_mps(handle, state)?;
        }
        // The state now refers to the new buffers, as its computed MPS and,
        // once captured, as its start; the stream has finished with the old
        // ones.
        let fresh = self.allocations[mark..].to_vec();
        let previous = std::mem::replace(&mut self.computed_buffers, fresh);
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
        first_error.map_or(Ok(realized), Err)
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
        let mark = self.allocations.len();
        let query = self.open_query(products);
        let release = self.release_query(mark);
        combine_execution_and_cleanup(query, release)
    }

    fn open_query(&mut self, products: &[Product]) -> Result<(Complex64, Complex64), EngineError> {
        let session = Rc::clone(&self.session);
        let (api, handle, stream) = (
            session.resources.api(),
            session.resources.handle(),
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
        let workspace_bytes = api.workspace_size(handle, query_workspace)?;
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
            session.resources.handle(),
            session.resources.stream(),
        );
        let maximum_workspace_bytes = session.policy.maximum_workspace_bytes;
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
            let workspace_bytes = api.workspace_size(handle, workspace)?;
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
        combine_execution_and_cleanup(execution, self.free_since(mark))
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
        while let Some(allocation) = self.allocations.pop() {
            if let Err(error) = api.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
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
