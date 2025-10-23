// Used for local testing of the GPU runner (e.g. profiling in Xcode)

// Run with: cargo run --bin gpu-runner [--release]
// Build with: cargo build --bin gpu-runner [--release]

use qdk_simulators::run_gpu_shots;
use qdk_simulators::shader_types::Op;
use std::time::Instant;

fn main() {
    simple_bell_pair();
    test_simple_rotation_and_entanglement();
    bell_at_scale();
    scale_teleport();
    test_pauli_noise();
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
    let results = run_gpu_shots(12, 2, ops, 10);
    let elapsed = start.elapsed();
    println!("[GPU Runner]: Simple Bell Pair on 12 qubits for 10 shots: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_pauli_noise() {
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;
    let x_noise: f32 = 0.0;

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
    let results = run_gpu_shots(3, 3, ops, 10);
    let elapsed = start.elapsed();
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

    // Verify that Bob's qubit (every 3rd result) is always 0
    let bob_results: Vec<u32> = results.iter().skip(2).step_by(3).copied().collect();
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
        Op::new_cx_gate(12, 24),
        Op::new_mresetz_gate(1, 0),
        Op::new_mresetz_gate(12, 1),
        Op::new_mresetz_gate(24, 2),
    ];
    // At 22 qubits, 32 shots fits into 1GB of GPU memory.
    // At 25 qubits, 4 shots fits into 1GB of GPU memory.
    let start = Instant::now();
    let results = run_gpu_shots(24, 3, ops, 8);
    let elapsed = start.elapsed();
    println!("[GPU Runner]: Results of GHZ state for 8 shots on 24 qubits: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}
