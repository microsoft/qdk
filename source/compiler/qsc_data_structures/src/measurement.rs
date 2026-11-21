// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// The Pauli basis of a single-qubit measurement.
///
/// Unlike the Q# `Pauli` value type, this has no identity variant, since every
/// single-qubit measurement is in one of the X, Y or Z bases.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MeasurementBasis {
    X,
    Y,
    Z,
}

/// The Pauli basis of a joint measurement of two qubits.
///
/// The first letter denotes the basis of the first qubit, and the second
/// letter denotes the basis of the second qubit. Thus the observable measured
/// is the tensor product of the two single-qubit Pauli observables.
/// # Examples
///
/// - `XX` measures the observable `X ⊗ X`.
/// - `YY` measures the observable `Y ⊗ Y`.
/// - `ZZ` measures the observable `Z ⊗ Z`.
/// - `ZY` measures the observable `Z ⊗ Y`.
/// # Panics
///
/// This enum has no variants that can cause a panic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JointMeasurementBasis {
    XX,
    YY,
    ZZ,
    ZY,
}
