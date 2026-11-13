// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CuTensorNetEngine`]: the `tensornet::MpsEngine` on cuTensorNet, one
//! CUDA device's engine and the matrix product states (MPS) it holds.
//!
//! An MPS simulation method (the algorithm that runs a program on an MPS)
//! needs an engine that holds the chain, applies gates, truncates, and reads
//! probabilities; on a GPU, cuTensorNet does that work, and samples are
//! drawn from its probabilities. This engine implements the trait's methods
//! with cuTensorNet's State/MPS API, so an MPS simulation method written
//! against the trait runs on the GPU unchanged. It knows nothing of a
//! program: it takes matrices on sites and answers probabilities, samples,
//! expectations and shapes, and refuses a call that breaks the contract
//! with `tensornet::checks`, as every engine does.
//!
//! ```text
//! CuTensorNetEngine ── holds ──► DeviceSession   device, stream, handle
//!                                                (device_session.rs)
//! CuTensorNetState  ── holds ──► NativeState     one native state
//!                                                (native_state.rs)
//! DeviceSession and NativeState
//!   └─ every native call ──────► NativeCalls     CUDA and cuTensorNet
//!                                                (native_calls.rs)
//! engine-wide                    error.rs, policy.rs
//! ```
//!
//! The files here hold the cuTensorNet mechanics: no circuit or shot, only
//! states, operators and reads. The session and the states make their
//! native calls through `NativeCalls`, so the engine's real code also runs
//! in CI, which has no GPU. The crate's `docs/engine.md`
//! ("Native calls of each phase") shows the native calls in order.
//!
//! An engine and its states are `!Send`: the stream, the handles and the
//! states created on them stay on the thread that created them. Each call
//! selects the engine's device first. The compiler enforces it; each block
//! below is a test that must fail to compile, one for the engine and one
//! for its states.
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::CuTensorNetEngine>();
//! ```
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::CuTensorNetState>();
//! ```

mod dev_tuning;
mod device_session;
mod error;
pub(crate) mod native_calls;
mod native_state;
pub(crate) mod policy;

#[doc(hidden)]
pub use dev_tuning::{DevTimes, DevTuning, PhaseClock, SvdAlgorithm};
pub use error::EngineError;

// What the tests under `src/tests` name of the engine's private modules:
// the session, and the settings they check it keeps.
#[cfg(test)]
pub(crate) use self::{
    device_session::DeviceSession,
    policy::{DISCARDED_WEIGHT_CUTOFF, EXPECTATION_HYPER_SAMPLES, RESTART_PERIOD},
};

use native_calls::NativeCalls;
use native_state::NativeState;
use num_complex::Complex64;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tensornet::{MpsEngine, MpsShape, PauliSum, Refusal, checks};

/// A `tensornet::MpsEngine` on one CUDA device, through cuTensorNet.
///
/// Open one with `new`, on a discovery's [`Availability`](crate::Availability)
/// (Linux x86-64). Each state's bonds are at most the capacity `prepare` is
/// given; a truncation may also drop singular values of negligible weight.
/// How the engine truncates, factorizes and manages its scratch is fixed,
/// not tunable (`policy.rs`).
pub struct CuTensorNetEngine {
    device_session: Rc<device_session::DeviceSession<dyn NativeCalls>>,
    /// Development only: `with_dev_wide_operators`.
    dev_wide_operators: bool,
}

/// One MPS state of a [`CuTensorNetEngine`]: a cuTensorNet state, its
/// workspace and the device buffers it reads.
///
/// The trait reads a state through a shared reference, but a read may
/// compute it, so its native state is in a `RefCell`, borrowed for one call
/// of the engine.
///
/// Dropping it releases them, its device memory and native objects, which
/// is all `tensornet`'s contract asks of a state ("Dropping it releases
/// it"). A failed release is not returned: `Drop` cannot return an error,
/// and a caller could not act on one, as the state is gone either way. What
/// a caller needs is not to trust later results: a failure that means a
/// broken device stays in the CUDA context, so the engine's next call fails
/// and reports it; any other failure is a fault in the engine's call order,
/// which the tests check. Debug builds also
/// write it to stderr.
pub struct CuTensorNetState {
    native_state: RefCell<NativeState>,
}

impl CuTensorNetEngine {
    /// An engine on the first CUDA device the process sees (device 0), on
    /// the libraries `availability` loaded and checked. On a host with
    /// several GPUs, `CUDA_VISIBLE_DEVICES` sets which one that is, so one
    /// process per GPU, each given its own, uses them all. Taking an
    /// `Availability` means no engine runs on libraries discovery has not
    /// checked. The engine keeps the libraries loaded for as long as it
    /// lives, so the `Availability` may be dropped first.
    ///
    /// # Errors
    ///
    /// Fails if no CUDA device is available, or if creating the device's
    /// stream or cuTensorNet handle fails.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn new(availability: &crate::Availability) -> Result<Self, EngineError> {
        Self::with_native_calls(availability.libraries().clone())
    }

    /// An engine on device 0 that makes its native calls through
    /// `native_calls`. It opens the engine's device session
    /// (`device_session.rs`).
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        allow(
            dead_code,
            reason = "only host tests create an engine off Linux x86-64"
        )
    )]
    pub(crate) fn with_native_calls(
        native_calls: Arc<dyn NativeCalls>,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            device_session: Rc::new(device_session::DeviceSession::new(
                native_calls,
                policy::DEVICE_ORDINAL,
            )?),
            dev_wide_operators: false,
        })
    }

    /// EXPERIMENTAL, development only (not for review): the same engine,
    /// whose `apply` also takes an operator on three distinct sites, in any
    /// order and any distance apart, and passes it to cuTensorNet as one
    /// tensor operator on their three modes, instead of refusing it; when
    /// `on`. NVIDIA documents operators on non-adjacent modes in MPS mode
    /// (the 2.x release notes: improved accuracy for tensor operators,
    /// controlled operators and MPOs on non-adjacent qudits); the gate's
    /// unitarity is not checked.
    ///
    /// # Panics
    ///
    /// In `apply`, when the three sites are not distinct or the matrix is
    /// not 8×8.
    #[doc(hidden)]
    #[must_use]
    pub fn with_dev_wide_operators(self, on: bool) -> Self {
        Self {
            dev_wide_operators: on,
            ..self
        }
    }

    /// Development only (the tuning bench): the engine with `tuning`'s
    /// knobs instead of its fixed choices (`dev_tuning.rs`).
    #[doc(hidden)]
    #[must_use]
    pub fn with_dev_tuning(self, tuning: DevTuning) -> Self {
        self.device_session.set_dev_tuning(tuning);
        self
    }

    /// Development only: the time spent in each phase since the engine was
    /// made or `dev_reset_times` was called.
    #[doc(hidden)]
    #[must_use]
    pub fn dev_times(&self) -> DevTimes {
        self.device_session.dev_times()
    }

    /// Development only: restarts the clocks of `dev_times`.
    #[doc(hidden)]
    pub fn dev_reset_times(&self) {
        self.device_session.reset_dev_times();
    }

    /// Development only (the tuning bench): asks cuTensorNet to make the
    /// next compute of `state` canonical around `site`, so that
    /// `DevTuning::canonical_centre_reads` can read P(One) there from the
    /// site's tensor. It keeps the centre across restarts.
    ///
    /// # Errors
    ///
    /// Fails if the site is out of range, or if cuTensorNet refuses the
    /// configuration (the state is then unusable).
    #[doc(hidden)]
    pub fn dev_set_canonical_centre(
        &self,
        state: &CuTensorNetState,
        site: usize,
    ) -> Result<(), EngineError> {
        self.native_state(state, |site_count| {
            checks::probability_of_one(site_count, site)
        })?
        .set_canonical_centre(site)
    }

    /// The native state of `state`, if this engine created it and `check`,
    /// the contract's check of the call (`tensornet::checks`), accepts the
    /// call for the state's site count, with the engine's device selected.
    /// A refused call makes no native call.
    fn native_state<'state>(
        &self,
        state: &'state CuTensorNetState,
        check: impl FnOnce(usize) -> Result<(), Refusal>,
    ) -> Result<std::cell::RefMut<'state, NativeState>, EngineError> {
        // No call holds the borrow across another call on the same state.
        let native_state = state.native_state.borrow_mut();
        if !Rc::ptr_eq(native_state.device_session(), &self.device_session) {
            return Err(EngineError::ForeignState);
        }
        check(native_state.site_count())?;
        self.device_session.bind_device()?;
        Ok(native_state)
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
        self.device_session.bind_device()?;
        Ok(CuTensorNetState {
            native_state: RefCell::new(NativeState::new(&self.device_session, capacity)?),
        })
    }

    fn apply(
        &self,
        state: &mut CuTensorNetState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), EngineError> {
        if self.dev_wide_operators && sites.len() == 3 {
            assert!(
                sites[0] != sites[1] && sites[0] != sites[2] && sites[1] != sites[2],
                "development only: a wide operator on distinct sites"
            );
            assert_eq!(matrix.len(), 64, "development only: an 8×8 operator");
            let in_range = |site_count| {
                sites
                    .iter()
                    .try_for_each(|&site| checks::probability_of_one(site_count, site))
            };
            return self.native_state(state, in_range)?.apply(sites, matrix);
        }
        self.native_state(state, |site_count| checks::apply(site_count, sites, matrix))?
            .apply(sites, matrix)
    }

    fn probability_of_one(
        &self,
        state: &CuTensorNetState,
        site: usize,
    ) -> Result<f64, EngineError> {
        self.native_state(state, |site_count| {
            checks::probability_of_one(site_count, site)
        })?
        .probability_of_one(site)
    }

    /// Two expectations on the state, no fork: the condition's projector
    /// alone, and with `site`'s One projector.
    fn conditional_probability_of_one(
        &self,
        state: &CuTensorNetState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, EngineError> {
        self.native_state(state, |site_count| {
            checks::conditional_probability_of_one(site_count, site, given)
        })?
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
        self.native_state(state, |site_count| checks::project(site_count, site))?
            .project(site, one)
    }

    /// Copies the state's computed MPS on the device and starts a new
    /// native state from the copy: cuTensorNet has no state copy, and a
    /// replay of the operators would truncate along another path.
    fn fork(&self, state: &CuTensorNetState) -> Result<CuTensorNetState, EngineError> {
        let copy = self.native_state(state, refuses_nothing)?.fork()?;
        Ok(CuTensorNetState {
            native_state: RefCell::new(copy),
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
        self.native_state(state, |site_count| checks::sample(site_count, sites))?
            .sample(sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &CuTensorNetState,
        observable: &PauliSum,
    ) -> Result<Complex64, EngineError> {
        self.native_state(state, |site_count| {
            checks::expectation(site_count, observable)
        })?
        .expectation(observable)
    }

    fn shape(&self, state: &CuTensorNetState) -> Result<MpsShape, EngineError> {
        self.native_state(state, refuses_nothing)?.shape()
    }

    /// The squared norm cuTensorNet reports for the computed state, one
    /// expectation like any read. It is the retained weight because the
    /// engine pins `SvdNormalizationNone`, so the kept singular values are
    /// never rescaled. Computing the state truncates it, so reading the
    /// weight may truncate, as the contract allows.
    fn retained_weight(&self, state: &CuTensorNetState) -> Result<f64, EngineError> {
        self.native_state(state, refuses_nothing)?.retained_weight()
    }
}

/// The contract's check of a method that refuses nothing (`fork`, `shape`,
/// `retained_weight`): its only argument is the state.
fn refuses_nothing(_site_count: usize) -> Result<(), Refusal> {
    Ok(())
}
