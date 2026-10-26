// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The A100 qualification of `CuTensorNetEngine`: the first check, on the
//! real libraries, that it computes the right numbers, and the measurements
//! ②′'s open GPU questions need.
//!
//! - `conformance` runs [`tensornet::conformance::check`] on the engine
//!   [`Availability::mps_engine`](qdk_cutensornet::Availability::mps_engine)
//!   creates, which restarts each state every 10 computes on a new
//!   cuTensorNet handle, and fails listing every
//!   [`Failure`](tensornet::conformance::Failure);
//!   `conformance_without_capture` does the same without capture, and so
//!   without restarts, except for the retained weight, which it reports but
//!   is not held to; `conformance_with_restart` does it on two engines that
//!   restart their native state from the computed MPS at every opportunity,
//!   the first on the handle it is on, the second on a new cuTensorNet
//!   handle each time, as the engine does every 10 computes (its rows
//!   prefixed `handle=fresh;`). They are the only tests that assert.
//! - The others measure, and print what they measured as CSV rows (see
//!   `report`): an engine error is a row, and the sweep goes on. They fail
//!   only when no engine can be created. All but `read_cost`'s rows
//!   without capture and `history_growth`'s variants measure the engine
//!   `mps_engine` creates, restarts included. `environment` records the versions
//!   found; `empty_compute` reads states no operator was applied to;
//!   `read_cost` times reads as operators accumulate, with and without
//!   capture, and extrapolates them to H₂; `fork_cost` times a
//!   fork (D21); `free_after_compute` compares a long-lived state with
//!   [`ExactReference`] over many computes, and records the process's device
//!   memory (E17); `shape_and_tolerance` compares realized shapes and
//!   values with the reference's; `impossible_condition` and
//!   `rare_condition` place the threshold under which a conditional is
//!   refused (E16); `history_growth` profiles what grows with a shot's
//!   history, on the engine and on variants that never restart (the
//!   baseline, with the minimum workspace, with two computes per step) or
//!   restart on the same handle, and probes what a state with no history
//!   asks for, on the engine's handle and on a new one.
//!
//! Every test needs CUDA runtime 12.9 and cuTensorNet 2.13, found as
//! [`discover`] finds them, and an NVIDIA GPU, so every test is ignored. Run
//! them on one thread, so that no two share the GPU while timed:
//!
//! ```text
//! cargo test --release -p qdk_cutensornet --features qualification \
//!     --test qualification -- --ignored --nocapture --test-threads=1
//! ```
//!
//! The feature `qualification` builds this target, and exposes the engine
//! without capture that `read_cost` compares with.
//!
//! With `QDK_ENGINE_QUALIFICATION_QUICK=1`, every sweep is smaller.
//! `scripts/qualify-engine-on-a100.sh` runs them on the VM and collects the
//! rows.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

mod circuits;
mod history;
mod measure;
mod memory;
mod report;

use measure::{Measured, quick};
use qdk_cutensornet::{CuTensorNetEngine, EngineError, EngineVariant, discover};
use report::{Report, device_mib, milliseconds, or_unavailable, timed};
use std::num::NonZeroUsize;
use tensornet::conformance::check;

/// The cases of [`check`], in its order: it reports failures, not the cases
/// it ran, so this list is what a passing case is printed from.
const CONFORMANCE_CASES: [&str; 10] = [
    "bell",
    "layout",
    "conditional",
    "project",
    "fork",
    "sample",
    "shape",
    "retained weight",
    "lone site",
    "refusals",
];

/// The case the engine without capture is not held to: each of its
/// computes truncates the whole history again, so a projection may change
/// its retained weight (`Availability::mps_engine_without_capture`). Its
/// row is printed; it does not fail the test.
const NOT_HELD_WITHOUT_CAPTURE: &[&str] = &["retained weight"];

impl Measured for CuTensorNetEngine {
    fn refusal(error: &EngineError) -> Option<String> {
        match error {
            EngineError::InvalidRequest { reason } => Some(reason.clone()),
            _ => None,
        }
    }
}

fn engine() -> CuTensorNetEngine {
    let availability = discover().unwrap_or_else(|error| panic!("no cuTensorNet: {error}"));
    availability
        .mps_engine()
        .unwrap_or_else(|error| panic!("no engine on CUDA device 0: {error}"))
}

/// Closes `engine`, after its states are dropped, and records the result.
fn close(report: &mut Report, engine: CuTensorNetEngine) {
    close_as(report, "closed", engine);
}

/// [`close`], recorded as `item`.
fn close_as(report: &mut Report, item: &str, engine: CuTensorNetEngine) {
    if report.record(item, engine.close()).is_some() {
        report.row(item, true, "");
    }
}

/// Runs `run` on a new engine, under `case`'s rows.
fn measure(case: &'static str, run: fn(&mut Report, &CuTensorNetEngine)) {
    let mut report = Report::new(case);
    let engine = engine();
    run(&mut report, &engine);
    close(&mut report, engine);
    report.finish();
}

#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn environment() {
    let mut report = Report::new("environment");
    let availability = discover().unwrap_or_else(|error| panic!("no cuTensorNet: {error}"));
    let found = availability.report();
    report.row("cutensornet_version", found.cutensornet_version, "");
    report.row(
        "cutensornet_cuda_runtime_version",
        found.cutensornet_cuda_runtime_version,
        "",
    );
    report.row("cuda_runtime_version", found.cuda_runtime_version, "");
    report.row("cuda_driver_version", found.cuda_driver_version, "");
    report.note("cutensornet_library", found.cutensornet_library.display());
    report.note("cuda_runtime_library", found.cuda_runtime_library.display());
    report.row("quick", quick(), "");
    report.row(
        "device_mib_before_engine",
        or_unavailable(device_mib()),
        "MiB",
    );
    let engine = availability
        .mps_engine()
        .unwrap_or_else(|error| panic!("no engine on CUDA device 0: {error}"));
    report.row(
        "device_mib_with_engine",
        or_unavailable(device_mib()),
        "MiB",
    );
    close(&mut report, engine);
    report.finish();
}

/// Conformance of the engine, which captures each compute and restarts
/// each state every 10 computes on a new cuTensorNet handle.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn conformance() {
    conform("conformance", engine(), &[]);
}

/// Conformance of the engine without capture, and so without restarts,
/// the path `read_cost` compares with.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn conformance_without_capture() {
    let availability = discover().unwrap_or_else(|error| panic!("no cuTensorNet: {error}"));
    let engine = availability
        .mps_engine_without_capture()
        .unwrap_or_else(|error| panic!("no engine on CUDA device 0: {error}"));
    conform(
        "conformance_without_capture",
        engine,
        NOT_HELD_WITHOUT_CAPTURE,
    );
}

/// Conformance of the engine restarting its native state from the computed
/// MPS before every operator after a compute, on the handle it is on: the
/// restart's `cutensornetStateInitializeMPS`, on the real library. Then the
/// same on an engine that moves the state to a new cuTensorNet handle at
/// each restart, as the engine does every 10 computes (`history_growth`'s
/// `default`), its rows prefixed `handle=fresh;`. A period of 1 makes the
/// conformance cases, whose states compute a few times each, restart.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn conformance_with_restart() {
    const NAME: &str = "conformance_with_restart";
    let availability = discover().unwrap_or_else(|error| panic!("no cuTensorNet: {error}"));
    let mut report = Report::new(NAME);
    let mut failures = Vec::new();
    for (prefix, fresh_handle_at_restart) in [("", false), ("handle=fresh;", true)] {
        let engine = availability
            .mps_engine_variant(EngineVariant {
                restart_period: NonZeroUsize::new(1),
                fresh_handle_at_restart,
                ..EngineVariant::default()
            })
            .unwrap_or_else(|error| panic!("no engine on CUDA device 0: {error}"));
        failures.extend(check_conformance(&mut report, prefix, engine, &[]));
    }
    report.finish();
    assert_conforms(NAME, &failures);
}

/// Runs [`check`] on `engine`, prints a `case=<name>;passed` row for each
/// case, and fails listing every failure outside `not_held`.
fn conform(name: &'static str, engine: CuTensorNetEngine, not_held: &[&str]) {
    let mut report = Report::new(name);
    let failures = check_conformance(&mut report, "", engine, not_held);
    report.finish();
    assert_conforms(name, &failures);
}

/// Fails listing `failures`, if any.
fn assert_conforms(name: &str, failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{name}: the engine fails {} conformance checks:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Runs [`check`] on `engine`, prints its rows, each item prefixed with
/// `prefix`, closes it, and returns the failures outside `not_held`,
/// prefixed too.
fn check_conformance(
    report: &mut Report,
    prefix: &str,
    engine: CuTensorNetEngine,
    not_held: &[&str],
) -> Vec<String> {
    let (failures, duration) =
        timed(|| Ok::<_, EngineError>(check(&engine))).unwrap_or_else(|error| panic!("{error}"));
    report.row(&format!("{prefix}duration"), milliseconds(duration), "ms");
    let mut cases = CONFORMANCE_CASES.to_vec();
    for failure in &failures {
        if !cases.contains(&failure.case) {
            cases.push(failure.case);
        }
    }
    for case in cases {
        let passed = !failures.iter().any(|failure| failure.case == case);
        report.row(&format!("{prefix}case={case};passed"), passed, "");
    }
    for failure in &failures {
        report.note(&format!("{prefix}case={}", failure.case), &failure.message);
    }
    close_as(report, &format!("{prefix}closed"), engine);
    failures
        .iter()
        .filter(|failure| !not_held.contains(&failure.case))
        .map(|failure| format!("{prefix}{failure}"))
        .collect()
}

/// Reads of a state no operator was applied to: the first compute then
/// computes |0…0⟩ alone.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn empty_compute() {
    measure("empty_compute", measure::empty_compute);
}

/// How a read's cost grows with the operators applied, on the engine, which
/// captures each compute (and restarts every 10), and on the same engine
/// without capture or restart, whose reads compute from |0…0⟩ with every
/// operator; both extrapolated to H₂'s 1,404 reads over 48,000 gates.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn read_cost() {
    let mut report = Report::new("read_cost");
    let availability = discover().unwrap_or_else(|error| panic!("no cuTensorNet: {error}"));
    let captured = engine();
    let uncaptured = availability
        .mps_engine_without_capture()
        .unwrap_or_else(|error| panic!("no engine on CUDA device 0: {error}"));
    measure::read_cost(&mut report, &[("on", &captured), ("off", &uncaptured)]);
    close(&mut report, captured);
    close(&mut report, uncaptured);
    report.finish();
}

/// What a fork costs (D21): a new native state with every operator applied
/// again, so its first read computes from |0…0⟩.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn fork_cost() {
    measure("fork_cost", measure::fork_cost);
}

/// A long-lived state against the exact reference over many computes; a
/// compute frees the previous compute's buffers (E17), so the device memory
/// should not grow with them.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn free_after_compute() {
    measure("free_after_compute", measure::free_after_compute);
}

/// The realized shapes and values against the exact reference's, under the
/// qualified cutoff (1e-10) and the conformance tolerance (1e-9).
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn shape_and_tolerance() {
    measure("shape_and_tolerance", measure::shape_and_tolerance);
}

/// Conditions of probability zero, which the engine should refuse (E16).
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn impossible_condition() {
    measure("impossible_condition", measure::impossible_condition);
}

/// Conditions of shrinking probability: where the engine starts refusing
/// (E16's threshold is 1e-12), and how accurate it is above that.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn rare_condition() {
    measure("rare_condition", measure::rare_condition);
}

/// The device memory along H₂'s pattern, step after step, with variants of
/// the step's measurement: what the 80 MiB per `read_cost` checkpoint grows
/// with, and how far H₂'s 1,404 measurements would take it.
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn projection_memory() {
    measure("projection_memory", memory::projection_memory);
}

/// What grows with a shot's history along H₂'s pattern, call by call, on
/// the engine, which restarts every 10 computes on a new handle, and on a
/// baseline that never restarts: whether the minimum workspace or a restart
/// every 10 or 50 steps on the same handle stops it; whether it grows per
/// compute; and what a state with no history asks for (`history`).
#[test]
#[ignore = "needs an NVIDIA GPU, CUDA runtime 12.9 and cuTensorNet 2.13"]
fn history_growth() {
    let mut report = Report::new("history_growth");
    history::history_growth(&mut report);
    report.finish();
}
