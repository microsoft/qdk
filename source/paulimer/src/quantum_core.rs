#[cfg(test)]
mod tests;

use num_derive::{FromPrimitive, ToPrimitive};
use std::ops::Neg;

/// All elements of a given type. Usually used for simple enums.
pub trait All<const NUM_ELEMENTS: usize>: Sized {
    fn all() -> [Self; NUM_ELEMENTS];
}

/// Pauli matrices not equal to identity, also axes of the Block sphere.
/// See also [`PauliMatrix`], [`Axis`].
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, FromPrimitive, ToPrimitive)]
pub enum Axis {
    /// Pauli X, also x axis of the Bloch sphere
    /// ```text
    /// |0 1|
    /// |1 0|
    /// ```
    X = 0b001,

    /// Pauli Y, also y axis of the Bloch sphere
    /// ```text
    /// |0 -i|
    /// |i  0|
    /// ```
    Y = 0b011,

    /// Pauli Z, also z axis of the Bloch sphere
    /// ```text
    /// |1  0|
    /// |0 -1|
    /// ```
    Z = 0b010,
}

/// Pauli matrices on one qubit.
/// See also [`Axis`].
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, FromPrimitive, ToPrimitive)]
pub enum PauliMatrix {
    /// One qubit identity matrix
    /// ```text
    /// |1 0|
    /// |0 1|
    /// ```
    I = 0b000,

    /// Pauli X
    /// ```text
    /// |0 1|
    /// |1 0|
    /// ```
    X = (Axis::X as isize),

    /// Pauli Y
    /// ```text
    /// |0 -i|
    /// |i  0|
    /// ```
    Y = (Axis::Y as isize),

    /// Pauli Z
    /// ```text
    /// |1  0|
    /// |0 -1|
    /// ```
    Z = (Axis::Z as isize),
}

const MINUS_IDENTITY: isize = 0b100;

/// Hermitian one qubit Pauli operators that are not identity.
/// See also [`Axis`].
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, FromPrimitive, ToPrimitive)]
pub enum DirectedAxis {
    /// Pauli X
    PlusX = (Axis::X as isize),
    /// Pauli Y
    PlusY = (Axis::Y as isize),
    /// Pauli Z
    PlusZ = (Axis::Z as isize),
    /// Pauli -X
    MinusX = (Axis::X as isize) ^ MINUS_IDENTITY,
    /// Pauli -Y
    MinusY = (Axis::Y as isize) ^ MINUS_IDENTITY,
    /// Pauli -Z
    MinusZ = (Axis::Z as isize) ^ MINUS_IDENTITY,
}

/// Hermitian one qubit Pauli operators.
/// See also [`Axis`], [`DirectedAxis`], [`PauliMatrix`]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, FromPrimitive, ToPrimitive)]
pub enum PauliObservable {
    /// Pauli I
    PlusI = (PauliMatrix::I as isize),
    /// Pauli X
    PlusX = (PauliMatrix::X as isize),
    /// Pauli Y
    PlusY = (PauliMatrix::Y as isize),
    /// Pauli Z
    PlusZ = (PauliMatrix::Z as isize),
    /// Pauli -I
    MinusI = (PauliMatrix::I as isize) ^ MINUS_IDENTITY,
    /// Pauli -X
    MinusX = (PauliMatrix::X as isize) ^ MINUS_IDENTITY,
    /// Pauli -Y
    MinusY = (PauliMatrix::Y as isize) ^ MINUS_IDENTITY,
    /// Pauli -Z
    MinusZ = (PauliMatrix::Z as isize) ^ MINUS_IDENTITY,
}

impl From<Axis> for PauliMatrix {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::X => PauliMatrix::X,
            Axis::Y => PauliMatrix::Y,
            Axis::Z => PauliMatrix::Z,
        }
    }
}

impl From<Axis> for DirectedAxis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::X => DirectedAxis::PlusX,
            Axis::Y => DirectedAxis::PlusY,
            Axis::Z => DirectedAxis::PlusZ,
        }
    }
}

impl From<Axis> for PauliObservable {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::X => PauliObservable::PlusX,
            Axis::Y => PauliObservable::PlusY,
            Axis::Z => PauliObservable::PlusZ,
        }
    }
}

impl From<PauliMatrix> for PauliObservable {
    fn from(pauli: PauliMatrix) -> Self {
        match pauli {
            PauliMatrix::I => PauliObservable::PlusI,
            PauliMatrix::X => PauliObservable::PlusX,
            PauliMatrix::Y => PauliObservable::PlusY,
            PauliMatrix::Z => PauliObservable::PlusZ,
        }
    }
}

impl From<DirectedAxis> for PauliObservable {
    fn from(directed_axis: DirectedAxis) -> Self {
        match directed_axis {
            DirectedAxis::PlusX => PauliObservable::PlusX,
            DirectedAxis::PlusY => PauliObservable::PlusY,
            DirectedAxis::PlusZ => PauliObservable::PlusZ,
            DirectedAxis::MinusX => PauliObservable::MinusX,
            DirectedAxis::MinusY => PauliObservable::MinusY,
            DirectedAxis::MinusZ => PauliObservable::MinusZ,
        }
    }
}

impl All<4> for PauliMatrix {
    fn all() -> [PauliMatrix; 4] {
        [PauliMatrix::I, PauliMatrix::X, PauliMatrix::Y, PauliMatrix::Z]
    }
}

impl All<3> for Axis {
    fn all() -> [Axis; 3] {
        [Axis::X, Axis::Y, Axis::Z]
    }
}

impl All<6> for DirectedAxis {
    fn all() -> [DirectedAxis; 6] {
        [
            DirectedAxis::PlusX,
            DirectedAxis::PlusY,
            DirectedAxis::PlusZ,
            DirectedAxis::MinusX,
            DirectedAxis::MinusY,
            DirectedAxis::MinusZ,
        ]
    }
}

impl All<8> for PauliObservable {
    fn all() -> [PauliObservable; 8] {
        [
            PauliObservable::PlusI,
            PauliObservable::PlusX,
            PauliObservable::PlusY,
            PauliObservable::PlusZ,
            PauliObservable::MinusI,
            PauliObservable::MinusX,
            PauliObservable::MinusY,
            PauliObservable::MinusZ,
        ]
    }
}

impl Neg for PauliObservable {
    type Output = PauliObservable;
    fn neg(self) -> Self::Output {
        match self {
            PauliObservable::PlusI => PauliObservable::MinusI,
            PauliObservable::PlusX => PauliObservable::MinusX,
            PauliObservable::PlusY => PauliObservable::MinusY,
            PauliObservable::PlusZ => PauliObservable::MinusZ,
            PauliObservable::MinusI => PauliObservable::PlusI,
            PauliObservable::MinusX => PauliObservable::PlusX,
            PauliObservable::MinusY => PauliObservable::PlusY,
            PauliObservable::MinusZ => PauliObservable::PlusZ,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct PositionedPauliObservable {
    pub qubit_id: usize,
    pub observable: PauliObservable,
}

impl Neg for PositionedPauliObservable {
    type Output = PositionedPauliObservable;
    fn neg(self) -> Self::Output {
        PositionedPauliObservable {
            qubit_id: self.qubit_id,
            observable: -self.observable,
        }
    }
}

#[must_use]
pub fn id(qubit_id: usize) -> PositionedPauliObservable {
    PositionedPauliObservable {
        qubit_id,
        observable: PauliObservable::PlusI,
    }
}

#[must_use]
pub fn x(qubit_id: usize) -> PositionedPauliObservable {
    PositionedPauliObservable {
        qubit_id,
        observable: PauliObservable::PlusX,
    }
}

#[must_use]
pub fn y(qubit_id: usize) -> PositionedPauliObservable {
    PositionedPauliObservable {
        qubit_id,
        observable: PauliObservable::PlusY,
    }
}

#[must_use]
pub fn z(qubit_id: usize) -> PositionedPauliObservable {
    PositionedPauliObservable {
        qubit_id,
        observable: PauliObservable::PlusZ,
    }
}

impl From<(usize, PauliObservable)> for PositionedPauliObservable {
    fn from(value: (usize, PauliObservable)) -> Self {
        PositionedPauliObservable {
            qubit_id: value.0,
            observable: value.1,
        }
    }
}

impl From<PositionedPauliObservable> for (usize, PauliObservable) {
    fn from(value: PositionedPauliObservable) -> Self {
        (value.qubit_id, value.observable)
    }
}

impl From<(PauliObservable, usize)> for PositionedPauliObservable {
    fn from(value: (PauliObservable, usize)) -> Self {
        PositionedPauliObservable {
            qubit_id: value.1,
            observable: value.0,
        }
    }
}

impl From<PositionedPauliObservable> for (PauliObservable, usize) {
    fn from(value: PositionedPauliObservable) -> Self {
        (value.observable, value.qubit_id)
    }
}
