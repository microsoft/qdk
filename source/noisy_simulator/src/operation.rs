// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module contains the `Operation` struct and the `operation!` macro
//! to conveniently construct operations from Kraus matrices.

#[cfg(test)]
mod tests;
use crate::{Error, SquareMatrix};

/// A helper macro to write operations more conveniently.
///
/// Example usage:
/// ```
/// // Create operation from two 2x2 Kraus matrices.
/// use noisy_simulator::{Operation, operation};
///
/// let op = operation!(
///     [1., 0.;
///      0., 0.;],
///     [0., 0.;
///      0., 0.;]
/// ).expect("operation should be valid");
/// ```
#[macro_export]
macro_rules! operation {
    ($([$($($v:expr),* );*]),*) => {
        $crate::Operation::new(vec![
            $(nalgebra::dmatrix![
                $($(num_complex::Complex::<f64>::from($v)),* );*
            ]),*
        ])
    };
}

#[cfg(test)]
pub(crate) use operation;

/// This struct represents a quantum operation.
/// A quantum operation is a linear transformation that maps a valid density
/// matrix to another valid density matrices.
#[derive(Clone)]
pub struct Operation {
    number_of_qubits: usize,
    kraus_operators: Vec<SquareMatrix>,
    #[allow(clippy::struct_field_names)]
    operation_matrix: SquareMatrix,
    effect_matrix: SquareMatrix,
    effect_matrix_transpose: SquareMatrix,
}

impl Operation {
    /// Construct an operation from a list of Kraus operators.
    /// Matrices must be of dimension 2^k x 2^k, where k is an integer.
    /// Returns `None` if the kraus matrices are ill formed.
    pub fn new(mut kraus_operators: Vec<SquareMatrix>) -> Result<Self, Error> {
        let (dim, _) = kraus_operators
            .first()
            .ok_or(Error::FailedToConstructOperation(
                "there should be at least one Kraus Operator".to_string(),
            ))?
            .shape();

        let number_of_qubits = dim.ilog2() as usize;
        if 1 << number_of_qubits != dim {
            return Err(Error::FailedToConstructOperation(
                "kraus operators should have dimensions 2^k x 2^k".to_string(),
            ));
        }

        for kraus_operator in &kraus_operators {
            let (rows, cols) = kraus_operator.shape();
            if rows != dim || cols != dim {
                return Err(Error::FailedToConstructOperation(
                    "kraus operators should be square matrices and have the same dimensions"
                        .to_string(),
                ));
            }
        }

        // Performance note: Because `nalgebra` stores its matrices in column major
        // form, we use `gemv_tr` in the `apply_kernel` function when multiplying to avoid
        // incurring cache misses. That is why we transpose all Kraus operators when they
        // enter the simulator:
        //   `gemv(1, matrix, vec, 0)` is equivalent to `gemv_tr(1, matrix_tr, vec, 0)`,
        // but the later has much better performance.
        //
        // SAFETY of transposing: all these matrices are only consumed by the
        // `kernel.rs/apply_kernel` function which effectively transposes them
        // back when multypling, so it is safe to do this transformation.
        for kraus_operator in &mut kraus_operators {
            kraus_operator.transpose_mut();
        }

        // Performance note: The effect_matrix = Σᵢ (kᵢ† ⋅ k), but due to performance
        // reasons described above we want to store its transpose. But (A ⋅ B)^T = B^T ⋅ A^T.
        // Therefore, we have to swap the order of the factors.
        let effect_matrix: SquareMatrix = kraus_operators.iter().map(|k| k * k.adjoint()).sum();

        // Performance note: (A ⊗ B)^T = A^T ⊗ B^T, so we don't need to change
        // anything here.
        let operation_matrix: SquareMatrix = kraus_operators
            .iter()
            .map(|k| k.kronecker(&k.conjugate()))
            .sum();

        let effect_matrix_transpose = effect_matrix.transpose();

        Ok(Self {
            number_of_qubits,
            kraus_operators,
            operation_matrix,
            effect_matrix,
            effect_matrix_transpose,
        })
    }

    /// Return matrix representation:
    /// Σᵢ (Kᵢ ⊗ Kᵢ*)
    /// where Kᵢ are Kraus operators, ⊗ is the Kronecker product
    /// and * denotes the complex conjugate of the matrix.
    #[must_use]
    pub fn matrix(&self) -> &SquareMatrix {
        &self.operation_matrix
    }

    /// Returns effect matrix:
    /// Σᵢ (Kᵢ Kᵢ†)
    /// where Kᵢ are Kraus operators and † denotes the adjoint of the matrix.
    #[must_use]
    pub fn effect_matrix(&self) -> &SquareMatrix {
        &self.effect_matrix
    }

    /// Return transpose of effect matrix:
    /// Σᵢ (Kᵢ Kᵢ†)^T
    /// where Kᵢ are Kraus operators and † denotes the adjoint of the matrix.
    #[must_use]
    pub fn effect_matrix_transpose(&self) -> &SquareMatrix {
        &self.effect_matrix_transpose
    }

    /// Return list of Kraus operators.
    #[must_use]
    pub fn kraus_operators(&self) -> &Vec<SquareMatrix> {
        &self.kraus_operators
    }

    /// Return the number of qubits that the operation acts on.
    #[must_use]
    pub fn number_of_qubits(&self) -> usize {
        self.number_of_qubits
    }
}
