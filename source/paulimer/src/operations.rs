// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_derive::{FromPrimitive, ToPrimitive};

#[derive(FromPrimitive, ToPrimitive, Clone, Copy, Debug)]
pub enum UnitaryOp {
    I,
    X,
    Y,
    Z,
    SqrtX,
    SqrtXInv,
    SqrtY,
    SqrtYInv,
    SqrtZ,
    SqrtZInv,
    Hadamard,
    Swap,
    ControlledX,
    ControlledZ,
    PrepareBell,
}

#[macro_export]
macro_rules! assert_1q_gate {
    ($x: expr) => {
        debug_assert_eq!($x.len(), 1);
    };
}

#[macro_export]
macro_rules! assert_2q_gate {
    ($x: expr) => {
        debug_assert_eq!($x.len(), 2);
        debug_assert!($x[0] != $x[1]);
    };
}

pub type Operations = Vec<(UnitaryOp, Vec<usize>)>;

#[must_use]
pub fn qubit_operations(qubit_count: usize, qubit_op: UnitaryOp) -> Operations {
    let mut res = Vec::new();
    for qubit in 0..qubit_count {
        res.push((qubit_op, vec![qubit]));
    }
    res
}

#[must_use]
pub fn asymmetric_two_qubit_operations(qubit_count: usize, qubit_op: UnitaryOp) -> Operations {
    let mut res = Vec::new();
    for qubit1 in 0..qubit_count {
        for qubit2 in 0..qubit_count {
            if qubit1 != qubit2 {
                res.push((qubit_op, vec![qubit1, qubit2]));
            }
        }
    }
    res
}

#[must_use]
pub fn symmetric_two_qubit_operations(qubit_count: usize, qubit_op: UnitaryOp) -> Operations {
    let mut res = Vec::new();
    for qubit1 in 0..qubit_count {
        for qubit2 in 0..qubit1 {
            let gen = (qubit_op, vec![qubit1, qubit2]);
            res.push(gen);
        }
    }
    res
}

#[must_use]
pub fn diagonal_operations(qubit_count: usize) -> Operations {
    use UnitaryOp::{ControlledZ, SqrtZ};
    let mut res = Vec::new();
    res.append(&mut symmetric_two_qubit_operations(
        qubit_count,
        ControlledZ,
    ));
    res.append(&mut qubit_operations(qubit_count, SqrtZ));
    res
}

#[must_use]
pub fn css_operations(qubit_count: usize) -> Operations {
    use UnitaryOp::{ControlledX, Swap};
    let mut res = Vec::new();
    res.append(&mut symmetric_two_qubit_operations(qubit_count, Swap));
    res.append(&mut asymmetric_two_qubit_operations(
        qubit_count,
        ControlledX,
    ));
    res
}
