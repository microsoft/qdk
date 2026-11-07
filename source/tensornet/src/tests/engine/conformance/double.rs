// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Double`], an engine built on the reference that truncates as an MPS
//! engine does, with one [`Defect`] or none.

use super::REFERENCE;
use super::defect::{Defect, LOOSE_UNITARITY, Method, QUARTER_UNITARITY, TIGHT_UNITARITY};
use crate::conformance::{condition_probability, normalized, truncated};
use crate::reference::{ReferenceError, ReferenceState};
use crate::sampling::splitmix64::SplitMix64;
use crate::tests::c;
use crate::{MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal, checks};
use num_complex::Complex64;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The error of [`Double`]: the reference's, or a refusal it hides or
/// rewords.
#[derive(Debug, thiserror::Error)]
pub(super) enum DoubleError {
    #[error(transparent)]
    Reference(#[from] ReferenceError),
    #[error("{0}")]
    Opaque(String),
    #[error("refused: {0}")]
    Reworded(Refusal),
    #[error("{shown}")]
    Mislabelled { shown: Refusal, exposed: Refusal },
}

impl From<Refusal> for DoubleError {
    fn from(refusal: Refusal) -> Self {
        Self::Reference(refusal.into())
    }
}

impl MpsEngineError for DoubleError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Reference(error) => error.refusal(),
            Self::Opaque(_) => None,
            Self::Reworded(refusal) => Some(refusal),
            Self::Mislabelled { exposed, .. } => Some(exposed),
        }
    }
}

/// A state of [`Double`]: the reference's, truncated after each operator
/// at every cut whose Schmidt rank exceeds its capacity's bond; with what
/// the defects that cache or read stale states keep.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a defect's own record"
)]
pub(super) struct Held {
    reference: ReferenceState,
    capacity: MpsShape,
    /// Whether the capacity caps a bond below what the state may need.
    capped: bool,
    /// Whether a lone site is held as two.
    padded: bool,
    /// The state as it was before the last truncation.
    before_truncation: Option<ReferenceState>,
    /// The state as it was at the last projection.
    at_projection: Option<ReferenceState>,
    /// The state as the last two-site truncation left it.
    at_two_site_truncation: Option<ReferenceState>,
    /// The chain first reported.
    first_shape: Option<MpsShape>,
    /// Whether the state was projected.
    projected: bool,
    lineage: Lineage,
    /// The weight the first truncation that lost weight kept.
    first_weight: Option<f64>,
    /// The sum of the fractions of the weight the truncations lost.
    lost: f64,
    /// How many truncations lost weight.
    losses: u32,
    /// Whether the last operation was a projection.
    just_projected: bool,
    /// The product of the fractions the truncations at cut 0 kept.
    first_cut_weight: f64,
    /// Whether the last gate acted on distant sites and left a Schmidt rank
    /// above a bond above one.
    widened_from_afar: bool,
    /// What every operation refreshes, shared with forks by the defects
    /// that share it.
    caches: Rc<RefCell<Caches>>,
    /// The bond held at each cut by an engine of lazy bonds: the rank the
    /// last operator across the cut left, capped.
    bonds: Vec<usize>,
}

/// The chain and the weight of a [`Held`] state after its last operation.
#[derive(Clone, Debug, Default)]
struct Caches {
    shape: Option<MpsShape>,
    weight: Option<f64>,
}

/// Where a [`Held`] state came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lineage {
    Prepared,
    /// A fork no operation changed yet.
    FreshFork,
    Fork,
}

impl Held {
    /// Records an operation on the state.
    fn operated(&mut self) {
        if self.lineage == Lineage::FreshFork {
            self.lineage = Lineage::Fork;
        }
    }

    /// Refreshes the caches after an operation.
    fn refresh(&self) {
        *self.caches.borrow_mut() = Caches {
            shape: REFERENCE.shape(&self.reference).ok(),
            weight: REFERENCE.retained_weight(&self.reference).ok(),
        };
    }
}

/// `state` with every amplitude multiplied by `factor`.
fn scaled(state: &ReferenceState, factor: f64) -> ReferenceState {
    let amplitudes: Vec<Complex64> = (0..1 << state.site_count())
        .map(|i| state.amplitude(i) * factor)
        .collect();
    ReferenceState::from_amplitudes(&amplitudes)
}

/// ⟨ψ|ψ⟩ of `state`. The reference's reads divide by it, so a read times
/// it is the read of `state` as it is held, divided by nothing.
fn norm_of(state: &ReferenceState) -> f64 {
    REFERENCE
        .retained_weight(state)
        .expect("the reference reads its weight")
}

/// An engine built on the reference that truncates as an MPS engine does,
/// at every cut whose Schmidt rank exceeds its capacity's bond, keeping the
/// largest Schmidt pairs without renormalizing them, so it holds the
/// retained weight as the norm of its state (one way to hold it); with a
/// [`Defect`] or none.
pub(super) struct Double {
    pub(super) defect: Option<Defect>,
    /// Whether it sets a bond only when an operator acts across its cut.
    pub(super) lazy_bonds: bool,
    /// What its factorizations leave on an outcome that is impossible in
    /// exact arithmetic.
    pub(super) rounding: Rounding,
    /// The share of a cut's weight its own cutoff may drop after every
    /// gate, as the CPU engine's default of 1e-14 does. Zero for none.
    pub(super) cutoff: f64,
    /// What [`Defect::LeaksConditionalPath`] and its variants leak, either
    /// sign.
    pub(super) leak: f64,
    pub(super) sample_calls: Cell<u32>,
}

/// What an engine's factorizations leave on an outcome (a value or a
/// condition) that is impossible in exact arithmetic.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Rounding {
    /// What the reference leaves: exactly zero, or, after a truncation,
    /// its own rounding.
    AsTheReference,
    /// Once the state was projected, `probability` on a condition the
    /// reference refuses, which it answers (with 1/2), and on such a value
    /// too when `in_projections`.
    Leaves {
        probability: f64,
        in_projections: bool,
    },
    /// As `Leaves`, but only on a condition of two values or more, as an
    /// engine whose rounding grows with the sites a condition sweeps: a
    /// condition of one value it computes as exactly zero, and refuses, and
    /// so do its projections.
    LeavesOnLongerConditions { probability: f64 },
    /// Exactly zero, so a refusal, where the reference's own rounding
    /// leaves a probability below 1e-30 after a truncation that lost
    /// weight (a probability that small on a state no truncation touched,
    /// such as 1e-200, is exact, and answered).
    ZeroesTheReferences,
}

impl Double {
    pub(super) fn new(defect: Option<Defect>) -> Self {
        Self {
            defect,
            lazy_bonds: false,
            rounding: Rounding::AsTheReference,
            cutoff: 0.0,
            leak: 1e-10,
            sample_calls: Cell::new(0),
        }
    }

    /// The routing defects of a gate on sites `first` and `second`, in the
    /// caller's order, not neighbours, after the gate:
    /// [`Defect::RoutesEntanglingGatesAtTheBond`],
    /// [`Defect::RoutesThroughTheNarrowCut`], the same in one order only,
    /// [`Defect::RoutesWithPhasedSwaps`] and [`Defect::LeaksRouting`].
    fn route(&self, held: &mut Held, first: usize, second: usize, matrix: &[Complex64]) {
        let (low, high) = (first.min(second), first.max(second));
        if self.has(Defect::RoutesEntanglingGatesAtTheBond) && !one_site_gates(matrix) {
            route_at_the_bond(held, low, high);
        }
        if self.has(Defect::RoutesThroughTheNarrowCut)
            || first < second && self.has(Defect::RoutesUpThroughTheNarrowCut)
            || first > second && self.has(Defect::RoutesDownThroughTheNarrowCut)
        {
            route_through_the_narrow_cut(held, low, high);
        }
        if self.has(Defect::RoutesWithPhasedSwaps) {
            for between in low + 1..high {
                phase_flip(&mut held.reference, between);
            }
        }
        if self.has(Defect::LeaksRouting) {
            let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
            let swap = [l, o, o, o, o, o, l, o, o, l, o, o, o, o, o, l];
            REFERENCE
                .apply(&mut held.reference, &[low + 1, high], &swap)
                .expect("a swap of two sites the state has");
        }
    }

    /// A conforming engine whose factorizations, once a projection acted,
    /// leave a condition, and a value when `in_projections`, that is
    /// impossible in exact arithmetic the probability `rounding`, as an fp64
    /// engine that moves its orthogonality centre through a projected site
    /// does.
    pub(super) fn rounding(probability: f64, in_projections: bool) -> Self {
        Self {
            rounding: Rounding::Leaves {
                probability,
                in_projections,
            },
            ..Self::new(None)
        }
    }

    /// A conforming engine that computes exactly zero where the reference's
    /// own rounding leaves a probability below 1e-30, as an engine whose
    /// factorizations round differently may.
    pub(super) fn zeroing_rounding() -> Self {
        Self {
            rounding: Rounding::ZeroesTheReferences,
            ..Self::new(None)
        }
    }

    /// A conforming engine like [`Double::rounding`], whose projections
    /// refuse an impossible value, but which leaves `probability` only on
    /// a condition of two values or more, and refuses one of one value
    /// ([`Rounding::LeavesOnLongerConditions`]).
    pub(super) fn rounding_on_longer_conditions(probability: f64) -> Self {
        Self {
            rounding: Rounding::LeavesOnLongerConditions { probability },
            ..Self::new(None)
        }
    }

    /// Whether this engine answers `given` on `held`, a condition the
    /// reference refuses, as rounding left it a probability.
    fn answers_impossible(&self, held: &Held, given: &[(usize, bool)]) -> bool {
        self.rounding_left(held).is_some()
            && (given.len() >= 2
                || !matches!(self.rounding, Rounding::LeavesOnLongerConditions { .. }))
    }

    /// The values of `given` this engine reads at `site` when a defect
    /// skips some of them (those out of its order), or `None` when it
    /// reads them all: only where the reference answers.
    fn skipped_condition(
        &self,
        held: &Held,
        site: usize,
        given: &[(usize, bool)],
    ) -> Option<Vec<(usize, bool)>> {
        let skips = self.has(Defect::SkipsUnsortedCondition)
            || self.has(Defect::SkipsConditionAgainstItsSweep)
            || self.has(Defect::SkipsDecreasingCondition)
                && given.windows(2).all(|pair| pair[1].0 < pair[0].0);
        if !skips
            || REFERENCE
                .conditional_probability_of_one(&held.reference, site, given)
                .is_err()
        {
            return None;
        }
        let up = !self.has(Defect::SkipsConditionAgainstItsSweep)
            || !matches!(given, [(first, _), (second, _), ..] if second < first);
        let mut last = None;
        Some(
            given
                .iter()
                .copied()
                .filter(|&(at, _)| {
                    let sorted = last.is_none_or(|last| (at > last) == up);
                    last = if sorted { Some(at) } else { last };
                    sorted
                })
                .collect(),
        )
    }

    /// Whether this engine computes exactly zero for `given` on `held`,
    /// which the reference's rounding left a tiny probability.
    fn zeroes(&self, held: &Held, given: &[(usize, bool)]) -> bool {
        self.rounding == Rounding::ZeroesTheReferences && held.losses > 0 && {
            let p = summed_probability(&held.reference, given);
            p > 0.0 && p < 1e-30
        }
    }

    /// The probability rounding leaves on an impossible outcome of `held`,
    /// if any.
    fn rounding_left(&self, held: &Held) -> Option<f64> {
        if self.has(Defect::RoundsUnprojectedStates)
            || held.projected && self.has(Defect::FailsRoundedProjections)
        {
            Some(1e-32)
        } else if let Rounding::Leaves { probability, .. }
        | Rounding::LeavesOnLongerConditions { probability } = self.rounding
            && held.projected
        {
            Some(probability)
        } else {
            None
        }
    }

    /// A conforming engine that drops, after every gate, the smallest
    /// Schmidt pairs at each cut while their weight stays within `cutoff`
    /// of the cut's.
    pub(super) fn with_cutoff(cutoff: f64) -> Self {
        Self {
            cutoff,
            ..Self::new(None)
        }
    }

    /// A conforming engine that sets a bond only when an operator acts
    /// across its cut, as an engine that keeps its chain in canonical form
    /// around each gate does: after a truncation or a projection, a bond
    /// elsewhere may exceed the state's Schmidt rank there.
    pub(super) fn lazy_bonds() -> Self {
        Self {
            lazy_bonds: true,
            ..Self::new(None)
        }
    }

    fn has(&self, defect: Defect) -> bool {
        self.defect == Some(defect)
    }

    /// Whether this engine refuses the padding's site of `held` in
    /// `method`: [`Defect::TakesPaddingOnlyIn`] another method.
    fn refuses_padding(&self, held: &Held, method: Method) -> bool {
        held.padded
            && matches!(self.defect, Some(Defect::TakesPaddingOnlyIn(only)) if only != method)
    }

    /// The reference's result, with a refusal hidden or reworded by the
    /// defect.
    fn report<T>(&self, result: Result<T, ReferenceError>) -> Result<T, DoubleError> {
        result.map_err(|error| match (error.refusal(), self.defect) {
            (Some(refusal @ &Refusal::SiteOutOfRange { site, .. }), Some(Defect::WrongRefusal)) => {
                DoubleError::Mislabelled {
                    shown: refusal.clone(),
                    exposed: Refusal::RepeatedSite { site },
                }
            }
            (Some(refusal), Some(Defect::RefusalAsFailure)) => {
                DoubleError::Opaque(refusal.to_string())
            }
            (Some(refusal), Some(Defect::RewordsRefusal)) => DoubleError::Reworded(refusal.clone()),
            _ => error.into(),
        })
    }

    /// The state reads are answered from: the held one, or a stale one.
    fn read<'h>(&self, held: &'h Held) -> &'h ReferenceState {
        let stale = match self.defect {
            Some(Defect::ReadsBeforeTruncation) => held.before_truncation.as_ref(),
            Some(Defect::CachesReadsAcrossGates) => held.at_projection.as_ref(),
            Some(Defect::CachesReadsAtTruncation) => held.at_two_site_truncation.as_ref(),
            _ => None,
        };
        stale.unwrap_or(&held.reference)
    }

    /// What reads of `read`, the state `held` answers them from, divide by:
    /// the weight the truncations kept, which is the norm of `read`.
    fn weight(&self, held: &Held, read: &ReferenceState) -> f64 {
        match (self.defect, held.first_weight) {
            (Some(Defect::UnnormalizedReads), _) => 1.0,
            (Some(Defect::NormalizesTwoSitesOnly), _) if read.site_count() != 2 => 1.0,
            (Some(Defect::ReadsByFirstWeight), Some(first)) => first,
            _ => norm_of(read),
        }
    }

    /// The probability below which the double refuses an outcome as if it
    /// were zero, with the threshold defect `at_1e_12` or `at_epsilon_squared`
    /// (of `project`, or of conditions).
    fn refusal_threshold(&self, at_1e_12: Defect, at_epsilon_squared: Defect) -> Option<f64> {
        if self.has(at_1e_12) {
            Some(1e-12)
        } else if self.has(at_epsilon_squared) {
            Some(f64::EPSILON * f64::EPSILON)
        } else {
            None
        }
    }

    /// The seed the sampler draws with: `seed`, or another.
    fn draw_seed(&self, seed: u64) -> u64 {
        let calls = self.sample_calls.get() + 1;
        self.sample_calls.set(calls);
        match self.defect {
            Some(Defect::IgnoresSeed) => 1,
            Some(Defect::ReseedsEachCall) => seed ^ u64::from(calls) << 32,
            Some(Defect::SeedLow32) => seed & 0xFFFF_FFFF,
            Some(Defect::SeedParityOnly) => seed & 1,
            Some(Defect::SeedDropsLowBit) => seed >> 1,
            Some(Defect::SeedDropsTopBit) => seed & !(1 << 63),
            Some(Defect::SeedThroughF64) => through_f64(seed),
            _ => seed,
        }
    }

    /// What [`MpsEngine::sample`] draws from the held `reference`, with the
    /// sampling defects.
    fn sample_held(
        &self,
        reference: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, DoubleError> {
        let n = sites.len();
        let values = match self.defect {
            Some(Defect::DrawsOneShotTooFew) => {
                self.report(REFERENCE.sample(reference, sites, shots.saturating_sub(1), seed))?
            }
            Some(Defect::DrawsEachShotTwice) => {
                let half =
                    self.report(REFERENCE.sample(reference, sites, shots.div_ceil(2), seed))?;
                let mut values = Vec::with_capacity(shots * n);
                for shot in half.chunks_exact(n.max(1)) {
                    values.extend_from_slice(shot);
                    values.extend_from_slice(shot);
                }
                values.truncate(shots * n);
                values
            }
            Some(Defect::FillsEmptySample) if sites.is_empty() => vec![false; shots],
            Some(Defect::SamplesRealParts) => {
                let real: Vec<Complex64> = (0..1usize << reference.site_count())
                    .map(|index| Complex64::new(reference.amplitude(index).re, 0.0))
                    .collect();
                let norm = real.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
                let real: Vec<Complex64> = real.iter().map(|amplitude| amplitude / norm).collect();
                let real = ReferenceState::from_amplitudes(&real);
                self.report(REFERENCE.sample(&real, sites, shots, seed))?
            }
            Some(
                Defect::SampleUnnormalized
                | Defect::SampleBiasedSmall
                | Defect::SampleBiasedEven
                | Defect::SampleDropsRare
                | Defect::SampleAntithetic
                | Defect::SampleReusesPrefixDraws
                | Defect::SampleCarriesDraw,
            ) => self.sample_by_conditionals(reference, sites, shots, seed)?,
            Some(Defect::SampleRepeatsFirstHalf | Defect::SampleRepeatsFirstQuarterLast) => {
                let fresh = match self.defect {
                    Some(Defect::SampleRepeatsFirstHalf) => shots.div_ceil(2),
                    _ => shots - shots / 4,
                };
                let drawn = self.report(REFERENCE.sample(reference, sites, fresh, seed))?;
                let repeated = &drawn[..(shots - fresh) * n];
                drawn.iter().chain(repeated).copied().collect()
            }
            // The reference draws its shots in sequence from one stream, so
            // a chunk's shots are the first of every call with that seed.
            Some(Defect::RestartsStreamEvery(chunk)) => {
                let chunk =
                    self.report(REFERENCE.sample(reference, sites, chunk.min(shots), seed))?;
                chunk.iter().cycle().copied().take(shots * n).collect()
            }
            Some(Defect::SamplesSitesApart) => {
                let mut apart = Vec::new();
                for (k, &site) in sites.iter().enumerate() {
                    let k = u64::try_from(k).expect("the kit samples few sites");
                    let seed = seed.wrapping_add(k);
                    apart.push(self.report(REFERENCE.sample(reference, &[site], shots, seed))?);
                }
                (0..shots * n).map(|at| apart[at % n][at / n]).collect()
            }
            _ => self.report(REFERENCE.sample(reference, sites, shots, seed))?,
        };
        Ok(match self.defect {
            Some(Defect::SamplesSiteMajor) => (0..values.len())
                .map(|at| values[(at % shots) * n + at / shots])
                .collect(),
            Some(Defect::InvertsSamples) => values.iter().map(|&one| !one).collect(),
            Some(Defect::BreaksOneShot) if !values.is_empty() => {
                let mut broken = values.clone();
                broken[0] = !broken[0];
                broken
            }
            Some(Defect::InvertsLoneSiteSamples) if reference.site_count() == 1 => {
                values.iter().map(|&one| !one).collect()
            }
            Some(Defect::SkewsSamples) => values
                .chunks_exact(n.max(1))
                .enumerate()
                .flat_map(|(shot, values)| values.iter().map(move |&one| one && shot % 2 == 0))
                .collect(),
            Some(Defect::SampleCopiesOneShotInFive) if n >= 1 => {
                let mut copied = values.clone();
                for shot in (1..shots).step_by(5) {
                    copied.copy_within((shot - 1) * n..shot * n, shot * n);
                }
                copied
            }
            Some(Defect::CorrelatesSamples) if n >= 2 => {
                let mut correlated = values.clone();
                let mut before = false;
                for shot in correlated.chunks_exact_mut(n) {
                    let last = shot[n - 1];
                    shot[n - 1] = shot[0] && (last || before);
                    before = last;
                }
                correlated
            }
            _ => values,
        })
    }

    /// Draws site by site, as an engine that samples from conditional
    /// probabilities, with a xorshift generator, and the sampling defects
    /// that change a probability or a draw.
    fn sample_by_conditionals(
        &self,
        reference: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, DoubleError> {
        self.report(checks::sample(reference.site_count(), sites).map_err(Into::into))?;
        // The seed's first `SplitMix64` value, so that every bit of the seed
        // matters, and at least 1, so that the xorshift state is never 0.
        let mut x = SplitMix64::new(seed).next_u64().max(1);
        let mut uniform = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            #[expect(clippy::cast_precision_loss, reason = "53 bits")]
            let u = (x >> 11) as f64 / (1u64 << 53) as f64;
            u
        };
        let mut known: BTreeMap<Vec<(usize, bool)>, f64> = BTreeMap::new();
        let mut values: Vec<bool> = Vec::with_capacity(shots * sites.len());
        let mut draws: Vec<f64> = Vec::new();
        for shot in 0..shots {
            let mut prefix: Vec<(usize, bool)> = Vec::with_capacity(sites.len());
            let mut this_shot = Vec::with_capacity(sites.len());
            for (t, &site) in sites.iter().enumerate() {
                if t == 1
                    && shot > 0
                    && self.has(Defect::SampleReusesPrefixDraws)
                    && prefix[0].1 == values[values.len() - sites.len() - 1]
                {
                    let before = values.len() - sites.len();
                    let rest = values[before..values.len() - 1].to_vec();
                    values.extend(rest);
                    break;
                }
                let p1 = if let Some(&p1) = known.get(&prefix) {
                    p1
                } else {
                    let p1 = if t == 0 && self.has(Defect::SampleUnnormalized) {
                        self.report(REFERENCE.probability_of_one(reference, site))?
                            * norm_of(reference)
                    } else {
                        self.report(
                            REFERENCE.conditional_probability_of_one(reference, site, &prefix),
                        )?
                    };
                    known.insert(prefix.clone(), p1);
                    p1
                };
                let p1 = match self.defect {
                    Some(Defect::SampleBiasedSmall) if p1 > 0.0 && p1 < 1.0 => (p1 + 0.02).min(1.0),
                    Some(Defect::SampleBiasedEven) if p1 > 0.3 && p1 < 0.7 => p1 + 0.02,
                    Some(Defect::SampleDropsRare) if p1 < 0.05 => 0.0,
                    Some(Defect::SampleDropsRare) if p1 > 0.95 => 1.0,
                    _ => p1,
                };
                let u = if self.has(Defect::SampleAntithetic) && shot % 2 == 1 {
                    1.0 - draws[t]
                } else if self.has(Defect::SampleCarriesDraw) && t == 0 && shot > 0 {
                    draws[sites.len() - 1]
                } else {
                    uniform()
                };
                this_shot.push(u);
                let one = u < p1;
                values.push(one);
                prefix.push((site, one));
            }
            draws = this_shot;
        }
        Ok(values)
    }

    /// Truncates `held` after a gate on `sites`, `distant` when on two
    /// sites not neighbours, as the truncation and routing defects do.
    fn truncate_after_gate(
        &self,
        held: &mut Held,
        sites: &[usize],
        distant: bool,
    ) -> Result<(), DoubleError> {
        let width = sites.len();
        let phased = self.has(Defect::PhasesANarrowCutTruncation)
            && held.capped
            && !distant
            && width == 2
            && narrow_cut_overflows(held, sites[0].min(sites[1]));
        let above_bond_one = held.capped && exceeds_a_bond_above_one(held);
        held.widened_from_afar = distant && above_bond_one;
        if distant && above_bond_one && self.has(Defect::FailsDistantTruncationAboveBondOne) {
            return Err(DoubleError::Opaque("no route for this bond".into()));
        }
        let skipped = distant && self.has(Defect::SkipsDistantTruncation)
            || above_bond_one && distant && self.has(Defect::SkipsDistantTruncationAboveBondOne);
        if held.capped && !skipped {
            let before = REFERENCE
                .retained_weight(&held.reference)
                .expect("the reference reads its weight");
            self.truncate(held, width);
            if distant
                && above_bond_one
                && self.has(Defect::RenormalizesDistantTruncationAboveBondOne)
            {
                let after = REFERENCE
                    .retained_weight(&held.reference)
                    .expect("the reference reads its weight");
                held.reference = scaled(&held.reference, (before / after).sqrt());
            }
        }
        if phased {
            phase_flip(&mut held.reference, sites[0].max(sites[1]));
        }
        Ok(())
    }

    /// Truncates `held` at every cut whose Schmidt rank exceeds its
    /// capacity's bond, after an operator on `width` sites, and records
    /// what the truncations kept.
    fn truncate(&self, held: &mut Held, width: usize) {
        let weight = |state: &ReferenceState| {
            REFERENCE
                .retained_weight(state)
                .expect("the reference reads its weight")
        };
        let before = held.reference.clone();
        let n = held.reference.site_count();
        let cuts = if self.has(Defect::TruncatesFirstCutOnly) {
            1
        } else {
            n.saturating_sub(1)
        };
        for cut in 0..cuts {
            let ranks = REFERENCE.shape(&held.reference).expect("a chain of qubits");
            let (Some(rank), Some(bond)) = (ranks.bond_dim(cut), held.capacity.bond_dim(cut))
            else {
                continue;
            };
            let keep = bond + usize::from(self.has(Defect::KeepsChiPlusOne));
            if rank <= keep {
                continue;
            }
            // At a tie the kit reports its own state; the double keeps it.
            let Ok(kept) = truncated(&held.reference, cut, keep) else {
                continue;
            };
            let fraction = weight(&kept) / weight(&held.reference);
            held.lost += 1.0 - fraction;
            if cut == 0 {
                held.first_cut_weight *= fraction;
            }
            if fraction < 1.0 - 1e-9 {
                held.losses += 1;
                if held.first_weight.is_none() {
                    held.first_weight = Some(weight(&kept));
                }
            }
            held.reference = kept;
        }
        held.before_truncation = Some(before);
        if width == 2 {
            held.at_two_site_truncation = Some(held.reference.clone());
        }
    }

    /// Whether a fork of `held` holds `before`, the state before the last
    /// gate's truncation: [`Defect::ForkUndoesATruncationAboveBondOne`] and
    /// [`Defect::ForkUndoesANarrowCutTruncation`].
    fn fork_undoes(&self, held: &Held, before: &ReferenceState) -> bool {
        let ranks = |state| REFERENCE.shape(state).expect("a chain of qubits");
        let (was, is) = (ranks(before), ranks(&held.reference));
        (0..is.site_count().saturating_sub(1))
            .filter(|&cut| was.bond_dim(cut) > is.bond_dim(cut))
            .filter_map(|cut| held.capacity.bond_dim(cut))
            .any(|bond| {
                bond > 1 && self.has(Defect::ForkUndoesATruncationAboveBondOne)
                    || bond < held.capacity.largest_bond()
                        && self.has(Defect::ForkUndoesANarrowCutTruncation)
            })
    }

    /// The failure or refusal of a gate by the defects that depend on its
    /// number of sites, before the Double applies it.
    fn apply_arity_defects(
        &self,
        held: &Held,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), DoubleError> {
        if self.has(Defect::FailsThreeSites) && sites.len() > 2 {
            return Err(DoubleError::Opaque(
                "gates on more than two sites are not supported".to_string(),
            ));
        }
        let method = if sites.len() == 1 {
            Method::OneSiteApply
        } else {
            Method::TwoSiteApply
        };
        if self.refuses_padding(held, method) {
            self.report(checks::apply(1, sites, matrix).map_err(Into::into))?;
        }
        if self.has(Defect::RefusesThreeSitesAsMatrixLength) && sites.len() == 3 {
            return Err(Refusal::MatrixLength {
                sites: 3,
                expected: 4,
                actual: matrix.len(),
            }
            .into());
        }
        Ok(())
    }

    /// The tolerance a unitarity defect checks against, if any.
    fn unitarity_tolerance(&self) -> Option<f64> {
        match self.defect {
            Some(Defect::RefusesRoundedUnitary) => Some(TIGHT_UNITARITY),
            Some(Defect::RefusesNearUnitary) => Some(QUARTER_UNITARITY),
            Some(Defect::AcceptsLooseUnitary) => Some(LOOSE_UNITARITY),
            _ => None,
        }
    }
}

/// Whether `held` has a Schmidt rank above its capacity's bond at a cut
/// whose bond is above one.
fn exceeds_a_bond_above_one(held: &Held) -> bool {
    let ranks = REFERENCE.shape(&held.reference).expect("a chain of qubits");
    (0..ranks.site_count().saturating_sub(1)).any(|cut| {
        matches!(
            (ranks.bond_dim(cut), held.capacity.bond_dim(cut)),
            (Some(rank), Some(bond)) if bond > 1 && rank > bond
        )
    })
}

/// Swaps site `low` up to the neighbour of `high` and back, capping each
/// cut it crosses at twice that cut's bond:
/// [`Defect::RoutesThroughTheNarrowCut`].
fn route_through_the_narrow_cut(held: &mut Held, low: usize, high: usize) {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    let swap = [l, o, o, o, o, o, l, o, o, l, o, o, o, o, o, l];
    let cuts: Vec<usize> = (low..high - 1).chain((low..high - 1).rev()).collect();
    for cut in cuts {
        REFERENCE
            .apply(&mut held.reference, &[cut, cut + 1], &swap)
            .expect("a swap of two sites the state has");
        let ranks = REFERENCE.shape(&held.reference).expect("a chain of qubits");
        if let (Some(rank), Some(bond)) = (ranks.bond_dim(cut), held.capacity.bond_dim(cut))
            && rank > 2 * bond
            && let Ok(kept) = truncated(&held.reference, cut, 2 * bond)
        {
            held.reference = kept;
        }
    }
}

/// Swaps site `low` up to the neighbour of `high` and back, capping each
/// cut it crosses, and the cut above it, at the capacity's bond there:
/// [`Defect::RoutesEntanglingGatesAtTheBond`].
fn route_at_the_bond(held: &mut Held, low: usize, high: usize) {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    let swap = [l, o, o, o, o, o, l, o, o, l, o, o, o, o, o, l];
    let cuts: Vec<usize> = (low..high - 1).chain((low..high - 1).rev()).collect();
    for cut in cuts {
        REFERENCE
            .apply(&mut held.reference, &[cut, cut + 1], &swap)
            .expect("a swap of two sites the state has");
        for capped in [cut + 1, cut] {
            let ranks = REFERENCE.shape(&held.reference).expect("a chain of qubits");
            if let (Some(rank), Some(bond)) =
                (ranks.bond_dim(capped), held.capacity.bond_dim(capped))
                && rank > bond
                && let Ok(kept) = truncated(&held.reference, capped, bond)
            {
                held.reference = kept;
            }
        }
    }
}

/// Whether the two-site `matrix` is a product of one-site gates: its
/// realignment `R[(r₀c₀), (r₁c₁)] = M[(r₀r₁), (c₀c₁)]` has rank one, so
/// every 2×2 minor of it vanishes.
fn one_site_gates(matrix: &[Complex64]) -> bool {
    let r = |i: usize, j: usize| matrix[((i >> 1) * 2 + (j >> 1)) * 4 + (i & 1) * 2 + (j & 1)];
    (0..4).all(|i| {
        (0..4).all(|j| {
            (0..4).all(|k| (0..4).all(|l| (r(i, j) * r(k, l) - r(i, l) * r(k, j)).norm() < 1e-12))
        })
    })
}

/// `seed` through an `f64`: [`Defect::SeedThroughF64`].
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the defect is the loss"
)]
fn through_f64(seed: u64) -> u64 {
    (seed as f64) as u64
}

/// Whether `given` holds Zero values only, each on a site whose P(One) on
/// `reference` is zero to rounding, as a projection on Zero leaves it:
/// [`Defect::LeaksConditionalPathGivenCertainZeros`].
fn only_certain_zeros(reference: &ReferenceState, given: &[(usize, bool)]) -> bool {
    !given.is_empty()
        && given.iter().all(|&(at, one)| {
            !one && REFERENCE
                .conditional_probability_of_one(reference, at, &[])
                .is_ok_and(|p| p < 1e-15)
        })
}

fn flip_site_zero(reference: &mut ReferenceState) {
    REFERENCE
        .apply(reference, &[0], &Pauli::X.matrix())
        .expect("every state has site 0");
}

/// Drops, at every cut, the smallest Schmidt values while the sum of
/// their squares stays within `cutoff` of the cut's weight: 1e-8 for
/// [`Defect::CoarseCutoff`], the engine's own cutoff for [`Double::cutoff`].
fn cut_off(reference: &mut ReferenceState, cutoff: f64) {
    let weight = |state: &ReferenceState| {
        REFERENCE
            .retained_weight(state)
            .expect("the reference reads its weight")
    };
    for cut in 0..reference.site_count().saturating_sub(1) {
        let ranks = REFERENCE.shape(reference).expect("a chain of qubits");
        let rank = ranks.bond_dim(cut).unwrap_or(1);
        // Counted down from the rank read once, so that a reference whose
        // ranks are wrong fails the tests rather than hangs them.
        for keep in (1..rank).rev() {
            match truncated(reference, cut, keep) {
                Ok(kept) if weight(&kept) >= (1.0 - cutoff) * weight(reference) => {
                    *reference = kept;
                }
                _ => break,
            }
        }
    }
}

/// The probability of the condition `given` in `reference`, summed over
/// the basis states that meet it, not from projections as the kit computes
/// it.
fn summed_probability(reference: &ReferenceState, given: &[(usize, bool)]) -> f64 {
    let indices = 0..1usize << reference.site_count();
    let weight: f64 = indices
        .clone()
        .map(|index| reference.amplitude(index).norm_sqr())
        .sum();
    let met: f64 = indices
        .filter(|index| given.iter().all(|&(at, one)| (index >> at & 1 == 1) == one))
        .map(|index| reference.amplitude(index).norm_sqr())
        .sum();
    met / weight
}

/// Whether the state's Schmidt rank at `cut` exceeds the capacity's bond
/// there, which is narrower than another cut's.
fn narrow_cut_overflows(held: &Held, cut: usize) -> bool {
    let bond = held.capacity.bond_dim(cut).expect("a cut the chain has");
    let rank = REFERENCE
        .shape(&held.reference)
        .expect("a chain of qubits")
        .bond_dim(cut)
        .expect("a cut the chain has");
    bond < held.capacity.largest_bond() && rank > bond
}

/// Applies Z, a phase of -1 on One, to `site`.
fn phase_flip(reference: &mut ReferenceState, site: usize) {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    REFERENCE
        .apply(reference, &[site], &[l, o, o, -l])
        .expect("a site the state has");
}

/// Applies S, a phase of i on One, to `site`.
fn phase_site(reference: &mut ReferenceState, site: usize) {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    REFERENCE
        .apply(reference, &[site], &[l, o, o, c(0.0, 1.0)])
        .expect("a site the state has");
}

/// `observable` with each term's sites put in increasing order and its
/// Paulis paired with them as `defect` pairs them: in the order given
/// (`SortsSitesNotPaulis`), reversed when the first site is the highest
/// (`MixedOrderPairing`), or rotated as the factors would be to put the
/// lowest site first (`RotationPairing`).
fn paired_on_sorted_sites(observable: &PauliSum, defect: Defect) -> PauliSum {
    let terms = observable
        .terms()
        .iter()
        .map(|term| {
            let factors = term.factors();
            let mut sites: Vec<usize> = factors.iter().map(|&(site, _)| site).collect();
            sites.sort_unstable();
            let mut paulis: Vec<Pauli> = factors.iter().map(|&(_, pauli)| pauli).collect();
            match defect {
                Defect::MixedOrderPairing
                    if factors.first().map(|f| f.0) > factors.last().map(|f| f.0) =>
                {
                    paulis.reverse();
                }
                Defect::RotationPairing => {
                    let lowest = (0..factors.len())
                        .min_by_key(|&at| factors[at].0)
                        .unwrap_or(0);
                    paulis.rotate_left(lowest);
                }
                _ => {}
            }
            PauliTerm::new(term.coefficient(), sites.into_iter().zip(paulis).collect())
        })
        .collect::<Result<_, _>>()
        .expect("the same sites, the same coefficients");
    PauliSum::new(observable.identity_coefficient(), terms).expect("the identity part is finite")
}

impl MpsEngine for Double {
    type State = Rc<RefCell<Held>>;
    type Error = DoubleError;

    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, DoubleError> {
        let sites = capacity.site_count();
        let padded = sites == 1
            && matches!(
                self.defect,
                Some(Defect::PadsLoneSite | Defect::HidesPadding | Defect::TakesPaddingOnlyIn(_))
            );
        let qubits = (0..sites).all(|site| capacity.physical_dim(site) == Some(2));
        let reference = if padded || (!qubits && self.has(Defect::AcceptsQutrits)) {
            let chain = MpsShape::qubit_chain(sites.max(2), usize::MAX).expect("a chain of qubits");
            self.report(REFERENCE.prepare(&chain))?
        } else {
            self.report(REFERENCE.prepare(capacity))?
        };
        // Whether a cut's bond is below the largest Schmidt rank a chain of
        // qubits can reach there.
        let capped = (0..sites.saturating_sub(1)).any(|cut| {
            let qubits = u32::try_from((cut + 1).min(sites - cut - 1)).unwrap_or(u32::MAX);
            let largest = 1_usize.checked_shl(qubits).unwrap_or(usize::MAX);
            capacity.bond_dim(cut).is_some_and(|bond| bond < largest)
        });
        let largest = (0..sites.saturating_sub(1))
            .filter_map(|cut| capacity.bond_dim(cut))
            .max();
        let capacity = match largest {
            Some(bond) if qubits && self.has(Defect::IgnoresNarrowCuts) => {
                MpsShape::qubit_chain(sites, bond).expect("a chain of qubits")
            }
            _ => capacity.clone(),
        };
        Ok(Rc::new(RefCell::new(Held {
            reference,
            capacity,
            capped,
            padded,
            before_truncation: None,
            at_projection: None,
            at_two_site_truncation: None,
            first_shape: None,
            projected: false,
            lineage: Lineage::Prepared,
            first_weight: None,
            lost: 0.0,
            losses: 0,
            just_projected: false,
            first_cut_weight: 1.0,
            widened_from_afar: false,
            caches: Rc::default(),
            bonds: vec![1; sites.saturating_sub(1)],
        })))
    }

    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), DoubleError> {
        let held = &mut *state.borrow_mut();
        held.operated();
        held.just_projected = false;
        self.apply_arity_defects(held, sites, matrix)?;
        if self.has(Defect::AcceptsInfinity)
            && matrix
                .iter()
                .any(|entry| entry.re.is_infinite() || entry.im.is_infinite())
        {
            return Ok(());
        }
        if let Some(tolerance) = self.unitarity_tolerance()
            && matches!(
                checks::apply(held.reference.site_count(), sites, matrix),
                Ok(()) | Err(Refusal::NotUnitary { .. })
            )
        {
            let deviation = checks::unitarity_deviation(matrix, 1 << sites.len());
            if deviation > tolerance {
                return Err(Refusal::NotUnitary { deviation }.into());
            }
            if deviation > checks::UNITARITY_TOLERANCE {
                return Ok(());
            }
        }
        let swapped;
        let sites = match *sites {
            [a, b] if self.has(Defect::SwapsSites) => {
                swapped = [b, a];
                &swapped[..]
            }
            _ => sites,
        };
        let applied = REFERENCE.apply(&mut held.reference, sites, matrix);
        if let Err(error) = applied {
            if self.has(Defect::ChangesStateWhenRefusing) {
                let (s, k) = 0.5f64.sin_cos();
                let ry = [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)];
                REFERENCE
                    .apply(&mut held.reference, &[0], &ry)
                    .expect("every state has site 0");
            }
            if (self.has(Defect::AcceptsNonUnitary)
                || self.has(Defect::AcceptsNonUnitaryOnTwoSites) && sites.len() == 2)
                && matches!(error.refusal(), Some(Refusal::NotUnitary { .. }))
            {
                return Ok(());
            }
            return self.report(Err(error));
        }
        let distant = matches!(*sites, [a, b] if a.abs_diff(b) > 1);
        if let &[a, b] = sites
            && distant
        {
            self.route(held, a, b, matrix);
        }
        self.truncate_after_gate(held, sites, distant)?;
        if self.has(Defect::CoarseCutoff) {
            cut_off(&mut held.reference, 1e-8);
        } else if self.cutoff > 0.0 {
            cut_off(&mut held.reference, self.cutoff);
        }
        let ranks = REFERENCE.shape(&held.reference).expect("a chain of qubits");
        let (low, high) = (sites.iter().min(), sites.iter().max());
        if let (Some(&low), Some(&high)) = (low, high) {
            for cut in low..high.min(held.bonds.len()) {
                held.bonds[cut] = ranks.bond_dim(cut).unwrap_or(1);
            }
        }
        held.refresh();
        Ok(())
    }

    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, DoubleError> {
        let held = &mut *state.borrow_mut();
        if self.refuses_padding(held, Method::ProbabilityOfOne) {
            self.report(checks::probability_of_one(1, site).map_err(Into::into))?;
        }
        let read = self.read(held);
        let p1 = self.report(REFERENCE.probability_of_one(read, site))? * norm_of(read)
            / self.weight(held, read);
        if self.has(Defect::ReadsNanProbability) && site == 1 {
            return Ok(f64::NAN);
        }
        if self.has(Defect::ReadingLeavesPhase) {
            phase_site(&mut held.reference, site);
        }
        if self.has(Defect::ReadsInSinglePrecision) {
            #[expect(clippy::cast_possible_truncation, reason = "the defect")]
            return Ok(f64::from(p1 as f32));
        }
        Ok(p1)
    }

    fn conditional_probability_of_one(
        &self,
        state: &Self::State,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, DoubleError> {
        let held = state.borrow();
        let first_values;
        let given = if self.has(Defect::ConditionRefusesOnlyAdjacentRepeats)
            && given.windows(2).all(|pair| pair[0].0 != pair[1].0)
        {
            first_values = first_occurrences(given, |&(at, _)| at);
            &first_values[..]
        } else {
            given
        };
        if self.zeroes(&held, given) {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        let skipped = self.skipped_condition(&held, site, given);
        let given = skipped.as_deref().unwrap_or(given);
        let answer = REFERENCE.conditional_probability_of_one(&held.reference, site, given);
        if self.has(Defect::IgnoresCondition) && answer.is_ok() {
            drop(held);
            return self.probability_of_one(state, site);
        }
        if self.has(Defect::ConditionalLeavesPhase) && answer.is_ok() {
            drop(held);
            phase_site(&mut state.borrow_mut().reference, site);
            return self.report(answer);
        }
        match answer {
            Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
                if self.has(Defect::AcceptsZeroCondition)
                    || self.answers_impossible(&held, given) =>
            {
                Ok(0.5)
            }
            Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
                if self.has(Defect::AcceptsUnsortedZeroCondition)
                    && given.windows(2).any(|pair| pair[1].0 < pair[0].0) =>
            {
                Ok(0.5)
            }
            Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
                if self.has(Defect::AcceptsSortedZeroCondition)
                    && given.windows(2).all(|pair| pair[0].0 < pair[1].0) =>
            {
                Ok(0.5)
            }
            Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
                if self.has(Defect::ProjectsStateOnZeroCondition) =>
            {
                drop(held);
                let (at, one) = given[0];
                // Refused, leaving the state, when that value alone has
                // probability zero.
                let _ = REFERENCE.project(&mut state.borrow_mut().reference, at, one);
                Err(Refusal::ZeroProbabilityOutcome.into())
            }
            Ok(_)
                if self
                    .refusal_threshold(
                        Defect::ConditionalThreshold,
                        Defect::ConditionalThresholdEpsilonSquared,
                    )
                    .is_some_and(|threshold| {
                        condition_probability(&held.reference, given) < threshold
                    }) =>
            {
                Err(Refusal::ZeroProbabilityOutcome.into())
            }
            Ok(p1) if self.has(Defect::ConditionalDividedByWeight) => {
                Ok(p1 / self.weight(&held, &held.reference))
            }
            Ok(p1)
                if self.has(Defect::LeaksConditionalPathOnZeroSteps)
                    && held.projected
                    && p1 > 1.0 - 1e-15 =>
            {
                Ok(1.0 - self.leak)
            }
            Ok(p1)
                if (self.has(Defect::LeaksConditionalPath)
                    || self.has(Defect::LeaksConditionalPathGivenCertainZeros)
                        && only_certain_zeros(&held.reference, given))
                    && held.projected
                    && !(1e-30..=1.0 - 1e-15).contains(&p1) =>
            {
                Ok(if p1 < 0.5 { self.leak } else { 1.0 - self.leak })
            }
            result => self.report(result),
        }
    }

    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, DoubleError> {
        let held = &mut *state.borrow_mut();
        if self.refuses_padding(held, Method::Project) {
            self.report(checks::project(1, site).map_err(Into::into))?;
        }
        held.operated();
        if held.projected && self.has(Defect::SkipsLaterProjections) {
            let mut untouched = held.reference.clone();
            return self.report(REFERENCE.project(&mut untouched, site, one));
        }
        let one = one != self.has(Defect::ProjectsOtherValue);
        if self.zeroes(held, &[(site, one)]) {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        if let Some(threshold) = self.refusal_threshold(
            Defect::ProjectThreshold,
            Defect::ProjectThresholdEpsilonSquared,
        ) && REFERENCE
            .project(&mut held.reference.clone(), site, one)
            .is_ok_and(|p| p < threshold)
        {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        if self.has(Defect::SkipsTinyProjection)
            && let Ok(p) = REFERENCE.project(&mut held.reference.clone(), site, one)
            && p < 1e-12
        {
            return Ok(p);
        }
        let p = match REFERENCE.project(&mut held.reference, site, one) {
            Ok(p) => p,
            Err(error) => {
                if error.refusal() == Some(&Refusal::ZeroProbabilityOutcome) {
                    if self.has(Defect::AcceptsZeroOutcome) {
                        return Ok(0.0);
                    }
                    if self.rounding_left(held).is_some()
                        && self.has(Defect::FailsRoundedProjections)
                    {
                        return Err(DoubleError::Opaque("a device error".to_string()));
                    }
                    if let Some(rounding) = self.rounding_left(held)
                        && (matches!(
                            self.rounding,
                            Rounding::Leaves {
                                in_projections: true,
                                ..
                            }
                        ) || self.has(Defect::RoundsUnprojectedStates))
                    {
                        // A meaningless state, left as it was.
                        return Ok(rounding);
                    }
                    if self.has(Defect::ProjectsStateOnZeroOutcome) {
                        flip_site_zero(&mut held.reference);
                    }
                }
                return self.report(Err(error));
            }
        };
        let p = if self.has(Defect::LiftsRoundingOutcomes) && p < 1e-30 {
            1e-10
        } else {
            p
        };
        held.projected = true;
        held.just_projected = true;
        if held.capped && self.has(Defect::KeepsProjectionNorm) {
            held.reference = scaled(&held.reference, p.sqrt());
        }
        if self.has(Defect::RenormalizesProjection) {
            held.reference = normalized(&held.reference);
        }
        held.at_projection = Some(held.reference.clone());
        held.refresh();
        Ok(match self.defect {
            Some(Defect::ReturnsOtherProbability) => 1.0 - p,
            Some(Defect::ReturnsUnnormalizedProbability) => {
                p * REFERENCE
                    .retained_weight(&held.reference)
                    .expect("the reference reads its weight")
            }
            _ => p,
        })
    }

    fn fork(&self, state: &Self::State) -> Result<Self::State, DoubleError> {
        if self.has(Defect::ForkShares)
            || (self.has(Defect::ForkOfForkShares) && state.borrow().lineage != Lineage::Prepared)
        {
            return Ok(Rc::clone(state));
        }
        let mut copy = state.borrow().clone();
        if !self.has(Defect::ForkSharesShape)
            && !self.has(Defect::ForkSharesWeight)
            && !self.has(Defect::ForkReadsSharedWeight)
            && !self.has(Defect::OriginalReadsSharedWeight)
        {
            let caches = copy.caches.borrow().clone();
            copy.caches = Rc::new(RefCell::new(caches));
        }
        if self.has(Defect::ForkDiffers)
            || copy.losses >= 1 && self.has(Defect::ForkOfTruncatedDiffers)
        {
            flip_site_zero(&mut copy.reference);
        }
        if copy.losses >= 1 && self.has(Defect::ForkOfTruncatedFlipsPhase) {
            let z = [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(-1.0, 0.0)];
            REFERENCE
                .apply(&mut copy.reference, &[1], &z)
                .expect("every state of this defect has site 1");
        }
        if let Some(before) = &copy.before_truncation
            && self.fork_undoes(&copy, before)
        {
            copy.reference = before.clone();
        }
        if self.has(Defect::ForkResetsWeight) {
            let weight = REFERENCE
                .retained_weight(&copy.reference)
                .expect("the reference reads its weight");
            copy.reference = scaled(&copy.reference, weight.sqrt().recip());
        }
        copy.lineage = Lineage::FreshFork;
        Ok(Rc::new(RefCell::new(copy)))
    }

    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, DoubleError> {
        if self.refuses_padding(&state.borrow(), Method::Sample) {
            self.report(checks::sample(1, sites).map_err(Into::into))?;
        }
        if shots == 0 && self.has(Defect::SampleZeroShotsUnchecked) {
            return Ok(Vec::new());
        }
        if shots == 0 && self.has(Defect::SampleZeroShotsChecksRangeOnly) {
            let held = state.borrow().reference.site_count();
            if let Some(&site) = sites.iter().find(|&&site| site >= held) {
                return Err(Refusal::SiteOutOfRange { site, sites: held }.into());
            }
            return Ok(Vec::new());
        }
        if (self.has(Defect::NarrowsSeed) && seed > 0x7FFF_FFFF)
            || (self.has(Defect::RefusesSeedZero) && seed == 0)
        {
            return Err(DoubleError::Opaque(format!(
                "the seed {seed} is out of range"
            )));
        }
        let seed = self.draw_seed(seed);
        if self.has(Defect::SampleRefusesOnlyAdjacentRepeats)
            && sites.windows(2).all(|pair| pair[0] != pair[1])
        {
            let distinct = first_occurrences(sites, |&site| site);
            let drawn = self.sample_held(&state.borrow().reference, &distinct, shots, seed)?;
            return Ok(drawn
                .chunks_exact(distinct.len().max(1))
                .flat_map(|shot| {
                    sites.iter().map(|site| {
                        let at = distinct.iter().position(|d| d == site);
                        shot[at.expect("every site is among the distinct ones")]
                    })
                })
                .collect());
        }
        let values = self.sample_held(&state.borrow().reference, sites, shots, seed)?;
        if self.has(Defect::SamplingLeavesPhase) {
            phase_site(&mut state.borrow_mut().reference, 0);
        }
        Ok(values)
    }

    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, DoubleError> {
        let held = state.borrow();
        if self.refuses_padding(&held, Method::Expectation) {
            self.report(checks::expectation(1, observable).map_err(Into::into))?;
        }
        let read = self.read(&held);
        let changed;
        let observable = match self.defect {
            Some(Defect::ConjugatesCoefficients) => {
                let terms = observable
                    .terms()
                    .iter()
                    .map(|term| PauliTerm::new(term.coefficient().conj(), term.factors().to_vec()))
                    .collect::<Result<_, _>>()
                    .expect("a conjugate coefficient is finite");
                changed = PauliSum::new(observable.identity_coefficient().conj(), terms)
                    .expect("a conjugate coefficient is finite");
                &changed
            }
            Some(Defect::ReadsRealCoefficients) => {
                let terms = observable
                    .terms()
                    .iter()
                    .map(|term| {
                        PauliTerm::new(term.coefficient().re.into(), term.factors().to_vec())
                    })
                    .collect::<Result<_, _>>()
                    .expect("a real part is finite");
                changed = PauliSum::new(observable.identity_coefficient(), terms)
                    .expect("the identity part is finite");
                &changed
            }
            Some(Defect::ReadsFirstTermOnly) => {
                let terms = observable.terms().iter().take(1).cloned().collect();
                changed = PauliSum::new(observable.identity_coefficient(), terms)
                    .expect("the identity part is finite");
                &changed
            }
            Some(Defect::ReadsNanForSums) if observable.terms().len() > 1 => {
                return Ok(Complex64::new(f64::NAN, f64::NAN));
            }
            Some(Defect::ReadsIdentityAloneAsZero) if observable.terms().is_empty() => {
                return Ok(Complex64::new(0.0, 0.0));
            }
            Some(
                defect @ (Defect::MixedOrderPairing
                | Defect::RotationPairing
                | Defect::SortsSitesNotPaulis),
            ) => {
                changed = paired_on_sorted_sites(observable, defect);
                &changed
            }
            Some(Defect::ChecksFirstFactorOnly) => {
                let sites = read.site_count();
                let mut identity = observable.identity_coefficient();
                let mut terms = Vec::new();
                for (t, term) in observable.terms().iter().enumerate() {
                    let factors: Vec<_> = term
                        .factors()
                        .iter()
                        .enumerate()
                        .filter(|&(f, &(site, _))| t == 0 && f == 0 || site < sites)
                        .map(|(_, &factor)| factor)
                        .collect();
                    match PauliTerm::new(term.coefficient(), factors) {
                        Ok(kept) => terms.push(kept),
                        Err(_) => identity += term.coefficient(),
                    }
                }
                changed = PauliSum::new(identity, terms).expect("the coefficients are finite");
                &changed
            }
            Some(Defect::MergesRepeatedTerms) => {
                let mut terms: Vec<PauliTerm> = Vec::new();
                for term in observable.terms() {
                    if !terms.iter().any(|kept| kept.factors() == term.factors()) {
                        terms.push(term.clone());
                    }
                }
                changed = PauliSum::new(observable.identity_coefficient(), terms)
                    .expect("the identity part is finite");
                &changed
            }
            _ => observable,
        };
        let mut value = self.report(REFERENCE.expectation(read, observable))?;
        if let [term] = observable.terms()
            && self.has(Defect::RoundsLoneStrings)
            && term.factors().iter().any(|&(_, pauli)| pauli == Pauli::Y)
        {
            value += 1e-8;
        }
        let identity_coefficient = observable.identity_coefficient();
        let weight = if self.has(Defect::ExpectationByFirstCutWeight) {
            held.first_cut_weight
        } else {
            self.weight(&held, read)
        };
        Ok((value - identity_coefficient) * norm_of(read) / weight + identity_coefficient)
    }

    fn shape(&self, state: &Self::State) -> Result<MpsShape, DoubleError> {
        let held = &mut *state.borrow_mut();
        if held.padded && !self.has(Defect::PadsLoneSite) {
            return Ok(MpsShape::qubit_chain(1, 1).expect("a lone qubit is a chain"));
        }
        if held.lineage == Lineage::FreshFork && self.has(Defect::ForkReportsPreparedShape) {
            return Ok(MpsShape::qubit_chain(held.reference.site_count(), 1)
                .expect("a chain of the state's sites"));
        }
        if self.has(Defect::ForkSharesShape)
            && let Some(shape) = held.caches.borrow().shape.clone()
        {
            return Ok(shape);
        }
        let sites = held.reference.site_count();
        if held.projected && sites == 1 && self.has(Defect::PadsLoneSiteAtProjection) {
            return Ok(MpsShape::qubit_chain(2, 1).expect("two qubits are a chain"));
        }
        if (held.projected && self.has(Defect::ReportsProductChainAfterProjection))
            || (held.losses >= 1 && self.has(Defect::ReportsProductChainAfterTruncation))
        {
            return Ok(MpsShape::qubit_chain(sites, 1).expect("a chain of the state's sites"));
        }
        if held.capped && self.has(Defect::ReportsCapacityUnderCap) {
            return Ok(held.capacity.clone());
        }
        let shape = self.report(REFERENCE.shape(&held.reference))?;
        if self.has(Defect::CachesShape) {
            return Ok(held.first_shape.get_or_insert(shape).clone());
        }
        if (held.projected && self.has(Defect::WidensChainAfterProjection))
            || (held.losses >= 2 && self.has(Defect::WidensChainAfterTruncations))
        {
            let (physical, bonds) = physical_dims_and_bonds(&shape);
            let doubled: Vec<usize> = bonds.iter().map(|bond| 2 * bond).collect();
            return Ok(MpsShape::new(chain_extents(&physical, &doubled))
                .expect("a chain with its bonds doubled is a chain"));
        }
        if self.lazy_bonds && !held.padded {
            let n = held.bonds.len() + 1;
            let lazy = (0..n)
                .map(|site| {
                    let left = site.checked_sub(1).map(|cut| held.bonds[cut]);
                    let right = held.bonds.get(site).copied();
                    left.into_iter().chain([2]).chain(right).collect()
                })
                .collect();
            return Ok(MpsShape::new(lazy).expect("bonds of at least one form a chain"));
        }
        if self.has(Defect::MirrorsShape) {
            let (mut physical, mut bonds) = physical_dims_and_bonds(&shape);
            physical.reverse();
            bonds.reverse();
            return Ok(MpsShape::new(chain_extents(&physical, &bonds))
                .expect("a mirrored chain is a chain"));
        }
        Ok(shape)
    }

    fn retained_weight(&self, state: &Self::State) -> Result<f64, DoubleError> {
        let held = state.borrow();
        if self.has(Defect::WeighsOnlyTruncation) && !held.capped {
            return Ok(0.0);
        }
        let weight = self.report(REFERENCE.retained_weight(&held.reference))?;
        let sites = held.reference.site_count();
        Ok(match self.defect {
            Some(Defect::OverstatesWeight) => weight * 1.5,
            Some(Defect::RegainsWeight) if held.losses >= 2 => weight * 1.5,
            Some(Defect::WeightBecomesNan) if held.losses >= 2 => f64::NAN,
            Some(Defect::WeightFallsToZero) if held.losses >= 2 => 0.0,
            Some(Defect::WeightNanAfterProjection) if held.just_projected && held.losses >= 1 => {
                f64::NAN
            }
            Some(Defect::LoneSiteWeight) if sites == 1 => 0.5,
            Some(Defect::WeighsFirstCutOnly) => held.first_cut_weight,
            Some(Defect::WeightZeroAfterDistantTruncationAboveBondOne)
                if held.widened_from_afar =>
            {
                0.0
            }
            Some(Defect::WeightOneBeyondTwoSites) if sites > 2 => 1.0,
            Some(Defect::WeightFrozen) => held.first_weight.unwrap_or(weight),
            Some(Defect::WeightOneMinusSum) => 1.0 - held.lost,
            Some(Defect::ForkSharesWeight) => held.caches.borrow().weight.unwrap_or(weight),
            Some(Defect::ForkReadsSharedWeight) if held.lineage != Lineage::Prepared => {
                held.caches.borrow().weight.unwrap_or(weight)
            }
            Some(Defect::OriginalReadsSharedWeight) if held.lineage == Lineage::Prepared => {
                held.caches.borrow().weight.unwrap_or(weight)
            }
            _ => weight,
        })
    }
}

/// The items of `items` whose `key` no earlier item has, in their order.
fn first_occurrences<T: Copy>(items: &[T], key: impl Fn(&T) -> usize) -> Vec<T> {
    let mut seen = Vec::new();
    items
        .iter()
        .copied()
        .filter(|item| {
            let new = !seen.contains(&key(item));
            seen.push(key(item));
            new
        })
        .collect()
}

/// The physical extent of every site of `shape`, and its bond at every cut,
/// in chain order.
fn physical_dims_and_bonds(shape: &MpsShape) -> (Vec<usize>, Vec<usize>) {
    let sites = shape.site_count();
    let physical = (0..sites)
        .map(|site| shape.physical_dim(site).expect("a site of the chain"))
        .collect();
    let bonds = (0..sites.saturating_sub(1))
        .map(|cut| shape.bond_dim(cut).expect("a cut of the chain"))
        .collect();
    (physical, bonds)
}

/// The extents of the chain whose sites carry `physical` levels, joined by
/// `bonds`, as [`MpsShape::new`] takes them.
fn chain_extents(physical: &[usize], bonds: &[usize]) -> Vec<Vec<usize>> {
    physical
        .iter()
        .enumerate()
        .map(|(site, &levels)| {
            let left = site.checked_sub(1).map(|cut| bonds[cut]);
            let right = bonds.get(site).copied();
            left.into_iter().chain([levels]).chain(right).collect()
        })
        .collect()
}
