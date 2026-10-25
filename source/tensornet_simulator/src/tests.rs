// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's tests, by subject:
//!
//! | Module          | What it checks                                                        |
//! | --------------- | --------------------------------------------------------------------- |
//! | `programs`      | M0's ④ preflight and replay, ported (execution and consumer tests)    |
//! | `batch`         | the batch strategy: needs, seeds, submissions, failures (MPS7–MPS13)  |
//! | `per_shot`      | `PerShot(TensorNetShots)`: draws, ln p, replayability, bonds (MPS1–4) |
//! |                 | and phase estimation with feedback against the legacy runtime         |
//! | `queries`       | state queries (MPS14–MPS17, MPS19) and their preflight                |
//! | `parity`        | parity with M0: the sampler seed, the samples, the records (L10)      |
//! | `qualification` | the A100 qualification and H₂ QPE, `#[ignore]`d (Linux x86-64 only)   |
//!
//! The cases that hold for any engine are generic over [`MpsEngine`] in
//! `cases`, run here on [`ExactReference`] and on the A100 by
//! `qualification` on cuTensorNet.

use std::{cell::Cell, f64::consts::FRAC_1_SQRT_2};

use executor_protocol::{
    Batch, Executor, Rejected, ShotId, ShotOutcome, ShotSet, Submission, SubmissionRef, Value, wait,
};
use num_complex::Complex64;
use qdk_simulators::{
    QubitID,
    bytecode::{AdaptiveProgram, Block, Instruction, Op},
    execution::{
        MeasurementKind, MeasurementRequest, PauliSum, PreparedAdaptiveProgram,
        QuantumEvolutionRegion, RegionId, RegionInstruction, RegionPartitionError,
        UnitaryOperation,
    },
};
use tensornet::{
    MpsEngine, MpsShape, SampleSeed,
    reference::{ExactReference, ReferenceError, ReferenceState},
};

// The reader of H₂'s bytecode JSON that `qdk_simulators`'s `h2_qpe` test
// uses, for `qualification`'s H₂ run; test support, not API.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "../../simulators/tests/common/adaptive_program_json.rs"]
mod adaptive_program_json;
mod batch;
mod cases;
mod parity;
mod per_shot;
mod programs;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod qualification;
mod queries;

// Bytecode, as M0's tests built it.
const IMMEDIATE_SRC0: u64 = 1 << 16;
const IMMEDIATE_SRC1: u64 = 1 << 17;
const IMMEDIATE_AUX1: u64 = 1 << 20;
const IMMEDIATE_AUX2: u64 = 1 << 21;
const OP_QUANTUM_GATE: u64 = 0x10;
const OP_MEASURE: u64 = 0x11;
const OP_RESET: u64 = 0x12;
const OP_READ_RESULT: u64 = 0x13;
const OP_RECORD_OUTPUT: u64 = 0x14;
const OP_RET: u64 = 0x02;
const OP_JUMP: u64 = 0x04;
const OP_BRANCH: u64 = 0x05;
const OP_OR: u64 = 0x29;
/// `ICMP` with predicate 1, `ne`, in the sub-opcode (bits 15:8).
const OP_ICMP_NE: u64 = 0x30 | (1 << 8);

pub(super) const OPID_RESETZ: u64 = 1;
pub(super) const OPID_X: u64 = 2;
pub(super) const OPID_Y: u64 = 3;
pub(super) const OPID_Z: u64 = 4;
pub(super) const OPID_H: u64 = 5;
pub(super) const OPID_S: u64 = 6;
pub(super) const OPID_RX: u64 = 12;
pub(super) const OPID_RZ: u64 = 14;
pub(super) const OPID_CX: u64 = 15;
pub(super) const OPID_CZ: u64 = 16;
pub(super) const OPID_MZ: u64 = 21;
pub(super) const OPID_MRESETZ: u64 = 22;

/// A Base or Adaptive program written instruction by instruction: each
/// operation is added to the program's table once, on first use.
#[derive(Debug)]
pub(super) struct Bytecode {
    qubits: u32,
    results: u32,
    instructions: Vec<Instruction<u64>>,
    operations: Vec<Op<u64>>,
}

// `Op` is not `Clone`.
impl Clone for Bytecode {
    fn clone(&self) -> Self {
        Self {
            qubits: self.qubits,
            results: self.results,
            instructions: self.instructions.clone(),
            operations: self
                .operations
                .iter()
                .map(|op| Op {
                    op_id: op.op_id,
                    q1: op.q1,
                    q2: op.q2,
                    q3: op.q3,
                    angle: op.angle,
                })
                .collect(),
        }
    }
}

impl Bytecode {
    pub(super) fn new(qubits: u32) -> Self {
        Self {
            qubits,
            results: 0,
            instructions: Vec::new(),
            operations: Vec::new(),
        }
    }

    fn operation(&mut self, op_id: u64) -> u64 {
        let index = self
            .operations
            .iter()
            .position(|operation| operation.op_id == op_id)
            .unwrap_or_else(|| {
                self.operations.push(Op {
                    op_id,
                    q1: 0,
                    q2: 0,
                    q3: 0,
                    angle: 0,
                });
                self.operations.len() - 1
            });
        index as u64
    }

    /// Gate `op_id` on `q1` (and `q2` when it takes two), at `angle`.
    pub(super) fn gate(mut self, op_id: u64, q1: u64, q2: u64, angle: f64) -> Self {
        let aux0 = self.operation(op_id);
        self.instructions.push(Instruction {
            opcode: OP_QUANTUM_GATE | IMMEDIATE_SRC0 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0,
            aux1: q1,
            aux2: q2,
            src0: angle.to_bits(),
            ..Instruction::default()
        });
        self
    }

    pub(super) fn h(self, q: u64) -> Self {
        self.gate(OPID_H, q, 0, 0.0)
    }

    pub(super) fn x(self, q: u64) -> Self {
        self.gate(OPID_X, q, 0, 0.0)
    }

    pub(super) fn s(self, q: u64) -> Self {
        self.gate(OPID_S, q, 0, 0.0)
    }

    pub(super) fn rx(self, angle: f64, q: u64) -> Self {
        self.gate(OPID_RX, q, 0, angle)
    }

    pub(super) fn rz(self, angle: f64, q: u64) -> Self {
        self.gate(OPID_RZ, q, 0, angle)
    }

    pub(super) fn cx(self, control: u64, target: u64) -> Self {
        self.gate(OPID_CX, control, target, 0.0)
    }

    /// Measurement `op_id` (`MZ` or `MResetZ`) of `qubit` into `result`.
    pub(super) fn measurement(mut self, op_id: u64, qubit: u64, result: u64) -> Self {
        let aux0 = self.operation(op_id);
        self.results = self
            .results
            .max(u32::try_from(result + 1).expect("a small result"));
        self.instructions.push(Instruction {
            opcode: OP_MEASURE | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0,
            aux1: qubit,
            aux2: result,
            ..Instruction::default()
        });
        self
    }

    pub(super) fn mz(self, qubit: u64, result: u64) -> Self {
        self.measurement(OPID_MZ, qubit, result)
    }

    pub(super) fn mresetz(self, qubit: u64, result: u64) -> Self {
        self.measurement(OPID_MRESETZ, qubit, result)
    }

    pub(super) fn reset(mut self, qubit: u64) -> Self {
        let aux0 = self.operation(OPID_RESETZ);
        self.instructions.push(Instruction {
            opcode: OP_RESET | IMMEDIATE_AUX1,
            aux0,
            aux1: qubit,
            ..Instruction::default()
        });
        self
    }

    pub(super) fn record(mut self, result: u64) -> Self {
        self.results = self
            .results
            .max(u32::try_from(result + 1).expect("a small result"));
        self.instructions.push(Instruction {
            opcode: OP_RECORD_OUTPUT | IMMEDIATE_SRC0,
            src0: result,
            ..Instruction::default()
        });
        self
    }

    /// Measures each of `qubits` into the next result and records it.
    pub(super) fn measure_all(mut self, qubits: &[u64]) -> Self {
        let first = u64::from(self.results);
        for (k, &qubit) in (0..).zip(qubits) {
            self = self.mz(qubit, first + k);
        }
        for k in 0..qubits.len() as u64 {
            self = self.record(first + k);
        }
        self
    }

    pub(super) fn try_build(
        mut self,
    ) -> Result<PreparedAdaptiveProgram<u64>, RegionPartitionError> {
        self.instructions.push(Instruction {
            opcode: OP_RET,
            ..Instruction::default()
        });
        let all = self.instructions.len() as u64;
        self.prepare(vec![Block {
            instr_offset: 0,
            instr_count: all,
        }])
    }

    fn prepare(
        self,
        block_table: Vec<Block<u64>>,
    ) -> Result<PreparedAdaptiveProgram<u64>, RegionPartitionError> {
        PreparedAdaptiveProgram::new(AdaptiveProgram {
            num_qubits: self.qubits,
            num_results: self.results,
            num_registers: 1,
            entry_block: 0,
            block_table,
            instructions: self.instructions,
            function_table: Vec::new(),
            phi_entries: Vec::new(),
            switch_cases: Vec::new(),
            call_args: Vec::new(),
            constant_data: Vec::new(),
            quantum_ops: self.operations,
        })
    }

    pub(super) fn build(self) -> PreparedAdaptiveProgram<u64> {
        self.try_build().expect("test program should prepare")
    }
}

/// H on qubit 0 in a loop that runs its region twice, then qubit 0 measured
/// and recorded. The loop is classical: block 0 is the region and a
/// `BRANCH` on register 0, which is 0 the first time, so block 2 sets it
/// with an `OR` and jumps back; block 1 measures. HH = I, so every shot
/// reads Zero, where a region applied once would read One half the time.
pub(super) fn region_run_twice() -> PreparedAdaptiveProgram<u64> {
    let mut program = Bytecode::new(1).h(0);
    program.instructions.extend([
        Instruction {
            opcode: OP_BRANCH,
            src0: 0,
            aux0: 1,
            aux1: 2,
            ..Instruction::default()
        },
        Instruction {
            opcode: OP_OR | IMMEDIATE_SRC0 | IMMEDIATE_SRC1,
            dst: 0,
            src0: 1,
            src1: 0,
            ..Instruction::default()
        },
        Instruction {
            opcode: OP_JUMP,
            dst: 0,
            ..Instruction::default()
        },
    ]);
    let mut program = program.mz(0, 0).record(0);
    program.instructions.push(Instruction {
        opcode: OP_RET,
        ..Instruction::default()
    });
    let block = |instr_offset, instr_count| Block {
        instr_offset,
        instr_count,
    };
    program
        .prepare(vec![block(0, 2), block(4, 3), block(2, 2)])
        .expect("a loop prepares")
}

/// The ancilla of [`iterative_phase_estimation`]; its target is qubit 2, so
/// that the controlled phase acts across an idle qubit, as H₂'s gates act
/// on qubits far apart in the chain.
const IPE_ANCILLA: u64 = 0;
const IPE_TARGET: u64 = 2;

/// Iterative phase estimation of `phase` (in turns) to `bits` bits: H₂'s
/// kind of program at three qubits. The target holds |1⟩, the eigenstate of
/// U = P(2π·`phase`); iteration i applies controlled-U^(2^(bits−1−i)) from
/// the ancilla and measures it into result i, which is bit bits−i of the
/// estimate. Before that, each earlier result j that read One rotates the
/// ancilla by −π/2^(i−j): a `READ_RESULT`, an `ICMP ne` and a `BRANCH` to a
/// block with the `Rz`, as H₂ conditions its gates. Even iterations measure
/// with `MResetZ`; odd ones with `MZ` and then `RESET` the ancilla. The
/// results are recorded most significant bit first, so a row read as a
/// binary number is the estimate x of 2^bits·`phase`. This is Griffiths and
/// Niu's semiclassical Fourier transform, so x is distributed as in
/// textbook phase estimation ([`phase_estimation_distribution`]).
pub(super) fn iterative_phase_estimation(bits: u32, phase: f64) -> PreparedAdaptiveProgram<u64> {
    let mut program = Bytecode::new(3).x(IPE_TARGET);
    let mut starts = vec![0];
    for i in 0..u64::from(bits) {
        program = program.h(IPE_ANCILLA);
        for j in 0..i {
            let apply = starts.len() as u64;
            // Register 0 holds result j, then whether it is One.
            program.instructions.extend([
                Instruction {
                    opcode: OP_READ_RESULT | IMMEDIATE_SRC0,
                    dst: 0,
                    src0: j,
                    ..Instruction::default()
                },
                Instruction {
                    opcode: OP_ICMP_NE | IMMEDIATE_SRC1,
                    dst: 0,
                    src0: 0,
                    src1: 0,
                    ..Instruction::default()
                },
                Instruction {
                    opcode: OP_BRANCH,
                    src0: 0,
                    aux0: apply,
                    aux1: apply + 1,
                    ..Instruction::default()
                },
            ]);
            starts.push(program.instructions.len());
            let correction = -std::f64::consts::PI / f64::from(1u32 << (i - j));
            program = program.rz(correction, IPE_ANCILLA);
            program.instructions.push(Instruction {
                opcode: OP_JUMP,
                dst: apply + 1,
                ..Instruction::default()
            });
            starts.push(program.instructions.len());
        }
        let power = f64::from(1u32 << (u64::from(bits) - 1 - i));
        let theta = std::f64::consts::TAU * phase * power;
        // Controlled-P(θ) = P(θ/2)_a P(θ/2)_t CX P(−θ/2)_t CX, with Rz for P:
        // the phases Rz adds are global.
        program = program
            .rz(theta / 2.0, IPE_ANCILLA)
            .rz(theta / 2.0, IPE_TARGET)
            .cx(IPE_ANCILLA, IPE_TARGET)
            .rz(-theta / 2.0, IPE_TARGET)
            .cx(IPE_ANCILLA, IPE_TARGET)
            .h(IPE_ANCILLA);
        program = if i % 2 == 0 {
            program.mresetz(IPE_ANCILLA, i)
        } else {
            program.mz(IPE_ANCILLA, i).reset(IPE_ANCILLA)
        };
    }
    for i in (0..u64::from(bits)).rev() {
        program = program.record(i);
    }
    program.instructions.push(Instruction {
        opcode: OP_RET,
        ..Instruction::default()
    });
    starts.push(program.instructions.len());
    let blocks = starts
        .windows(2)
        .map(|pair| Block {
            instr_offset: pair[0] as u64,
            instr_count: (pair[1] - pair[0]) as u64,
        })
        .collect();
    program.prepare(blocks).expect("phase estimation prepares")
}

/// The probability of each estimate x in `0..2^bits` of 2^bits·`phase` in
/// textbook phase estimation: |2^−bits Σᵧ e^(2πi·y·(phase − x/2^bits))|².
pub(super) fn phase_estimation_distribution(bits: u32, phase: f64) -> Vec<f64> {
    let n = 1u32 << bits;
    (0..n)
        .map(|x| {
            let delta = phase - f64::from(x) / f64::from(n);
            let (re, im) = (0..n).fold((0.0, 0.0), |(re, im), y| {
                let angle = std::f64::consts::TAU * f64::from(y) * delta;
                (re + angle.cos(), im + angle.sin())
            });
            (re * re + im * im) / f64::from(n * n)
        })
        .collect()
}

/// |Φ⁺⟩ on qubits 0 and 1, both measured and recorded.
pub(super) fn bell() -> PreparedAdaptiveProgram<u64> {
    Bytecode::new(2).h(0).cx(0, 1).measure_all(&[0, 1]).build()
}

/// A region instruction applying `operations` in order.
pub(super) fn region(operations: impl Into<Box<[UnitaryOperation]>>) -> RegionInstruction {
    RegionInstruction::ExecuteRegion {
        region_id: RegionId::new(0),
        region: QuantumEvolutionRegion::new(operations),
    }
}

pub(super) fn measure(qubit: QubitID, result_id: usize) -> RegionInstruction {
    RegionInstruction::Measure(MeasurementRequest {
        kind: MeasurementKind::MeasureZ,
        qubit,
        result_id,
    })
}

pub(super) fn measure_reset(qubit: QubitID, result_id: usize) -> RegionInstruction {
    RegionInstruction::Measure(MeasurementRequest {
        kind: MeasurementKind::MeasureResetZ,
        qubit,
        result_id,
    })
}

pub(super) fn h(target: QubitID) -> UnitaryOperation {
    UnitaryOperation::H { target }
}

pub(super) fn cx(control: QubitID, target: QubitID) -> UnitaryOperation {
    UnitaryOperation::Cx { control, target }
}

/// Two Bell pairs, (0, 2) and (1, 3), across the middle of four qubits: the
/// middle bond is 4.
pub(super) fn crossing_pairs() -> [UnitaryOperation; 4] {
    [h(0), cx(0, 2), h(1), cx(1, 3)]
}

/// `coefficient·labels` on `qubits`, as a sum.
pub(super) fn pauli(coefficient: f64, labels: &str, qubits: &[QubitID]) -> PauliSum {
    let mut sum = PauliSum::new();
    sum.push_labels(Complex64::new(coefficient, 0.0), labels, qubits)
        .expect("a well-formed term");
    sum
}

/// An L3 driver: one batch, its outcomes as (tag, value), and how it ended.
pub(super) fn run_batch<X: Executor<Batch<RegionInstruction>>>(
    executor: &mut X,
    batch: &[RegionInstruction],
) -> (Vec<(u32, Value)>, Result<(), String>) {
    let mut out = Vec::new();
    let end = wait(executor.call(batch).expect("accepted"), &mut out);
    (out.into_iter().map(|o| (o.tag, o.value)).collect(), end)
}

/// An L2 driver: one submission on `shots`, its events and how it ended.
pub(super) fn submit<X: Executor<Submission<RegionInstruction>>>(
    session: &mut X,
    shots: &ShotSet,
    batch: &[RegionInstruction],
) -> Result<(Vec<ShotOutcome>, Result<(), String>), Rejected> {
    let mut out = Vec::new();
    let reply = session.call(SubmissionRef { shots, batch })?;
    let end = wait(reply, &mut out);
    Ok((out, end))
}

/// The value of each shot's `tag`, in event order.
pub(super) fn values_of(events: &[ShotOutcome], tag: u32) -> Vec<(ShotId, Value)> {
    events
        .iter()
        .filter(|event| event.tag == tag)
        .map(|event| (event.shot, event.value))
        .collect()
}

/// `ExactReference`, counting the states it prepares, or refusing to
/// prepare its first ones (as an engine without a device, or past its
/// budget until memory is freed).
#[derive(Default)]
pub(super) struct TestDoubleEngine {
    prepared: Cell<usize>,
    refusals: Cell<usize>,
}

impl TestDoubleEngine {
    pub(super) fn counting() -> Self {
        Self::default()
    }

    /// Refuses every state.
    pub(super) fn refusing() -> Self {
        Self::refusing_first(usize::MAX)
    }

    /// Refuses the first `refusals` states, then prepares.
    pub(super) fn refusing_first(refusals: usize) -> Self {
        Self {
            prepared: Cell::new(0),
            refusals: Cell::new(refusals),
        }
    }

    /// How many states it prepared (refused ones included).
    pub(super) fn prepared(&self) -> usize {
        self.prepared.get()
    }
}

/// The double's own refusal, beside the reference's errors.
#[derive(Debug, thiserror::Error)]
pub(super) enum TestDoubleError {
    #[error("the test double refuses to prepare a state")]
    Refused,
    #[error(transparent)]
    Reference(#[from] ReferenceError),
}

impl MpsEngine for TestDoubleEngine {
    type State = ReferenceState;
    type Error = TestDoubleError;

    fn prepare(&self, capacity: &MpsShape) -> Result<ReferenceState, TestDoubleError> {
        self.prepared.set(self.prepared.get() + 1);
        if self.refusals.get() > 0 {
            self.refusals.set(self.refusals.get() - 1);
            return Err(TestDoubleError::Refused);
        }
        Ok(ExactReference.prepare(capacity)?)
    }

    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), TestDoubleError> {
        Ok(ExactReference.apply(state, sites, matrix)?)
    }

    fn probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
    ) -> Result<f64, TestDoubleError> {
        Ok(ExactReference.probability_of_one(state, site)?)
    }

    fn project(
        &self,
        state: &mut ReferenceState,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), TestDoubleError> {
        Ok(ExactReference.project(state, site, one, probability)?)
    }

    fn fork(&self, state: &ReferenceState) -> Result<ReferenceState, TestDoubleError> {
        Ok(ExactReference.fork(state)?)
    }

    fn sample(
        &self,
        state: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, TestDoubleError> {
        Ok(ExactReference.sample(state, sites, shots, seed)?)
    }

    fn expectation(
        &self,
        state: &ReferenceState,
        observable: &tensornet::PauliSum,
    ) -> Result<Complex64, TestDoubleError> {
        Ok(ExactReference.expectation(state, observable)?)
    }

    fn shape(&self, state: &ReferenceState) -> Result<MpsShape, TestDoubleError> {
        Ok(ExactReference.shape(state)?)
    }
}

#[test]
fn the_counting_double_is_an_engine() {
    let failures = tensornet::conformance::check(&TestDoubleEngine::counting());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_capacity_caps_each_bond_at_the_smaller_side_and_at_chi() {
    let capacity = crate::mps_capacity(6, 4).expect("a chain");
    assert_eq!(capacity.max_bond(), 4);
    assert_eq!(
        capacity.element_counts(),
        [2 * 2, 2 * 2 * 4, 4 * 2 * 4, 4 * 2 * 4, 4 * 2 * 2, 2 * 2]
    );
    assert_eq!(crate::mps_capacity(1, 4).map(|c| c.max_bond()), Ok(1));
    assert!(crate::mps_capacity(0, 4).is_err());
    assert!(crate::mps_capacity(3, 0).is_err());
    // χ beyond 2^63 saturates instead of overflowing, and a chain whose
    // elements do not fit a usize is refused, not wrapped.
    assert_eq!(
        crate::mps_capacity(40, usize::MAX).map(|c| c.max_bond()),
        Ok(1 << 20)
    );
    assert!(matches!(
        crate::mps_capacity(200, usize::MAX),
        Err(tensornet::MpsShapeError::ElementCountOverflow { .. })
    ));
}

#[test]
fn h_and_cx_make_a_bell_pair_on_the_reference() {
    let engine = ExactReference::new();
    let mut state = engine
        .prepare(&crate::mps_capacity(2, 2).expect("a chain"))
        .expect("prepares");
    for operation in [h(0), cx(0, 1), UnitaryOperation::I { target: 1 }] {
        crate::lowering::apply_operation(&engine, &mut state, operation).expect("applies");
    }
    for (index, amplitude) in [FRAC_1_SQRT_2, 0.0, 0.0, FRAC_1_SQRT_2]
        .into_iter()
        .enumerate()
    {
        assert!(
            (state.amplitude(index) - amplitude).norm() < 1e-12,
            "{index}"
        );
    }
    assert!(!crate::lowering::is_applied(UnitaryOperation::I {
        target: 0
    }));
}
