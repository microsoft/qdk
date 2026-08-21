// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4};

use num_complex::Complex64;
use qdk_mps::{OneQubitGate, Pauli, TwoQubitGate};

pub(super) type Matrix2 = [[Complex64; 2]; 2];
pub(super) type Matrix4 = [[Complex64; 4]; 4];

pub(super) struct DenseState {
    values: Vec<Complex64>,
}

impl DenseState {
    pub(super) fn zero(num_qubits: usize) -> Self {
        let mut values = vec![Complex64::ZERO; 1 << num_qubits];
        values[0] = Complex64::ONE;
        Self { values }
    }

    pub(super) fn apply_one(&mut self, target: usize, gate: OneQubitGate) {
        let matrix = one_matrix(gate);
        let mask = 1 << target;
        for base in 0..self.values.len() {
            if base & mask == 0 {
                let one = base | mask;
                let zero_value = self.values[base];
                let one_value = self.values[one];
                self.values[base] = matrix[0][0] * zero_value + matrix[0][1] * one_value;
                self.values[one] = matrix[1][0] * zero_value + matrix[1][1] * one_value;
            }
        }
    }

    pub(super) fn apply_two(&mut self, first: usize, second: usize, gate: TwoQubitGate) {
        let matrix = two_matrix(gate);
        let first_mask = 1 << first;
        let second_mask = 1 << second;
        for base in 0..self.values.len() {
            if base & (first_mask | second_mask) == 0 {
                let indices = [
                    base,
                    base | first_mask,
                    base | second_mask,
                    base | first_mask | second_mask,
                ];
                let previous = indices.map(|index| self.values[index]);
                for (row, index) in indices.into_iter().enumerate() {
                    self.values[index] = matrix[row]
                        .iter()
                        .zip(previous)
                        .map(|(entry, value)| entry * value)
                        .sum();
                }
            }
        }
    }

    pub(super) fn expectation(&self, factors: &[(usize, Pauli)]) -> f64 {
        let mut transformed = self.values.clone();
        for (target, pauli) in factors {
            apply_matrix(&mut transformed, *target, pauli_matrix(*pauli));
        }
        let norm_squared: f64 = self.values.iter().map(Complex64::norm_sqr).sum();
        self.values
            .iter()
            .zip(transformed)
            .map(|(left, right)| left.conj() * right)
            .sum::<Complex64>()
            .re
            / norm_squared
    }
}

fn apply_matrix(values: &mut [Complex64], target: usize, matrix: Matrix2) {
    let mask = 1 << target;
    for base in 0..values.len() {
        if base & mask == 0 {
            let one = base | mask;
            let zero_value = values[base];
            let one_value = values[one];
            values[base] = matrix[0][0] * zero_value + matrix[0][1] * one_value;
            values[one] = matrix[1][0] * zero_value + matrix[1][1] * one_value;
        }
    }
}

fn one_matrix(gate: OneQubitGate) -> Matrix2 {
    let zero = Complex64::ZERO;
    let one = Complex64::ONE;
    let i = Complex64::I;
    match gate {
        OneQubitGate::H => [
            [one * FRAC_1_SQRT_2, one * FRAC_1_SQRT_2],
            [one * FRAC_1_SQRT_2, -one * FRAC_1_SQRT_2],
        ],
        OneQubitGate::X => [[zero, one], [one, zero]],
        OneQubitGate::Y => [[zero, -i], [i, zero]],
        OneQubitGate::Z => [[one, zero], [zero, -one]],
        OneQubitGate::S => [[one, zero], [zero, i]],
        OneQubitGate::SAdj => [[one, zero], [zero, -i]],
        OneQubitGate::Sx => {
            let diagonal = Complex64::new(0.5, 0.5);
            let off_diagonal = Complex64::new(0.5, -0.5);
            [[diagonal, off_diagonal], [off_diagonal, diagonal]]
        }
        OneQubitGate::SxAdj => {
            let diagonal = Complex64::new(0.5, -0.5);
            let off_diagonal = Complex64::new(0.5, 0.5);
            [[diagonal, off_diagonal], [off_diagonal, diagonal]]
        }
        OneQubitGate::T => [[one, zero], [zero, Complex64::from_polar(1.0, FRAC_PI_4)]],
        OneQubitGate::TAdj => [[one, zero], [zero, Complex64::from_polar(1.0, -FRAC_PI_4)]],
        OneQubitGate::Rx(angle) => {
            let (sin, cos) = (angle / 2.0).sin_cos();
            [[one * cos, -i * sin], [-i * sin, one * cos]]
        }
        OneQubitGate::Ry(angle) => {
            let (sin, cos) = (angle / 2.0).sin_cos();
            [[one * cos, -one * sin], [one * sin, one * cos]]
        }
        OneQubitGate::Rz(angle) => [
            [Complex64::from_polar(1.0, -angle / 2.0), zero],
            [zero, Complex64::from_polar(1.0, angle / 2.0)],
        ],
    }
}

fn two_matrix(gate: TwoQubitGate) -> Matrix4 {
    let zero = Complex64::ZERO;
    let one = Complex64::ONE;
    let i = Complex64::I;
    match gate {
        TwoQubitGate::Cx => [
            [one, zero, zero, zero],
            [zero, zero, zero, one],
            [zero, zero, one, zero],
            [zero, one, zero, zero],
        ],
        TwoQubitGate::Cy => [
            [one, zero, zero, zero],
            [zero, zero, zero, -i],
            [zero, zero, one, zero],
            [zero, i, zero, zero],
        ],
        TwoQubitGate::Cz => diagonal4([one, one, one, -one]),
        TwoQubitGate::Swap => [
            [one, zero, zero, zero],
            [zero, zero, one, zero],
            [zero, one, zero, zero],
            [zero, zero, zero, one],
        ],
        TwoQubitGate::Rxx(angle) => {
            pauli_rotation(angle, [(0, 3, one), (1, 2, one), (2, 1, one), (3, 0, one)])
        }
        TwoQubitGate::Ryy(angle) => pauli_rotation(
            angle,
            [(0, 3, -one), (1, 2, one), (2, 1, one), (3, 0, -one)],
        ),
        TwoQubitGate::Rzz(angle) => {
            let even = Complex64::from_polar(1.0, -angle / 2.0);
            let odd = Complex64::from_polar(1.0, angle / 2.0);
            diagonal4([even, odd, odd, even])
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

fn pauli_matrix(pauli: Pauli) -> Matrix2 {
    match pauli {
        Pauli::I => [
            [Complex64::ONE, Complex64::ZERO],
            [Complex64::ZERO, Complex64::ONE],
        ],
        Pauli::X => one_matrix(OneQubitGate::X),
        Pauli::Y => one_matrix(OneQubitGate::Y),
        Pauli::Z => one_matrix(OneQubitGate::Z),
    }
}
