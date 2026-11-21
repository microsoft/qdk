// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a chain by the `on` that makes its runner, wherever the
//! brackets go: the instructions each part sends against those the part
//! below accepts; what arrives at each part, what the part above requires
//! of what arrives there, against what it provides; and what a joined
//! part requires and provides.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::composition::runner::refused;
use crate::tests::composition::{faults, plain, records};
use crate::tests::fake_parts::{
    Bits, CoinCopy, CoinGates, Log, Pool, Profiler, Program, Renamed, Splitter, Spread, Stating,
    in_each_bracketing, qubits,
};
use crate::{
    Batch, BatchReadouts, BlockType, BoundaryMismatch, Capabilities, CapabilitiesMismatch,
    CompositionFault, CompositionFaults, ExecuteError, Feature, FeatureMismatch, Features,
    InstructionDef, InstructionSet, OpcodeMap, PartId, Resource, ResourceMismatch, Resources,
    RunRefusal, Seed, ShotSet,
};

/// The instructions of [`CoinGates`] but `missing`, as the set `name`.
fn coin_gates_without(name: &str, missing: &str) -> InstructionSet {
    let defs = [("h", 0), ("x", 0), ("mz", 1)]
        .into_iter()
        .filter(|&(mnemonic, _)| mnemonic != missing)
        .map(|(mnemonic, outcomes)| {
            InstructionDef::new(mnemonic)
                .acts_on([BlockType::qubit()])
                .outcomes(outcomes)
        });
    InstructionSet::new(name, [(BlockType::qubit(), 1)], defs).expect("a valid set")
}

/// [`CoinGates`] without `x`.
pub(super) fn no_x() -> InstructionSet {
    coin_gates_without("NoX", "x")
}

/// [`CoinGates`] without `mz`.
fn no_mz() -> InstructionSet {
    coin_gates_without("NoMz", "mz")
}

/// [`CoinGates`] without `h`.
fn no_h() -> InstructionSet {
    coin_gates_without("NoH", "h")
}

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

/// The mismatch of the instructions `sent` against `accepted`.
fn instructions(sent: &InstructionSet, accepted: &InstructionSet) -> BoundaryMismatch {
    BoundaryMismatch {
        instructions: sent.fits(accepted).expect_err("sets that do not fit"),
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

/// The mismatch of `required` of `key` against `provided`.
pub(super) fn short(key: Resource, required: u32, provided: u32) -> BoundaryMismatch {
    capabilities(CapabilitiesMismatch {
        resources: vec![ResourceMismatch {
            key,
            required,
            provided,
        }],
        ..CapabilitiesMismatch::default()
    })
}

/// The mismatch of `required` qubits against `provided`.
fn short_qubits(required: u32, provided: u32) -> BoundaryMismatch {
    short(Resource::Blocks(BlockType::qubit()), required, provided)
}

/// The mismatch of `feature`, required and not provided.
fn missing(feature: Feature) -> BoundaryMismatch {
    capabilities(CapabilitiesMismatch {
        features: vec![FeatureMismatch {
            key: feature,
            required: true,
            provided: false,
        }],
        ..CapabilitiesMismatch::default()
    })
}

/// The program measuring qubit 0, requiring `n` qubits and `features`.
fn program(n: u32, features: Features) -> Program {
    Program::new(&[("mz", 0)])
        .with_resources(qubits(n))
        .with_features(features)
}

/// Readout parities, required or provided.
fn parity() -> Features {
    Features::unconstrained().with_readout_parity(true)
}

/// Bits providing `n` qubits and resolving readout parities.
fn bits_over(n: u32, log: &Log) -> Bits {
    Bits::new(log).with_resources(qubits(n)).resolving_parity()
}

/// Checks, in each bracketing of the parts `$setup` binds, that the
/// chain's faults are `$expected`, as the row `$row`: none if it composes.
macro_rules! faults_in_each_bracketing {
    ($row:expr, [$($part:ident),+] from $setup:tt, $expected:expr $(,)?) => {
        let expected: Vec<CompositionFault> = $expected;
        in_each_bracketing!(
            [$($part),+] from $setup,
            |chain, brackets| assert_eq!(
                chain.err().as_ref().map_or(&[][..], CompositionFaults::faults),
                expected.as_slice(),
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
/// parts.
#[test]
fn every_boundary_that_does_not_fit_is_a_fault_of_the_chain() {
    let counted = Rc::new(Cell::new(0));
    let expected = || {
        vec![
            fault(
                part::<CoinCopy>(0),
                part::<Profiler>(1),
                instructions(&CoinGates::set(), &no_x()),
            ),
            fault(
                part::<Profiler>(1),
                part::<Profiler>(2),
                instructions(&no_x(), &no_mz()),
            ),
            fault(
                part::<Profiler>(2),
                part::<Bits>(3),
                instructions(&no_mz(), &no_h()),
            ),
        ]
    };
    faults_in_each_bracketing!(
        "sets",
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = Profiler::on(no_x(), &counted, &log);
            let b = Profiler::on(no_mz(), &counted, &log);
            let e = Bits::accepting(no_h(), &log);
        },
        expected(),
    );
    let log = Log::default();
    let profiler = |set| Profiler::on(set, &counted, &log);
    let mut lent_executor = profiler(no_x())
        .through(profiler(no_mz()))
        .on(Bits::accepting(no_h(), &log));
    let mut lent_adapter = profiler(no_x()).through(profiler(no_mz()));
    for chain in ["first", "second"] {
        assert_eq!(
            faults(CoinCopy::new().on(&mut lent_executor)).faults(),
            expected(),
            "a lent executor, {chain} chain"
        );
        assert_eq!(
            faults(
                CoinCopy::new()
                    .through(&mut lent_adapter)
                    .on(Bits::accepting(no_h(), &log))
            )
            .faults(),
            expected(),
            "a lent adapter, {chain} chain"
        );
    }
}

/// A program requiring exactly what the executor provides composes, and
/// its records are those on a larger executor.
#[test]
fn a_program_within_what_the_executor_provides_runs_unchanged() {
    let log = Log::default();
    let shots = ShotSet::ids(0..32);
    let exact = CoinCopy::new().on(Bits::new(&log).with_resources(qubits(2)));
    assert_eq!(records(exact, &shots), plain(&shots));
}

/// A program on 7 qubits, through an encoding of 7 qubits per qubit, over
/// 30 qubits: the `on` fails, naming the encoding, which requires 49, and
/// the bits, which provide 30.
#[test]
fn a_requirement_not_provided_is_a_fault_naming_both_parts() {
    faults_in_each_bracketing!(
        "an encoding",
        [d, a, e] from {
            let log = Log::default();
            let d = program(7, Features::unconstrained());
            let a = Spread::new(7, &log).built_for(7);
            let e = bits_over(30, &log);
        },
        vec![fault(part::<Spread>(1), part::<Bits>(2), short_qubits(49, 30))],
    );
}

/// A part is checked below for the instructions it sends, not for those
/// it accepts: a decomposer accepting [`CoinGates`] and sending them
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
            let a = Decomposer(no_x());
            let e = Bits::accepting(no_x(), &log);
        },
        vec![],
    );
    faults_in_each_bracketing!(
        "a decomposer over less than it sends",
        [d, a, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = Decomposer(no_x());
            let e = Bits::accepting(no_mz(), &log);
        },
        vec![fault(
            part::<Decomposer>(1),
            part::<Bits>(2),
            instructions(&no_x(), &no_mz()),
        )],
    );
}

/// A declaration fake of a decomposer: it accepts [`CoinGates`] and sends
/// the set it holds. It is never run.
struct Decomposer(InstructionSet);

impl Adapter for Decomposer {
    fn accepts(&self) -> &InstructionSet {
        crate::tests::fake_parts::coin_gates()
    }

    fn sends(&self) -> &InstructionSet {
        &self.0
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    async fn adapt(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        _below: &mut impl Executor,
        _readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        unreachable!("a declaration fake is never run")
    }
}

// ------------------------------------------------------- what arrives

/// A value a part passes below unchanged arrives at each part below it,
/// down to the executor, and is checked against what each provides, in
/// each bracketing, a fault naming the part that first required it. A row
/// per part a writer meets that passes it: a logger, which states
/// nothing; an interleaver, under a program larger than its least; a
/// decoder's fixed limit on qubits, and a job queue's on shots, each over
/// a smaller limit below; a lowering without readout parities; a
/// replayer, which passes the seeded runs replay states. And one where
/// nothing limits it: an executor stating nothing.
#[test]
#[expect(clippy::too_many_lines, reason = "one row per part a writer meets")]
fn a_requirement_passed_unchanged_is_checked_at_each_part_below_naming_its_requirer() {
    let counted = Rc::new(Cell::new(0));
    faults_in_each_bracketing!(
        "qubits through a logger",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Profiler::new(&counted, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(5, 4))],
    );
    faults_in_each_bracketing!(
        "seeded runs through a logger",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, Features::unconstrained().with_seeded_runs(true));
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log).with_features(Features::unconstrained().with_seeded_runs(false));
        },
        vec![fault(
            part::<Program>(0),
            part::<Bits>(2),
            missing(Feature::SeededRuns),
        )],
    );
    faults_in_each_bracketing!(
        "readout parities through a logger",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, parity());
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log);
        },
        vec![fault(
            part::<Program>(0),
            part::<Bits>(2),
            missing(Feature::ReadoutParity),
        )],
    );
    faults_in_each_bracketing!(
        "qubits through an interleaver, more than its least",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Stating::interleaving(3, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(5, 4))],
    );
    faults_in_each_bracketing!(
        "qubits over a decoder's limit",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Stating::capping(4, &log);
            let e = bits_over(10, &log);
        },
        vec![fault(part::<Program>(0), part::<Stating>(1), short_qubits(5, 4))],
    );
    faults_in_each_bracketing!(
        "qubits through a decoder's larger limit",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, Features::unconstrained());
            let a = Stating::capping(64, &log);
            let e = bits_over(8, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(9, 8))],
    );
    let shots = |n| program(1, Features::unconstrained()).with_resources(qubits(1).with_shots(n));
    let bits_of_3_shots = |log: &Log| Bits::new(log).with_resources(qubits(64).with_shots(3));
    faults_in_each_bracketing!(
        "shots through a queue",
        [d, a, e] from {
            let log = Log::default();
            let d = shots(5);
            let a = Stating::queue(10, &log);
            let e = bits_of_3_shots(&log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short(Resource::Shots, 5, 3))],
    );
    faults_in_each_bracketing!(
        "shots short at a queue and below it",
        [d, a, e] from {
            let log = Log::default();
            let d = shots(12);
            let a = Stating::queue(10, &log);
            let e = bits_of_3_shots(&log);
        },
        vec![
            fault(part::<Program>(0), part::<Stating>(1), short(Resource::Shots, 12, 10)),
            fault(part::<Program>(0), part::<Bits>(2), short(Resource::Shots, 12, 3)),
        ],
    );
    faults_in_each_bracketing!(
        "a lowering without readout parities",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, parity());
            let a = Stating::lowering_without_parity(&log);
            let e = bits_over(64, &log);
        },
        vec![fault(
            part::<Program>(0),
            part::<Stating>(1),
            missing(Feature::ReadoutParity),
        )],
    );
    let replay = || Features::unconstrained().with_replay(true);
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
            missing(Feature::SeededRuns),
        )],
    );
    faults_in_each_bracketing!(
        "qubits over an executor stating none",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log).with_resources(Resources::unconstrained());
        },
        vec![],
    );
}

/// What a part requires below, given what arrives at it, is what arrives
/// at the part below, checked there, a fault naming that part, in each
/// bracketing. A row per part a writer meets that requires otherwise: an
/// interleaver, under a program smaller than its least, over bits too
/// small or large enough; a reservation of 2 ancillas; a pool drawing
/// qubits and carbon blocks from qubits; a splitter, requiring batches of
/// one instruction below any batch; a replayer, requiring no replay
/// below; a sampler, requiring no seeded runs below and providing no
/// replay; and a decoder's limit above an ancilla's reservation, where
/// each part falls short of its own.
#[test]
#[expect(clippy::too_many_lines, reason = "one row per part a writer meets")]
fn what_a_part_requires_of_what_arrives_is_checked_below_naming_it() {
    for (bits, expected) in [
        (
            2,
            vec![fault(
                part::<Stating>(1),
                part::<Bits>(2),
                short_qubits(3, 2),
            )],
        ),
        (4, vec![]),
    ] {
        faults_in_each_bracketing!(
            format!("an interleaver of 3 over {bits} qubits, under 2"),
            [d, a, e] from {
                let log = Log::default();
                let d = program(2, Features::unconstrained());
                let a = Stating::interleaving(3, &log);
                let e = bits_over(bits, &log);
            },
            expected.clone(),
        );
    }
    faults_in_each_bracketing!(
        "ancillas",
        [d, a, e] from {
            let log = Log::default();
            let d = program(3, Features::unconstrained());
            let a = Stating::reserving_ancillas(2, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Stating>(1), part::<Bits>(2), short_qubits(5, 4))],
    );
    // 1 qubit and 3 carbon blocks, drawn from one pool of 7 qubits, over 5
    // qubits and no carbon block: the pool is named for its 7 qubits; it
    // requires no carbon block.
    faults_in_each_bracketing!(
        "a pool",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, Features::unconstrained())
                .with_resources(qubits(1).with_blocks(Pool::carbon(), 3));
            let a = Pool::new(&log);
            let e = Bits::new(&log)
                .with_resources(qubits(5).with_blocks(Pool::carbon(), 0))
                .resolving_parity();
        },
        vec![fault(part::<Pool>(1), part::<Bits>(2), short_qubits(7, 5))],
    );
    let batches_of = |n| {
        program(1, Features::unconstrained())
            .with_resources(qubits(1).with_instructions_per_batch(n))
    };
    let bits_taking = |most, log: &Log| {
        Bits::new(log).with_resources(qubits(64).with_instructions_per_batch(most))
    };
    faults_in_each_bracketing!(
        "a splitter over batches of one",
        [d, a, e] from {
            let log = Log::default();
            let d = batches_of(100);
            let a = Splitter::default();
            let e = bits_taking(1, &log);
        },
        vec![],
    );
    faults_in_each_bracketing!(
        "a splitter over no batch",
        [d, a, e] from {
            let log = Log::default();
            let d = batches_of(100);
            let a = Splitter::default();
            let e = bits_taking(0, &log);
        },
        vec![fault(
            part::<Splitter>(1),
            part::<Bits>(2),
            short(Resource::InstructionsPerBatch, 1, 0),
        )],
    );
    let replay = || Features::unconstrained().with_replay(true);
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
        "a sampler over shots not replayable",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = Stating::seeding_itself(&log);
            let e = Bits::new(&log).with_features(not_replayable());
        },
        vec![fault(
            part::<Program>(0),
            part::<Stating>(1),
            missing(Feature::Replay),
        )],
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
            fault(part::<Program>(0), part::<Stating>(1), short_qubits(6, 4)),
            fault(part::<Stating>(2), part::<Bits>(3), short_qubits(7, 5)),
        ],
    );
}

/// A chain whose parts fall short in several places: every fault, by its
/// requirer top to bottom, then by its provider. 9 qubits and readout
/// parities, through a lowering without parities, over bits of 8 qubits:
/// the parities at the lowering and the qubits at the bits, both the
/// program's; then an encoding built for 1 qubit, whose 9 qubits pass one
/// built for 9 unchanged to bits of 8.
#[test]
fn every_fault_is_listed_by_requirer_then_by_provider() {
    faults_in_each_bracketing!(
        "shortages of different kinds",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, parity());
            let a = Stating::lowering_without_parity(&log);
            let e = bits_over(8, &log);
        },
        vec![
            fault(
                part::<Program>(0),
                part::<Stating>(1),
                missing(Feature::ReadoutParity),
            ),
            fault(part::<Program>(0), part::<Bits>(2), short_qubits(9, 8)),
        ],
    );
    faults_in_each_bracketing!(
        "two short parts",
        [d, a, b, e] from {
            let log = Log::default();
            let d = program(3, Features::unconstrained());
            let a = Spread::new(3, &log).built_for(1);
            let b = Spread::new(1, &log).built_for(9);
            let e = bits_over(8, &log);
        },
        vec![
            fault(part::<Program>(0), part::<Spread>(1), short_qubits(3, 1)),
            fault(part::<Spread>(1), part::<Bits>(3), short_qubits(9, 8)),
        ],
    );
}

/// The reservation of an ancilla over too few qubits, then the same with
/// a profiler above and below it: the same parts named, with the same
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
        without.faults(),
        [fault(
            part::<Stating>(1),
            part::<Bits>(2),
            short_qubits(10, 9)
        )]
    );
    assert_eq!(
        with.faults(),
        [fault(
            part::<Stating>(2),
            part::<Bits>(4),
            short_qubits(10, 9)
        )]
    );
}

// ---------------------------------------------------------- the joined

/// A joined part, as its public methods state it: it requires what its
/// lower part requires of what its upper part requires of what arrives;
/// and it provides what its top part provides, as the inputs its parts
/// take together are not one value. Its inner limits are checked part by
/// part, by the chain's check (see
/// [`a_joined_part_is_checked_part_by_part_in_every_bracketing`]).
#[test]
fn a_joined_part_requires_its_parts_requirements_in_turn_and_provides_its_top_parts_limits() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let read = |capabilities: &Capabilities| {
        (
            capabilities.resources().clone(),
            capabilities.features().clone(),
        )
    };
    let two_qubits = Capabilities::UNCONSTRAINED.map_resources(|_| qubits(2));
    let rows = [
        // The encoding's 14 qubits for the program's 2, and the program's
        // readout parities.
        (
            "a driver through an encoding",
            read(&Driver::requires(
                &program(2, parity()).through(Spread::new(7, &log).built_for(2)),
                &Capabilities::UNCONSTRAINED,
            )),
            (qubits(14), parity()),
        ),
        (
            "an encoding through a profiler, requiring",
            read(&Adapter::requires(
                &Spread::new(7, &log)
                    .built_for(2)
                    .through(Profiler::new(&counted, &log)),
                &two_qubits,
            )),
            (qubits(14), Features::unconstrained()),
        ),
        (
            "a profiler through an encoding, providing",
            read(Adapter::provides(
                &Profiler::new(&counted, &log).through(Spread::new(7, &log).built_for(2)),
            )),
            (Resources::unconstrained(), Features::unconstrained()),
        ),
        // The interleaver's least, for a program smaller than it.
        (
            "a driver through an interleaver",
            read(&Driver::requires(
                &program(2, Features::unconstrained()).through(Stating::interleaving(3, &log)),
                &Capabilities::UNCONSTRAINED,
            )),
            (qubits(3), Features::unconstrained()),
        ),
        // The queue's 4 shots, not the bits' 8 qubits.
        (
            "a queue on bits",
            read(Executor::provides(
                &Stating::queue(4, &log)
                    .on(Bits::new(&log).with_resources(qubits(8).with_shots(8))),
            )),
            (
                Resources::unconstrained().with_shots(4),
                Features::unconstrained(),
            ),
        ),
        // The decoder's 64 qubits, not the queue's 4 shots.
        (
            "a decoder through a queue",
            read(Adapter::provides(
                &Stating::capping(64, &log).through(Stating::queue(4, &log)),
            )),
            (qubits(64), Features::unconstrained()),
        ),
    ];
    for (row, read, (resources, features)) in rows {
        assert_eq!(read, (resources, features), "{row}");
    }
}

/// An interleaver of 3 joined with a decoder limiting qubits to 10,
/// between a program on 5 qubits and bits of 4, in every bracketing: the
/// program's qubits arrive at the bits through both, as through the two parts
/// unjoined. Wrapped by a part of a user's that forwards only its role's
/// public methods, the joined part still passes them, as its `requires`
/// composes its parts'; but its decoder's limit of 4, under the program's
/// 5, is past what the chain's check sees through it: the chain composes,
/// and the joined part refuses every run at `ready`, naming the user's
/// part, with the mismatch.
#[test]
fn a_joined_part_is_checked_part_by_part_in_every_bracketing() {
    faults_in_each_bracketing!(
        "an interleaver and a decoder",
        [d, i, c, e] from {
            let log = Log::default();
            let d = program(5, Features::unconstrained());
            let i = Stating::interleaving(3, &log);
            let c = Stating::capping(10, &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(3), short_qubits(5, 4))],
    );
    let log = Log::default();
    let wrapped = |cap| {
        Renamed::new(
            Stating::interleaving(3, &log).through(Stating::capping(cap, &log)),
            "user's adapter",
        )
    };
    assert_eq!(
        faults(
            program(5, Features::unconstrained())
                .through(wrapped(10))
                .on(bits_over(4, &log))
        )
        .faults(),
        [fault(
            part::<Program>(0),
            part::<Bits>(2),
            short_qubits(5, 4)
        )],
        "the two wrapped, over bits of 4"
    );
    let Ok(mut runner) = program(5, Features::unconstrained())
        .through(wrapped(4))
        .on(bits_over(10, &log))
    else {
        panic!("a wrapped decoder's limit is past the chain's check");
    };
    let refusal = refused(runner.run(&ShotSet::ids(0..2), None));
    let RunRefusal::PartNotReady { part, not_ready } = &refusal else {
        panic!("the wrapped part refuses: {refusal:?}");
    };
    assert_eq!(*part, PartId::new(1, "user's adapter"), "{refusal:?}");
    assert_eq!(
        not_ready.why().downcast_ref::<BoundaryMismatch>(),
        Some(&short_qubits(5, 4)),
        "{refusal:?}"
    );
}

// ----------------------------------------------------------- the message

/// Faults of each kind, an instruction's and a resource's at one pair
/// included, with parts named by their types, or, as a part built at run
/// time is, by what it was built from: each fault names its parts by
/// those names, in its fields and in its message, beside every mismatch.
#[test]
fn a_composition_fault_names_its_parts_and_every_mismatch() {
    let log = Log::default();
    let named = |position, name| PartId::new(position, name);
    let x = instructions(&CoinGates::set(), &no_x()).instructions[0].to_string();
    let nine_qubits = short_qubits(9, 8).capabilities.resources[0].to_string();
    let unresolved = missing(Feature::ReadoutParity).capabilities.features[0].to_string();
    let rows = [
        (
            "an instruction",
            faults(CoinCopy::new().on(Bits::accepting(no_x(), &log))),
            vec![(part::<CoinCopy>(0), part::<Bits>(1))],
            vec![&x],
        ),
        (
            "qubits",
            faults(program(9, Features::unconstrained()).on(bits_over(8, &log))),
            vec![(part::<Program>(0), part::<Bits>(1))],
            vec![&nine_qubits],
        ),
        (
            "an instruction and qubits at one pair",
            faults(
                program(9, Features::unconstrained())
                    .on(Bits::accepting(no_x(), &log).with_resources(qubits(8))),
            ),
            vec![(part::<Program>(0), part::<Bits>(1))],
            vec![&x, &nine_qubits],
        ),
        (
            "parts built at run time",
            faults(
                Renamed::new(program(9, parity()), "h2 program")
                    .through(Renamed::new(
                        Stating::lowering_without_parity(&log),
                        "parity-free lowering",
                    ))
                    .on(Renamed::new(bits_over(8, &log), "qodec runtime")),
            ),
            vec![
                (named(0, "h2 program"), named(1, "parity-free lowering")),
                (named(0, "h2 program"), named(2, "qodec runtime")),
            ],
            vec![&unresolved, &nine_qubits],
        ),
    ];
    for (row, faults, parts, mismatches) in rows {
        let named: Vec<_> = (faults.faults().iter())
            .map(|fault| (fault.requirer.clone(), fault.provider.clone()))
            .collect();
        assert_eq!(named, parts, "{row}");
        let message = faults.to_string();
        let parts = parts
            .iter()
            .flat_map(|(requirer, provider)| [requirer, provider]);
        for said in parts
            .map(ToString::to_string)
            .chain(mismatches.into_iter().cloned())
        {
            assert!(message.contains(&said), "{row}: {said} in {message}");
        }
    }
}

/// A program through two encodings of one type, the upper requiring more
/// qubits than the lower provides: the fault tells them apart by their
/// positions, in its fields and its message.
#[test]
fn two_parts_of_one_type_are_told_apart_by_position() {
    let log = Log::default();
    let composed = program(1, Features::unconstrained())
        .through(Spread::new(2, &log).built_for(1))
        .through(Spread::new(2, &log).built_for(1))
        .on(bits_over(64, &log));
    let faults = faults(composed);
    assert_eq!(
        faults.faults(),
        [fault(
            part::<Spread>(1),
            part::<Spread>(2),
            short_qubits(2, 1)
        )]
    );
    let message = faults.to_string();
    let name = type_name::<Spread>();
    assert!(message.contains(&format!("`{name}` (part 1)")), "{message}");
    assert!(message.contains(&format!("`{name}` (part 2)")), "{message}");
}
