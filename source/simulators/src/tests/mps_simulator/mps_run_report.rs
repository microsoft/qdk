// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The run's report: the largest bond, the max bond dimension the engine is
//! given, and the warning when truncation lost more than the tolerance.

use std::num::NonZeroUsize;

use crate::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use tensornet::reference::ExactReference;

use super::programs::{branch, coins, gate1, gate2, mz, program, read_result, ret, set};
use super::reference_spy::{ReferenceSpy, Weight};
use super::{results, run};
use crate::bytecode::encoding::{OPID_CX, OPID_H};
use crate::mps_simulator::{
    DEFAULT_MAX_BOND_DIMENSION, MpsResumer, MpsRunReport, TRUNCATION_TOLERANCE,
};

/// A GHZ state on `qubits` qubits, each read at the end.
fn ghz(qubits: u32) -> QirProgram {
    let n = u64::from(qubits);
    let mut block = vec![gate1(OPID_H, 0)];
    block.extend((1..n).map(|qubit| gate2(OPID_CX, qubit - 1, qubit)));
    block.extend((0..n).map(mz));
    block.push(ret());
    program(qubits, &[&block])
}

/// Runs two shots of `program` on `resumer`, which must not fail.
fn run_once<E: tensornet::MpsEngine>(resumer: &mut MpsResumer<E>, program: &QirProgram) {
    expect_ready(resumer.run(&QirMachine::new(program), &QirShotId::first(2)))
        .expect("the program runs");
}

/// The report gives the largest bond any state of the run reached. Each
/// row is a program and the bond it reports: a GHZ state's bond of 2, read
/// before the readout projects it back to one; a Bell pair's bond of 2,
/// read before its first qubit is measured, not the bond of 1 a later gate
/// on the second qubit ends with; and a Bell pair made after the shot's
/// last measurement, and never read.
#[test]
fn the_largest_bond_is_the_largest_any_state_reached() {
    let rows = [
        ("a GHZ state, read", ghz(4)),
        (
            "a Bell pair, then a smaller bond read later",
            program(
                2,
                &[&[
                    gate1(OPID_H, 0),
                    gate2(OPID_CX, 0, 1),
                    mz(0),
                    gate1(OPID_H, 1),
                    ret(),
                ]],
            ),
        ),
        (
            "a Bell pair after the last measurement",
            program(
                3,
                &[&[mz(2), gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), ret()]],
            ),
        ),
    ];
    for (scene, program) in rows {
        let mut resumer = MpsResumer::new(ExactReference::new(), 7);
        run_once(&mut resumer, &program);
        assert_eq!(resumer.last_run().largest_bond, 2, "{scene}");
    }
}

/// The bond is read once per sequence of gates, where it ends, not before
/// every draw: a shot of a Bell pair, both qubits read, then an H gate,
/// reads it before the first measurement (none before the second, with no
/// gate between) and after the H, as its context ends.
#[test]
fn the_bond_is_read_once_per_gate_sequence() {
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    let program = program(
        2,
        &[&[
            gate1(OPID_H, 0),
            gate2(OPID_CX, 0, 1),
            mz(0),
            mz(1),
            gate1(OPID_H, 1),
            ret(),
        ]],
    );
    run(&mut resumer, &program, &QirShotId::first(1)).expect("the program runs");
    assert_eq!(resumer.engine().shapes(), 2);
}

/// A split adds no read: the group that waits holds a state whose bond was
/// read before the draw, with no gate since. Eight shots of a Bell
/// pair, both qubits read, then an H gate, split at the first measurement:
/// the bond is read once before it, then once per group after the H.
#[test]
fn a_split_reads_the_bond_of_neither_group_again_before_a_gate() {
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    let program = program(
        2,
        &[&[
            gate1(OPID_H, 0),
            gate2(OPID_CX, 0, 1),
            mz(0),
            mz(1),
            gate1(OPID_H, 1),
            ret(),
        ]],
    );
    run(&mut resumer, &program, &QirShotId::first(8)).expect("the program runs");
    assert_eq!(resumer.engine().forks(), 1, "the shots drew both values");
    assert_eq!(resumer.engine().shapes(), 3);
}

/// The report shows a bond above the max bond dimension, which an engine
/// that does not truncate holds: a GHZ state on the exact reference, with
/// χ set to 1, holds bonds of 2.
#[test]
fn a_bond_above_the_max_bond_dimension_on_an_exact_engine_is_shown() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7)
        .with_max_bond_dimension(NonZeroUsize::new(1).expect("not zero"));
    run_once(&mut resumer, &ghz(3));
    assert_eq!(resumer.last_run().largest_bond, 2);
}

/// A failed run's report keeps what the run read before the failure, and
/// what was read after it was met, by the groups of shots before the
/// failing one, which run on. Each row is a scene, whose report gives a
/// bond of 2:
/// - before: GHZ shots, on an engine whose projections on One fail, read
///   the bond of 2 before the readout, as a run that ends would;
/// - after: a coin, whose shots that read 1 trap, and whose shots that
///   read 0 then make a Bell pair. Of a shot that reads 0, one that reads
///   1, and another that reads 0, the shot that reads 1 goes on, the fewer,
///   and traps before any bond above 1 is read; the two that read 0 run
///   after it, as the first comes before it, and read their pair's bond.
#[test]
fn a_failed_run_reports_what_it_read_before_and_after_the_failure_was_met() {
    let mut before = MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 7);
    expect_ready(before.run(&QirMachine::new(&ghz(4)), &QirShotId::first(16)))
        .expect_err("some of 16 shots read One, whose projection fails");

    let (trap, rest) = (1, 2);
    let program = program(
        3,
        &[
            &[
                gate1(OPID_H, 0),
                mz(0),
                read_result(0, 0),
                branch(0, trap, rest),
            ],
            &[set(0, 2), branch(0, rest, rest)],
            &[gate1(OPID_H, 1), gate2(OPID_CX, 1, 2), ret()],
        ],
    );
    let coin = coins(1);
    let reading = |value: u8| {
        let coin = &coin;
        QirShotId::first(16).into_iter().filter(move |&shot| {
            run(
                &mut MpsResumer::new(ExactReference::new(), 5),
                coin,
                &[shot],
            )
            .expect("a coin runs")
                == [results(&[value])]
        })
    };
    let one = reading(1).next().expect("some of 16 coins read 1");
    let mut zeros = reading(0);
    let zero = zeros.next().expect("some of 16 coins read 0");
    let other_zero = zeros.next().expect("two of 16 coins read 0");
    let mut after = MpsResumer::new(ExactReference::new(), 5);
    run(&mut after, &program, &[zero, one, other_zero]).expect_err("the shot that reads 1 traps");

    for (scene, largest_bond) in [
        ("before", before.last_run().largest_bond),
        ("after", after.last_run().largest_bond),
    ] {
        assert_eq!(largest_bond, 2, "{scene}");
    }
}

/// A run's report is its own: a smaller run after a larger one reports its
/// own largest bond.
#[test]
fn each_run_reports_its_own_cost() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut resumer, &ghz(4));
    run_once(&mut resumer, &ghz(1));
    assert_eq!(resumer.last_run().largest_bond, 1);
}

/// A run of no shots reports what a run that prepared no state reports,
/// not the cost of the run before it: a caller that runs an empty batch
/// after a truncated run reads no stale warning.
#[test]
fn a_run_of_no_shots_reports_that_no_state_was_prepared() {
    let mut resumer = MpsResumer::new(ReferenceSpy::retaining(Weight::Fixed(0.99)), 7);
    run_once(&mut resumer, &ghz(4));
    assert!(
        resumer.last_run().truncation_warning().is_some(),
        "the first run lost 1% of its weight"
    );
    expect_ready(resumer.run(&QirMachine::new(&ghz(4)), &[])).expect("no shot fails");
    assert_eq!(
        *resumer.last_run(),
        MpsRunReport {
            largest_bond: 1,
            least_retained_weight: 1.0,
            max_bond_dimension: DEFAULT_MAX_BOND_DIMENSION.get(),
        }
    );
}

/// A run whose shots apply no gate reports a largest bond of one: a
/// program that only reads its qubits.
#[test]
fn a_run_of_no_gate_reports_a_largest_bond_of_one() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut resumer, &program(2, &[&[mz(0), mz(1), ret()]]));
    assert_eq!(resumer.last_run().largest_bond, 1);
}

/// The report keeps the least weight among the shots, whichever ends last:
/// two coins, each history's state keeping another weight (1 − 10⁻³ for
/// 0, 0 down to 1 − 4·10⁻³ for 1, 1), over four seeds.
#[test]
fn the_report_keeps_the_least_weight_among_the_shots() {
    for seed in 1..=4 {
        let mut resumer = MpsResumer::new(ReferenceSpy::retaining(Weight::ByBasisState), seed);
        let records = expect_ready(resumer.run(&QirMachine::new(&coins(2)), &QirShotId::first(16)))
            .expect("the program runs");
        let both_one = vec![crate::OutputRecord::Result(crate::MeasurementResult::One); 2];
        assert!(records.contains(&both_one), "some of 16 shots read 1, 1");
        let least = resumer.last_run().least_retained_weight;
        assert!((least - (1.0 - 4e-3)).abs() < 1e-15, "seed {seed}: {least}");
    }
}

/// The report gives the max bond dimension χ the run's states were
/// prepared within, and, before any run, the resumer's own. Each row is a
/// scene and the χ it reports: 128 by default and 1 when set, after a run;
/// and, before any run, the 16 a caller set, not the default.
#[test]
fn the_report_gives_the_max_bond_dimension_set_even_before_a_run() {
    let with_max_bond_dimension = |chi: usize| {
        MpsResumer::new(ExactReference::new(), 7)
            .with_max_bond_dimension(NonZeroUsize::new(chi).expect("not zero"))
    };
    let mut by_default = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut by_default, &ghz(3));
    let mut set_to_one = with_max_bond_dimension(1);
    run_once(&mut set_to_one, &ghz(3));
    let not_run = with_max_bond_dimension(16);
    let rows = [
        ("by default", &by_default, DEFAULT_MAX_BOND_DIMENSION.get()),
        ("set to 1", &set_to_one, 1),
        ("set to 16, before any run", &not_run, 16),
    ];
    for (scene, resumer, max_bond_dimension) in rows {
        assert_eq!(
            resumer.last_run().max_bond_dimension,
            max_bond_dimension,
            "{scene}"
        );
    }
}

/// A run warns when its shots lost more weight to truncation than the
/// tolerance, giving the weight lost (not the weight kept) and the max bond
/// dimension, and only then. Each row is a run's report, and the weight
/// lost it warns of: an exact run, which keeps all its weight, none; a run
/// that lost 1%, 1%; and a loss of exactly the tolerance, none.
#[test]
fn a_run_warns_only_when_it_lost_more_than_the_tolerance() {
    let mut exact = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut exact, &ghz(3));
    assert!(
        (exact.last_run().least_retained_weight - 1.0).abs() < 1e-12,
        "an exact run keeps all its weight"
    );
    let mut lossy = MpsResumer::new(ReferenceSpy::retaining(Weight::Fixed(0.99)), 7);
    run_once(&mut lossy, &ghz(3));
    let at_tolerance = MpsRunReport {
        largest_bond: 1,
        least_retained_weight: 1.0 - TRUNCATION_TOLERANCE,
        max_bond_dimension: 8,
    };
    let rows = [
        ("an exact run", *exact.last_run(), None),
        ("a run that lost 1%", *lossy.last_run(), Some(0.01)),
        ("a loss of exactly the tolerance", at_tolerance, None),
    ];
    for (scene, report, lost_weight) in rows {
        let warning = report.truncation_warning();
        assert_eq!(
            warning.is_some(),
            lost_weight.is_some(),
            "{scene}: {warning:?}"
        );
        if let (Some(warning), Some(lost_weight)) = (warning, lost_weight) {
            assert!((warning.lost_weight - lost_weight).abs() < 1e-15, "{scene}");
            assert_eq!(
                warning.max_bond_dimension,
                DEFAULT_MAX_BOND_DIMENSION.get(),
                "{scene}"
            );
            let message = warning.to_string();
            assert!(
                message.contains("max bond dimension 128"),
                "{scene}: {message}"
            );
            let numbers: Vec<f64> = message
                .split(|c: char| !(c.is_ascii_digit() || "+-.eE".contains(c)))
                .filter_map(|token| {
                    token
                        .trim_matches(|c: char| !c.is_ascii_digit())
                        .parse()
                        .ok()
                })
                .collect();
            let shows = |weight: f64| {
                numbers
                    .iter()
                    .any(|number| (number - weight).abs() <= 0.01 * weight)
            };
            assert!(
                shows(lost_weight) && !shows(1.0 - lost_weight),
                "{scene}: the message gives the weight lost, not the weight kept: {message}"
            );
        }
    }
}
