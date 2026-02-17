// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the noisy GPU full-state simulator.
//!
//! The GPU full-state simulator supports noisy simulation with Pauli noise
//! and qubit loss, executed via WebGPU compute shaders.
//!
//! # Notes
//!
//! - All tests require a compatible GPU adapter; they are skipped otherwise.
//! - The GPU simulator uses f32 precision for gate operations and has its own
//!   RNG implementation, so exact shot counts will differ from the CPU simulator.
//! - The GPU simulator applies noise using the same `NoiseConfig` format as the
//!   CPU simulator, converted to f32 precision internally.
//!
//! # Noise Model
//!
//! Each gate can have an associated noise configuration:
//!
//! - **Pauli noise**: X (bit-flip), Y (bit+phase flip), Z (phase-flip)
//! - **Loss noise**: Qubit loss producing '-' measurement result
//! - **Two-qubit noise**: Pauli strings like XI, IX, XX, YZ, etc.
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
//! | Gate-specific noise   | Different gates can have different noise   |
//! | Rotation gate noise   | Noise on Rx, Ry, Rz, Rxx, Ryy, Rzz gates   |
//! ```

use super::{SEED, test_utils::*};
use expect_test::expect;

// ==================== Noiseless Config Tests ====================

#[test]
fn noiseless_config_produces_clean_results() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        noise: noise_config! {},
        format: histogram,
        output: expect![[r#"1: 100"#]],
    }
}

// ==================== X Noise (Bit-Flip) Tests ====================

#[test]
fn x_noise_on_x_gate_causes_bit_flips() {
    require_gpu!();
    // X noise on X gate: X·X = I, so some results flip back to 0
    check_sim! {
        simulator: GpuSimulator,
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
            0: 96
            1: 904"#]],
    }
}

#[test]
fn x_noise_on_h_gate_does_not_affect_outcome() {
    require_gpu!();
    // H already creates superposition; X noise doesn't change the distribution
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            h: { x: 0.3 },
        },
        format: histogram,
        output: expect![[r#"
            0: 495
            1: 505"#]],
    }
}

// ==================== Z Noise (Phase-Flip) Tests ====================

#[test]
fn z_noise_does_not_affect_computational_basis() {
    require_gpu!();
    // Z noise should not change measurement outcomes in computational basis
    check_sim! {
        simulator: GpuSimulator,
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
    require_gpu!();
    // Z noise on H gate affects phase, changing interference pattern
    // H·H = I without noise → always |0⟩
    // Z noise during H causes some flips due to broken interference
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            h(0);
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
            0: 810
            1: 190"#]],
    }
}

// ==================== Loss Noise Tests ====================

#[test]
fn loss_noise_produces_loss_marker() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
            -: 90
            1: 910"#]],
    }
}

// ==================== Two-Qubit Gate Noise Tests ====================

#[test]
fn cx_xi_noise_flips_control_qubit() {
    require_gpu!();
    // XI noise on CX flips the control qubit
    check_sim! {
        simulator: GpuSimulator,
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
            01: 108
            11: 892"#]],
    }
}

#[test]
fn cx_ix_noise_flips_target_qubit() {
    require_gpu!();
    // IX noise on CX flips the target qubit
    check_sim! {
        simulator: GpuSimulator,
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
            10: 108
            11: 892"#]],
    }
}

#[test]
fn cx_xx_noise_flips_both_qubits() {
    require_gpu!();
    // XX noise on CX flips both qubits
    check_sim! {
        simulator: GpuSimulator,
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
            00: 108
            11: 892"#]],
    }
}

#[test]
fn cz_noise_affects_outcome() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
        format: histogram,
        output: expect![[r#"
            00: 904
            10: 96"#]],
    }
}

#[test]
fn swap_noise_affects_swapped_qubits() {
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
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            swap: { ix: 0.1 },
        },
        format: histogram,
        output: expect![[r#"
            00: 108
            01: 892"#]],
    }
}

#[test]
fn two_qubit_loss() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
            --: 12
            -0: 87
            0-: 84
            00: 817"#]],
    }
}

// ==================== Gate-Specific Noise Tests ====================

#[test]
fn different_gates_have_different_noise() {
    require_gpu!();
    // Z gate has noise, X gate doesn't
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            z(0);
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 1000,
        seed: SEED,
        noise: noise_config! {
            z: { x: 0.2 },
        },
        format: histogram,
        output: expect![[r#"
            0: 190
            1: 810"#]],
    }
}

// ==================== Multiple Gates / Accumulated Noise Tests ====================

#[test]
fn noise_accumulates_across_multiple_gates() {
    require_gpu!();
    // Two X gates, each with noise - errors compound
    // X·X = I without noise, so clean result is 0
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            x(0);  // X·X = I, so result should be 0 without noise
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 10_000,
        seed: SEED,
        noise: noise_config! {
            x: { x: 0.1 },
        },
        format: histogram_percent,
        output: expect![[r#"
            0: 82.14%
            1: 17.86%"#]],
    }
}

#[test]
fn bell_state_with_combined_noise() {
    require_gpu!();
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
        shots: 10_000,
        seed: SEED,
        noise: noise_config! {
            h: { loss: 0.1 },
            cx: { xi: 0.02, ix: 0.02 },
        },
        format: histogram_percent,
        output: expect![[r#"
            -0: 9.85%
            00: 42.61%
            01: 1.76%
            10: 1.91%
            11: 43.87%"#]],
    }
}

// ==================== Rotation Gate Noise Tests ====================

#[test]
fn rx_gate_with_noise() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
            0: 96
            1: 904"#]],
    }
}

#[test]
fn rz_gate_with_z_noise_no_effect_on_basis() {
    require_gpu!();
    // Rz followed by Z noise - no effect on computational basis
    check_sim! {
        simulator: GpuSimulator,
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
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
            01: 96
            11: 904"#]],
    }
}

// ==================== Correlated Noise Intrinsic Tests ====================

#[test]
fn noise_intrinsic_single_qubit_x_noise() {
    require_gpu!();
    // Single-qubit X noise via intrinsic
    check_sim! {
        simulator: GpuSimulator,
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
            0: 887
            1: 113"#]],
    }
}

#[test]
fn noise_intrinsic_single_qubit_z_noise_no_effect() {
    require_gpu!();
    // Z noise on |0⟩ has no observable effect
    check_sim! {
        simulator: GpuSimulator,
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
    require_gpu!();
    // Two-qubit XX noise causes correlated bit flips
    check_sim! {
        simulator: GpuSimulator,
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
            00: 887
            11: 113"#]],
    }
}

#[test]
fn noise_intrinsic_two_qubit_independent_noise() {
    require_gpu!();
    // XI and IX noise cause independent flips on each qubit
    check_sim! {
        simulator: GpuSimulator,
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
            01: 104
            10: 113"#]],
    }
}

#[test]
fn noise_intrinsic_multiple_ids() {
    require_gpu!();
    // Multiple intrinsic IDs with different noise configurations
    check_sim! {
        simulator: GpuSimulator,
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
                0: { x: 0.1 },
                1: { x: 0.5 },
            },
        },
        format: histogram,
        output: expect![[r#"
            00: 455
            01: 432
            10: 58
            11: 55"#]],
    }
}

#[test]
fn noise_intrinsic_three_qubit_correlated() {
    require_gpu!();
    // Three-qubit correlated noise (XXX flips all three)
    check_sim! {
        simulator: GpuSimulator,
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
            000: 887
            111: 113"#]],
    }
}

#[test]
fn noise_intrinsic_combined_with_gate_noise() {
    require_gpu!();
    // Intrinsic noise combined with regular gate noise
    check_sim! {
        simulator: GpuSimulator,
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
            0: 181
            1: 819"#]],
    }
}
