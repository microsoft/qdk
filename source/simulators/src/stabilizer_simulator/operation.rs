// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::QubitID;

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
    SAdj { target: QubitID },
    SX { target: QubitID },
    CZ { control: QubitID, target: QubitID },
    Move { target: QubitID },
    MResetZ { target: QubitID, result_id: QubitID },
}

#[must_use]
pub fn id(target: QubitID) -> Operation {
    Operation::I { target }
}

#[must_use]
pub fn x(target: QubitID) -> Operation {
    Operation::X { target }
}

#[must_use]
pub fn y(target: QubitID) -> Operation {
    Operation::Y { target }
}

#[must_use]
pub fn z(target: QubitID) -> Operation {
    Operation::Z { target }
}

#[must_use]
pub fn h(target: QubitID) -> Operation {
    Operation::H { target }
}

#[must_use]
pub fn s(target: QubitID) -> Operation {
    Operation::S { target }
}

#[must_use]
pub fn cz(control: QubitID, target: QubitID) -> Operation {
    Operation::CZ { control, target }
}

#[must_use]
pub fn mz(target: QubitID) -> Operation {
    Operation::MResetZ {
        target,
        result_id: target,
    }
}

#[must_use]
pub fn mov(target: QubitID) -> Operation {
    Operation::Move { target }
}
