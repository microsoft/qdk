// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Joining parts with `through` and `on`: each sender writing as its
//! binding to its receiver's set says, the laws of the brackets, and what
//! each part gets ready for on every run. The joins' check of a chain, what each part
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
use crate::tests::example_sets::Measuring;
use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, FromThread, Later, Log, NamesBelow, Profiler, Program, Renamed,
    Splitter, Spread, Stating, Trap, Values, batches_of, coin_gate, in_each_bracketing,
};
use crate::{CompositionFaults, Features, Resources, RunRequirement, Runner, ShotSet};

const SEED: u64 = 7;

/// The records of `shots` on the chain `composed` gave, which must compose.
pub(super) fn records<D: Driver, E: Executor<Accepts = D::Sends>>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> Vec<D::Output> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    runner.run(shots, SEED).expect("the run succeeds")
}

/// The faults of the chain `composed` gave, which must not compose.
pub(super) fn faults<D: Driver, E: Executor<Accepts = D::Sends>>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> CompositionFaults {
    composed.err().expect("the chain does not compose")
}

/// The records of [`CoinCopy`] on bits alone, for `shots`.
pub(super) fn plain(shots: &ShotSet) -> Vec<Values> {
    let log = Log::default();
    records(CoinCopy::new().on(Bits::new(&log)), shots)
}

// ------------------------------------------------------------ the binding

/// An executor whose set lists the driver's gates in another order: the
/// driver writes each gate as its binding to the executor's set says, so
/// the records are those on an executor of the driver's own set.
#[test]
fn a_sender_writes_as_its_binding_to_the_receivers_set_says() {
    let log = Log::default();
    let reordered = Measuring::new(&["mz", "x", "h"]);
    let shots = ShotSet::ids(0..16);
    for name in ["h", "mz"] {
        assert_ne!(reordered.gate(name, 0), coin_gate(name, 0), "another order");
    }
    let composed = CoinCopy::new().on(Bits::accepting(reordered, &log));
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
fn two_runs<D: Driver, E: Executor<Accepts = D::Sends>>(
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
    let one_instruction = || batches_of(1);
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
fn logged_over_two_runs<D: Driver, E: Executor<Accepts = D::Sends>>(
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

/// Parts over bits, in each bracketing the joins make a runner of, the
/// inner parts of each composed part included, over two runs: each part
/// gets ready, on every run and bottom up, for what arrives at it, which
/// each part above makes of what arrives at that part. Under a driver
/// sending batches of 2 instructions, through encodings of 2 and 3, they
/// come down as batches of 4, then 12. Under a program of 1 instruction,
/// through two encodings that state nothing, each part below it gets
/// ready for the program's batches of 1, which they pass. Through an
/// interleaver sending batches of 3 instructions at least, the bits get
/// ready for a program's batches of 5, or of 3 under a program of 2; over
/// bits that do not state instructions a batch, for the program's 5; and
/// for 30 through an encoding of 3 built for 10, under a program of 10.
#[test]
fn every_part_gets_ready_on_every_run_for_what_arrives_at_it() {
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
                "Bits ready for batches of 12".to_owned(),
                "Spread ready for batches of 4".to_owned(),
                "Spread ready for batches of 2".to_owned(),
                CoinCopy::ready_for(&run_of(4, true)),
            ]
            .iter()
            .cycle()
            .take(8)
            .cloned()
            .collect::<Vec<_>>(),
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
                "Bits ready for batches of 1",
                "Spread ready for batches of 1",
                "Spread ready for batches of 1",
            ]
            .repeat(2),
            "a requirement passing two encodings, {brackets}"
        ),
    );
    let ready_for = |n| vec![format!("Bits ready for batches of {n}"); 2];
    let program_of = |n| Program::new(&vec![("mz", 0); n]);
    for (program, ready) in [(5, 5), (2, 3)] {
        in_each_bracketing!(
            [d, a, e] from {
                let log = Log::default();
                let d = program_of(program);
                let a = Stating::interleaving(3, &log);
                let e = Bits::new(&log).with_resources(batches_of(6));
            },
            |chain, brackets| assert_eq!(
                logged_over_two_runs(chain, &log, "Bits ready"),
                ready_for(ready),
                "through an interleaver, under a program of {program}, {brackets}"
            ),
        );
    }
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program_of(5);
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log).with_resources(Resources::unconstrained());
        },
        |chain, brackets| assert_eq!(
            logged_over_two_runs(chain, &log, "Bits ready"),
            ready_for(5),
            "over bits not stating instructions a batch, {brackets}"
        ),
    );
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = program_of(10);
            let a = Spread::new(3, &log).built_for(10);
            let e = Bits::new(&log).with_resources(batches_of(40));
        },
        |chain, brackets| assert_eq!(
            logged_over_two_runs(chain, &log, "Bits ready"),
            ready_for(30),
            "through an encoding, {brackets}"
        ),
    );
}

/// What a run of `shots` shots requires, as its driver gets it: that many
/// shots, and seeded runs if `seeded`, its seed being fixed.
fn run_of(shots: u32, seeded: bool) -> RunRequirement {
    let features = Features::unconstrained();
    RunRequirement {
        resources: Resources::unconstrained().with_shots(shots),
        features: if seeded {
            features.with_seeded_runs(true)
        } else {
            features
        },
    }
}

/// What a logging [`CoinCopy`] at the top of `composed`, which must
/// compose, logs that it gets ready for, on a run of 3 shots with `seed`.
fn driver_ready_for<D: Driver, E: Executor<Accepts = D::Sends>>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    log: &Log,
    seed: Option<u64>,
) -> Vec<String> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    runner
        .run(&ShotSet::ids(0..3), seed)
        .expect("the run succeeds");
    log.entries()
        .into_iter()
        .filter(|entry| entry.starts_with("CoinCopy ready"))
        .collect()
}

/// A coin copy through an encoding of 2 built for its batches of 2, over
/// bits, on a run of 3 shots, seeded or not: the driver gets ready for its
/// run, what the run requires, however it is joined or wrapped: alone at
/// the top, joined with the encoding, and so joined and held by a part of
/// a user's. Neither for what it requires below (the run in batches of 2)
/// nor for what the encoding does (in batches of 4), nor for nothing
/// stated, as a runner
/// or a joined driver readying it with one of those would.
#[test]
fn a_driver_gets_ready_for_its_run_however_it_is_joined_or_wrapped() {
    for seed in [Some(SEED), None] {
        let expected = [CoinCopy::ready_for(&run_of(3, seed.is_some()))];
        in_each_bracketing!(
            [d, a, e] from {
                let log = Log::default();
                let d = CoinCopy::new().logging(&log);
                let a = Spread::new(2, &log).built_for(2);
                let e = Bits::new(&log);
            },
            |chain, brackets| assert_eq!(
                driver_ready_for(chain, &log, seed),
                expected,
                "{brackets}, seed {seed:?}"
            ),
        );
        let log = Log::default();
        let held = Renamed::new(
            CoinCopy::new()
                .logging(&log)
                .through(Spread::new(2, &log).built_for(2)),
            "user's driver",
        );
        assert_eq!(
            driver_ready_for(held.on(Bits::new(&log)), &log, seed),
            expected,
            "held by a part of a user's, seed {seed:?}"
        );
    }
}

/// One device's stack, an encoding on bits, kept across programs: lent to
/// a program of 1 instruction, then to one of 3, each run gets the bits
/// ready for what that run's program requires, which passes the encoding,
/// as it does not state instructions a batch.
#[test]
fn a_joined_part_gets_ready_for_what_each_chain_it_is_lent_to_requires() {
    let log = Log::default();
    let mut lent = Spread::new(1, &log).on(Bits::new(&log));
    for program in [Program::new(&[("mz", 0)]), Program::new(&[("mz", 0); 3])] {
        records(program.on(&mut lent), &ShotSet::ids(0..1));
    }
    let ready: Vec<_> = (log.entries().into_iter())
        .filter(|entry| entry.starts_with("Bits ready"))
        .collect();
    assert_eq!(
        ready,
        ["Bits ready for batches of 1", "Bits ready for batches of 3"]
    );
}

/// Two adapters joined, kept across devices: lent to a chain over bits
/// that take 2 instructions a batch, then to one over bits that take 1,
/// each adapter reads, on each run, what the part directly below it
/// provides: the lower one, that run's bits; the upper one, the lower
/// adapter's own limits, which do not state one, never the bits' through
/// it.
#[test]
fn an_adapter_reads_what_the_part_directly_below_provides_in_each_chain_it_is_lent_to() {
    let log = Log::default();
    let mut lent = NamesBelow::new(&log).through(NamesBelow::new(&log));
    for most in [2, 1] {
        let bits = Bits::new(&log).with_resources(batches_of(most));
        let program = Program::new(&[("mz", 0)]);
        records(program.through(&mut lent).on(bits), &ShotSet::ids(0..1));
    }
    let told: Vec<_> = (log.entries().into_iter())
        .filter(|entry| entry.starts_with("below takes"))
        .collect();
    let takes = |most: Option<u32>| format!("below takes {most:?} a batch");
    assert_eq!(
        told,
        [takes(None), takes(Some(2)), takes(None), takes(Some(1))]
    );
}
