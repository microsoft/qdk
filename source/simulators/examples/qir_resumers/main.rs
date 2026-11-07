// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Two resumers of the QIR machine, written on the `qir` module's public
//! API as any implementer's would be, and a run of one program on each:
//!
//! - [`QirSimulatorResumer`] runs each shot on its own simulator of this
//!   crate, one shot after another (`simulator_resumer.rs`);
//! - [`QirGroupingResumer`] runs the shots together, grouped by their next
//!   batch, one dispatch per group (`grouping_resumer.rs`), as a target
//!   that runs one batch on many shots at once would.
//!
//! Run with `cargo run -p qdk_simulators --example qir_resumers`. The
//! resumers' tests (`tests/resumers/`) and the benchmark `qir_machine`
//! include the two resumers too.

mod grouping_resumer;
mod simulator_resumer;

/// The bytecode's encoding, which the crate keeps to itself; the tests'
/// programs are written in it.
#[allow(dead_code)]
#[path = "../../src/bytecode/encoding.rs"]
mod encoding;
/// The tests' programs, written in the bytecode by hand; the QDK's Python
/// pass writes the programs a user runs.
#[allow(dead_code)]
#[path = "../../tests/resumers/programs.rs"]
mod programs;

use std::sync::Arc;

use qdk_simulators::Simulator;
use qdk_simulators::cpu_full_state_simulator::FullStateSimulator;
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};

use grouping_resumer::QirGroupingResumer;
use simulator_resumer::QirSimulatorResumer;

fn main() {
    // A coin tossed (`h`, `mz`) and a branch on it, which flips a second
    // qubit on 1; that qubit then measured, a second coin tossed, and the
    // three results recorded: three batches a shot.
    let program = programs::coin_then_branch();
    let machine = QirMachine::new(&program);
    let shots = QirShotId::first(8);

    // Each shot on a full-state simulator of the program's qubits and
    // results, seeded by the shot's id, so that a shot reads the same
    // outcomes whichever resumer runs it.
    let full_state = |program: &QirProgram, shot: QirShotId| {
        FullStateSimulator::new(
            program.qubits() as usize,
            program.results() as usize,
            shot.0,
            Arc::default(),
        )
    };

    // The resumers never wait, so their runs' futures are ready at once.
    let shot_by_shot = expect_ready(QirSimulatorResumer::new(full_state).run(&machine, &shots))
        .expect("the program has no fault a shot meets");
    for (shot, records) in shots.iter().zip(&shot_by_shot) {
        println!("shot {}: {records:?}", shot.0);
    }

    let mut grouping = QirGroupingResumer::new(full_state);
    let grouped = expect_ready(grouping.run(&machine, &shots))
        .expect("the program has no fault a shot meets");
    assert_eq!(
        grouped, shot_by_shot,
        "a shot seeded by its id gives the same records grouped"
    );
    let batches: usize = grouping.dispatches().iter().map(Vec::len).sum();
    println!(
        "grouped: the same records, {batches} batches in {} dispatches",
        grouping.dispatches().len()
    );
}
