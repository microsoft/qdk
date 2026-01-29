// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noiseless full-state simulator.
//!
//! The full-state simulator uses a dense state vector representation to
//! simulate quantum circuits exactly. This module verifies that gates
//! satisfy their expected algebraic identities.
//!
//! # Supported Gates
//!
//! ```text
//! | Category          | Gates                                      |
//! |-------------------|--------------------------------------------|
//! | Single-qubit      | I, X, Y, Z, H, S, S_ADJ, SX, SX_ADJ, T, T_ADJ |
//! | Two-qubit         | CX, CY, CZ, SWAP                           |
//! | Three-qubit       | CCX                                        |
//! | Rotation          | Rx, Ry, Rz, Rxx, Ryy, Rzz                  |
//! | Measurement       | M, MZ, MRESETZ, RESET                      |
//! | Other             | MOV                                        |
//! ```
//!
//! # Gate Properties
//!
//! The `~` symbol denotes equivalence up to global phase.
//!
//! ```text
//! | Gate    | Properties                                        |
//! |---------|---------------------------------------------------|
//! | I       | I ~ {} (identity does nothing)                    |
//! | X       | X flips qubit, X X ~ I                            |
//! | Y       | Y ~ X Z ~ Z X, Y Y ~ I                            |
//! | Z       | H Z H ~ X                                         |
//! | H       | H^2 ~ I (self-inverse), H X H ~ Z                 |
//! | S       | S^2 ~ Z                                           |
//! | S_ADJ   | S S_ADJ ~ I, S_ADJ^2 ~ Z                          |
//! | SX      | SX^2 ~ X                                          |
//! | SX_ADJ  | SX SX_ADJ ~ I, SX_ADJ^2 ~ X                       |
//! | T       | T^4 ~ Z                                           |
//! | T_ADJ   | T T_ADJ ~ I, T_ADJ^4 ~ Z                          |
//! | CX      | CX on |0⟩ control ~ I, CX on |1⟩ control ~ X      |
//! | CZ      | CZ on |0⟩ control ~ I, CZ on |1⟩ control ~ Z      |
//! | SWAP    | (X ⊗ Z) SWAP ~ Z ⊗ X                              |
//! | Rx      | Rx(0) ~ I, Rx(π) ~ X, Rx(π/2) ~ SX                |
//! | Ry      | Ry(0) ~ I, Ry(π) ~ Y                              |
//! | Rz      | Rz(0) ~ I, Rz(π) ~ Z, Rz(π/2) ~ S, Rz(π/4) ~ T    |
//! | Rxx     | Rxx(0) ~ I, Rxx(π) ~ X ⊗ X                        |
//! | Ryy     | Ryy(0) ~ I, Ryy(π) ~ Y ⊗ Y                        |
//! | Rzz     | Rzz(0) ~ I, Rzz(π) ~ Z ⊗ Z                        |
//! | M       | M ~ M M (idempotent)                              |
//! | RESET   | OP RESET ~ I (resets to |0⟩)                      |
//! | MRESETZ | OP MRESETZ ~ I (measures and resets)              |
//! | MOV     | MOV ~ I (no-op in noiseless simulation)           |
//! ```

use super::{super::*, test_utils::*};
use expect_test::expect;
use std::f64::consts::PI;

// ==================== Single-Qubit Gate Tests ====================

// I gate tests
#[test]
fn i_gate_does_nothing() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! {},
            qir! { i(0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// H gate tests
#[test]
fn h_squared_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { h(0); h(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn h_x_h_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0) },
            qir! { h(0); x(0); h(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// X gate tests
#[test]
fn x_gate_flips_qubit() {
    check_sim! {
        simulator: NoiselessSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn double_x_gate_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); x(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// Z gate tests
#[test]
fn x_gate_eq_h_z_h() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0) },
            qir! { h(0); z(0); h(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// Y gate tests
#[test]
fn y_gate_eq_x_z_and_z_x() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { y(0) },
            qir! { x(0); z(0); },
            qir! { z(0); x(0); },
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// S gate tests
#[test]
fn s_squared_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0) },
            qir! { s(0); s(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// S_ADJ gate tests
#[test]
fn s_and_s_adj_cancel() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { s(0); s_adj(0); },
            qir! { s_adj(0); s(0); },
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn s_adj_squared_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0) },
            qir! { s_adj(0); s_adj(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// SX gate tests
#[test]
fn sx_squared_eq_x() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0) },
            qir! { sx(0); sx(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// SX_ADJ gate tests
#[test]
fn sx_and_sx_adj_cancel() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { sx(0); sx_adj(0); },
            qir! { sx_adj(0); sx(0); },
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn sx_adj_squared_eq_x() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0) },
            qir! { sx_adj(0); sx_adj(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// T gate tests
#[test]
fn t_fourth_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0) },
            qir! { t(0); t(0); t(0); t(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// T_ADJ gate tests
#[test]
fn t_and_t_adj_cancel() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { t(0); t_adj(0); },
            qir! { t_adj(0); t(0); },
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn t_adj_fourth_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0) },
            qir! { t_adj(0); t_adj(0); t_adj(0); t_adj(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// M gate tests
#[test]
fn m_eq_m_m() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { mz(0, 0) },
            qir! { mz(0, 0); mz(0, 0); }
        ],
        num_qubits: 1,
        num_results: 1,
    }
}

// RESET gate tests
#[test]
fn op_reset_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); reset(0); }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// MRESETZ gate tests
#[test]
fn op_mresetz_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); mresetz(0, 0); }
        ],
        num_qubits: 1,
        num_results: 1,
    }
}

// MOV gate tests
#[test]
fn mov_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { mov(0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// ==================== Two-Qubit Gate Tests ====================

// CX gate tests
#[test]
fn cx_on_zero_control_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(1) },
            qir! { cx(0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

#[test]
fn cx_on_one_control_eq_x() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0); x(1) },
            qir! { x(0); cx(0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

// CZ gate tests
#[test]
fn cz_on_zero_control_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(1) },
            qir! { cz(0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

#[test]
fn cz_on_one_control_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0); within { h(1) } apply { z(1) } },
            qir! { x(0); within { h(1) } apply { cz(0, 1) } }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

// SWAP gate tests
#[test]
fn xz_swap_eq_zx() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0); x(1) },
            qir! { x(0); z(1); swap(0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

// ==================== Rotation Gate Tests ====================

// Rx gate tests
#[test]
fn rx_zero_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { rx(0.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rx_two_pi_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { rx(2.0 * PI, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rx_pi_eq_x() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0) },
            qir! { rx(PI, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rx_half_pi_eq_sx() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { sx(0) },
            qir! { rx(PI / 2.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rx_neg_half_pi_eq_sx_adj() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { sx_adj(0) },
            qir! { rx(-PI / 2.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// Ry gate tests
#[test]
fn ry_zero_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { ry(0.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn ry_two_pi_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { ry(2.0 * PI, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn ry_pi_eq_y() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { y(0) },
            qir! { ry(PI, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// Rz gate tests
#[test]
fn rz_zero_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { rz(0.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rz_two_pi_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0) },
            qir! { rz(2.0 * PI, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rz_pi_eq_z() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0) },
            qir! { rz(PI, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rz_half_pi_eq_s() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { s(0) },
            qir! { rz(PI / 2.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rz_neg_half_pi_eq_s_adj() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { s_adj(0) },
            qir! { rz(-PI / 2.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rz_quarter_pi_eq_t() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { t(0) },
            qir! { rz(PI / 4.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

#[test]
fn rz_neg_quarter_pi_eq_t_adj() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { t_adj(0) },
            qir! { rz(-PI / 4.0, 0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// ==================== Two-Qubit Rotation Gate Tests ====================

// Rxx gate tests
#[test]
fn rxx_zero_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0); i(1) },
            qir! { rxx(0.0, 0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

#[test]
fn rxx_pi_eq_x_tensor_x() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { x(0); x(1) },
            qir! { rxx(PI, 0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

// Ryy gate tests
#[test]
fn ryy_zero_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0); i(1) },
            qir! { ryy(0.0, 0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

#[test]
fn ryy_pi_eq_y_tensor_y() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { y(0); y(1) },
            qir! { ryy(PI, 0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

// Rzz gate tests
#[test]
fn rzz_zero_eq_identity() {
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { i(0); i(1) },
            qir! { rzz(0.0, 0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}

#[test]
fn rzz_pi_eq_z_tensor_z() {
    // Z⊗Z on |00⟩ gives |00⟩ (both have eigenvalue +1)
    // This is equivalent to identity on computational basis states
    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { z(0); z(1) },
            qir! { rzz(PI, 0, 1) }
        ],
        num_qubits: 2,
        num_results: 0,
    }

    check_programs_are_eq! {
        simulator: NoiselessSimulator,
        programs: [
            qir! { within { h(0); h(1) } apply { z(0); z(1) } },
            qir! { within { h(0); h(1) } apply { rzz(PI, 0, 1) } }
        ],
        num_qubits: 2,
        num_results: 0,
    }
}
