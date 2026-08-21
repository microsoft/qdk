// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::f64::consts::FRAC_1_SQRT_2;

use num_complex::Complex64;

use crate::{Matrix2, Matrix4};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QubitId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OneQubitGate {
    H,
    X,
    Y,
    Z,
    S,
    SAdj,
    Sx,
    SxAdj,
    T,
    TAdj,
    Rx(f64),
    Ry(f64),
    Rz(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TwoQubitGate {
    Cx,
    Cy,
    Cz,
    Swap,
    Rxx(f64),
    Ryy(f64),
    Rzz(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    One {
        gate: OneQubitGate,
        target: QubitId,
    },
    Two {
        gate: TwoQubitGate,
        first: QubitId,
        second: QubitId,
    },
    MeasureZ {
        target: QubitId,
    },
    MeasureResetZ {
        target: QubitId,
    },
    ResetZ {
        target: QubitId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationOutcome {
    Unit,
    Measurement(Measurement),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measurement {
    Zero,
    One,
}

impl OneQubitGate {
    pub(crate) fn matrix(self) -> Matrix2 {
        let zero = Complex64::ZERO;
        let one = Complex64::ONE;
        let i = Complex64::I;
        match self {
            Self::H => [
                [one * FRAC_1_SQRT_2, one * FRAC_1_SQRT_2],
                [one * FRAC_1_SQRT_2, -one * FRAC_1_SQRT_2],
            ],
            Self::X => [[zero, one], [one, zero]],
            Self::Y => [[zero, -i], [i, zero]],
            Self::Z => [[one, zero], [zero, -one]],
            Self::S => [[one, zero], [zero, i]],
            Self::SAdj => [[one, zero], [zero, -i]],
            Self::Sx => {
                let diagonal = Complex64::new(0.5, 0.5);
                let off_diagonal = Complex64::new(0.5, -0.5);
                [[diagonal, off_diagonal], [off_diagonal, diagonal]]
            }
            Self::SxAdj => {
                let diagonal = Complex64::new(0.5, -0.5);
                let off_diagonal = Complex64::new(0.5, 0.5);
                [[diagonal, off_diagonal], [off_diagonal, diagonal]]
            }
            Self::T => [
                [one, zero],
                [
                    zero,
                    Complex64::from_polar(1.0, std::f64::consts::FRAC_PI_4),
                ],
            ],
            Self::TAdj => [
                [one, zero],
                [
                    zero,
                    Complex64::from_polar(1.0, -std::f64::consts::FRAC_PI_4),
                ],
            ],
            Self::Rx(angle) => {
                let (sin, cos) = (angle / 2.0).sin_cos();
                [[one * cos, -i * sin], [-i * sin, one * cos]]
            }
            Self::Ry(angle) => {
                let (sin, cos) = (angle / 2.0).sin_cos();
                [[one * cos, -one * sin], [one * sin, one * cos]]
            }
            Self::Rz(angle) => [
                [Complex64::from_polar(1.0, -angle / 2.0), zero],
                [zero, Complex64::from_polar(1.0, angle / 2.0)],
            ],
        }
    }
}

impl TwoQubitGate {
    pub(crate) fn matrix(self) -> Matrix4 {
        let zero = Complex64::ZERO;
        let one = Complex64::ONE;
        let i = Complex64::I;
        match self {
            Self::Cx => [
                [one, zero, zero, zero],
                [zero, zero, zero, one],
                [zero, zero, one, zero],
                [zero, one, zero, zero],
            ],
            Self::Cy => [
                [one, zero, zero, zero],
                [zero, zero, zero, -i],
                [zero, zero, one, zero],
                [zero, i, zero, zero],
            ],
            Self::Cz => diagonal4([one, one, one, -one]),
            Self::Swap => [
                [one, zero, zero, zero],
                [zero, zero, one, zero],
                [zero, one, zero, zero],
                [zero, zero, zero, one],
            ],
            Self::Rxx(angle) => {
                pauli_rotation(angle, [(0, 3, one), (1, 2, one), (2, 1, one), (3, 0, one)])
            }
            Self::Ryy(angle) => pauli_rotation(
                angle,
                [(0, 3, -one), (1, 2, one), (2, 1, one), (3, 0, -one)],
            ),
            Self::Rzz(angle) => {
                let positive = Complex64::from_polar(1.0, -angle / 2.0);
                let negative = Complex64::from_polar(1.0, angle / 2.0);
                diagonal4([positive, negative, negative, positive])
            }
        }
    }
}

fn diagonal4(values: [Complex64; 4]) -> Matrix4 {
    let mut matrix = [[Complex64::ZERO; 4]; 4];
    for (index, value) in values.into_iter().enumerate() {
        matrix[index][index] = value;
    }
    matrix
}

fn pauli_rotation(angle: f64, entries: [(usize, usize, Complex64); 4]) -> Matrix4 {
    let (sin, cos) = (angle / 2.0).sin_cos();
    let mut matrix = diagonal4([Complex64::new(cos, 0.0); 4]);
    for (row, column, value) in entries {
        matrix[row][column] += -Complex64::I * sin * value;
    }
    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cx_uses_first_qubit_as_little_endian_control() {
        let matrix = TwoQubitGate::Cx.matrix();
        assert_eq!(matrix[3][1], Complex64::ONE);
        assert_eq!(matrix[1][3], Complex64::ONE);
    }
}
