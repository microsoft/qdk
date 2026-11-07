// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The [`QirSimulatorResumer`]: the kit's rules, and the simulator
//! operation it applies for each quantum call of a batch, as `run_shot`
//! applies it.

use std::sync::Arc;

use qdk_simulators::bytecode::runtime::run_shot;
use qdk_simulators::bytecode::{AdaptiveProgram, Instruction};
use qdk_simulators::cpu_full_state_simulator::FullStateSimulator;
use qdk_simulators::noise_config::{
    CumulativeNoiseConfig, LossPolicy, NoiseConfig, NoiseTable, encode_pauli,
};
use qdk_simulators::qir::{
    QirMachine, QirProgram, QirResumer, QirShotId, conformance, expect_ready,
};
use qdk_simulators::stabilizer_simulator::StabilizerSimulator;
use qdk_simulators::{MeasurementResult, OutputRecord, Simulator};

use super::{Log, Operations, full_state, parity};
use crate::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, OP_PEEK_LOSS, OP_READOUT_NOISE, OPID_CX, OPID_CY,
    OPID_CZ, OPID_H, OPID_MOVE, OPID_MRESETZ, OPID_MZ, OPID_RX, OPID_RXX, OPID_RY, OPID_RYY,
    OPID_RZ, OPID_RZZ, OPID_S, OPID_S_ADJ, OPID_SWAP, OPID_SX, OPID_SX_ADJ, OPID_T, OPID_T_ADJ,
    OPID_X, OPID_Y, OPID_Z,
};
use crate::programs::{
    built, copy_of, correlated_noise, gate, measure, op, program, read_result, readout_noise,
    record_result, reset, ret, set, with_noise_table,
};
use crate::simulator_resumer::QirSimulatorResumer;

/// The qubits, results and registers of the programs of these tests.
const SIZE: u32 = 8;

/// The angle of the rotations.
const ANGLE: f64 = 0.25;

/// The probability that readout noise reads a `One` as `Zero`; its
/// probability of reading a `Zero` as `One` is [`ANGLE`].
const ONE_AS_ZERO: f64 = 0.5;

/// The noise table of correlated noise in the programs of these tests.
const NOISE_TABLE: u64 = 5;

/// The records of `shots` of `bytecode`, run by a simulator resumer on the
/// simulators `make` makes.
fn records<S: Simulator>(
    bytecode: &AdaptiveProgram<u64>,
    shots: &[QirShotId],
    make: impl FnMut(&QirProgram, QirShotId) -> S,
) -> Vec<Vec<OutputRecord>> {
    let program = built(copy_of(bytecode));
    let machine = QirMachine::new(&program);
    expect_ready(QirSimulatorResumer::new(make).run(&machine, shots))
        .expect("the shots do not trap")
}

/// The operations a shot of `bytecode` applies on a [`Log`] reading each
/// qubit as
/// its [`parity`], and its records: run by a simulator resumer, and by
/// `run_shot`.
fn logs(bytecode: &AdaptiveProgram<u64>) -> [(Vec<String>, Vec<OutputRecord>); 2] {
    let operations = Operations::default();
    let make = |_: &QirProgram, _| Log::new_reading(SIZE as usize, operations.clone(), parity);
    let mut resumed = records(bytecode, &[QirShotId(0)], make);
    let resumed = (operations.take(), resumed.remove(0));
    let mut log = Log::new_reading(SIZE as usize, operations.clone(), parity);
    let reference = run_shot(bytecode, &mut log);
    [resumed, (operations.take(), reference)]
}

/// Shots on a full-state simulator each, seeded by their ids.
#[test]
fn keeps_every_rule_of_the_kit() {
    // A fresh resumer per run of the kit, each shot on a full-state
    // simulator seeded by its id (`full_state`), as the kit's rule 7 needs.
    conformance::check(|| QirSimulatorResumer::new(full_state));
}

/// A program running every QIR instruction of the machine, on qubits 0 and
/// 1 and result 2, reading each result written into register 4, and
/// recording the result: correlated noise on qubits 3 and 1, held in
/// registers 6 and 7, from noise table [`NOISE_TABLE`].
fn every_instruction() -> AdaptiveProgram<u64> {
    let gates = [
        OPID_X,
        OPID_Y,
        OPID_Z,
        OPID_H,
        OPID_S,
        OPID_S_ADJ,
        OPID_T,
        OPID_T_ADJ,
        OPID_SX,
        OPID_SX_ADJ,
        OPID_RX,
        OPID_RY,
        OPID_RZ,
        OPID_CX,
        OPID_CY,
        OPID_CZ,
        OPID_RXX,
        OPID_RYY,
        OPID_RZZ,
        OPID_SWAP,
        OPID_MOVE,
    ];
    let mut instructions = vec![set(6, 3), set(7, 1)];
    instructions.extend(gates.into_iter().map(|operation| Instruction {
        src0: ANGLE.to_bits(),
        ..gate(operation, 0, 1)
    }));
    let read = read_result(4, 2);
    instructions.extend([
        correlated_noise(2, 0),
        measure(OPID_MZ, 1, 2),
        read,
        measure(OPID_MRESETZ, 0, 2),
        read,
        Instruction {
            aux0: 1,
            aux1: 2,
            ..op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
        },
        read,
        Instruction {
            aux0: ANGLE.to_bits(),
            aux1: ONE_AS_ZERO.to_bits(),
            aux2: 2,
            ..op(
                OP_READOUT_NOISE,
                FLAG_AUX0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
            )
        },
        reset(0),
        record_result(2),
        ret(),
    ]);
    let mut bytecode = program(SIZE, &[&instructions]);
    bytecode.call_args = vec![6, 7];
    with_noise_table(&mut bytecode, NOISE_TABLE);
    bytecode
}

/// Every QIR instruction of the machine, on qubits 0 and 1, result 2, and
/// constant angle and probabilities, correlated noise on qubits 3 and 1:
/// each quantum call runs on the simulator as the operation of its
/// instruction, on its operands, in the program's order.
#[test]
fn each_quantum_call_applies_its_instruction_s_operation_on_its_operands() {
    let [resumed, reference] = logs(&every_instruction());
    let mut expected = Vec::new();
    for gate in [
        "x", "y", "z", "h", "s", "s_adj", "t", "t_adj", "sx", "sx_adj",
    ] {
        expected.push(format!("{gate}(0)"));
    }
    for gate in ["rx", "ry", "rz"] {
        expected.push(format!("{gate}({ANGLE}, 0)"));
    }
    for gate in ["cx", "cy", "cz"] {
        expected.push(format!("{gate}(0, 1)"));
    }
    for gate in ["rxx", "ryy", "rzz"] {
        expected.push(format!("{gate}({ANGLE}, 0, 1)"));
    }
    expected.extend([
        "swap(0, 1)".to_string(),
        "mov(0)".to_string(),
        format!("correlated_noise_intrinsic({NOISE_TABLE}, [3, 1])"),
        "mz(1, 2)".to_string(),
        "mresetz(0, 2)".to_string(),
        "peek_loss(1, 2)".to_string(),
        format!("apply_readout_noise({ANGLE}, {ONE_AS_ZERO}, 2)"),
        "resetz(0)".to_string(),
    ]);
    assert_eq!(resumed.0, expected);
    // COMPATIBILITY NOTE: these operations are those `run_shot`
    // (bytecode/runtime.rs, `OP_QUANTUM_GATE`, `OP_MEASURE`, ...) applies,
    // hence the resumer keeps them, so that it may replace it.
    assert_eq!(resumed, reference);
}

/// A measurement of a qubit that a simulator finds lost, recorded.
#[test]
fn a_lost_qubit_s_result_is_a_loss() {
    let bytecode = program(SIZE, &[&[measure(OPID_MZ, 0, 0), record_result(0), ret()]]);
    let lost = |_: &QirProgram, _| {
        Log::new_reading(SIZE as usize, Operations::default(), |_| {
            MeasurementResult::Loss
        })
    };
    assert_eq!(
        records(&bytecode, &[QirShotId(0)], lost),
        [[OutputRecord::Result(MeasurementResult::Loss)]]
    );
}

/// A program applying correlated noise from noise table [`NOISE_TABLE`]
/// to the `count` qubits held in the registers of the call arguments from
/// 1: registers 7, 4 and 6, holding qubits 1, 2 and 0, between arguments
/// naming register 5, holding qubit 3.
fn noise_on(count: u64) -> AdaptiveProgram<u64> {
    let mut bytecode = program(
        SIZE,
        &[&[
            set(4, 2),
            set(5, 3),
            set(6, 0),
            set(7, 1),
            correlated_noise(count, 1),
            ret(),
        ]],
    );
    bytecode.call_args = vec![5, 7, 4, 6, 5];
    with_noise_table(&mut bytecode, NOISE_TABLE);
    bytecode
}

/// Correlated noise on none to three of the qubits 1, 2 and 0, read from
/// registers out of order, even on no qubit.
#[test]
fn correlated_noise_is_one_operation_on_its_qubits_in_order() {
    let expected = [
        "correlated_noise_intrinsic(5, [])",
        "correlated_noise_intrinsic(5, [1])",
        "correlated_noise_intrinsic(5, [1, 2])",
        "correlated_noise_intrinsic(5, [1, 2, 0])",
    ];
    for (count, expected) in (0..).zip(expected) {
        let [resumed, reference] = logs(&noise_on(count));
        assert_eq!(resumed.0, [expected], "on {count} qubits");
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs) applies the
        // operation even on no qubit, which draws from its random numbers;
        // we keep it so that a seeded shot draws what it drew there.
        assert_eq!(resumed.0, reference.0, "on {count} qubits");
    }
}

/// The records of 20 shots of `bytecode`, seeded 0 to 19, on simulators
/// `S` with `noise`: by a simulator resumer, and by `run_shot`.
fn shots_on<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
    bytecode: &AdaptiveProgram<u64>,
    noise: &Arc<CumulativeNoiseConfig>,
) -> [Vec<Vec<OutputRecord>>; 2] {
    let simulator = |seed| S::new(SIZE as usize, SIZE as usize, seed, noise.clone());
    let shots = QirShotId::first(20);
    let resumed = records(bytecode, &shots, |_, shot: QirShotId| simulator(shot.0));
    let reference = shots
        .iter()
        .map(|shot| run_shot(bytecode, &mut simulator(shot.0)))
        .collect();
    [resumed, reference]
}

/// Correlated noise on qubits 1, 2 and 0 from a noise table flipping one
/// of its qubits, each with probability 0.3, then each qubit measured and
/// recorded, on the full-state and the stabilizer simulators: the faults
/// land on its qubits, in the shots that draw them, shot by shot as under
/// `run_shot` with the same seeds (the oracle).
#[test]
fn correlated_noise_applies_its_noise_table_s_faults_to_its_qubits() {
    let mut config = NoiseConfig::<f64, f64>::NOISELESS;
    config.intrinsics.insert(
        u32::try_from(NOISE_TABLE).expect("a noise table id"),
        NoiseTable {
            qubits: 3,
            pauli_strings: ["XII", "IXI", "IIX"].map(encode_pauli).to_vec(),
            probabilities: vec![0.3; 3],
            on_loss: LossPolicy::Skip,
        },
    );
    let noise = Arc::new(config.into());
    let mut bytecode = noise_on(3);
    let ret = bytecode.instructions.pop().expect("the program returns");
    for qubit in 0..3 {
        bytecode
            .instructions
            .extend([measure(OPID_MZ, qubit, qubit), record_result(qubit)]);
    }
    bytecode.instructions.push(ret);
    bytecode.block_table[0].instr_count = bytecode.instructions.len() as u64;
    for [resumed, reference] in [
        shots_on::<FullStateSimulator>(&bytecode, &noise),
        shots_on::<StabilizerSimulator>(&bytecode, &noise),
    ] {
        let first = &resumed[0];
        assert!(
            resumed.iter().any(|records| records != first),
            "precondition: the noise fires in some shots only: {resumed:?}"
        );
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs) applies one
        // simulator operation per correlated noise, on its qubits in order; we
        // keep it so that a seeded shot draws the faults it drew there.
        assert_eq!(resumed, reference);
    }
}

/// A noisy measurement as the Stim compiler writes it, after a flip: `mz`
/// of qubit 0, then readout noise that always flips its result, which is
/// recorded. Each shot records the flipped `Zero`, though the readout
/// noise runs in a second batch, after the measurement's.
#[test]
fn readout_noise_flips_the_result_of_the_measurement_before_it() {
    // `x` on qubit 0, `mz` of it into result 0, readout noise that always
    // flips result 0, then a record of result 0.
    let bytecode = program(
        SIZE,
        &[&[
            gate(OPID_X, 0, 0),
            measure(OPID_MZ, 0, 0),
            readout_noise(1.0, 0),
            record_result(0),
            ret(),
        ]],
    );
    // Check and decode the bytecode once; a fault would be refused here.
    // The program takes its bytecode: it gets a copy, as `run_shot` reads
    // the bytecode below.
    let program = QirProgram::new(copy_of(&bytecode)).expect("the test's program has no fault");
    // One machine runs the classical part of every shot; it holds only the
    // program.
    let machine = QirMachine::new(&program);
    // The resumer holds each shot's quantum state: here a full-state
    // simulator per shot, made from the program's facts and seeded by the
    // shot's id; any `Simulator` of this crate fits.
    let mut resumer = QirSimulatorResumer::new(|program: &QirProgram, shot: QirShotId| {
        FullStateSimulator::new(
            program.qubits() as usize,
            program.results() as usize,
            shot.0,
            Arc::default(),
        )
    });
    // Run shots 0 to 3: the resumer runs each batch on the shot's
    // simulator and resumes the shot's context with the outcomes, until
    // the shot ends. Its run never waits, so its future is ready at once.
    let records =
        expect_ready(resumer.run(&machine, &QirShotId::first(4))).expect("the shots do not trap");
    let flipped = vec![OutputRecord::Result(MeasurementResult::Zero)];
    assert_eq!(records, vec![flipped.clone(); 4]);
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, its
    // `OP_READOUT_NOISE` arm) applies readout noise to the result the
    // simulator holds, so Python's Stim-compiled programs record the
    // noisy value; we keep it so that the resumer may replace it.
    let mut simulator = FullStateSimulator::new(SIZE as usize, SIZE as usize, 7, Arc::default());
    assert_eq!(run_shot(&bytecode, &mut simulator), flipped);
}
