// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noiseless Clifford/stabilizer simulator.
//!
//! The stabilizer simulator efficiently simulates quantum circuits composed
//! of Clifford gates using the stabilizer formalism.
//!
//! # Equivalence
//!
//! The `~` symbol means: for every computational basis state |b⟩, the two
//! programs produce the same output state up to a global phase (which may
//! differ per basis state). This is verified by `check_programs_are_eq!`.
//!
//! Gate truth tables on all basis states are tested separately using
//! `check_basis_table!`.
//!
//! # Supported Gates
//!
//! ```text
//! | Category        | Gates                                      |
//! |-----------------|--------------------------------------------|
//! | Single-qubit    | I, X, Y, Z, H, S, S_ADJ, SX, SX_ADJ        |
//! | Two-qubit       | CX, CZ, SWAP                               |
//! | Measurement     | MZ, MRESETZ, RESET                         |
//! | Other           | MOV                                        |
//! ```
//!
//! # Not Supported (Panics)
//!
//! `T`, `T_ADJ`, `Rx`, `Ry`, `Rz`, `Rxx`, `Ryy`, `Rzz` (non-Clifford gates)
//!
//! # Gate Properties
//!
//! The `~` symbol denotes equivalence up to global phase.
//!
//! ```text
//! | Gate    | Properties                                        |
//! |---------|---------------------------------------------------|
//! | I       | I ~ {} (identity does nothing)                    |
//! | X       | X flips qubit, X X ~ I, X ~ H Z H                 |
//! | Y       | Y flips qubit, Y Y ~ I, Y ~ X Z ~ Z X             |
//! | Z       | Z|0⟩ = |0⟩, H Z H ~ X                             |
//! | H       | H^2 ~ I, H X H ~ Z, creates superposition         |
//! | S       | S^2 ~ Z, S S_ADJ ~ I                              |
//! | S_ADJ   | S_ADJ^2 ~ Z                                       |
//! | SX      | SX^2 ~ X, SX SX_ADJ ~ I                           |
//! | SX_ADJ  | SX_ADJ^2 ~ X                                      |
//! | CX      | CX|00⟩ = |00⟩, CX|10⟩ = |11⟩                      |
//! | CZ      | CZ|x0⟩ = |x0⟩, CZ(a,b) = CZ(b,a)                  |
//! | SWAP    | Exchanges states, SWAP^2 ~ I                      |
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

use super::{super::*, SEED, test_utils::*};
use expect_test::expect;

// ==================== Generic Simulator Tests ====================

#[test]
fn simulator_completes_all_shots() {
    check_sim! {
        simulator: StabilizerSimulator,
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

// Note: Gate truth tables (check_basis_table!) are not used here because the
// stabilizer simulator's state_dump() returns a CliffordUnitary (the operator),
// not a state vector. Comparing operators is too strict for basis-state tests
// (e.g., Y|0⟩ ~ |1⟩ as states, but Y ≠ X as operators).
//
// Instead, gate behavior on specific basis states is verified via check_sim!
// tests below (cx_on_zero_control_eq_identity, cx_on_one_control_flips_target,
// etc.), and algebraic identities are verified via check_programs_are_eq! which
// correctly checks unitary equivalence on all basis states.

#[test]
fn single_qubit_gate_truth_tables() {
    check_basis_table! {
        simulator: StabilizerSimulator,
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
        ],
    }
}

#[test]
fn two_qubit_gate_truth_tables() {
    check_basis_table! {
        simulator: StabilizerSimulator,
        num_qubits: 2,
        table: [
            // CX(control=q0, target=q1): flips q1 when q0=|1⟩
            (qir! { cx(0, 1) }, 0b00 => 0b00),
            (qir! { cx(0, 1) }, 0b01 => 0b11),  // q0=1 → flip q1
            (qir! { cx(0, 1) }, 0b10 => 0b10),  // q0=0 → identity
            (qir! { cx(0, 1) }, 0b11 => 0b01),  // q0=1 → flip q1
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); x(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn x_eq_h_z_h() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { y(0); y(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn y_gate_eq_x_z_and_z_x() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { within { h(0) } apply { z(0); z(0) } }
        ],
        num_qubits: 1,
    }
}

#[test]
fn z_eq_h_x_h() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    // H creates equal superposition - should see both 0 and 1
    check_sim! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { z(0) },
            qir! { s(0); s(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn s_and_s_adj_cancel() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { x(0) },
            qir! { sx(0); sx(0) }
        ],
        num_qubits: 1,
    }
}

#[test]
fn sx_and_sx_adj_cancel() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { x(0) },
            qir! { sx_adj(0); sx_adj(0) }
        ],
        num_qubits: 1,
    }
}

// ==================== Two-Qubit Gate Tests ====================

// CX gate tests
#[test]
fn cx_on_zero_control_eq_identity() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            cx(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"00"#]],
    }
}

#[test]
fn cx_on_one_control_flips_target() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            cx(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"11"#]],
    }
}

// CZ gate tests
#[test]
fn cz_on_zero_control_eq_identity() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            cz(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"00"#]],
    }
}

#[test]
fn cz_applies_phase_when_control_is_one() {
    // CZ applies Z to target when control is |1⟩
    // H·Z·H = X, so if we conjugate target by H, we see the flip
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);           // Set control to |1⟩
            within { h(1) } apply { cz(0, 1) }
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"11"#]],
    }
}

#[test]
fn cz_symmetric() {
    // CZ is symmetric: CZ(a,b) = CZ(b,a)
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { within { x(0); h(1) } apply { cz(0, 1) } },
            qir! { within { x(0); h(1) } apply { cz(1, 0) } }
        ],
        num_qubits: 2,
    }
}

// SWAP gate tests
#[test]
fn swap_exchanges_qubit_states() {
    check_sim! {
        simulator: StabilizerSimulator,
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
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { x(0) },
            qir! { x(0); swap(0, 1); swap(0, 1) }
        ],
        num_qubits: 2,
    }
}

// ==================== Reset and Measurement Tests ====================

#[test]
fn reset_takes_qubit_back_to_zero() {
    check_sim! {
        simulator: StabilizerSimulator,
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
    check_sim! {
        simulator: StabilizerSimulator,
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

#[test]
fn mz_does_not_reset() {
    check_sim! {
        simulator: StabilizerSimulator,
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

#[test]
fn mz_is_idempotent() {
    // M M ~ M (repeated measurement gives same result)
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { x(0); mz(0, 0) },
            qir! { x(0); mz(0, 0); mz(0, 1) }
        ],
        num_qubits: 1,
        num_results: 2,
    }
}

// ==================== MOV Gate Tests ====================

#[test]
fn mov_is_noop_without_noise() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
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
    // Bell state produces only correlated outcomes: 00 or 11
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            h(0);
            cx(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 100,
        seed: SEED,
        format: outcomes,
        output: expect![[r#"
                    00
                    11"#]],
    }
}

#[test]
fn ghz_state_three_qubits() {
    // GHZ state produces only 000 or 111
    check_sim! {
        simulator: StabilizerSimulator,
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
        seed: SEED,
        format: outcomes,
        output: expect![[r#"
                    000
                    111"#]],
    }
}
