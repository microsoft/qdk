// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The run's report: the largest bond dimension, the least weight kept by
//! the shots that ended, the max bond dimension the engine is given, and
//! the warning when truncation lost more than the tolerance.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use tensornet::reference::ExactReference;

use crate::bytecode::encoding::{OPID_CX, OPID_H, OPID_X};
use crate::mps_simulator::test_utils::programs::{
    branch, gate1, gate2, mz, program, read_result, ret, set,
};
use crate::mps_simulator::test_utils::reference_spy::{ReferenceSpy, Weight};
use crate::mps_simulator::test_utils::run;
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

/// The report gives the largest bond dimension read where a sequence of
/// gates ends, before the measurements after it, even above the max bond
/// dimension. Each row is a program, the max bond dimension χ, and the
/// bond dimension it reports: a GHZ state's bond of 2, read before the
/// readout projects it back to one; a Bell pair's bond of 2, read before
/// its first qubit is measured, not the bond of 1 a later gate on the
/// second qubit ends with; a Bell pair made after the shot's last
/// measurement, its qubits never measured, its bond read as the shot's
/// context ends; and a GHZ state with χ set to 1, whose bonds of 2 the
/// exact reference, which does not truncate, holds.
#[test]
fn the_largest_bond_dimension_is_the_largest_read_where_a_gate_sequence_ends() {
    let rows = [
        ("a GHZ state, read", ghz(4), DEFAULT_MAX_BOND_DIMENSION),
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
            DEFAULT_MAX_BOND_DIMENSION,
        ),
        (
            "a Bell pair after the last measurement",
            program(
                3,
                &[&[mz(2), gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), ret()]],
            ),
            DEFAULT_MAX_BOND_DIMENSION,
        ),
        (
            "a GHZ state above χ",
            ghz(3),
            NonZeroUsize::new(1).expect("not zero"),
        ),
    ];
    for (scene, program, max_bond_dimension) in rows {
        let mut resumer =
            MpsResumer::new(ExactReference::new(), 7).with_max_bond_dimension(max_bond_dimension);
        run_once(&mut resumer, &program);
        assert_eq!(resumer.last_run().largest_bond_dimension, 2, "{scene}");
    }
}

/// A failed run's report keeps what the run read before the failure: GHZ
/// shots, on an engine whose projections on One fail, read the bond of 2
/// before the readout, as a run that ends would.
#[test]
fn a_failed_run_reports_what_it_read_before_the_failure() {
    let mut resumer = MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 7);
    expect_ready(resumer.run(&QirMachine::new(&ghz(4)), &QirShotId::first(16)))
        .expect_err("some of 16 shots read One, whose projection fails");
    assert_eq!(resumer.last_run().largest_bond_dimension, 2);
}

/// A run's report is its own, not the cost of the run before it, whether
/// the run ends, fails, or prepares no state; before any run, it is that
/// of a run of no state, at the resumer's χ. Each scene is a report of no
/// bond above 1 and no lost weight, at its χ:
/// - before any run, with χ set to 16: a caller reads the χ it set, not
///   the default;
/// - after a first run that reports a bond of 2 or a lost weight, a second
///   run's report:
///   - a smaller run after a larger one, and a run of no gate after a GHZ
///     state, at the default χ;
///   - a stop after a run that lost 1% of its weight, at the default χ;
///   - a max bond dimension that makes no chain of 64 qubits, after a
///     Bell pair, at that χ;
///   - a run of no shots, and χ set to 16, after a run that lost 1% of its
///     weight, at the default χ and 16: a caller that runs an empty batch,
///     or sets χ for the next run, after a truncated run reads no stale
///     warning.
#[test]
fn each_run_reports_its_own_cost() {
    let default_chi = DEFAULT_MAX_BOND_DIMENSION.get();
    let no_gate = program(2, &[&[mz(0), mz(1), ret()]]);
    let rows = [
        ("before any run, χ set to 16", report_before_any_run(16), 16),
        (
            "a smaller run",
            report_after_a_ghz_run(&ghz(1)),
            default_chi,
        ),
        (
            "a run of no gate",
            report_after_a_ghz_run(&no_gate),
            default_chi,
        ),
        ("a stop", report_of_a_stop_after_a_lossy_run(), default_chi),
        (
            "no chain",
            report_of_no_chain_after_a_bell_pair(),
            usize::MAX,
        ),
        (
            "no shots",
            report_of_no_shots_after_a_lossy_run(),
            default_chi,
        ),
        ("χ set to 16", report_of_chi_set_after_a_lossy_run(16), 16),
    ];
    for (scene, report, max_bond_dimension) in rows {
        assert_eq!(
            (report.largest_bond_dimension, report.max_bond_dimension),
            (1, max_bond_dimension),
            "{scene}"
        );
        assert!(
            (report.least_retained_weight - 1.0).abs() < ROUNDING,
            "{scene}: {report:?}"
        );
    }
}

/// The rounding by which an exact run's weight differs from 1 at most,
/// from its gates' floating-point arithmetic.
const ROUNDING: f64 = 1e-12;

/// The spy's engine, keeping 99% of each state's weight, as an engine that
/// truncates would.
fn losing_one_percent() -> ReferenceSpy {
    ReferenceSpy::retaining(Weight::Fixed(0.99))
}

/// Runs a GHZ state on `resumer`, whose engine is [`losing_one_percent`],
/// and checks the run warns: a report the next run must not keep.
fn run_lossy(resumer: &mut MpsResumer<ReferenceSpy>) {
    run_once(resumer, &ghz(4));
    assert!(
        resumer.last_run().truncation_warning().is_some(),
        "the first run lost 1% of its weight"
    );
}

/// The report of a run of `second` after a run of a GHZ state, whose bond
/// of 2 the second run must not keep.
fn report_after_a_ghz_run(second: &QirProgram) -> MpsRunReport {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut resumer, &ghz(4));
    assert_eq!(resumer.last_run().largest_bond_dimension, 2, "a GHZ bond");
    run_once(&mut resumer, second);
    *resumer.last_run()
}

/// The report of a run the stop flag stops, after a lossy run.
fn report_of_a_stop_after_a_lossy_run() -> MpsRunReport {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut resumer =
        MpsResumer::new(losing_one_percent(), 7).with_stop_flag(Arc::clone(&stop_flag));
    run_lossy(&mut resumer);
    stop_flag.store(true, Ordering::Relaxed);
    run(&mut resumer, &ghz(4), &QirShotId::first(2)).expect_err("the flag stops the run");
    *resumer.last_run()
}

/// The report of a run of 64 qubits whose max bond dimension (unbounded)
/// makes no chain, after a Bell pair at that χ, whose bond of 2 it must
/// not keep.
fn report_of_no_chain_after_a_bell_pair() -> MpsRunReport {
    let mut resumer =
        MpsResumer::new(ExactReference::new(), 7).with_max_bond_dimension(NonZeroUsize::MAX);
    run_once(&mut resumer, &ghz(2));
    assert_eq!(
        resumer.last_run().largest_bond_dimension,
        2,
        "a Bell pair's bond"
    );
    run(
        &mut resumer,
        &program(64, &[&[gate1(OPID_H, 0), ret()]]),
        &QirShotId::first(2),
    )
    .expect_err("64 qubits with unbounded bonds make no chain");
    *resumer.last_run()
}

/// The report of a run of no shots, after a lossy run: a caller that runs
/// an empty batch reads no stale warning.
fn report_of_no_shots_after_a_lossy_run() -> MpsRunReport {
    let mut resumer = MpsResumer::new(losing_one_percent(), 7);
    run_lossy(&mut resumer);
    expect_ready(resumer.run(&QirMachine::new(&ghz(4)), &[])).expect("no shot fails");
    *resumer.last_run()
}

/// The report of a resumer whose max bond dimension is set to `chi`, before
/// any run.
fn report_before_any_run(chi: usize) -> MpsRunReport {
    *MpsResumer::new(ExactReference::new(), 7)
        .with_max_bond_dimension(NonZeroUsize::new(chi).expect("not zero"))
        .last_run()
}

/// The report of a resumer whose max bond dimension is set to `chi` after a
/// lossy run: a caller that sets χ for the next run reads no stale warning.
fn report_of_chi_set_after_a_lossy_run(chi: usize) -> MpsRunReport {
    let mut resumer = MpsResumer::new(losing_one_percent(), 7);
    run_lossy(&mut resumer);
    let resumer = resumer.with_max_bond_dimension(NonZeroUsize::new(chi).expect("not zero"));
    *resumer.last_run()
}

/// The least weight is read from the shots that ended, and only from them: a
/// shot that fails reaches no end at which its weight is read (the least kept
/// whichever group ends last, and a group that ends in a run that failed
/// reading its own: `mps_simulator/grouped_run/tests.rs`). Each row is a run of
/// two shots on an engine whose every state keeps [`KEPT_WEIGHT`], and the
/// least weight it reports: shots that trap after an X, 1, as no shot ended;
/// and shots that end after an X, [`KEPT_WEIGHT`].
#[test]
fn the_least_weight_is_read_only_from_shots_that_ended() {
    let (trap, end) = (1, 2);
    let x_then_trap = program(
        1,
        &[
            &[
                gate1(OPID_X, 0),
                mz(0),
                read_result(0, 0),
                branch(0, trap, end),
            ],
            &[set(0, 2), branch(0, end, end)],
            &[ret()],
        ],
    );
    let x_then_end = program(1, &[&[gate1(OPID_X, 0), mz(0), ret()]]);
    let rows = [
        ("shots that trap", x_then_trap, true, 1.0),
        ("shots that end", x_then_end, false, KEPT_WEIGHT),
    ];
    for (scene, program, traps, least) in rows {
        let mut resumer = MpsResumer::new(keeping_half(), 7);
        let result = run(&mut resumer, &program, &QirShotId::first(2));
        assert_eq!(result.is_err(), traps, "{scene}");
        let report = resumer.last_run();
        assert!(
            (report.least_retained_weight - least).abs() < ROUNDING,
            "{scene}: {report:?}"
        );
    }
}

/// The weight each state of [`keeping_half`]'s engine keeps.
const KEPT_WEIGHT: f64 = 0.5;

/// The spy's engine, keeping [`KEPT_WEIGHT`] of each state's weight.
fn keeping_half() -> ReferenceSpy {
    ReferenceSpy::retaining(Weight::Fixed(KEPT_WEIGHT))
}

/// A run warns when its shots lost more weight to truncation than the
/// tolerance, giving the weight lost (not the weight kept) and the max bond
/// dimension the run was given, and only then (the message the warning gives:
/// `mps_simulator/mps_run_report/truncation_warning/tests.rs`). Each row is a
/// run's report, and the weight lost and χ it warns of: an exact run, which
/// keeps all its weight, none; a run at χ = 16 that lost 1%, 1% at 16; and a
/// loss of exactly the tolerance, none.
#[test]
fn a_run_warns_only_when_it_lost_more_than_the_tolerance() {
    let mut exact = MpsResumer::new(ExactReference::new(), 7);
    run_once(&mut exact, &ghz(3));
    assert!(
        (exact.last_run().least_retained_weight - 1.0).abs() < ROUNDING,
        "an exact run keeps all its weight"
    );
    let mut lossy = MpsResumer::new(losing_one_percent(), 7)
        .with_max_bond_dimension(NonZeroUsize::new(16).expect("not zero"));
    run_once(&mut lossy, &ghz(3));
    let at_tolerance = MpsRunReport {
        largest_bond_dimension: 1,
        least_retained_weight: 1.0 - TRUNCATION_TOLERANCE,
        max_bond_dimension: 8,
    };
    let rows = [
        ("an exact run", *exact.last_run(), None),
        (
            "a run at χ = 16 that lost 1%",
            *lossy.last_run(),
            Some((0.01, 16)),
        ),
        ("a loss of exactly the tolerance", at_tolerance, None),
    ];
    for (scene, report, warns_of) in rows {
        let warning = report.truncation_warning();
        assert_eq!(
            warning.is_some(),
            warns_of.is_some(),
            "{scene}: {warning:?}"
        );
        if let (Some(warning), Some((lost_weight, max_bond_dimension))) = (warning, warns_of) {
            assert!((warning.lost_weight - lost_weight).abs() < 1e-15, "{scene}");
            assert_eq!(warning.max_bond_dimension, max_bond_dimension, "{scene}");
        }
    }
}
