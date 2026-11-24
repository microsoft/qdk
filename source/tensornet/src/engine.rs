// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsEngine`], whose documentation states the contract, and what every
//! engine shares: its checks, refusals, errors, sampling and exact
//! reference.

pub mod checks;
mod mps_engine_error;
#[cfg(any(test, feature = "conformance"))]
pub mod reference;
mod refusal;
pub mod sampling;

pub use mps_engine_error::MpsEngineError;
pub use refusal::Refusal;

use crate::{MpsShape, PauliSum};
use num_complex::Complex64;

/// A matrix product state engine: what a library that holds matrix product
/// states does for a simulation method, in tensor terms only.
///
/// The engine knows sites, matrices, probabilities and seeds, never
/// instructions, shots, RNG objects or ISAs. The method turns its gates
/// into matrices and draws its own randomness; the engine evolves, reads
/// and samples the state. Each method's documentation states its contract.
/// The crate's `README.md` ("The engine") shows the order the method calls
/// them in.
///
/// Each site holds one qubit: an engine prepares one two-level site per
/// qubit. The caller chooses which qubit sits on which site. Every call and
/// every answer is in the caller's sites. An engine may move sites to bring
/// a gate's sites together (its routing); it hides that from every read,
/// [`shape`](Self::shape) included. How it routes is its own choice.
///
/// The method draws a measurement's values in one of three ways, each with
/// its own promise:
///
/// - per shot, with [`probability_of_one`](Self::probability_of_one) then
///   [`project`](Self::project): the method draws each value with its own
///   generator, so a shot replays from its own random stream, whatever
///   other shots ran;
/// - per shot on an unchanged state, with
///   [`conditional_probability_of_one`](Self::conditional_probability_of_one):
///   the same promise, for many shots on one state; each shot asks for
///   P(One) given its own earlier values;
/// - all at once, with [`sample`](Self::sample): each shot is an
///   independent draw. The values reproduce as a whole, not shot by shot,
///   and agree with another engine's only in distribution.
///
/// Every method takes `&self`, so one engine serves many states. Every
/// method may fail, because a state may live on a device that runs out of
/// memory. Only [`apply`](Self::apply) and [`project`](Self::project)
/// change the state. The reads take it by shared reference: an engine that
/// evaluates lazily keeps its pending work behind interior mutability, and
/// a read may first apply the operators pending since the last one.
/// Neither the engine nor its state need be `Send`; an engine says so where
/// its state is.
///
/// A call whose arguments break the contract is refused, with the
/// [`Refusal`] its method's `# Errors` section names. The method's
/// [`checks`] function makes the refusal before the method changes
/// anything, so the state is unchanged. The one refusal that reads the
/// state, [`Refusal::ZeroProbabilityOutcome`], comes after the checks and
/// also leaves the state unchanged. Any other error is the engine's own
/// failure; [`MpsEngineError::refusal`] tells the two apart. After a
/// failure the state is unspecified and the caller must drop it; no engine
/// undoes what a failed call began (`docs/design.md` says why).
///
/// `tensornet::reference::ExactReference` (feature `conformance`) gives the
/// exact answer every engine with enough bond must give.
///
/// `docs/design.md` lists the MPS libraries checked against this trait, and
/// what each needs.
pub trait MpsEngine {
    /// One matrix product state the engine holds. Dropping it releases it.
    type State;

    /// Why an operation failed: refused, with a [`Refusal`], or failed in
    /// the engine.
    type Error: MpsEngineError;

    /// The state |0…0⟩ on `capacity.site_count()` sites, one qubit each;
    /// its bonds (all 1) fit `capacity`. From then on, an engine that
    /// truncates keeps its chain within `capacity`. An engine that does not
    /// (the reference) may report, through [`shape`](Self::shape), a chain
    /// that outgrows it; the caller checks.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::prepare`]) with [`Refusal::NotTwoLevel`] when a
    /// site of `capacity` is not two-level. Fails when the engine cannot
    /// hold such a state.
    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, Self::Error>;

    /// Applies the unitary `matrix` to `sites`: ψ ← (M ⊗ I)ψ, with I on
    /// every other site. The matrix is row-major, `M[out][in]`, over the
    /// sites' joint index, `sites[0]` the most significant bit: the values
    /// b₀ … bₖ₋₁ of `sites[0]` … `sites[k − 1]` are index Σⱼ bⱼ·2^(k−1−j).
    /// This is the textbook convention: `CX(control, target)` on
    /// `[control, target]` is the matrix textbooks print, so a method
    /// writes its gates as they are written down.
    ///
    /// On every engine, an operator acts on one or two sites. A method
    /// decomposes a larger gate (a Toffoli, for instance) before it calls
    /// `apply`. Every engine refuses an operator on more sites, so a caller
    /// learns the limit from the contract, not from an engine's failure.
    ///
    /// The matrix is checked to be unitary ([`checks::apply`]), because an
    /// engine relies on it. A unitary keeps the chain's canonical form and
    /// the state's norm, which every later read assumes; a matrix that is
    /// not unitary would make every later read wrong, with no error. A
    /// non-unitary operation (a Kraus operator of amplitude damping) is a
    /// planned `apply_kraus`, which [`project`](Self::project) is a special
    /// case of.
    ///
    /// Where the state the gate leaves fits the capacity in the caller's
    /// sites, the gate drops only what the engine's cutoff discards at each
    /// split it makes, its routing's included; otherwise a result would
    /// depend on where its routing left the sites. Where that state does
    /// not fit, an engine that truncates keeps a state that fits, losing
    /// weight; which state it keeps may depend on its routing.
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
    /// on its value and Π₁ projects `site` on One. A method that samples N
    /// shots, each with its own random stream, asks for one such
    /// probability per distinct set of earlier values. So an engine that
    /// can contract with projected sites should do so rather than copy the
    /// state.
    ///
    /// The default checks the arguments
    /// ([`checks::conditional_probability_of_one`], with the site count read
    /// from [`shape`](Self::shape)). It then forks the state and
    /// [`project`](Self::project)s the fork on each condition in order, as a
    /// split would. It passes on the projection's refusal of a value of
    /// probability zero or NaN. It refuses an answer of NaN itself, since an
    /// empty `given` projects nothing. So on a state whose norm is zero or
    /// NaN it refuses every `given`, as every engine must. It reads
    /// [`shape`](Self::shape) only for the site count: an engine whose
    /// `shape` costs a synchronization or a contraction should override
    /// this method.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::conditional_probability_of_one`]) with
    /// [`Refusal::SiteOutOfRange`] or [`Refusal::RepeatedSite`] for a site
    /// of `given` out of range or named twice, or `site` out of range; and,
    /// once those pass, with [`Refusal::ZeroProbabilityOutcome`] when
    /// `given` has probability zero or NaN. Fails as every method may.
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
        let probability = self.probability_of_one(&fork, site)?;
        // An answer of NaN is refused: an empty `given` projects nothing,
        // so no projection has refused a state whose norm is zero or NaN.
        if probability.is_nan() {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        Ok(probability)
    }

    /// Projects `site` on One when `one` is true, on Zero when it is false.
    /// Returns `p`, the probability of that value in the normalized state
    /// (⟨ψ|Π|ψ⟩ / ⟨ψ|ψ⟩, where Π projects `site` on that value), and divides
    /// the projected state by `√p`, which keeps its norm ⟨ψ|ψ⟩. A projection
    /// is no truncation: it leaves the
    /// [`retained_weight`](Self::retained_weight) unchanged.
    ///
    /// The engine computes `p` from the projection itself, so the state a
    /// projection leaves never depends on a caller's value: a wrong `p`
    /// would scale ⟨ψ|ψ⟩ by its ratio to the right one. A library whose
    /// call takes `p`, to spare a second contraction, keeps that saving
    /// inside its engine: it reuses the probability it last read for that
    /// site, or defers the division to its next contraction.
    ///
    /// # Errors
    ///
    /// Refused ([`checks::project`]) with [`Refusal::SiteOutOfRange`]; and,
    /// once that passes, with [`Refusal::ZeroProbabilityOutcome`] when `p` is
    /// zero or NaN, leaving the state unchanged. Fails as every method may.
    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, Self::Error>;

    /// An independent copy of `state`: what one changes, the other does
    /// not see. Its uses are the default
    /// [`conditional_probability_of_one`], which projects a fork, and
    /// grouped shots, which share one state until a measurement splits
    /// them.
    ///
    /// The copy holds the chain `state` holds, truncated or not: to
    /// rounding, it reads what `state` reads and has its retained weight.
    /// Recomputing from |0…0⟩ by the same operators would not do: it
    /// truncates along another path, giving another approximation with
    /// another retained weight, so grouped shots would depend on how the
    /// shots are grouped.
    ///
    /// # Errors
    ///
    /// Fails when the engine cannot hold the copy; refuses nothing.
    ///
    /// [`conditional_probability_of_one`]: Self::conditional_probability_of_one
    fn fork(&self, state: &Self::State) -> Result<Self::State, Self::Error>;

    /// Draws a value for each of `sites` (distinct), for `shots` shots, from
    /// one seed, without changing the state: `values[s * sites.len() + t]`
    /// is shot `s`'s value of `sites[t]`. Each shot is a draw from the
    /// normalized state's distribution of those sites' values, independent
    /// of the other shots drawn with it. One call draws every shot, so an
    /// engine may share work among them (the conditional probabilities of
    /// a common prefix of values, or one sweep of the chain).
    ///
    /// The values are reproducible as a whole: on the same engine, the same
    /// seed and number of shots give the same values. So a shot's values
    /// are reproduced only by drawing again with that seed and number of
    /// shots, not alone. Any `u64` is a seed, and every bit of it selects
    /// the values: another seed, even one that differs in the lowest or the
    /// highest bit only, gives other values (with overwhelming probability,
    /// when enough of the values are random). Engines agree with one
    /// another only in distribution, not value for value. An empty list of
    /// sites, or zero shots, gives no values; the sites are checked all the
    /// same.
    ///
    /// An engine that draws the values itself from its conditional
    /// probabilities may use the shared
    /// [`sample_by_conditionals`](crate::sampling::sample_by_conditionals).
    ///
    /// # Errors
    ///
    /// Refused ([`checks::sample`]) with [`Refusal::SiteOutOfRange`] or
    /// [`Refusal::RepeatedSite`], with any number of shots, zero included.
    /// An engine that draws from its conditional probabilities also passes
    /// on their refusal, [`Refusal::ZeroProbabilityOutcome`], for example
    /// on a state whose norm is zero or NaN. Fails as every method may, for
    /// example when the values do not fit in memory.
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
    /// gate grow with these bonds. The largest
    /// ([`MpsShape::largest_bond_dimension`]) is the cost a method reports.
    /// An engine that truncates holds at most the capacity's bonds, even
    /// after a gate whose result does not fit them ([`apply`](Self::apply)).
    ///
    /// It says how the engine behaves, not how entangled the state is: a
    /// bond is not the state's exact Schmidt rank at its cut. After gates
    /// alone, an engine whose truncation drops only the singular values at
    /// or below a cutoff holds exactly the state's Schmidt values above
    /// that cutoff at each cut, up to the capacity's bond. An engine whose
    /// cutoff bounds the discarded weight instead may drop larger values.
    /// After a measurement ([`project`](Self::project)), an engine may keep
    /// a larger bond when it does not compress again (its choice, for
    /// speed); two engines may then report different chains for one
    /// program.
    ///
    /// # Errors
    ///
    /// Fails as every method may; refuses nothing.
    fn shape(&self, state: &Self::State) -> Result<MpsShape, Self::Error>;

    /// The weight the engine's truncations have kept: the product of the
    /// fractions of the squared norm each truncation kept. An engine may
    /// keep it as the norm ⟨ψ|ψ⟩ of a state it never renormalizes, or
    /// renormalize after each truncation and track the product apart;
    /// either way every read is of the normalized state.
    ///
    /// It is 1 after [`prepare`](Self::prepare). Up to rounding and the
    /// distance from unitarity that [`apply`](Self::apply) tolerates, which
    /// adds up over gates, it never increases (a truncation lowers it,
    /// gates and projections keep it), and on an engine that never
    /// truncates (the reference) it stays 1. It stays above zero, because
    /// callers divide by it: an engine keeps at least the largest Schmidt
    /// value of each cut it truncates, whatever its cutoff. A bond's size
    /// does not say whether its cut truncated; the weight says what the
    /// truncations cost in total. `docs/design.md` ("What the retained
    /// weight bounds") says what it bounds: the fidelity, and a readout's
    /// distance from the exact one.
    ///
    /// Like the other reads, it first applies the operators pending since
    /// the last read, so on an engine that truncates when it computes,
    /// reading it may truncate.
    ///
    /// # Errors
    ///
    /// Fails as every method may; refuses nothing.
    fn retained_weight(&self, state: &Self::State) -> Result<f64, Self::Error>;
}
