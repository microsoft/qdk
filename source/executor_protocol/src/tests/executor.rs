// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeMap;

use crate::tests::test_double::TestDoubleExecutor;
use crate::{Close, ExecutorReport, Fold, ShotId, ShotSet};

use crate::conformance;
use crate::tests::Count;
use crate::tests::test_double::{TestDoubleModel, step};

/// Executors that split the shots and run the same batches report, once
/// folded, what one executor over all the shots reports, shot by shot,
/// whether a split cuts the shots into halves or interleaves them;
/// `calls` counts each executor's calls, so it adds up. Call `k`
/// runs on the shots from `k` on, so shot `j` runs `j` calls and has
/// its own log probability, `-j`: a fold that mixed up shots would differ.
#[test]
fn the_reports_of_a_split_fold_into_the_report_of_one_executor() {
    fn run_on(shots: &[ShotId]) -> ExecutorReport<Count> {
        let doubles = TestDoubleModel {
            log_probability: true,
            ..TestDoubleModel::default()
        };
        let held = ShotSet::new(shots.to_vec()).expect("a test names distinct shots");
        let mut executor = TestDoubleExecutor::with_model(doubles, 1);
        for k in 1..=6 {
            if let Some(from_k) = held.subset(|j| j >= k) {
                step(
                    &mut executor,
                    &from_k,
                    &[true],
                    "the executor runs its shots",
                );
            }
        }
        executor.close()
    }
    let whole = run_on(&[1, 2, 3, 4, 5, 6]);
    assert_eq!(whole.calls, 6);
    assert_eq!(
        (1..=6).map(|j| whole.log_p.get(j)).collect::<Vec<_>>(),
        (1..=6).map(|j| Some(-f64::from(j))).collect::<Vec<_>>(),
        "shot `j` runs `j` calls"
    );
    for (left, right, calls) in [([1, 2, 3], [4, 5, 6], 3 + 6), ([1, 3, 5], [2, 4, 6], 5 + 6)] {
        let mut split = run_on(&left);
        split.fold(run_on(&right));
        assert_eq!(
            (&split.log_p, &split.states),
            (&whole.log_p, &whole.states),
            "{left:?} and {right:?}"
        );
        assert_eq!(split.calls, calls, "{left:?} and {right:?}");
    }
}

/// Executor reports, with log probabilities on shared and separate shots.
/// The log probabilities are small integers, so that their sums are exact.
fn reports() -> [ExecutorReport<Count>; 3] {
    let report = |calls, states, log_p: &[(ShotId, f64)]| {
        let mut report = ExecutorReport {
            calls,
            states: Count(states),
            ..ExecutorReport::default()
        };
        for &(j, ln_p) in log_p {
            report.log_p.add(j, ln_p);
        }
        report
    };
    [
        report(1, 2, &[(1, -1.0), (2, -2.0)]),
        report(2, 1, &[(2, -3.0)]),
        report(3, 4, &[(1, -4.0), (3, -5.0)]),
    ]
}

/// An executor that ran nothing adds nothing to a folded report.
#[test]
fn the_default_executor_report_is_empty() {
    for report in reports() {
        conformance::default_is_empty(&report);
    }
}

/// Executor reports fold to the same total whichever executor closes first:
/// counts and log probabilities add, shot by shot.
#[test]
fn executor_reports_fold_in_any_order() {
    let [a, b, c] = reports();
    conformance::folds_in_any_order(&a, &b, &c);
}

/// Two shots' operation counts: a count folds into another by adding, so
/// the total counts every operation of both.
#[test]
fn integers_fold_by_adding() {
    let mut total = 2_u64;
    total.fold(3);
    assert_eq!(total, 5);
    let mut total = -2_i32;
    total.fold(3);
    assert_eq!(total, 1);
}

/// A shot that ran an operation 255 times, counted in a byte, folded with
/// one more: the count overflows, which panics in a debug build, as `+`
/// does, rather than wrap to a count that looks right.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "attempt to add with overflow")]
fn an_integers_fold_that_overflows_panics() {
    let mut count = u8::MAX;
    count.fold(1);
}

/// A shot that ran no operation of a kind counts zero of it: folding zero
/// in adds nothing.
#[test]
fn zero_adds_nothing() {
    conformance::default_is_empty(&7_u64);
}

/// Three shots' counts of an operation fold to the same total whichever
/// shot ends first.
#[test]
fn integers_fold_in_any_order() {
    conformance::folds_in_any_order(&1_u64, &2, &4);
}

/// Two profilers' counts per operation: the counts of an operation both ran
/// fold, and an operation only one ran keeps its count.
#[test]
fn maps_fold_value_by_value() {
    let mut total = BTreeMap::from([("h", 2_u64), ("m", 1)]);
    total.fold(BTreeMap::from([("m", 3), ("x", 1)]));
    assert_eq!(total, BTreeMap::from([("h", 2), ("m", 4), ("x", 1)]));
}

/// Counts per operation, on shared and separate operations, with a map
/// that holds a zero count.
fn counts() -> [BTreeMap<&'static str, u64>; 3] {
    [
        BTreeMap::from([("h", 2), ("m", 1)]),
        BTreeMap::from([("m", 3), ("x", 0)]),
        BTreeMap::from([("h", 4), ("x", 5)]),
    ]
}

/// A profiler that ran no shot counts no operation: folding its empty map
/// in adds nothing.
#[test]
fn the_empty_map_adds_nothing() {
    let [_, counts, _] = counts();
    conformance::default_is_empty(&counts);
}

/// Three profilers' counts per operation fold to the same total whichever
/// profiler closes first.
#[test]
fn maps_fold_in_any_order() {
    let [a, b, c] = counts();
    conformance::folds_in_any_order(&a, &b, &c);
}

/// Two placements of a chain whose profiler counts operations, each
/// closing to the profiler's counts beside its executor's: the pairs fold
/// part by part, counts with counts and report with report.
#[test]
fn pairs_fold_part_by_part() {
    let report = |calls| ExecutorReport {
        calls,
        ..ExecutorReport::<()>::default()
    };
    let mut total = (BTreeMap::from([("h", 2_u64)]), report(1));
    total.fold((BTreeMap::from([("h", 1), ("m", 1)]), report(2)));
    assert_eq!(total, (BTreeMap::from([("h", 3), ("m", 1)]), report(3)));
}

/// Three profiled chains' reports, each a profiler's counts per operation
/// beside its executor's report.
fn profiled_reports() -> [(BTreeMap<&'static str, u64>, ExecutorReport<Count>); 3] {
    let [a, b, c] = counts();
    let [report_a, report_b, report_c] = reports();
    [(a, report_a), (b, report_b), (c, report_c)]
}

/// A profiled chain that ran nothing adds nothing to a folded report: the
/// pair of empty reports is empty, as an executor's report is (the
/// conformance kit's check, run on an adapter's report).
#[test]
fn the_default_pair_is_empty() {
    for report in profiled_reports() {
        conformance::default_is_empty(&report);
    }
}

/// Profiled chains' reports fold to the same total whichever chain closes
/// first (the conformance kit's check, run on an adapter's report).
#[test]
fn pairs_fold_in_any_order() {
    let [a, b, c] = profiled_reports();
    conformance::folds_in_any_order(&a, &b, &c);
}
