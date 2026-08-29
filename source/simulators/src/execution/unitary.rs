// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Resolved unitary operations and the legacy simulator application bridge.

use crate::{QubitID, Simulator};

const OPID_I: u64 = 0;
const OPID_X: u64 = 2;
const OPID_Y: u64 = 3;
const OPID_Z: u64 = 4;
const OPID_H: u64 = 5;
const OPID_S: u64 = 6;
const OPID_S_ADJ: u64 = 7;
const OPID_T: u64 = 8;
const OPID_T_ADJ: u64 = 9;
const OPID_SX: u64 = 10;
const OPID_SX_ADJ: u64 = 11;
const OPID_RX: u64 = 12;
const OPID_RY: u64 = 13;
const OPID_RZ: u64 = 14;
const OPID_CX: u64 = 15;
const OPID_CZ: u64 = 16;
const OPID_RXX: u64 = 17;
const OPID_RYY: u64 = 18;
const OPID_RZZ: u64 = 19;
pub(crate) const OPID_MZ: u64 = 21;
pub(crate) const OPID_MRESETZ: u64 = 22;
const OPID_SWAP: u64 = 24;
const OPID_CY: u64 = 29;

/// A unitary operation whose parameters and qubit operands have been resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnitaryOperation {
    I {
        target: QubitID,
    },
    X {
        target: QubitID,
    },
    Y {
        target: QubitID,
    },
    Z {
        target: QubitID,
    },
    H {
        target: QubitID,
    },
    S {
        target: QubitID,
    },
    SAdj {
        target: QubitID,
    },
    Sx {
        target: QubitID,
    },
    SxAdj {
        target: QubitID,
    },
    T {
        target: QubitID,
    },
    TAdj {
        target: QubitID,
    },
    Rx {
        angle: f64,
        target: QubitID,
    },
    Ry {
        angle: f64,
        target: QubitID,
    },
    Rz {
        angle: f64,
        target: QubitID,
    },
    Cx {
        control: QubitID,
        target: QubitID,
    },
    Cy {
        control: QubitID,
        target: QubitID,
    },
    Cz {
        control: QubitID,
        target: QubitID,
    },
    Rxx {
        angle: f64,
        q1: QubitID,
        q2: QubitID,
    },
    Ryy {
        angle: f64,
        q1: QubitID,
        q2: QubitID,
    },
    Rzz {
        angle: f64,
        q1: QubitID,
        q2: QubitID,
    },
    Swap {
        q1: QubitID,
        q2: QubitID,
    },
}

pub(crate) fn resolve_unitary_operation(
    operation_id: u64,
    angle: f64,
    q1: QubitID,
    q2: QubitID,
) -> Option<UnitaryOperation> {
    Some(match operation_id {
        OPID_I => UnitaryOperation::I { target: q1 },
        OPID_X => UnitaryOperation::X { target: q1 },
        OPID_Y => UnitaryOperation::Y { target: q1 },
        OPID_Z => UnitaryOperation::Z { target: q1 },
        OPID_H => UnitaryOperation::H { target: q1 },
        OPID_S => UnitaryOperation::S { target: q1 },
        OPID_S_ADJ => UnitaryOperation::SAdj { target: q1 },
        OPID_T => UnitaryOperation::T { target: q1 },
        OPID_T_ADJ => UnitaryOperation::TAdj { target: q1 },
        OPID_SX => UnitaryOperation::Sx { target: q1 },
        OPID_SX_ADJ => UnitaryOperation::SxAdj { target: q1 },
        OPID_RX => UnitaryOperation::Rx { angle, target: q1 },
        OPID_RY => UnitaryOperation::Ry { angle, target: q1 },
        OPID_RZ => UnitaryOperation::Rz { angle, target: q1 },
        OPID_CX => UnitaryOperation::Cx {
            control: q1,
            target: q2,
        },
        OPID_CZ => UnitaryOperation::Cz {
            control: q1,
            target: q2,
        },
        OPID_RXX => UnitaryOperation::Rxx { angle, q1, q2 },
        OPID_RYY => UnitaryOperation::Ryy { angle, q1, q2 },
        OPID_RZZ => UnitaryOperation::Rzz { angle, q1, q2 },
        OPID_SWAP => UnitaryOperation::Swap { q1, q2 },
        OPID_CY => UnitaryOperation::Cy {
            control: q1,
            target: q2,
        },
        _ => return None,
    })
}

/// Applies one resolved unitary operation directly through the legacy simulator interface.
pub(crate) fn apply_unitary_immediately<S: Simulator>(
    simulator: &mut S,
    operation: UnitaryOperation,
) {
    match operation {
        UnitaryOperation::I { .. } => {}
        UnitaryOperation::X { target } => simulator.x(target),
        UnitaryOperation::Y { target } => simulator.y(target),
        UnitaryOperation::Z { target } => simulator.z(target),
        UnitaryOperation::H { target } => simulator.h(target),
        UnitaryOperation::S { target } => simulator.s(target),
        UnitaryOperation::SAdj { target } => simulator.s_adj(target),
        UnitaryOperation::Sx { target } => simulator.sx(target),
        UnitaryOperation::SxAdj { target } => simulator.sx_adj(target),
        UnitaryOperation::T { target } => simulator.t(target),
        UnitaryOperation::TAdj { target } => simulator.t_adj(target),
        UnitaryOperation::Rx { angle, target } => simulator.rx(angle, target),
        UnitaryOperation::Ry { angle, target } => simulator.ry(angle, target),
        UnitaryOperation::Rz { angle, target } => simulator.rz(angle, target),
        UnitaryOperation::Cx { control, target } => simulator.cx(control, target),
        UnitaryOperation::Cy { control, target } => simulator.cy(control, target),
        UnitaryOperation::Cz { control, target } => simulator.cz(control, target),
        UnitaryOperation::Rxx { angle, q1, q2 } => simulator.rxx(angle, q1, q2),
        UnitaryOperation::Ryy { angle, q1, q2 } => simulator.ryy(angle, q1, q2),
        UnitaryOperation::Rzz { angle, q1, q2 } => simulator.rzz(angle, q1, q2),
        UnitaryOperation::Swap { q1, q2 } => simulator.swap(q1, q2),
    }
}