// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Region gates to matrices: each [`UnitaryOperation`] of a region is applied
//! as its matrix from `qdk_simulators`' shared table
//! ([`unitary_matrix`]) on its qubits in operand order
//! ([`UnitaryOperation::qubits`]), which is the layout
//! [`MpsEngine::apply`] takes (row-major, the first site most significant).
//!
//! M0's ④ did the same through its own `Circuit` of `Gate`s: it listed the
//! gates, checked their qubits and angles, dropped `I`, and applied one
//! operator per gate; it did not fuse gates. So one matrix per operation,
//! unfused, keeps ④'s numbers: the batch strategy and the state queries
//! apply gates so. The per-shot terminal applies a region as its
//! [`GateFusion`](crate::GateFusion) groups it (`fusion.rs`), unfused by
//! default.

use qdk_simulators::{
    QubitID,
    execution::{QuantumEvolutionRegion, UnitaryOperation, unitary_matrix},
};
use tensornet::MpsEngine;

/// Why `operation` cannot be applied to `qubits` qubits: a qubit out of
/// range, a two-qubit operation on one qubit, or an angle that is not
/// finite. The messages are M0's `Circuit` checks'.
pub(crate) fn check_operation(operation: UnitaryOperation, qubits: usize) -> Option<String> {
    let (first, second) = operation.qubits();
    if let Some(qubit) = std::iter::once(first)
        .chain(second)
        .find(|&qubit| qubit >= qubits)
    {
        return Some(format!(
            "{}: qubit {qubit} is outside a {qubits}-qubit circuit",
            operation.name()
        ));
    }
    if second == Some(first) {
        return Some(format!(
            "{}: its two qubits must differ (both are {first})",
            operation.name()
        ));
    }
    if angle(operation).is_some_and(|angle| !angle.is_finite()) {
        return Some(format!(
            "{}: rotation angle must be finite",
            operation.name()
        ));
    }
    None
}

/// The first operation of `region` that cannot be applied, and why.
pub(crate) fn check_region(region: &QuantumEvolutionRegion, qubits: usize) -> Option<String> {
    region
        .operations()
        .iter()
        .find_map(|&operation| check_operation(operation, qubits))
}

/// Applies `operation`, checked by [`check_operation`], to `state`, each
/// qubit on the site of the same number (the identity
/// [`QubitOrder`](crate::QubitOrder)). `I` is dropped, as M0 dropped it.
pub(crate) fn apply_operation<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    operation: UnitaryOperation,
) -> Result<(), E::Error> {
    apply_placed(engine, state, operation, |qubit| qubit)
}

/// Applies `operation`, checked by [`check_operation`], to `state`, each
/// qubit on the site `site_of` gives it, in operand order, so the gate
/// table's matrix holds on those sites. `I` is dropped, as M0 dropped it.
pub(crate) fn apply_placed<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    operation: UnitaryOperation,
    site_of: impl Fn(QubitID) -> usize,
) -> Result<(), E::Error> {
    let Some(matrix) = unitary_matrix(operation) else {
        return Ok(());
    };
    let sites: &[usize] = match operation.qubits() {
        (first, None) => &[site_of(first)],
        (first, Some(second)) => &[site_of(first), site_of(second)],
    };
    engine.apply(state, sites, matrix.row_major())
}

/// Flips the qubit on `site` (X), as a reset does to a qubit measured One.
pub(crate) fn flip<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    site: usize,
) -> Result<(), E::Error> {
    apply_operation(engine, state, UnitaryOperation::X { target: site })
}

/// Whether `operation` is applied, rather than dropped (`I`).
pub(crate) fn is_applied(operation: UnitaryOperation) -> bool {
    !matches!(operation, UnitaryOperation::I { .. })
}

fn angle(operation: UnitaryOperation) -> Option<f64> {
    match operation {
        UnitaryOperation::Rx { angle, .. }
        | UnitaryOperation::Ry { angle, .. }
        | UnitaryOperation::Rz { angle, .. }
        | UnitaryOperation::Rxx { angle, .. }
        | UnitaryOperation::Ryy { angle, .. }
        | UnitaryOperation::Rzz { angle, .. } => Some(angle),
        _ => None,
    }
}
