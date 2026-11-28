// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering: the translation of each quantum call of a batch, on qubits
//! ([`QirCall`]), into the steps the MPS simulation method runs on an
//! engine ([`SiteStep`]), on sites, through the qubit order
//! ([`QubitOrder`]), so that the engine knows no gate names and no qubits,
//! only matrices and sites. For instance, when the order puts qubit 0 on
//! site 2 and qubit 3 on site 1, `cx(0, 3)` becomes CX's matrix on sites
//! 2 and 1, in that order. It is the only place the resumer refuses a
//! call (the engine may still refuse a gate).

use crate::qir::{OperatorMatrix, QirCall, QirGate, QirInstruction};
use qsc_data_structures::measurement::MeasurementBasis;

use super::mps_resumer_error::MpsResumerError;
use super::qubit_order::QubitOrder;

/// What the MPS simulation method does with a call, on sites. A new kind
/// of step is made here, by [`lower`], from the calls it stands for.
#[expect(
    clippy::large_enum_variant,
    reason = "a gate's matrix is held in place, unboxed: a batch's steps \
              live in one slice, and a box would allocate once per gate"
)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SiteStep {
    /// Applies a gate on one or two sites.
    Apply(SiteGate),
    /// Draws the value of `site`, projects on it, and answers `result`
    /// with it; then, when `reset`, returns the site to |0⟩.
    Measure {
        site: usize,
        result: u32,
        reset: bool,
    },
    /// Draws the value of `site`, projects on it, and returns it to |0⟩.
    Reset { site: usize },
}

/// One gate of the program, on the sites the qubit order gave its qubits:
/// a unitary on one or two distinct sites, as the engine takes it. The
/// first of [`sites`](Self::sites) is the matrix's most significant bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SiteGate {
    /// Room for two sites. A one-site matrix leaves the second unused.
    site_slots: [usize; 2],
    /// The gate's unitary at its angle, on the sites of
    /// [`sites`](Self::sites).
    pub(crate) matrix: OperatorMatrix,
}

impl SiteGate {
    /// `matrix` on `sites`, one site per qubit of `matrix`, in the matrix's
    /// order.
    pub(crate) fn new(matrix: &OperatorMatrix, sites: &[usize]) -> Self {
        debug_assert_eq!(
            sites.len(),
            matrix.qubit_count(),
            "a site per qubit of the matrix"
        );
        let mut site_slots = [0; 2];
        site_slots[..sites.len()].copy_from_slice(sites);
        Self {
            site_slots,
            matrix: *matrix,
        }
    }

    /// The sites it acts on, in the matrix's order.
    pub(crate) fn sites(&self) -> &[usize] {
        &self.site_slots[..self.matrix.qubit_count()]
    }
}

/// The steps of `calls`, in order, each qubit on the site `qubit_order`
/// gives it.
///
/// - A gate is its matrix at its angle
///   ([`QirGate::matrix_at`](crate::qir::QirGate::matrix_at)), as the
///   engine takes it: `M[out][in]`, row-major, the first site the most
///   significant. It acts on the sites of its qubits, in its operand
///   order. So the matrix holds on them unchanged ([`lower_gate`]).
/// - A measurement and a reset are draws the resumer makes; a measurement
///   in X or Y is one in Z between the gates that turn that basis into Z
///   and back ([`lower_measurement`]).
/// - `move` changes no state, so it is no step.
///
/// # Errors
///
/// The first call that the MPS simulation method does not run (correlated
/// noise, readout noise, a qubit's loss check:
/// [`MpsResumerError::Unsupported`]) or cannot run (a gate on one qubit
/// twice: [`MpsResumerError::RepeatedQubit`]).
pub(crate) fn lower<E>(
    calls: &[QirCall],
    qubit_order: &QubitOrder,
) -> Result<Vec<SiteStep>, MpsResumerError<E>> {
    let mut steps = Vec::with_capacity(calls.len());
    for call in calls {
        match *call {
            QirCall::Gate {
                gate,
                qubits,
                angle,
            } => steps.push(lower_gate(gate, qubits, angle, qubit_order)?),
            QirCall::Measure {
                basis,
                qubit,
                result,
            } => lower_measurement(basis, qubit_order.site(qubit), result, &mut steps),
            QirCall::MeasureReset { qubit, result } => steps.push(SiteStep::Measure {
                site: qubit_order.site(qubit),
                result,
                reset: true,
            }),
            QirCall::Reset { qubit } => steps.push(SiteStep::Reset {
                site: qubit_order.site(qubit),
            }),
            QirCall::Move { .. } => {}
            QirCall::CorrelatedNoise { .. }
            | QirCall::PeekLoss { .. }
            | QirCall::ReadoutNoise { .. } => {
                return Err(MpsResumerError::Unsupported {
                    instruction: call.instruction(),
                });
            }
        }
    }
    Ok(steps)
}

/// The step of `gate` at `angle` on `qubits`: its matrix on their sites,
/// in its operand order.
///
/// # Errors
///
/// [`MpsResumerError::RepeatedQubit`] when a two-qubit gate names one
/// qubit twice, which no unitary on two distinct sites can hold.
fn lower_gate<E>(
    gate: QirGate,
    (first, second): (u32, Option<u32>),
    angle: Option<f64>,
    qubit_order: &QubitOrder,
) -> Result<SiteStep, MpsResumerError<E>> {
    let sites: &[usize] = match second {
        None => &[qubit_order.site(first)],
        Some(second) if second == first => {
            return Err(MpsResumerError::RepeatedQubit {
                instruction: QirInstruction::Gate(gate),
                qubit: first,
            });
        }
        Some(second) => &[qubit_order.site(first), qubit_order.site(second)],
    };
    let site_gate = SiteGate::new(&gate.matrix_at(angle), sites);
    Ok(SiteStep::Apply(site_gate))
}

/// Appends to `steps` the measurement of `site` in `basis` into `result`.
/// It is a draw in Z, between the gates that turn `basis` into Z and back
/// ([`basis_change`]). The steps, in the order they run:
///
/// ```text
///  mz   measure
///  mx   H, measure, H
///  my   S†, H, measure, H, S
/// ```
fn lower_measurement(basis: MeasurementBasis, site: usize, result: u32, steps: &mut Vec<SiteStep>) {
    let (to_z, from_z) = basis_change(basis);
    steps.extend(to_z.iter().map(|&gate| on_site(gate, site)));
    steps.push(SiteStep::Measure {
        site,
        result,
        reset: false,
    });
    steps.extend(from_z.iter().map(|&gate| on_site(gate, site)));
}

/// The gates that turn `basis` into Z, and those that turn Z back into
/// `basis`, each in the order they run.
fn basis_change(basis: MeasurementBasis) -> (&'static [QirGate], &'static [QirGate]) {
    // COMPATIBILITY NOTE: this behavior was present in the QDK's Python
    // simulation's `DecomposeMxMyPass` (qdk/simulation/_simulation.py),
    // hence we lower `mx` and `my` into the same gates around `mz`.
    match basis {
        MeasurementBasis::Z => (&[], &[]),
        MeasurementBasis::X => (&[QirGate::H], &[QirGate::H]),
        MeasurementBasis::Y => (&[QirGate::SAdj, QirGate::H], &[QirGate::H, QirGate::S]),
    }
}

/// The step that applies the one-qubit `gate` on `site`.
fn on_site(gate: QirGate, site: usize) -> SiteStep {
    SiteStep::Apply(SiteGate::new(&gate.matrix_at(None), &[site]))
}

#[cfg(test)]
mod tests;
