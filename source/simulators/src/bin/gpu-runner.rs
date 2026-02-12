// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Used for local testing of the GPU runner (e.g. profiling in Xcode)

// Run with: cargo run --bin gpu-runner [--release]
// Build with: cargo build --bin gpu-runner [--release]

use core::panic;
use qdk_simulators::gpu_context::{GpuContext, RunResults};
use qdk_simulators::noise_config::{NoiseConfig, encode_pauli};
use qdk_simulators::run_shots_sync;
use qdk_simulators::shader_types::{Op, ops};
use regex_lite::Regex;
use std::f32::consts::PI;
use std::time::Instant;

const DEFAULT_SEED: u32 = 0xfeed_face;

fn main() {
    repeated_noise();
    correlated_noise();
    two_measurements();
    just_pauli();
    simple_bell_pair();
    bell_at_scale();
    scale_teleport();
    test_pauli_noise();
    test_simple_rotation_and_entanglement();
    test_2q_pauli_noise();
    test_move_noise();
    test_benzene();
    test_cx_various_state();
    test_cy_phase();
    test_cy_noise_inverts_phase();
    test_mz_idempotent();
    test_reset_preserves_distribution();
    gates_on_lost_qubits();
    scaled_ising();
    scaled_grover();
    noise_config();
}

fn check_success(results: &RunResults) {
    if !results.success {
        let diag = results
            .diagnostics
            .as_ref()
            .expect("GPU run failed without diagnostics");
        panic!(
            "GPU run failed with error codes: {:?}.
            First failure at shot {}, op index {}, op type {}.
            Attach a debugger to see full diagnostics structure.",
            results.shot_result_codes, diag.shot.shot_id, diag.shot.op_idx, diag.shot.op_type
        );
    }
}

fn assert_ratio(results: &[Vec<u32>], expected: &[u32], expected_ratio: f64, tolerance: f64) {
    let actual_count = results.iter().filter(|x| *x == expected).count();
    #[allow(clippy::cast_precision_loss)]
    let actual_ratio = actual_count as f64 / results.len() as f64;
    assert!(
        (expected_ratio - tolerance..=expected_ratio + tolerance).contains(&actual_ratio),
        "Expected ratio {expected_ratio:.4}, got {actual_ratio:.4} with tolerance {tolerance:.4}"
    );
}

fn two_measurements() {
    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_loss_noise(0, 0.333),
        // Should be 33% chance of lost, 66% chance of 1

        // If not using the noise model processing, need to turn pauli on measurement into Id with noise then mesurement
        Op::new_id_gate(0),
        Op::new_pauli_noise_1q(0, 0.5, 0.0, 0.0),
        Op::new_mresetz_gate(0, 0),
        // Measurement will be 50/50 if qubit is not lost, so now 33% chance each of loss, 1, or 2
        Op::new_mresetz_gate(0, 1),
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(1, 2, &ops, &None, 300, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    // Get a vector of only the first measurement results
    let result_0: Vec<Vec<u32>> = results.shot_results.iter().map(|r| vec![r[0]]).collect();
    assert_ratio(&result_0, &[0], 0.333, 0.1);
    assert_ratio(&result_0, &[1], 0.333, 0.1);
    assert_ratio(&result_0, &[2], 0.333, 0.1);

    // All of the 2nd measurements should be 0
    assert!(
        results.shot_results.iter().all(|r| r[1] == 0),
        "All second measurements should be 0, but got {results:?}"
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn just_pauli() {
    let ops: Vec<Op> = vec![
        Op::new_id_gate(0),
        // 50% bit and phase flip chance
        Op::new_pauli_noise_1q(0, 0.0, 0.5, 0.0),
        Op::new_mresetz_gate(0, 0),
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(1, 1, &ops, &None, 100, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    // Verify the results are 50/50
    assert_ratio(&results.shot_results, &[0], 0.5, 0.1);
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn simple_bell_pair() {
    let ops: Vec<Op> = vec![
        Op::new_h_gate(9),           // 5, 9
        Op::new_cx_gate(9, 11),      // 15, 9, 11
        Op::new_mresetz_gate(9, 0),  // 22, 9, 0
        Op::new_mresetz_gate(11, 1), // 22, 11, 1
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(12, 2, &ops, &None, 100, DEFAULT_SEED, 0).expect("GPU shots failed");
    //let results = run_parallel_shots(12, 2, ops, 100, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    assert_ratio(&results.shot_results, &[0, 0], 0.5, 0.1);
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_pauli_noise() {
    let x_noise: f32 = 0.5;

    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_pauli_noise_1q(0, x_noise, 0.0, 0.0),
        Op::new_mresetz_gate(0, 0),
        Op::new_x_gate(1),
        Op::new_pauli_noise_1q(1, x_noise, 0.0, 0.0),
        Op::new_mresetz_gate(1, 1),
        Op::new_x_gate(2),
        Op::new_pauli_noise_1q(2, x_noise, 0.0, 0.0),
        Op::new_mresetz_gate(2, 2),
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(3, 3, &ops, &None, 100, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    let num_flipped = results
        .shot_results
        .iter()
        .flatten()
        .filter(|&&x| x == 0)
        .count();
    assert!(
        (140..=160).contains(&num_flipped),
        "Expected 140-160 results to be flipped to 0, got {num_flipped}"
    );

    println!(
        "[GPU Runner]: Run 100 shots of X with pauli noise of {x_noise}: {:?}",
        results.shot_results
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn gates_on_lost_qubits() {
    let angle: f32 = PI / 2.0;

    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_loss_noise(0, 0.1),
        Op::new_x_gate(1),
        Op::new_loss_noise(1, 0.1),
        Op::new_cx_gate(0, 1),
        Op::new_rx_gate(angle, 2),
        Op::new_rx_gate(angle, 2),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
        Op::new_mresetz_gate(2, 2),
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(3, 3, &ops, &None, 1000, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
    check_success(&results);
    // Qubit 2 should always be 1 after two Rx(pi/2) gates (and never lost)
    assert!(
        results.shot_results.iter().all(|r| r[2] == 1),
        "Qubit 2 should always be 1 after two Rx(pi/2) gates, got {results:?}"
    );
    // Qubit 0 should be lost about 10% of the time, else 1
    let qubit_0_results: Vec<Vec<u32>> = results.shot_results.iter().map(|r| vec![r[0]]).collect();
    assert_ratio(&qubit_0_results, &[2], 0.1, 0.05);
    assert_ratio(&qubit_0_results, &[1], 0.9, 0.05);

    // Qubit 1 should be lost about 10% of the time, 1 about 9% of the time (when 0 is lost and this isn't), else 0
    let qubit_1_results: Vec<Vec<u32>> = results.shot_results.iter().map(|r| vec![r[1]]).collect();
    assert_ratio(&qubit_1_results, &[2], 0.1, 0.01);
    assert_ratio(&qubit_1_results, &[1], 0.09, 0.01);
    assert_ratio(&qubit_1_results, &[0], 0.8, 0.01);
}

fn scale_teleport() {
    // Create a circuit that does an Rx by a random amount, does a teleport using controlled gates,
    // then does the inverse Rx by the same amount, measurement at the end to verify correctness.
    /* The teleport itself in Q# would be
    H(alice);
    CNOT(alice, bob);

    // Encode the message into the entangled pair.
    CNOT(msg, alice);
    H(msg);

    CNOT(alice, bob);
    Controlled Z([msg], bob);
         */

    use rand::Rng;

    let msg_qubit = 0;
    let alice_qubit = 1;
    let bob_qubit = 2;

    // Generate random angle between 0 and 2π
    let mut rng = rand::thread_rng();
    let angle: f32 = rng.gen_range(0.0..(2.0 * PI));

    let ops: Vec<Op> = vec![
        // Prepare message qubit with rotation
        Op::new_rx_gate(angle, msg_qubit),
        // Create entangled pair (alice, bob)
        Op::new_h_gate(alice_qubit),
        Op::new_cx_gate(alice_qubit, bob_qubit),
        // Teleport: encode message into entangled pair
        Op::new_cx_gate(msg_qubit, alice_qubit),
        Op::new_h_gate(msg_qubit),
        // Apply corrections on bob based on measurements
        Op::new_cx_gate(alice_qubit, bob_qubit),
        Op::new_cz_gate(msg_qubit, bob_qubit),
        // Apply inverse rotation to verify correctness
        Op::new_rx_gate(-angle, bob_qubit),
        // Measure all qubits
        Op::new_mresetz_gate(msg_qubit, 0),
        Op::new_mresetz_gate(alice_qubit, 1),
        Op::new_mresetz_gate(bob_qubit, 2),
    ];

    let start = Instant::now();
    let results =
        run_shots_sync(3, 3, &ops, &None, 50000, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    // Verify that Bob's qubit (every 3rd result) is always 0
    let bob_results: Vec<u32> = results
        .shot_results
        .iter()
        .flatten()
        .skip(2)
        .step_by(3)
        .copied()
        .collect();
    let all_zeros = bob_results.iter().all(|&x| x == 0);
    let num_ones = bob_results.iter().filter(|&&x| x == 1).count();

    println!(
        "[GPU Runner]: Teleport test with random angle {angle:.4} on 3 qubits for 50000 shots"
    );
    println!(
        "[GPU Runner]: Bob's qubit results (every 3rd): {} zeros, {} ones",
        bob_results.len() - num_ones,
        num_ones
    );
    println!("[GPU Runner]: All Bob's measurements are 0: {all_zeros}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}
fn bell_at_scale() {
    let ops: Vec<Op> = vec![
        Op::new_h_gate(0),
        Op::new_cx_gate(0, 1),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(2, 2, &ops, &None, 60000, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);
    println!(
        "[GPU Runner]: 60,000 shots of Bell Pair completed, results length: {}",
        results.shot_results.len()
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_simple_rotation_and_entanglement() {
    let ops: Vec<Op> = vec![
        Op::new_rx_gate(2.25, 1),
        Op::new_cx_gate(1, 12),
        Op::new_cx_gate(12, 23),
        Op::new_mresetz_gate(1, 0),
        Op::new_mresetz_gate(12, 1),
        Op::new_mresetz_gate(23, 2),
    ];
    // At 24 qubits, 8 shots fits into 1GB of GPU memory.
    let start = Instant::now();
    let results = run_shots_sync(24, 3, &ops, &None, 8, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    println!(
        "[GPU Runner]: Results of GHZ state for 8 shots on 24 qubits: {:?}",
        results.shot_results
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

// Given 2 qubits and a vector of (pauli_string, probability) tuples, make a noise op
fn make_2q_pauli_noise(q1: u32, q2: u32, p: Vec<(&str, f32)>) -> Op {
    let mut op = Op::new_2q_gate(ops::PAULI_NOISE_2Q, q1, q2);
    let mut total_prob = 0.0;

    for (pauli_str, prob) in p {
        match pauli_str {
            "IX" => {
                op.r01 = prob;
            }
            "IY" => {
                op.r02 = prob;
            }
            "IZ" => {
                op.r03 = prob;
            }
            "XI" => {
                op.r10 = prob;
            }
            "XX" => {
                op.r11 = prob;
            }
            "XY" => {
                op.r12 = prob;
            }
            "XZ" => {
                op.r13 = prob;
            }
            "YI" => {
                op.r20 = prob;
            }
            "YX" => {
                op.r21 = prob;
            }
            "YY" => {
                op.r22 = prob;
            }
            "YZ" => {
                op.r23 = prob;
            }
            "ZI" => {
                op.r30 = prob;
            }
            "ZX" => {
                op.r31 = prob;
            }
            "ZY" => {
                op.r32 = prob;
            }
            "ZZ" => {
                op.r33 = prob;
            }
            _ => panic!("Invalid pauli string: {}", pauli_str),
        }
        total_prob += prob;
    }
    assert!(total_prob <= 1.0, "Total probability exceeds 1.0");
    op.r00 = 1.0 - total_prob;
    op
}

fn test_2q_pauli_noise() {
    let ops: Vec<Op> = vec![
        Op::new_h_gate(0),
        Op::new_cx_gate(0, 1),
        make_2q_pauli_noise(0, 1, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_cx_gate(1, 2),
        make_2q_pauli_noise(1, 2, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_cx_gate(2, 3),
        make_2q_pauli_noise(2, 3, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_cx_gate(3, 4),
        make_2q_pauli_noise(3, 4, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_cx_gate(4, 5),
        make_2q_pauli_noise(4, 5, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_cx_gate(5, 6),
        make_2q_pauli_noise(5, 6, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_cx_gate(6, 7),
        make_2q_pauli_noise(6, 7, vec![("XX", 0.1), ("YY", 0.1), ("ZZ", 0.1)]),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
        Op::new_mresetz_gate(2, 2),
        Op::new_mresetz_gate(3, 3),
        Op::new_mresetz_gate(4, 4),
        Op::new_mresetz_gate(5, 5),
        Op::new_mresetz_gate(6, 6),
        Op::new_mresetz_gate(7, 7),
    ];
    let start = Instant::now();
    let results = run_shots_sync(8, 8, &ops, &None, 20, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    // Check the results: The first 3 qubits should always agree, the 4th usually with the first 3,
    // and after that it gets messy.
    println!(
        "[GPU Runner]: Results of 2q Pauli noise: {:?}",
        results.shot_results
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_move_noise() {
    let ops: Vec<Op> = vec![
        // Move to interaction zone
        Op::new_move_gate(0),
        Op::new_pauli_noise_1q(0, 0.1, 0.0, 0.0),
        // Do 2 SX gates (i.e. one X gate)
        Op::new_sx_gate(0),
        Op::new_sx_gate(0),
        // Move back
        Op::new_move_gate(0),
        Op::new_pauli_noise_1q(0, 0.1, 0.0, 0.0),
        // Move to measurement one
        Op::new_mresetz_gate(0, 0),
        Op::new_move_gate(0),
        Op::new_pauli_noise_1q(0, 0.1, 0.0, 0.0),
    ];
    // At 24 qubits, 8 shots fits into 1GB of GPU memory.
    let start = Instant::now();
    let results =
        run_shots_sync(1, 1, &ops, &None, 100, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    println!(
        "[GPU Runner]: Results of move op: {:?}",
        results.shot_results
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_benzene() {
    #[allow(clippy::unreadable_literal)]
    let ops: Vec<Op> = vec![
        Op::new_h_gate(2),
        Op::new_pauli_noise_1q(2, 0.000166, 0.000166, 0.000166),
        Op::new_rz_gate(1.87, 2),
        Op::new_pauli_noise_1q(2, 0.000233, 0.000233, 0.000233),
        Op::new_h_gate(2),
        Op::new_pauli_noise_1q(2, 0.000166, 0.000166, 0.000166),
        Op::new_s_adj_gate(2),
        Op::new_pauli_noise_1q(2, 0.000166, 0.000166, 0.000166),
        Op::new_x_gate(0),
        Op::new_cx_gate(0, 7),
        make_2q_pauli_noise(
            0,
            7,
            vec![("XX", 0.000166), ("YY", 0.000166), ("ZZ", 0.000166)],
        ),
        Op::new_cx_gate(0, 6),
        make_2q_pauli_noise(
            0,
            6,
            vec![("XX", 0.000166), ("YY", 0.000166), ("ZZ", 0.000166)],
        ),
        Op::new_cx_gate(0, 1),
        make_2q_pauli_noise(
            0,
            1,
            vec![("XX", 0.000166), ("YY", 0.000166), ("ZZ", 0.000166)],
        ),
        Op::new_x_gate(3),
        Op::new_cx_gate(2, 3),
        make_2q_pauli_noise(
            2,
            3,
            vec![("XX", 0.000166), ("YY", 0.000166), ("ZZ", 0.000166)],
        ),
        Op::new_cx_gate(2, 8),
        make_2q_pauli_noise(
            2,
            8,
            vec![("XX", 0.000166), ("YY", 0.000166), ("ZZ", 0.000166)],
        ),
        Op::new_cx_gate(3, 9),
        make_2q_pauli_noise(
            3,
            9,
            vec![("XX", 0.000166), ("YY", 0.000166), ("ZZ", 0.000166)],
        ),
        Op::new_h_gate(2),
        Op::new_pauli_noise_1q(2, 0.000166, 0.000166, 0.000166),
        Op::new_h_gate(3),
        Op::new_pauli_noise_1q(3, 0.000166, 0.000166, 0.000166),
        Op::new_h_gate(8),
        Op::new_pauli_noise_1q(8, 0.000166, 0.000166, 0.000166),
        Op::new_h_gate(9),
        Op::new_pauli_noise_1q(9, 0.000166, 0.000166, 0.000166),
        Op::new_mresetz_gate(2, 0),
        Op::new_mresetz_gate(3, 1),
        Op::new_mresetz_gate(8, 2),
        Op::new_mresetz_gate(9, 3),
    ];
    let start = Instant::now();
    let results =
        run_shots_sync(10, 4, &ops, &None, 1024, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);
    // TODO: Check that buckets for 1101 & 1110 are over 10%, but 1100 & 1111 are less than 1%
    assert!(
        results.shot_results[4] == vec![1, 1, 0, 1],
        "Expected fourth result to be [1101], got {:?}",
        results.shot_results[4]
    );
    println!("[GPU Runner]: Benzene elapsed time for 1024 shots: {elapsed:.2?}");
}

fn test_cx_various_state() {
    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_cx_gate(0, 7),
        Op::new_cx_gate(0, 6),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(7, 1),
        Op::new_mresetz_gate(6, 2),
    ];
    // With 10 qubits, entries should be 1024, so 8kb per shot, i.e. 0x2000 byte size
    // After X, entry 0x01 is 1.0.
    // For 2 qubit op on qubits 0 and 7
    // - entry00 is bitstring 0
    // - entry01 is bitstring 0x80 (x8 is byte offset 0x400)
    // - entry10 is bitstring 0x01 (x8 is byte offset 0x08)
    // - entry11 is bitstring 0x81 (x8 is byte offset 0x408)
    //
    // After first CX, offset 0x408 should be 1.0 (only entry with a value)
    // Bitstring at 1.0 is |10000001>. is_1_mask is 129 and is_0_mask is 894.
    //
    // For 2 qubit CX on qubits 0 and 6
    // - entry00 is bitstring 0
    // - entry01 is bitstring 0x40 (x8 is byte offset 0x200)
    // - entry10 is bitstring 0x01 (x8 is byte offset 0x08)
    // - entry11 is bitstring 0x41 (x8 is byte offset 0x208)
    // All those entries are 0, but qubit 0 is still 100% 1, so should still flip qubit 6 to 1.
    // i.e. entry 0x208 (bitstring 0x41) should become density 0.5 and 0x408 (0x81) should be reduced to density 0.5
    //
    // To do that, it need to read the value for 0x81, halve it to 0.5, and write back.
    // So it needs to NOT skip entry 10000001, even bits 0 and 7 are 100% 1 state, and target is in 0 state.
    // is_1_mask: 0010000001
    // is_0_mask: 1101111110
    // entry_xx:  001_00000_ // offset 32
    // entry_00:  0010000000
    // entry_11:  0011000001
    // ~1_mask:   1100111110
    // 00andis0:  0000000000
    // 11and~1:   0000000000 - this and above are both 0, so it shouldn't skip this entry!
    //
    // There was a bug where the offset wasn't incrementing when skipping entries, and took all this to find it :-/
    let start = Instant::now();
    let results =
        run_shots_sync(10, 3, &ops, &None, 10, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    // TODO: Check results 0 & 2 are in the 1 state
    //println!("[GPU Runner]: CX Various State on 2 qubits for 10 shots: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_cy_phase() {
    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_h_gate(1),
        Op::new_cy_gate(0, 1),
        Op::new_cy_gate(2, 1),
        Op::new_h_gate(1),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
        Op::new_mresetz_gate(2, 2),
    ];

    let start = Instant::now();
    let results = run_shots_sync(3, 3, &ops, &None, 50, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    assert!(
        results.shot_results.iter().all(|r| r == &[1, 1, 0]),
        "Expected all results to be [1, 1, 0], got {results:?}"
    );
    println!("[GPU Runner]: CY phase test for 50 shots: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_cy_noise_inverts_phase() {
    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_h_gate(1),
        Op::new_cy_gate(0, 1),
        Op::new_h_gate(1),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
    ];

    let mut noise: NoiseConfig<f32, f64> = NoiseConfig::NOISELESS.clone();
    noise.cy.pauli_strings.push(encode_pauli("IZ"));
    noise.cy.probabilities.push(1.0);

    let start = Instant::now();
    let results =
        run_shots_sync(2, 2, &ops, &Some(noise), 50, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    assert!(
        results.shot_results.iter().all(|r| r == &[1, 0]),
        "Expected all results to be [1, 0], got {results:?}"
    );
    println!("[GPU Runner]: CY phase test with IZ noise for 50 shots: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

/// Test that MZ (measure without reset) is idempotent: measuring twice
/// should always produce the same result, since the qubit stays in the
/// measured state after the first measurement.
fn test_mz_idempotent() {
    let ops: Vec<Op> = vec![
        Op::new_h_gate(0),     // Put qubit in |+⟩ superposition
        Op::new_mz_gate(0, 0), // First measurement -> result slot 0
        Op::new_mz_gate(0, 1), // Second measurement -> result slot 1 (should match result slot 0)
    ];
    let shot_count = 1000;
    let results =
        run_shots_sync(1, 2, &ops, &None, shot_count, DEFAULT_SEED, 0).expect("GPU shots failed");
    check_success(&results);

    // Both measurements should always agree
    let mismatches: Vec<&Vec<u32>> = results
        .shot_results
        .iter()
        .filter(|r| r[0] != r[1])
        .collect();
    assert!(
        mismatches.is_empty(),
        "MZ should be idempotent: both measurements must match. Found {} mismatches out of {} shots. First mismatch: {:?}",
        mismatches.len(),
        shot_count,
        mismatches.first()
    );

    // Verify it's roughly 50/50 (Hadamard produces equal superposition)
    assert_ratio(&results.shot_results, &[0, 0], 0.5, 0.1);
    assert_ratio(&results.shot_results, &[1, 1], 0.5, 0.1);

    println!("[GPU Runner]: test_mz_idempotent passed ({shot_count} shots)");
}

/// Test that `ResetGate` properly resets a qubit to |0⟩ without producing a result,
/// while preserving the correct probability distribution on entangled qubits.
/// Circuit: Rx(π/6, q0) -> CNOT(q0,q1) -> ResetGate(q0) -> Measure both
/// Rx(π/6) gives cos²(π/12) ≈ 0.933 for |0⟩ and sin²(π/12) ≈ 0.067 for |1⟩.
/// After CNOT the state is cos(π/12)|00⟩ + sin(π/12)|11⟩.
/// Reset on q0 collapses it, leaving q1 with the same skewed distribution.
fn test_reset_preserves_distribution() {
    let ops: Vec<Op> = vec![
        Op::new_rx_gate(PI / 6.0, 0), // q0 -> cos(π/12)|0⟩ + i·sin(π/12)|1⟩
        Op::new_cx_gate(0, 1),        // Entangle: cos(π/12)|00⟩ + i·sin(π/12)|11⟩
        Op::new_reset_gate_proper(0), // Reset q0 to |0⟩ (no result stored)
        Op::new_mresetz_gate(0, 0),   // Measure q0 -> result slot 0
        Op::new_mresetz_gate(1, 1),   // Measure q1 -> result slot 1
    ];
    let shot_count = 1000;
    let results =
        run_shots_sync(2, 2, &ops, &None, shot_count, DEFAULT_SEED, 0).expect("GPU shots failed");
    check_success(&results);

    // q0 should always be 0 after reset
    let q0_nonzero: Vec<&Vec<u32>> = results.shot_results.iter().filter(|r| r[0] != 0).collect();
    assert!(
        q0_nonzero.is_empty(),
        "ResetGate should always produce |0⟩. Found {} non-zero results out of {} shots. First: {:?}",
        q0_nonzero.len(),
        shot_count,
        q0_nonzero.first()
    );

    // q1 should reflect the skewed distribution: ~93.3% |0⟩, ~6.7% |1⟩
    let q1_results: Vec<Vec<u32>> = results.shot_results.iter().map(|r| vec![r[1]]).collect();
    assert_ratio(&q1_results, &[0], 0.933, 0.05);
    assert_ratio(&q1_results, &[1], 0.067, 0.05);

    println!("[GPU Runner]: test_reset_preserves_distribution passed ({shot_count} shots)");
}

fn repeated_noise() {
    let mut ops: Vec<Op> = Vec::new();

    // Add X(0), X(1), CX(0,1) to the ops 100 times
    for _ in 0..100 {
        ops.push(Op::new_x_gate(0));
        ops.push(Op::new_x_gate(1));
        ops.push(Op::new_cx_gate(0, 1));
    }

    // Do an Rx(PI/2) on qubit 4 twice
    ops.push(Op::new_rx_gate(PI / 2.0, 4));
    ops.push(Op::new_rx_gate(PI / 2.0, 4));

    // Measure qubits 0-4
    for i in 0..5 {
        ops.push(Op::new_mresetz_gate(i, i));
    }

    // Add bit-flip and loss noise to X gates of 0.1%
    let mut noise: NoiseConfig<f32, f64> = NoiseConfig::NOISELESS.clone();
    noise.x.pauli_strings.push(encode_pauli("X"));
    noise.x.probabilities.push(0.001);
    noise.x.loss = 0.001;

    let start = Instant::now();
    // Run for 20,000 shots
    let results =
        run_shots_sync(5, 5, &ops, &Some(noise), 20000, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    /* Compare with the expected results we get from the CPU simulator, which are ('-' == 2 for lost):

    00001: 13504 (67.52%)
    0-001: 1575 (7.88%)
    01001: 1412 (7.06%)
    -0001: 896 (4.48%)
    -1001: 854 (4.27%)
    11001: 734 (3.67%)
    10001: 720 (3.60%)
    --001: 160 (0.80%)
    1-001: 145 (0.72%)
        */
    assert_ratio(&results.shot_results, &[0, 0, 0, 0, 1], 0.675, 0.01);
    assert_ratio(&results.shot_results, &[0, 2, 0, 0, 1], 0.078, 0.003);
    assert_ratio(&results.shot_results, &[0, 1, 0, 0, 1], 0.071, 0.003);
    assert_ratio(&results.shot_results, &[2, 0, 0, 0, 1], 0.045, 0.002);
    assert_ratio(&results.shot_results, &[2, 1, 0, 0, 1], 0.043, 0.002);
    assert_ratio(&results.shot_results, &[1, 1, 0, 0, 1], 0.037, 0.002);
    assert_ratio(&results.shot_results, &[1, 0, 0, 0, 1], 0.036, 0.002);
    assert_ratio(&results.shot_results, &[2, 2, 0, 0, 1], 0.008, 0.001);
    assert_ratio(&results.shot_results, &[1, 2, 0, 0, 1], 0.007, 0.001);
    assert_ratio(&results.shot_results, &[1, 2, 0, 0, 0], 0.0, 0.0);

    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn scaled_ising() {
    let grover_ir = include_str!("./ising.ll");

    let mut ops: Vec<Op> = Vec::new();

    // Iterate through grover lines and add ops for each (handling CCX decomposition)
    for line in grover_ir.lines() {
        let mut line_ops = op_from_ir_line(line);
        ops.append(&mut line_ops);
    }

    let start = Instant::now();
    // Run for 10 shots, which should scale across 3 batches on the GPU (max 4 per batch)
    let results =
        run_shots_sync(25, 25, &ops, &None, 10, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    println!("[GPU Runner]: Scaled Ising (5x5) results for 10 shots:");
    for res in &results.shot_results {
        println!("  {res:?}");
    }
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn scaled_grover() {
    let grover_ir = include_str!("./grover_full.ir");

    let mut ops: Vec<Op> = Vec::new();

    // Iterate through grover lines and add ops for each (handling CCX decomposition)
    for line in grover_ir.lines() {
        let mut line_ops = op_from_ir_line(line);
        ops.append(&mut line_ops);
    }

    let start = Instant::now();
    let results =
        run_shots_sync(24, 20, &ops, &None, 4, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    for res in &results.shot_results {
        assert!(
            res == &vec![0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0],
            "Expected result to be [01010 x4], got {res:?}",
        );
    }
    println!("[GPU Runner]: Scaled Grover (2344 ops on 24 qubits) results for 4 shots:");
    for res in &results.shot_results {
        println!("  {res:?}");
    }

    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn noise_config() {
    let mut noise: NoiseConfig<f32, f64> = NoiseConfig::NOISELESS.clone();
    noise.x.pauli_strings.push(encode_pauli("X"));
    noise.x.probabilities.push(0.5);
    noise.x.loss = 0.333_333;

    let ops: Vec<Op> = vec![
        Op::new_x_gate(0),
        Op::new_x_gate(1),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
    ];

    let start = Instant::now();
    let results =
        run_shots_sync(2, 2, &ops, &Some(noise), 500, DEFAULT_SEED, 0).expect("GPU shots failed");
    let elapsed = start.elapsed();
    check_success(&results);

    let results = results.shot_results;
    let result_0: Vec<Vec<u32>> = results.iter().map(|r| vec![r[0]]).collect();
    let result_1: Vec<Vec<u32>> = results.iter().map(|r| vec![r[1]]).collect();
    assert_ratio(&result_0, &[0], 0.333, 0.1);
    assert_ratio(&result_0, &[1], 0.333, 0.1);
    assert_ratio(&result_0, &[2], 0.333, 0.1);
    assert_ratio(&result_1, &[0], 0.333, 0.1);
    assert_ratio(&result_1, &[1], 0.333, 0.1);
    assert_ratio(&result_1, &[2], 0.333, 0.1);

    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn correlated_noise() {
    let result: Result<RunResults, String> = futures::executor::block_on(async {
        let ops: Vec<Op> = vec![
            Op::new_x_gate(0),
            Op::new_x_gate(1),
            Op::new_correlated_noise_gate(0, &[0, 1]),
            Op::new_mresetz_gate(0, 0),
            Op::new_mresetz_gate(1, 1),
        ];
        let mut context = GpuContext::default();
        context.set_program(&ops, 2, 2);
        context.add_correlated_noise_table(
            "my_noise",
            "
XX,0.25
ZZ,0.25
IY,0.125
YI,0.125
",
        );
        context.run_shots(1000, DEFAULT_SEED, 0).await
    });
    let result = result.expect("GPU shots failed");
    check_success(&result);
    // Without noise would be 100& [1,1]
    // With the noise above, should be 50% [1,1] (II), 25% [0,0] (XX), 12.5% [0,1] (YI), 12.5% [1,0] (IY)
    assert_ratio(&result.shot_results, &[0, 0], 0.25, 0.1);
    assert_ratio(&result.shot_results, &[1, 1], 0.50, 0.1);
    assert_ratio(&result.shot_results, &[0, 1], 0.125, 0.1);
    assert_ratio(&result.shot_results, &[1, 0], 0.125, 0.1);
}

#[allow(clippy::too_many_lines)]
fn op_from_ir_line(line: &str) -> Vec<Op> {
    let line = line.trim();

    // Skip non-quantum operation lines
    assert!(
        line.starts_with("call void @__quantum__qis__"),
        "Unexpected IR line: {line}"
    );

    // Regex to parse the entire IR line in one go
    let re = Regex::new(r"call void @__quantum__qis__(\w+)__(body|adj).*").expect("Invalid regex");
    let Some(captures) = re.captures(line) else {
        panic!("Failed to parse IR line: {line}");
    };

    let op_name = &captures[1];
    let is_adj = &captures[2] == "adj";

    // Extract angle parameter for rotation gates
    let angle_re = Regex::new(r"double ([+-]?[0-9]*\.?[0-9]+(?:[eE][+-]?[0-9]+)?)")
        .expect("Invalid angle regex");
    let angle: Option<f32> = angle_re.captures(line).and_then(|cap| cap[1].parse().ok());

    // Extract qubit and result indices using regex
    let qubit_re = Regex::new(r"inttoptr \(i64 (\d+) to %Qubit\*\)").expect("Invalid qubit regex");
    let result_re =
        Regex::new(r"inttoptr \(i64 (\d+) to %Result\*\)").expect("Invalid result regex");

    let qubits: Vec<u32> = qubit_re
        .captures_iter(line)
        .filter_map(|cap| cap[1].parse().ok())
        .collect();

    let result_ids: Vec<u32> = result_re
        .captures_iter(line)
        .filter_map(|cap| cap[1].parse().ok())
        .collect();

    // Create operations based on the operation name
    match op_name {
        "h" => vec![Op::new_h_gate(qubits[0])],
        "x" => vec![Op::new_x_gate(qubits[0])],
        "y" => vec![Op::new_y_gate(qubits[0])],
        "z" => vec![Op::new_z_gate(qubits[0])],
        "s" => {
            if is_adj {
                vec![Op::new_s_adj_gate(qubits[0])]
            } else {
                vec![Op::new_s_gate(qubits[0])]
            }
        }
        "t" => {
            if is_adj {
                vec![Op::new_t_adj_gate(qubits[0])]
            } else {
                vec![Op::new_t_gate(qubits[0])]
            }
        }
        "sx" => {
            if is_adj {
                vec![Op::new_sx_adj_gate(qubits[0])]
            } else {
                vec![Op::new_sx_gate(qubits[0])]
            }
        }
        "rx" => {
            if let Some(angle_val) = angle {
                vec![Op::new_rx_gate(angle_val, qubits[0])]
            } else {
                eprintln!("Warning: RX gate missing angle parameter");
                Vec::new()
            }
        }
        "ry" => {
            if let Some(angle_val) = angle {
                vec![Op::new_ry_gate(angle_val, qubits[0])]
            } else {
                eprintln!("Warning: RY gate missing angle parameter");
                Vec::new()
            }
        }
        "rz" => {
            if let Some(angle_val) = angle {
                vec![Op::new_rz_gate(angle_val, qubits[0])]
            } else {
                eprintln!("Warning: RZ gate missing angle parameter");
                Vec::new()
            }
        }
        "cx" => vec![Op::new_cx_gate(qubits[0], qubits[1])],
        "cy" => vec![Op::new_cy_gate(qubits[0], qubits[1])],
        "cz" => vec![Op::new_cz_gate(qubits[0], qubits[1])],
        "rxx" => {
            if let Some(angle_val) = angle {
                vec![Op::new_rxx_gate(angle_val, qubits[0], qubits[1])]
            } else {
                eprintln!("Warning: RXX gate missing angle parameter");
                Vec::new()
            }
        }
        "ryy" => {
            if let Some(angle_val) = angle {
                vec![Op::new_ryy_gate(angle_val, qubits[0], qubits[1])]
            } else {
                eprintln!("Warning: RYY gate missing angle parameter");
                Vec::new()
            }
        }
        "rzz" => {
            if let Some(angle_val) = angle {
                vec![Op::new_rzz_gate(angle_val, qubits[0], qubits[1])]
            } else {
                eprintln!("Warning: RZZ gate missing angle parameter");
                Vec::new()
            }
        }
        "m" | "mresetz" => vec![Op::new_mresetz_gate(qubits[0], result_ids[0])],
        "ccx" => {
            // Decompose CCX (Toffoli) gate as per the Python implementation
            let ctrl1 = qubits[0];
            let ctrl2 = qubits[1];
            let target = qubits[2];

            vec![
                Op::new_h_gate(target),
                Op::new_t_adj_gate(ctrl1),
                Op::new_t_adj_gate(ctrl2),
                Op::new_h_gate(ctrl1),
                Op::new_cz_gate(target, ctrl1),
                Op::new_h_gate(ctrl1),
                Op::new_t_gate(ctrl1),
                Op::new_h_gate(target),
                Op::new_cz_gate(ctrl2, target),
                Op::new_h_gate(target),
                Op::new_h_gate(ctrl1),
                Op::new_cz_gate(ctrl2, ctrl1),
                Op::new_h_gate(ctrl1),
                Op::new_t_gate(target),
                Op::new_t_adj_gate(ctrl1),
                Op::new_h_gate(target),
                Op::new_cz_gate(ctrl2, target),
                Op::new_h_gate(target),
                Op::new_h_gate(ctrl1),
                Op::new_cz_gate(target, ctrl1),
                Op::new_h_gate(ctrl1),
                Op::new_t_adj_gate(target),
                Op::new_t_gate(ctrl1),
                Op::new_h_gate(ctrl1),
                Op::new_cz_gate(ctrl2, ctrl1),
                Op::new_h_gate(ctrl1),
                Op::new_h_gate(target),
            ]
        }
        _ => {
            eprintln!("Warning: Unrecognized operation: {op_name}");
            Vec::new()
        }
    }
}
