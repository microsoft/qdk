// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gate fusion: the operators apply the product of the gates, runs end at
//! measurements and resets, and a shot records the same fused or not.

use crate::qir::{OperatorMatrix, QirGate, QirMachine, QirResumer, QirShotId, expect_ready};
use num_complex::Complex64;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::{MpsEngine, MpsShape};

use super::programs::{gate, gate1, gate2, mz, program, ret};
use crate::bytecode::encoding::{OPID_CX, OPID_H, OPID_RX, OPID_RY, OPID_RZZ};
use crate::mps_simulator::resumer::lowering::{Operator, Step};
use crate::mps_simulator::resumer::splitmix64::SplitMix64;
use crate::mps_simulator::{GateFusion, MpsResumer};

const SITES: usize = 4;

/// A random gate of `stream` on `SITES` sites.
fn random_gate(stream: &mut SplitMix64) -> Operator {
    let gates = [
        QirGate::H,
        QirGate::Rx,
        QirGate::Ry,
        QirGate::T,
        QirGate::Cx,
        QirGate::Cy,
        QirGate::Rzz,
        QirGate::Swap,
    ];
    let pick = super::below;
    let gate = gates[pick(stream, gates.len())];
    let angle = matches!(gate, QirGate::Rx | QirGate::Ry | QirGate::Rzz)
        .then(|| stream.next_uniform() * 6.0);
    let matrix = gate.matrix_at(angle);
    let first = pick(stream, SITES);
    let second = (first + 1 + pick(stream, SITES - 1)) % SITES;
    Operator {
        sites: [first, second],
        matrix,
    }
}

/// The state the operators leave, from |0…0⟩ with a layer of `h`.
fn applied(operators: impl IntoIterator<Item = Operator>) -> ReferenceState {
    let engine = ExactReference::new();
    let capacity = MpsShape::qubit_chain(SITES, usize::MAX).expect("a chain");
    let mut state = engine
        .prepare(&capacity)
        .expect("the reference holds four sites");
    let h = QirGate::H.matrix_at(None);
    for site in 0..SITES {
        engine
            .apply(&mut state, &[site], h.row_major())
            .expect("h is unitary");
    }
    for operator in operators {
        engine
            .apply(&mut state, operator.sites(), operator.matrix.row_major())
            .expect("an operator on distinct sites is unitary");
    }
    state
}

/// The operators of `steps`, which hold only gates.
fn operators(steps: Vec<Step>) -> Vec<Operator> {
    steps
        .into_iter()
        .map(|step| match step {
            Step::Apply(operator) => operator,
            other => panic!("only gates: {other:?}"),
        })
        .collect()
}

/// Pairs fusion applies the product of the gates, on random circuits.
#[test]
fn pairs_apply_the_product_of_the_gates() {
    let mut stream = SplitMix64::new(11);
    for _ in 0..20 {
        let gates: Vec<Operator> = (0..30).map(|_| random_gate(&mut stream)).collect();
        let fused =
            operators(GateFusion::Pairs.fuse(gates.iter().copied().map(Step::Apply).collect()));
        assert!(fused.len() < gates.len(), "fusion multiplies some gates");
        let (expected, actual) = (applied(gates), applied(fused));
        for index in 0..1 << SITES {
            let difference: Complex64 = expected.amplitude(index) - actual.amplitude(index);
            assert!(
                difference.norm() < 1e-12,
                "amplitude {index} differs by {difference}"
            );
        }
    }
}

/// Consecutive gates on one pair of sites, and the one-site gates on them,
/// make one operator.
#[test]
fn gates_on_one_pair_make_one_operator() {
    let on = |gate: QirGate, sites: [usize; 2]| {
        Step::Apply(Operator {
            sites,
            matrix: gate.matrix_at(None),
        })
    };
    let steps = vec![
        on(QirGate::H, [2, 0]),
        on(QirGate::Cx, [2, 0]),
        on(QirGate::S, [0, 0]),
        on(QirGate::Cz, [0, 2]),
    ];
    let fused = operators(GateFusion::Pairs.fuse(steps));
    assert_eq!(fused.len(), 1);
    assert!(matches!(fused[0].matrix, OperatorMatrix::Two(_)));
}

/// A measurement ends a run of gates: no operator spans it.
#[test]
fn a_measurement_ends_a_run_of_gates() {
    let h = Step::Apply(Operator {
        sites: [0, 0],
        matrix: QirGate::H.matrix_at(None),
    });
    let measure = Step::Measure {
        site: 0,
        result: 0,
        reset: false,
    };
    let steps = vec![h.clone(), measure.clone(), h.clone()];
    assert_eq!(GateFusion::Pairs.fuse(steps), vec![h.clone(), measure, h]);
}

/// Unfused, every gate is its own operator.
#[test]
fn unfused_keeps_one_operator_per_gate() {
    let mut stream = SplitMix64::new(3);
    let steps: Vec<Step> = (0..10)
        .map(|_| Step::Apply(random_gate(&mut stream)))
        .collect();
    assert_eq!(GateFusion::Unfused.fuse(steps.clone()), steps);
}

/// A shot records the same with gates fused or not.
#[test]
fn fusion_never_changes_a_shots_records() {
    let mut block = vec![
        gate1(OPID_H, 0),
        gate(OPID_RX, 1, 0, 0.7),
        gate2(OPID_CX, 0, 2),
        gate(OPID_RZZ, 2, 1, 1.1),
        gate(OPID_RY, 0, 0, 2.3),
        gate2(OPID_CX, 1, 0),
    ];
    block.extend((0..3).map(mz));
    block.push(ret());
    let program = program(3, &[&block]);
    let shots = QirShotId::first(32);
    let records = |fusion| {
        let mut resumer = MpsResumer::new(ExactReference::new(), 5).with_gate_fusion(fusion);
        expect_ready(resumer.run(&QirMachine::new(&program), &shots)).expect("the program runs")
    };
    assert_eq!(records(GateFusion::Pairs), records(GateFusion::Unfused));
}
