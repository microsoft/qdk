// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a shot costs run by a [`QirMachine`], alone and resumed by each
//! resumer of the example `qir_resumers`, against the bytecode's
//! reference interpreter ([`run_shot`]).
//!
//! The runtimes, each in a column:
//!
//! - the machine alone: shots resumed with every outcome `Zero`, on no
//!   simulator, which is what a shot on [`Nothing`] reads: the machine's
//!   own cost;
//! - the [`QirSimulatorResumer`] (`examples/qir_resumers/`): each
//!   shot on its own simulator, one after another;
//! - the [`QirGroupingResumer`]: the shots together, grouped
//!   by their next batch, one dispatch per group and round;
//! - [`run_shot`], shot by shot.
//!
//! First, synthetic programs on a simulator that does nothing ([`Nothing`]), in
//! nanoseconds per quantum call (constant operands, or operands read from
//! registers), per classical instruction (a loop of arithmetic, comparison and
//! branch), per round of feed-forward (a measurement whose result a branch
//! reads), and per function call and its return (in a loop, the function
//! returning its argument, the counter). The loops record their counter, and
//! the loop of function calls the value its last call returned, so that the
//! records, on which the runtimes must agree before they are timed, catch a
//! lost function call or a wrong return. Correlated noise is timed on 1, 2 and
//! 3 qubits, on [`Nothing`] and on a [`FullStateSimulator`] drawing from a
//! noise table; its qubits rotate from quantum call to quantum call and are
//! then measured and recorded, so that the records catch a lost quantum call or
//! misplaced qubits on 2 or 3 qubits ([`noise_program`]).
//!
//! Then each row is a program: the corpus of `run_shot`'s tests (one row
//! for all of it, its times summed, and the case whose ratio is the
//! worst), then each bytecode file named on the command line, in the JSON
//! form of `AdaptiveProgram.as_dict()`. Per program it asserts first that
//! the runtimes give the same records on the same shots (for a program
//! that records nothing, `run_shot`'s are every result, as the package's
//! `run_adaptive` takes them from the simulator), then reports:
//!
//! - the time to build the [`QirProgram`], paid once per program, the
//!   fastest of [`RUNS`] builds;
//! - the batches a shot of the machine yields with every outcome `Zero`:
//!   each a round trip to whatever runs the quantum part;
//! - a shot on a [`FullStateSimulator`], noiseless, by each resumer and by
//!   `run_shot`, and the resumers' ratios to `run_shot`'s (×s, ×g);
//!   skipped above [`FULL_STATE_QUBITS`] qubits, whose state does not fit;
//! - a shot on [`Nothing`], by the machine alone, each resumer and
//!   `run_shot`, and the ratios (×m, ×s, ×g): the classical part alone.
//!
//! Each shot creates its simulator, in every runtime alike; shot `j` is
//! the resumers' shot of id `j` and is seeded `j`, so that the runtimes
//! read the same outcomes. A resumer's run, of every shot measured,
//! includes making the resumer, as a caller's job does; the machine alone
//! reuses one context ([`QirMachine::restart_shot`]).
//!
//! Run with `cargo bench -p qdk_simulators --bench qir_machine -- <file>...`,
//! or `-- --synthetic` for the synthetic programs alone; `--no-full-state`
//! skips the full-state columns. The numbers depend on the machine and its
//! load, so this asserts only that the runtimes agree, not the numbers:
//! compare the rows of one run, not runs on different machines.
//!
//! # What it found
//!
//! Two runs on one machine gave, on the corpus of `run_shot`'s 244 cases
//! (3,530 instructions, 374 batches with every outcome `Zero`), in µs per
//! shot, with each runtime's ratio to `run_shot`'s; building, paid once
//! per program, took 483.9 and 465.9 µs. On a full-state simulator:
//!
//! | Run | Simulator resumer | Grouping resumer | `run_shot` |
//! | --- | ----------------- | ---------------- | ---------- |
//! | 1   | 236.3 (×0.98)     | 319.1 (×1.33)    | 240.2      |
//! | 2   | 228.8 (×1.01)     | 306.2 (×1.35)    | 227.4      |
//!
//! On a simulator that does nothing, the classical part alone:
//!
//! | Run | Machine alone | Simulator resumer | Grouping resumer | `run_shot` |
//! | --- | ------------- | ----------------- | ---------------- | ---------- |
//! | 1   | 17.91 (×0.59) | 26.35 (×0.87)     | 80.56 (×2.66)    | 30.34      |
//! | 2   | 17.22 (×0.60) | 23.92 (×0.83)     | 75.92 (×2.62)    | 28.93      |
//!
//! So on a full-state simulator a shot costs what it costs through
//! `run_shot` when the simulator resumer runs it, and the classical part
//! alone costs the machine about 0.6 times what it costs `run_shot`. The
//! grouping resumer pays for holding every shot's context and batch and
//! for grouping equal batches, which pays back where a call to the
//! quantum side costs more than the comparison (a device, a batched
//! simulator), not on a simulator in the same process. The synthetic rows
//! show where the classical part costs more than `run_shot`'s: per
//! quantum call, the simulator resumer costs 1.4 to 1.5 times `run_shot`
//! with constant operands and about 2.6 times with operands read from
//! registers (the machine alone, about 2.0 times), the cost of the batch
//! it builds and answers; per call of correlated noise 1.5 to 1.8 times,
//! and 1.2 to 1.3 times on a full-state simulator; per round of
//! feed-forward about 1.4 times; per classical instruction about 0.7
//! times; per call and return of a function about 0.75 times.
//!
//! The batches of a shot, each a round trip to whatever runs the quantum
//! part, are about its reads of fresh outcomes, plus its calls writing a
//! result their batch already writes, plus one for the calls left at the
//! program's return. For example, an H₂ quantum phase estimation program
//! from QDK Chemistry's SOSSA example (29 qubits, 1,404 results; not in
//! this repository), compiled as `run_qir` compiles it for the CPU, has
//! 1,399 `READ_RESULT`s, and a shot of it with every outcome `Zero`
//! yields 1,338 batches. Readout noise right after each measurement, as
//! noisy measurements are compiled for Stim, ends a batch before each
//! noise call: a batch per noisy measurement, however few reads.

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use qdk_simulators::bytecode::{
    AdaptiveProgram, Block, Function, Instruction, Op, runtime::run_shot,
};
use qdk_simulators::cpu_full_state_simulator::FullStateSimulator;
use qdk_simulators::noise_config::{
    CumulativeNoiseConfig, LossPolicy, NoiseConfig, NoiseTable, encode_pauli,
};
use qdk_simulators::qir::{
    QirCall, QirMachine, QirOutcome, QirProgram, QirReadout, QirResumer, QirShotId, QirStop,
    expect_ready,
};
use qdk_simulators::{MeasurementResult, OutputRecord, QubitID, ResultID, Simulator};
use serde::Deserialize;

#[path = "../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;
#[allow(dead_code)]
#[path = "../src/bytecode/encoding.rs"]
mod encoding;
// `dispatches()`, which the grouping resumer's tests read, is not timed.
#[allow(dead_code)]
#[path = "../examples/qir_resumers/grouping_resumer.rs"]
mod grouping_resumer;
#[path = "../examples/qir_resumers/simulator_resumer.rs"]
mod simulator_resumer;

use bytecode_json::Bytecode;
use encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_SLT, OP_ADD,
    OP_BRANCH, OP_CALL, OP_CALL_RETURN, OP_ICMP, OP_JUMP, OP_MEASURE, OP_MOV, OP_QUANTUM_GATE,
    OP_READ_RESULT, OP_RECORD_OUTPUT, OP_RET, OPID_CORRELATED_NOISE, OPID_CX, OPID_H, OPID_MZ,
    OPID_RZ, OPID_X,
};
use grouping_resumer::QirGroupingResumer;
use simulator_resumer::QirSimulatorResumer;

/// The most qubits a program run on a [`FullStateSimulator`] has here: 20
/// qubits take 16 MiB of state.
const FULL_STATE_QUBITS: u32 = 20;
/// The runs of each measurement, the runtimes in turn; the fastest is
/// reported.
const RUNS: usize = 5;
/// The shots compared before a program is timed, at most.
const AGREEMENT_SHOTS: u32 = 10;
/// The shots of a run, at most: the grouping resumer holds every shot's batch
/// at once, and 6 600 shots of a synthetic batch of 10 000 quantum calls took
/// more than the 3 GB the bench was given.
const MOST_SHOTS: u32 = 1_000;
/// The quantum calls, classical instructions or rounds of each synthetic
/// program.
const SYNTHETIC: u64 = 10_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    synthetic();
    if args.iter().any(|arg| arg == "--synthetic") {
        return;
    }
    let files: Vec<&String> = args.iter().filter(|arg| !arg.starts_with("--")).collect();
    println!();
    println!(
        "µs per shot, the fastest of {RUNS} runs; ×m, ×s, ×g are the machine alone, the \
         simulator resumer, the grouping resumer / run_shot"
    );
    println!(
        "{:<34} {:>6} {:>7} {:>8} {:>10} {:>10} {:>10} {:>6} {:>6} {:>10} {:>10} {:>10} {:>10} {:>6} {:>6} {:>6}",
        "program",
        "instr.",
        "batches",
        "build",
        "full sim.",
        "grouping",
        "run_shot",
        "×s",
        "×g",
        "machine",
        "sim.",
        "grouping",
        "run_shot",
        "×m",
        "×s",
        "×g"
    );
    let full_state = !args.iter().any(|arg| arg == "--no-full-state");
    corpus(full_state);
    for file in files {
        let json = std::fs::read_to_string(file).unwrap_or_else(|error| panic!("{file}: {error}"));
        let bytecode: Bytecode =
            serde_json::from_str(&json).unwrap_or_else(|error| panic!("{file}: {error}"));
        let name = std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".bytecode.json"))
            .unwrap_or(file);
        measure(
            name,
            &bytecode.program(),
            full_state,
            Duration::from_millis(200),
        )
        .print(name);
    }
}

/// The synthetic programs, on [`Nothing`] unless the row names another
/// simulator, in nanoseconds per unit.
fn synthetic() {
    println!(
        "ns per unit on a simulator that does nothing (unless the row names another), the \
         fastest of {RUNS} runs; ×m, ×s, ×g are the machine alone, the simulator resumer, the \
         grouping resumer / run_shot"
    );
    println!(
        "{:<34} {:>8} {:>7} {:>10} {:>10} {:>10} {:>10} {:>6} {:>6} {:>6}",
        "program", "units", "batches", "machine", "sim.", "grouping", "run_shot", "×m", "×s", "×g"
    );
    quantum_rows();
    classical_rows();
}

/// The rows of quantum calls: gates, then correlated noise.
fn quantum_rows() {
    let gates = [
        (OPID_X, 0, 0),
        (OPID_H, 1, 0),
        (OPID_CX, 0, 1),
        (OPID_RZ, 2, 0),
    ];
    let constant: Vec<_> = (0..SYNTHETIC)
        .map(|i| {
            let (op_id, q1, q2) = gates[(i % 4) as usize];
            Instruction {
                src0: ANGLE.to_bits(),
                aux0: op_id,
                aux1: q1,
                aux2: q2,
                ..op(
                    OP_QUANTUM_GATE,
                    FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
                )
            }
        })
        .chain([ret()])
        .collect();
    row("quantum call, constant", &[&constant]);

    // Registers 0 to 2 hold the qubits 0 to 2, register 3 the angle.
    let mut from_registers: Vec<_> = (0..3)
        .map(|qubit| set(qubit, qubit))
        .chain([set(3, ANGLE.to_bits())])
        .collect();
    from_registers.extend((0..SYNTHETIC).map(|i| {
        let (op_id, q1, q2) = gates[(i % 4) as usize];
        Instruction {
            src0: 3,
            aux0: op_id,
            aux1: q1,
            aux2: q2,
            ..op(OP_QUANTUM_GATE, 0)
        }
    }));
    from_registers.push(ret());
    row("quantum call, from registers", &[&from_registers]);

    for qubits in 1..=3 {
        let bytecode = noise_program(qubits);
        let name = format!("correlated noise on {qubits}");
        row_of::<Nothing>(&name, &bytecode, &());
        row_of::<FullStateSimulator>(
            &format!("{name}, full state"),
            &bytecode,
            &noise_table(qubits),
        );
    }
}

/// The rows of classical instructions, of feed-forward rounds, then of
/// function calls.
fn classical_rows() {
    // Three classical instructions a turn: add, compare, branch.
    let turns = SYNTHETIC / 3;
    let counter = [
        binary(OP_ADD, FLAG_SRC1_IMM, 0, 0, 1),
        Instruction {
            opcode: u64::from(OP_ICMP) | (u64::from(ICMP_SLT) << 8) | FLAG_SRC1_IMM,
            dst: 1,
            src0: 0,
            src1: turns,
            ..Instruction::default()
        },
        branch(1, 1, 2),
    ];
    // The exit block records the counter, so that the runtimes' records
    // agree only if each counted alike.
    let exit = [record_int(0), ret()];
    row(
        "classical instruction",
        &[&[set(0, 0), jump(1)], &counter, &exit],
    );

    // A round: measure, read the result, count, compare, branch.
    let round = [
        Instruction {
            aux0: OPID_MZ,
            aux1: 0,
            aux2: 0,
            ..op(OP_MEASURE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
        },
        Instruction {
            dst: 2,
            src0: 0,
            ..op(OP_READ_RESULT, FLAG_SRC0_IMM)
        },
        counter[0],
        Instruction {
            src1: SYNTHETIC,
            ..counter[1]
        },
        counter[2],
    ];
    row(
        "feed-forward round",
        &[&[set(0, 0), jump(1)], &round, &exit],
    );

    // A turn: call a function of one parameter (register 3), the counter,
    // that returns it, kept in register 2; count, compare, branch. A unit
    // is a turn. The exit block records the value the last call kept, so
    // that a lost call or a wrong return changes the records.
    let turn = [
        Instruction {
            dst: 2,
            aux0: 0,
            aux1: 1,
            aux2: 0,
            ..op(OP_CALL, 0)
        },
        counter[0],
        Instruction {
            src1: SYNTHETIC,
            ..counter[1]
        },
        counter[2],
    ];
    let returns = Instruction {
        src0: 3,
        ..op(OP_CALL_RETURN, 0)
    };
    let mut bytecode = synthetic_program(&[
        &[set(0, 0), jump(1)],
        &turn,
        &[record_int(2), ret()],
        &[returns],
    ]);
    bytecode.function_table = vec![Function::from_tuple((3, 1, 3))];
    row_of::<Nothing>("call and return, a turn", &bytecode, &());
}

/// The angle of the synthetic programs' rotations.
const ANGLE: f64 = 0.5;

/// One row of [`synthetic`]: the program of `blocks`, [`SYNTHETIC`] units.
fn row(name: &str, blocks: &[&[Instruction<u64>]]) {
    row_of::<Nothing>(name, &synthetic_program(blocks), &());
}

/// One row of [`synthetic`]: `bytecode`, [`SYNTHETIC`] units, on `S` with
/// `noise`; its batches and the machine alone as on [`Nothing`].
fn row_of<S: Simulator<Noise: Clone>>(
    name: &str,
    bytecode: &AdaptiveProgram<u64>,
    noise: &S::Noise,
) {
    let program = QirProgram::new(bytecode).unwrap_or_else(|faults| panic!("{name}: {faults}"));
    let times = compare::<S>(name, bytecode, &program, noise, Duration::from_millis(200));
    #[allow(clippy::cast_precision_loss)]
    let per_unit = 1e3 / SYNTHETIC as f64;
    let Times {
        machine,
        simulator,
        grouping,
        reference,
    } = times;
    let (machine, simulator, grouping, reference) = (
        machine * per_unit,
        simulator * per_unit,
        grouping * per_unit,
        reference * per_unit,
    );
    println!(
        "{name:<34} {SYNTHETIC:>8} {:>7} {machine:>10.2} {simulator:>10.2} {grouping:>10.2} {reference:>10.2} {:>6.2} {:>6.2} {:>6.2}",
        batches(&program),
        machine / reference,
        simulator / reference,
        grouping / reference
    );
}

/// A program of `blocks`, the first its entry block, on 3 qubits, 1 result
/// and 4 registers, with an operation for each operation id at the index
/// of its id.
fn synthetic_program(blocks: &[&[Instruction<u64>]]) -> AdaptiveProgram<u64> {
    let mut instructions = Vec::new();
    let mut block_table = Vec::new();
    for block in blocks {
        block_table.push(Block {
            instr_offset: instructions.len() as u64,
            instr_count: block.len() as u64,
        });
        instructions.extend_from_slice(block);
    }
    AdaptiveProgram {
        num_qubits: 3,
        num_results: 1,
        num_registers: 4,
        entry_block: 0,
        instructions,
        block_table,
        function_table: vec![Default::default()],
        phi_entries: vec![Default::default()],
        switch_cases: vec![Default::default()],
        call_args: vec![0],
        constant_data: vec![0],
        quantum_ops: (0..32)
            .map(|op_id| Op::from_tuple((op_id, 0, 0, 0, 0)))
            .collect(),
    }
}

/// The noise table that [`noise_program`]s draw from.
const NOISE_TABLE: u64 = 7;

/// [`SYNTHETIC`] quantum calls of correlated noise on the qubits `0..qubits`
/// (at most 3), each held in the register of its number, from [`NOISE_TABLE`],
/// the call `k` on them rotated by `k` (from qubit `k mod qubits`); then each
/// of those qubits measured into the result of its number, and recorded. The
/// records, on a simulator that runs the noise, hold each qubit's parity of
/// faults, a draw's fault landing on the qubit that the call's rotation puts at
/// its position: so a machine or a resumer that misplaces the qubits, or loses
/// a quantum call (shifting the later draws onto other qubits), on 2 or 3
/// qubits differs from `run_shot` in about half of the shots or more. On 1
/// qubit a lost quantum call goes unseen unless the last draw faults; the
/// simulator resumer's tests pin it.
fn noise_program(qubits: u64) -> AdaptiveProgram<u64> {
    let mut block: Vec<_> = (0..3).map(|qubit| set(qubit, qubit)).collect();
    block.extend((0..SYNTHETIC).map(|k| Instruction {
        aux0: NOISE_OPERATION,
        aux1: qubits,
        aux2: k % qubits,
        ..op(OP_QUANTUM_GATE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }));
    block.extend((0..qubits).map(|qubit| Instruction {
        aux0: OPID_MZ,
        aux1: qubit,
        aux2: qubit,
        ..op(OP_MEASURE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }));
    block.extend((0..qubits).map(|result| Instruction {
        src0: result,
        aux1: RECORD_RESULT,
        ..op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM)
    }));
    block.push(ret());
    let mut bytecode = synthetic_program(&[&block]);
    bytecode.num_results = u32::try_from(qubits).expect("a few qubits");
    // The registers `0..qubits` twice, but the last, so that each
    // rotation's `qubits` arguments follow one another.
    bytecode.call_args = (0..2 * qubits - 1).map(|i| i % qubits).collect();
    bytecode.quantum_ops.push(Op::from_tuple((
        OPID_CORRELATED_NOISE,
        NOISE_TABLE,
        qubits,
        0,
        0,
    )));
    bytecode
}

/// The index of correlated noise's operation in a [`noise_program`]'s
/// table, after the one of each operation id below 32.
const NOISE_OPERATION: u64 = 32;

/// The kind of output a record of a result is (`aux1` of
/// `OP_RECORD_OUTPUT`).
const RECORD_RESULT: u64 = 0;

/// The kind of output a record of an integer is (`aux1` of
/// `OP_RECORD_OUTPUT`).
const RECORD_INT: u64 = 4;

/// A record of register `register` as an integer.
fn record_int(register: u64) -> Instruction<u64> {
    Instruction {
        src0: register,
        aux1: RECORD_INT,
        ..op(OP_RECORD_OUTPUT, 0)
    }
}

/// A noiseless configuration but for [`NOISE_TABLE`], on `qubits`
/// qubits: an X on one of them, each with probability 0.01.
fn noise_table(qubits: u64) -> Arc<CumulativeNoiseConfig> {
    let qubits = usize::try_from(qubits).expect("a few qubits");
    let mut config = NoiseConfig::<f64, f64>::NOISELESS;
    config.intrinsics.insert(
        u32::try_from(NOISE_TABLE).expect("a noise table id"),
        NoiseTable {
            qubits: u32::try_from(qubits).expect("a few qubits"),
            pauli_strings: (0..qubits)
                .map(|x| {
                    let string: String = (0..qubits)
                        .map(|qubit| if qubit == x { 'X' } else { 'I' })
                        .collect();
                    encode_pauli(&string)
                })
                .collect(),
            probabilities: vec![0.01; qubits],
            on_loss: LossPolicy::Skip,
        },
    );
    Arc::new(config.into())
}

/// An instruction of `opcode` with `flags`, its fields zero.
fn op(opcode: u8, flags: u64) -> Instruction<u64> {
    Instruction {
        opcode: u64::from(opcode) | flags,
        ..Instruction::default()
    }
}

/// The program's return.
fn ret() -> Instruction<u64> {
    op(OP_RET, FLAG_DST_IMM)
}

/// A jump to `block`.
fn jump(block: u64) -> Instruction<u64> {
    Instruction {
        dst: block,
        ..op(OP_JUMP, 0)
    }
}

/// A branch to `taken` when register `condition` holds 1, to `other` when
/// it holds 0.
fn branch(condition: u64, taken: u64, other: u64) -> Instruction<u64> {
    Instruction {
        src0: condition,
        aux0: taken,
        aux1: other,
        ..op(OP_BRANCH, 0)
    }
}

/// Register `dst` set to the constant `value`.
fn set(dst: u64, value: u64) -> Instruction<u64> {
    Instruction {
        dst,
        src0: value,
        ..op(OP_MOV, FLAG_SRC0_IMM)
    }
}

/// An instruction of `opcode` reading `src0` and `src1` (constants when
/// `flags` says so) and writing register `dst`.
fn binary(opcode: u8, flags: u64, dst: u64, src0: u64, src1: u64) -> Instruction<u64> {
    Instruction {
        dst,
        src0,
        src1,
        ..op(opcode, flags)
    }
}

/// The corpus of `run_shot`'s tests, in the form its tests read.
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

/// A case of the corpus: its program, by its name.
#[derive(Deserialize)]
struct Case {
    name: String,
    bytecode: Bytecode,
}

/// One row for the whole corpus: the times of its programs summed, and the
/// case whose ratio is the worst.
fn corpus(full_state: bool) {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../tests/run_shot_compatibility/run_shot_cases.json"
    ))
    .expect("the corpus is valid JSON of its format");
    let mut total = Row {
        full_state: Some(Times::default()),
        ..Row::default()
    };
    let mut skipped = 0;
    let mut worst = (String::new(), 0.0);
    let mut worst_grouping = (String::new(), 0.0);
    let mut most_batches = (String::new(), 0);
    for case in &corpus.cases {
        let row = measure(
            &case.name,
            &case.bytecode.program(),
            full_state,
            Duration::from_millis(2),
        );
        total.instructions += row.instructions;
        total.batches += row.batches;
        if row.batches > most_batches.1 {
            most_batches = (case.name.clone(), row.batches);
        }
        total.build += row.build;
        match (&mut total.full_state, row.full_state) {
            (Some(sum), Some(times)) => *sum = sum.add(&times),
            _ => skipped += 1,
        }
        total.nothing = total.nothing.add(&row.nothing);
        let ratio = row.nothing.simulator / row.nothing.reference;
        if ratio > worst.1 {
            worst = (case.name.clone(), ratio);
        }
        let ratio = row.nothing.grouping / row.nothing.reference;
        if ratio > worst_grouping.1 {
            worst_grouping = (case.name.clone(), ratio);
        }
    }
    if skipped == corpus.cases.len() {
        total.full_state = None;
    }
    total.print(&format!("corpus ({} cases)", corpus.cases.len()));
    println!(
        "  full state skipped for {skipped} cases; worst ×s on nothing: {:.2}, {}; worst ×g: {:.2}, {}; most batches: {}, {}",
        worst.1, worst.0, worst_grouping.1, worst_grouping.0, most_batches.1, most_batches.0
    );
}

/// The times of one program, in µs.
#[derive(Default)]
struct Row {
    instructions: usize,
    build: f64,
    /// The batches of a shot of the machine alone.
    batches: usize,
    /// A shot on a [`FullStateSimulator`]; its machine alone is unused.
    full_state: Option<Times>,
    /// A shot on [`Nothing`].
    nothing: Times,
}

impl Row {
    fn print(&self, name: &str) {
        let [
            full,
            full_grouping,
            full_reference,
            full_ratio,
            full_grouping_ratio,
        ] = match &self.full_state {
            Some(times) => [
                format!("{:.1}", times.simulator),
                format!("{:.1}", times.grouping),
                format!("{:.1}", times.reference),
                format!("{:.2}", times.simulator / times.reference),
                format!("{:.2}", times.grouping / times.reference),
            ],
            None => ["-"; 5].map(String::from),
        };
        let Times {
            machine,
            simulator,
            grouping,
            reference,
        } = self.nothing;
        println!(
            "{name:<34} {:>6} {:>7} {:>8.1} {full:>10} {full_grouping:>10} {full_reference:>10} {full_ratio:>6} {full_grouping_ratio:>6} {machine:>10.2} {simulator:>10.2} {grouping:>10.2} {reference:>10.2} {:>6.2} {:>6.2} {:>6.2}",
            self.instructions,
            self.batches,
            self.build,
            machine / reference,
            simulator / reference,
            grouping / reference
        );
    }
}

/// The times of `bytecode`, each run of a measurement taking about
/// `budget`; on a [`FullStateSimulator`] only when `full_state`.
fn measure(name: &str, bytecode: &AdaptiveProgram<u64>, full_state: bool, budget: Duration) -> Row {
    let build_once = || {
        let start = Instant::now();
        let program = QirProgram::new(bytecode).unwrap_or_else(|faults| panic!("{name}: {faults}"));
        (program, start.elapsed().as_secs_f64() * 1e6)
    };
    let (program, mut build) = build_once();
    for _ in 1..RUNS {
        build = build.min(build_once().1);
    }
    let full_state = (full_state && bytecode.num_qubits <= FULL_STATE_QUBITS).then(|| {
        let noise: Arc<CumulativeNoiseConfig> = Arc::new(<NoiseConfig<f64, f64>>::NOISELESS.into());
        compare::<FullStateSimulator>(name, bytecode, &program, &noise, budget)
    });
    Row {
        instructions: bytecode.instructions.len(),
        build,
        batches: batches(&program),
        full_state,
        nothing: compare::<Nothing>(name, bytecode, &program, &(), budget),
    }
}

/// The batches a shot of `program` yields, every outcome `Zero`.
fn batches(program: &QirProgram) -> usize {
    let machine = QirMachine::new(program);
    let mut context = machine.start_shot();
    let mut outcomes = Vec::new();
    let mut batches = 0;
    while let Ok(QirStop::Batch(batch)) = machine.resume(&mut context, &outcomes) {
        batches += 1;
        outcomes.clear();
        answer_zero(batch, &mut outcomes);
    }
    batches
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

/// The µs per shot of `bytecode`, built as `program`, on `S`, by each
/// runtime, once their records agree; the machine alone's as on
/// [`Nothing`], compared with `run_shot`'s on `S` only when `S` is
/// [`Nothing`].
#[allow(clippy::too_many_lines)]
fn compare<S: Simulator<Noise: Clone>>(
    name: &str,
    bytecode: &AdaptiveProgram<u64>,
    program: &QirProgram,
    noise: &S::Noise,
    budget: Duration,
) -> Times {
    let machine = QirMachine::new(program);
    let simulator = |shot: QirShotId| {
        S::new(
            bytecode.num_qubits as usize,
            bytecode.num_results as usize,
            shot.0,
            noise.clone(),
        )
    };
    let make = |_: &QirProgram, shot| simulator(shot);
    let by_simulator_resumer = |shots: &[QirShotId]| {
        expect_ready(QirSimulatorResumer::new(make).run(&machine, shots))
            .unwrap_or_else(|error| panic!("{name}: the simulator resumer's run fails: {error}"))
    };
    let by_grouping_resumer = |shots: &[QirShotId]| {
        expect_ready(QirGroupingResumer::new(make).run(&machine, shots))
            .unwrap_or_else(|error| panic!("{name}: the grouping resumer's run fails: {error}"))
    };
    let records_output = records_output(bytecode);
    let by_run_shot = |shot: QirShotId| {
        let mut simulator = simulator(shot);
        let records = run_shot(bytecode, &mut simulator);
        if records_output {
            records
        } else {
            simulator
                .take_measurements()
                .into_iter()
                .map(OutputRecord::Result)
                .collect()
        }
    };
    // One context for every shot, as a resumer keeps it.
    let mut context = machine.start_shot();
    let mut outcomes = Vec::new();
    let mut alone = |records: &mut Vec<OutputRecord>| {
        machine.restart_shot(&mut context);
        outcomes.clear();
        loop {
            match machine.resume(&mut context, &outcomes) {
                Ok(QirStop::Batch(batch)) => {
                    let batch = black_box(batch);
                    outcomes.clear();
                    answer_zero(batch, &mut outcomes);
                }
                Ok(QirStop::Ended(ended)) => {
                    records.clear();
                    records.extend_from_slice(ended);
                    break;
                }
                Err(trap) => panic!("{name}: a shot traps: {trap}"),
            }
        }
    };

    let start = Instant::now();
    let first = by_run_shot(QirShotId(0));
    let once = start.elapsed();
    let agreement = u32::try_from(Duration::from_secs(1).as_nanos() / once.as_nanos().max(1))
        .unwrap_or(u32::MAX)
        .clamp(1, AGREEMENT_SHOTS);
    let shots = QirShotId::first(agreement);
    let per_shot = by_simulator_resumer(&shots);
    let grouped = by_grouping_resumer(&shots);
    let nothing = std::any::type_name::<S>() == std::any::type_name::<Nothing>();
    let mut records = Vec::new();
    for &shot in &shots {
        let reference = if shot.0 == 0 {
            first.clone()
        } else {
            by_run_shot(shot)
        };
        let place = shot.0 as usize;
        assert_eq!(
            per_shot[place], reference,
            "{name}: {shot} by the simulator resumer differs"
        );
        assert_eq!(
            grouped[place], reference,
            "{name}: {shot} by the grouping resumer differs"
        );
        if nothing {
            alone(&mut records);
            assert_eq!(
                records, reference,
                "{name}: {shot} by the machine alone differs"
            );
        }
    }

    let count = u32::try_from(budget.as_nanos() / once.as_nanos().max(1))
        .unwrap_or(u32::MAX)
        .clamp(1, MOST_SHOTS);
    let shots = QirShotId::first(count);
    let mut fastest = Times {
        machine: f64::INFINITY,
        simulator: f64::INFINITY,
        grouping: f64::INFINITY,
        reference: f64::INFINITY,
    };
    for _ in 0..RUNS {
        let start = Instant::now();
        for _ in &shots {
            alone(&mut records);
            black_box(&records);
        }
        fastest.machine = fastest.machine.min(start.elapsed().as_secs_f64());
        let start = Instant::now();
        black_box(by_simulator_resumer(black_box(&shots)));
        fastest.simulator = fastest.simulator.min(start.elapsed().as_secs_f64());
        let start = Instant::now();
        black_box(by_grouping_resumer(black_box(&shots)));
        fastest.grouping = fastest.grouping.min(start.elapsed().as_secs_f64());
        let start = Instant::now();
        for &shot in &shots {
            black_box(by_run_shot(black_box(shot)));
        }
        fastest.reference = fastest.reference.min(start.elapsed().as_secs_f64());
    }
    let per_shot = 1e6 / f64::from(count);
    Times {
        machine: fastest.machine * per_shot,
        simulator: fastest.simulator * per_shot,
        grouping: fastest.grouping * per_shot,
        reference: fastest.reference * per_shot,
    }
}

/// A program's µs per shot by each runtime ([`compare`]).
#[derive(Clone, Copy, Default)]
struct Times {
    /// The machine alone, every outcome `Zero`.
    machine: f64,
    /// The [`QirSimulatorResumer`].
    simulator: f64,
    /// The [`QirGroupingResumer`].
    grouping: f64,
    /// [`run_shot`].
    reference: f64,
}

impl Times {
    /// The sums of two rows' times, runtime by runtime.
    fn add(&self, other: &Times) -> Times {
        Times {
            machine: self.machine + other.machine,
            simulator: self.simulator + other.simulator,
            grouping: self.grouping + other.grouping,
            reference: self.reference + other.reference,
        }
    }
}

/// Whether `bytecode` records any output; when it does not, a shot's
/// records are every result, as the machine gives them.
fn records_output(bytecode: &AdaptiveProgram<u64>) -> bool {
    bytecode
        .instructions
        .iter()
        .any(|instruction| instruction.primary_opcode() == OP_RECORD_OUTPUT)
}

/// A simulator that does nothing, and measures every result as `Zero`;
/// a result written is kept, as the simulators keep it.
struct Nothing {
    measurements: Vec<MeasurementResult>,
}

impl Simulator for Nothing {
    type Noise = ();
    type StateDumpData = ();

    fn new(_: usize, num_results: usize, _: u32, (): ()) -> Self {
        Nothing {
            measurements: vec![MeasurementResult::Zero; num_results],
        }
    }
    fn x(&mut self, _: QubitID) {}
    fn y(&mut self, _: QubitID) {}
    fn z(&mut self, _: QubitID) {}
    fn h(&mut self, _: QubitID) {}
    fn s(&mut self, _: QubitID) {}
    fn s_adj(&mut self, _: QubitID) {}
    fn sx(&mut self, _: QubitID) {}
    fn sx_adj(&mut self, _: QubitID) {}
    fn t(&mut self, _: QubitID) {}
    fn t_adj(&mut self, _: QubitID) {}
    fn rx(&mut self, _: f64, _: QubitID) {}
    fn ry(&mut self, _: f64, _: QubitID) {}
    fn rz(&mut self, _: f64, _: QubitID) {}
    fn cx(&mut self, _: QubitID, _: QubitID) {}
    fn cy(&mut self, _: QubitID, _: QubitID) {}
    fn cz(&mut self, _: QubitID, _: QubitID) {}
    fn rxx(&mut self, _: f64, _: QubitID, _: QubitID) {}
    fn ryy(&mut self, _: f64, _: QubitID, _: QubitID) {}
    fn rzz(&mut self, _: f64, _: QubitID, _: QubitID) {}
    fn swap(&mut self, _: QubitID, _: QubitID) {}
    fn mz(&mut self, _: QubitID, result_id: ResultID) {
        self.measurements[result_id] = MeasurementResult::Zero;
    }
    fn mresetz(&mut self, _: QubitID, result_id: ResultID) {
        self.measurements[result_id] = MeasurementResult::Zero;
    }
    fn resetz(&mut self, _: QubitID) {}
    fn mov(&mut self, _: QubitID) {}
    fn correlated_noise_intrinsic(&mut self, _: u32, _: &[usize]) {}
    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }
    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }
    fn state_dump(&self) -> &() {
        &()
    }
    fn peek_loss(&mut self, _: QubitID, _: ResultID) {}
    fn apply_loss_noise(&mut self, _: f64, _: QubitID) {}
    fn apply_readout_noise(&mut self, _: f64, _: f64, _: ResultID) {}
    fn write_result(&mut self, value: bool, result_id: ResultID) {
        self.measurements[result_id] = if value {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
    }
}
