// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsEngine`]: what a library that holds matrix product states does for
//! a simulator, in tensor terms only.
//!
//! The trait sits below every simulator protocol: no instructions, shots,
//! RNG objects or ISAs, only sites, matrices, probabilities and seeds. A
//! simulator's terminal turns its gates into matrices and draws its own
//! randomness; the engine evolves, reads and samples the state. This keeps
//! the engine usable outside any simulator, and keeps the protocol free of
//! tensors.
//!
//! ```text
//! terminal (a simulator)                    engine (this trait)
//! gate on qubits a, b ─► matrix ─────────► apply(state, [a, b], M)
//! measurement ────────► P(One) ──────────► probability_of_one(state, a)
//!     draws with its own RNG ─► value ───► project(state, a, value, P(value))
//! a group of shots splits ───────────────► fork(state)
//! terminal measurements, N shots, seed ──► sample(state, sites, N, seed)
//! state query ⟨O⟩ ───────────────────────► expectation(state, O)
//! cost, capacity check ──────────────────► shape(state) ─► MpsShape
//! ```
//!
//! Sites are qubits: an engine prepares one two-level site per qubit, and
//! site `k` is qubit `k`.

use crate::{MpsShape, PauliSum};
use num_complex::Complex64;

/// A matrix product state engine.
///
/// Every method takes `&self`, so one engine serves many states (a group of
/// shots each, Q8), and every method may fail, because a state may live on
/// a device that runs out of memory. The read methods take the state by
/// shared reference: an engine that evaluates lazily keeps its pending work
/// behind interior mutability.
///
/// The conformance tests (`tensornet::conformance`, feature `conformance`)
/// check an engine against [`ExactReference`](crate::reference::ExactReference),
/// the exact answer every engine with enough bond must give.
pub trait MpsEngine {
    /// One matrix product state the engine holds. Dropping it releases it.
    type State;

    /// Why an operation failed.
    type Error: std::error::Error + 'static;

    /// The state |0…0⟩ on `capacity.site_count()` sites, one qubit each,
    /// whose bonds never exceed `capacity`'s. An engine that truncates keeps
    /// within it; an engine that does not (the reference) reports, through
    /// [`shape`](Self::shape), a chain that does not fit, for the caller to
    /// check. `Err` when the engine cannot hold such a state, or when a site
    /// of `capacity` is not two-level.
    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, Self::Error>;

    /// Applies the unitary `matrix` to `sites`. The matrix is row-major,
    /// `M[out][in]`, over the sites' joint index with `sites[0]` the most
    /// significant bit: so `CX(control, target)` on `[control, target]` is
    /// the textbook matrix. This is cuTensorNet's layout for an operator
    /// tensor on its modes in order (④'s `fixture_operator`), so no engine
    /// reorders a matrix the terminal built. The sites are distinct and the
    /// matrix has `4^sites.len()` elements; an engine may rely on the
    /// matrix being unitary (cuTensorNet's `unitary` flag).
    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), Self::Error>;

    /// P(One) for a measurement in Z of `site`, without changing the state.
    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, Self::Error>;

    /// P(One) for a measurement in Z of `site`, given that each site of
    /// `given` was measured with the value beside it, without changing the
    /// state. A terminal that samples N shots with each shot's own RNG
    /// asks for one such probability per distinct set of earlier values
    /// (Q8 (a)), so an engine that can contract with projected sites should
    /// do so rather than copy the state. The default copies and projects,
    /// as a split would. `Err` when `given` has probability zero.
    fn conditional_probability_of_one(
        &self,
        state: &Self::State,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, Self::Error> {
        let mut copy = self.fork(state)?;
        for &(at, one) in given {
            let p1 = self.probability_of_one(&copy, at)?;
            self.project(&mut copy, at, one, if one { p1 } else { 1.0 - p1 })?;
        }
        self.probability_of_one(&copy, site)
    }

    /// Projects `site` on `one` and renormalizes. `probability` is the
    /// probability of that value, as the caller read it from
    /// [`probability_of_one`](Self::probability_of_one) on this state: the
    /// caller has always just read it to draw the value, and passing it
    /// spares an engine a second contraction (④'s `apply_projection` takes
    /// the mass the same way). `Err` when `probability` is not in (0, 1],
    /// to rounding.
    fn project(
        &self,
        state: &mut Self::State,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), Self::Error>;

    /// An independent copy of `state`, for the shots that drew another
    /// value. Its cost depends on the engine (D21: on cuTensorNet it is open).
    ///
    /// A copy up to the engine's truncation: an engine that truncates (an
    /// MPS of bond χ below the exact Schmidt rank) may build the copy along
    /// another path than `state` took, and each path truncates differently,
    /// so their values may differ within the truncation error. Without
    /// truncation they agree to rounding. A caller that needs the copy to
    /// equal the state value for value under truncation cannot rely on
    /// `fork` (`qdk_cutensornet`'s fork is such a case).
    fn fork(&self, state: &Self::State) -> Result<Self::State, Self::Error>;

    /// Draws a value for each of `sites` (distinct), for `shots` shots, from
    /// one seed, without changing the state: `values[s * sites.len() + t]`
    /// is shot `s`'s value of `sites[t]`. All shots draw from the one seed,
    /// so a shot's values depend on the others drawn with it; this is the
    /// shape of cuTensorNet's sampler (Q8 (b)). The same seed gives the same
    /// values on the same engine; engines need not agree with one another
    /// value for value, only in distribution.
    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, Self::Error>;

    /// ⟨ψ|O|ψ⟩ of the normalized state. Every site of `observable` is a site
    /// of the state.
    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, Self::Error>;

    /// The chain as realized now: one site per qubit, in order, with the
    /// bond the engine uses at each cut (D22). The largest bond is the cost
    /// a method reports.
    fn shape(&self, state: &Self::State) -> Result<MpsShape, Self::Error>;
}

/// The seed of one [`MpsEngine::sample`] call: an integer in 1..=2³¹−1.
///
/// The range is cuTensorNet's: its sampler takes a positive 32-bit seed
/// (`CUTENSORNET_SAMPLER_CONFIG_DETERMINISTIC`), so the type makes an
/// out-of-range seed unrepresentable rather than an error of one engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SampleSeed(u32);

impl SampleSeed {
    /// The largest seed, 2³¹ − 1.
    pub const MAX: u32 = 0x7FFF_FFFF;

    /// The seed, as the engine passes it on.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

/// A seed outside 1..=2³¹−1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a sample seed is in 1..=2147483647, not {0}")]
pub struct SeedOutOfRange(pub u64);

impl TryFrom<u64> for SampleSeed {
    type Error = SeedOutOfRange;
    fn try_from(seed: u64) -> Result<Self, SeedOutOfRange> {
        match u32::try_from(seed) {
            Ok(s @ 1..=Self::MAX) => Ok(Self(s)),
            _ => Err(SeedOutOfRange(seed)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_seed_is_a_positive_32_bit_integer() {
        assert_eq!(SampleSeed::try_from(0), Err(SeedOutOfRange(0)));
        assert_eq!(SampleSeed::try_from(1).map(SampleSeed::get), Ok(1));
        assert_eq!(
            SampleSeed::try_from(u64::from(SampleSeed::MAX)).map(SampleSeed::get),
            Ok(SampleSeed::MAX)
        );
        assert_eq!(SampleSeed::try_from(1 << 31), Err(SeedOutOfRange(1 << 31)));
    }
}
