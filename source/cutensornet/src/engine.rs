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
//!        └─ memory_workspace.rs, ffi.rs, error.rs, policy.rs
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
//! An engine and its states are `!Send`: the stream, the handle and the
//! states created on them stay on the thread that created them. Each call
//! selects the engine's device first, so engines on different devices may
//! share a thread.

mod error;
mod execution;
mod ffi;
mod memory_workspace;
mod policy;
mod resources;
mod sampler;
mod state;
#[cfg(test)]
mod test_double;
#[cfg(test)]
mod tests;

pub use error::EngineError;

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

use num_complex::Complex64;
use resources::SessionResources;
use state::MpsExecution;
use std::{cell::RefCell, ffi::c_void, ptr::NonNull, rc::Rc, sync::Arc};
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
}

/// A `tensornet::MpsEngine` on one CUDA device, through cuTensorNet.
///
/// Get one from `Availability::mps_engine` (Linux x86-64). Each state's bond
/// dimensions come from the capacity `prepare` is given; the truncation
/// cutoffs, SVD algorithm and gauge are the engine's, ④'s qualified values.
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
            session: Rc::new(Session { resources, policy }),
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

    /// Creates the |0…0⟩ state of `capacity`'s sites, which must be at
    /// least two qubits; nothing is computed until a read.
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
    /// within the truncation error.
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
}
