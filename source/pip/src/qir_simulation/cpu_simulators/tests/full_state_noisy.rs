// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noisy full-state simulator.
//!
//! The noisy full-state simulator extends the noiseless simulator with
//! configurable Pauli noise and qubit loss. This module verifies that
//! noise is correctly applied to quantum operations.
//!
//! # Supported Gates
//!
//! Same as noiseless full-state simulator (see `full_state_noiseless`).
//!
//! # Noise Model
//!
//! Each gate can have an associated noise configuration:
//!
//! - **Pauli noise**: X (bit-flip), Y (bit+phase flip), Z (phase-flip)
//! - **Loss noise**: Qubit loss producing '-' measurement result
//! - **Two-qubit noise**: Pauli strings like XI, IX, XX, YZ, etc.
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
//! | Multiple gates        | Noise accumulates across gate sequence     |
//! | Gate-specific noise   | Different gates can have different noise   |
//! | Rotation gate noise   | Noise on Rx, Ry, Rz, Rxx, Ryy, Rzz gates   |
//! ```

use super::{super::*, SEED, test_utils::*};
use expect_test::expect;

// ==================== Noiseless Config Tests ====================

#[test]
fn noiseless_config_produces_clean_results() {
    check_sim! {
        simulator: NoisySimulator,
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
    // X noise on X gate: X·X = I, so some results flip back to 0
    check_sim! {
        simulator: NoisySimulator,
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

#[test]
fn x_noise_on_h_gate_affects_superposition() {
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            h(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            h: { x: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    0: 475
                    1: 525"#]],
    }
}

// ==================== Z Noise (Phase-Flip) Tests ====================

#[test]
fn z_noise_does_not_affect_computational_basis() {
    // Z noise should not change measurement outcomes in computational basis
    check_sim! {
        simulator: NoisySimulator,
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

#[test]
fn z_noise_on_superposition_affects_interference() {
    // Z noise on H gate affects phase, changing interference pattern
    // H·Z·H = X, so Z errors in superposition can flip outcomes
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            h(0);
            h(0);  // H·H = I, should give |0⟩ without noise
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            h: { z: 0.2 },
        },
        format: histogram,
        output: expect![[r#"
                    0: 819
                    1: 181"#]],
    }
}

// ==================== Loss Noise Tests ====================

#[test]
fn loss_noise_produces_loss_marker() {
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            x: { loss: 0.1 },
        },
        format: summary,
        output: expect![[r#"
                    shots: 1000
                    unique: 2
                    loss: 119"#]],
    }
}

#[test]
fn loss_appears_in_histogram() {
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            x: { loss: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    -: 119
                    1: 881"#]],
    }
}

// ==================== Two-Qubit Gate Noise Tests ====================

#[test]
fn cx_xi_noise_flips_control_qubit() {
    // XI noise on CX flips the control qubit
    check_sim! {
        simulator: NoisySimulator,
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
            cx: { xi: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    01: 92
                    11: 908"#]],
    }
}

#[test]
fn cx_ix_noise_flips_target_qubit() {
    // IX noise on CX flips the target qubit
    check_sim! {
        simulator: NoisySimulator,
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
            cx: { ix: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    10: 92
                    11: 908"#]],
    }
}

#[test]
fn cx_xx_noise_flips_both_qubits() {
    // XX noise on CX flips both qubits
    check_sim! {
        simulator: NoisySimulator,
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
            cx: { xx: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    00: 92
                    11: 908"#]],
    }
}

#[test]
fn cz_noise_affects_state() {
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            h(0);
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
        format: histogram,
        output: expect![[r#"
                    00: 506
                    10: 494"#]],
    }
}

#[test]
fn swap_noise_affects_swapped_qubits() {
    check_sim! {
        simulator: NoisySimulator,
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

// ==================== Gate-Specific Noise Tests ====================

#[test]
fn different_gates_have_different_noise() {
    // X gate has noise, H gate doesn't - H produces 50/50, X noise flips some
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            h(0);
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            x: { x: 0.2 },
        },
        format: histogram,
        output: expect![[r#"
                    0: 506
                    1: 494"#]],
    }
}

// ==================== Multiple Gates / Accumulated Noise Tests ====================

#[test]
fn noise_accumulates_across_multiple_gates() {
    // Two X gates, each with noise - errors compound
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            x(0);
            x(0);  // X·X = I, so result should be 0 without noise
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
                    0: 818
                    1: 182"#]],
    }
}

#[test]
fn bell_state_with_combined_noise() {
    check_sim! {
        simulator: NoisySimulator,
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
        format: top_n(4),
        output: expect![[r#"
                    00: 491
                    11: 481
                    01: 18
                    10: 10"#]],
    }
}

// ==================== Rotation Gate Noise Tests ====================

#[test]
fn rx_gate_with_noise() {
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            rx(std::f64::consts::PI, 0);  // Rx(π) ~ X
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            rx: { x: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    0: 97
                    1: 903"#]],
    }
}

#[test]
fn rz_gate_with_z_noise_no_effect_on_basis() {
    // Rz followed by Z noise - no effect on computational basis
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            rz(std::f64::consts::PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        noise: noise_config! {
            rz: { z: 0.5 },
        },
        format: histogram,
        output: expect![[r#"0: 100"#]],
    }
}

// ==================== Multi-Qubit Rotation Gate Noise Tests ====================

#[test]
fn rxx_gate_with_noise() {
    check_sim! {
        simulator: NoisySimulator,
        program: qir! {
            rxx(std::f64::consts::PI, 0, 1);  // Rxx(π) ~ X⊗X
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            rxx: { xi: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
                    01: 89
                    11: 911"#]],
    }
}
