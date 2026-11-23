// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the QIR machine's walk of a program costs ([`QirMachine`]),
//! against the bytecode's reference interpreter, `run_shot`.
//!
//! The machine runs a program's classical part and hands its quantum calls
//! to a resumer; `run_shot` runs both, calling a simulator. So both run
//! here with no quantum work: the machine alone, every outcome `Zero`, and
//! `run_shot` on a simulator that does nothing and measures `Zero`
//! ([`NoOpSimulator`]). What each costs is then its classical part: the
//! walk, and the quantum calls built and handed over.
//!
//! Each measurement is a synthetic program of [`UNITS`] units, in the file
//! named for what it measures:
//!
//! - `quantum_calls`: a quantum call with constant operands, prebuilt
//!   when the program is built, against one whose operands are read from
//!   registers, built when a shot reaches it;
//! - `correlated_noise`: a call of correlated noise on 1, 2 and 3 qubits;
//! - `classical_instructions`: a loop of arithmetic, comparison and branch;
//! - `read_rounds`: a round of measurement, read of its result and
//!   branch, each round a stop of the machine;
//! - `function_calls`: a function call and its return, in a loop.
//!
//! Before a program is timed, [`assert_same_work`] checks that the two
//! runtimes do the same work on it: the same quantum calls in the same
//! order, and the same records. Each program's build
//! ([`QirProgram::new`]), which the machine pays once per program and
//! `run_shot` does not, is timed beside its shots.
//!
//! Run with `cargo bench -p qdk_simulators --bench qir_machine`, or name
//! the measurements to run, as criterion filters them:
//! `cargo bench -p qdk_simulators --bench qir_machine -- quantum_calls`.
//! Each result of `machine` and `run_shot` is the time of one shot, so of
//! [`UNITS`] units, and each of `build` the time of one build; criterion
//! also gives the units per second.

mod classical_instructions;
mod correlated_noise;
mod function_calls;
mod parts;
mod quantum_calls;
mod read_rounds;
mod timing;
mod workload;

#[allow(dead_code)]
#[path = "../../src/bytecode/encoding.rs"]
mod encoding;

use criterion::{BenchmarkGroup, criterion_group, criterion_main, measurement::WallTime};
use qdk_simulators::OutputRecord;
use qdk_simulators::bytecode::{AdaptiveProgram, runtime::run_shot};
#[cfg(doc)]
use qdk_simulators::qir::QirMachine;
use qdk_simulators::qir::{QirCall, QirProgram};

use parts::LoggingSimulator;
#[cfg(doc)]
use parts::NoOpSimulator;

/// The units of each measured program: its quantum calls, classical
/// instructions, rounds or function calls.
const UNITS: u64 = 10_000;

criterion_group!(
    benches,
    quantum_calls::bench,
    correlated_noise::bench,
    classical_instructions::bench,
    read_rounds::bench,
    function_calls::bench
);
criterion_main!(benches);

/// Times `bytecode`, named `name` in `group`, by each runtime, once they
/// agree on it ([`assert_same_work`]).
fn time_against_run_shot(
    group: &mut BenchmarkGroup<WallTime>,
    name: &str,
    bytecode: &AdaptiveProgram<u64>,
) {
    let program = QirProgram::new(workload::copy_of(bytecode))
        .unwrap_or_else(|faults| panic!("{name}: {faults}"));
    assert_same_work(name, bytecode, &program);
    timing::time(group, name, bytecode, &program);
}

/// Asserts that a shot of `program` by the machine alone, every outcome
/// `Zero`, and one of its `bytecode` by `run_shot`, on a simulator that
/// logs its calls and measures `Zero`, make the same quantum calls in the
/// same order and give the same records; so that a runtime that loses,
/// adds or misplaces a call is not timed. A program that records nothing
/// gives every result, as the machine gives them, from the simulator.
fn assert_same_work(name: &str, bytecode: &AdaptiveProgram<u64>, program: &QirProgram) {
    let (machine_calls, machine_records) = timing::machine_shot(program);
    let mut simulator = LoggingSimulator::with_results(bytecode.num_results as usize);
    let mut records = run_shot(bytecode, &mut simulator);
    if !workload::records_output(bytecode) {
        records = simulator.results().map(OutputRecord::Result).collect();
    }
    let calls: Vec<QirCall> = simulator.into_calls();
    assert_eq!(
        machine_calls.len(),
        calls.len(),
        "{name}: the runtimes make different numbers of quantum calls"
    );
    for (index, (machine, reference)) in machine_calls.iter().zip(&calls).enumerate() {
        assert_eq!(
            machine, reference,
            "{name}: quantum call {index} differs from run_shot's"
        );
    }
    assert_eq!(
        machine_records, records,
        "{name}: the records differ from run_shot's"
    );
}
