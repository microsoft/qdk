// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noiseless GPU full-state simulator.
//!
//! The GPU full-state simulator runs quantum circuits on the GPU using
//! WebGPU compute shaders. This module verifies that the GPU simulator
//! produces correct measurement results for noiseless circuits.
//!
//! # Notes
//!
//! - All tests require a compatible GPU adapter; they are skipped otherwise.
//! - The GPU simulator does not expose internal state, so only measurement-based
//!   tests (`check_sim!`) are used. State-equivalence tests (`check_programs_are_eq!`)
//!   are not applicable.
//! - The GPU has a minimum of 8 qubits internally, but this is transparent to tests.
//! - Rotation gates on the GPU use f32 precision, so minor numerical differences
//!   compared to the f64 CPU simulator are expected.
//!
//! # Supported Gates
//!
//! ```text
//! | Category          | Gates                                      |
//! |-------------------|--------------------------------------------|
//! | Single-qubit      | I, X, Y, Z, H, S, S_ADJ, SX, SX_ADJ, T, T_ADJ |
//! | Two-qubit         | CX, CY, CZ, SWAP                           |
//! | Rotation          | Rx, Ry, Rz, Rxx, Ryy, Rzz                  |
//! | Measurement       | MRESETZ                                    |
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
//! | Y       | Y flips qubit, Y ~ X Z ~ Z X, Y Y ~ I             |
//! | Z       | Z|0⟩ = |0⟩, Z|1⟩ = |1⟩, H Z H ~ X                 |
//! | H       | H^2 ~ I, H X H ~ Z, creates superposition         |
//! | S       | S^2 ~ Z, S preserves computational basis          |
//! | S_ADJ   | S S_ADJ ~ I, S_ADJ^2 ~ Z                          |
//! | SX      | SX^2 ~ X                                          |
//! | SX_ADJ  | SX SX_ADJ ~ I, SX_ADJ^2 ~ X                       |
//! | T       | T^4 ~ Z                                           |
//! | T_ADJ   | T T_ADJ ~ I, T_ADJ^4 ~ Z                          |
//! | CX      | CX on |0⟩ control ~ I, CX on |1⟩ control ~ X      |
//! | CZ      | CZ on |0⟩ control ~ I, CZ(a,b) = CZ(b,a)          |
//! | SWAP    | Exchanges states, SWAP SWAP ~ I                   |
//! | Rx      | Rx(0) ~ I, Rx(π) ~ X, Rx(π/2) ~ SX                |
//! | Ry      | Ry(0) ~ I, Ry(π) ~ Y                              |
//! | Rz      | Rz(0) ~ I, Rz(π) ~ Z, Rz(π/2) ~ S, Rz(π/4) ~ T    |
//! | Rxx     | Rxx(0) ~ I, Rxx(π) ~ X ⊗ X                        |
//! | Ryy     | Ryy(0) ~ I, Ryy(π) ~ Y ⊗ Y                        |
//! | Rzz     | Rzz(0) ~ I, Rzz(π) ~ Z ⊗ Z                        |
//! | M       | M ~ M M (idempotent, does not reset)              |
//! | MZ      | MZ ~ MZ MZ (idempotent, does not reset)           |
//! | RESET   | OP RESET ~ |0⟩ (resets to |0⟩)                      |
//! | MRESETZ | OP MRESETZ ~ |0⟩ (measures and resets)              |
//! | MOV     | MOV ~ I (no-op in noiseless simulation)           |
//! ```
//!
//! # Multi-Qubit States
//!
//! ```text
//! | State | Preparation                | Expected Outcomes   |
//! |-------|----------------------------|---------------------|
//! | Bell  | H(0); CX(0,1)              | 00 or 11 (50/50)    |
//! | GHZ   | H(0); CX(0,1); CX(1,2)     | 000 or 111 (50/50)  |
//! ```

use super::{SEED, test_utils::*};
use expect_test::expect;
use std::f64::consts::PI;

// ==================== Generic Simulator Tests ====================

#[test]
fn simulator_completes_all_shots() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 50,
        format: summary,
        output: expect![[r#"
            shots: 50
            unique: 1
            loss: 0"#]],
    }
}

// ==================== Gate Truth Table Tests ====================
//
// These tests verify each gate's action on all computational basis states.
// Bit i of the input/output value represents qubit i.

#[test]
fn single_qubit_gate_truth_tables() {
    require_gpu!();
    check_basis_table! {
        simulator: GpuSimulator,
        num_qubits: 1,
        table: [
            // I gate: identity
            (qir! { i(0) }, 0 => 0),
            (qir! { i(0) }, 1 => 1),
            // X gate: bit flip
            (qir! { x(0) }, 0 => 1),
            (qir! { x(0) }, 1 => 0),
            // Y gate: bit flip (phase differs but same basis state)
            (qir! { y(0) }, 0 => 1),
            (qir! { y(0) }, 1 => 0),
            // Z gate: phase only, no bit change
            (qir! { z(0) }, 0 => 0),
            (qir! { z(0) }, 1 => 1),
            // Z gate: bit flip within H
            (qir! { within { h(0) } apply { z(0) } }, 0 => 1),
            (qir! { within { h(0) } apply { z(0) } }, 1 => 0),
            // S gate: phase only
            (qir! { s(0) }, 0 => 0),
            (qir! { s(0) }, 1 => 1),
            // S_ADJ gate: phase only
            (qir! { s_adj(0) }, 0 => 0),
            (qir! { s_adj(0) }, 1 => 1),
            // T gate: phase only
            (qir! { t(0) }, 0 => 0),
            (qir! { t(0) }, 1 => 1),
            // T_ADJ gate: phase only
            (qir! { t_adj(0) }, 0 => 0),
            (qir! { t_adj(0) }, 1 => 1),
        ],
    }
}

#[test]
fn two_qubit_gate_truth_tables() {
    require_gpu!();
    check_basis_table! {
        simulator: GpuSimulator,
        num_qubits: 2,
        table: [
            // CX(control=q0, target=q1): flips q1 when q0=|1⟩
            (qir! { cx(0, 1) }, 0b00 => 0b00),
            (qir! { cx(0, 1) }, 0b01 => 0b11),  // q0=1 → flip q1
            (qir! { cx(0, 1) }, 0b10 => 0b10),  // q0=0 → identity
            (qir! { cx(0, 1) }, 0b11 => 0b01),  // q0=1 → flip q1
            // CY gate: phase only, flips q1 when q0=|1⟩
            (qir! { cy(0, 1) }, 0b00 => 0b00),
            (qir! { cy(0, 1) }, 0b01 => 0b11),  // q0=1 → flip q1
            (qir! { cy(0, 1) }, 0b10 => 0b10),  // q0=0 → identity
            (qir! { cy(0, 1) }, 0b11 => 0b01),  // q0=1 → flip q1
            // CZ gate: phase only, no bit changes
            (qir! { cz(0, 1) }, 0b00 => 0b00),
            (qir! { cz(0, 1) }, 0b01 => 0b01),
            (qir! { cz(0, 1) }, 0b10 => 0b10),
            (qir! { cz(0, 1) }, 0b11 => 0b11),
            // CZ gate: bitflip within H
            (qir! { within { h(1) } apply { cz(0, 1) } }, 0b00 => 0b00),
            (qir! { within { h(1) } apply { cz(0, 1) } }, 0b01 => 0b11),
            (qir! { within { h(1) } apply { cz(0, 1) } }, 0b10 => 0b10),
            (qir! { within { h(1) } apply { cz(0, 1) } }, 0b11 => 0b01),
            // SWAP gate: exchanges qubit states
            (qir! { swap(0, 1) }, 0b00 => 0b00),
            (qir! { swap(0, 1) }, 0b01 => 0b10),
            (qir! { swap(0, 1) }, 0b10 => 0b01),
            (qir! { swap(0, 1) }, 0b11 => 0b11),
        ],
    }
}

// ==================== Single-Qubit Gate Tests ====================

// X gate tests
#[test]
fn x_is_self_adjoint() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); x(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn x_eq_h_z_h() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { x(0) },
            qir! { within { h(0) } apply { z(0) } }
        ],
        num_qubits: 1,
    }
}

// Y gate tests
#[test]
fn y_is_self_adjoint() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { y(0); y(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn y_gate_eq_x_z_and_z_x() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { y(0) },
            qir! { x(0); z(0) },
            qir! { z(0); x(0) },
        ],
        num_qubits: 1,
    }
}

// Z gate tests
#[test]
fn z_is_self_adjoint() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { within { h(0) } apply { z(0); z(0) } }
        ],
        num_qubits: 1,
    }
}

#[test]
fn z_eq_h_x_h() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0) },
            qir! { within { h(0) } apply { x(0) } }
        ],
        num_qubits: 1,
    }
}

// H gate tests
#[test]
fn h_gate_creates_superposition() {
    require_gpu!();
    // H creates equal superposition - should see both 0 and 1
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        format: outcomes,
        output: expect![[r#"
                    0
                    1"#]],
    }
}

#[test]
fn h_is_self_adjoint() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { h(0); h(0) }
        ],
        num_qubits: 1,
    }
}

// S gate tests
#[test]
fn s_squared_eq_z() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0) },
            qir! { s(0); s(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn s_and_s_adj_cancel() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { s(0); s_adj(0) },
            qir! { s_adj(0); s(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn s_adj_squared_eq_z() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0) },
            qir! { s_adj(0); s_adj(0) }
        ],
        num_qubits: 1,
    }
}

// SX gate tests
#[test]
fn sx_squared_eq_x() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { x(0) },
            qir! { sx(0); sx(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn sx_and_sx_adj_cancel() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { sx(0); sx_adj(0) },
            qir! { sx_adj(0); sx(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn sx_adj_squared_eq_x() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { x(0) },
            qir! { sx_adj(0); sx_adj(0) }
        ],
        num_qubits: 1,
    }
}

// T gate tests
#[test]
fn t_fourth_eq_z() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0) },
            qir! { t(0); t(0); t(0); t(0); }
        ],
        num_qubits: 1,
    }
}

// T_ADJ gate tests
#[test]
fn t_and_t_adj_cancel() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { t(0); t_adj(0); },
            qir! { t_adj(0); t(0); },
        ],
        num_qubits: 1,
    }
}

#[test]
fn t_adj_fourth_eq_z() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0) },
            qir! { t_adj(0); t_adj(0); t_adj(0); t_adj(0); }
        ],
        num_qubits: 1,
    }
}

// ==================== Two-Qubit Gate Tests ====================

#[test]
fn cz_symmetric() {
    require_gpu!();
    // CZ is symmetric: CZ(a,b) = CZ(b,a)
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { within { x(0); h(1) } apply { cz(0, 1) } },
            qir! { within { x(0); h(1) } apply { cz(1, 0) } }
        ],
        num_qubits: 2,
    }
}

// SWAP gate tests
#[test]
fn swap_commutes_operands() {
    require_gpu!();
    // SWAP · (A⊗B) = (B⊗A) · SWAP for any single-qubit gates A, B.
    // Test with A=X, B=H: SWAP·(X⊗H)·SWAP = H⊗X
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { h(0); x(1) },
            qir! { within { swap(0, 1) } apply { x(0); h(1) } }
        ],
        num_qubits: 2,
    }
}

#[test]
fn swap_exchanges_qubit_states() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            swap(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"01"#]],
    }
}

#[test]
fn swap_twice_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { x(0) },
            qir! { x(0); swap(0, 1); swap(0, 1) }
        ],
        num_qubits: 2,
    }
}

// ==================== Rotation Gate Tests ====================

// Rx gate tests
#[test]
fn rx_zero_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { rx(0.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rx_two_pi_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { rx(2.0 * PI, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rx_pi_eq_x() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { x(0) },
            qir! { rx(PI, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rx_half_pi_eq_sx() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { sx(0) },
            qir! { rx(PI / 2.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rx_neg_half_pi_eq_sx_adj() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { sx_adj(0) },
            qir! { rx(-PI / 2.0, 0) }
        ],
        num_qubits: 1,
    }
}

// Ry gate tests
#[test]
fn ry_zero_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { ry(0.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn ry_two_pi_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { ry(2.0 * PI, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn ry_pi_eq_y() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { y(0) },
            qir! { ry(PI, 0) }
        ],
        num_qubits: 1,
    }
}

// Rz gate tests
#[test]
fn rz_zero_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { rz(0.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rz_two_pi_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0) },
            qir! { rz(2.0 * PI, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rz_pi_eq_z() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0) },
            qir! { rz(PI, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rz_half_pi_eq_s() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { s(0) },
            qir! { rz(PI / 2.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rz_neg_half_pi_eq_s_adj() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { s_adj(0) },
            qir! { rz(-PI / 2.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rz_quarter_pi_eq_t() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { t(0) },
            qir! { rz(PI / 4.0, 0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn rz_neg_quarter_pi_eq_t_adj() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { t_adj(0) },
            qir! { rz(-PI / 4.0, 0) }
        ],
        num_qubits: 1,
    }
}

// ==================== Two-Qubit Rotation Gate Tests ====================

// Rxx gate tests
#[test]
fn rxx_zero_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0); i(1) },
            qir! { rxx(0.0, 0, 1) }
        ],
        num_qubits: 2,
    }
}

#[test]
fn rxx_pi_eq_x_tensor_x() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { x(0); x(1) },
            qir! { rxx(PI, 0, 1) }
        ],
        num_qubits: 2,
    }
}

// Ryy gate tests
#[test]
fn ryy_zero_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0); i(1) },
            qir! { ryy(0.0, 0, 1) }
        ],
        num_qubits: 2,
    }
}

#[test]
fn ryy_pi_eq_y_tensor_y() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { y(0); y(1) },
            qir! { ryy(PI, 0, 1) }
        ],
        num_qubits: 2,
    }
}

// Rzz gate tests
#[test]
fn rzz_zero_eq_identity() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { i(0); i(1) },
            qir! { rzz(0.0, 0, 1) }
        ],
        num_qubits: 2,
    }

    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { within { h(0); h(1) } apply { i(0); i(1) } },
            qir! { within { h(0); h(1) } apply { rzz(0.0, 0, 1) } }
        ],
        num_qubits: 2,
    }
}

#[test]
fn rzz_pi_eq_z_tensor_z() {
    require_gpu!();
    // Z⊗Z on |00⟩ gives |00⟩ (both have eigenvalue +1)
    // This is equivalent to identity on computational basis states
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { z(0); z(1) },
            qir! { rzz(PI, 0, 1) }
        ],
        num_qubits: 2,
    }

    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! { within { h(0); h(1) } apply { z(0); z(1) } },
            qir! { within { h(0); h(1) } apply { rzz(PI, 0, 1) } }
        ],
        num_qubits: 2,
    }
}

// ==================== Reset and Measurement Tests ====================

#[ignore = "unimplemented"]
#[test]
fn reset_takes_qubit_back_to_zero() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            reset(0);  // Resets to 0
            mz(0, 0);  // Measures 0
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn mresetz_resets_after_measurement() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);  // Measures 1, resets to 0
            mresetz(0, 1);  // Measures 0
        },
        num_qubits: 1,
        num_results: 2,
        output: expect![[r#"10"#]],
    }
}

#[ignore = "mz is implemented as mresetz for the GPU"]
#[test]
fn mz_does_not_reset() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            mz(0, 0);  // Measures 1, does not reset
            mz(0, 1);  // Measures 1 again
        },
        num_qubits: 1,
        num_results: 2,
        output: expect![[r#"11"#]],
    }
}

// ==================== MOV Gate Tests ====================

#[test]
fn mov_is_noop_without_noise() {
    require_gpu!();
    check_programs_are_eq! {
        simulator: GpuSimulator,
        programs: [
            qir! {},
            qir! { mov(0) }
        ],
        num_qubits: 1,
    }
}

// ==================== Multi-Qubit State Tests ====================

#[test]
fn bell_state_produces_correlated_measurements() {
    require_gpu!();
    // Bell state produces only correlated outcomes: 00 or 11
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            cx(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 100,
        format: outcomes,
        output: expect![[r#"
            00
            11"#]],
    }
}

#[test]
fn ghz_state_three_qubits() {
    require_gpu!();
    // GHZ state produces only 000 or 111
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            cx(0, 1);
            cx(1, 2);
            mresetz(0, 0);
            mresetz(1, 1);
            mresetz(2, 2);
        },
        num_qubits: 3,
        num_results: 3,
        shots: 100,
        format: outcomes,
        output: expect![[r#"
            000
            111"#]],
    }
}
