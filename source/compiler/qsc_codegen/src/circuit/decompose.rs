// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rewriting gates that have no QIR intrinsic into ones that do.
//!
//! The compiler emits a fixed set of quantum intrinsics, and several gates a
//! circuit can hold are not in it. Rather than refusing those circuits, each
//! such gate is rewritten here into a sequence drawn from the set that is
//! emitted.
//!
//! Every rewrite is verified against the gate's own matrix in this module's
//! tests, because a wrong rewrite produces a program that runs and is silently
//! incorrect.

#[cfg(test)]
mod tests;

use qsc_circuit_store::StandardGate;

/// One gate in a rewrite.
pub struct Step {
    pub gate: StandardGate,
    pub qubits: Vec<u32>,
    pub angles: Vec<f64>,
}

impl Step {
    fn new(gate: StandardGate, qubits: &[u32], angles: &[f64]) -> Self {
        Self {
            gate,
            qubits: qubits.to_vec(),
            angles: angles.to_vec(),
        }
    }
}

/// Rewrites a gate into the intrinsic set, or returns `None` if it needs no
/// rewriting.
///
/// `qubits` and `angles` are the operands of the gate being rewritten.
#[must_use]
pub fn decompose(gate: StandardGate, qubits: &[u32], angles: &[f64]) -> Option<Vec<Step>> {
    let angle = angles.first().copied().unwrap_or_default();
    let half = angle / 2.0;
    Some(match gate {
        // The square root of X has order four, so its adjoint is its cube. That
        // is exact, where a rotation would differ by a global phase.
        StandardGate::SqrtXDag => {
            let q = &qubits[..1];
            vec![
                Step::new(StandardGate::SqrtX, q, &[]),
                Step::new(StandardGate::SqrtX, q, &[]),
                Step::new(StandardGate::SqrtX, q, &[]),
            ]
        }
        StandardGate::Cswap => {
            let (control, a, b) = (qubits[0], qubits[1], qubits[2]);
            vec![
                Step::new(StandardGate::Cx, &[b, a], &[]),
                Step::new(StandardGate::Ccx, &[control, a, b], &[]),
                Step::new(StandardGate::Cx, &[b, a], &[]),
            ]
        }
        // A controlled rotation splits into two half rotations separated by
        // controlled flips, so the halves cancel when the control is zero and
        // compose when it is one.
        StandardGate::Crz => controlled_rotation(StandardGate::Rz, qubits, half),
        StandardGate::Cry => controlled_rotation(StandardGate::Ry, qubits, half),
        // Conjugating by H turns the Z rotation into an X rotation.
        StandardGate::Crx => {
            let target = &qubits[1..2];
            let mut steps = vec![Step::new(StandardGate::H, target, &[])];
            steps.extend(controlled_rotation(StandardGate::Rz, qubits, half));
            steps.push(Step::new(StandardGate::H, target, &[]));
            steps
        }
        // Measuring another basis rotates it onto Z, measures, and rotates back,
        // so the qubit is left in an eigenstate of the basis that was measured.
        StandardGate::MeasureX => basis_measurement(&qubits[..1], &[StandardGate::H]),
        StandardGate::MeasureY => {
            basis_measurement(&qubits[..1], &[StandardGate::SDag, StandardGate::H])
        }
        // Preparing another basis prepares the Z basis state and rotates it.
        StandardGate::PrepareX => {
            let q = &qubits[..1];
            vec![
                Step::new(StandardGate::PrepareZ, q, &[]),
                Step::new(StandardGate::H, q, &[]),
            ]
        }
        StandardGate::PrepareY => {
            let q = &qubits[..1];
            vec![
                Step::new(StandardGate::PrepareZ, q, &[]),
                Step::new(StandardGate::H, q, &[]),
                Step::new(StandardGate::S, q, &[]),
            ]
        }
        _ => return None,
    })
}

/// Builds a controlled rotation from half rotations and controlled flips.
fn controlled_rotation(rotation: StandardGate, qubits: &[u32], half: f64) -> Vec<Step> {
    let (control, target) = (qubits[0], qubits[1]);
    let flip = || Step::new(StandardGate::Cx, &[control, target], &[]);
    vec![
        Step::new(rotation, &[target], &[half]),
        flip(),
        Step::new(rotation, &[target], &[-half]),
        flip(),
    ]
}

/// Builds a measurement in the basis reached by `onto_z`, restoring it after.
fn basis_measurement(qubits: &[u32], onto_z: &[StandardGate]) -> Vec<Step> {
    let mut steps: Vec<Step> = onto_z
        .iter()
        .map(|gate| Step::new(*gate, qubits, &[]))
        .collect();
    steps.push(Step::new(StandardGate::MeasureZ, qubits, &[]));
    steps.extend(
        onto_z
            .iter()
            .rev()
            .map(|gate| Step::new(adjoint(*gate), qubits, &[])),
    );
    steps
}

/// The adjoint of a gate used to change basis.
fn adjoint(gate: StandardGate) -> StandardGate {
    match gate {
        StandardGate::H => StandardGate::H,
        StandardGate::S => StandardGate::SDag,
        StandardGate::SDag => StandardGate::S,
        other => panic!("no adjoint recorded for {}", other.name()),
    }
}
