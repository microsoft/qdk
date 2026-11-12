// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The run's report: the largest bond, the max bond dimension the engine is
//! given, and the warning when truncation lost more than the tolerance.

use std::num::NonZeroUsize;

use crate::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use tensornet::reference::ExactReference;

use super::super::programs::{coins, gate1, gate2, mz, program, ret};
use super::super::reference_spy::{ReferenceSpy, Weight};
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

/// The report gives the largest bond the gates made, read before the
/// readout projects it back to one: a GHZ state's bond is 2.
#[test]
fn the_report_gives_the_largest_bond_before_the_readout() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut resumer, &ghz(4));
    assert_eq!(resumer.last_run().largest_bond, 2);
}

/// Gates after a shot's last measurement count in the largest bond: a
/// program that reads qubit 2, then entangles qubits 0 and 1 into a Bell
/// pair and ends without reading them, reports the pair's bond of 2.
#[test]
fn gates_after_the_last_measurement_count_in_the_largest_bond() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let program = program(
        3,
        &[&[mz(2), gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), ret()]],
    );
    run_once(&mut resumer, &program);
    assert_eq!(resumer.last_run().largest_bond, 2);
}

/// A failed run's report keeps what the run read before it failed: GHZ
/// shots, on an engine whose projections on One fail, report the bond of 2
/// read before the readout, as a run that ends would.
#[test]
fn a_failed_run_reports_what_it_read_before_it_failed() {
    let mut resumer = MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 7);
    expect_ready(resumer.run(&QirMachine::new(&ghz(4)), &QirShotId::first(16)))
        .expect_err("some of 16 shots read One, whose projection fails");
    assert_eq!(resumer.last_run().largest_bond, 2);
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
/// prepared within: 128 by default, else the one set.
#[test]
fn the_report_gives_the_max_bond_dimension_of_the_run() {
    for (max_bond_dimension, expected) in [(None, DEFAULT_MAX_BOND_DIMENSION.get()), (Some(1), 1)] {
        let mut resumer = MpsResumer::new(ExactReference::new(), 7);
        if let Some(max_bond_dimension) = max_bond_dimension {
            resumer = resumer
                .with_max_bond_dimension(NonZeroUsize::new(max_bond_dimension).expect("not zero"));
        }
        run_once(&mut resumer, &ghz(3));
        assert_eq!(resumer.last_run().max_bond_dimension, expected);
    }
}

/// An exact run, which keeps all its weight, warns of nothing.
#[test]
fn an_exact_run_warns_of_nothing() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut resumer, &ghz(3));
    assert!((resumer.last_run().least_retained_weight - 1.0).abs() < 1e-12);
    assert_eq!(resumer.last_run().truncation_warning(), None);
}

/// A run whose shots lost more weight to truncation than the tolerance
/// warns, giving the weight lost and the max bond dimension.
#[test]
fn a_run_that_lost_more_than_the_tolerance_warns() {
    let mut resumer = MpsResumer::new(ReferenceSpy::retaining(Weight::Fixed(0.99)), 7);
    run_once(&mut resumer, &ghz(3));
    let warning = resumer
        .last_run()
        .truncation_warning()
        .expect("1% is more than the tolerance");
    assert!((warning.lost_weight - 0.01).abs() < 1e-15);
    assert_eq!(warning.max_bond_dimension, DEFAULT_MAX_BOND_DIMENSION.get());
    assert!(
        warning.to_string().contains("max bond dimension 128"),
        "{warning}"
    );
}

/// A loss of exactly the tolerance does not warn.
#[test]
fn a_loss_of_exactly_the_tolerance_does_not_warn() {
    let report = MpsRunReport {
        least_retained_weight: 1.0 - TRUNCATION_TOLERANCE,
        ..MpsRunReport::new(8)
    };
    assert_eq!(report.truncation_warning(), None);
}
