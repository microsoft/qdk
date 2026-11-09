// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Observed`]: the exact reference, observed: it counts the gates it
//! applies, the states it prepares and forks, and the states alive at once;
//! keeps the capacities it is given; and, as a test sets it, reports a
//! retained weight below one, as an engine that truncates would, or fails
//! where a device would.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use num_complex::Complex64;
use tensornet::reference::{ExactReference, ReferenceError, ReferenceState};
use tensornet::{MpsEngine, MpsEngineError, MpsShape, PauliSum, Refusal};

/// What weight [`Observed`] reports for a state.
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

/// The exact reference, observed (see the module documentation).
pub(super) struct Observed {
    reference: ExactReference,
    weight: Weight,
    /// Projections on One fail, as a device's error would.
    fail_projections_on_one: bool,
    prepared: RefCell<Vec<MpsShape>>,
    applies: Cell<usize>,
    forks: Cell<usize>,
    live: Rc<Cell<usize>>,
    most_live: Cell<usize>,
}

/// A state of [`Observed`]: the reference's, counted while it lives.
pub(super) struct Tracked {
    state: ReferenceState,
    live: Rc<Cell<usize>>,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

impl Observed {
    /// The reference, reporting its own retained weight.
    pub(super) fn new() -> Self {
        Self::with(Weight::Exact, false)
    }

    /// The reference, reporting `weight`.
    pub(super) fn retaining(weight: Weight) -> Self {
        Self::with(weight, false)
    }

    /// The reference, failing every projection on One.
    pub(super) fn failing_projections_on_one() -> Self {
        Self::with(Weight::Exact, true)
    }

    fn with(weight: Weight, fail_projections_on_one: bool) -> Self {
        Self {
            reference: ExactReference::new(),
            weight,
            fail_projections_on_one,
            prepared: RefCell::new(Vec::new()),
            applies: Cell::new(0),
            forks: Cell::new(0),
            live: Rc::new(Cell::new(0)),
            most_live: Cell::new(0),
        }
    }

    /// The capacity of each state it prepared, in order.
    pub(super) fn prepared(&self) -> Vec<MpsShape> {
        self.prepared.borrow().clone()
    }

    /// The gates it applied.
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

    fn track(&self, state: ReferenceState) -> Tracked {
        self.live.set(self.live.get() + 1);
        self.most_live
            .set(self.most_live.get().max(self.live.get()));
        Tracked {
            state,
            live: Rc::clone(&self.live),
        }
    }
}

/// Why [`Observed`] failed: a refusal or failure of the reference, or the
/// failure a test set.
#[derive(Debug)]
pub(super) enum ObservedError {
    Reference(ReferenceError),
    Device,
}

impl std::fmt::Display for ObservedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reference(error) => error.fmt(f),
            Self::Device => f.write_str("the device failed"),
        }
    }
}

impl std::error::Error for ObservedError {}

impl From<ReferenceError> for ObservedError {
    fn from(error: ReferenceError) -> Self {
        Self::Reference(error)
    }
}

impl From<Refusal> for ObservedError {
    fn from(refusal: Refusal) -> Self {
        Self::Reference(refusal.into())
    }
}

impl MpsEngineError for ObservedError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Reference(error) => error.refusal(),
            Self::Device => None,
        }
    }
}

impl MpsEngine for Observed {
    type State = Tracked;
    type Error = ObservedError;

    fn prepare(&self, capacity: &MpsShape) -> Result<Tracked, ObservedError> {
        self.prepared.borrow_mut().push(capacity.clone());
        Ok(self.track(self.reference.prepare(capacity)?))
    }

    fn apply(
        &self,
        state: &mut Tracked,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ObservedError> {
        self.applies.set(self.applies.get() + 1);
        Ok(self.reference.apply(&mut state.state, sites, matrix)?)
    }

    fn probability_of_one(&self, state: &Tracked, site: usize) -> Result<f64, ObservedError> {
        Ok(self.reference.probability_of_one(&state.state, site)?)
    }

    fn project(&self, state: &mut Tracked, site: usize, one: bool) -> Result<f64, ObservedError> {
        if one && self.fail_projections_on_one {
            return Err(ObservedError::Device);
        }
        Ok(self.reference.project(&mut state.state, site, one)?)
    }

    fn fork(&self, state: &Tracked) -> Result<Tracked, ObservedError> {
        self.forks.set(self.forks.get() + 1);
        Ok(self.track(self.reference.fork(&state.state)?))
    }

    fn sample(
        &self,
        state: &Tracked,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, ObservedError> {
        Ok(self.reference.sample(&state.state, sites, shots, seed)?)
    }

    fn expectation(
        &self,
        state: &Tracked,
        observable: &PauliSum,
    ) -> Result<Complex64, ObservedError> {
        Ok(self.reference.expectation(&state.state, observable)?)
    }

    fn shape(&self, state: &Tracked) -> Result<MpsShape, ObservedError> {
        Ok(self.reference.shape(&state.state)?)
    }

    fn retained_weight(&self, state: &Tracked) -> Result<f64, ObservedError> {
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
