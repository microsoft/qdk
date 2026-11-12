// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsEngine`]: what a library that holds matrix product states does for
//! a simulation method, in tensor terms only.
//!
//! The trait sits below every simulator protocol: no instructions, shots,
//! RNG objects or ISAs, only sites, matrices, probabilities and seeds. The
//! MPS method turns its gates into matrices and draws its own randomness;
//! the engine evolves, reads and samples the state. This keeps the engine
//! usable outside any simulator, and keeps the protocol free of tensors.
//! Each method's documentation states its contract; the crate's
//! `README.md` ("The engine") shows the order the method calls it in.
//!
//! Sites hold qubits: an engine prepares one two-level site per qubit.
//! Which qubit sits on which site is its caller's choice; the engine knows
//! only sites. Every call and every answer is in the caller's sites: where
//! an engine moves sites to bring a gate's sites together (its routing),
//! it hides that, so no read and no [`shape`](MpsEngine::shape) shows it.
//! How it routes is its own: the trait has no routing option.
//!
//! The method draws the values of a measurement one of three ways, each
//! with its own promise:
//!
//! - per shot, with [`probability_of_one`](MpsEngine::probability_of_one)
//!   then [`project`](MpsEngine::project): the method draws each value
//!   with its own generator, so a shot is replayable from its own random
//!   stream, whatever other shots ran;
//! - per shot without changing the state, with
//!   [`conditional_probability_of_one`](MpsEngine::conditional_probability_of_one):
//!   the same, for many shots on one state, each shot asking for P(One)
//!   given its own earlier values;
//! - all at once, with [`sample`](MpsEngine::sample): each shot is an
//!   independent draw, and the values are reproducible as a whole, not
//!   shot by shot, and agree with another engine's only in distribution.
//!
//! The trait states what a simulation method needs, in terms every MPS
//! library shares; what one library needs stays in its engine
//! (`design.md` lists the libraries checked, and what each needs).
//! Reads that every library offers but no method needs yet (the amplitude
//! of a basis state, the reduced density matrix of a few sites, the
//! Schmidt values at a cut) are planned, for a debugger that shows a
//! state.

pub mod checks;
#[cfg(any(test, feature = "conformance"))]
pub mod reference;
pub mod sampling;

#[cfg(doc)]
use crate::Refusal;
use crate::{MpsEngineError, MpsShape, PauliSum};
use num_complex::Complex64;

/// A matrix product state engine.
///
/// Every method takes `&self`, so one engine serves many states, and every
/// method may fail, because a state may live on a device that runs out of
/// memory. The read methods take the state by shared reference: an engine
/// that evaluates lazily keeps its pending work behind interior mutability.
/// Only [`apply`](Self::apply) and [`project`](Self::project) change the
/// state; a read may first apply the operators pending since the last one.
/// Neither the engine nor its state need be `Send`: an engine says so where
/// its state is.
///
/// A call whose arguments break the contract is refused, with the
/// [`Refusal`] its method's `# Errors` section names, and leaves the state
/// unchanged: the method's [`checks`] function makes the refusal, before
/// the method changes anything. The one refusal that reads the state,
/// [`Refusal::ZeroProbabilityOutcome`], is the projection's and the
/// conditional probability's own, made after their checks. Any other error
/// is the engine's own failure ([`MpsEngineError::refusal`] tells them
/// apart). After a failure the state is unspecified, and the caller must
/// drop it: no use case continues after a device error (the method fails
/// the shot), so no engine must undo what the failed call began.
///
/// `tensornet::reference::ExactReference` (feature `conformance`) gives the
/// exact answer every engine with enough bond must give.
pub trait MpsEngine {
    /// One matrix product state the engine holds. Dropping it releases it.
    type State;

    /// Why an operation failed: refused, with a [`Refusal`], or failed in
    /// the engine.
    type Error: MpsEngineError;

    /// The state |0…0⟩ on `capacity.site_count()` sites, one qubit each,
    /// whose bonds (all 1) fit `capacity`. An engine that truncates keeps
    /// its chain within `capacity` from then on; an engine that does not
    /// (the reference) reports, through [`shape`](Self::shape), a chain
    /// that may outgrow it, for the caller to check.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::prepare`]) with [`Refusal::NotTwoLevel`] when a
    /// site of `capacity` is not two-level. Fails when the engine cannot
    /// hold such a state.
    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, Self::Error>;

    /// Applies the unitary `matrix` to `sites`: ψ ← (M ⊗ I)ψ, with I on
    /// every other site. The matrix is row-major, `M[out][in]`, over the
    /// sites' joint index with `sites[0]` the most significant bit (the
    /// values b₀ … bₖ₋₁ of `sites[0]` … `sites[k − 1]` are index
    /// Σⱼ bⱼ·2^(k−1−j)): the textbook convention, in which
    /// `CX(control, target)` on `[control, target]` is the matrix every
    /// textbook prints, so a method writes its gates as they are written
    /// down.
    ///
    /// An operator acts on one or two sites, on every engine: a method
    /// decomposes a larger gate (a Toffoli, for instance) into gates on one
    /// or two sites before it calls `apply`. An operator on more
    /// sites is refused, the same on every engine, so a caller learns the
    /// limit from the contract rather than from an engine's failure.
    ///
    /// The matrix is checked to be unitary ([`checks::apply`], the sparse
    /// simulator's rule), because an engine relies on it: a unitary keeps
    /// the chain's canonical form and the state's norm, which every later
    /// read assumes, so a matrix that is not unitary would make every later
    /// read wrong without any error. A non-unitary operation (a Kraus
    /// operator of amplitude damping) is a planned `apply_kraus`, of which
    /// [`project`](Self::project) is the special case it generalizes.
    ///
    /// Where the state the gate leaves fits the capacity in the caller's
    /// sites, the gate drops nothing but what the engine's cutoff discards,
    /// at each split it makes, its routing's included (an engine that
    /// moves sites to bring the gate's sites together): otherwise a result
    /// would depend on where its routing left the sites. Where that state
    /// does not fit, an engine that truncates keeps a state that fits,
    /// losing weight, and which one may depend on its routing.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::apply`]) with [`Refusal::NoSites`]
    /// for no site; [`Refusal::TooManySites`] for more than two;
    /// [`Refusal::SiteOutOfRange`] or [`Refusal::RepeatedSite`]
    /// for a site out of range or named twice; [`Refusal::MatrixLength`]
    /// when the matrix does not have `4^sites.len()` elements; and
    /// [`Refusal::NotUnitary`]. Fails as every method may.
    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), Self::Error>;

    /// P(One) for a measurement in Z of `site`, without changing the state:
    /// ⟨ψ|Π₁|ψ⟩ / ⟨ψ|ψ⟩, the probability in the normalized state, where Π₁
    /// projects `site` on One.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::probability_of_one`]) with
    /// [`Refusal::SiteOutOfRange`]. Fails as every method may.
    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, Self::Error>;

    /// P(One) for a measurement in Z of `site`, given that each site of
    /// `given` was measured with the value beside it, without changing the
    /// state: ⟨ψ|Π Π₁|ψ⟩ / ⟨ψ|Π|ψ⟩, where Π projects each site of `given`
    /// on its value and Π₁ projects `site` on One. A method that
    /// samples N shots with each shot's own random stream asks for one such
    /// probability per distinct set of earlier values, so an engine that
    /// can contract with projected sites should do so rather than copy the
    /// state.
    ///
    /// The default checks the arguments
    /// ([`checks::conditional_probability_of_one`], with the site count read
    /// from [`shape`](Self::shape)), then forks the state and
    /// [`project`](Self::project)s the fork on each condition in order, as a
    /// split would, passing on the projection's refusal of a value of
    /// probability zero. It reads [`shape`](Self::shape) only for the site
    /// count, so an engine whose `shape` costs a synchronization or a
    /// contraction should override this method.
    ///
    /// A condition is refused only when its probability is exactly zero (as
    /// the QDK's `PostSelectZ` refuses it) or NaN. An impossible condition
    /// may keep a tiny probability from rounding, and the answer given it
    /// is then meaningless: a method that must refuse such a condition does
    /// so by its own threshold, before it calls the engine.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::conditional_probability_of_one`]) with
    /// [`Refusal::SiteOutOfRange`] or [`Refusal::RepeatedSite`] for a site
    /// of `given` out of range or named twice, or `site` out of range; and,
    /// once those pass, with [`Refusal::ZeroProbabilityOutcome`] when
    /// `given` has probability exactly zero (as the QDK's `PostSelectZ`
    /// refuses it) or NaN. Fails as every method may.
    fn conditional_probability_of_one(
        &self,
        state: &Self::State,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, Self::Error> {
        let site_count = self.shape(state)?.site_count();
        checks::conditional_probability_of_one(site_count, site, given)?;
        let mut fork = self.fork(state)?;
        for &(at, one) in given {
            self.project(&mut fork, at, one)?;
        }
        self.probability_of_one(&fork, site)
    }

    /// Projects `site` on One when `one` is true, on Zero when it is false,
    /// and returns `p`, the probability of that value in the normalized
    /// state (⟨ψ|Π|ψ⟩ / ⟨ψ|ψ⟩, where Π projects `site` on that value),
    /// having divided the projected state by `√p`, which
    /// keeps its norm ⟨ψ|ψ⟩. A projection leaves the
    /// [`retained_weight`](Self::retained_weight) unchanged: it is no
    /// truncation.
    ///
    /// The engine computes `p` from the projection itself, so the state a
    /// projection leaves never depends on what a caller passes (a `p`
    /// passed in that was not the value's probability would scale ⟨ψ|ψ⟩ by
    /// the ratio of the two). A library that takes `p` from its caller, to
    /// spare a second contraction, keeps that saving inside its engine: it
    /// reuses the probability it last read for that site, or defers the
    /// division to its next contraction.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::project`]) with [`Refusal::SiteOutOfRange`]; and,
    /// once that passes, with [`Refusal::ZeroProbabilityOutcome`] when `p` is
    /// exactly zero (as the QDK's `PostSelectZ` refuses it) or NaN, leaving
    /// the state unchanged. A value that is
    /// impossible may keep a tiny probability from rounding, and is then
    /// projected on, which gives a meaningless state: a method that must
    /// refuse it does so by its own threshold, before it calls the engine.
    /// Fails as every method may.
    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, Self::Error>;

    /// An independent copy of `state`: what one changes, the other does
    /// not see. Its case is grouped shots: the shots of a group share one
    /// state until a measurement splits them, and each value drawn takes a
    /// fork.
    ///
    /// The copy holds the state it copies, truncated or not: it reads what
    /// `state` reads and has its retained weight, to rounding. So a fork
    /// copies the chain `state` holds: a state recomputed from |0…0⟩ by the
    /// same operators truncates along another path and is another
    /// approximation, with another retained weight, so grouped shots would
    /// depend on how the shots are grouped.
    ///
    /// # Errors
    ///
    /// Fails when the engine cannot hold the copy; refuses nothing.
    fn fork(&self, state: &Self::State) -> Result<Self::State, Self::Error>;

    /// Draws a value for each of `sites` (distinct), for `shots` shots, from
    /// one seed, without changing the state: `values[s * sites.len() + t]`
    /// is shot `s`'s value of `sites[t]`. Each shot is a draw from the
    /// normalized state's distribution of those sites' values, independent
    /// of the other shots drawn with it. One call draws every shot, so an
    /// engine may share work among them (the conditional probabilities of
    /// a common prefix of values, or one sweep of the chain).
    ///
    /// The values are reproducible as a whole: the same seed and the same
    /// number of shots give the same values on the same engine, so a shot's
    /// values are reproduced only by drawing again with that seed and that
    /// number of shots, not alone. Another seed gives other values (with
    /// overwhelming probability, when enough of the values are random).
    /// Engines need not agree with one another value for value, only in
    /// distribution. An empty list of sites, or zero shots, is no fault in
    /// itself and gives no values; the sites are checked all the same.
    ///
    /// Any `u64` is a seed, and every bit of it selects the values: seeds
    /// that differ in one bit, the lowest or the highest, give other values.
    /// An engine that draws the values itself from its conditional
    /// probabilities may use the shared
    /// [`sample_by_conditionals`](crate::sampling::sample_by_conditionals).
    ///
    /// # Errors
    ///
    /// Refused ([`checks::sample`]) with [`Refusal::SiteOutOfRange`] or
    /// [`Refusal::RepeatedSite`], with any number of shots, zero included.
    /// Fails as every method may, for example when the values do not fit in
    /// memory.
    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, Self::Error>;

    /// ⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩: the expectation of `observable` in the normalized
    /// state, without changing it.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::expectation`]) with [`Refusal::SiteOutOfRange`]
    /// when a factor of `observable` is on a site the state does not have.
    /// Fails as every method may.
    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, Self::Error>;

    /// The chain the engine holds now, in the caller's sites: one site per
    /// qubit, in order, with the bond extent the engine holds at each cut.
    /// It is what the run pays for: the engine's memory and its time per
    /// gate grow with these bonds, and the largest
    /// ([`MpsShape::largest_bond`](crate::MpsShape::largest_bond)) is the
    /// cost a method reports. An engine that truncates holds at most the
    /// capacity's bonds, after a gate whose result does not fit them too
    /// ([`apply`](Self::apply)).
    ///
    /// It says how the engine behaves, not how entangled the state is: a
    /// bond is not the state's exact Schmidt rank at its cut. An engine
    /// whose truncation drops only the singular values at or below a cutoff
    /// holds, after gates alone, exactly the Schmidt values of its state
    /// above that cutoff at each cut, up to the capacity's bond; an engine
    /// whose cutoff bounds the discarded weight instead may drop larger
    /// values.
    /// After a measurement ([`project`](Self::project)) an engine may keep a
    /// bond larger than that, when it does not compress again (its choice,
    /// for speed), so two engines may report different chains for one
    /// program there.
    ///
    /// # Errors
    ///
    /// Fails as every method may; refuses nothing.
    fn shape(&self, state: &Self::State) -> Result<MpsShape, Self::Error>;

    /// The weight the engine's truncations have kept: the product of the
    /// fractions of the squared norm each truncation kept, however the
    /// engine holds it. An engine may keep it as the norm ⟨ψ|ψ⟩ of a state
    /// it never renormalizes, or renormalize after each truncation and
    /// track the product apart; either way every read is of the normalized
    /// state. It is 1 after [`prepare`](Self::prepare), and on an engine
    /// that never truncates (the reference) it stays 1 up to rounding and
    /// the distance from unitarity that [`apply`](Self::apply) tolerates,
    /// which adds up over gates. It never increases, up to the same: a
    /// truncation lowers it, and gates and projections keep it. It stays
    /// above zero, because callers divide by it: an engine keeps at least
    /// the largest Schmidt value of each cut it truncates, whatever its
    /// cutoff. A bond's size does not say whether its cut truncated; the
    /// weight says what the truncations cost in total.
    ///
    /// What it bounds depends on how many truncations there were, with
    /// gates only after them, when each truncation is an orthogonal
    /// projection (a truncated singular value decomposition at a canonical
    /// cut). Write ψ for the exact state and ψ̃ for the state the
    /// truncations leave before any renormalizing, whose squared norm is
    /// the weight w, and εₜ for the fraction of the squared norm truncation
    /// t discarded, so that w = Πₜ (1 − εₜ). After one truncation, the
    /// fidelity of the normalized state the reads use is
    /// F = |⟨ψ|ψ̃⟩|² / w = w, so the total variation distance of any readout
    /// is at most √(1 − w). After several, F ≈ w is an estimate (Zhou,
    /// Stoudenmire and Waintal, 2020), not a bound; ‖ψ − ψ̃‖ ≤ Σₜ √εₜ is.
    ///
    /// Like the other reads, it applies the operators pending since the
    /// last read first, so on an engine that truncates when it computes,
    /// reading it may truncate.
    ///
    /// # Errors
    ///
    /// Fails as every method may; refuses nothing.
    fn retained_weight(&self, state: &Self::State) -> Result<f64, Self::Error>;
}
