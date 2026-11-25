// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMpsEngine`]: `tensornet`'s [`MpsEngine`] on any
//! [`SiteAlgebra`], a matrix product state kept in canonical form.
//!
//! A simulator runs each shot of a program through the contract. It
//! prepares a state and applies one- and two-site gates, possibly on qubits
//! far apart in the chain. Between them, it reads probabilities, projects
//! measured values, estimates observables and draws samples. This engine
//! holds the rules of all of it once. So a backend writes only the tensor
//! operations of [`SiteAlgebra`], and every backend fuses, applies and
//! reads alike, and truncates within the same bounds.
//!
//! The crate's `docs/engine.md` gives the phases each method runs, how and
//! why, the backend operations each phase calls, where each method leaves
//! the centre, and the numerics of the truncations and reads. The crate
//! README's Concepts define the words the engine's modules use, for a reader
//! new to tensor networks.
//!
//! **The canonical form** keeps every site but the centre an isometry
//! towards it. An isometry multiplied by its own adjoint gives the
//! identity. So, with the centre on a site, the rest of the chain drops out
//! of any read of that site. A gate on two neighbours, one of them the
//! centre, changes only those two. So a read or a gate costs what its sites
//! and the distance the centre moves cost, not the chain's length.
//!
//! Each operation moves the centre only as far as it must. The engine's
//! parts:
//!
//! ```text
//! engine.rs            CanonicalMpsEngine: each MpsEngine method
//!   state.rs           CanonicalMpsState, which holds the chain
//!   chain.rs           the sites, the centre, the pending gates, the
//!                      statistics
//!   fusion.rs          the pending window: which gates join it, when it
//!                      is applied
//!   gate_products.rs   the pending gates' arithmetic, on the host
//!   splits.rs          a gate on two neighbours by one gate_split, the one
//!                      place the state's bonds are truncated
//!   mpo.rs             any other window, as a matrix product operator
//!     operator_mpo.rs  the window's operator, one MPO tensor per site
//!     mpo_site.rs      one site's MPO tensor, and its product with a site
//!   centre_moves.rs    the centre moved by one qr_move per site crossed
//!   host_site.rs       a site's elements on the host, its pending gate
//!                      applied
//!   measurement.rs     probabilities and projections, at the centre
//!   expectation.rs     an observable's expectation, in one walk
//!     term.rs          one Pauli string, its factors in site order
//!     environment.rs   the sites on one side of a bond, contracted
//!   statistics.rs      CanonicalMpsStatistics: what a state's history cost
//!   cutoffs.rs         DEFAULT_CUTOFF and the operator's rank cutoff, with
//!                      their reasons
//!   canonical_mps_error.rs
//!                      CanonicalMpsError: refusals and failures
//!   conformance.rs     the circuits that check a backend under the engine
//!                      (feature `conformance`)
//! ```
//!
//! Each method the contract checks first calls its `tensornet::checks`
//! function, then its part. So the algebra never sees a refused call.
//!
//! # Further reading
//!
//! - U. Schollwöck, "The density-matrix renormalization group in the age
//!   of matrix product states", arXiv:1008.3477
//!   (<https://arxiv.org/abs/1008.3477>), 2011, published in Ann. Phys.
//!   326(1):96–192: §4.1,
//!   "Introduction of matrix product states", for the sites, the bonds and
//!   the SVD; §4.4, "Bringing a matrix product state into canonical form",
//!   for the isometries and the centre; §7.3.2, "Time-evolving block
//!   decimation (TEBD)", for a gate on two neighbours applied and truncated
//!   by one SVD, as a split does.

mod canonical_mps_error;
mod centre_moves;
mod chain;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod cutoffs;
mod expectation;
mod fusion;
mod gate_products;
mod host_site;
mod measurement;
mod mpo;
mod splits;
mod state;
mod statistics;

pub use canonical_mps_error::CanonicalMpsError;
pub use cutoffs::DEFAULT_CUTOFF;
pub use state::CanonicalMpsState;
pub use statistics::CanonicalMpsStatistics;

use crate::site_algebra::SiteAlgebra;
use chain::Chain;
use num_complex::Complex64;
use std::cell::RefCell;
use tensornet::{MpsEngine, MpsShape, PauliSum, checks, sampling};

/// The [`MpsEngine`] of a matrix product state in canonical form, on the
/// backend `A`. The backend holds the site tensors and runs every operation
/// on them. A backend crate names its own engine as this one on its algebra.
///
/// Beyond the contract, a caller may rely on three things, the same on
/// every backend: the bounds its truncations stay within, how it defers
/// gates, and how it applies a gate to sites far apart. The paragraphs
/// below say what each gives a caller. The engine's files say how it does
/// it, and the crate's `docs/engine.md` gives the numerics.
///
/// **Truncation.** Every split drops a tail of singular values within the
/// engine's cutoff, never the largest value. It keeps at most the
/// capacity's bond at its cut (the `χ` of the chain a caller asks for in
/// [`MpsEngine::prepare`]). [`SiteAlgebra::gate_split`]'s contract sets
/// those bounds for every backend. A split normalises the values it keeps,
/// so the chain keeps a norm of about 1. It multiplies the state's retained
/// weight, which [`MpsEngine::retained_weight`] reports, by the share of
/// the squared values they held. Callers divide by the weight, so a split
/// that would take it below the smallest normal `f64` fails with
/// [`CanonicalMpsError::WeightUnderflow`].
///
/// **Fusion.** The engine defers gates until an operation needs the
/// tensors they change. Two-site gates on up to three sites, neighbours or
/// not, gather into one pending window, and a one-site gate waits on its
/// site. So the backend sees one application for a run of gates. A read
/// always sees every gate applied. Fusion moves where the truncations
/// fall. So a truncated run's values may differ from those of a run that
/// applies each gate at once, within the truncation error. Without
/// truncation, up to rounding, they agree. The engine's `fusion.rs` gives
/// the rules: which gates join the window, which operations apply it, and
/// where it leaves the centre.
///
/// **Sites far apart.** A window on two neighbours is applied by one split.
/// Any other, on sites further apart or on three sites, is applied as a
/// matrix product operator (MPO) across its span, the sites from its first
/// to its last. So the chain stays in the caller's order, and every split
/// is at a caller's cut, where the state must fit the capacity anyway.
/// While the span is multiplied, its bonds reach up to the capacity's times
/// the operator's bond (at most 4). [`CanonicalMpsState::statistics`]
/// reports them apart from the bonds at rest. The engine's `mpo.rs` gives
/// the steps. The crate's `docs/engine.md` gives their cost, and compares
/// this with routing the sites together by SWAP gates.
///
/// **Failures.** After a failure the state is unspecified, as
/// [`MpsEngine`] says, and the caller must drop it. [`CanonicalMpsError`]
/// lists the failures. An application of the pending window that fails may
/// lose its gates.
#[derive(Clone, Debug)]
pub struct CanonicalMpsEngine<A> {
    algebra: A,
    cutoff: f64,
    fuses: bool,
}

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// The engine on `algebra`, with the cutoff [`DEFAULT_CUTOFF`].
    pub fn new(algebra: A) -> Self {
        Self {
            algebra,
            cutoff: DEFAULT_CUTOFF,
            fuses: true,
        }
    }

    /// The same engine, at `cutoff`. Each split keeps its largest singular
    /// value. It drops values whose squares sum to at most `cutoff` times
    /// the sum of all their squares, and beyond it values of rounding's size
    /// ([`GateSplitCall::cutoff`](crate::site_algebra::GateSplitCall::cutoff)
    /// says how small). The capacity's bond may cut more: every split keeps
    /// at most that bond (Truncation on [`CanonicalMpsEngine`]). These are
    /// the bounds of [`SiteAlgebra::gate_split`]'s contract. Within them,
    /// the algebra chooses which values a split drops.
    ///
    /// The cutoff and the capacity's bond `χ` are the two settings that
    /// trade time for accuracy. The cutoff bounds the weight a split may
    /// drop, so it keeps a bond as small as the state allows. `χ` bounds the
    /// bond, whatever the weight it drops. Whichever binds first sets a
    /// split's bond, and the bonds set the time. The weight the splits drop
    /// adds up over a run. So a run of `N` splits drops up to about
    /// `N · cutoff` of the state's weight, plus whatever `χ` cuts.
    /// [`MpsEngine::retained_weight`] reports what they kept.
    ///
    /// # Panics
    ///
    /// When `cutoff` is not in `[0, 1)`, NaN included. It is a fraction of a
    /// split's weight, and a cutoff of 1 or more would drop every value.
    #[must_use]
    pub fn with_cutoff(self, cutoff: f64) -> Self {
        assert!(
            (0.0..1.0).contains(&cutoff),
            "a cutoff is a fraction of a split's weight, in [0, 1); got {cutoff}"
        );
        Self { cutoff, ..self }
    }
}

impl<A: SiteAlgebra> MpsEngine for CanonicalMpsEngine<A> {
    type State = CanonicalMpsState<A>;
    type Error = CanonicalMpsError<A::Error>;

    /// Prepares the state |0…0⟩ within `capacity`: one site per qubit,
    /// each uploaded to the algebra with bonds of extent 1, the centre on
    /// site 0 and a retained weight of 1. The bonds grow, up to the
    /// capacity's, only as gates entangle.
    ///
    /// Fails when the algebra fails to upload a site.
    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, Self::Error> {
        checks::prepare(capacity)?;
        let capacity_bonds = (0..capacity.site_count() - 1)
            .map(|cut| capacity.bond_dimension(cut).expect("a cut of the capacity"))
            .collect();
        Ok(CanonicalMpsState {
            chain: RefCell::new(Chain::zero(&self.algebra, capacity_bonds)?),
        })
    }

    /// Applies `matrix` to `sites`, deferred until an operation needs the
    /// tensors it changes (Fusion on [`CanonicalMpsEngine`]). A one-site
    /// gate joins its site's pending gate, or the pending window when its
    /// site is in it. A two-site gate joins the pending window; when the
    /// window cannot take it, the window is applied first and the gate
    /// opens a new one.
    ///
    /// Fails only when it applies a pending window, as an application may
    /// (Failures on [`CanonicalMpsEngine`]): in the algebra's moves,
    /// splits, reads or uploads, or in the factorisation of its operator.
    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), Self::Error> {
        let chain = state.chain.get_mut();
        checks::apply(chain.sites.len(), sites, matrix)?;
        match *sites {
            [site] => {
                chain.apply_one(site, matrix);
                Ok(())
            }
            [_, _] => self.apply_two(chain, sites, matrix),
            _ => unreachable!("`checks::apply` refuses an operator on no site or more than two"),
        }
    }

    /// The probability that `site` reads One. Any pending window is
    /// applied toward the site, the centre moved onto it, and the site
    /// read; the rest of the chain drops out there.
    ///
    /// Reads P(One) from the value's own weight at the centre, not as one
    /// minus the other's. So, when the other value's weight is positive and
    /// finite, a value of weight exactly zero reads exactly 0.
    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        checks::probability_of_one(chain.sites.len(), site)?;
        Ok(self.probabilities(&mut chain, site)?[1])
    }

    /// The probability that `site` reads One, given that each site of
    /// `given` read the value beside it (`true` for One), leaving `state`
    /// unprojected.
    ///
    /// Forks once, when `given` is not empty, and projects every condition
    /// on that fork. Reads each condition's probability from its value's
    /// own weight, not as one minus the other's. So a rare value whose
    /// complement rounds to one is not refused. Refuses an answer of NaN,
    /// which a state whose norm is zero or NaN gives even when `given` is
    /// empty.
    fn conditional_probability_of_one(
        &self,
        state: &Self::State,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, Self::Error> {
        let site_count = state.chain.borrow().sites.len();
        checks::conditional_probability_of_one(site_count, site, given)?;
        Ok(self.conditional_probabilities(state, site, given)?[1])
    }

    /// Projects `site` on One when `one` is true, on Zero when it is false,
    /// as a measurement that read that value, and returns that value's
    /// probability `p`. The centre is moved onto the site first, as for a
    /// probability.
    ///
    /// Reads `p` from the value's own weight at the centre site, not as one
    /// minus the other's. Divides the site by the norm of what it kept,
    /// computed scaled rather than as `√p`. So, up to rounding, the state's
    /// norm is 1, however few digits `p` holds.
    ///
    /// Drops no singular value of its own. A projection does not compress
    /// the chain again. So the bonds [`MpsEngine::shape`] reports after it
    /// may exceed the state's Schmidt ranks. On the chemistry circuits
    /// measured, recompressing cost more than it saved, at the same accuracy.
    /// The gates that follow regrow the bonds it shrinks.
    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, Self::Error> {
        let chain = state.chain.get_mut();
        checks::project(chain.sites.len(), site)?;
        self.project_on(chain, site, one)
    }

    /// A copy of `state` that changes apart from it, with its centre,
    /// pending one-site gates, retained weight and statistics.
    ///
    /// Applies the pending window first, so the forks do not each apply it.
    /// Then duplicates every site through the algebra.
    fn fork(&self, state: &Self::State) -> Result<Self::State, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        self.flush_window_toward_centre(&mut chain)?;
        Ok(CanonicalMpsState {
            chain: RefCell::new(chain.duplicate(&self.algebra)?),
        })
    }

    /// Draws `shots` samples, each the values of `sites`, from the state's
    /// distribution, seeded by `seed`, leaving `state` unprojected.
    ///
    /// Draws by `tensornet`'s shared sampler,
    /// [`sample_by_conditionals`](sampling::sample_by_conditionals), from the
    /// [conditional probabilities](Self::conditional_probability_of_one).
    /// Each new nonempty prefix of values projects them on a fork, so the
    /// state is not projected. A value of probability exactly zero is never
    /// drawn, and a probability of NaN is refused, as
    /// [`conditional_probability_of_one`](Self::conditional_probability_of_one)
    /// refuses it. Fails with [`CanonicalMpsError::TooManyValues`] when the
    /// values cannot be held.
    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, Self::Error> {
        checks::sample(state.chain.borrow().sites.len(), sites)?;
        sampling::sample_by_conditionals(sites, shots, seed, |site, given| {
            Ok(self.conditional_probabilities(state, site, given)?[1])
        })
    }

    /// The expectation of `observable`, a sum of Pauli strings, in the
    /// state.
    ///
    /// Contracts only the sites between the observable's sites and the
    /// centre, and divides by the squared norm at the centre. Each site is
    /// read once for all the terms.
    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        checks::expectation(chain.sites.len(), observable)?;
        self.expectation_of(&mut chain, observable)
    }

    /// The bonds the chain holds. A pending window is applied first, toward
    /// the centre, since its truncations set its bonds; it may fail as any
    /// application of a window (Failures on [`CanonicalMpsEngine`]).
    fn shape(&self, state: &Self::State) -> Result<MpsShape, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        self.flush_window_toward_centre(&mut chain)?;
        Ok(chain.shape())
    }

    /// The product of the splits' retained fractions, kept apart from the
    /// chain's norm. A pending window is applied first, toward the centre,
    /// since its truncations set their fractions; it may fail as any
    /// application of a window (Failures on [`CanonicalMpsEngine`]).
    fn retained_weight(&self, state: &Self::State) -> Result<f64, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        self.flush_window_toward_centre(&mut chain)?;
        Ok(chain.retained_weight)
    }
}

#[cfg(test)]
mod tests;
