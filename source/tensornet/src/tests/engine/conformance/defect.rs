// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The defects of an engine the kit must catch, one per rule it checks.

use crate::{Refusal, checks};

/// The tolerance of [`Defect::RefusesRoundedUnitary`]: above the deviation
/// of every gate the kit applies but its product of a thousand gates.
pub(super) const TIGHT_UNITARITY: f64 = 1e-13;

/// The tolerance of [`Defect::RefusesNearUnitary`]: a quarter of the
/// contract's, above the deviation of every gate the kit applies, a
/// product of a thousand gates included.
pub(super) const QUARTER_UNITARITY: f64 = checks::UNITARITY_TOLERANCE / 4.0;

/// The tolerance of [`Defect::AcceptsLooseUnitary`].
pub(super) const LOOSE_UNITARITY: f64 = 1e-6;

/// A defect of an engine, which the kit must catch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Defect {
    /// Applies a two-site matrix with its sites the other way round.
    SwapsSites,
    /// Leaves two sites swapped after a gate on sites that are not
    /// neighbours, as an engine that routes and forgets to route back.
    LeaksRouting,
    /// Leaves a phase of -1 on One (a Z) on each site between a gate's
    /// sites that are not neighbours, as an engine that routes with swaps
    /// carrying a phase (an iSWAP) and does not undo it.
    RoutesWithPhasedSwaps,
    /// Leaves a Z on the higher site of a gate on neighbours that truncates
    /// a cut narrower than another cut of the capacity, which keeps the
    /// weight, the chain and every P(One) right.
    PhasesANarrowCutTruncation,
    /// Answers a conditional probability without its condition, once the
    /// condition passed its checks, so that it refuses what it must.
    IgnoresCondition,
    /// Ignores each condition on a site lower than the one before it, once
    /// the whole condition passed its checks and has a probability above
    /// zero, as a sweep along the chain that expects its condition sorted.
    SkipsUnsortedCondition,
    /// Ignores each condition against the direction its first two set (a
    /// site lower than the one before it where they increase, higher where
    /// they decrease), once the whole condition passed its checks and has
    /// a probability above zero, as a sweep that takes its direction from
    /// them: right in increasing and in decreasing order, not in a mixed
    /// one.
    SkipsConditionAgainstItsSweep,
    /// Ignores each value of a condition after its first when its sites
    /// decrease, once the whole condition passed its checks and has a
    /// probability above zero, as a sweep that runs up the chain only and
    /// drops the values it has passed: right in increasing and in mixed
    /// order, not in decreasing order, two sites included.
    SkipsDecreasingCondition,
    /// Answers 1/2 for a condition of probability zero.
    AcceptsZeroCondition,
    /// Answers 1/2 for a condition of probability zero whose sites are not
    /// in increasing order, as a sweep that checks the probability only as
    /// it moves up the chain.
    AcceptsUnsortedZeroCondition,
    /// Answers 1/2 for a condition of probability zero whose sites are in
    /// increasing order, as a sweep that checks the probability only as it
    /// moves down the chain.
    AcceptsSortedZeroCondition,
    /// Refuses a condition of probability zero rightly, but only after it
    /// projected the state it holds on the condition's first value.
    ProjectsStateOnZeroCondition,
    /// Divides a conditional probability by the retained weight, which the
    /// probability, a ratio, already divides out.
    ConditionalDividedByWeight,
    /// Applies S to the site whose conditional P(One) it answers: a phase,
    /// which leaves every probability as it was.
    ConditionalLeavesPhase,
    /// Projects on the other value.
    ProjectsOtherValue,
    /// Projects rightly, but returns the other value's probability.
    ReturnsOtherProbability,
    /// Returns a probability of 0 for a value of probability zero, leaving
    /// the state as it was, instead of refusing it.
    AcceptsZeroOutcome,
    /// Refuses a value of probability below 1e-12 as if it were zero.
    ProjectThreshold,
    /// Refuses a condition of probability below 1e-12 as if it were zero.
    ConditionalThreshold,
    /// Refuses a value of probability below the square of `f64`'s epsilon
    /// (about 4.9e-32) as if it were zero, taking it for rounding.
    ProjectThresholdEpsilonSquared,
    /// Refuses a condition of probability below the square of `f64`'s
    /// epsilon as if it were zero, taking it for rounding.
    ConditionalThresholdEpsilonSquared,
    /// Answers a condition, and projects on a value, that the reference
    /// finds impossible, as if rounding had left it a probability of 1e-32,
    /// even on a state that only gates prepared (a GHZ state, or a site in
    /// |0⟩), where the reference computes exactly 0.
    RoundsUnprojectedStates,
    /// Answers a condition, once the state was projected, that the
    /// reference finds impossible, as if rounding had left it a
    /// probability of 1e-32, but fails, with no refusal, to project a fork
    /// on it.
    FailsRoundedProjections,
    /// Returns 1e-10 for a projection on a value that the reference's
    /// own rounding left a probability below 1e-30, so it answers a
    /// condition on it with a probability of the condition far above
    /// rounding.
    LiftsRoundingOutcomes,
    /// Once the state was projected, answers the double's
    /// [`leak`](Double::leak), 1e-10 unless a test sets it, for a
    /// conditional probability of One that the reference computes as zero
    /// to rounding, and one minus the leak for one it computes as one, as
    /// a conditional path that leaks where the engine's projections do
    /// not: each answer within the kit's tolerance, but a condition the
    /// reference refuses gets a probability of about the leak along that
    /// path.
    LeaksConditionalPath,
    /// Once the state was projected, answers one minus the double's
    /// [`leak`](Double::leak) for a conditional probability of One that
    /// the reference computes as one to rounding, so its path leaks only
    /// where the condition asks Zero, through P(Zero) = 1 − P(One): within
    /// the kit's tolerance, but about the leak.
    LeaksConditionalPathOnZeroSteps,
    /// Leaks as [`Defect::LeaksConditionalPath`] does, but only given
    /// Zero values, each on a site whose P(One) is zero (as a projection on
    /// Zero leaves it): a condition the reference refuses gets about 1e-10
    /// along the path only after such a certain Zero, whose P(Zero) is
    /// 1 − P(One) = 1.
    LeaksConditionalPathGivenCertainZeros,
    /// Caps every cut at the capacity's largest bond, ignoring a narrower
    /// cut's cap.
    IgnoresNarrowCuts,
    /// Brings a gate's distant sites together by swaps, capping each cut on
    /// the way at twice that cut's bond (rather than twice the bond of the
    /// cut the moving site left), so it loses weight on a capacity whose
    /// bonds differ from cut to cut, even where the state fits it.
    RoutesThroughTheNarrowCut,
    /// Applies a product of one-site gates on distant sites as those gates,
    /// with no routing, and brings an entangling gate's distant sites
    /// together by swaps, capping each cut on the way at the capacity's
    /// bond there, as a library whose swaps take the bond as their limit.
    RoutesEntanglingGatesAtTheBond,
    /// Routes as [`Defect::RoutesThroughTheNarrowCut`] does, but only when
    /// the caller names the lower site first.
    RoutesUpThroughTheNarrowCut,
    /// Routes as [`Defect::RoutesThroughTheNarrowCut`] does, but only when
    /// the caller names the higher site first.
    RoutesDownThroughTheNarrowCut,
    /// Truncates nothing after a gate on distant sites, as an engine that
    /// forgets to compress after the swaps that brought them together.
    SkipsDistantTruncation,
    /// Truncates nothing after a gate on three sites.
    SkipsThreeSiteTruncation,
    /// Truncates nothing after a gate on distant sites whose result
    /// exceeds a bond above one: an engine whose compression after its
    /// routing is right only for a bond of one.
    SkipsDistantTruncationAboveBondOne,
    /// Truncates, after a gate on three sites whose result exceeds bonds
    /// above one, only the first cut that exceeds its bond: an engine that
    /// compresses only one of the bonds such a gate widens.
    ThreeSiteTruncatesOneWideCut,
    /// Renormalizes what it keeps after a gate on distant sites whose
    /// result exceeds a bond above one, so its retained weight forgets the
    /// loss: an engine that renormalizes and tracks the loss only for a
    /// bond of one.
    RenormalizesDistantTruncationAboveBondOne,
    /// Reports a retained weight of 0 after a gate on distant sites whose
    /// result exceeds a bond above one.
    WeightZeroAfterDistantTruncationAboveBondOne,
    /// Fails, with no refusal, a gate on distant sites whose result
    /// exceeds a bond above one.
    FailsDistantTruncationAboveBondOne,
    /// Applies a matrix on three sites transposed.
    ThreeSiteTransposed,
    /// Applies a gate on three sites in the order the inverse permutation
    /// gives: where the caller's order takes the sorted sites by a
    /// permutation `p`, it takes them by `p⁻¹`. That is the caller's order
    /// when `p` swaps two sites (its own inverse), but another for a cycle
    /// of three.
    PermutesThreeSitesInversely,
    /// Refuses a gate on three sites, as if its matrix had the wrong
    /// length, instead of failing it.
    RefusesThreeSites,
    /// Reports the bonds a gate on three sites left as they were before it,
    /// as an engine that updates its bond list for gates on two sites only.
    ThreeSiteKeepsBonds,
    /// Accepts a value of probability below 1e-12 and returns its
    /// probability, but leaves the state as it was.
    SkipsTinyProjection,
    /// Refuses a value of probability zero rightly, but only after it
    /// flipped site 0.
    ProjectsStateOnZeroOutcome,
    /// Returns the probability not divided by the retained weight:
    /// ⟨ψ|Π|ψ⟩, which is the probability only when nothing truncated.
    ReturnsUnnormalizedProbability,
    /// Answers reads from a copy of the state taken at each projection,
    /// which a gate does not refresh.
    CachesReadsAcrossGates,
    /// Projects each state once: its later projections return the right
    /// probability, but do nothing.
    SkipsLaterProjections,
    /// Renormalizes the state to weight 1 after a projection.
    RenormalizesProjection,
    /// Forks a state with site 0 flipped.
    ForkDiffers,
    /// Forks a state that a truncation lost weight with site 0 flipped,
    /// as an engine that recomputes the copy with other settings.
    ForkOfTruncatedDiffers,
    /// Forks a state that a truncation lost weight with a phase flip on
    /// site 0, which no probability of a value shows.
    ForkOfTruncatedFlipsPhase,
    /// Forks a state renormalized, its retained weight reset to 1.
    ForkResetsWeight,
    /// Forks the state as it was before the last gate's truncation, when
    /// that truncation lowered a cut whose bond is above one, as an engine
    /// whose fork copies the block it split.
    ForkUndoesATruncationAboveBondOne,
    /// Forks the state as it was before the last gate's truncation, when
    /// that truncation lowered a cut narrower than another cut of the
    /// capacity.
    ForkUndoesANarrowCutTruncation,
    /// Forks by sharing the state.
    ForkShares,
    /// Forks a state by sharing it when the state is itself a fork.
    ForkOfForkShares,
    /// Reports a fresh fork as the prepared chain, every bond 1, until its
    /// first operation.
    ForkReportsPreparedShape,
    /// Reports the chain from a cache that every operation refreshes, and
    /// that a fork shares with its original.
    ForkSharesShape,
    /// Reports the retained weight from a cache that every operation
    /// refreshes, and that a fork shares with its original.
    ForkSharesWeight,
    /// Shares the weight cache as [`Defect::ForkSharesWeight`], but only a
    /// fork reports its weight from it.
    ForkReadsSharedWeight,
    /// Shares the weight cache as [`Defect::ForkSharesWeight`], but only
    /// the original reports its weight from it.
    OriginalReadsSharedWeight,
    /// Samples the same values whatever the seed.
    IgnoresSeed,
    /// Draws with a seed that changes at each call: the same seed gives
    /// other samples.
    ReseedsEachCall,
    /// Returns the samples site by site, not shot by shot.
    SamplesSiteMajor,
    /// Returns each sampled value inverted, One for Zero.
    InvertsSamples,
    /// Draws each site on its own, breaking the correlations of a shot.
    SamplesSitesApart,
    /// Draws one shot fewer than asked.
    DrawsOneShotTooFew,
    /// Fails a seed above 2³¹−1, as an engine would that passed the seed
    /// on to a library with a 31-bit seed without mapping it.
    NarrowsSeed,
    /// Fails the seed 0, as an engine would that passed the seed on to a
    /// library that takes no zero seed without mapping it.
    RefusesSeedZero,
    /// Draws each shot twice in a row: every shot a draw of the right
    /// distribution, but every other shot a copy of the one before it.
    DrawsEachShotTwice,
    /// Returns one value per shot when asked for no site.
    FillsEmptySample,
    /// Draws every other shot as all Zero.
    SkewsSamples,
    /// Draws each value with its conditional P(One) raised by 0.02 where
    /// it is neither 0 nor 1.
    SampleBiasedSmall,
    /// Draws each value with its conditional P(One) raised by 0.02 where it
    /// is between 0.3 and 0.7.
    SampleBiasedEven,
    /// Draws each value with a conditional P(One) below 0.05 as 0 and one
    /// above 0.95 as 1, dropping the rare outcomes.
    SampleDropsRare,
    /// Draws every odd shot from the uniform draws of the shot before it,
    /// each `u` turned into `1 − u`: every shot a draw of the right
    /// distribution, but each pair of shots anticorrelated.
    SampleAntithetic,
    /// Copies the rest of the shot before when the first value it draws
    /// agrees with that shot's: every shot a draw of the right
    /// distribution, but consecutive shots correlated past their first
    /// value.
    SampleReusesPrefixDraws,
    /// Copies the shot before into one shot in five (shots 1, 6, 11, …):
    /// every shot a draw of the right distribution, but consecutive shots
    /// correlated, weakly.
    SampleCopiesOneShotInFive,
    /// Draws the first half of the shots and repeats them: every shot a
    /// draw of the right distribution, but each the copy of one half the
    /// shots before.
    SampleRepeatsFirstHalf,
    /// Draws three quarters of the shots and repeats the first quarter
    /// after them: every shot a draw of the right distribution, but each of
    /// the last quarter the copy of one three quarters of the shots before,
    /// more than half of them.
    SampleRepeatsFirstQuarterLast,
    /// Restarts its stream with the same seed every `chunk` shots, as a
    /// chunked or threaded sampler that seeds each chunk alike: every shot
    /// a draw of the right distribution, but each the copy of one `chunk`
    /// shots before.
    RestartsStreamEvery(usize),
    /// Draws the first value of each shot with the uniform the shot before
    /// drew its last value with: every shot a draw of the right
    /// distribution, but one site of each shot tied to another site of the
    /// shot before.
    SampleCarriesDraw,
    /// Seeds its generator with the low 32 bits of the seed.
    SeedLow32,
    /// Seeds its generator with the parity of the seed.
    SeedParityOnly,
    /// Seeds its generator with the seed shifted right by one bit.
    SeedDropsLowBit,
    /// Seeds its generator with the seed's top bit cleared.
    SeedDropsTopBit,
    /// Seeds its generator with the seed passed through an `f64`, which
    /// drops the low bits of a seed above 2⁵³.
    SeedThroughF64,
    /// Returns no values for no shots before checking the sites.
    SampleZeroShotsUnchecked,
    /// Returns no values for no shots after checking only that the sites
    /// are in range, so that a repeated site goes unrefused.
    SampleZeroShotsChecksRangeOnly,
    /// Refuses a repeated site in a sample only where it repeats the site
    /// before it, as a check that compares neighbours: elsewhere it draws
    /// the site once and repeats its value.
    SampleRefusesOnlyAdjacentRepeats,
    /// Refuses a repeated site in a condition only where it repeats the
    /// site before it, as a check that compares neighbours: elsewhere it
    /// keeps the site's first value.
    ConditionRefusesOnlyAdjacentRepeats,
    /// Inverts the first value it draws: every frequency moves by one shot,
    /// within the tolerance, but that shot breaks a correlation.
    BreaksOneShot,
    /// Draws from the state the real parts of its amplitudes make,
    /// normalized: an engine that drops the imaginary parts when it
    /// samples.
    SamplesRealParts,
    /// Draws the last site of each shot as One only when its first site is
    /// One and the last site was One in the shot or the one before: every
    /// marginal within the frequency tolerance, the joint outcomes not.
    CorrelatesSamples,
    /// Applies S to site 0 of the state it samples: a phase, which leaves
    /// every sample as it was.
    SamplingLeavesPhase,
    /// Applies S to the site whose P(One) it reads.
    ReadingLeavesPhase,
    /// Reads P(One) in single precision.
    ReadsInSinglePrecision,
    /// Takes the conjugate of each coefficient of an observable.
    ConjugatesCoefficients,
    /// Keeps only the real part of each term's coefficient.
    ReadsRealCoefficients,
    /// Reads only the first term of an observable, and its identity part.
    ReadsFirstTermOnly,
    /// Reads NaN for an observable of more than one term.
    ReadsNanForSums,
    /// Reads P(One) of site 1 as NaN.
    ReadsNanProbability,
    /// Reads an observable of no term, the identity alone, as zero.
    ReadsIdentityAloneAsZero,
    /// Puts a term's sites in increasing order and leaves its Paulis in
    /// the order given, so that each Pauli lands on another site.
    SortsSitesNotPaulis,
    /// Puts a term's sites in increasing order, and reverses its Paulis
    /// when its first site is its highest: right for factors in increasing
    /// or decreasing site order only.
    MixedOrderPairing,
    /// Puts a term's sites in increasing order, and rotates its Paulis as
    /// it would rotate the factors to put the lowest site first: right for
    /// factors in any rotation of increasing site order only.
    RotationPairing,
    /// Reads each string once, however many terms repeat it.
    MergesRepeatedTerms,
    /// Checks only the first factor of an observable's first term against
    /// the chain, and reads every other factor on a site out of range as
    /// the identity, as an engine that validates one factor of a sum.
    ChecksFirstFactorOnly,
    /// Adds 1e-8 to the read of a lone Pauli string with a Y factor, as a
    /// fast path for single strings that rounds coarsely.
    RoundsLoneStrings,
    /// Reports the chain mirrored, its last site first.
    MirrorsShape,
    /// Reports the chain it first reported for a state, however the state
    /// changed since.
    CachesShape,
    /// Reports every bond doubled once the state was projected, beyond the
    /// capacity.
    WidensChainAfterProjection,
    /// Reports every bond as 1 once the state was projected, below the
    /// Schmidt rank.
    ReportsProductChainAfterProjection,
    /// Reports the capacity as its chain under a capacity that caps a
    /// bond, as an engine that allocates every bond at its capacity.
    ReportsCapacityUnderCap,
    /// Reports every bond doubled once a second truncation lost weight,
    /// beyond the capacity.
    WidensChainAfterTruncations,
    /// Reports every bond as 1 once a truncation lost weight, below the
    /// Schmidt rank.
    ReportsProductChainAfterTruncation,
    /// Reports more weight than the state has.
    OverstatesWeight,
    /// Reports a weight of 0 for a state whose capacity caps no bond, as an
    /// engine that counts the weight only where it truncates.
    WeighsOnlyTruncation,
    /// Projects a truncated state without dividing by the root of the
    /// probability: its reads, divided by its weight, stay right, but the
    /// weight falls by the probability.
    KeepsProjectionNorm,
    /// Reports the weight right until a second truncation lost weight, then
    /// more.
    RegainsWeight,
    /// Reports the weight right until a second truncation lost weight, then
    /// NaN.
    WeightBecomesNan,
    /// Reports the weight right until a second truncation lost weight, then
    /// 0.
    WeightFallsToZero,
    /// Reports the weight as NaN right after a projection of a state a
    /// truncation lost weight of, and right otherwise.
    WeightNanAfterProjection,
    /// Reports a lone site's weight as one half.
    LoneSiteWeight,
    /// Truncates the first cut only, as an engine that sweeps its chain
    /// from one end and stops after a cut.
    TruncatesFirstCutOnly,
    /// Reports the product of the fractions the truncations at the first
    /// cut kept, ignoring those at the others.
    WeighsFirstCutOnly,
    /// Reports a weight of 1 on chains of more than two sites.
    WeightOneBeyondTwoSites,
    /// Divides a Pauli sum's expectation by the fractions the truncations
    /// at the first cut kept, not by the whole retained weight.
    ExpectationByFirstCutWeight,
    /// Divides reads by the weight on states of two sites only.
    NormalizesTwoSitesOnly,
    /// Keeps one Schmidt pair more than the bond at each truncation.
    KeepsChiPlusOne,
    /// Drops every Schmidt value whose square is below 1e-8 of its cut's
    /// weight, after every gate, as an engine whose cutoff is coarser than
    /// the kit's states allow.
    CoarseCutoff,
    /// Reports the weight the first truncation that lost weight kept,
    /// whatever the later truncations keep.
    WeightFrozen,
    /// Reports `1 − Σ ε` for the fractions `ε` the truncations lost, not
    /// their product `Π (1 − ε)`: the same after one truncation.
    WeightOneMinusSum,
    /// Reads a truncated state without dividing by its weight.
    UnnormalizedReads,
    /// Divides reads by the weight the first truncation that lost weight
    /// kept, which a later truncation does not refresh.
    ReadsByFirstWeight,
    /// Draws the first site of each shot with ⟨ψ|Π₁|ψ⟩, not divided by
    /// ⟨ψ|ψ⟩: right only when nothing truncated.
    SampleUnnormalized,
    /// Answers reads from the state as it was before the last gate's
    /// truncation, as an engine that reads its two-site block before
    /// splitting it.
    ReadsBeforeTruncation,
    /// Answers reads from the state as its last two-site truncation left
    /// it, which a later one-site gate does not refresh.
    CachesReadsAtTruncation,
    /// Holds a lone site as two sites, as a library that needs a bond.
    PadsLoneSite,
    /// Reports a lone site as two sites once it was projected, as a
    /// library that pads the chain to project.
    PadsLoneSiteAtProjection,
    /// Holds a lone site as two sites and reports a lone site, but takes
    /// the padding's site in every method.
    HidesPadding,
    /// Holds a lone site as two sites and reports a lone site, but takes
    /// the padding's site in `method` only, refusing it in the others.
    TakesPaddingOnlyIn(Method),
    /// Returns each sampled value of a lone site inverted.
    InvertsLoneSiteSamples,
    /// Prepares a capacity of sites that are not qubits as qubits.
    AcceptsQutrits,
    /// Takes a matrix that is not unitary, doing nothing.
    AcceptsNonUnitary,
    /// Takes a matrix on two sites that is not unitary, doing nothing, as
    /// an engine whose own check reads only the rows of one site.
    AcceptsNonUnitaryOnTwoSites,
    /// Fails every gate on more than two sites, as no refusal, before its
    /// checks, as an engine that tests the arity it supports first.
    FailsBeforeChecksOnThreeSites,
    /// Reports, for a gate on three sites, the refusal of `fault` as a
    /// failure of its own, as an engine whose path for gates on more than
    /// two sites leaves that check to its library.
    FailsThreeSiteRefusal(Fault),
    /// Takes a matrix with an infinite entry, doing nothing, as an engine
    /// whose own check of the entries looks for NaN only.
    AcceptsInfinity,
    /// Refuses a matrix whose `M·M† − I` sums to more than
    /// [`TIGHT_UNITARITY`]: unitary to rounding is not enough for it.
    RefusesRoundedUnitary,
    /// Refuses a matrix whose `M·M† − I` sums to more than
    /// [`QUARTER_UNITARITY`].
    RefusesNearUnitary,
    /// Takes a matrix whose `M·M† − I` sums to at most [`LOOSE_UNITARITY`]
    /// as unitary, doing nothing.
    AcceptsLooseUnitary,
    /// Exposes, for a site out of range, the refusal of a repeated site,
    /// while its message names the right refusal: an error type whose
    /// `refusal` maps the library's error wrongly.
    WrongRefusal,
    /// Reports a refusal as a failure of its own.
    RefusalAsFailure,
    /// Displays a refusal in words of its own.
    RewordsRefusal,
    /// Turns site 0 by `Ry(1)` before refusing an apply: unlike a flip,
    /// an even number of refusals does not undo it.
    ChangesStateWhenRefusing,
}

/// A fault of a call to `apply`, for [`Defect::FailsThreeSiteRefusal`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    NotUnitary,
    MatrixLength,
    RepeatedSite,
    SiteOutOfRange,
}

impl Fault {
    pub(super) fn of(refusal: &Refusal) -> Option<Self> {
        match refusal {
            Refusal::NotUnitary { .. } => Some(Self::NotUnitary),
            Refusal::MatrixLength { .. } => Some(Self::MatrixLength),
            Refusal::RepeatedSite { .. } => Some(Self::RepeatedSite),
            Refusal::SiteOutOfRange { .. } => Some(Self::SiteOutOfRange),
            _ => None,
        }
    }
}

/// A method of an engine, for [`Defect::TakesPaddingOnlyIn`]; `apply` on
/// one site and on two count apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Method {
    OneSiteApply,
    TwoSiteApply,
    ProbabilityOfOne,
    Project,
    Expectation,
    Sample,
}
