// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The matrices of the gates, as one- and two-qubit operators in one layout
//! ([`OperatorMatrix`]), for a resumer that applies gates as tensors.

use num_complex::Complex64;

pub(crate) const ZERO: Complex64 = Complex64::new(0.0, 0.0);
pub(crate) const ONE: Complex64 = Complex64::new(1.0, 0.0);
pub(crate) const MINUS_ONE: Complex64 = Complex64::new(-1.0, 0.0);
pub(crate) const I: Complex64 = Complex64::new(0.0, 1.0);
pub(crate) const MINUS_I: Complex64 = Complex64::new(0.0, -1.0);

/// A one- or two-qubit operator, as
/// [`QirGate::matrix_at`](super::QirGate::matrix_at) gives a gate's matrix.
///
/// Its layout is `M[out][in]`, row-major. So `M[out][in]` is at
/// `out · d + in` for dimension `d`. For two qubits `(a, b)`, the first
/// operand is the most significant bit: basis index `2a + b`. This is the
/// textbook matrix. A simulator whose layout differs (a tensor library
/// reading its modes in another order, say) maps the entries itself.
///
/// The layout fixes the orientation, not just the entries. Y, Ry and CY are
/// not symmetric, and their transposes (-Y, Ry(-θ), CY with -Y) rotate the
/// other way.
///
/// Which operators a simulator accepts is its own decision. This layout
/// only defines their values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OperatorMatrix {
    /// A 2×2 operator, on one qubit.
    One([Complex64; 4]),
    /// A 4×4 operator, on two qubits.
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

    /// Whether every off-diagonal entry is exactly zero, so a simulator may
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
