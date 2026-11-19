// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering: the translation of each quantum call of a batch, on qubits
//! ([`QirCall`]), into the steps the MPS simulation method runs on an
//! engine ([`SiteStep`]), on sites, through the qubit order
//! ([`QubitOrder`]): so that the engine knows no gate names and no qubits,
//! only matrices and sites. It is the only place the resumer refuses a
//! call (the engine may still refuse a gate).
//!
//! A gate is its matrix at its angle
//! ([`QirGate::matrix_at`](crate::qir::QirGate::matrix_at)), as the engine
//! takes it (`M[out][in]`, row-major, the first site the most
//! significant), on the sites of its qubits in its operand order, so the
//! matrix holds on them unchanged. A measurement and a reset are draws
//! the resumer makes; a measurement in X or Y is one in Z between the
//! gates that turn that basis into Z and back, as the QDK's Python
//! simulation lowers `mx` and `my` (`DecomposeMxMyPass`): `h` around an
//! `mx`, `s_adj` then `h` before an `my`, and `h` then `s` after it.
//! `move` changes no state (a noisy simulator may
//! charge it with idle noise or loss, planned), so it is no step.
//! Correlated noise, readout noise and loss checks are refused
//! ([`MpsResumerError::Unsupported`]), as is a gate on one qubit twice.

use crate::qir::{OperatorMatrix, QirCall, QirGate};
use qsc_data_structures::measurement::MeasurementBasis;

use super::mps_resumer_error::MpsResumerError;
use super::qubit_order::QubitOrder;

/// What the MPS simulation method does with a call, on sites. A new kind
/// of step runs in `GroupedRun::run_step`, which decides whether it reads
/// the largest bond first, as a draw does.
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
/// a unitary on one or two distinct sites, as the engine takes it, the
/// first of [`sites`](Self::sites) the matrix's most significant bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SiteGate {
    /// Room for two sites, the second unused for a one-site matrix: read
    /// the sites it acts on through [`sites`](Self::sites).
    pub(crate) site_slots: [usize; 2],
    pub(crate) matrix: OperatorMatrix,
}

impl SiteGate {
    /// The sites it acts on, in the matrix's order.
    pub(crate) fn sites(&self) -> &[usize] {
        &self.site_slots[..self.matrix.qubit_count()]
    }
}

/// The steps of `calls`, in order, each qubit on the site `qubit_order`
/// gives it.
///
/// # Errors
///
/// The first call that the MPS simulation method does not run (noise,
/// loss) or cannot run (a gate on one qubit twice).
pub(crate) fn lower<E>(
    calls: &[QirCall],
    qubit_order: &QubitOrder,
) -> Result<Vec<SiteStep>, MpsResumerError<E>> {
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
                    None => [qubit_order.site(first), 0],
                    Some(second) if second == first => {
                        return Err(MpsResumerError::RepeatedQubit {
                            instruction: call.instruction(),
                            qubit: first,
                        });
                    }
                    Some(second) => [qubit_order.site(first), qubit_order.site(second)],
                };
                steps.push(SiteStep::Apply(SiteGate {
                    site_slots: sites,
                    matrix,
                }));
            }
            QirCall::Measure {
                basis,
                qubit,
                result,
            } => {
                let site = qubit_order.site(qubit);
                let (to_z, from_z): (&[QirGate], &[QirGate]) = match basis {
                    MeasurementBasis::Z => (&[], &[]),
                    MeasurementBasis::X => (&[QirGate::H], &[QirGate::H]),
                    MeasurementBasis::Y => {
                        (&[QirGate::SAdj, QirGate::H], &[QirGate::H, QirGate::S])
                    }
                };
                steps.extend(to_z.iter().map(|&gate| on_site(gate, site)));
                steps.push(SiteStep::Measure {
                    site,
                    result,
                    reset: false,
                });
                steps.extend(from_z.iter().map(|&gate| on_site(gate, site)));
            }
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

/// The step that applies the one-qubit `gate` on `site`.
fn on_site(gate: QirGate, site: usize) -> SiteStep {
    SiteStep::Apply(SiteGate {
        site_slots: [site, 0],
        matrix: gate.matrix_at(None),
    })
}
