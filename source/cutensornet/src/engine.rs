// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CuTensorNetEngine`]: the `tensornet::MpsEngine` on cuTensorNet, one
//! CUDA device's engine and the matrix product states (MPS) it holds.
//!
//! A simulation method that runs an MPS needs an engine that holds the
//! chain, applies gates, truncates, and reads probabilities and samples; on
//! a GPU, cuTensorNet does that work. This engine implements the trait with
//! cuTensorNet's State/MPS API, so a method written against the trait runs
//! on the GPU unchanged. It knows nothing of a simulator: it takes matrices
//! on sites and answers probabilities, samples, expectations and shapes, and
//! refuses a call that breaks the contract with `tensornet::checks`, as every
//! engine does.
//!
//! ```text
//! CuTensorNetEngine      DeviceSession: stream + handle (session.rs)
//!   └─ CuTensorNetState  NativeState: one native state (state.rs)
//!        ├─ state_calls.rs StateCalls, NativeCapacity, MpsLayout
//!        └─ memory_calls.rs, ffi.rs, error.rs, policy.rs
//! library::engine        the native calls (Linux x86-64)
//! ```
//!
//! The files here hold the cuTensorNet mechanics: no circuit or shot, only
//! states, operators and reads. Their native calls go through three traits
//! (`SessionCalls`, `MemoryCalls`, `StateCalls`) that
//! `library::engine` implements with the loaded libraries, so the engine's
//! real code also runs in CI, which has no GPU, against a CPU double that
//! enforces cuTensorNet's call order (`double.rs`, compiled for the tests
//! only). The
//! crate's `README.md` ("The MPS engine") shows the order of the native
//! calls.
//!
//! An engine and its states are `!Send`: the stream, the handles and the
//! states created on them stay on the thread that created them. Each call
//! selects the engine's device first.

#[cfg(test)]
pub(crate) mod double;
mod error;
mod ffi;
mod memory_calls;
mod policy;
mod session;
mod state;
mod state_calls;

pub use error::EngineError;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) use self::{
    ffi::Complex64Abi,
    memory_calls::MemoryCalls,
    session::SessionCalls,
    state_calls::{
        MpsLayout, NativeCapacity, StateCalls, StateF64Configuration, StateU32Configuration,
    },
};

use num_complex::Complex64;
use session::DeviceSession;
use state::NativeState;
use std::{cell::RefCell, ffi::c_void, ptr::NonNull, rc::Rc, sync::Arc};
use tensornet::{MpsEngine, MpsShape, PauliSum, checks};

pub(crate) type OpaqueHandle = NonNull<c_void>;
pub(crate) type Stream = NonNull<c_void>;

/// Every native call the engine makes: what `library::engine` implements
/// with the loaded libraries, and the tests' double with a CPU model.
pub(crate) trait NativeCalls: session::SessionCalls + state_calls::StateCalls {}

impl<Calls> NativeCalls for Calls where
    Calls: session::SessionCalls + state_calls::StateCalls + ?Sized
{
}

/// A `tensornet::MpsEngine` on one CUDA device, through cuTensorNet.
///
/// Get one from `Availability::mps_engine` (Linux x86-64). Each state's
/// bonds are the capacity `prepare` is given; how it truncates beyond them
/// and factorizes is fixed (`policy.rs`): a split discards at most 1e-14 of
/// its cut's squared norm, with GESVD and the simple-update gauge. Each
/// state restarts every 10 computes on a cuTensorNet handle of its own,
/// which keeps the scratch a compute asks for flat over a long shot.
pub struct CuTensorNetEngine {
    session: Rc<DeviceSession<dyn NativeCalls>>,
}

/// One MPS state of a [`CuTensorNetEngine`]: a cuTensorNet state, its
/// workspace and the device buffers it reads.
///
/// Dropping it releases them, its device memory and native objects, which
/// is all `tensornet`'s contract asks of a state ("Dropping it releases
/// it"). A failed release is not reported: there is no `close` that returns
/// one, as no case has needed it; debug builds write it to stderr. In
/// practice a failed release means a broken device, so the engine's next
/// call fails too, and its error is reported.
pub struct CuTensorNetState {
    native: RefCell<NativeState>,
}

impl CuTensorNetEngine {
    /// Creates the stream and handle on the engine's device.
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        allow(
            dead_code,
            reason = "only host tests create an engine off Linux x86-64"
        )
    )]
    pub(crate) fn with_calls(calls: Arc<dyn NativeCalls>) -> Result<Self, EngineError> {
        Ok(Self {
            session: Rc::new(DeviceSession::new(calls, policy::DEVICE_ORDINAL)?),
        })
    }

    /// The state's native, if `state` was created by this engine, with
    /// the engine's device selected.
    fn native<'state>(
        &self,
        state: &'state CuTensorNetState,
    ) -> Result<std::cell::RefMut<'state, NativeState>, EngineError> {
        // No call holds the borrow across another call on the same state.
        let native = state.native.borrow_mut();
        if !Rc::ptr_eq(native.session(), &self.session) {
            return Err(EngineError::ForeignState);
        }
        self.session.bind_device()?;
        Ok(native)
    }
}

impl std::fmt::Debug for CuTensorNetEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CuTensorNetEngine")
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

    /// Creates the |0…0⟩ state of `capacity`'s sites; nothing is computed
    /// until a read. A lone qubit is padded with an idle site the library
    /// needs and the caller never sees (see `NativeCapacity::new`).
    fn prepare(&self, capacity: &MpsShape) -> Result<CuTensorNetState, EngineError> {
        checks::prepare(capacity)?;
        self.session.bind_device()?;
        Ok(CuTensorNetState {
            native: RefCell::new(NativeState::new(&self.session, capacity)?),
        })
    }

    fn apply(
        &self,
        state: &mut CuTensorNetState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        self.native(state)?.apply(sites, matrix)
    }

    fn probability_of_one(
        &self,
        state: &CuTensorNetState,
        site: usize,
    ) -> Result<f64, EngineError> {
        self.native(state)?.probability_of_one(site)
    }

    /// Two expectations on the state, no fork: the condition's projector
    /// alone, and with `site`'s One projector.
    fn conditional_probability_of_one(
        &self,
        state: &CuTensorNetState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, EngineError> {
        self.native(state)?
            .conditional_probability_of_one(site, given)
    }

    /// Reuses the P(One) `probability_of_one` just read for `site`, when no
    /// operator was applied since, so a measurement costs one expectation.
    fn project(
        &self,
        state: &mut CuTensorNetState,
        site: usize,
        one: bool,
    ) -> Result<f64, EngineError> {
        self.native(state)?.project(site, one)
    }

    /// Copies the state's computed MPS on the device and starts a new
    /// native state from the copy: cuTensorNet has no state copy, and a
    /// replay of the operators would truncate along another path.
    fn fork(&self, state: &CuTensorNetState) -> Result<CuTensorNetState, EngineError> {
        let copy = self.native(state)?.fork()?;
        Ok(CuTensorNetState {
            native: RefCell::new(copy),
        })
    }

    /// Draws by `tensornet::sampling::sample_by_conditionals`, from this
    /// engine's conditional probabilities: two expectations per distinct
    /// prefix of values drawn.
    fn sample(
        &self,
        state: &CuTensorNetState,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, EngineError> {
        self.native(state)?.sample(sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &CuTensorNetState,
        observable: &PauliSum,
    ) -> Result<Complex64, EngineError> {
        self.native(state)?.expectation(observable)
    }

    fn shape(&self, state: &CuTensorNetState) -> Result<MpsShape, EngineError> {
        self.native(state)?.shape()
    }

    /// The squared norm cuTensorNet reports for the computed state, one
    /// expectation like any read. It is the retained weight because the
    /// engine pins `SvdNormalizationNone`, so the kept singular values are
    /// never rescaled. Computing the state truncates it, so reading the
    /// weight may truncate, as the contract allows.
    fn retained_weight(&self, state: &CuTensorNetState) -> Result<f64, EngineError> {
        self.native(state)?.retained_weight()
    }
}
