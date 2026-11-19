// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Composing parts with `>>`: the fit at each boundary, the laws of the
//! brackets, the fit and the admission reported together, and what each
//! part gets ready for and is told on every run, as composition read it.
//! The admission of a chain is in `composition/admission.rs`, running its
//! shots in `composition/runner.rs`, the calls across its boundaries in
//! `composition/boundary_crossing.rs`, and its parts' panics in
//! `composition/part_panics.rs`.

mod admission;
mod boundary_crossing;
mod part_panics;
mod runner;

use std::cell::Cell;
use std::rc::Rc;

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, CoinGates, FromThread, Later, Log, Profiler, Program, Shifting,
    Spread, Values, in_each_bracketing,
};
use crate::{
    Adapter, BlockType, CompositionFaults, Drive, Driver, Execute, Executor, FitFaults,
    InstructionDef, InstructionSet, Needs, Offer, Runner, ShotSet,
};

const SEED: u64 = 7;

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
fn no_x() -> InstructionSet {
    coin_gates_without("NoX", "x")
}

/// [`CoinGates`] without `mz`.
fn no_mz() -> InstructionSet {
    coin_gates_without("NoMz", "mz")
}

/// The records of `shots` on the chain `composed` gave, which must compose.
pub(super) fn records<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> Vec<D::Output> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    runner.run(shots, SEED).expect("the run succeeds")
}

/// The faults of the chain `composed` gave, which must not compose.
pub(super) fn faults<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> CompositionFaults {
    composed.err().expect("the chain does not compose")
}

/// The records of [`CoinCopy`] on bits alone, for `shots`.
pub(super) fn plain(shots: &ShotSet) -> Vec<Values> {
    let log = Log::default();
    records(
        Driver::new(CoinCopy::new()) >> Executor::new(Bits::new(&log)),
        shots,
    )
}

/// The fit faults of `sent` against `accepted`, as the fit finds them.
fn fit_faults(sent: &InstructionSet, accepted: &InstructionSet) -> FitFaults {
    FitFaults {
        sent: sent.name().to_owned(),
        accepted: accepted.name().to_owned(),
        mismatches: match sent.fits(accepted) {
            Ok(_) => panic!("a boundary that does not fit"),
            Err(mismatches) => mismatches,
        },
    }
}

// ---------------------------------------------------------------- the fit

/// A code level whose sets do not fit, at two boundaries, in each
/// bracketing: the runner is never made, and the fault holds each
/// boundary, top to bottom, with both sets named.
#[test]
fn every_boundary_that_does_not_fit_is_a_fault_of_the_chain() {
    let counted = Rc::new(Cell::new(0));
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(Profiler::on(no_x(), &counted, &log));
            let e = Executor::new(Bits::accepting(no_mz(), &log));
        },
        |chain, brackets| assert_eq!(
            faults(chain).fit(),
            [
                fit_faults(&CoinGates::set(), &no_x()),
                fit_faults(&no_x(), &no_mz()),
            ],
            "{brackets}"
        ),
    );
}

/// A join that does not make a runner keeps the faults of its boundary,
/// for its `check`; one that fits checks `Ok`.
#[test]
fn a_join_reports_the_faults_of_its_boundaries_on_check() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let fits = Adapter::new(Profiler::new(&counted, &log)) >> Executor::new(Bits::new(&log));
    assert_eq!(fits.check(), Ok(()));
    let driver = Driver::new(CoinCopy::new()) >> Adapter::new(Profiler::on(no_x(), &counted, &log));
    assert_eq!(
        driver.check(),
        Err(&[fit_faults(&CoinGates::set(), &no_x())][..])
    );
    let adapter = Adapter::new(Profiler::new(&counted, &log))
        >> Adapter::new(Profiler::on(no_x(), &counted, &log));
    assert_eq!(
        adapter.check(),
        Err(&[fit_faults(&CoinGates::set(), &no_x())][..])
    );
}

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
    let composed = Driver::new(CoinCopy::new()) >> Executor::new(Bits::accepting(reordered, &log));
    assert_eq!(records(composed, &shots), plain(&shots));
}

// ---------------------------------------------------------------- the laws

/// One driver, unchanged, on several builds of the chain below it: the
/// same records on each.
#[test]
fn one_driver_runs_on_every_chain_it_fits() {
    let log = Log::default();
    let shots = ShotSet::ids(0..32);
    let expected = plain(&shots);
    let driver = || Driver::new(CoinCopy::new());
    assert_eq!(
        records(driver() >> Executor::new(Later(Bits::new(&log))), &shots),
        expected,
        "answering later"
    );
    assert_eq!(
        records(
            driver() >> Executor::new(FromThread::new(Bits::new(&log), Default::default())),
            &shots
        ),
        expected,
        "answering from a thread"
    );
    assert_eq!(
        records(
            driver() >> (Adapter::new(Spread::new(2, &log)) >> Executor::new(Bits::new(&log))),
            &shots
        ),
        expected,
        "through an encoding"
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
        Driver::new(CoinCopy::new())
            >> Adapter::new(Profiler::new(&counted, &log))
            >> Adapter::new(BitFlip::new(0.3, &log))
            >> Executor::new(Bits::new(&log)),
        &shots,
    );
    in_each_bracketing!(
        [d, a, b, e] from {
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(Profiler::new(&counted, &log));
            let b = Adapter::new(BitFlip::new(0.3, &log));
            let e = Executor::new(Bits::new(&log));
        },
        |chain, brackets| assert_eq!(records(chain, &shots), expected, "{brackets}"),
    );
}

// --------------------------------------------------------- the faults together

/// A chain that does not fit and whose needs are unmet: both, at once, in
/// its fields and in its message.
#[test]
fn the_fit_and_the_admission_are_reported_together() {
    let log = Log::default();
    let composed = Driver::new(CoinCopy::new())
        >> Executor::new(Bits::accepting(no_mz(), &log).offering(Offer::unlimited().max_qubits(1)));
    let faults = faults(composed);
    let fit = fit_faults(&CoinGates::set(), &no_mz());
    assert_eq!(faults.fit(), std::slice::from_ref(&fit));
    let admission = faults.admission().expect("an unmet need");
    let message = faults.to_string();
    assert!(message.contains(&fit.to_string()), "{message}");
    assert!(message.contains(&admission.to_string()), "{message}");
}

// ------------------------------------------------- what each run is told

/// What `composed` logs over two runs, in order, of entries containing
/// `containing`.
fn logged_over_two_runs<D: Drive, E: Execute>(
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

/// Two encodings over bits, in each bracketing that `>>` makes a runner
/// of, the inner parts of each composed part included, over two runs:
/// each part gets ready, on every run and bottom up, for the needs that
/// reach it as the `>>` that makes the runner read them. Under a driver of
/// 2 qubits, they come down as 4, then 12. Under a program stating 1 qubit,
/// then 3 once ready, which its rule forbids, each part below it still
/// gets ready for the 1 qubit the admission checked.
#[test]
fn every_part_gets_ready_on_every_run_for_the_needs_read_at_composition() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Driver::new(CoinCopy::new().logging(&log));
            let a = Adapter::new(Spread::new(2, &log));
            let b = Adapter::new(Spread::new(3, &log));
            let e = Executor::new(Bits::new(&log));
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
            let d = Driver::new(
                Program::new(&[("mz", 0)])
                    .needing(Needs::qubits(1))
                    .needing_once_ready(Needs::qubits(3)),
            );
            let a = Adapter::new(Spread::new(1, &log));
            let b = Adapter::new(Spread::new(1, &log));
            let e = Executor::new(Bits::new(&log));
        },
        |chain, brackets| assert_eq!(
            logged_over_two_runs(chain, &log, "ready"),
            [
                "Bits ready for 1 qubits",
                "Spread ready for 1 qubits",
                "Spread ready for 1 qubits",
            ]
            .repeat(2),
            "needs changed once ready, {brackets}"
        ),
    );
}

/// Two adapters whose offers narrow from 2 instructions a batch to 1 once
/// ready, which their rule forbids, over bits, under a program sending
/// batches of 2, unchecked or checked, in each of the five bracketings,
/// over two runs: every boundary checks each batch's count of
/// instructions against the offer the `>>` that makes the runner read, and
/// the upper adapter is told that offer, so each run succeeds alike.
#[test]
fn every_boundary_answers_the_offer_read_at_composition() {
    for checked in [false, true] {
        in_each_bracketing!(
            [d, a, b, e] from {
                let log = Log::default();
                let program = Program::new(&[("x", 0), ("mz", 0)]);
                let d = Driver::new(if checked {
                    program.with_checked_batches()
                } else {
                    program
                });
                let a = Adapter::new(Shifting::new(&log));
                let b = Adapter::new(Shifting::new(&log));
                let e = Executor::new(Bits::new(&log));
            },
            |chain, brackets| assert_eq!(
                logged_over_two_runs(chain, &log, "is offered"),
                [
                    "Shifting is offered Some(2) instructions a batch",
                    "Shifting is offered None instructions a batch",
                ]
                .repeat(2),
                "{brackets}, checked: {checked}"
            ),
        );
    }
}
