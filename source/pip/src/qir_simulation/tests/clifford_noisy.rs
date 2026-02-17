// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noisy Clifford/stabilizer simulator.
//!
//! The stabilizer simulator supports noisy simulation with Pauli noise
//! and qubit loss, efficiently tracking errors in the stabilizer formalism.
//!
//! # Supported Gates
//!
//! Same as noiseless stabilizer simulator (see `clifford_noiseless`).
//!
//! # Noise Model
//!
//! Same as noisy full-state simulator (see `full_state_noisy`):
//!
//! - **Pauli noise**: X (bit-flip), Y (bit+phase flip), Z (phase-flip)
//! - **Loss noise**: Qubit loss producing '-' measurement result
//! - **Two-qubit noise**: Pauli strings like XI, IX, XX, etc.
//!
//! # Notes
//!
//! - The I gate is a no-op, so noise on I gate is not applied
//! - MRESETZ noise is applied before measurement, not after
//!
//! # Test Categories
//!
//! ```text
//! | Category              | Description                                |
//! |-----------------------|--------------------------------------------|
//! | Noiseless config      | Empty noise config produces clean results  |
//! | X noise (bit-flip)    | Flips measurement outcomes                 |
//! | Z noise (phase-flip)  | No effect on computational basis           |
//! | Loss noise            | Produces '-' marker in measurements        |
//! | Two-qubit gate noise  | XI, IX, XX, etc. affect respective qubits  |
//! | Combined noise        | Multiple noise sources on entangled states |
//! ```

use super::{SEED, test_utils::*};
use expect_test::expect;
use qdk_simulators::stabilizer_simulator::StabilizerSimulator;

// ==================== Noiseless Config Tests ====================

#[test]
fn noiseless_config_produces_clean_results() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        noise: noise_config! {},
        format: histogram,
        output: expect![[r#"1: 100"#]],
    }
}

// ==================== X Noise (Bit-Flip) Tests ====================

#[test]
fn x_noise_on_x_gate_causes_bit_flips() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            x: { x: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    0: 97
                    1: 903"#]],
    }
}

// ==================== Z Noise (Phase-Flip) Tests ====================

#[test]
fn z_noise_does_not_affect_computational_basis() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        noise: noise_config! {
            x: { z: 0.5 },
        },
        format: histogram,
        output: expect![[r#"1: 100"#]],
    }
}

// ==================== Loss Noise Tests ====================

#[test]
fn loss_noise_produces_loss_marker() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        noise: noise_config! {
            x: { loss: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
            -: 5
            1: 95"#]],
    }
}

#[test]
fn max_loss_probability_always_results_in_loss() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        noise: noise_config! {
            x: { loss: 1.0 },
        },
        format: histogram,
        output: expect!["-: 100"],
    }
}

// ==================== Two-Qubit Gate Noise Tests ====================

#[test]
fn cx_noise_affects_entangled_qubits() {
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
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            cx: {
                xi: 0.05,
                ix: 0.05,
            },
        },
        format: histogram,
        output: expect![[r#"
            01: 36
            10: 56
            11: 908"#]],
    }
}

#[test]
fn cz_noise_affects_state() {
    // CZ with noise introduces errors
    // Should only see 00 in a noiseless simulation,
    // but because of noisy we should also see 10 now.
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            cz(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            cz: { xi: 0.1 },
        },
        format: outcomes,
        output: expect![[r#"
                    00
                    10"#]],
    }
}

#[test]
fn swap_noise_affects_swapped_qubits() {
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
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            swap: { xi: 0.1, ix: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    00: 103
                    01: 805
                    11: 92"#]],
    }
}

#[test]
fn two_qubit_loss() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            cz(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            cz: { loss: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
            --: 107
            00: 893"#]],
    }
}

// ==================== Combined Noise Tests ====================

#[test]
fn bell_state_with_combined_noise() {
    // Bell state with noise - should see all 4 computational basis states
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
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            h: { x: 0.02 },
            cx: { xi: 0.02, ix: 0.02 },
        },
        format: outcomes,
        output: expect![[r#"
                    00
                    01
                    10
                    11"#]],
    }
}

// ==================== MOV Gate Noise Tests ====================

#[test]
fn mov_with_loss_noise() {
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            mov(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            mov: { loss: 0.1 },
        },
        format: summary,
        output: expect![[r#"
                    shots: 1000
                    unique: 2
                    loss: 97"#]],
    }
}

// ==================== Correlated Noise Intrinsic Tests ====================

#[test]
fn noise_intrinsic_single_qubit_x_noise() {
    // Single-qubit X noise via intrinsic
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            noise_intrinsic(0, &[0]);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            intrinsics: {
                0: { x: 0.1 },
            },
        },
        format: histogram,
        output: expect![[r#"
            0: 886
            1: 114"#]],
    }
}

#[test]
fn noise_intrinsic_single_qubit_z_noise_no_effect() {
    // Z noise on |0⟩ has no observable effect
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            noise_intrinsic(0, &[0]);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        noise: noise_config! {
            intrinsics: {
                0: { z: 0.5 },
            },
        },
        format: histogram,
        output: expect![[r#"0: 100"#]],
    }
}

#[test]
fn noise_intrinsic_two_qubit_correlated_xx_noise() {
    // Two-qubit XX noise causes correlated bit flips
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            noise_intrinsic(0, &[0, 1]);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            intrinsics: {
                0: { xx: 0.1 },
            },
        },
        format: histogram,
        output: expect![[r#"
            00: 886
            11: 114"#]],
    }
}

#[test]
fn noise_intrinsic_two_qubit_independent_noise() {
    // XI and IX noise cause independent flips on each qubit
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            noise_intrinsic(0, &[0, 1]);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            intrinsics: {
                0: { xi: 0.1, ix: 0.1 },
            },
        },
        format: histogram,
        output: expect![[r#"
            00: 783
            01: 103
            10: 114"#]],
    }
}

#[test]
fn noise_intrinsic_multiple_ids() {
    // Multiple intrinsic IDs with different noise configurations
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            noise_intrinsic(0, &[0]);
            noise_intrinsic(1, &[1]);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            intrinsics: {
                0: { x: 0.2 },
                1: { x: 0.1 },
            },
        },
        format: histogram,
        output: expect![[r#"
            00: 702
            01: 81
            10: 191
            11: 26"#]],
    }
}

#[test]
fn noise_intrinsic_three_qubit_correlated() {
    // Three-qubit correlated noise (XXX flips all three)
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            noise_intrinsic(0, &[0, 1, 2]);
            mresetz(0, 0);
            mresetz(1, 1);
            mresetz(2, 2);
        },
        num_qubits: 3,
        num_results: 3,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            intrinsics: {
                0: { xxx: 0.1 },
            },
        },
        format: histogram,
        output: expect![[r#"
            000: 886
            111: 114"#]],
    }
}

#[test]
fn noise_intrinsic_combined_with_gate_noise() {
    // Intrinsic noise combined with regular gate noise
    check_sim! {
        simulator: StabilizerSimulator,
        program: qir! {
            x(0);
            noise_intrinsic(0, &[0]);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            x: { x: 0.1 },
            intrinsics: {
                0: { x: 0.1 },
            },
        },
        format: histogram,
        output: expect![[r#"
            0: 178
            1: 822"#]],
    }
}
