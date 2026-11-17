// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMpsEngine`]: `tensornet`'s [`MpsEngine`] on any
//! [`SiteAlgebra`], a matrix product state kept in canonical form.
//!
//! A simulator runs each shot of a program through the contract: it
//! prepares a state, applies one- and two-site gates, possibly on qubits
//! far apart in the chain, and between them reads probabilities, projects
//! measured values, estimates observables and draws samples. This engine
//! holds the rules of all of it once, so that a backend (the CPU, a GPU
//! library) writes only the tensor operations of [`SiteAlgebra`], and every
//! backend fuses, applies and reads alike, and truncates within the same
//! bounds.
//!
//! **The canonical form.** The state of `n` qubits is a chain of `n` site
//! tensors, one per qubit, neighbours sharing a bond. Every site but one is
//! an isometry towards the remaining one, the orthogonality centre (the centre,
//! below), which holds the state's norm. With the centre on a site, the
//! rest of the chain drops out of any read of that site, and a gate on two
//! neighbours, one of them the centre, changes only those two. So a read or
//! a gate costs what its sites and the distance the centre moves cost, not
//! the chain's length.
//!
//! Each operation moves the centre only as far as it must, by its part:
//!
//! - [`chain`]: the sites, the centre, the gates still pending, and the
//!   state's statistics;
//! - [`window`]: fusion; a one-site gate waits, pending, on its site, and
//!   two-site gates on up to three sites, neighbours or not, gather into
//!   the pending window, which the first operation it does not serve
//!   applies at once;
//! - [`split`]: the split of two neighbours, a gate applied, by one
//!   [`gate_split`](SiteAlgebra::gate_split), the one place where the
//!   state's bonds are truncated; it applies a window on two neighbours,
//!   and leaves the centre on the side it is asked for;
//! - [`mpo`]: any other window, applied as a matrix product operator
//!   across its span, compressed back by splits;
//! - [`canonical_form`]: moving the centre by one
//!   [`qr_move`](SiteAlgebra::qr_move) per site crossed, which carries the
//!   pending gate of the site it leaves;
//! - [`gates`]: the arithmetic of the pending gates, on the host;
//! - [`reads`]: probabilities, projections and conditional probabilities,
//!   at the centre;
//! - [`expectation`]: an observable's expectation, all its terms in one
//!   walk;
//! - [`policy`]: the default cutoff and the operator's rank threshold,
//!   with their reasons;
//! - [`error`]: why an operation was refused or failed.
//!
//! Each method the contract checks, here, first calls its
//! `tensornet::checks` function, so the algebra never sees a refused call,
//! then calls its part.

mod canonical_form;
mod chain;
mod error;
mod expectation;
mod gates;
mod mpo;
mod policy;
mod reads;
mod split;
mod window;

pub use chain::SplitStatistics;
pub use error::CanonicalMpsError;
pub use policy::DEFAULT_CUTOFF;

use crate::algebra::SiteAlgebra;
use chain::Chain;
use num_complex::Complex64;
use std::cell::RefCell;
use tensornet::{MpsEngine, MpsShape, PauliSum, checks, sampling};

/// The [`MpsEngine`] of a matrix product state in canonical form, on the
/// backend `A`, which holds the site tensors and runs every operation on
/// them. A backend crate names its own engine as this one on its algebra.
///
/// Beyond the contract, a caller may rely on how it fuses gates and applies
/// them to sites far apart, the same on every backend, and on the bounds
/// its truncations stay within, which [`SiteAlgebra::gate_split`]'s
/// contract sets for every backend.
///
/// **Truncation.** Every split drops a tail of singular values within the
/// engine's cutoff, never the largest value, and keeps at most the
/// capacity's bond at its cut (the `χ` of the chain a caller asks for in
/// [`MpsEngine::prepare`]). A split normalises the values it keeps,
/// so the chain keeps a norm of about 1, and multiplies the state's
/// retained weight by the share of the squared values they held, which
/// [`MpsEngine::retained_weight`] reports. An MPO's factorisation of its
/// window's operator (Sites far apart) also drops the operator's values
/// at or below `1e-12` times its largest, which that weight does not
/// count. A split that would take that weight below the smallest normal
/// `f64` fails with [`CanonicalMpsError::WeightUnderflow`]: the contract
/// keeps the weight
/// above zero, because callers divide by it. A gate may drift from norm 1
/// within the unitarity check's tolerance; on the centre's site it divides
/// out, as the reads divide by the squared norm at the centre, elsewhere it
/// shifts reads by at most that order. A projection divides its site by the
/// norm of what it kept, and leaves the retained weight unchanged.
///
/// **Fusion.** Two-site gates gather into a pending window of up to three
/// sites, neighbours or not: a gate joins while the window's sites and its
/// own number at most three, and one-site gates on the window's sites
/// multiply into it. The first operation the window does not serve (a
/// two-site gate that does not fit, any read, projection, fork, shape or
/// retained weight) applies it, so the backend sees one application for
/// all its gates, and leaves the centre on the end of the window's span
/// nearer the site that operation names (the lower end on a tie): a gate's
/// lower site; a read's site, but for a conditional read with conditions,
/// which forks; or the centre, for an expectation, a fork, a shape or the
/// retained weight. A one-site gate on another site waits on its site
/// until the next split or move of the centre across it, a projection of
/// it, or a window whose span holds it applies it; a read sees it applied.
/// Fusion moves where the truncations fall, so a truncated run's values
/// may differ from those of a run that applies each gate at once, within
/// the truncation error; without truncation they agree to rounding.
///
/// **Sites far apart.** A window on two neighbours is applied by one split.
/// Any other, on sites further apart or on three sites, is applied as a
/// matrix product operator (MPO) across its span, the sites from its first
/// to its last: its operator is factored, on the host, into one tensor per
/// site, the identity on the sites between; each site of the span is
/// multiplied by its tensor; and the span is compressed back by exact moves
/// of the centre across it and one split per cut, truncated as above. So
/// the chain is in the caller's order after every call, every split is at
/// a caller's cut, where the state must fit the capacity anyway, and an MPO
/// on a span of `d + 1` sites costs, once the centre is in the span,
/// `d − 1` moves of the centre and `d` splits. The price is transient:
/// while the span is multiplied, its bonds reach up to the capacity's
/// times the operator's bond (at most 4), which
/// [`CanonicalMpsState::split_statistics`] reports apart from the bonds at
/// rest. The crate's `docs/distant_sites.md` lists other ways to apply
/// gates to sites far apart, with what is known of each.
///
/// **Failures.** After a failure the state is unspecified, as
/// [`MpsEngine`] says, and the caller must drop it: an application of the
/// pending window that fails in the algebra may lose its gates, or leave
/// its span half compressed. A failed read or upload of a site of an MPO's
/// span leaves the span's sites unmultiplied, since the multiplied sites
/// replace them only once every upload succeeded, so neighbours still
/// share each bond; a failed factorisation of the window's operator keeps
/// the window and changes nothing ([`CanonicalMpsError`]).
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

    /// The same engine, whose splits each keep their largest singular value
    /// and drop values whose squares sum to at most `cutoff` times the sum
    /// of all their squares, unless the capacity's bond, which every split
    /// keeps at most (Truncation on [`CanonicalMpsEngine`]), cuts more.
    /// These are the bounds of [`SiteAlgebra::gate_split`]'s contract;
    /// which values a split drops within them is the algebra's to choose.
    ///
    /// The cutoff and the capacity's bond `χ` are the two settings that
    /// trade time for accuracy. The cutoff bounds the weight a split may
    /// drop, so it keeps a bond as small as the state allows; `χ` bounds the
    /// bond, whatever the weight it drops. Whichever binds first sets a
    /// split's bond, and the bonds set the time. The weight the splits drop
    /// adds up over a run, so a run of `N` splits drops up to about
    /// `N · cutoff` of the state's weight, plus whatever `χ` cuts;
    /// [`MpsEngine::retained_weight`] reports what they kept.
    ///
    /// # Panics
    ///
    /// When `cutoff` is not in `[0, 1)`, NaN included: it is a fraction of
    /// a split's weight, and a cutoff of 1 or more would drop every value.
    #[must_use]
    pub fn with_cutoff(self, cutoff: f64) -> Self {
        assert!(
            (0.0..1.0).contains(&cutoff),
            "a cutoff is a fraction of a split's weight, in [0, 1); got {cutoff}"
        );
        Self { cutoff, ..self }
    }

    /// The same engine, fusing when `fuses` (Fusion on
    /// [`CanonicalMpsEngine`]). The crate's users always get fusion;
    /// `with_fusion(false)` applies each two-site gate at once, as a window
    /// of its own, which the crate's tests compare with.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_fusion(self, fuses: bool) -> Self {
        Self { fuses, ..self }
    }
}

/// A state of a [`CanonicalMpsEngine`]: a chain of site tensors in
/// canonical form, wherever the algebra `A` keeps them, with the bond each
/// cut may grow to.
///
/// Every method that takes the state by shared reference may change its
/// chain: the reads move the centre, which changes the tensors but not the
/// state they describe, and the reads and [`MpsEngine::fork`] apply the
/// window that fusion left pending, which applies the truncation its gates
/// deferred. So the chain sits in a [`RefCell`]. The state is `Send` when
/// its algebra's tensors are.
pub struct CanonicalMpsState<A: SiteAlgebra> {
    chain: RefCell<Chain<A::Tensor>>,
}

impl<A: SiteAlgebra> CanonicalMpsState<A> {
    /// What the splits and moves of this state's history cost so far. A
    /// state made by [`MpsEngine::fork`] starts with its parent's history.
    #[must_use]
    pub fn split_statistics(&self) -> SplitStatistics {
        self.chain.borrow().statistics
    }
}

/// The chain's length, centre, retained weight and statistics, not its
/// tensors, which the algebra may keep where the host cannot print them.
impl<A: SiteAlgebra> std::fmt::Debug for CanonicalMpsState<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("CanonicalMpsState");
        if let Ok(chain) = self.chain.try_borrow() {
            debug
                .field("sites", &chain.sites.len())
                .field("centre", &chain.centre)
                .field("retained_weight", &chain.retained_weight)
                .field("statistics", &chain.statistics);
        }
        debug.finish_non_exhaustive()
    }
}

impl<A: SiteAlgebra> MpsEngine for CanonicalMpsEngine<A> {
    type State = CanonicalMpsState<A>;
    type Error = CanonicalMpsError<A::Error>;

    /// Fails when the algebra fails to upload a site.
    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, Self::Error> {
        checks::prepare(capacity)?;
        let capacity_bonds = (0..capacity.site_count() - 1)
            .map(|cut| capacity.bond_dim(cut).expect("a cut of the capacity"))
            .collect();
        Ok(CanonicalMpsState {
            chain: RefCell::new(Chain::zero(&self.algebra, capacity_bonds)?),
        })
    }

    /// Fails as the application of a pending window may (Failures on
    /// [`CanonicalMpsEngine`]): in the algebra's moves, splits, reads or
    /// uploads, or in the factorisation of its operator.
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

    /// Reads P(One) from the value's own weight at the centre, not as one
    /// minus the other's, so that a value of weight exactly zero reads
    /// exactly 0.
    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        checks::probability_of_one(chain.sites.len(), site)?;
        Ok(self.probabilities(&mut chain, site)?[1])
    }

    /// Forks once, when `given` is not empty, and projects every condition
    /// on that fork; reads each condition's probability from its value's
    /// own weight, not as one minus the other's, so a rare value whose
    /// complement rounds to one is not refused.
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

    /// Reads `p` from the value's own weight at the centre site, not as one
    /// minus the other's, and divides the site by the norm of what it kept,
    /// computed scaled rather than as `√p`, so the state's norm is 1 to
    /// rounding, however few digits `p` holds.
    ///
    /// Drops no singular value of its own: a projection does not compress
    /// the chain again, so the bonds [`MpsEngine::shape`] reports after it
    /// may exceed the state's Schmidt ranks. Recompressing was measured
    /// slower at the same accuracy, on H₂ phase estimation and a polyene's:
    /// the gates that follow regrow the bonds it shrinks.
    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, Self::Error> {
        let chain = state.chain.get_mut();
        checks::project(chain.sites.len(), site)?;
        self.project_on(chain, site, one)
    }

    /// Applies the pending window first, so the forks do not each apply it,
    /// then duplicates every site through the algebra.
    fn fork(&self, state: &Self::State) -> Result<Self::State, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        self.flush_window_toward_centre(&mut chain)?;
        Ok(CanonicalMpsState {
            chain: RefCell::new(chain.duplicate(&self.algebra)?),
        })
    }

    /// Draws by `tensornet`'s shared sampler,
    /// [`sample_by_conditionals`](sampling::sample_by_conditionals), from
    /// [`conditional_probability_of_one`][conditional]'s probabilities:
    /// each new nonempty prefix of values projects them on a fork, so the
    /// state is not projected. A value of probability exactly zero is never
    /// drawn. Fails with [`CanonicalMpsError::TooManyValues`] when the
    /// values cannot be held.
    ///
    /// [conditional]: MpsEngine::conditional_probability_of_one
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

    /// Contracts only the sites between the observable's sites and the
    /// centre, and divides by the squared norm at the centre.
    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        checks::expectation(chain.sites.len(), observable)?;
        self.expectation_of(&mut chain, observable)
    }

    fn shape(&self, state: &Self::State) -> Result<MpsShape, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        self.flush_window_toward_centre(&mut chain)?;
        Ok(chain.shape())
    }

    fn retained_weight(&self, state: &Self::State) -> Result<f64, Self::Error> {
        let mut chain = state.chain.borrow_mut();
        self.flush_window_toward_centre(&mut chain)?;
        Ok(chain.retained_weight)
    }
}
