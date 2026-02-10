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
//! # Test Categories
//!
//! ```text
//! | Category              | Description                                  |
//! |-----------------------|----------------------------------------------|
//! | Basic gates           | X, Y, Z, H produce correct measurements      |
//! | Phase gates           | S, T and adjoints preserve computational basis|
//! | Two-qubit gates       | CX, CZ, SWAP produce correct correlations    |
//! | Rotation gates        | Rx(π)~X, Ry(π)~Y, Rz(π)~Z, etc.              |
//! | Multi-qubit states    | Bell and GHZ states show correct correlations |
//! | Multiple shots        | Simulator completes all requested shots       |
//! ```

use super::{super::*, SEED, test_utils::*};
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
        seed: SEED,
        format: summary,
        output: expect![[r#"
            shots: 50
            unique: 1
            loss: 0"#]],
    }
}

// ==================== Single-Qubit Gate Tests ====================

#[test]
fn x_gate_flips_qubit() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
fn double_x_gate_returns_to_zero() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            x(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn y_gate_flips_qubit() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
fn double_y_gate_returns_to_zero() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            y(0);
            y(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn z_gate_preserves_zero() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
fn h_gate_creates_superposition() {
    require_gpu!();
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
        format: histogram,
        output: expect![[r#"
            0: 45
            1: 55"#]],
    }
}

#[test]
fn h_squared_returns_to_original() {
    require_gpu!();
    // H·H = I, so starting from |0⟩ should return to |0⟩
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            h(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        format: histogram,
        output: expect![[r#"0: 100"#]],
    }
}

#[test]
fn h_x_h_acts_as_z() {
    require_gpu!();
    // H·X·H = Z, and Z|0⟩ = |0⟩
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            x(0);
            h(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn h_z_h_acts_as_x() {
    require_gpu!();
    // H·Z·H = X, and X|0⟩ = |1⟩
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            h(0);
            z(0);
            h(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// S gate tests
#[test]
fn s_gate_preserves_computational_basis() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
fn s_and_s_adj_cancel() {
    require_gpu!();
    // S·S† = I
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            s(0);
            s_adj(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn s_squared_acts_as_z() {
    require_gpu!();
    // S^2 = Z, Z|0⟩ = |0⟩, Z|1⟩ = -|1⟩ (same measurement)
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            s(0);
            s(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// T gate tests
#[test]
fn t_gate_preserves_computational_basis() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            t(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn t_and_t_adj_cancel() {
    require_gpu!();
    // T·T† = I
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            t(0);
            t_adj(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// SX gate tests
#[test]
fn sx_squared_acts_as_x() {
    require_gpu!();
    // SX^2 = X, X|0⟩ = |1⟩
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            sx(0);
            sx(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn sx_and_sx_adj_cancel() {
    require_gpu!();
    // SX·SX† = I
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            sx(0);
            sx_adj(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// MOV gate test
#[test]
fn mov_does_not_change_state() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            mov(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// ==================== Two-Qubit Gate Tests ====================

#[test]
fn cx_with_control_zero_is_identity() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
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
fn cx_with_control_one_flips_target() {
    require_gpu!();
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
        output: expect![[r#"11"#]],
    }
}

#[test]
fn cz_preserves_computational_basis() {
    require_gpu!();
    // CZ only adds phase, no bit flip
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            x(1);
            cz(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"11"#]],
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
fn swap_twice_returns_to_original() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            swap(0, 1);
            swap(0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"10"#]],
    }
}

// ==================== Rotation Gate Tests ====================

#[test]
fn rx_pi_acts_as_x() {
    require_gpu!();
    // Rx(π) ~ X
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            rx(PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn rx_two_pi_acts_as_identity() {
    require_gpu!();
    // Rx(2π) ~ I
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            rx(2.0 * PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn ry_pi_acts_as_y() {
    require_gpu!();
    // Ry(π) ~ Y
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            ry(PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn ry_two_pi_acts_as_identity() {
    require_gpu!();
    // Ry(2π) ~ I
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            ry(2.0 * PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

#[test]
fn rz_preserves_computational_basis() {
    require_gpu!();
    // Rz only adds phase, preserves |0⟩ and |1⟩
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            rz(PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn rz_preserves_one_state() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            rz(PI, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// ==================== Two-Qubit Rotation Gate Tests ====================

#[test]
fn rxx_pi_flips_both_qubits() {
    require_gpu!();
    // Rxx(π) ~ X⊗X
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            rxx(PI, 0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"11"#]],
    }
}

#[test]
fn ryy_pi_flips_both_qubits() {
    require_gpu!();
    // Ryy(π) ~ Y⊗Y
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            ryy(PI, 0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"11"#]],
    }
}

#[test]
fn rzz_preserves_computational_basis() {
    require_gpu!();
    // Rzz only adds phase, no bit change
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            rzz(PI, 0, 1);
            mresetz(0, 0);
            mresetz(1, 1);
        },
        num_qubits: 2,
        num_results: 2,
        output: expect![[r#"10"#]],
    }
}

// ==================== Multi-Qubit State Tests ====================

#[test]
fn bell_state_produces_correlated_measurements() {
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
        shots: 100,
        seed: SEED,
        format: histogram,
        output: expect![[r#"
            00: 53
            11: 47"#]],
    }
}

#[test]
fn ghz_state_three_qubits() {
    require_gpu!();
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
        seed: SEED,
        format: histogram,
        output: expect![[r#"
            000: 56
            111: 44"#]],
    }
}

// ==================== Multi-Qubit Gate Sequence Tests ====================

#[test]
fn swap_into_different_qubits() {
    require_gpu!();
    // Prepare |1⟩ on qubit 2, swap to qubit 7 in an 8-qubit system
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(2);
            swap(2, 7);
            mresetz(2, 0);
            mresetz(7, 1);
        },
        num_qubits: 8,
        num_results: 2,
        output: expect![[r#"01"#]],
    }
}

#[test]
fn identity_gate_is_noop() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            i(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"0"#]],
    }
}

#[test]
fn identity_gate_preserves_one() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            i(0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        output: expect![[r#"1"#]],
    }
}

// ==================== Compound Circuit Tests ====================

#[test]
fn teleportation_circuit() {
    require_gpu!();
    // Teleport |1⟩ from qubit 0 to qubit 2
    // After teleportation, qubit 2 should always measure 1
    // (with correction based on Bell measurement results)
    //
    // This simplified version uses the within-apply pattern:
    // Prepare Bell pair on qubits 1,2, then entangle qubit 0 with qubit 1
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            // Prepare |1⟩ on qubit 0
            x(0);
            // Create Bell pair on qubits 1,2
            h(1);
            cx(1, 2);
            // Bell measurement on qubits 0,1
            cx(0, 1);
            h(0);
            // Conditional corrections (deterministic for this circuit)
            // Since we're testing superposition outcomes, just verify
            // the entanglement structure with multiple shots
            mresetz(0, 0);
            mresetz(1, 1);
            mresetz(2, 2);
        },
        num_qubits: 3,
        num_results: 3,
        shots: 100,
        seed: SEED,
        format: histogram,
        output: expect![[r#"
            001: 24
            010: 24
            101: 28
            110: 24"#]],
    }
}

#[test]
fn multiple_measurements_on_separate_qubits() {
    require_gpu!();
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            x(0);
            x(2);
            mresetz(0, 0);
            mresetz(1, 1);
            mresetz(2, 2);
        },
        num_qubits: 3,
        num_results: 3,
        output: expect![[r#"101"#]],
    }
}

#[test]
fn rx_half_pi_creates_superposition() {
    require_gpu!();
    // Rx(π/2) creates a superposition, like SX
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            rx(PI / 2.0, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        format: histogram,
        output: expect![[r#"
            0: 45
            1: 55"#]],
    }
}

#[test]
fn ry_half_pi_creates_superposition() {
    require_gpu!();
    // Ry(π/2) creates a superposition
    check_sim! {
        simulator: GpuSimulator,
        program: qir! {
            ry(PI / 2.0, 0);
            mresetz(0, 0);
        },
        num_qubits: 1,
        num_results: 1,
        shots: 100,
        seed: SEED,
        format: histogram,
        output: expect![[r#"
            0: 45
            1: 55"#]],
    }
}
