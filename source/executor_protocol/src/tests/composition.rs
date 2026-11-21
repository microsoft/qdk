// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Joining parts with `through` and `on`: each sender writing in its
//! receiver's opcodes, the laws of the brackets, and what each part gets
//! ready for on every run. The joins' check of a chain, what each part
//! requires against what the parts below provide, is in
//! `composition/driver_joins.rs`, running its shots in
//! `composition/runner.rs`, the calls across its boundaries in
//! `composition/boundary_crossing.rs`, and its parts' panics in
//! `composition/part_panics.rs`.

mod boundary_crossing;
mod driver_joins;
mod part_panics;
mod runner;

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, CoinGates, FromThread, Later, Log, NamesBelow, Profiler, Program,
    Renamed, Splitter, Spread, Trap, Values, in_each_bracketing, qubits,
};
use crate::{BlockType, CompositionFaults, InstructionDef, InstructionSet, Runner, ShotSet};

const SEED: u64 = 7;

/// The records of `shots` on the chain `composed` gave, which must compose.
pub(super) fn records<D: Driver, E: Executor>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> Vec<D::Output> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    runner.run(shots, SEED).expect("the run succeeds")
}

/// The faults of the chain `composed` gave, which must not compose.
pub(super) fn faults<D: Driver, E: Executor>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> CompositionFaults {
    composed.err().expect("the chain does not compose")
}

/// The records of [`CoinCopy`] on bits alone, for `shots`.
pub(super) fn plain(shots: &ShotSet) -> Vec<Values> {
    let log = Log::default();
    records(CoinCopy::new().on(Bits::new(&log)), shots)
}

// ------------------------------------------------------------ the opcodes

/// An executor whose opcodes are in another order than the driver's: the
/// driver writes in the executor's opcodes, so the records are those on
/// an executor of the driver's own set.
#[test]
fn a_sender_writes_in_the_receivers_opcodes() {
    let log = Log::default();
    let reordered = InstructionSet::new(
        "CoinGates",
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("mz")
                .acts_on([BlockType::qubit()])
                .outcomes(1),
            InstructionDef::new("x").acts_on([BlockType::qubit()]),
            InstructionDef::new("h").acts_on([BlockType::qubit()]),
        ],
    )
    .expect("a valid set");
    let shots = ShotSet::ids(0..16);
    for (mnemonic, opcode) in [("h", CoinGates::H), ("mz", CoinGates::MZ)] {
        assert_ne!(reordered.opcode(mnemonic), Some(opcode), "another order");
    }
    let composed = CoinCopy::new().on(Bits::providing(reordered, &log));
    assert_eq!(records(composed, &shots), plain(&shots));
}

// ---------------------------------------------------------------- the laws

/// One driver, unchanged, on several builds of the chain below it,
/// joined parts lent by `&mut` among them: the same records on each.
#[test]
fn one_driver_runs_on_every_chain_it_fits() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let shots = ShotSet::ids(0..32);
    let expected = plain(&shots);
    let driver = CoinCopy::new;
    assert_eq!(
        records(driver().on(Later(Bits::new(&log))), &shots),
        expected,
        "answering later"
    );
    assert_eq!(
        records(
            driver().on(FromThread::new(Bits::new(&log), Default::default())),
            &shots
        ),
        expected,
        "answering from a thread"
    );
    assert_eq!(
        records(
            driver().on(Spread::new(2, &log).on(Bits::new(&log))),
            &shots
        ),
        expected,
        "through an encoding"
    );
    let mut lent_executor = Spread::new(2, &log).on(Bits::new(&log));
    assert_eq!(
        records(driver().on(&mut lent_executor), &shots),
        expected,
        "on a lent encoding over bits"
    );
    let mut lent_adapter = Profiler::new(&counted, &log).through(Spread::new(2, &log));
    assert_eq!(
        records(
            driver().through(&mut lent_adapter).on(Bits::new(&log)),
            &shots
        ),
        expected,
        "through a lent profiler over an encoding"
    );
}

/// A chain of a driver, a profiler, a noise and bits, in each of the five
/// bracketings of four parts: the same records.
#[test]
fn where_the_brackets_go_changes_no_record() {
    let shots = ShotSet::ids(0..32);
    let counted = Rc::new(Cell::new(0));
    let log = Log::default();
    let expected = records(
        CoinCopy::new()
            .through(Profiler::new(&counted, &log))
            .through(BitFlip::new(0.3, &log))
            .on(Bits::new(&log)),
        &shots,
    );
    in_each_bracketing!(
        [d, a, b, e] from {
            let d = CoinCopy::new();
            let a = Profiler::new(&counted, &log);
            let b = BitFlip::new(0.3, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| assert_eq!(records(chain, &shots), expected, "{brackets}"),
    );
}

/// The results of two runs on the chain `composed` gave, which must
/// compose, a run of shots 0 to 3 and one of shots 0 to 7, and all `log`
/// saw.
fn two_runs<D: Driver, E: Executor>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    log: &Log,
) -> (String, Vec<String>)
where
    D::Output: std::fmt::Debug,
{
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    let first = runner.run(&ShotSet::ids(0..4), SEED);
    let second = runner.run(&ShotSet::ids(0..8), SEED);
    (format!("{first:?}, then {second:?}"), log.entries())
}

/// Coin copies through two encodings, with a trap on shot 5 between them,
/// over bits, each join's result held by a part of a user's that forwards
/// only its role's public methods, as a user's wrapper does, at one depth
/// or two, or lent by `&mut` to a chain since dropped first: over a run
/// that succeeds and one the trap fails, the same results, the same
/// requirements each part gets ready for, and the same shots ended on each
/// part, in the same order, as the same chain unwrapped. So too for what
/// is provided across the boundary inside the held part: a splitter, which
/// takes batches of any length, over bits that take one instruction a
/// batch, under a driver or an adapter sending two; and what a part over
/// bits reads the bits provide.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
fn a_joined_part_runs_the_same_held_by_a_part_forwarding_its_role() {
    struct Parts {
        log: Log,
        d: CoinCopy,
        a: Spread,
        b: Trap,
        c: Spread,
        e: Bits,
    }
    let parts = || {
        let log = Log::default();
        Parts {
            d: CoinCopy::new().logging(&log),
            a: Spread::new(2, &log).built_for(2),
            b: Trap::on(5).logging(&log),
            c: Spread::new(3, &log).built_for(4),
            e: Bits::new(&log),
            log,
        }
    };
    let unwrapped = {
        let Parts { log, d, a, b, c, e } = parts();
        two_runs(d.through(a).through(b).through(c).on(e), &log)
    };
    let one_instruction = || qubits(64).with_instructions_per_batch(1);
    let provision_rows = [
        ("a driver through a splitter", {
            let [log, unwrapped_log] = [Log::default(), Log::default()];
            let held = Renamed::new(
                CoinCopy::new().through(Splitter::default()),
                "user's driver",
            );
            let bits = Bits::new(&log).with_resources(one_instruction());
            let unwrapped_bits = Bits::new(&unwrapped_log).with_resources(one_instruction());
            (
                two_runs(held.through(Spread::new(1, &log)).on(bits), &log),
                two_runs(
                    CoinCopy::new()
                        .through(Splitter::default())
                        .through(Spread::new(1, &unwrapped_log))
                        .on(unwrapped_bits),
                    &unwrapped_log,
                ),
            )
        }),
        ("an adapter through a splitter", {
            let [log, unwrapped_log] = [Log::default(), Log::default()];
            let held = Renamed::new(
                Spread::new(1, &log).through(Splitter::default()),
                "user's adapter",
            );
            let bits = Bits::new(&log).with_resources(one_instruction());
            let unwrapped_bits = Bits::new(&unwrapped_log).with_resources(one_instruction());
            (
                two_runs(CoinCopy::new().through(held).on(bits), &log),
                two_runs(
                    CoinCopy::new()
                        .through(Spread::new(1, &unwrapped_log))
                        .through(Splitter::default())
                        .on(unwrapped_bits),
                    &unwrapped_log,
                ),
            )
        }),
        ("an adapter reading what bits provide", {
            let [log, unwrapped_log] = [Log::default(), Log::default()];
            let held = Renamed::new(NamesBelow::new(&log).on(Bits::new(&log)), "user's executor");
            (
                two_runs(CoinCopy::new().on(held), &log),
                two_runs(
                    CoinCopy::new()
                        .through(NamesBelow::new(&unwrapped_log))
                        .on(Bits::new(&unwrapped_log)),
                    &unwrapped_log,
                ),
            )
        }),
    ];
    let rows = [
        ("a driver through an adapter", {
            let Parts { log, d, a, b, c, e } = parts();
            let held = Renamed::new(d.through(a), "user's driver");
            two_runs(held.through(b).through(c).on(e), &log)
        }),
        ("an adapter through adapters", {
            let Parts { log, d, a, b, c, e } = parts();
            let held = Renamed::new(a.through(b).through(c), "user's adapter");
            two_runs(d.through(held).on(e), &log)
        }),
        ("an adapter on an executor", {
            let Parts { log, d, a, b, c, e } = parts();
            let held = Renamed::new(c.on(e), "user's executor");
            two_runs(d.through(a).through(b).on(held), &log)
        }),
        ("held at two depths", {
            let Parts { log, d, a, b, c, e } = parts();
            let inner = Renamed::new(d.through(a), "user's inner driver");
            let held = Renamed::new(inner.through(b), "user's outer driver");
            two_runs(held.through(c).on(e), &log)
        }),
        ("lent to a chain since dropped first", {
            let Parts { log, d, a, b, c, e } = parts();
            let mut lent = c.on(e);
            records(
                Program::new(&[("mz", 0)]).on(&mut lent),
                &ShotSet::ids(0..1),
            );
            let earlier_entries = log.entries().len();
            let held = Renamed::new(&mut lent, "user's executor");
            let (results, entries) = two_runs(d.through(a).through(b).on(held), &log);
            (results, entries[earlier_entries..].to_vec())
        }),
    ];
    for (row, held) in rows {
        assert_eq!(held, unwrapped, "{row}");
    }
    for (row, (held, unwrapped)) in provision_rows {
        assert_eq!(held, unwrapped, "{row}");
    }
}

/// Each role's part joined over another, as its users name it in their
/// logs and reports: it answers the name of its top part, the one its
/// caller wrote first.
#[test]
fn a_joined_part_is_named_for_its_top_part() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    for (row, named, top) in [
        (
            "a driver through an adapter",
            Driver::name(&CoinCopy::new().through(Spread::new(2, &log))).to_owned(),
            type_name::<CoinCopy>(),
        ),
        (
            "an adapter through an adapter",
            Adapter::name(&Profiler::new(&counted, &log).through(Spread::new(2, &log))).to_owned(),
            type_name::<Profiler>(),
        ),
        (
            "an adapter on an executor",
            Executor::name(&Spread::new(2, &log).on(Bits::new(&log))).to_owned(),
            type_name::<Spread>(),
        ),
    ] {
        assert_eq!(named, top, "{row}");
    }
}

// ------------------------------------------------- what each run is told

/// What `composed` logs over two runs, in order, of entries containing
/// `containing`.
fn logged_over_two_runs<D: Driver, E: Executor>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    log: &Log,
    containing: &str,
) -> Vec<String> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    for _ in 0..2 {
        runner
            .run(&ShotSet::ids(0..4), SEED)
            .expect("the run succeeds");
    }
    log.entries()
        .into_iter()
        .filter(|entry| entry.contains(containing))
        .collect()
}

/// Two encodings over bits, in each bracketing the joins make a runner
/// of, the inner parts of each composed part included, over two runs:
/// each part gets ready, on every run and bottom up, for what the part
/// above it requires. Under a driver of 2 qubits, through encodings built
/// for 2 and 4 qubits, they come down as 4, then 12. Under a program of 1
/// qubit, through two encodings that state no qubits, each part below it
/// gets ready for the 1 qubit they forward.
#[test]
fn every_part_gets_ready_on_every_run_for_what_the_part_above_requires() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new().logging(&log);
            let a = Spread::new(2, &log).built_for(2);
            let b = Spread::new(3, &log).built_for(4);
            let e = Bits::new(&log);
        },
        |chain, brackets| assert_eq!(
            logged_over_two_runs(chain, &log, "ready"),
            [
                "Bits ready for 12 qubits",
                "Spread ready for 4 qubits",
                "Spread ready for 2 qubits",
                "CoinCopy ready",
            ]
            .repeat(2),
            "two encodings, {brackets}"
        ),
    );
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]);
            let a = Spread::new(1, &log);
            let b = Spread::new(1, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| assert_eq!(
            logged_over_two_runs(chain, &log, "ready"),
            [
                "Bits ready for 1 qubits",
                "Spread ready for 1 qubits",
                "Spread ready for 1 qubits",
            ]
            .repeat(2),
            "a requirement forwarded, {brackets}"
        ),
    );
}

/// One device's stack, an encoding on bits, kept across programs: lent to
/// a program of 1 qubit, then to one of 3, each run gets the bits ready
/// for what that run's program requires, forwarded through the encoding,
/// which states no qubits.
#[test]
fn a_joined_part_gets_ready_for_what_each_chain_it_is_lent_to_requires() {
    let log = Log::default();
    let mut lent = Spread::new(1, &log).on(Bits::new(&log));
    for program in [Program::new(&[("mz", 0)]), Program::new(&[("mz", 2)])] {
        records(program.on(&mut lent), &ShotSet::ids(0..1));
    }
    let ready: Vec<_> = (log.entries().into_iter())
        .filter(|entry| entry.starts_with("Bits ready"))
        .collect();
    assert_eq!(
        ready,
        ["Bits ready for 1 qubits", "Bits ready for 3 qubits"]
    );
}

/// Two adapters joined, kept across devices: lent to a chain over bits
/// that take 2 instructions a batch, then to one over bits that take 1,
/// the upper adapter is told, on each run, what that run's bits provide,
/// forwarded through the lower adapter, which states no count.
#[test]
fn a_joined_adapter_is_told_what_each_chain_it_is_lent_to_provides() {
    let log = Log::default();
    let mut lent = NamesBelow::new(&log).through(NamesBelow::new(&log));
    for most in [2, 1] {
        let bits = Bits::new(&log).with_resources(qubits(1).with_instructions_per_batch(most));
        let program = Program::new(&[("mz", 0)]);
        records(program.through(&mut lent).on(bits), &ShotSet::ids(0..1));
    }
    let told: Vec<_> = (log.entries().into_iter())
        .filter(|entry| entry.starts_with("below takes"))
        .collect();
    let takes = |most| format!("below takes Some({most}) a batch");
    assert_eq!(told, [takes(2), takes(2), takes(1), takes(1)]);
}
