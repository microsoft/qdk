//! The cuTensorNet `MpsEngine`: one CUDA device's stream and cuTensorNet
//! handle, and the MPS states created on them.
//!
//! [`CuTensorNetEngine`] implements `tensornet::MpsEngine` with cuTensorNet's
//! State/MPS API. It knows nothing of a simulator's protocol: it takes
//! matrices on sites and answers probabilities, samples, expectations and
//! shapes, and `tensornet::conformance` checks it against
//! `tensornet::reference::ExactReference`, the exact engine every other one
//! answers to.
//!
//! ```text
//! CuTensorNetEngine        Session: stream + handle + policy (resources.rs)
//!   └─ CuTensorNetState    MpsExecution: one native state (state.rs)
//!        ├─ execution.rs   MpsExecutionApi, MpsTarget, output metadata
//!        ├─ sampler.rs     SamplerApi, PreparedSampler
//!        └─ memory_workspace.rs, ffi.rs, error.rs, policy.rs, profile.rs
//! library::engine          the four traits on the loaded libraries (Linux x86-64)
//! ```
//!
//! The files here are block ④'s cuTensorNet mechanics (M0,
//! `library/simulation/` at a3628099a), moved below the protocol seam: the
//! circuit, the request, the branch qualification and the consumer stay
//! with ④. They compile on every platform, so their host tests run against
//! a test double of the native API (`test_double.rs`); only
//! `library::engine` calls the loaded libraries.
//!
//! A state starts on the engine's handle, and the engine's policy restarts
//! it from its computed MPS every few computes on a cuTensorNet handle of
//! its own (`state.rs`, `policy.rs`), which bounds the workspace a compute
//! asks for over a long shot. The engine's handle lives as long as the
//! engine; a state's own handle as long as the state, or its next restart.
//!
//! An engine and its states are `!Send`: the stream, the handles and the
//! states created on them stay on the thread that created them. Each call
//! selects the engine's device first, so engines on different devices may
//! share a thread.

mod error;
mod execution;
mod ffi;
mod memory_workspace;
mod policy;
mod profile;
mod resources;
mod sampler;
mod state;
#[cfg(test)]
mod test_double;
#[cfg(test)]
mod tests;

pub use error::EngineError;
#[cfg(feature = "qualification")]
pub use policy::{MpsGauge, SvdAlgorithm};
#[cfg(feature = "qualification")]
pub use profile::{EngineVariant, NativeHandle, StateProfile, WorkspaceSizes};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) use self::{
    execution::{
        MpsExecutionApi, MpsTarget, OutputMetadata, StateF64Attribute, StateU32Configuration,
    },
    ffi::Complex64Abi,
    memory_workspace::{MemorySpace, MemoryWorkspaceApi, WorkspaceKind, WorkspacePreference},
    policy::ExecutionPolicy,
    resources::SessionApi,
    sampler::SamplerApi,
};
// `library::engine` maps them to cuTensorNet's values; the `qualification`
// feature exports them above.
#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    not(feature = "qualification")
))]
pub(crate) use policy::{MpsGauge, SvdAlgorithm};

use num_complex::Complex64;
use resources::SessionResources;
use state::MpsExecution;
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr::NonNull,
    rc::Rc,
    sync::Arc,
};
use tensornet::{MpsEngine, MpsShape, PauliSum, SampleSeed};

pub(crate) type OpaqueHandle = NonNull<c_void>;
pub(crate) type Stream = NonNull<c_void>;

/// Every native operation the engine calls: the injection seam that
/// `library::engine` implements with cuTensorNet and host tests with a test
/// double.
pub(crate) trait NativeApi:
    resources::SessionApi + execution::MpsExecutionApi + sampler::SamplerApi
{
}

impl<Api> NativeApi for Api where
    Api: resources::SessionApi + execution::MpsExecutionApi + sampler::SamplerApi + ?Sized
{
}

/// The stream and handle of one device, and the policy every state of the
/// engine is computed with.
struct Session {
    resources: SessionResources<dyn NativeApi>,
    policy: policy::ExecutionPolicy,
    /// The computes done on the session's handle, by any of its states, for
    /// the profile: the A100 measured the workspace growing with them.
    handle_computes: Cell<usize>,
}

/// A `tensornet::MpsEngine` on one CUDA device, through cuTensorNet.
///
/// Get one from `Availability::mps_engine` (Linux x86-64). Each state's bond
/// dimensions come from the capacity `prepare` is given; the truncation
/// cutoffs, SVD algorithm and gauge are the engine's: ④'s qualified
/// cutoffs, and M0's GESVD and SIMPLE gauge unless the qualification picks
/// others.
/// Each state restarts every 10 computes on a cuTensorNet handle of its own
/// (`policy.rs`'s `RESTART_PERIOD`, from the A100's `history_growth`).
pub struct CuTensorNetEngine {
    session: Rc<Session>,
}

/// One MPS state of a [`CuTensorNetEngine`]: a cuTensorNet state, its
/// workspace and the device buffers it reads.
///
/// Dropping it releases them; [`CuTensorNetState::close`] does the same and
/// reports a failure.
pub struct CuTensorNetState {
    execution: RefCell<MpsExecution>,
}

impl CuTensorNetEngine {
    /// Creates the stream and handle on the policy's device.
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        allow(
            dead_code,
            reason = "only host tests create an engine off Linux x86-64"
        )
    )]
    pub(crate) fn with_api(
        api: Arc<dyn NativeApi>,
        policy: policy::ExecutionPolicy,
    ) -> Result<Self, EngineError> {
        let policy = policy.validate()?;
        let resources = SessionResources::new(api, policy.device_ordinal)?;
        Ok(Self {
            session: Rc::new(Session {
                resources,
                policy,
                handle_computes: Cell::new(0),
            }),
        })
    }

    /// Releases the stream and handle, reporting the first failure.
    ///
    /// # Errors
    ///
    /// Fails if a state of the engine is still open (the engine is then
    /// released when its last state is), or if a native release fails.
    pub fn close(self) -> Result<(), EngineError> {
        let open = Rc::strong_count(&self.session) - 1;
        match Rc::try_unwrap(self.session) {
            Ok(session) => session.resources.close(),
            Err(_) => Err(EngineError::InvalidRequest {
                reason: format!("{open} states of the engine are still open"),
            }),
        }
    }

    /// What `state`'s native calls have cost so far, and the device buffers
    /// it holds, for the A100 qualification's `history_growth`.
    ///
    /// # Errors
    ///
    /// Fails if `state` belongs to another engine or is in use.
    #[cfg(any(test, feature = "qualification"))]
    pub fn profile(&self, state: &CuTensorNetState) -> Result<profile::StateProfile, EngineError> {
        Ok(self.execution(state)?.profile())
    }

    /// The scratch cuTensorNet asks for to compute a new state that starts
    /// at `state`'s last computed MPS and has only `operators` applied,
    /// prepared on `handle`: the engine's, or one created for the probe.
    /// The new state, its operator tensors and a fresh handle are released
    /// before it returns, and `state` is not changed. For the A100
    /// qualification's `history_growth`, which compares it with what
    /// `state` itself asks for, to tell whether the workspace grows with the
    /// state's history, with its handle's, or with neither.
    ///
    /// # Errors
    ///
    /// Fails if `state` belongs to another engine, is in use or has never
    /// been computed, if an operator is not one `apply` would take, or if a
    /// native call fails; every release is attempted.
    #[cfg(any(test, feature = "qualification"))]
    pub fn fresh_state_workspace(
        &self,
        state: &CuTensorNetState,
        handle: profile::NativeHandle,
        operators: &[(&[usize], &[Complex64])],
    ) -> Result<profile::WorkspaceSizes, EngineError> {
        self.execution(state)?
            .fresh_state_workspace(handle, operators)
    }

    /// The state's execution, if `state` was created by this engine, with
    /// the engine's device selected.
    fn execution<'state>(
        &self,
        state: &'state CuTensorNetState,
    ) -> Result<std::cell::RefMut<'state, MpsExecution>, EngineError> {
        let execution =
            state
                .execution
                .try_borrow_mut()
                .map_err(|_| EngineError::InvalidRequest {
                    reason: "the state is already in use".to_string(),
                })?;
        if !Rc::ptr_eq(execution.session(), &self.session) {
            return Err(EngineError::InvalidRequest {
                reason: "the state belongs to another engine".to_string(),
            });
        }
        self.session.resources.bind_device()?;
        Ok(execution)
    }
}

impl CuTensorNetState {
    /// Releases the state's native objects and buffers, reporting the first
    /// failure.
    ///
    /// # Errors
    ///
    /// Fails if a native release fails; every release is attempted.
    pub fn close(self) -> Result<(), EngineError> {
        self.execution.into_inner().close()
    }
}

impl std::fmt::Debug for CuTensorNetEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CuTensorNetEngine")
            .field("device_ordinal", &self.session.policy.device_ordinal)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CuTensorNetState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CuTensorNetState")
            .finish_non_exhaustive()
    }
}

impl MpsEngine for CuTensorNetEngine {
    type State = CuTensorNetState;
    type Error = EngineError;

    /// Creates the |0…0⟩ state of `capacity`'s sites, which must be
    /// qubits; nothing is computed until a read. A lone qubit is padded with
    /// an idle site the library needs and the caller never sees (see
    /// `MpsTarget::from_capacity`).
    fn prepare(&self, capacity: &MpsShape) -> Result<CuTensorNetState, EngineError> {
        self.session.resources.bind_device()?;
        Ok(CuTensorNetState {
            execution: RefCell::new(MpsExecution::new(&self.session, capacity)?),
        })
    }

    /// Applies a one- or two-site operator; cuTensorNet's MPS API takes no
    /// wider one.
    fn apply(
        &self,
        state: &mut CuTensorNetState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        self.execution(state)?.apply(sites, matrix)
    }

    fn probability_of_one(
        &self,
        state: &CuTensorNetState,
        site: usize,
    ) -> Result<f64, EngineError> {
        self.execution(state)?.probability_of_one(site)
    }

    /// Two expectations on the state, no fork: the condition's projector
    /// alone, and with `site`'s One projector.
    fn conditional_probability_of_one(
        &self,
        state: &CuTensorNetState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, EngineError> {
        self.execution(state)?
            .conditional_probability_of_one(site, given)
    }

    fn project(
        &self,
        state: &mut CuTensorNetState,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), EngineError> {
        self.execution(state)?.project(site, one, probability)
    }

    /// A new native state with every operator applied again: cuTensorNet
    /// has no state copy, so a fork costs what building the state did. The
    /// fork is computed once from |0…0⟩, not from the parent's captured
    /// MPS, so under truncation its values may differ from the parent's
    /// within the truncation error: the parent, read along the way, was
    /// computed and truncated in steps, one per read since capture
    /// (DECISIONS G8).
    ///
    /// Where it matters: nothing in M1 forks. A per-shot run projects its
    /// shot's own state in place, and this engine's conditional
    /// probabilities are expectations of projectors on the computed state,
    /// with no copy. A strategy that branches shots from a shared prefix would
    /// fork a state read along the way and see the difference; it is the
    /// case to revisit, with the A100 qualification's `fork_cost` rows
    /// (`stepped_fork_difference`) as the measure of how large it is.
    fn fork(&self, state: &CuTensorNetState) -> Result<CuTensorNetState, EngineError> {
        let copy = self.execution(state)?.fork()?;
        Ok(CuTensorNetState {
            execution: RefCell::new(copy),
        })
    }

    fn sample(
        &self,
        state: &CuTensorNetState,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, EngineError> {
        self.execution(state)?.sample(sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &CuTensorNetState,
        observable: &PauliSum,
    ) -> Result<Complex64, EngineError> {
        self.execution(state)?.expectation(observable)
    }

    fn shape(&self, state: &CuTensorNetState) -> Result<MpsShape, EngineError> {
        self.execution(state)?.shape()
    }

    /// The squared norm cuTensorNet reports for the computed state, one
    /// expectation Query like any read. It is the retained weight because
    /// the engine pins `SvdNormalizationNone`, so the kept singular values
    /// are never rescaled. Computing the state truncates it, so when the
    /// reads come changes the weight (DECISIONS G8). The real part is
    /// returned as the library computed it, zero or a rounding-sized
    /// negative included; a non-finite or materially complex norm is an
    /// error.
    ///
    /// A hypothesis, not yet measured: the SVD's absolute cutoff (1e-10)
    /// applies to this unnormalized state. If the singular values at a cut
    /// scale as `√w` once the weight is `w` (the MPS in canonical form
    /// about that cut), a Schmidt value of the normalized state below
    /// `1e-10/√w` is discarded, so the cutoff cuts deeper as the weight
    /// falls, and drops every value once `w` is near 1e-20. That would
    /// explain the A100's "nonpositive squared norm" at χ = 8; the
    /// qualification's `projection_memory` records this weight to check
    /// it.
    fn retained_weight(&self, state: &CuTensorNetState) -> Result<f64, EngineError> {
        self.execution(state)?.retained_weight()
    }
}
