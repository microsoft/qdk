// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Joining parts with `through` and `on`: each sender writing in its
//! receiver's opcodes, the laws of the brackets, and what each part gets
//! ready for and is told on every run, as composition read it. The check
//! of a chain, its fit and its admission, is in
//! `composition/chain_check.rs`, running its shots in
//! `composition/runner.rs`, the calls across its boundaries in
//! `composition/boundary_crossing.rs`, and its parts' panics in
//! `composition/part_panics.rs`.

mod boundary_crossing;
mod chain_check;
mod part_panics;
mod runner;

use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, CoinGates, FromThread, Later, Log, Profiler, Program, Shifting,
    Spread, Values, in_each_bracketing,
};
use crate::{BlockType, CompositionFaults, InstructionDef, InstructionSet, Needs, Runner, ShotSet};

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
/// each part gets ready, on every run and bottom up, for the needs that
/// reach it as the `on` that makes the runner read them. Under a driver of
/// 2 qubits, they come down as 4, then 12. Under a program stating 1 qubit,
/// then 3 once ready, which its rule forbids, each part below it still
/// gets ready for the 1 qubit the admission checked.
#[test]
fn every_part_gets_ready_on_every_run_for_the_needs_read_at_composition() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new().logging(&log);
            let a = Spread::new(2, &log);
            let b = Spread::new(3, &log);
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
            let d = Program::new(&[("mz", 0)])
                    .needing(Needs::qubits(1))
                    .needing_once_ready(Needs::qubits(3));
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
            "needs changed once ready, {brackets}"
        ),
    );
}

/// Two adapters whose offers narrow from 2 instructions a batch to 1 once
/// ready, which their rule forbids, over bits, under a program sending
/// batches of 2, unchecked or checked, in each of the five bracketings,
/// over two runs: every boundary checks each batch's count of
/// instructions against the offer the `on` that makes the runner read, and
/// the upper adapter is told that offer, so each run succeeds alike.
#[test]
fn every_boundary_answers_the_offer_read_at_composition() {
    for checked in [false, true] {
        in_each_bracketing!(
            [d, a, b, e] from {
                let log = Log::default();
                let program = Program::new(&[("x", 0), ("mz", 0)]);
                let d = if checked {
                    program.with_checked_batches()
                } else {
                    program
                };
                let a = Shifting::new(&log);
                let b = Shifting::new(&log);
                let e = Bits::new(&log);
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
