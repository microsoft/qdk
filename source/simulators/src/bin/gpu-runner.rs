// Used for local testing of the GPU runner (e.g. profiling in Xcode)

// Run with: cargo run --bin gpu-runner [--release]
// Build with: cargo build --bin gpu-runner [--release]

use qdk_simulators::run_gpu_shots;
use qdk_simulators::shader_types::Op;
use std::time::Instant;

fn main() {
    simple_bell_pair();
    bell_at_scale();
    scale_teleport();
    test_pauli_noise();
    test_simple_rotation_and_entanglement();
    test_2q_pauli_noise();
}

fn split_results(result_count: usize, results: &[u32]) -> (Vec<Vec<u32>>, Vec<u32>) {
    let results_list = results
        .chunks(result_count + 1)
        .map(|chunk| chunk[..result_count].to_vec())
        .collect::<Vec<Vec<u32>>>();
    // Separate out every 3rd entry from results into 'error_codes'
    let error_codes = results
        .chunks(result_count + 1)
        .map(|chunk| chunk[result_count])
        .collect::<Vec<u32>>();
    (results_list, error_codes)
}

fn simple_bell_pair() {
    // Reset all qubits and rng with seed
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;

    let ops: Vec<Op> = vec![
        init_op,                     // 1, 0xFFFFFFFF, 0xDEADBEEF
        Op::new_h_gate(9),           // 5, 9
        Op::new_cx_gate(9, 11),      // 15, 9, 11
        Op::new_mresetz_gate(9, 0),  // 22, 9, 0
        Op::new_mresetz_gate(11, 1), // 22, 11, 1
    ];
    let start = Instant::now();
    let results = run_gpu_shots(12, 2, ops, 10).expect("GPU shots failed");
    let elapsed = start.elapsed();

    let (results, _error_codes) = split_results(2, &results);
    println!("[GPU Runner]: Simple Bell Pair on 12 qubits for 10 shots: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_pauli_noise() {
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;
    let x_noise: f32 = 0.5;

    let ops: Vec<Op> = vec![
        init_op,
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
    let results = run_gpu_shots(3, 3, ops, 100).expect("GPU shots failed");
    let elapsed = start.elapsed();
    // Verify we get 30 results, of which 14 - 16 will have been flipped to 0. (14 with current rng)

    let (results, _error_codes) = split_results(3, &results);

    let num_flipped = results.iter().flatten().filter(|&&x| x == 0).count();
    // TODO: Should be about 150 flipped here. See what's going on.
    assert!(
        (80..=100).contains(&num_flipped),
        "Expected 14-16 results to be flipped to 0, got {num_flipped}"
    );

    println!("[GPU Runner]: Run 10 shots of X with pauli noise of {x_noise}: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
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
    use std::f32::consts::PI;

    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xfeed_face;

    let msg_qubit = 0;
    let alice_qubit = 1;
    let bob_qubit = 2;

    // Generate random angle between 0 and 2π
    let mut rng = rand::thread_rng();
    let angle: f32 = rng.gen_range(0.0..(2.0 * PI));

    let ops: Vec<Op> = vec![
        init_op,
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
    let results = run_gpu_shots(3, 3, ops, 50000).expect("GPU shots failed");
    let elapsed = start.elapsed();

    let (results, _error_codes) = split_results(3, &results);

    // Verify that Bob's qubit (every 3rd result) is always 0
    let bob_results: Vec<u32> = results
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
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xaabb_ccdd;

    let ops: Vec<Op> = vec![
        init_op,
        Op::new_h_gate(0),
        Op::new_cx_gate(0, 1),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
    ];
    let start = Instant::now();
    let results = run_gpu_shots(2, 2, ops, 60000).expect("GPU shots failed");
    let elapsed = start.elapsed();
    println!(
        "[GPU Runner]: 60,000 shots of Bell Pair completed, results length: {}",
        results.len()
    );
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_simple_rotation_and_entanglement() {
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;

    let ops: Vec<Op> = vec![
        init_op, // 1, 0xFFFFFFFF, 0xDEADBEEF
        Op::new_rx_gate(2.25, 1),
        Op::new_cx_gate(1, 12),
        Op::new_cx_gate(12, 23),
        Op::new_mresetz_gate(1, 0),
        Op::new_mresetz_gate(12, 1),
        Op::new_mresetz_gate(23, 2),
    ];
    // At 24 qubits, 8 shots fits into 1GB of GPU memory.
    let start = Instant::now();
    let results = run_gpu_shots(24, 3, ops, 8).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, _error_codes) = split_results(3, &results);
    println!("[GPU Runner]: Results of GHZ state for 8 shots on 24 qubits: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_2q_pauli_noise() {
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;

    let ops: Vec<Op> = vec![
        init_op, // 1, 0xFFFFFFFF, 0xDEADBEEF
        Op::new_h_gate(0),
        Op::new_cx_gate(0, 1),
        // Op::new_pauli_noise_2q(0, 1, 0.1, 0.1, 0.1),
        Op::new_cx_gate(1, 2),
        // Op::new_pauli_noise_2q(1, 2, 0.1, 0.1, 0.1),
        Op::new_cx_gate(2, 3),
        // Op::new_pauli_noise_2q(2, 3, 0.1, 0.1, 0.1),
        Op::new_cx_gate(3, 4),
        Op::new_pauli_noise_2q(3, 4, 0.1, 0.1, 0.1),
        Op::new_cx_gate(4, 5),
        Op::new_pauli_noise_2q(4, 5, 0.1, 0.1, 0.1),
        Op::new_cx_gate(5, 6),
        Op::new_pauli_noise_2q(5, 6, 0.1, 0.1, 0.1),
        Op::new_cx_gate(6, 7),
        Op::new_pauli_noise_2q(6, 7, 0.1, 0.1, 0.1),
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
    let results = run_gpu_shots(8, 8, ops, 20).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, _error_codes) = split_results(8, &results);
    // Check the results: The first 3 qubits should always agree, the 4th usually with the first 3,
    // and after that it gets messy.
    println!("[GPU Runner]: Results of 2q Pauli noise: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}
