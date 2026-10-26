// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gate fusion ([`GateFusion`]): every fusion applies the product of a
//! region's gates, so fused and unfused shots draw the same values with the
//! same probabilities; no operator crosses a measurement; and the edge
//! cases of the grouping (operand order, distant pairs, idle qubits,
//! single-qubit regions, empty regions). The blocks of up to three qubits
//! that `fusion::blocks` also makes, which `GateFusion` does not offer (no
//! engine of M1 applies them on the A100), are checked here too, for the
//! counts.
//!
//! `h2_operators_per_shot` counts the operators of H₂'s bytecode under each
//! fusion, on CPU (`#[ignore]`d: it needs `QDK_H2_BYTECODE`).

use std::{env, fs};

use executor_protocol::{Close as _, PerShot, ShotSet, SubmissionWrapper, open};
use num_complex::Complex64;
use qdk_simulators::{
    OutputRecord, QubitID,
    bytecode::{AdaptiveProgram, Block, Instruction},
    execution::{PreparedAdaptiveProgram, RuntimeAdapter, UnitaryOperation},
};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use tensornet::{MpsEngine, MpsShape, SampleSeed, reference::ExactReference};

use super::{
    Bytecode, OP_BRANCH, OP_ICMP_NE, OP_JUMP, OP_READ_RESULT, OP_RET, cx, h, measure,
    measure_reset, region, run_batch,
};
use crate::{
    GateFusion, MpsStack, QubitOperator, TensorNetReport, TensorNetShots, TensorNetTerminal,
    fusion::blocks, mps_capacity,
};

/// A fusion's operators: the public ones, or blocks of up to three qubits.
#[derive(Clone, Copy, Debug)]
enum Grouping {
    Public(GateFusion),
    UpToThree,
}

const GROUPINGS: [Grouping; 3] = [
    Grouping::Public(GateFusion::Unfused),
    Grouping::Public(GateFusion::Pairs),
    Grouping::UpToThree,
];

impl Grouping {
    fn operators(self, operations: &[UnitaryOperation]) -> Vec<QubitOperator> {
        match self {
            Self::Public(fusion) => fusion.operators(operations),
            Self::UpToThree => blocks(operations, 3),
        }
    }

    fn max_qubits(self) -> usize {
        match self {
            Self::Public(_) => 2,
            Self::UpToThree => 3,
        }
    }
}

fn z(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

/// Every unitary operation of the region ISA on `qubits` qubits (two or
/// more for the two-qubit ones), with random operands and angles.
pub(super) fn random_operation(rng: &mut StdRng, qubits: usize) -> UnitaryOperation {
    let target = rng.random_range(0..qubits);
    let angle = rng.random_range(-std::f64::consts::PI..std::f64::consts::PI);
    let kinds = if qubits >= 2 { 21 } else { 14 };
    let kind = rng.random_range(0..kinds);
    if kind >= 14 {
        let other = (target + rng.random_range(1..qubits)) % qubits;
        let (q1, q2) = (target, other);
        return match kind {
            14 => UnitaryOperation::Cx {
                control: q1,
                target: q2,
            },
            15 => UnitaryOperation::Cy {
                control: q1,
                target: q2,
            },
            16 => UnitaryOperation::Cz {
                control: q1,
                target: q2,
            },
            17 => UnitaryOperation::Rxx { angle, q1, q2 },
            18 => UnitaryOperation::Ryy { angle, q1, q2 },
            19 => UnitaryOperation::Rzz { angle, q1, q2 },
            _ => UnitaryOperation::Swap { q1, q2 },
        };
    }
    match kind {
        0 => UnitaryOperation::I { target },
        1 => UnitaryOperation::X { target },
        2 => UnitaryOperation::Y { target },
        3 => UnitaryOperation::Z { target },
        4 => UnitaryOperation::H { target },
        5 => UnitaryOperation::S { target },
        6 => UnitaryOperation::SAdj { target },
        7 => UnitaryOperation::Sx { target },
        8 => UnitaryOperation::SxAdj { target },
        9 => UnitaryOperation::T { target },
        10 => UnitaryOperation::TAdj { target },
        11 => UnitaryOperation::Rx { angle, target },
        12 => UnitaryOperation::Ry { angle, target },
        _ => UnitaryOperation::Rz { angle, target },
    }
}

/// The amplitudes of `qubits` qubits after `prefix`, applied unfused (a
/// generic state rather than |0…0⟩), then `operators`, on the reference.
fn amplitudes(
    qubits: usize,
    prefix: &[UnitaryOperation],
    operators: &[QubitOperator],
) -> Vec<Complex64> {
    let engine = ExactReference::new();
    let mut state = engine
        .prepare(&mps_capacity(qubits, 1 << qubits).expect("a chain"))
        .expect("prepares");
    for operator in GateFusion::Unfused
        .operators(prefix)
        .iter()
        .chain(operators)
    {
        engine
            .apply(&mut state, operator.qubits(), operator.matrix())
            .expect("applies");
    }
    (0..1 << qubits).map(|i| state.amplitude(i)).collect()
}

fn distance(a: &[Complex64], b: &[Complex64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).norm())
        .fold(0.0, f64::max)
}

/// The largest |(M†M − 1)ᵢⱼ| of an operator's matrix.
fn unitarity_defect(operator: &QubitOperator) -> f64 {
    let dimension = 1 << operator.qubits().len();
    let m = operator.matrix();
    let mut defect: f64 = 0.0;
    for i in 0..dimension {
        for j in 0..dimension {
            let product: Complex64 = (0..dimension)
                .map(|k| m[k * dimension + i].conj() * m[k * dimension + j])
                .sum();
            let identity = if i == j { 1.0 } else { 0.0 };
            defect = defect.max((product - identity).norm());
        }
    }
    defect
}

/// Checks the shape of `operators`: distinct qubits, at most `max_qubits`,
/// ascending when fused, `4^k` elements, unitary.
fn check_shape(operators: &[QubitOperator], max_qubits: usize, fused: bool) {
    for operator in operators {
        let qubits = operator.qubits();
        assert!(
            !qubits.is_empty() && qubits.len() <= max_qubits,
            "{qubits:?}"
        );
        if fused {
            assert!(qubits.windows(2).all(|w| w[0] < w[1]), "{qubits:?}");
        }
        assert_eq!(operator.matrix().len(), 1 << (2 * qubits.len()));
        assert!(unitarity_defect(operator) < 1e-12, "{operator:?}");
    }
}

#[test]
fn every_fusion_applies_the_product_of_the_regions_gates() {
    let mut rng = StdRng::seed_from_u64(7);
    for case in 0..300 {
        let qubits = rng.random_range(1..=5);
        let prefix: Vec<_> = (0..12)
            .map(|_| random_operation(&mut rng, qubits))
            .collect();
        let length = rng.random_range(0..40);
        let operations: Vec<_> = (0..length)
            .map(|_| random_operation(&mut rng, qubits))
            .collect();
        let expected = amplitudes(qubits, &prefix, &GateFusion::Unfused.operators(&operations));
        let applied = operations
            .iter()
            .filter(|op| !matches!(op, UnitaryOperation::I { .. }))
            .count();
        for grouping in GROUPINGS {
            let operators = grouping.operators(&operations);
            check_shape(
                &operators,
                grouping.max_qubits(),
                !matches!(grouping, Grouping::Public(GateFusion::Unfused)),
            );
            assert!(operators.len() <= applied, "case {case}, {grouping:?}");
            let fused = amplitudes(qubits, &prefix, &operators);
            assert!(
                distance(&fused, &expected) < 1e-12,
                "case {case}, {grouping:?}: {operations:?}"
            );
        }
        assert_eq!(GateFusion::Unfused.operators(&operations).len(), applied);
    }
}

#[test]
fn unfused_is_one_operator_per_gate_in_operand_order_without_i() {
    let operations = [
        cx(1, 0),
        UnitaryOperation::I { target: 2 },
        UnitaryOperation::T { target: 2 },
    ];
    let operators = GateFusion::Unfused.operators(&operations);
    let qubits: Vec<&[QubitID]> = operators.iter().map(QubitOperator::qubits).collect();
    assert_eq!(qubits, [&[1, 0][..], &[2]]);
}

#[test]
fn pairs_is_the_default() {
    assert_eq!(GateFusion::default(), GateFusion::Pairs);
}

/// The qubits of each operator `fusion` makes of `operations`.
fn qubits_of(grouping: Grouping, operations: &[UnitaryOperation]) -> Vec<Vec<QubitID>> {
    grouping
        .operators(operations)
        .iter()
        .map(|operator| operator.qubits().to_vec())
        .collect()
}

const PAIRS: Grouping = Grouping::Public(GateFusion::Pairs);

#[test]
fn pairs_absorb_single_qubit_gates_before_and_after_and_repeat_pairs() {
    let t = |target| UnitaryOperation::T { target };
    let cz = |control, target| UnitaryOperation::Cz { control, target };
    // Before (H 0, H 1) and after (T 0) the pair, and the pair again.
    assert_eq!(
        qubits_of(PAIRS, &[h(0), h(1), cx(0, 1), t(0), cz(0, 1), t(1)]),
        [vec![0, 1]]
    );
    // A pair closed by another on one of its qubits; the gate after on the
    // freed qubit is a run of its own.
    assert_eq!(
        qubits_of(PAIRS, &[cx(0, 1), cx(1, 2), h(0), t(0)]),
        [vec![0, 1], vec![1, 2], vec![0]]
    );
    // Only single-qubit gates: one 2×2 per qubit, in the order the runs
    // began.
    assert_eq!(
        qubits_of(PAIRS, &[h(2), t(0), h(0), t(2), h(1)]),
        [vec![2], vec![0], vec![1]]
    );
    // A gate on a qubit no other gate touches.
    assert_eq!(
        qubits_of(PAIRS, &[cx(0, 1), h(3), cz(1, 0)]),
        [vec![0, 1], vec![3]]
    );
    // An empty region, or one of `I` only: nothing to apply.
    for grouping in GROUPINGS {
        assert!(grouping.operators(&[]).is_empty());
        assert!(
            grouping
                .operators(&[UnitaryOperation::I { target: 0 }])
                .is_empty()
        );
    }
    // Up to three qubits, a pair and the next gate on a third qubit join.
    assert_eq!(
        qubits_of(Grouping::UpToThree, &[cx(0, 1), cx(1, 2), h(0), cx(2, 3)]),
        [vec![0, 1, 2], vec![2, 3]]
    );
}

#[test]
fn a_reversed_pair_is_the_same_block_with_its_matrix_reordered() {
    // CX(1, 0) alone: on qubits [0, 1] (ascending), qubit 0 the most
    // significant bit, so |a b⟩ ↦ |a⊕b, b⟩: the permutation 00, 11, 10, 01.
    let operators = GateFusion::Pairs.operators(&[cx(1, 0)]);
    assert_eq!(operators.len(), 1);
    assert_eq!(operators[0].qubits(), [0, 1]);
    let mut expected = vec![z(0.0); 16];
    for (output, input) in [(0, 0), (3, 1), (2, 2), (1, 3)] {
        expected[output * 4 + input] = z(1.0);
    }
    assert_eq!(operators[0].matrix(), expected);
    // CX(0, 1) then CX(1, 0) then CX(0, 1) is SWAP, one block.
    let swap = GateFusion::Pairs.operators(&[cx(0, 1), cx(1, 0), cx(0, 1)]);
    assert_eq!(swap.len(), 1);
    let mut expected = vec![z(0.0); 16];
    for (output, input) in [(0, 0), (2, 1), (1, 2), (3, 3)] {
        expected[output * 4 + input] = z(1.0);
    }
    assert_eq!(swap[0].matrix(), expected);
}

#[test]
fn a_distant_pair_is_one_block_on_its_two_qubits() {
    let operations = [
        h(0),
        cx(0, 3),
        UnitaryOperation::T { target: 3 },
        cx(3, 0),
        h(2),
    ];
    assert_eq!(qubits_of(PAIRS, &operations), [vec![0, 3], vec![2]]);
    let prefix = [h(1), cx(1, 2), h(3)];
    let expected = amplitudes(4, &prefix, &GateFusion::Unfused.operators(&operations));
    let fused = amplitudes(4, &prefix, &GateFusion::Pairs.operators(&operations));
    assert!(distance(&fused, &expected) < 1e-14);
}

fn terminal(fusion: GateFusion, qubits: usize) -> TensorNetTerminal<ExactReference> {
    TensorNetTerminal::new(
        std::rc::Rc::new(ExactReference::new()),
        mps_capacity(qubits, 8).expect("a chain"),
        5,
    )
    .expect("prepares")
    .with_gate_fusion(fusion)
}

#[test]
fn no_operator_crosses_a_measurement_or_a_reset() {
    let batch = [
        region([h(0), cx(0, 1)]),
        measure(0, 0),
        region([cx(0, 1), h(0)]),
        measure_reset(1, 1),
        region([]),
        region([h(1), h(0)]),
        measure(0, 2),
        measure(1, 3),
    ];
    let mut runs = Vec::new();
    for fusion in [GateFusion::Unfused, GateFusion::Pairs] {
        let mut terminal = terminal(fusion, 2);
        assert_eq!(terminal.gate_fusion(), fusion);
        let (outcomes, end) = run_batch(&mut terminal, &batch);
        end.expect("ran");
        let log_p = terminal.log_p();
        runs.push((outcomes, log_p, terminal.close()));
    }
    let [
        (unfused, unfused_p, unfused_report),
        (pairs, pairs_p, pairs_report),
    ] = runs.try_into().expect("two runs");
    assert_eq!(unfused, pairs);
    assert!((unfused_p - pairs_p).abs() < 1e-12);
    assert_eq!(
        unfused_report,
        TensorNetReport {
            operators: 6,
            ..pairs_report
        }
    );
    // One block per region, and none for the empty one: the measurement
    // between H CX and CX H keeps them apart.
    assert_eq!(pairs_report.operators, 4);
    assert_eq!(pairs_report.operations, 6);
    assert_eq!(pairs_report.regions, 4);
}

/// A random Adaptive program on `qubits` qubits: `rounds` regions of random
/// gates, each followed by a measurement (`MZ` or `MResetZ`) into the next
/// result, sometimes a `RESET`, and from the second round a region of
/// random gates run only when the previous result is One (`READ_RESULT`,
/// `ICMP ne`, `BRANCH`, as H₂ conditions its gates). Every result is
/// recorded.
pub(super) fn random_adaptive_program(
    seed: u64,
    qubits: u32,
    rounds: u64,
) -> PreparedAdaptiveProgram<u64> {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = qubits as usize;
    let mut program = Bytecode::new(qubits);
    let mut starts = vec![0];
    let gates = |mut program: Bytecode, rng: &mut StdRng, count: usize| {
        for _ in 0..count {
            let operation = random_operation(rng, n);
            let (op_id, q1, q2, angle) = bytecode_of(operation);
            program = program.gate(op_id, q1, q2, angle);
        }
        program
    };
    for round in 0..rounds {
        let count = rng.random_range(0..10);
        program = gates(program, &mut rng, count);
        if round > 0 {
            let apply = starts.len() as u64;
            program.instructions.extend([
                Instruction {
                    opcode: OP_READ_RESULT | super::IMMEDIATE_SRC0,
                    dst: 0,
                    src0: round - 1,
                    ..Instruction::default()
                },
                Instruction {
                    opcode: OP_ICMP_NE | super::IMMEDIATE_SRC1,
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
            let count = rng.random_range(1..6);
            program = gates(program, &mut rng, count);
            program.instructions.push(Instruction {
                opcode: OP_JUMP,
                dst: apply + 1,
                ..Instruction::default()
            });
            starts.push(program.instructions.len());
            let count = rng.random_range(0..6);
            program = gates(program, &mut rng, count);
        }
        let qubit = rng.random_range(0..u64::from(qubits));
        program = if rng.random_bool(0.5) {
            program.mz(qubit, round)
        } else {
            program.mresetz(qubit, round)
        };
        if rng.random_bool(0.3) {
            program = program.reset(rng.random_range(0..u64::from(qubits)));
        }
    }
    for result in 0..rounds {
        program = program.record(result);
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
    program.prepare(blocks).expect("a random program prepares")
}

/// The bytecode operation of `operation`: its id, operands and angle.
fn bytecode_of(operation: UnitaryOperation) -> (u64, u64, u64, f64) {
    use UnitaryOperation as U;
    let q = |qubit: QubitID| qubit as u64;
    match operation {
        U::I { target } => (0, q(target), 0, 0.0),
        U::X { target } => (2, q(target), 0, 0.0),
        U::Y { target } => (3, q(target), 0, 0.0),
        U::Z { target } => (4, q(target), 0, 0.0),
        U::H { target } => (5, q(target), 0, 0.0),
        U::S { target } => (6, q(target), 0, 0.0),
        U::SAdj { target } => (7, q(target), 0, 0.0),
        U::T { target } => (8, q(target), 0, 0.0),
        U::TAdj { target } => (9, q(target), 0, 0.0),
        U::Sx { target } => (10, q(target), 0, 0.0),
        U::SxAdj { target } => (11, q(target), 0, 0.0),
        U::Rx { angle, target } => (12, q(target), 0, angle),
        U::Ry { angle, target } => (13, q(target), 0, angle),
        U::Rz { angle, target } => (14, q(target), 0, angle),
        U::Cx { control, target } => (15, q(control), q(target), 0.0),
        U::Cz { control, target } => (16, q(control), q(target), 0.0),
        U::Rxx { angle, q1, q2 } => (17, q(q1), q(q2), angle),
        U::Ryy { angle, q1, q2 } => (18, q(q1), q(q2), angle),
        U::Rzz { angle, q1, q2 } => (19, q(q1), q(q2), angle),
        U::Swap { q1, q2 } => (24, q(q1), q(q2), 0.0),
        U::Cy { control, target } => (29, q(control), q(target), 0.0),
    }
}

/// Shot `j` of `program` through the walk over `strategy`, opened with
/// `seed`: its records, ln p and report.
pub(super) fn walk<E: MpsEngine>(
    strategy: &PerShot<TensorNetShots<E>>,
    program: &PreparedAdaptiveProgram<u64>,
    seed: u64,
    j: u32,
) -> (Vec<OutputRecord>, f64, TensorNetReport) {
    let session = open(strategy, seed, &ShotSet::one(j)).expect("opens");
    let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, j));
    let mut records = Vec::new();
    adapter.run(program, &mut records).expect("runs");
    let report = adapter.close().inner;
    let log_p = report.log_p.get(j).expect("known");
    (records, log_p, report.shots)
}

#[test]
fn fused_and_unfused_shots_draw_the_same_values_with_the_same_probabilities() {
    let stack = MpsStack::new(ExactReference::new());
    let mut fewer = 0;
    for seed in 0..40 {
        let qubits = 2 + (seed % 4) as u32;
        let program = random_adaptive_program(seed, qubits, 6);
        let unfused = stack.per_shot(qubits as usize).expect("a chain");
        let pairs = MpsStack::new(ExactReference::new())
            .with_gate_fusion(GateFusion::Pairs)
            .per_shot(qubits as usize)
            .expect("a chain");
        assert_eq!(pairs.0.gate_fusion(), GateFusion::Pairs);
        for j in 1..=5 {
            let (rows, log_p, report) = walk(&unfused, &program, seed, j);
            let (fused_rows, fused_log_p, fused_report) = walk(&pairs, &program, seed, j);
            assert_eq!(rows, fused_rows, "program {seed}, shot {j}");
            assert!(
                (log_p - fused_log_p).abs() < 1e-9,
                "program {seed}, shot {j}: ln p {log_p} unfused, {fused_log_p} in pairs"
            );
            assert_eq!(report.operations, fused_report.operations);
            assert_eq!(report.operators, report.operations);
            assert!(fused_report.operators <= report.operators);
            fewer += usize::from(fused_report.operators < report.operators);
            assert_eq!(
                TensorNetReport {
                    operators: 0,
                    ..report
                },
                TensorNetReport {
                    operators: 0,
                    ..fused_report
                }
            );
        }
    }
    assert!(fewer > 100, "pairs grouped gates in {fewer} of 200 shots");
}

#[test]
fn phase_estimation_with_feedback_in_pairs_follows_the_distribution() {
    super::cases::per_shot_phase_estimation(
        &MpsStack::new(ExactReference::new()).with_gate_fusion(GateFusion::Pairs),
        400,
        1e-9,
    );
}

/// An engine that applies nothing and draws every value with probability
/// ½: the walk's path through a program, with the operators the terminal
/// gives the engine counted by its report, without the state's cost.
pub(super) struct TestDoubleCoinEngine;

impl MpsEngine for TestDoubleCoinEngine {
    /// The qubits.
    type State = usize;
    type Error = std::convert::Infallible;

    fn prepare(&self, capacity: &MpsShape) -> Result<usize, Self::Error> {
        Ok(capacity.site_count())
    }

    fn apply(&self, _: &mut usize, _: &[usize], _: &[Complex64]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn probability_of_one(&self, _: &usize, _: usize) -> Result<f64, Self::Error> {
        Ok(0.5)
    }

    fn project(&self, _: &mut usize, _: usize, _: bool, _: f64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn fork(&self, state: &usize) -> Result<usize, Self::Error> {
        Ok(*state)
    }

    fn sample(
        &self,
        _: &usize,
        sites: &[usize],
        shots: usize,
        _: SampleSeed,
    ) -> Result<Vec<bool>, Self::Error> {
        Ok(vec![false; sites.len() * shots])
    }

    fn expectation(&self, _: &usize, _: &tensornet::PauliSum) -> Result<Complex64, Self::Error> {
        Ok(z(0.0))
    }

    fn shape(&self, state: &usize) -> Result<MpsShape, Self::Error> {
        Ok(mps_capacity(*state, 1).expect("a chain of bond 1"))
    }

    // The coin engine holds no amplitudes, so it never truncates.
    fn retained_weight(&self, _: &usize) -> Result<f64, Self::Error> {
        Ok(1.0)
    }
}

/// The A100's H₂ driver (`h2::h2_qpe`), with its environment
/// (`QDK_H2_FUSION`, `QDK_H2_STOP_AFTER`, `QDK_H2_READOUT`, `QDK_H2_SVD`,
/// `QDK_H2_GAUGE`, …), on
/// [`TestDoubleCoinEngine`]: its progress, its stop, its operator count and,
/// with `QDK_H2_READOUT=distribution`, its readout distribution (uniform:
/// every P(One) is ½) and trace, checked on CPU before a VM runs it. The
/// rows are coin flips, so it asserts no peaks.
#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE"]
fn h2_driver_on_a_coin_engine() {
    super::h2::h2_qpe(|_, _| MpsStack::new(TestDoubleCoinEngine), false);
}

/// `count.py`'s figures for H₂'s bytecode (`scratch/m1-run/perf/fusion/`):
/// its gates, and its operators with two-site fusion, over the regions it
/// cuts statically (at each measurement, reset, branch and switch), every
/// conditional block counted.
const COUNT_PY_GATES: u64 = 392_338;
const COUNT_PY_PAIRS: u64 = 87_598;
/// `count.py`'s greedy blocks of up to three qubits, an upper bound.
const COUNT_PY_GREEDY_THREE: u64 = 26_369;

/// The regions of `program` cut as `count.py` cuts them: at each
/// measurement, reset, branch and switch, over the instructions in order,
/// with each gate's qubits and angle from its operation table entry.
pub(super) fn static_regions(program: &AdaptiveProgram<u64>) -> Vec<Vec<UnitaryOperation>> {
    const GATE: u64 = 0x10;
    const CUTS: [u64; 4] = [0x11, 0x12, 0x05, 0x06];
    let mut regions = vec![Vec::new()];
    for instruction in &program.instructions {
        let opcode = instruction.opcode & 0xFF;
        if opcode == GATE {
            let op = &program.quantum_ops[usize::try_from(instruction.aux0).expect("fits")];
            let (q1, q2) = (
                usize::try_from(op.q1).expect("fits"),
                usize::try_from(op.q2).expect("fits"),
            );
            let angle = f64::from_bits(op.angle);
            let operation = match op.op_id {
                2 => UnitaryOperation::X { target: q1 },
                4 => UnitaryOperation::Z { target: q1 },
                5 => UnitaryOperation::H { target: q1 },
                6 => UnitaryOperation::S { target: q1 },
                7 => UnitaryOperation::SAdj { target: q1 },
                8 => UnitaryOperation::T { target: q1 },
                9 => UnitaryOperation::TAdj { target: q1 },
                14 => UnitaryOperation::Rz { angle, target: q1 },
                15 => UnitaryOperation::Cx {
                    control: q1,
                    target: q2,
                },
                16 => UnitaryOperation::Cz {
                    control: q1,
                    target: q2,
                },
                other => panic!("H₂ has no gate {other}"),
            };
            regions.last_mut().expect("one").push(operation);
        } else if CUTS.contains(&opcode) {
            regions.push(Vec::new());
        }
    }
    regions
}

/// The operators of `regions` under `grouping`, and the largest unitarity
/// defect of a fused operator.
fn count(grouping: Grouping, regions: &[Vec<UnitaryOperation>]) -> (u64, f64) {
    regions.iter().fold((0, 0.0), |(count, defect), region| {
        let operators = grouping.operators(region);
        let worst = operators
            .iter()
            .map(unitarity_defect)
            .fold(defect, f64::max);
        (count + operators.len() as u64, worst)
    })
}

/// `a / b`, for a report; counts here are far below 2^52.
#[allow(clippy::cast_precision_loss)]
fn ratio(a: u64, b: u64) -> f64 {
    a as f64 / b as f64
}

/// H₂'s engine operators per shot under each fusion, on CPU: statically,
/// over the regions `count.py` cuts, checked against its figures; and per
/// shot, through the walk over the per-shot terminal on a coin-flipping
/// engine (each measurement One with probability ½, so the branches taken
/// are a random path, not the physical one), from the terminal's report.
/// `QDK_H2_BYTECODE` names the JSON; `QDK_H2_SHOTS` (3) coin shots.
#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE"]
fn h2_operators_per_shot() {
    let path = env::var("QDK_H2_BYTECODE")
        .expect("QDK_H2_BYTECODE should name the JSON of H₂'s AdaptiveProgram.as_dict()");
    let shots: u32 = env::var("QDK_H2_SHOTS").map_or(3, |s| s.parse().expect("a number"));
    let json = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let program = super::adaptive_program_json::adaptive_program_from_json(&json);

    let regions = static_regions(&program);
    let gates: u64 = regions.iter().map(|r| r.len() as u64).sum();
    println!(
        "h2 static: {} regions, {} non-empty, {gates} gates (count.py: {COUNT_PY_GATES})",
        regions.len(),
        regions.iter().filter(|r| !r.is_empty()).count()
    );
    let mut statics = Vec::new();
    for grouping in GROUPINGS {
        let (operators, defect) = count(grouping, &regions);
        println!(
            "h2 static {grouping:?}: {operators} operators ({:.2}x fewer than gates), \
             largest unitarity defect {defect:.1e}",
            ratio(gates, operators)
        );
        assert!(defect < 1e-12, "{grouping:?}: {defect}");
        statics.push(operators);
    }
    if gates == COUNT_PY_GATES {
        assert_eq!(statics[0], COUNT_PY_GATES);
        assert_eq!(statics[1], COUNT_PY_PAIRS, "count.py's two-site model");
        assert!(statics[2] <= COUNT_PY_GREEDY_THREE, "{}", statics[2]);
    } else {
        println!("h2 static: not count.py's bytecode, its figures not checked");
    }

    let program = PreparedAdaptiveProgram::new(program).expect("H₂ prepares");
    let qubits = usize::try_from(program.program().num_qubits).expect("fits");
    for fusion in [GateFusion::Unfused, GateFusion::Pairs] {
        let strategy = MpsStack::new(TestDoubleCoinEngine)
            .with_gate_fusion(fusion)
            .per_shot(qubits)
            .expect("a chain");
        for j in 1..=shots {
            let (_, _, report) = walk(&strategy, &program, 0, j);
            println!(
                "h2 coin shot {j} {fusion:?}: {} regions, {} measurements, {} resets, \
                 {} gates, {} operators ({:.2}x fewer), {:.0} operators per measurement",
                report.regions,
                report.measurements,
                report.resets,
                report.operations,
                report.operators,
                ratio(report.operations, report.operators),
                ratio(report.operators, report.measurements)
            );
        }
    }
}
