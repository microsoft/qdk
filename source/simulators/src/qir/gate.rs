// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The gates of the machine's QIR instructions: its unitary instructions,
//! each with its matrix in the shared layout ([`OperatorMatrix`]).
//!
//! A simulator
//! that applies gates as matrices takes the matrix of a gate call
//! ([`QirCall::Gate`]) at the call's angle ([`QirGate::matrix_at`]), and
//! applies it to the call's qubits, which are in the order of the matrix's
//! operands. A gate the program does not call has its matrix at an angle
//! given the same way.
//!
//! Every unitary [`QirInstruction`] is a gate. Measurements, resets,
//! `move`, correlated noise, loss checks and readout noise are not gates:
//! they have no matrix.

use super::operator_matrix::{I, MINUS_I, MINUS_ONE, ONE, OperatorMatrix, ZERO};
#[cfg(doc)]
use super::{QirCall, QirInstruction};
use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;

/// A unitary [`QirInstruction`]: one that has a matrix.
/// Its variants are named for the instructions' mnemonics.
///
/// The matrix's operands are a call's qubits, in their order
/// ([`QirCall::Gate`]), the first the most significant: for `cx`, `cy`
/// and `cz`, the control. The
/// matrix of a gate that takes no angle has no negative zero, so that every
/// simulator applies the same bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QirGate {
    /// `x`, the Pauli X.
    X,
    /// `y`, the Pauli Y.
    Y,
    /// `z`, the Pauli Z.
    Z,
    /// `h`, the Hadamard gate.
    H,
    /// `s`, diag(1, i).
    S,
    /// `s_adj`, the inverse of S.
    SAdj,
    /// `t`, diag(1, e^{iπ/4}).
    T,
    /// `t_adj`, the inverse of T.
    TAdj,
    /// `sx`, the square root of X.
    Sx,
    /// `sx_adj`, the inverse of SX.
    SxAdj,
    /// `rx`, exp(-iθX/2).
    Rx,
    /// `ry`, exp(-iθY/2).
    Ry,
    /// `rz`, exp(-iθZ/2).
    Rz,
    /// `cx`, X on the second qubit when the first is 1.
    Cx,
    /// `cy`, Y on the second qubit when the first is 1.
    Cy,
    /// `cz`, Z on the second qubit when the first is 1.
    Cz,
    /// `rxx`, exp(-iθX⊗X/2).
    Rxx,
    /// `ryy`, exp(-iθY⊗Y/2).
    Ryy,
    /// `rzz`, exp(-iθZ⊗Z/2).
    Rzz,
    /// `swap`, which exchanges the two qubits.
    Swap,
}

impl QirGate {
    /// Its QIR mnemonic: the name of its `__quantum__qis__` function, as
    /// `cx` or `s_adj`.
    #[must_use]
    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::H => "h",
            Self::S => "s",
            Self::SAdj => "s_adj",
            Self::T => "t",
            Self::TAdj => "t_adj",
            Self::Sx => "sx",
            Self::SxAdj => "sx_adj",
            Self::Rx => "rx",
            Self::Ry => "ry",
            Self::Rz => "rz",
            Self::Cx => "cx",
            Self::Cy => "cy",
            Self::Cz => "cz",
            Self::Rxx => "rxx",
            Self::Ryy => "ryy",
            Self::Rzz => "rzz",
            Self::Swap => "swap",
        }
    }

    /// Whether this gate is a rotation, which takes an angle.
    pub(super) const fn is_rotation(self) -> bool {
        matches!(
            self,
            Self::Rx | Self::Ry | Self::Rz | Self::Rxx | Self::Ryy | Self::Rzz
        )
    }

    /// The number of qubits this gate acts on.
    pub(super) const fn qubit_count(self) -> usize {
        match self {
            Self::Cx | Self::Cy | Self::Cz | Self::Rxx | Self::Ryy | Self::Rzz | Self::Swap => 2,
            _ => 1,
        }
    }

    /// The matrix of this gate at `angle`, the same on any qubits: a gate
    /// call's angle ([`QirCall::Gate`]), or one given directly, for a
    /// simulator that applies a gate the program does not call, such as an
    /// `x` that returns a qubit read as 1 to |0⟩. `angle` is `Some` for a
    /// rotation and `None` for any other gate, as a gate call has it.
    ///
    /// # Panics
    ///
    /// When `angle` is `None` for a rotation, or `Some` for another gate.
    #[must_use]
    pub fn matrix_at(self, angle: Option<f64>) -> OperatorMatrix {
        assert!(
            angle.is_some() == self.is_rotation(),
            "a rotation takes an angle, and a gate that is not one takes none"
        );
        let c = Complex64::new;
        // The half angle's sine and cosine, for a rotation.
        let half = || (angle.unwrap_or_default() / 2.0).sin_cos();
        match self {
            Self::X => OperatorMatrix::One([ZERO, ONE, ONE, ZERO]),
            // Y = [[0, -i], [i, 0]]. It is not symmetric: its transpose is -Y.
            Self::Y => OperatorMatrix::One([ZERO, MINUS_I, I, ZERO]),
            Self::Z => OperatorMatrix::One([ONE, ZERO, ZERO, MINUS_ONE]),
            // Negative entries are written, not negated, so the zero parts
            // of the fixed gates' entries are +0.0 and the values
            // bit-identical across simulators.
            Self::H => OperatorMatrix::One([
                c(FRAC_1_SQRT_2, 0.0),
                c(FRAC_1_SQRT_2, 0.0),
                c(FRAC_1_SQRT_2, 0.0),
                c(-FRAC_1_SQRT_2, 0.0),
            ]),
            Self::S => OperatorMatrix::One([ONE, ZERO, ZERO, I]),
            Self::SAdj => OperatorMatrix::One([ONE, ZERO, ZERO, MINUS_I]),
            // T = diag(1, e^{iπ/4}), the square root of S.
            Self::T => OperatorMatrix::One([ONE, ZERO, ZERO, c(FRAC_1_SQRT_2, FRAC_1_SQRT_2)]),
            Self::TAdj => OperatorMatrix::One([ONE, ZERO, ZERO, c(FRAC_1_SQRT_2, -FRAC_1_SQRT_2)]),
            // SX = ((1 + i) I + (1 - i) X) / 2, the square root of X that
            // QIR's `sx` and the QDK simulators apply.
            Self::Sx => OperatorMatrix::One([c(0.5, 0.5), c(0.5, -0.5), c(0.5, -0.5), c(0.5, 0.5)]),
            // SX† = ((1 - i) I + (1 + i) X) / 2, the inverse of SX.
            Self::SxAdj => {
                OperatorMatrix::One([c(0.5, -0.5), c(0.5, 0.5), c(0.5, 0.5), c(0.5, -0.5)])
            }
            // Rx(θ) = exp(-iθX/2) = cos(θ/2) I - i sin(θ/2) X.
            Self::Rx => {
                let (sine, cosine) = half();
                OperatorMatrix::One([c(cosine, 0.0), c(0.0, -sine), c(0.0, -sine), c(cosine, 0.0)])
            }
            // Ry(θ) = exp(-iθY/2) = [[cos(θ/2), -sin(θ/2)], [sin(θ/2),
            // cos(θ/2)]]. It is not symmetric: the -sin entry is M[0][1], so
            // its transpose would apply Ry(-θ).
            Self::Ry => {
                let (sine, cosine) = half();
                OperatorMatrix::One([c(cosine, 0.0), c(-sine, 0.0), c(sine, 0.0), c(cosine, 0.0)])
            }
            // Rz(θ) = exp(-iθZ/2) = diag(e^{-iθ/2}, e^{iθ/2}).
            Self::Rz => {
                let (sine, cosine) = half();
                OperatorMatrix::One([c(cosine, -sine), ZERO, ZERO, c(cosine, sine)])
            }
            Self::Cx => OperatorMatrix::Two([
                ONE, ZERO, ZERO, ZERO, //
                ZERO, ONE, ZERO, ZERO, //
                ZERO, ZERO, ZERO, ONE, //
                ZERO, ZERO, ONE, ZERO,
            ]),
            // CY applies Y to the target (second operand) when the control
            // (first operand) is 1. Swapping the operands gives a different
            // matrix.
            Self::Cy => OperatorMatrix::Two([
                ONE, ZERO, ZERO, ZERO, //
                ZERO, ONE, ZERO, ZERO, //
                ZERO, ZERO, ZERO, MINUS_I, //
                ZERO, ZERO, I, ZERO,
            ]),
            Self::Cz => OperatorMatrix::Two([
                ONE, ZERO, ZERO, ZERO, //
                ZERO, ONE, ZERO, ZERO, //
                ZERO, ZERO, ONE, ZERO, //
                ZERO, ZERO, ZERO, MINUS_ONE,
            ]),
            // Rxx(θ) = exp(-iθX⊗X/2) = cos(θ/2) I - i sin(θ/2) X⊗X.
            Self::Rxx => {
                let (sine, cosine) = half();
                let (diagonal, flip) = (c(cosine, 0.0), c(0.0, -sine));
                OperatorMatrix::Two([
                    diagonal, ZERO, ZERO, flip, //
                    ZERO, diagonal, flip, ZERO, //
                    ZERO, flip, diagonal, ZERO, //
                    flip, ZERO, ZERO, diagonal,
                ])
            }
            // Ryy(θ) = exp(-iθY⊗Y/2) = cos(θ/2) I - i sin(θ/2) Y⊗Y. Y⊗Y is
            // -1 on |00⟩↔|11⟩ and +1 on |01⟩↔|10⟩, hence the opposite signs.
            Self::Ryy => {
                let (sine, cosine) = half();
                let (diagonal, equal, differ) = (c(cosine, 0.0), c(0.0, sine), c(0.0, -sine));
                OperatorMatrix::Two([
                    diagonal, ZERO, ZERO, equal, //
                    ZERO, diagonal, differ, ZERO, //
                    ZERO, differ, diagonal, ZERO, //
                    equal, ZERO, ZERO, diagonal,
                ])
            }
            // Rzz(θ) = exp(-iθZ⊗Z/2): phase e^{-iθ/2} on equal bits,
            // e^{iθ/2} otherwise.
            Self::Rzz => {
                let (sine, cosine) = half();
                let (equal, differ) = (c(cosine, -sine), c(cosine, sine));
                OperatorMatrix::Two([
                    equal, ZERO, ZERO, ZERO, //
                    ZERO, differ, ZERO, ZERO, //
                    ZERO, ZERO, differ, ZERO, //
                    ZERO, ZERO, ZERO, equal,
                ])
            }
            Self::Swap => OperatorMatrix::Two([
                ONE, ZERO, ZERO, ZERO, //
                ZERO, ZERO, ONE, ZERO, //
                ZERO, ONE, ZERO, ZERO, //
                ZERO, ZERO, ZERO, ONE,
            ]),
        }
    }
}
