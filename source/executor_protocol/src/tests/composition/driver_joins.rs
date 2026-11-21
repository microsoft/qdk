// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a chain by the `on` that makes its runner: what each part
//! requires against what the part below it provides, and what it forwards
//! against the parts further below, wherever the brackets go; and what a
//! joined part requires and provides.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::composition::{faults, plain, records};
use crate::tests::fake_parts::{
    Bits, CoinCopy, CoinGates, Log, Pool, Profiler, Program, Renamed, ReplaysItself, Spread,
    Stating, in_each_bracketing, qubits,
};
use crate::{
    BlockType, Capabilities, CapabilitiesMismatch, CompositionFault, Feature, FeatureMismatch,
    Features, InstructionDef, InstructionSet, PartId, Resource, ResourceMismatch, ShotSet,
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

/// The mismatch of `required` qubits against `provided`.
fn short_qubits(required: u32, provided: u32) -> CapabilitiesMismatch {
    CapabilitiesMismatch {
        resources: vec![ResourceMismatch {
            key: Resource::Blocks(BlockType::qubit()),
            required,
            provided,
        }],
        ..CapabilitiesMismatch::default()
    }
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
/// chain's faults are `$expected`, as the row `$row`.
macro_rules! faults_in_each_bracketing {
    ($row:expr, [$($part:ident),+] from $setup:tt, $expected:expr $(,)?) => {
        let expected: Vec<CompositionFault> = $expected;
        in_each_bracketing!(
            [$($part),+] from $setup,
            |chain, brackets| assert_eq!(
                faults(chain).faults(),
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

// --------------------------------------------------------- forwarding

/// What an adapter does not state, it forwards: the requirement of the
/// part above it is checked against the parts below it, down to the first
/// that states it again, which is checked in its place, in each
/// bracketing. A profiler states nothing; a pool states the qubits it
/// draws; an ancilla's reservation states the qubits it needs.
#[test]
fn a_requirement_an_adapter_does_not_state_is_checked_against_the_parts_below() {
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
    // A program using readout parities, through a profiler, over bits
    // that resolve none.
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
    // A program on 9 qubits, through a part reserving an ancilla, which
    // requires 10, over bits providing 9: the part is named, not the
    // program, whose qubits it states again.
    faults_in_each_bracketing!(
        "qubits stated again",
        [d, a, e] from {
            let log = Log::default();
            let d = program(9, Features::new());
            let a = Stating::reserving_an_ancilla(9, &log);
            let e = bits_over(9, &log);
        },
        vec![fault(
            part::<Stating>(1),
            part::<Bits>(2),
            short_qubits(10, 9),
        )],
    );
    // 1 qubit and 3 carbon blocks, drawn from one pool of 7 qubits, over 5
    // qubits: the pool is named for its 7 qubits; the carbon blocks, which
    // the pool requires again, as none, are not checked below it.
    faults_in_each_bracketing!(
        "a pool",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, Features::new())
                .with_resources(qubits(1).with_blocks(Pool::carbon(), 3));
            let a = Pool::new(1, 3, &log);
            let e = bits_over(5, &log);
        },
        vec![fault(part::<Pool>(1), part::<Bits>(2), short_qubits(7, 5))],
    );
}

/// An adapter that provides more than the part below it cannot raise it:
/// a decoder restating 64 qubits, requiring none, over bits of 8, under a
/// program on 9: the program's qubits are checked against the bits too,
/// in each bracketing.
#[test]
fn a_provision_restated_above_cannot_raise_the_one_below() {
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

/// An adapter that states a feature both ways replaces it: what the part
/// above requires of it is checked against it alone, and what it requires
/// against the parts below. A lowering that cannot carry readout parities,
/// over bits that resolve them, refuses a program using them; an adapter
/// replaying its shots itself serves a debugger over bits whose shots are
/// not replayable, but requires seeded runs, and forwards the qubits it does
/// not state; a sampler drawing a run's shots together, which requires no
/// seeded runs below, and so no replay, provides no replay, and refuses a
/// debugger over bits whose shots are not replayable. Bracketed either way.
#[test]
fn an_adapter_stating_a_feature_both_ways_replaces_it() {
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
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = ReplaysItself::new(&log);
            let e = Bits::new(&log).with_features(not_replayable());
        },
        |chain, brackets| assert!(chain.is_ok(), "not replayable below, {brackets}"),
    );
    faults_in_each_bracketing!(
        "no seeded runs below",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = ReplaysItself::new(&log);
            let e = Bits::new(&log).with_features(not_replayable().with_seeded_runs(false));
        },
        vec![fault(
            part::<ReplaysItself>(1),
            part::<Bits>(2),
            missing(Feature::SeededRuns),
        )],
    );
    faults_in_each_bracketing!(
        "no qubit below",
        [d, a, e] from {
            let log = Log::default();
            let d = program(1, replay());
            let a = ReplaysItself::new(&log);
            let e = Bits::new(&log).with_resources(qubits(0));
        },
        vec![fault(part::<Program>(0), part::<Bits>(2), short_qubits(1, 0))],
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
/// lowest part requires, and what its upper parts require that the lowest
/// does not state; it provides what its top part provides, and what the
/// parts below provide that the top one does not require, the stricter
/// where both state it.
#[test]
fn a_joined_part_requires_and_provides_what_its_parts_forward() {
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
        // The encoding's 14 qubits, and the program's readout parities.
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
        // The ancilla's 9 qubits, not the bits' 64, which it requires
        // again; and the bits' parities.
        (
            "an ancilla's reservation on bits",
            read(Executor::provides(
                &Stating::reserving_an_ancilla(9, &log).on(Bits::new(&log)),
            )),
            (qubits(9), no_parity()),
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
