// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A batch's quantum calls ([`QirCall`]) as the steps the method runs on
//! an engine ([`Step`]), on sites: so that the engine knows no gate names
//! and no qubits, only matrices and sites.
//!
//! A gate is its matrix at its angle ([`QirGate::matrix_at`]), in the
//! layout the engine takes (`M[out][in]`, row-major, the first site the
//! most significant), on the sites of its qubits in its operand order, so
//! the matrix holds on them unchanged. A measurement and a reset are draws
//! the resumer makes; `move` changes no state and is no step. Correlated
//! noise, readout noise and loss checks are refused
//! ([`MpsResumerError::Unsupported`]), as is a gate on one qubit twice.
//!
//! [`QirGate::matrix_at`]: crate::qir::QirGate::matrix_at

use crate::qir::{OperatorMatrix, QirCall};

use super::order::QubitOrder;
use crate::mps_simulator::MpsResumerError;

/// What the method does with a call, on sites.
// A gate's matrix is held in place, unboxed: a batch's steps live in one
// slice, and a box would allocate once per gate.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    /// Applies a unitary on one or two sites.
    Apply(Operator),
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

/// A unitary on one or two distinct sites, in the engine's layout: the
/// first of [`sites`](Self::sites) is the matrix's most significant bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Operator {
    /// The sites, the second unused for a one-site matrix.
    pub(crate) sites: [usize; 2],
    pub(crate) matrix: OperatorMatrix,
}

impl Operator {
    /// The sites it acts on, in the matrix's order.
    pub(crate) fn sites(&self) -> &[usize] {
        &self.sites[..self.matrix.qubit_count()]
    }
}

/// The steps of `calls`, in order, each qubit on the site `order` gives it.
///
/// # Errors
///
/// The first call that the method does not run (noise, loss) or cannot run
/// (a gate on one qubit twice).
pub(crate) fn lower<E>(
    calls: &[QirCall],
    order: &QubitOrder,
) -> Result<Vec<Step>, MpsResumerError<E>> {
    let mut steps = Vec::with_capacity(calls.len());
    for call in calls {
        match *call {
            QirCall::Gate {
                gate,
                qubits: (first, second),
                angle,
            } => {
                let matrix = gate.matrix_at(angle);
                let sites = match second {
                    None => [order.site(first), 0],
                    Some(second) if second == first => {
                        return Err(MpsResumerError::RepeatedQubit {
                            instruction: call.instruction(),
                            qubit: first,
                        });
                    }
                    Some(second) => [order.site(first), order.site(second)],
                };
                steps.push(Step::Apply(Operator { sites, matrix }));
            }
            QirCall::Measure { qubit, result } => steps.push(Step::Measure {
                site: order.site(qubit),
                result,
                reset: false,
            }),
            QirCall::MeasureReset { qubit, result } => steps.push(Step::Measure {
                site: order.site(qubit),
                result,
                reset: true,
            }),
            QirCall::Reset { qubit } => steps.push(Step::Reset {
                site: order.site(qubit),
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
