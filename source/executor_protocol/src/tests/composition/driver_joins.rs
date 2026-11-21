// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a chain by the `on` that makes its runner: what each part
//! requires against what the part below it provides, and against each part
//! further below its requirement reaches, wherever the brackets go; and
//! what a joined part requires and provides.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::composition::{faults, plain, records};
use crate::tests::fake_parts::{
    Bits, CoinCopy, CoinGates, Log, Pool, Profiler, Program, Renamed, ReplaysItself, Splitter,
    Spread, Stating, in_each_bracketing, qubits,
};
use crate::{
    BlockType, Capabilities, CapabilitiesMismatch, CompositionFault, CompositionFaults, Feature,
    FeatureMismatch, Features, InstructionDef, InstructionSet, PartId, Resource, ResourceMismatch,
    Resources, ShotSet,
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
fn part<P>(position: usize) -> PartId {
    PartId::new(position, type_name::<P>())
}

/// The fault of the part `requirer` against the part `provider`, which
/// falls short by `mismatch`.
fn fault(requirer: PartId, provider: PartId, mismatch: CapabilitiesMismatch) -> CompositionFault {
    CompositionFault {
        requirer,
        provider,
        mismatch,
    }
}

/// The mismatch of the instructions `sent` against `accepted`.
fn instructions(sent: &InstructionSet, accepted: &InstructionSet) -> CapabilitiesMismatch {
    CapabilitiesMismatch {
        instructions: sent.fits(accepted).expect_err("sets that do not fit"),
        ..CapabilitiesMismatch::default()
    }
}

/// The mismatch of `required` of `key` against `provided`.
fn short(key: Resource, required: u32, provided: u32) -> CapabilitiesMismatch {
    CapabilitiesMismatch {
        resources: vec![ResourceMismatch {
            key,
            required,
            provided,
        }],
        ..CapabilitiesMismatch::default()
    }
}

/// The mismatch of `required` qubits against `provided`.
fn short_qubits(required: u32, provided: u32) -> CapabilitiesMismatch {
    short(Resource::Blocks(BlockType::qubit()), required, provided)
}

/// The mismatch of `feature`, required and not provided.
fn missing(feature: Feature) -> CapabilitiesMismatch {
    CapabilitiesMismatch {
        features: vec![FeatureMismatch {
            key: feature,
            required: true,
            provided: false,
        }],
        ..CapabilitiesMismatch::default()
    }
}

/// The program measuring qubit 0, requiring `n` qubits and `features`.
fn program(n: u32, features: Features) -> Program {
    Program::new(&[("mz", 0)])
        .with_resources(qubits(n))
        .with_features(features)
}

/// Readout parities, required or provided.
fn parity() -> Features {
    Features::new().with_readout_parity(true)
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
            let e = Bits::providing(no_h(), &log);
        },
        expected(),
    );
    let log = Log::default();
    let profiler = |set| Profiler::on(set, &counted, &log);
    let mut lent_executor = profiler(no_x())
        .through(profiler(no_mz()))
        .on(Bits::providing(no_h(), &log));
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
                    .on(Bits::providing(no_h(), &log))
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
            let d = program(7, Features::new());
            let a = Spread::new(7, &log).built_for(7);
            let e = bits_over(30, &log);
        },
        vec![fault(part::<Spread>(1), part::<Bits>(2), short_qubits(49, 30))],
    );
}

// --------------------------------------------------------------- reach

/// A requirement reaches each part below it that states its resource or
/// feature, down to and including the first that serves it, and is checked
/// against each, in each bracketing; a part that states nothing of it
/// passes it through. A row per part a writer meets: a profiler, which
/// states nothing; a replayer, which states features, not qubits; an
/// interleaver, which needs qubits below for its own work, and passes them
/// through; a job queue, which limits a run's shots; a decoder restating
/// its qubits as a limit, above an ancilla's reservation, which serves the
/// qubits it lowers; a pool, which serves the qubits and carbon blocks it
/// draws; a part that needs batches of one instruction below, passing the
/// count through, and a splitter, which serves any count.
#[test]
#[expect(clippy::too_many_lines, reason = "one row per part a writer meets")]
fn a_requirement_reaches_each_part_below_down_to_the_first_that_serves_it() {
    let counted = Rc::new(Cell::new(0));
    // A program on 9 qubits, through a profiler, over bits providing 8.
    faults_in_each_bracketing!(
        "qubits through a profiler",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, Features::new());
            let a = Profiler::new(&counted, &log);
            let e = bits_over(8, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(9, 8))],
    );
    // A program using readout parities, or seeded runs, through a
    // profiler, over bits that provide neither.
    faults_in_each_bracketing!(
        "readout parities through a profiler",
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
        "seeded runs through a profiler",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, Features::new().with_seeded_runs(true));
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log).with_features(Features::new().with_seeded_runs(false));
        },
        vec![fault(
            part::<Program>(0),
            part::<Bits>(2),
            missing(Feature::SeededRuns),
        )],
    );
    // A debugger on 1 qubit, through an adapter replaying its shots
    // itself, over bits of none.
    faults_in_each_bracketing!(
        "qubits through a replayer",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, Features::new().with_replay(true));
            let a = ReplaysItself::new(&log);
            let e = Bits::new(&log).with_resources(qubits(0));
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(1, 0))],
    );
    // A program on 5 qubits, through an interleaver needing 3, over bits
    // providing 4: the program's 5 reach the bits.
    faults_in_each_bracketing!(
        "qubits through an interleaver",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::new());
            let a = Stating::requiring(qubits(3), &log);
            let e = bits_over(4, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(5, 4))],
    );
    // A program of runs of 5 shots, then of 12, through a queue of 10, over
    // bits of 3: the program is checked against both, and named at each
    // that falls short.
    let shots = |n| program(1, Features::new()).with_resources(qubits(1).with_shots(n));
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
    // A program on 6 qubits, through a decoder limiting them to 4, then a
    // part reserving an ancilla, which serves 6 and requires 7, over bits
    // of 5: the program falls short at the decoder, not past the ancilla's
    // reservation, which falls short at the bits.
    faults_in_each_bracketing!(
        "a limit above a part that serves",
        [d, c, w, e] from {
            let log = Log::default();
            let d = program(6, Features::new());
            let c = Stating::restating(4, &log);
            let w = Stating::reserving_an_ancilla(6, &log);
            let e = bits_over(5, &log);
        },
        vec![
            fault(part::<Program>(0), part::<Stating>(1), short_qubits(6, 4)),
            fault(part::<Stating>(2), part::<Bits>(3), short_qubits(7, 5)),
        ],
    );
    // A program on 9 qubits, through a part reserving an ancilla, which
    // requires 10, over bits providing 8: the part is named, not the
    // program, whose qubits it serves.
    faults_in_each_bracketing!(
        "qubits served",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, Features::new());
            let a = Stating::reserving_an_ancilla(9, &log);
            let e = bits_over(8, &log);
        },
        vec![fault(
            part::<Stating>(1),
            part::<Bits>(2),
            short_qubits(10, 8),
        )],
    );
    // 1 qubit and 3 carbon blocks, drawn from one pool of 7 qubits, over 5
    // qubits and no carbon block: the pool is named for its 7 qubits; the
    // carbon blocks, which the pool serves, are not checked below it.
    faults_in_each_bracketing!(
        "a pool",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, Features::new())
                .with_resources(qubits(1).with_blocks(Pool::carbon(), 3));
            let a = Pool::new(1, 3, &log);
            let e = Bits::new(&log)
                .with_resources(qubits(5).with_blocks(Pool::carbon(), 0))
                .resolving_parity();
        },
        vec![fault(part::<Pool>(1), part::<Bits>(2), short_qubits(7, 5))],
    );
    // A program of batches of 100 instructions, over bits taking 1: through
    // a part that needs batches of one below, the program's 100 reach the
    // bits; through a splitter, which serves any count, they stop there.
    let batches_of =
        |n| program(1, Features::new()).with_resources(qubits(1).with_instructions_per_batch(n));
    let bits_taking_1 =
        |log: &Log| Bits::new(log).with_resources(qubits(64).with_instructions_per_batch(1));
    faults_in_each_bracketing!(
        "instructions a batch past a part that requires them",
        [d, a, e] from {
            let log = Log::default();
            let d = batches_of(100);
            let a = Stating::requiring(Resources::new().with_instructions_per_batch(1), &log);
            let e = bits_taking_1(&log);
        },
        vec![fault(
            part::<Program>(0),
            part::<Bits>(2),
            short(Resource::InstructionsPerBatch, 100, 1),
        )],
    );
    faults_in_each_bracketing!(
        "instructions a batch through a splitter",
        [d, a, e] from {
            let log = Log::default();
            let d = batches_of(100);
            let a = Splitter::default();
            let e = bits_taking_1(&log);
        },
        vec![],
    );
    // A program on 5 qubits, through a profiler, over bits that state no
    // qubits: nothing limits them.
    faults_in_each_bracketing!(
        "qubits over an executor stating none",
        [d, a, e] from {
            let log = Log::default();
            let d = program(5, Features::new());
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log).with_resources(Resources::new());
        },
        vec![],
    );
}

/// A part of a user's holding an interleaver needing 3 qubits joined with
/// a decoder limiting them to 10, between a program on 5 qubits and bits
/// of 4: the program's qubits reach the bits through it, as they do
/// through the two parts unheld; and one holding an ancilla's reservation
/// joined with a profiler, under a program on 9 qubits, over bits of 8:
/// the program's qubits stop at it, as they stop at the reservation
/// unheld. A joined part keeps its parts' reach when held.
#[test]
fn a_held_joined_part_keeps_the_reach_of_its_parts() {
    fn held<A: Adapter>(part: A) -> Renamed<A> {
        Renamed::new(part, "user's adapter")
    }
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let interleaver = || Stating::requiring(qubits(3), &log);
    let decoder = || Stating::restating(10, &log);
    let users = PartId::new(1, "user's adapter");
    assert_eq!(
        faults(
            program(5, Features::new())
                .through(interleaver())
                .through(decoder())
                .on(bits_over(4, &log))
        )
        .faults(),
        [fault(
            part::<Program>(0),
            part::<Bits>(3),
            short_qubits(5, 4)
        )],
        "an interleaver and a decoder"
    );
    assert_eq!(
        faults(
            program(5, Features::new())
                .through(held(interleaver().through(decoder())))
                .on(bits_over(4, &log))
        )
        .faults(),
        [fault(
            part::<Program>(0),
            part::<Bits>(2),
            short_qubits(5, 4)
        )],
        "the two held"
    );
    let ancilla = || Stating::reserving_an_ancilla(9, &log);
    let profiler = || Profiler::new(&counted, &log);
    assert_eq!(
        faults(
            program(9, Features::new())
                .through(ancilla())
                .through(profiler())
                .on(bits_over(8, &log))
        )
        .faults(),
        [fault(
            part::<Stating>(1),
            part::<Bits>(3),
            short_qubits(10, 8)
        )],
        "an ancilla's reservation and a profiler"
    );
    assert_eq!(
        faults(
            program(9, Features::new())
                .through(held(ancilla().through(profiler())))
                .on(bits_over(8, &log))
        )
        .faults(),
        [fault(users, part::<Bits>(2), short_qubits(10, 8))],
        "the two held"
    );
}

/// A limit passes a requirement through, and cannot raise the limit below:
/// a decoder restating 64 qubits, requiring none, over bits of 8, under a
/// program on 9: the program's qubits are checked against the bits too,
/// in each bracketing.
#[test]
fn a_limit_cannot_raise_the_limit_below() {
    faults_in_each_bracketing!(
        "a decoder restating its qubits",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, Features::new());
            let a = Stating::restating(64, &log);
            let e = bits_over(8, &log);
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(9, 8))],
    );
}

/// A feature's requirement stops at the part that serves it, and passes a
/// part that limits it. A lowering that cannot carry readout parities, over
/// bits that resolve them, refuses a program using them; an adapter
/// replaying its shots itself serves a debugger replay over bits whose
/// shots are not replayable, but the seeded runs that replay states pass
/// it, as they pass its own requirement of them, to bits without them; a
/// sampler drawing a run's shots together serves seeded runs, but limits
/// replay to none, so a debugger's replay falls short there and at bits
/// whose shots are not replayable. Bracketed either way.
#[test]
fn a_feature_requirement_stops_at_the_part_that_serves_it() {
    let replay = || Features::new().with_replay(true);
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
    let not_replayable = || Features::new().with_replay(false);
    faults_in_each_bracketing!(
        "a replayer over shots not replayable",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = ReplaysItself::new(&log);
            let e = Bits::new(&log).with_features(not_replayable());
        },
        vec![],
    );
    faults_in_each_bracketing!(
        "no seeded runs below a replayer",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = ReplaysItself::new(&log);
            let e = Bits::new(&log).with_features(not_replayable().with_seeded_runs(false));
        },
        vec![
            fault(
                part::<Program>(0),
                part::<Bits>(2),
                missing(Feature::SeededRuns),
            ),
            fault(
                part::<ReplaysItself>(1),
                part::<Bits>(2),
                missing(Feature::SeededRuns),
            ),
        ],
    );
    faults_in_each_bracketing!(
        "a sampler over shots not replayable",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = Stating::seeding_itself(&log);
            let e = Bits::new(&log).with_features(not_replayable());
        },
        vec![
            fault(
                part::<Program>(0),
                part::<Stating>(1),
                missing(Feature::Replay),
            ),
            fault(
                part::<Program>(0),
                part::<Bits>(2),
                missing(Feature::Replay),
            ),
        ],
    );
}

/// A chain whose parts fall short in several places: every fault, by its
/// requirer top to bottom, then by its provider. 9 qubits and readout
/// parities, through a lowering without parities, over bits of 8 qubits:
/// the parities at the lowering and the qubits at the bits, both the
/// program's; then an encoding built for 1 qubit under one built for 3,
/// which requires 9.
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
            let d = program(3, Features::new());
            let a = Spread::new(3, &log).built_for(1);
            let b = Spread::new(1, &log).built_for(9);
            let e = bits_over(8, &log);
        },
        vec![
            fault(part::<Program>(0), part::<Spread>(1), short_qubits(3, 1)),
            fault(part::<Spread>(2), part::<Bits>(3), short_qubits(9, 8)),
        ],
    );
}

/// The ancilla's reservation over too few qubits, then the same with a
/// profiler above and below it: the same parts named, with the same
/// mismatch, at the positions the profilers shift.
#[test]
fn inserting_a_profiler_changes_only_the_positions_of_the_faults() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let profiler = || Profiler::new(&counted, &log);
    let ancilla = || Stating::reserving_an_ancilla(9, &log);
    let without = faults(
        program(9, Features::new())
            .through(ancilla())
            .on(bits_over(9, &log)),
    );
    let with = faults(
        program(9, Features::new())
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

/// A joined part, as the chain above it reads it: it requires what its
/// lowest part requires, and what its upper parts require that reaches
/// past it, the larger where both state it; it provides its top part's
/// provision over the parts below it, the smaller where both state it, down
/// to the first part that serves it, and served if one of its parts
/// serves it; and no replay where it provides no seeded runs.
#[test]
fn a_joined_part_states_what_its_parts_state_together() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let read = |capabilities: &Capabilities| {
        (
            capabilities.resources().clone(),
            capabilities.features().clone(),
        )
    };
    let no_parity = || Features::new().with_readout_parity(false);
    let rows = [
        // The encoding's 14 qubits, as it serves the program's 2, and the
        // program's readout parities.
        (
            "a driver through an encoding",
            read(Driver::requires(
                &program(2, parity()).through(Spread::new(7, &log).built_for(2)),
            )),
            (qubits(14), parity()),
        ),
        (
            "an encoding through a profiler",
            read(Adapter::requires(
                &Spread::new(7, &log)
                    .built_for(2)
                    .through(Profiler::new(&counted, &log)),
            )),
            (qubits(14), Features::new()),
        ),
        // The program's 5 qubits, larger than the interleaver's 3, which
        // it passes through.
        (
            "a driver through an interleaver",
            read(Driver::requires(
                &program(5, Features::new()).through(Stating::requiring(qubits(3), &log)),
            )),
            (qubits(5), Features::new()),
        ),
        // The queue's 4 shots, beside the bits' 8 qubits and their
        // parities, which the queue does not state.
        (
            "a queue on bits",
            read(Executor::provides(
                &Stating::queue(4, &log)
                    .on(Bits::new(&log).with_resources(qubits(8).with_shots(8))),
            )),
            (qubits(8).with_shots(4), no_parity()),
        ),
        // The ancilla's 9 qubits, served, not the bits' 64; and the bits'
        // parities.
        (
            "an ancilla's reservation on bits",
            read(Executor::provides(
                &Stating::reserving_an_ancilla(9, &log).on(Bits::new(&log)),
            )),
            (
                qubits(9).serving(Resource::Blocks(BlockType::qubit())),
                no_parity(),
            ),
        ),
        // Any count of instructions a batch, served, not the bits' 1.
        (
            "a splitter on bits",
            read(Executor::provides(&Splitter::default().on(
                Bits::new(&log).with_resources(qubits(64).with_instructions_per_batch(1)),
            ))),
            (
                qubits(64).serving(Resource::InstructionsPerBatch),
                no_parity(),
            ),
        ),
        // A decoder restating 64 qubits over a queue of 4 shots: the
        // decoder's qubits and the queue's shots.
        (
            "a decoder through a queue",
            read(Adapter::provides(
                &Stating::restating(64, &log).through(Stating::queue(4, &log)),
            )),
            (qubits(64).with_shots(4), Features::new()),
        ),
        // A replayer, which serves replay and limits seeded runs, on bits
        // without seeded runs: neither, as a shot is replayed with its
        // run's seed; replay stays served, a stop.
        (
            "a replayer on bits without seeded runs",
            read(Executor::provides(&Stating::replaying(&log).on(
                Bits::new(&log).with_features(Features::new().with_seeded_runs(false)),
            ))),
            (
                qubits(64),
                (Features::new().with_seeded_runs(false)).serving(Feature::Replay),
            ),
        ),
    ];
    for (row, read, (resources, features)) in rows {
        assert_eq!(read, (resources, features), "{row}");
    }
}

// ----------------------------------------------------------- the message

/// The shortages of different kinds, with each part named as one built at
/// run time is, by what it was built from: each fault names its parts by
/// those names, in its fields and in its message, beside every mismatch.
#[test]
fn a_composition_fault_names_its_parts_and_every_mismatch() {
    let log = Log::default();
    let composed = Renamed::new(program(9, parity()), "h2 program")
        .through(Renamed::new(
            Stating::lowering_without_parity(&log),
            "parity-free lowering",
        ))
        .on(Renamed::new(bits_over(8, &log), "qodec runtime"));
    let faults = faults(composed);
    let named = |position, name| PartId::new(position, name);
    let parts: Vec<_> = (faults.faults().iter())
        .map(|fault| (fault.requirer.clone(), fault.provider.clone()))
        .collect();
    assert_eq!(
        parts,
        [
            (named(0, "h2 program"), named(1, "parity-free lowering")),
            (named(0, "h2 program"), named(2, "qodec runtime")),
        ]
    );
    let message = faults.to_string();
    for said in [
        "`h2 program` (part 0)",
        "`parity-free lowering` (part 1)",
        "`qodec runtime` (part 2)",
        &missing(Feature::ReadoutParity).features[0].to_string(),
        &short_qubits(9, 8).resources[0].to_string(),
    ] {
        assert!(message.contains(said), "{said}: {message}");
    }
}

/// A program through two encodings of one type, the upper requiring more
/// qubits than the lower provides: the fault tells them apart by their
/// positions, in its fields and its message.
#[test]
fn two_parts_of_one_type_are_told_apart_by_position() {
    let log = Log::default();
    let composed = program(1, Features::new())
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
