// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ReferenceSpy`]: a test spy over the exact reference, which runs every
//! call on [`ExactReference`] and records what the resumer asked of it: it
//! counts the operators it applies, the states it prepares and forks, and
//! the states alive at once; keeps the capacities it is given; and, as a
//! test sets it, reports a retained weight below one, as an engine that
//! truncates would, or fails where a device would. A state a call failed
//! on is unspecified, as the `MpsEngine` contract says (the source, for a
//! fork): the spy counts each later call given it, which it runs on the
//! reference's state as it is.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use num_complex::Complex64;
use tensornet::reference::{ExactReference, ReferenceError, ReferenceState};
use tensornet::{MpsEngine, MpsEngineError, MpsShape, PauliSum, Refusal};

/// What weight [`ReferenceSpy`] reports for a state.
#[derive(Clone, Copy)]
pub(super) enum Weight {
    /// The reference's own.
    Exact,
    /// This one, for every state.
    Fixed(f64),
    /// 1 − 10⁻³·(1 + i), for the basis state `i` the state reads (bit `k`
    /// the value of site `k`): a weight that differs from state to state.
    ByBasisState,
}

/// A call of [`ReferenceSpy`] a test may fail once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Call {
    Prepare,
    Apply,
    ProbabilityOfOne,
    Shape,
    RetainedWeight,
}

/// Which calls [`ReferenceSpy`] fails, as a device's error would.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Failing {
    Nothing,
    ProjectionsOnOne,
    Forks,
    XGates,
    /// The first call of this kind, and no later one.
    First(Call),
}

/// A test spy over the exact reference (see the module documentation).
pub(super) struct ReferenceSpy {
    reference: ExactReference,
    weight: Weight,
    failing: Failing,
    /// Whether the call a [`Failing::First`] names has failed.
    failed_first: Cell<bool>,
    prepared: RefCell<Vec<MpsShape>>,
    applies: Cell<usize>,
    forks: Cell<usize>,
    live: Rc<Cell<usize>>,
    most_live: Cell<usize>,
    uses_after_failure: Cell<usize>,
}

/// A state of [`ReferenceSpy`]: the reference's, counted while it lives,
/// and whether a call failed on it.
pub(super) struct ReferenceSpyState {
    state: ReferenceState,
    live: Rc<Cell<usize>>,
    failed: Cell<bool>,
}

impl Drop for ReferenceSpyState {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

impl ReferenceSpy {
    /// The reference, reporting its own retained weight.
    pub(super) fn new() -> Self {
        Self::with(Weight::Exact, Failing::Nothing)
    }

    /// The reference, reporting `weight`.
    pub(super) fn retaining(weight: Weight) -> Self {
        Self::with(weight, Failing::Nothing)
    }

    /// The reference, failing every projection on One.
    pub(super) fn failing_projections_on_one() -> Self {
        Self::with(Weight::Exact, Failing::ProjectionsOnOne)
    }

    /// The reference, failing every fork.
    pub(super) fn failing_forks() -> Self {
        Self::with(Weight::Exact, Failing::Forks)
    }

    /// The reference, failing every operator that is the X gate's matrix.
    pub(super) fn failing_x_gates() -> Self {
        Self::with(Weight::Exact, Failing::XGates)
    }

    /// The reference, failing the first `call` it is given, and no later
    /// one.
    pub(super) fn failing_first(call: Call) -> Self {
        Self::with(Weight::Exact, Failing::First(call))
    }

    fn with(weight: Weight, failing: Failing) -> Self {
        Self {
            reference: ExactReference::new(),
            weight,
            failing,
            failed_first: Cell::new(false),
            prepared: RefCell::new(Vec::new()),
            applies: Cell::new(0),
            forks: Cell::new(0),
            live: Rc::new(Cell::new(0)),
            most_live: Cell::new(0),
            uses_after_failure: Cell::new(0),
        }
    }

    /// The capacity of each state it prepared, in order.
    pub(super) fn prepared(&self) -> Vec<MpsShape> {
        self.prepared.borrow().clone()
    }

    /// The operators it applied, one per `apply` call.
    pub(super) fn applies(&self) -> usize {
        self.applies.get()
    }

    /// The states it forked.
    pub(super) fn forks(&self) -> usize {
        self.forks.get()
    }

    /// The most states alive at once.
    pub(super) fn most_live(&self) -> usize {
        self.most_live.get()
    }

    /// The calls given a state that a call had failed on.
    pub(super) fn uses_after_failure(&self) -> usize {
        self.uses_after_failure.get()
    }

    fn track(&self, state: ReferenceState) -> ReferenceSpyState {
        self.live.set(self.live.get() + 1);
        self.most_live
            .set(self.most_live.get().max(self.live.get()));
        ReferenceSpyState {
            state,
            live: Rc::clone(&self.live),
            failed: Cell::new(false),
        }
    }

    /// Whether this call of `call` is the one a test set to fail.
    fn fails_now(&self, call: Call) -> bool {
        self.failing == Failing::First(call) && !self.failed_first.replace(true)
    }

    /// A device's failure on `state`, which leaves it unspecified.
    fn fail_on(state: &ReferenceSpyState) -> ReferenceSpyError {
        state.failed.set(true);
        ReferenceSpyError::Device
    }

    /// Counts a call given `state`, when a call had failed on it.
    fn note_use(&self, state: &ReferenceSpyState) {
        if state.failed.get() {
            self.uses_after_failure
                .set(self.uses_after_failure.get() + 1);
        }
    }
}

/// Why [`ReferenceSpy`] failed: a refusal or failure of the reference, or the
/// failure a test set.
#[derive(Debug)]
pub(super) enum ReferenceSpyError {
    Reference(ReferenceError),
    Device,
}

impl std::fmt::Display for ReferenceSpyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reference(error) => error.fmt(f),
            Self::Device => f.write_str("the device failed"),
        }
    }
}

impl std::error::Error for ReferenceSpyError {}

impl From<ReferenceError> for ReferenceSpyError {
    fn from(error: ReferenceError) -> Self {
        Self::Reference(error)
    }
}

impl From<Refusal> for ReferenceSpyError {
    fn from(refusal: Refusal) -> Self {
        Self::Reference(refusal.into())
    }
}

impl MpsEngineError for ReferenceSpyError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Reference(error) => error.refusal(),
            Self::Device => None,
        }
    }
}

impl MpsEngine for ReferenceSpy {
    type State = ReferenceSpyState;
    type Error = ReferenceSpyError;

    fn prepare(&self, capacity: &MpsShape) -> Result<ReferenceSpyState, ReferenceSpyError> {
        self.prepared.borrow_mut().push(capacity.clone());
        if self.fails_now(Call::Prepare) {
            return Err(ReferenceSpyError::Device);
        }
        Ok(self.track(self.reference.prepare(capacity)?))
    }

    fn apply(
        &self,
        state: &mut ReferenceSpyState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ReferenceSpyError> {
        self.note_use(state);
        let (zero, one) = (Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0));
        let x_gate = matrix == [zero, one, one, zero];
        if (self.failing == Failing::XGates && x_gate) || self.fails_now(Call::Apply) {
            return Err(Self::fail_on(state));
        }
        self.applies.set(self.applies.get() + 1);
        Ok(self.reference.apply(&mut state.state, sites, matrix)?)
    }

    fn probability_of_one(
        &self,
        state: &ReferenceSpyState,
        site: usize,
    ) -> Result<f64, ReferenceSpyError> {
        self.note_use(state);
        if self.fails_now(Call::ProbabilityOfOne) {
            return Err(Self::fail_on(state));
        }
        Ok(self.reference.probability_of_one(&state.state, site)?)
    }

    fn project(
        &self,
        state: &mut ReferenceSpyState,
        site: usize,
        one: bool,
    ) -> Result<f64, ReferenceSpyError> {
        self.note_use(state);
        if one && self.failing == Failing::ProjectionsOnOne {
            return Err(Self::fail_on(state));
        }
        Ok(self.reference.project(&mut state.state, site, one)?)
    }

    fn fork(&self, state: &ReferenceSpyState) -> Result<ReferenceSpyState, ReferenceSpyError> {
        self.note_use(state);
        if self.failing == Failing::Forks {
            return Err(Self::fail_on(state));
        }
        self.forks.set(self.forks.get() + 1);
        Ok(self.track(self.reference.fork(&state.state)?))
    }

    fn sample(
        &self,
        state: &ReferenceSpyState,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, ReferenceSpyError> {
        self.note_use(state);
        Ok(self.reference.sample(&state.state, sites, shots, seed)?)
    }

    fn expectation(
        &self,
        state: &ReferenceSpyState,
        observable: &PauliSum,
    ) -> Result<Complex64, ReferenceSpyError> {
        self.note_use(state);
        Ok(self.reference.expectation(&state.state, observable)?)
    }

    fn shape(&self, state: &ReferenceSpyState) -> Result<MpsShape, ReferenceSpyError> {
        self.note_use(state);
        if self.fails_now(Call::Shape) {
            return Err(Self::fail_on(state));
        }
        Ok(self.reference.shape(&state.state)?)
    }

    fn retained_weight(&self, state: &ReferenceSpyState) -> Result<f64, ReferenceSpyError> {
        self.note_use(state);
        if self.fails_now(Call::RetainedWeight) {
            return Err(Self::fail_on(state));
        }
        Ok(match self.weight {
            Weight::Exact => self.reference.retained_weight(&state.state)?,
            Weight::Fixed(weight) => weight,
            Weight::ByBasisState => {
                let mut index = 0;
                for site in 0..state.state.site_count() {
                    let one = self.reference.probability_of_one(&state.state, site)? > 0.5;
                    index |= usize::from(one) << site;
                }
                #[allow(clippy::cast_precision_loss)] // A few sites.
                let index = index as f64;
                1.0 - 1e-3 * (1.0 + index)
            }
        })
    }
}
