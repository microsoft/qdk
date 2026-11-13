// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CpuMpsEngine`]: the engine's entry points. This file holds the public
//! engine, with the rules it promises beyond the contract, and the contract's
//! operations, each checking its arguments and calling the mechanism that
//! serves it, in its own file.
//!
//! A simulator runs each shot of a program through the contract: it prepares a
//! state, applies one- and two-site gates, possibly on qubits far apart in the
//! chain, and between them reads probabilities, projects measured values,
//! estimates observables and draws samples. The state is a matrix product state
//! in canonical form ([`chain`](crate::chain)): a chain of site tensors, one
//! per qubit, of which one, the orthogonality centre, holds the state's norm,
//! so that an operation needs only the sites at the centre.
//!
//! [`CpuMpsEngine`] is the engine callers use. [`Engine`] holds every rule,
//! written once over the crate-private [`LinearAlgebra`], and [`CpuMpsEngine`]
//! is that engine on [`CpuLinearAlgebra`]; the crate's tests run the same rules
//! on algebras that count or fail its calls.
//!
//! A caller may rely on the contract; on the truncation, retained weight,
//! routing and fusion stated on [`CpuMpsEngine`]; and on a local cost. A gate
//! costs what its sites, the bonds there and the distance the centre travels
//! cost, not the chain's length. A read of a qubit moves the centre to its
//! site, and an expectation contracts the sites between the observable's sites
//! and the centre. A conditional read with conditions, and each new nonempty
//! prefix of a sample, also copies the chain (a fork).
//!
//! Each operation keeps the canonical form and moves the centre only as far as
//! it must, by the mechanism of its file:
//!
//! - [`canonical_form`]: moving the centre, one QR per site crossed;
//! - a one-site gate multiplies its site's tensor by the gate on the qubit's
//!   value, which keeps the form wherever the centre is (`apply`, here);
//! - [`gates`]: a gate on two neighbours contracts them into one block and
//!   applies to it, and a gate on sites further apart first brings them
//!   together by SWAPs, then moves them back (routing); with fusion, the gates
//!   that follow on one pair apply to its block before it is split;
//! - [`split`]: splitting a block back into two sites by one SVD, the one place
//!   where the engine truncates;
//! - [`reads`]: probabilities, projections and expectations;
//! - here, `sample`, which draws by `tensornet`'s shared sampler from the
//!   conditional probabilities of [`reads`], and `fork`, `shape` and
//!   `retained_weight`, which split a pending block, then copy or read the
//!   chain.

mod canonical_form;
mod dev_mpo;
mod gates;
mod reads;
mod split;

use crate::{
    CpuMpsError,
    chain::{CanonicalMps, Chain},
    linear_algebra::{CpuLinearAlgebra, LinearAlgebra},
};
use num_complex::Complex64;
use std::cell::RefCell;
use tensornet::{MpsEngine, MpsShape, PauliSum, checks, sampling};

/// The cutoff of [`CpuMpsEngine::default`]: a split discards the smallest
/// singular values whose squares sum to at most this fraction of the sum of
/// all their squares.
///
/// The value of quimb and tensor4all; see [`CpuMpsEngine::with_cutoff`] for
/// what a smaller or larger one trades.
pub const DEFAULT_CUTOFF: f64 = 1e-14;

/// A matrix product state engine on the CPU: the [`MpsEngine`] a simulator
/// runs a program on when it holds the state as a chain of site tensors
/// truncated to a max bond dimension. The state, its bonds, the mixed
/// canonical form and the orthogonality centre (the centre, below) are
/// described on [`CanonicalMps`].
///
/// Beyond the contract, a caller may rely on how it truncates: it drops the
/// tail of singular values below its cutoff at every split, and
/// truncates each split at rest to the capacity's bond at that cut (the `χ`
/// of the chain a caller asks for in [`MpsEngine::prepare`]); a route's
/// outbound SWAPs apply the cutoff alone (see Routing below). It
/// renormalizes at each split that discards a value, and tracks the
/// retained weight apart: the split scales the values it keeps so that they
/// hold the squared norm of the whole block, and multiplies the state's
/// retained weight by the fraction they held, which
/// [`MpsEngine::retained_weight`] reports. The fraction and the cutoff's
/// tail are computed on the values divided by the largest, so that their
/// sums of squares do not underflow (a defence: the block a split factors
/// has a norm of about 1). A projection divides the site by the norm of what
/// it kept, computed the same way, so the state's norm is 1 however few
/// digits the probability holds, and the retained weight is unchanged. A
/// gate keeps the norm up to rounding and the contract's unitarity
/// tolerance ([`checks::UNITARITY_TOLERANCE`]); the reads divide by the
/// squared norm at the centre, so that drift does not reach them. A split
/// that would take the retained weight below the smallest normal `f64` fails
/// with [`CpuMpsError::WeightUnderflow`]: the contract keeps the weight above
/// zero, because callers divide by it.
///
/// **Routing (`swap-back-2x`).** A gate on sites `low < high` that are not
/// neighbours moves the site at `high` down to `low + 1` by adjacent SWAPs
/// (outbound), applies the gate to the pair at `low`, and moves the site
/// back up by SWAPs (return). So the chain is in the caller's order after
/// every call, and no read and no shape shows the move. The split of
/// an outbound SWAP applies the cutoff alone, no capacity: after the SWAP
/// that moves the site down across cut `c`, the sites left of cut `c` are
/// the caller's sites left of cut `c − 1` and the moved site, so its bond is
/// at most twice the bond the chain holds at cut `c − 1` (the moved site
/// brings two levels), itself at most the capacity there. The gate's and
/// the return's splits keep at most the capacity's bond at their own cut,
/// the caller's cuts, where the state must fit the capacity anyway. So a
/// route discards nothing beyond what the cutoff discards at its splits
/// (negligible at [`DEFAULT_CUTOFF`]) and what the capacity cuts at the
/// gate's and the return's. The price is transient: during a route a bond
/// reaches up to twice the capacity's largest bond `χ`, so a site tensor up
/// to 4× the memory of one at `χ`, and
/// [`CanonicalMps::split_statistics`] reports it apart from the bonds at
/// rest. Other routings are not built; the crate's `docs/routing.md` lists
/// those that could be explored, with what is known of each.
///
/// **Fusion.** The engine fuses gates into a two-site block before its
/// split: a gate on two neighbours leaves their contracted pair unsplit, and
/// the gates that follow on the same two sites, one-site gates on either of
/// them and routing SWAPs included, apply to the pair, until another
/// operation (a two-site gate on another pair, or any read or fork) splits
/// the pair by one SVD. Fusion saves the splits of gates that follow one
/// another on a pair, and splits a pair toward where the next operation
/// needs the centre. It moves where the truncations fall, so a truncated
/// run's values may differ from those of an unfused run, one split per
/// gate, within the truncation error; without truncation they agree to
/// rounding. Fusion is always on; the crate's tests run the engine unfused
/// too, to compare.
///
/// **Failures.** After a failure the state is unspecified, as [`MpsEngine`]
/// says, and the caller must drop it: a split that fails
/// ([`CpuMpsError::Factorization`], [`CpuMpsError::WeightUnderflow`]) may
/// lose the gates fused into its pair, and a route that fails midway leaves
/// its site moved.
#[derive(Clone, Debug)]
pub struct CpuMpsEngine(Engine<CpuLinearAlgebra>);

/// The engine with the cutoff [`DEFAULT_CUTOFF`], fusing.
impl Default for CpuMpsEngine {
    fn default() -> Self {
        Self(Engine::new(CpuLinearAlgebra))
    }
}

impl CpuMpsEngine {
    /// An engine whose splits discard the smallest singular values whose
    /// squares sum to at most `cutoff` times the sum of all their squares
    /// (quimb's `cutoff_mode="rsum2"`), then, at rest, keep at most the
    /// capacity's bond (a route's outbound SWAPs apply the cutoff alone; see
    /// Routing on [`CpuMpsEngine`]); fusing. A cutoff of zero discards only
    /// zeros. A split always keeps its largest value, however close to 1 the
    /// cutoff.
    ///
    /// The cutoff and the capacity's bond `χ` are the two settings of the
    /// method that trade time for accuracy: a rough estimate is fast, and a
    /// better one is had by asking for it. The cutoff bounds the weight a
    /// split may discard, so it keeps a bond as small as the state allows;
    /// `χ` bounds the bond, whatever the weight it discards. Whichever binds
    /// first sets a split's bond, and the bonds set the time. The weight the
    /// splits discard adds up over the splits of a run, so a run of `N` splits
    /// discards up to about `N · cutoff` of the state's weight, plus whatever
    /// `χ` cuts; [`MpsEngine::retained_weight`] reports what they kept.
    ///
    /// On a chemistry circuit of tens of qubits the cutoff, not `χ`, keeps
    /// the bonds small. A cutoff of zero keeps every value the SVD returns,
    /// those just above rounding included, which grows the bonds and the
    /// time: on the Ising quench of 16 qubits at depth 20 and `χ` 256
    /// (`examples/sparse_comparison.rs`, `--family ising --n 16 --depth 20
    /// --cutoff 0 --only mps`), the largest bond at rest measured 254
    /// against 138 at [`DEFAULT_CUTOFF`], and the run about 2.4 times as
    /// long.
    ///
    /// # Panics
    ///
    /// When `cutoff` is not in `[0, 1)`, NaN included: it is a fraction of
    /// a split's weight, and a cutoff of 1 or more would discard every
    /// value.
    #[must_use]
    pub fn with_cutoff(cutoff: f64) -> Self {
        Self(Engine::with_cutoff(CpuLinearAlgebra, cutoff))
    }

    /// The same engine, fusing when `fuses`, as Fusion on [`CpuMpsEngine`]
    /// describes. The crate's users always get fusion; `with_fusion(false)`
    /// gives the unfused splits, one per gate, which the crate's tests
    /// compare with.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_fusion(self, fuses: bool) -> Self {
        Self(self.0.with_fusion(fuses))
    }

    /// EXPERIMENTAL, development only (not for review): applies `matrix`
    /// (row-major, `sites[0]` the most significant bit) to `sites`
    /// (ascending, at least two, possibly far apart) as a matrix product
    /// operator across their span, compressed by one QR and one SVD sweep,
    /// instead of routing it by SWAPs.
    ///
    /// # Errors
    ///
    /// As a split may.
    ///
    /// # Panics
    ///
    /// When `sites` are not ascending or `matrix` is not dense on them.
    pub fn dev_apply_operator(
        &self,
        state: &mut CanonicalMps,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CpuMpsError> {
        let chain = state.chain.get_mut();
        self.0.dev_apply_operator(chain, sites, matrix)
    }
}

impl MpsEngine for CpuMpsEngine {
    type State = CanonicalMps;
    type Error = CpuMpsError;

    fn prepare(&self, capacity: &MpsShape) -> Result<CanonicalMps, CpuMpsError> {
        self.0.prepare(capacity)
    }

    /// Fails as a split may (see Failures on [`CpuMpsEngine`]).
    fn apply(
        &self,
        state: &mut CanonicalMps,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CpuMpsError> {
        self.0.apply(state, sites, matrix)
    }

    fn probability_of_one(&self, state: &CanonicalMps, site: usize) -> Result<f64, CpuMpsError> {
        self.0.probability_of_one(state, site)
    }

    /// Reads each condition's probability from its value's own weight, not
    /// as one minus the other's, so a rare value whose complement rounds to
    /// one is not refused.
    fn conditional_probability_of_one(
        &self,
        state: &CanonicalMps,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, CpuMpsError> {
        self.0.conditional_probability_of_one(state, site, given)
    }

    /// Reads `p` from the value's own weight at the centre site, not as one
    /// minus the other's.
    ///
    /// Drops no singular value of its own: a projection does not recompress
    /// the chain, so the bonds [`MpsEngine::shape`] reports after it may
    /// exceed the state's Schmidt ranks. It moves the centre to the site by a
    /// thin QR per site crossed, which bounds each bond it crosses by twice
    /// the bond on the crossed site's other side.
    /// Recompressing the chain around the site by SVDs, to drop the
    /// singular values the projection zeroed, was slower on every run
    /// measured (see the crate's `README.md`).
    fn project(
        &self,
        state: &mut CanonicalMps,
        site: usize,
        one: bool,
    ) -> Result<f64, CpuMpsError> {
        self.0.project(state, site, one)
    }

    /// Splits a contracted pair first, so the forks do not each split it.
    fn fork(&self, state: &CanonicalMps) -> Result<CanonicalMps, CpuMpsError> {
        self.0.fork(state)
    }

    /// Draws by `tensornet`'s shared sampler,
    /// [`sample_by_conditionals`](tensornet::sampling::sample_by_conditionals):
    /// each shot's values in order, One when a uniform from the
    /// `SplitMix64` stream seeded with `seed` is
    /// below P(One | the shot's earlier values). Shots that drew the same
    /// values so far share that probability, so it is computed once per
    /// distinct prefix; each new nonempty prefix projects its values on a
    /// fork, a copy of the chain, so the state is not projected. A value
    /// of probability exactly zero is never drawn: the other value's
    /// probability is then exactly one. Fails with
    /// [`CpuMpsError::TooManyValues`] when the values cannot be held
    /// (`shots × sites.len()` overflows a `usize`, or the allocation fails),
    /// and returns at once when there are none.
    fn sample(
        &self,
        state: &CanonicalMps,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, CpuMpsError> {
        self.0.sample(state, sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &CanonicalMps,
        observable: &PauliSum,
    ) -> Result<Complex64, CpuMpsError> {
        self.0.expectation(state, observable)
    }

    fn shape(&self, state: &CanonicalMps) -> Result<MpsShape, CpuMpsError> {
        self.0.shape(state)
    }

    fn retained_weight(&self, state: &CanonicalMps) -> Result<f64, CpuMpsError> {
        self.0.retained_weight(state)
    }
}

/// The engine of [`CpuMpsEngine`], written once over the [`LinearAlgebra`]
/// `A`, which supplies its products, QR and SVD decompositions: every rule
/// is in its methods, here and in this module's files, and
/// [`CpuMpsEngine`] states them and forwards to this engine on
/// [`CpuLinearAlgebra`]. It is generic so that the crate's tests can run
/// the same rules on algebras that count the engine's QRs and products, or
/// fail its SVDs.
#[derive(Clone, Debug)]
pub(crate) struct Engine<A> {
    algebra: A,
    cutoff: f64,
    fuses: bool,
}

impl<A: LinearAlgebra> Engine<A> {
    /// An engine on `algebra` with the cutoff [`DEFAULT_CUTOFF`], fusing.
    pub(crate) fn new(algebra: A) -> Self {
        Self::with_cutoff(algebra, DEFAULT_CUTOFF)
    }

    /// An engine on `algebra` with `cutoff`, fusing (see
    /// [`CpuMpsEngine::with_cutoff`]).
    ///
    /// # Panics
    ///
    /// When `cutoff` is not in `[0, 1)`, NaN included.
    pub(crate) fn with_cutoff(algebra: A, cutoff: f64) -> Self {
        assert!(
            (0.0..1.0).contains(&cutoff),
            "a cutoff is a fraction of a split's weight, in [0, 1); got {cutoff}"
        );
        Self {
            algebra,
            cutoff,
            fuses: true,
        }
    }

    /// The same engine, fusing when `fuses` (see
    /// [`CpuMpsEngine::with_fusion`]).
    #[cfg(test)]
    pub(crate) fn with_fusion(self, fuses: bool) -> Self {
        Self { fuses, ..self }
    }
}

/// The operations of [`CpuMpsEngine`], which documents them, on any
/// algebra.
impl<A: LinearAlgebra> MpsEngine for Engine<A> {
    type State = CanonicalMps;
    type Error = CpuMpsError;

    fn prepare(&self, capacity: &MpsShape) -> Result<CanonicalMps, CpuMpsError> {
        checks::prepare(capacity)?;
        let capacity_bonds = (0..capacity.site_count() - 1)
            .map(|cut| capacity.bond_dim(cut).expect("a cut of the capacity"))
            .collect();
        Ok(CanonicalMps {
            chain: RefCell::new(Chain::zero(capacity_bonds)),
        })
    }

    fn apply(
        &self,
        state: &mut CanonicalMps,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CpuMpsError> {
        let chain = state.chain.get_mut();
        checks::apply(chain.sites.len(), sites, matrix)?;
        match *sites {
            [site] => {
                chain.apply_one(site, matrix);
                Ok(())
            }
            [first, second] => self.apply_pair(chain, first, second, matrix),
            _ => unreachable!("`checks::apply` refuses an operator on no site or more than two"),
        }
    }

    fn probability_of_one(&self, state: &CanonicalMps, site: usize) -> Result<f64, CpuMpsError> {
        let mut chain = state.chain.borrow_mut();
        checks::probability_of_one(chain.sites.len(), site)?;
        Ok(self.probabilities(&mut chain, site)?[1])
    }

    fn conditional_probability_of_one(
        &self,
        state: &CanonicalMps,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, CpuMpsError> {
        let site_count = state.chain.borrow().sites.len();
        checks::conditional_probability_of_one(site_count, site, given)?;
        Ok(self.conditional_probabilities(state, site, given)?[1])
    }

    fn project(
        &self,
        state: &mut CanonicalMps,
        site: usize,
        one: bool,
    ) -> Result<f64, CpuMpsError> {
        let chain = state.chain.get_mut();
        checks::project(chain.sites.len(), site)?;
        self.project_on(chain, site, one)
    }

    fn fork(&self, state: &CanonicalMps) -> Result<CanonicalMps, CpuMpsError> {
        {
            let mut chain = state.chain.borrow_mut();
            let centre = chain.centre;
            self.split_pair_toward(&mut chain, centre)?;
        }
        Ok(state.clone())
    }

    fn sample(
        &self,
        state: &CanonicalMps,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, CpuMpsError> {
        checks::sample(state.chain.borrow().sites.len(), sites)?;
        sampling::sample_by_conditionals(sites, shots, seed, |site, given| {
            Ok(self.conditional_probabilities(state, site, given)?[1])
        })
    }

    fn expectation(
        &self,
        state: &CanonicalMps,
        observable: &PauliSum,
    ) -> Result<Complex64, CpuMpsError> {
        let mut chain = state.chain.borrow_mut();
        checks::expectation(chain.sites.len(), observable)?;
        self.expectation_of(&mut chain, observable)
    }

    fn shape(&self, state: &CanonicalMps) -> Result<MpsShape, CpuMpsError> {
        let mut chain = state.chain.borrow_mut();
        let centre = chain.centre;
        self.split_pair_toward(&mut chain, centre)?;
        let n = chain.sites.len();
        let sites = chain
            .sites
            .iter()
            .enumerate()
            .map(|(at, site)| {
                let left = (at > 0).then_some(site.left);
                let right = (at + 1 < n).then_some(site.right);
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect();
        Ok(MpsShape::new(sites).expect("a chain whose neighbours share each bond"))
    }

    fn retained_weight(&self, state: &CanonicalMps) -> Result<f64, CpuMpsError> {
        let mut chain = state.chain.borrow_mut();
        let centre = chain.centre;
        self.split_pair_toward(&mut chain, centre)?;
        Ok(chain.retained_weight)
    }
}
