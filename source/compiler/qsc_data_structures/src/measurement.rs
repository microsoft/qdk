// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// The Pauli basis of a single-qubit measurement.
///
/// Unlike the Q# `Pauli` value type, this has no identity variant, since every
/// single-qubit measurement is in one of the X, Y or Z bases.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OneQubitMeasurement {
    X,
    Y,
    Z,
}
