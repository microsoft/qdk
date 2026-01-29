// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noiseless Clifford/stabilizer simulator.
//!
//! The stabilizer simulator efficiently simulates quantum circuits composed
//! of Clifford gates using the stabilizer formalism.
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
//! | X       | X flips qubit, X X ~ I                            |
//! | Y       | Y flips qubit, Y Y ~ I                            |
//! | Z       | Z|0⟩ = |0⟩, H Z H ~ X                             |
//! | H       | H^2 ~ I, creates superposition                    |
//! | S       | S^2 ~ Z, S S_ADJ ~ I                              |
//! | S_ADJ   | S_ADJ^2 ~ Z                                       |
//! | SX      | SX^2 ~ X, SX SX_ADJ ~ I                           |
//! | SX_ADJ  | SX_ADJ^2 ~ X                                      |
//! | CX      | CX|00⟩ = |00⟩, CX|10⟩ = |11⟩                      |
//! | CZ      | CZ|x0⟩ = |x0⟩, CZ(a,b) = CZ(b,a)                  |
//! | SWAP    | Exchanges qubit states, SWAP SWAP ~ I             |
//! | RESET   | Returns qubit to |0⟩                              |
//! | MRESETZ | Measures and resets to |0⟩                        |
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

// ==================== Single-Qubit Gate Tests ====================

// I gate tests
#[test]
fn i_gate_does_nothing() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! {},
            qir! { i(0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// X gate tests
#[test]
fn x_gate_flips_qubit() {
    check_sim! {
        simulator: StabilizerSimulator,
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
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); x(0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// Y gate tests
#[test]
fn y_gate_flips_qubit() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            y(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn double_y_gate_eq_identity() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { y(0); y(0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// Z gate tests
#[test]
fn z_gate_preserves_zero() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            z(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn z_gate_preserves_one() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            z(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn h_z_h_eq_x() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { x(0) },
            qir! { within { h(0) } apply { z(0) } }
        ],
        num_qubits: 1,
        num_results: 0,
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
fn h_squared_eq_identity() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { h(0); h(0) }
        ],
        num_qubits: 1,
        num_results: 0,
    }
}

// S gate tests
#[test]
fn s_gate_preserves_computational_basis() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            s(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn s_squared_eq_z() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { z(0) },
            qir! { s(0); s(0) }
        ],
        num_qubits: 1,
        num_results: 0,
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
        num_results: 0,
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
        num_results: 0,
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
        num_results: 0,
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
        num_results: 0,
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
        num_results: 0,
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
            qir! { cz(0, 1) },
            qir! { cz(1, 0) }
        ],
        num_qubits: 2,
        num_results: 0,
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
        num_results: 0,
    }
}

// ==================== Reset and Measurement Tests ====================

#[test]
fn reset_returns_qubit_to_zero() {
    check_programs_are_eq! {
        simulator: StabilizerSimulator,
        programs: [
            qir! { i(0) },
            qir! { x(0); reset(0) }
        ],
        num_qubits: 1,
        num_results: 0,
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
        num_results: 0,
    }
}
