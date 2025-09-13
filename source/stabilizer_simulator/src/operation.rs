// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::QubitID;

/// An extension of the Clifford gates, also including a `Move` operation.
/// A gate C is Clifford if it conjugates all elements of the Pauli group into
/// elements of the pauli group. That is, ∀ p ∈ PauliGroup, C†pC ∈ PauliGroup.
#[derive(Debug)]
pub enum Operation {
    I { target: QubitID },
    X { target: QubitID },
    Y { target: QubitID },
    Z { target: QubitID },
    H { target: QubitID },
    S { target: QubitID },
    CZ { control: QubitID, target: QubitID },
    Move { target: QubitID },
    MResetZ { target: QubitID },
}

pub fn id(target: QubitID) -> Operation {
    Operation::I { target }
}

pub fn x(target: QubitID) -> Operation {
    Operation::X { target }
}

pub fn y(target: QubitID) -> Operation {
    Operation::Y { target }
}

pub fn z(target: QubitID) -> Operation {
    Operation::Z { target }
}

pub fn h(target: QubitID) -> Operation {
    Operation::H { target }
}

pub fn s(target: QubitID) -> Operation {
    Operation::S { target }
}

pub fn cz(control: QubitID, target: QubitID) -> Operation {
    Operation::CZ { control, target }
}

pub fn mz(target: QubitID) -> Operation {
    Operation::MResetZ { target }
}

pub fn mov(target: QubitID) -> Operation {
    Operation::Move { target }
}
