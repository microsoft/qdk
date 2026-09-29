// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shared operator table: one definition of each gate matrix and of the
//! fixed-outcome basis operators |r⟩⟨b| for every backend.
//!
//! Layout: `M[out][in]`, row-major, so `M[out][in]` is at `out · d + in` for
//! dimension `d`. For two qubits `(a, b)` the first operand is the most
//! significant bit: basis index `2a + b`. This is the textbook matrix, which
//! cuTensorNet's `cutensornetStateApplyTensorOperator` reads correctly with
//! default (null) strides, so that backend copies it verbatim; the exact
//! tensor network maps entries into its column-major axes with
//! `tensornet::Indices::offset_of`.
//!
//! The layout must fix the orientation, not just the entries: every unitary
//! in the table is symmetric, but |0⟩⟨1| is not, and its transpose |1⟩⟨0|
//! would select the other branch.
//!
//! Which gates a backend accepts is its own decision; the table only defines
//! their values.

use std::f64::consts::FRAC_1_SQRT_2;

use num_complex::Complex64;

use super::UnitaryOperation;

/// A one- or two-qubit operator in the shared layout (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OperatorMatrix {
    /// A 2×2 operator, row-major `M[out][in]`.
    One([Complex64; 4]),
    /// A 4×4 operator, row-major `M[out][in]`, first operand most significant.
    Two([Complex64; 16]),
}

impl OperatorMatrix {
    /// The number of qubits the operator acts on.
    #[must_use]
    pub fn qubit_count(&self) -> usize {
        match self {
            Self::One(_) => 1,
            Self::Two(_) => 2,
        }
    }

    /// The matrix dimension: 2 for one qubit, 4 for two.
    #[must_use]
    pub fn dimension(&self) -> usize {
        1 << self.qubit_count()
    }

    /// All entries, row-major `M[out][in]`.
    #[must_use]
    pub fn row_major(&self) -> &[Complex64] {
        match self {
            Self::One(values) => values,
            Self::Two(values) => values,
        }
    }

    /// The entry `M[output][input]`.
    ///
    /// # Panics
    ///
    /// Panics if either index is not below [`Self::dimension`].
    #[must_use]
    pub fn entry(&self, output: usize, input: usize) -> Complex64 {
        let dimension = self.dimension();
        assert!(
            output < dimension && input < dimension,
            "operator index out of range"
        );
        self.row_major()[output * dimension + input]
    }

    /// Whether every off-diagonal entry is exactly zero, so a backend may
    /// store the diagonal alone.
    #[must_use]
    pub fn is_diagonal(&self) -> bool {
        let dimension = self.dimension();
        self.row_major()
            .iter()
            .enumerate()
            .all(|(index, value)| index / dimension == index % dimension || *value == ZERO)
    }
}

const ZERO: Complex64 = Complex64::new(0.0, 0.0);
const ONE: Complex64 = Complex64::new(1.0, 0.0);
const MINUS_ONE: Complex64 = Complex64::new(-1.0, 0.0);

/// The matrix of `operation`, or `None` for gates outside the table.
///
/// The table holds X, H, S, Sx, Rx, Rz, Cx, Cz and Rzz: the gates some
/// backend accepts. `I` is omitted because backends drop it rather than
/// apply it. Operands are ignored: the matrix is the same on any qubits, in
/// the operand order of `operation`.
#[must_use]
pub fn unitary_matrix(operation: UnitaryOperation) -> Option<OperatorMatrix> {
    let c = Complex64::new;
    let matrix = match operation {
        UnitaryOperation::X { .. } => OperatorMatrix::One([ZERO, ONE, ONE, ZERO]),
        // Negative entries are written, not negated, so their zero imaginary
        // parts stay +0.0 and the values stay bit-identical across backends.
        UnitaryOperation::H { .. } => OperatorMatrix::One([
            c(FRAC_1_SQRT_2, 0.0),
            c(FRAC_1_SQRT_2, 0.0),
            c(FRAC_1_SQRT_2, 0.0),
            c(-FRAC_1_SQRT_2, 0.0),
        ]),
        UnitaryOperation::S { .. } => OperatorMatrix::One([ONE, ZERO, ZERO, c(0.0, 1.0)]),
        // SX = ((1 + i) I + (1 - i) X) / 2, the square root of X that QIR's
        // `sx` and the QDK simulators apply.
        UnitaryOperation::Sx { .. } => {
            OperatorMatrix::One([c(0.5, 0.5), c(0.5, -0.5), c(0.5, -0.5), c(0.5, 0.5)])
        }
        // Rx(θ) = exp(-iθX/2) = cos(θ/2) I - i sin(θ/2) X.
        UnitaryOperation::Rx { angle, .. } => {
            let (sine, cosine) = (angle / 2.0).sin_cos();
            OperatorMatrix::One([c(cosine, 0.0), c(0.0, -sine), c(0.0, -sine), c(cosine, 0.0)])
        }
        // Rz(θ) = exp(-iθZ/2) = diag(e^{-iθ/2}, e^{iθ/2}).
        UnitaryOperation::Rz { angle, .. } => {
            let (sine, cosine) = (angle / 2.0).sin_cos();
            OperatorMatrix::One([c(cosine, -sine), ZERO, ZERO, c(cosine, sine)])
        }
        UnitaryOperation::Cx { .. } => OperatorMatrix::Two([
            ONE, ZERO, ZERO, ZERO, //
            ZERO, ONE, ZERO, ZERO, //
            ZERO, ZERO, ZERO, ONE, //
            ZERO, ZERO, ONE, ZERO,
        ]),
        UnitaryOperation::Cz { .. } => OperatorMatrix::Two([
            ONE, ZERO, ZERO, ZERO, //
            ZERO, ONE, ZERO, ZERO, //
            ZERO, ZERO, ONE, ZERO, //
            ZERO, ZERO, ZERO, MINUS_ONE,
        ]),
        // Rzz(θ) = exp(-iθZ⊗Z/2): phase e^{-iθ/2} on equal bits, e^{iθ/2} otherwise.
        UnitaryOperation::Rzz { angle, .. } => {
            let (sine, cosine) = (angle / 2.0).sin_cos();
            let (equal, differ) = (c(cosine, -sine), c(cosine, sine));
            OperatorMatrix::Two([
                equal, ZERO, ZERO, ZERO, //
                ZERO, differ, ZERO, ZERO, //
                ZERO, ZERO, differ, ZERO, //
                ZERO, ZERO, ZERO, equal,
            ])
        }
        UnitaryOperation::I { .. }
        | UnitaryOperation::Y { .. }
        | UnitaryOperation::Z { .. }
        | UnitaryOperation::SAdj { .. }
        | UnitaryOperation::SxAdj { .. }
        | UnitaryOperation::T { .. }
        | UnitaryOperation::TAdj { .. }
        | UnitaryOperation::Ry { .. }
        | UnitaryOperation::Cy { .. }
        | UnitaryOperation::Rxx { .. }
        | UnitaryOperation::Ryy { .. }
        | UnitaryOperation::Swap { .. } => return None,
    };
    Some(matrix)
}

/// The fixed-outcome basis operator |result⟩⟨basis| (`true` = |1⟩).
///
/// A measurement that reads `b` projects with ⟨b|; the qubit then continues
/// in |r⟩, with `r = 0` after a reset and `r = b` otherwise. |0⟩⟨1| is not
/// Hermitian: it maps |1⟩ to |0⟩ and annihilates |0⟩.
#[must_use]
pub fn basis_operator(result: bool, basis: bool) -> OperatorMatrix {
    let mut values = [ZERO; 4];
    values[usize::from(result) * 2 + usize::from(basis)] = ONE;
    OperatorMatrix::One(values)
}
