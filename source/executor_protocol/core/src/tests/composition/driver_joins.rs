// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a chain by the `on` that makes its runner, wherever the
//! brackets go: the set each part sends against the set the part below
//! accepts; what arrives at each part, what the part above requires of
//! what arrives there, against what it provides; and what a joined part
//! requires and provides.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::composition::faults;
use crate::tests::example_sets::{Measuring, ToffoliGate, TooFewQubits};
use crate::tests::fake_parts::{
    Bits, CczDecomposition, CliffordTRegister, CoinCopy, Log, Profiler, Program, Renamed, Spread,
    StatesNoLimits, Stating, ToffoliProgram, batches_of, coin_gates, in_each_bracketing,
};
use crate::{
    Batch, BatchReadouts, BoundaryCompatible, BoundaryMismatch, Capabilities, CapabilitiesMismatch,
    CompositionFault, CompositionFaults, ExecuteError, FeatureMismatch, Features, PartId,
    ResourceMismatch, Resources, RunRequirement, Seed, StatesLimits, TranslatingAdapter,
};

/// `h`, `x` and `mz` without `x`.
pub(super) const NO_X: Measuring = Measuring::new(&["h", "mz"]);

/// `h`, `x` and `mz` without `mz`.
const NO_MZ: Measuring = Measuring::new(&["h", "x"]);

/// `h`, `x` and `mz` without `h`.
const NO_H: Measuring = Measuring::new(&["x", "mz"]);

/// The part of type `P` at `position`, named by its type.
pub(super) fn part<P>(position: usize) -> PartId {
    PartId::new(position, type_name::<P>())
}

/// The fault of the part `requirer` against the part `provider`, which
/// falls short by `mismatch`.
pub(super) fn fault(
    requirer: PartId,
    provider: PartId,
    mismatch: BoundaryMismatch,
) -> CompositionFault {
    CompositionFault {
        requirer,
        provider,
        mismatch,
    }
}

/// `faults` as their `Debug` reads, which compares faults holding a set's
/// boxed mismatch.
pub(super) fn listed(faults: &[CompositionFault]) -> String {
    format!("{faults:#?}")
}

/// The mismatch of the set `sent` against `accepted`.
pub(super) fn instructions(sent: &Measuring, accepted: &Measuring) -> BoundaryMismatch {
    BoundaryMismatch {
        instructions: Some(Box::new(
            sent.fits(accepted).expect_err("sets that do not fit"),
        )),
        ..BoundaryMismatch::default()
    }
}

/// The mismatch of `capabilities` alone.
fn capabilities(capabilities: CapabilitiesMismatch) -> BoundaryMismatch {
    BoundaryMismatch {
        capabilities,
        ..BoundaryMismatch::default()
    }
}

/// The mismatch of `resource` alone.
fn short(resource: ResourceMismatch) -> BoundaryMismatch {
    capabilities(CapabilitiesMismatch {
        resources: vec![resource],
        ..CapabilitiesMismatch::default()
    })
}

/// The mismatch of batches of `required` instructions against `provided`.
pub(super) fn short_batches(required: u32, provided: u32) -> BoundaryMismatch {
    short(ResourceMismatch::InstructionsPerBatch { required, provided })
}

/// The mismatch of runs of `required` shots against `provided`.
pub(super) fn short_shots(required: u32, provided: u32) -> BoundaryMismatch {
    short(ResourceMismatch::Shots { required, provided })
}

/// The mismatch of `required` qubits against `provided`.
fn short_qubits(required: u32, provided: u32) -> BoundaryMismatch {
    capabilities(CapabilitiesMismatch {
        set_capabilities: Some(Box::new(TooFewQubits { required, provided })),
        ..CapabilitiesMismatch::default()
    })
}

/// A program of one `ccz` on 3 qubits.
fn one_ccz() -> StatesLimits<ToffoliProgram> {
    ToffoliProgram::new(&[ToffoliGate::Ccz(0, 1, 2)], 3)
}

/// The mismatch of `feature`, required and not provided.
fn missing(feature: FeatureMismatch) -> BoundaryMismatch {
    capabilities(CapabilitiesMismatch {
        features: vec![feature],
        ..CapabilitiesMismatch::default()
    })
}

/// The program measuring qubit 0, requiring batches of `n` instructions
/// and `features`.
pub(super) fn program(n: u32, features: Features) -> StatesLimits<Program> {
    Program::new(&[("mz", 0)])
        .with_resources(batches_of(n))
        .with_features(features)
}

/// Replay, required or provided, and the seeded runs it states.
fn replay() -> Features {
    Features::unconstrained().with_replay(true)
}

/// Bits taking batches of `n` instructions.
pub(super) fn bits_over(n: u32, log: &Log) -> StatesLimits<Bits> {
    Bits::new(log).with_resources(batches_of(n))
}

/// Checks, in each bracketing of the parts `$setup` binds, that the
/// chain's faults are `$expected`, as the row `$row`: none if it composes.
macro_rules! faults_in_each_bracketing {
    ($row:expr, [$($part:ident),+] from $setup:tt, $expected:expr $(,)?) => {
        let expected: Vec<CompositionFault> = $expected;
        let expected = listed(&expected);
        in_each_bracketing!(
            [$($part),+] from $setup,
            |chain, brackets| assert_eq!(
                listed(chain.err().as_ref().map_or(&[][..], CompositionFaults::faults)),
                expected,
                "{}, {brackets}",
                $row
            ),
        );
    };
}

// ------------------------------------------------------ adjacent parts

/// A code level whose sets do not fit at any of its three boundaries, in
/// each bracketing, so that each join meets a boundary that does not fit
/// (a driver or an adapter through an adapter, either on an executor),
/// and with its joined lower parts lent by `&mut` to the `on` of two
/// chains, as an executor or as an adapter: the runner is never made, and
/// the chain's faults hold each boundary, top to bottom, naming both
/// parts, with the set's own mismatch.
#[test]
fn every_boundary_that_does_not_fit_is_a_fault_of_the_chain() {
    let counted = Rc::new(Cell::new(0));
    let each_boundary = || {
        vec![
            fault(
                part::<CoinCopy>(0),
                part::<Profiler>(1),
                instructions(coin_gates(), &NO_X),
            ),
            fault(
                part::<Profiler>(1),
                part::<Profiler>(2),
                instructions(&NO_X, &NO_MZ),
            ),
            fault(
                part::<Profiler>(2),
                part::<Bits>(3),
                instructions(&NO_MZ, &NO_H),
            ),
        ]
    };
    faults_in_each_bracketing!(
        "sets",
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = Profiler::on(NO_X, &counted, &log);
            let b = Profiler::on(NO_MZ, &counted, &log);
            let e = Bits::accepting(NO_H, &log);
        },
        each_boundary(),
    );
    let expected = listed(&each_boundary());
    let log = Log::default();
    let profiler = |set| Profiler::on(set, &counted, &log);
    let mut lent_executor = profiler(NO_X)
        .through(profiler(NO_MZ))
        .on(Bits::accepting(NO_H, &log));
    let mut lent_adapter = profiler(NO_X).through(profiler(NO_MZ));
    for chain in ["first", "second"] {
        assert_eq!(
            listed(faults(CoinCopy::new().on(&mut lent_executor)).faults()),
            expected,
            "a lent executor, {chain} chain"
        );
        assert_eq!(
            listed(
                faults(
                    CoinCopy::new()
                        .through(&mut lent_adapter)
                        .on(Bits::accepting(NO_H, &log))
                )
                .faults()
            ),
            expected,
            "a lent adapter, {chain} chain"
        );
    }
}

/// A part is checked below for the set it sends, not for the set it
/// accepts: a decomposer accepting `h`, `x` and `mz` and sending them
/// without `x`, as it writes each `x` in other gates, composes over bits
/// that accept no `x`, and is named, not the driver, over bits that accept
/// no `mz`, in each bracketing.
#[test]
fn a_part_is_checked_below_for_the_instructions_it_sends() {
    faults_in_each_bracketing!(
        "a decomposer over what it sends",
        [d, a, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = Decomposer(NO_X);
            let e = Bits::accepting(NO_X, &log);
        },
        vec![],
    );
    faults_in_each_bracketing!(
        "a decomposer over less than it sends",
        [d, a, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = Decomposer(NO_X);
            let e = Bits::accepting(NO_MZ, &log);
        },
        vec![fault(
            part::<Decomposer>(1),
            part::<Bits>(2),
            instructions(&NO_X, &NO_MZ),
        )],
    );
}

/// A declaration fake of a decomposer: it accepts `h`, `x` and `mz`, and
/// sends the set it holds. It is never run.
struct Decomposer(Measuring);

impl Adapter for Decomposer {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        &self.0
    }

    fn bind(&mut self, _below: Vec<u32>) {}

    async fn adapt(
        &mut self,
        _batches: &[Batch<'_, Measuring>],
        _seed: Seed,
        _below: &mut impl Executor<Accepts = Measuring>,
        _readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        unreachable!("a declaration fake is never run")
    }
}

// ------------------------------------------------------- what arrives

/// A value a part passes below unchanged arrives at each part below it,
/// down to the executor, and is checked against what each provides, in
/// each bracketing, a fault naming the part that first required it. A row
/// per way a part passes it: stating nothing, as a logger does; requiring
/// otherwise only below its least, as an interleaver under a larger
/// program does; limiting it, as a decoder's limit on instructions a batch
/// does, short itself or over a smaller limit, and with an interleaver
/// above it; a job queue's limit on shots, where the value falls short at
/// both the queue and below it; and a replayer, which passes the seeded
/// runs replay states. A part stating limits is checked beside parts
/// stating none: a decoder's limit between a logger and an executor that
/// state none receives, through the logger, what the program requires, a
/// fault naming the decoder when its limit falls short; the executor
/// limits nothing, as it states none of the bits' batches of 4, so the
/// larger limit composes. A program's qubits pass unchanged through a
/// translating adapter that decomposes its `ccz` with no ancilla, so a
/// register of 2 qubits below falls short of the program's 3; and a
/// translating adapter taking at most 2 qubits falls short of them, a
/// fault naming it as the provider, while one taking 3 composes.
/// Instructions a batch, shots and qubits stand for any value: which
/// values fit is each kind's rule, pinned once.
#[test]
#[expect(clippy::too_many_lines, reason = "a row per way a part passes a value")]
fn a_requirement_passed_unchanged_is_checked_at_each_part_below_naming_its_requirer() {
    let counted = Rc::new(Cell::new(0));
    faults_in_each_bracketing!(
        "batches through a logger",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Profiler::new(&counted, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_batches(5, 4))],
    );
    faults_in_each_bracketing!(
        "batches through an interleaver, more than its least",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Stating::interleaving(3, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_batches(5, 4))],
    );
    faults_in_each_bracketing!(
        "batches over a decoder's limit",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Stating::capping(4, &log);
            let e = bits_over(10, &log);
        },
        vec![fault(part::<Program>(0), part::<Stating>(1), short_batches(5, 4))],
    );
    faults_in_each_bracketing!(
        "batches through a decoder's larger limit",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, Features::unconstrained());
            let a = Stating::capping(64, &log);
            let e = bits_over(8, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_batches(9, 8))],
    );
    faults_in_each_bracketing!(
        "batches through an interleaver and a decoder's larger limit",
        [d, i, c, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let i = Stating::interleaving(3, &log);
            let c = Stating::capping(10, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(3), short_batches(5, 4))],
    );
    let shots =
        |n| program(1, Features::unconstrained()).with_resources(batches_of(1).with_shots(n));
    let bits_of_3_shots = |log: &Log| Bits::new(log).with_resources(batches_of(64).with_shots(3));
    faults_in_each_bracketing!(
        "shots short at a queue and below it",
        [d, a, e] from {
            let log = Log::default();
            let d = shots(12);
            let a = Stating::queue(10, &log);
            let e = bits_of_3_shots(&log);
        },
        vec![
            fault(part::<Program>(0), part::<Stating>(1), short_shots(12, 10)),
            fault(part::<Program>(0), part::<Bits>(2), short_shots(12, 3)),
        ],
    );
    let not_replayable = || Features::unconstrained().with_replay(false);
    faults_in_each_bracketing!(
        "seeded runs through a replayer",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = Stating::replaying(&log);
            let e = Bits::new(&log).with_features(not_replayable().with_seeded_runs(false));
        },
        vec![fault(
            part::<Program>(0),
            part::<Bits>(2),
            missing(FeatureMismatch::SeededRuns),
        )],
    );
    faults_in_each_bracketing!(
        "qubits through a decomposition with no ancilla",
        [d, t, e] from {
            let log = Log::default();
            let d = one_ccz();
            let t = CczDecomposition::with_ancillas(0);
            let e = CliffordTRegister::new(2, &[], &log);
        },
        vec![fault(
            part::<ToffoliProgram>(0),
            part::<CliffordTRegister>(2),
            short_qubits(3, 2),
        )],
    );
    for (qubits, expected) in [
        (
            2,
            vec![fault(
                part::<ToffoliProgram>(0),
                part::<CczDecomposition>(1),
                short_qubits(3, 2),
            )],
        ),
        (3, vec![]),
    ] {
        faults_in_each_bracketing!(
            format!("qubits over a decomposition taking {qubits}"),
            [d, t, e] from {
                let log = Log::default();
                let d = one_ccz();
                let t = CczDecomposition::for_qubits(qubits);
                let e = CliffordTRegister::new(5, &[], &log);
            },
            expected,
        );
    }
    for (row, limit, expected) in [
        (
            "batches through a logger and a decoder's limit, over no limit",
            4,
            vec![fault(
                part::<Program>(0),
                part::<Stating>(2),
                short_batches(5, 4),
            )],
        ),
        (
            "batches through a logger and a decoder's larger limit, over no limit",
            10,
            vec![],
        ),
    ] {
        faults_in_each_bracketing!(
            row,
            [d, p, c, e] from {
                let log = Log::default();
                let d = program(5, Features::unconstrained());
                let p = Profiler::new(&counted, &log);
                let c = Stating::capping(limit, &log);
                let e = StatesNoLimits(bits_over(4, &log));
            },
            expected,
        );
    }
}

/// What a part requires below, given what arrives at it, is what arrives
/// at the part below, checked there, a fault naming that part, in each
/// bracketing. A row per way a part requires otherwise: more than
/// arrives, as an interleaver under a program smaller than its least,
/// over bits too small or large enough; less than arrives, as a replayer
/// requiring no replay below; and a part changing the value under one
/// limiting it, as an ancilla's reservation under a decoder's limit,
/// where each part falls short of its own; and a translating adapter
/// requiring of another set, as a decomposition of `ccz` into 13 gates
/// with 2 ancillas under a program of 3 qubits, which requires 5 qubits
/// and batches of 13 below: a register of 4 qubits, or one taking 12
/// instructions a batch, falls short of it, and one of 5 taking 13 does
/// not.
#[test]
fn what_a_part_requires_of_what_arrives_is_checked_below_naming_it() {
    for (bits, short) in [(2, true), (4, false)] {
        faults_in_each_bracketing!(
            format!("an interleaver of 3 over bits taking {bits}, under 2"),
            [d, a, e] from {
                let log = Log::default();
                let d = program(2, Features::unconstrained());
                let a = Stating::interleaving(3, &log);
                let e = bits_over(bits, &log);
            },
            if short {
                vec![fault(
                    part::<Stating>(1),
                    part::<Bits>(2),
                    short_batches(3, 2),
                )]
            } else {
                vec![]
            },
        );
    }
    let not_replayable = || Features::unconstrained().with_replay(false);
    faults_in_each_bracketing!(
        "a replayer over shots not replayable",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = Stating::replaying(&log);
            let e = Bits::new(&log).with_features(not_replayable());
        },
        vec![],
    );
    faults_in_each_bracketing!(
        "a decoder's limit above an ancilla's reservation",
        [d, c, a, e] from {
            let log = Log::default();
            let d = program(6, Features::unconstrained());
            let c = Stating::capping(4, &log);
            let a = Stating::reserving_ancillas(1, &log);
            let e = bits_over(5, &log);
        },
        vec![
            fault(part::<Program>(0), part::<Stating>(1), short_batches(6, 4)),
            fault(part::<Stating>(2), part::<Bits>(3), short_batches(7, 5)),
        ],
    );
    let decomposition = part::<CczDecomposition>(1);
    let register = part::<CliffordTRegister>(2);
    for (row, qubits, batches, expected) in [
        (
            "4 qubits",
            4,
            None,
            vec![fault(
                decomposition.clone(),
                register.clone(),
                short_qubits(5, 4),
            )],
        ),
        (
            "12 instructions a batch",
            5,
            Some(12),
            vec![fault(
                decomposition.clone(),
                register.clone(),
                short_batches(13, 12),
            )],
        ),
        ("5 qubits and 13 instructions a batch", 5, Some(13), vec![]),
    ] {
        faults_in_each_bracketing!(
            format!("a decomposition with 2 ancillas over a register of {row}"),
            [d, t, e] from {
                let log = Log::default();
                let d = one_ccz();
                let t = CczDecomposition::with_ancillas(2);
                let e = CliffordTRegister::new(qubits, &[], &log);
                let e = match batches {
                    Some(n) => e.taking(n),
                    None => e,
                };
            },
            expected,
        );
    }
}

/// A chain whose parts fall short in several places: every fault, by its
/// requirer top to bottom, then by its provider, each mismatch named by
/// the part its own value comes from. Batches of 9 instructions and
/// replay, through a sampler seeding itself, which takes no replay, over
/// bits taking 8: the replay at the sampler and the batches at the bits,
/// both the program's. Then batches of 8 and 12 shots, through an
/// ancilla's reservation raising the batches to 9 and a decoder's limit of
/// 8, over bits taking 8 and 10 shots: the program's shots at the bits,
/// then the ancilla's batches at the decoder and at the bits. A walk
/// naming one requirer for all a part falls short of lists the bits' two
/// mismatches under one part; one ordering by provider first lists the
/// decoder's fault first. Last, an ancilla's reservation of 9, sending
/// `x`, over bits taking 8 and accepting no `x`: one fault for the pair,
/// holding both the set's and the batches' mismatches. And a program of
/// batches of 9, through a profiler, over bits taking 8 and accepting no
/// `x`: the batches are the program's, the set the profiler's, so the bits
/// fall short of each in its own fault; one walk filing the bits' batches
/// under the part sending them instructions lists one fault.
#[test]
fn every_fault_is_listed_by_requirer_then_by_provider() {
    faults_in_each_bracketing!(
        "shortages of different kinds",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, replay());
            let a = Stating::seeding_itself(&log);
            let e = bits_over(8, &log);
        },
        vec![
            fault(
                part::<Program>(0),
                part::<Stating>(1),
                missing(FeatureMismatch::Replay),
            ),
            fault(part::<Program>(0), part::<Bits>(2), short_batches(9, 8)),
        ],
    );
    faults_in_each_bracketing!(
        "each value by its own requirer",
        [d, a, c, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]).with_resources(batches_of(8).with_shots(12));
            let a = Stating::reserving_ancillas(1, &log);
            let c = Stating::capping(8, &log);
            let e = Bits::new(&log).with_resources(batches_of(8).with_shots(10));
        },
        vec![
            fault(part::<Program>(0), part::<Bits>(3), short_shots(12, 10)),
            fault(part::<Stating>(1), part::<Stating>(2), short_batches(9, 8)),
            fault(part::<Stating>(1), part::<Bits>(3), short_batches(9, 8)),
        ],
    );
    faults_in_each_bracketing!(
        "a set and batches at one pair",
        [d, a, e] from {
            let log = Log::default();
            let d = program(8, Features::unconstrained());
            let a = Stating::reserving_ancillas(1, &log);
            let e = Bits::accepting(NO_X, &log).with_resources(batches_of(8));
        },
        vec![fault(
            part::<Stating>(1),
            part::<Bits>(2),
            BoundaryMismatch {
                capabilities: short_batches(9, 8).capabilities,
                ..instructions(coin_gates(), &NO_X)
            },
        )],
    );
    faults_in_each_bracketing!(
        "a set and batches from different parts at one provider",
        [d, p, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program(9, Features::unconstrained());
            let p = Profiler::new(&counted, &log);
            let e = Bits::accepting(NO_X, &log).with_resources(batches_of(8));
        },
        vec![
            fault(part::<Program>(0), part::<Bits>(2), short_batches(9, 8)),
            fault(
                part::<Profiler>(1),
                part::<Bits>(2),
                instructions(coin_gates(), &NO_X),
            ),
        ],
    );
}

/// The reservation of an ancilla over too small batches, then the same
/// with a profiler above and below it: the same parts named, with the same
/// mismatch, at the positions the profilers shift.
#[test]
fn inserting_a_profiler_changes_only_the_positions_of_the_faults() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let profiler = || Profiler::new(&counted, &log);
    let ancilla = || Stating::reserving_ancillas(1, &log);
    let without = faults(
        program(9, Features::unconstrained())
            .through(ancilla())
            .on(bits_over(9, &log)),
    );
    let with = faults(
        program(9, Features::unconstrained())
            .through(profiler())
            .through(ancilla())
            .through(profiler())
            .on(bits_over(9, &log)),
    );
    assert_eq!(
        listed(without.faults()),
        listed(&[fault(
            part::<Stating>(1),
            part::<Bits>(2),
            short_batches(10, 9)
        )])
    );
    assert_eq!(
        listed(with.faults()),
        listed(&[fault(
            part::<Stating>(2),
            part::<Bits>(4),
            short_batches(10, 9)
        )])
    );
}

// ---------------------------------------------------------- the joined

/// A joined part, as its public methods state it: it requires what its
/// lower part requires of what its upper part requires of what arrives;
/// and it provides what its top part provides, as the inputs its parts
/// take together are not one value. Its inner limits are checked part by
/// part, by the chain's check in every bracketing (the table of a
/// requirement passed unchanged, above), and, through a wrapper, by the
/// joined part's own `ready`.
#[test]
fn a_joined_part_requires_its_parts_requirements_in_turn_and_provides_its_top_parts_limits() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let read = |capabilities: Option<&Capabilities<Measuring>>| {
        capabilities.map(|capabilities| (*capabilities.resources(), *capabilities.features()))
    };
    let batches_of_two = Capabilities::needs_nothing().map_resources(|_| batches_of(2));
    let rows = [
        // The encoding's batches of 14 for the program's 2, and the
        // program's replay.
        (
            "a driver through an encoding",
            read(Some(&DriverReadings::required(
                &program(2, replay()).through(Spread::new(7, &log).built_for(2)),
                &RunRequirement::unconstrained(),
            ))),
            Some((batches_of(14), replay())),
        ),
        (
            "an encoding through a profiler, requiring",
            read(Some(&TranslatingAdapter::requires(
                &Spread::new(7, &log)
                    .built_for(2)
                    .through(Profiler::new(&counted, &log)),
                &batches_of_two,
            ))),
            Some((batches_of(14), Features::unconstrained())),
        ),
        (
            "a profiler through an encoding, providing",
            read(TranslatingAdapter::provides(
                &Profiler::new(&counted, &log).through(Spread::new(7, &log).built_for(2)),
            )),
            None,
        ),
        // The interleaver's least, for a program smaller than it.
        (
            "a driver through an interleaver",
            read(Some(&DriverReadings::required(
                &program(2, Features::unconstrained()).through(Stating::interleaving(3, &log)),
                &RunRequirement::unconstrained(),
            ))),
            Some((batches_of(3), Features::unconstrained())),
        ),
        // The queue's 4 shots, not the bits' batches of 8.
        (
            "a queue on bits",
            read(ExecutorReadings::provided(&Stating::queue(4, &log).on(
                Bits::new(&log).with_resources(batches_of(8).with_shots(8)),
            ))),
            Some((
                Resources::unconstrained().with_shots(4),
                Features::unconstrained(),
            )),
        ),
        // The decoder's batches of 64, not the queue's 4 shots.
        (
            "a decoder through a queue",
            read(TranslatingAdapter::provides(
                &Stating::capping(64, &log).through(Stating::queue(4, &log)),
            )),
            Some((batches_of(64), Features::unconstrained())),
        ),
    ];
    for (row, read, expected) in rows {
        assert_eq!(read, expected, "{row}");
    }
}

/// An interleaver of 3 joined with a decoder limiting batches to 10,
/// wrapped by a part of a user's that forwards its role's methods and the
/// limits of what it holds, between a program of batches of 2 and bits
/// taking 2: the wrapper requires what its parts require in turn, the
/// interleaver's batches of 3, so the chain's check meets them at the
/// bits, the fault naming the wrapper, which required them. A joined
/// `requires` passing what arrives unchanged, as the default does, or
/// reading its lower part alone, composes instead.
#[test]
fn a_wrapped_joined_part_requires_what_its_parts_require() {
    let log = Log::default();
    let wrapped = Renamed::new(
        Stating::interleaving(3, &log).through(Stating::capping(10, &log)),
        "user's adapter",
    );
    assert_eq!(
        listed(
            faults(
                program(2, Features::unconstrained())
                    .through(wrapped)
                    .on(bits_over(2, &log))
            )
            .faults()
        ),
        listed(&[fault(
            PartId::new(1, "user's adapter"),
            part::<Bits>(2),
            short_batches(3, 2)
        )])
    );
}

// ----------------------------------------------------------- the message

/// A program through two encodings of one type, the upper requiring
/// larger batches than the lower provides: the fault tells them apart by
/// their positions, in its fields and its message.
#[test]
fn two_parts_of_one_type_are_told_apart_by_position() {
    let log = Log::default();
    let composed = program(1, Features::unconstrained())
        .through(Spread::new(2, &log).built_for(1))
        .through(Spread::new(2, &log).built_for(1))
        .on(bits_over(64, &log));
    let faults = faults(composed);
    assert_eq!(
        listed(faults.faults()),
        listed(&[fault(
            part::<Spread>(1),
            part::<Spread>(2),
            short_batches(2, 1)
        )])
    );
    let message = faults.to_string();
    let name = type_name::<Spread>();
    assert!(message.contains(&format!("`{name}` (part 1)")), "{message}");
    assert!(message.contains(&format!("`{name}` (part 2)")), "{message}");
}
