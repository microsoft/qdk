// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gate fusion's own rules: the operators `Pairs` forms, `Unfused`'s one
//! operator per gate, and a shot's records the same fused or not. The
//! rewrite contract, which fusion keeps, is checked in `rewrites.rs`.

use crate::qir::{OperatorMatrix, QirGate, QirMachine, QirResumer, QirShotId, expect_ready};
use tensornet::reference::ExactReference;

use super::super::super::programs::{gate, gate1, gate2, mz, program, ret};
use super::random_gate;
use crate::bytecode::encoding::{OPID_CX, OPID_H, OPID_RX, OPID_RY, OPID_RZZ};
use crate::mps_simulator::resumer::lowering::Operator;
use crate::mps_simulator::resumer::rewrites::Rewrite;
use crate::mps_simulator::resumer::shot_stream::SplitMix64;
use crate::mps_simulator::{GateFusion, MpsResumer};

/// Consecutive gates on one pair of sites, and the one-site gates on them,
/// make one operator.
#[test]
fn gates_on_one_pair_make_one_operator() {
    let on = |gate: QirGate, sites: [usize; 2]| Operator {
        sites,
        matrix: gate.matrix_at(None),
    };
    let run = vec![
        on(QirGate::H, [2, 0]),
        on(QirGate::Cx, [2, 0]),
        on(QirGate::S, [0, 0]),
        on(QirGate::Cz, [0, 2]),
    ];
    let fused = GateFusion::Pairs.rewrite(run);
    assert_eq!(fused.len(), 1);
    assert!(matches!(fused[0].matrix, OperatorMatrix::Two(_)));
}

/// Unfused, every gate is its own operator.
#[test]
fn unfused_keeps_one_operator_per_gate() {
    let mut stream = SplitMix64::new(3);
    let run: Vec<Operator> = (0..10).map(|_| random_gate(&mut stream)).collect();
    assert_eq!(GateFusion::Unfused.rewrite(run.clone()), run);
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
