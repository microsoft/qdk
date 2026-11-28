// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ReferenceSpy`]: a test spy over the exact reference, which runs every
//! call on [`ExactReference`] and records what the resumer asked of it: it
//! counts the gates it applies, the probabilities of One and the shapes it
//! reads, the states it prepares and forks, and the states alive, now and
//! at most at once; keeps the capacities it is given; and, as a test sets
//! it, reports a retained weight below one, as an engine that truncates
//! would, fails where a device would, or sets a stop flag at a given gate,
//! as a user's interrupt would.
//! A state a call failed on is unspecified, by the `MpsEngine` contract's
//! rule for every method's failure, which a fork's source falls under as
//! well: the spy counts each later call given it, which it runs on the
//! reference's state as it is.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use num_complex::Complex64;
use tensornet::reference::{ExactReference, ReferenceError, ReferenceState};
use tensornet::{MpsEngine, MpsEngineError, MpsShape, PauliSum, Refusal};

/// What weight [`ReferenceSpy`] reports for a state.
#[derive(Clone, Copy)]
pub(crate) enum Weight {
    /// The reference's own.
    Exact,
    /// This one, for every state.
    Fixed(f64),
    /// A weight that differs from state to state, by the basis state the
    /// state reads ([`BASIS_STATE_WEIGHT_STEP`] gives it).
    ByBasisState,
}

/// The weight [`Weight::ByBasisState`] takes off per basis state: basis
/// state `i`, whose bit `k` is the value of site `k`, keeps
/// 1 − this·(1 + i).
pub(crate) const BASIS_STATE_WEIGHT_STEP: f64 = 1e-3;

/// The X gate's matrix, row-major, as the engine receives it: from a
/// program's X gate, or from a reset that drew One.
const X_GATE: [Complex64; 4] = [
    Complex64::new(0.0, 0.0),
    Complex64::new(1.0, 0.0),
    Complex64::new(1.0, 0.0),
    Complex64::new(0.0, 0.0),
];

/// A call of [`ReferenceSpy`] that a test sets to fail once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Call {
    /// `MpsEngine::prepare`.
    Prepare,
    /// `MpsEngine::apply`.
    Apply,
    /// `MpsEngine::probability_of_one`.
    ProbabilityOfOne,
    /// `MpsEngine::project`.
    Project,
    /// `MpsEngine::shape`.
    Shape,
    /// `MpsEngine::retained_weight`.
    RetainedWeight,
}

/// Which calls [`ReferenceSpy`] fails, as a device's error would.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Failing {
    /// No call.
    Nothing,
    /// Every projection on One.
    ProjectionsOnOne,
    /// Every fork.
    Forks,
    /// Every gate whose matrix is [`X_GATE`].
    XGates,
    /// The first call of this kind, and no later one.
    First(Call),
}

/// A test spy over the exact reference (see the module documentation).
pub(crate) struct ReferenceSpy {
    /// The engine every call runs on.
    reference: ExactReference,
    /// The weight it reports.
    weight: Weight,
    /// The calls it fails.
    failing: Failing,
    /// Whether the call a [`Failing::First`] names has failed.
    failed_first: Cell<bool>,
    /// The capacity of each state it prepared, in order.
    prepared: RefCell<Vec<MpsShape>>,
    /// How many gates it applied.
    applies: Cell<usize>,
    /// How many probabilities of One it read.
    probability_reads: Cell<usize>,
    /// How many shapes it read.
    shapes: Cell<usize>,
    /// How many states it forked.
    forks: Cell<usize>,
    /// How many states are alive now, shared with each state, which takes
    /// itself off when dropped.
    live: Rc<Cell<usize>>,
    /// The most states alive at once.
    most_live: Cell<usize>,
    /// How many calls were given a state that a call had failed on.
    uses_after_failure: Cell<usize>,
    /// The flag it sets when it applies its gate of this count (from 1);
    /// `None`: it sets no flag.
    stop_at_apply: Option<(usize, Arc<AtomicBool>)>,
}

/// A state of [`ReferenceSpy`]: the reference's, counted while it lives,
/// and whether a call failed on it.
pub(crate) struct ReferenceSpyState {
    /// The reference's state.
    state: ReferenceState,
    /// The spy's count of states alive, which its drop decrements.
    live: Rc<Cell<usize>>,
    /// Whether a call failed on it, which leaves it unspecified.
    failed: Cell<bool>,
}

impl Drop for ReferenceSpyState {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

impl ReferenceSpy {
    /// The reference, reporting its own retained weight.
    pub(crate) fn new() -> Self {
        Self::with(Weight::Exact, Failing::Nothing)
    }

    /// The reference, reporting `weight`.
    pub(crate) fn retaining(weight: Weight) -> Self {
        Self::with(weight, Failing::Nothing)
    }

    /// The reference, failing every projection on One.
    pub(crate) fn failing_projections_on_one() -> Self {
        Self::with(Weight::Exact, Failing::ProjectionsOnOne)
    }

    /// The reference, failing every fork.
    pub(crate) fn failing_forks() -> Self {
        Self::with(Weight::Exact, Failing::Forks)
    }

    /// The reference, failing every gate that is the X gate's matrix.
    pub(crate) fn failing_x_gates() -> Self {
        Self::with(Weight::Exact, Failing::XGates)
    }

    /// The reference, failing the first `call` it is given, and no later
    /// one.
    pub(crate) fn failing_first(call: Call) -> Self {
        Self::with(Weight::Exact, Failing::First(call))
    }

    /// The reference, setting `stop_flag` when it applies its gate number
    /// `apply` (from 1).
    pub(crate) fn stopping_at_apply(apply: usize, stop_flag: Arc<AtomicBool>) -> Self {
        Self {
            stop_at_apply: Some((apply, stop_flag)),
            ..Self::new()
        }
    }

    /// The reference, reporting `weight` and failing the `failing` calls,
    /// with every count at zero.
    fn with(weight: Weight, failing: Failing) -> Self {
        Self {
            reference: ExactReference::new(),
            weight,
            failing,
            failed_first: Cell::new(false),
            prepared: RefCell::new(Vec::new()),
            applies: Cell::new(0),
            probability_reads: Cell::new(0),
            shapes: Cell::new(0),
            forks: Cell::new(0),
            live: Rc::new(Cell::new(0)),
            most_live: Cell::new(0),
            uses_after_failure: Cell::new(0),
            stop_at_apply: None,
        }
    }

    /// The capacity of each state it prepared, in order.
    pub(crate) fn prepared(&self) -> Vec<MpsShape> {
        self.prepared.borrow().clone()
    }

    /// The gates it applied, one per `apply` call.
    pub(crate) fn applies(&self) -> usize {
        self.applies.get()
    }

    /// The probabilities of One it read, one per `probability_of_one` call.
    pub(crate) fn probability_reads(&self) -> usize {
        self.probability_reads.get()
    }

    /// The shapes it read, one per `shape` call.
    pub(crate) fn shapes(&self) -> usize {
        self.shapes.get()
    }

    /// The states it forked.
    pub(crate) fn forks(&self) -> usize {
        self.forks.get()
    }

    /// The most states alive at once.
    pub(crate) fn most_live(&self) -> usize {
        self.most_live.get()
    }

    /// The states alive now.
    pub(crate) fn live(&self) -> usize {
        self.live.get()
    }

    /// The calls given a state that a call had failed on.
    pub(crate) fn uses_after_failure(&self) -> usize {
        self.uses_after_failure.get()
    }

    /// Wraps a state the reference made, and counts it alive.
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

    /// The weight [`Weight::ByBasisState`] gives `state`, a basis state, as
    /// [`BASIS_STATE_WEIGHT_STEP`] states it.
    fn weight_by_basis_state(&self, state: &ReferenceSpyState) -> Result<f64, ReferenceSpyError> {
        let mut index = 0;
        for site in 0..state.state.site_count() {
            let one = self.reference.probability_of_one(&state.state, site)? > 0.5;
            index |= usize::from(one) << site;
        }
        #[expect(clippy::cast_precision_loss, reason = "a few sites")]
        let index = index as f64;
        Ok(1.0 - BASIS_STATE_WEIGHT_STEP * (1.0 + index))
    }
}

/// Why [`ReferenceSpy`] failed: a refusal or failure of the reference, or the
/// failure a test set.
#[derive(Debug)]
pub(crate) enum ReferenceSpyError {
    /// The reference refused or failed the call.
    Reference(ReferenceError),
    /// The failure a test set, as a device's.
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
        if (self.failing == Failing::XGates && matrix == X_GATE) || self.fails_now(Call::Apply) {
            return Err(Self::fail_on(state));
        }
        self.applies.set(self.applies.get() + 1);
        if let Some((apply, stop_flag)) = &self.stop_at_apply
            && *apply == self.applies.get()
        {
            stop_flag.store(true, Ordering::Relaxed);
        }
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
        self.probability_reads.set(self.probability_reads.get() + 1);
        Ok(self.reference.probability_of_one(&state.state, site)?)
    }

    fn project(
        &self,
        state: &mut ReferenceSpyState,
        site: usize,
        one: bool,
    ) -> Result<f64, ReferenceSpyError> {
        self.note_use(state);
        if (one && self.failing == Failing::ProjectionsOnOne) || self.fails_now(Call::Project) {
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
        self.shapes.set(self.shapes.get() + 1);
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
            Weight::ByBasisState => self.weight_by_basis_state(state)?,
        })
    }
}
