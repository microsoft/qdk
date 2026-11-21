// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How each runtime's shot is timed: the machine alone
//! ([`machine_shot`]'s loop, every outcome `Zero`), and `run_shot` on a
//! simulator that does nothing ([`NoOpSimulator`]); and how the program
//! the machine runs is timed as it is built.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Throughput, measurement::WallTime};
use qdk_simulators::bytecode::{AdaptiveProgram, runtime::run_shot};
use qdk_simulators::qir::{QirCall, QirMachine, QirOutcome, QirProgram, QirReadout, QirStop};
use qdk_simulators::{OutputRecord, Simulator};

use crate::UNITS;
use crate::parts::NoOpSimulator;
use crate::workload::copy_of;

/// Times a shot of `program`, named `name` in `group`, by the machine
/// alone, and one of its `bytecode` by `run_shot` on [`NoOpSimulator`];
/// and the build of `program` from `bytecode` ([`QirProgram::new`]), which
/// the machine pays once per program.
///
/// The machine reuses one context from shot to shot
/// ([`QirMachine::restart_shot`]), as a resumer running shots one after
/// another does; `run_shot` makes its simulator each shot, as its callers
/// do. The build takes its bytecode, so each build is given a copy, made
/// outside the time.
pub(crate) fn time(
    group: &mut BenchmarkGroup<WallTime>,
    name: &str,
    bytecode: &AdaptiveProgram<u64>,
    program: &QirProgram,
) {
    group.throughput(Throughput::Elements(UNITS));
    let machine = QirMachine::new(program);
    let mut context = machine.start_shot();
    let mut outcomes = Vec::new();
    group.bench_function(BenchmarkId::new("machine", name), |bencher| {
        bencher.iter(|| {
            machine.restart_shot(&mut context);
            outcomes.clear();
            loop {
                match machine.resume(&mut context, &outcomes) {
                    Ok(QirStop::Batch(batch)) => {
                        outcomes.clear();
                        answer_zero(black_box(batch), &mut outcomes);
                    }
                    Ok(QirStop::Ended(records)) => break black_box(records.len()),
                    Err(trap) => panic!("a shot traps: {trap}"),
                }
            }
        });
    });
    let (qubits, results) = (bytecode.num_qubits as usize, bytecode.num_results as usize);
    group.bench_function(BenchmarkId::new("run_shot", name), |bencher| {
        bencher.iter(|| {
            let mut simulator = NoOpSimulator::new(qubits, results, 0, ());
            run_shot(black_box(bytecode), &mut simulator)
        });
    });
    group.bench_function(BenchmarkId::new("build", name), |bencher| {
        bencher.iter_batched(
            || copy_of(bytecode),
            |copy| QirProgram::new(copy).expect("the program was built before it was timed"),
            BatchSize::LargeInput,
        );
    });
}

/// A shot of `program` by the machine alone, every outcome `Zero`: its
/// quantum calls, in order, and its records.
pub(crate) fn machine_shot(program: &QirProgram) -> (Vec<QirCall>, Vec<OutputRecord>) {
    let machine = QirMachine::new(program);
    let mut context = machine.start_shot();
    let mut outcomes = Vec::new();
    let mut calls = Vec::new();
    loop {
        match machine.resume(&mut context, &outcomes) {
            Ok(QirStop::Batch(batch)) => {
                calls.extend_from_slice(batch);
                outcomes.clear();
                answer_zero(batch, &mut outcomes);
            }
            Ok(QirStop::Ended(records)) => return (calls, records.to_vec()),
            Err(trap) => panic!("a shot traps: {trap}"),
        }
    }
}

/// Appends an outcome `Zero` for each quantum call of `batch` that writes a
/// result.
fn answer_zero(batch: &[QirCall], outcomes: &mut Vec<QirOutcome>) {
    outcomes.extend(
        batch
            .iter()
            .filter_map(QirCall::result)
            .map(|result| QirOutcome {
                result,
                value: QirReadout::Zero,
            }),
    );
}
