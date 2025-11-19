// Used for local testing of the GPU runner (e.g. profiling in Xcode)

// Run with: cargo run --bin gpu-runner [--release]
// Build with: cargo build --bin gpu-runner [--release]

use core::panic;
use qdk_simulators::run_parallel_shots;
use qdk_simulators::shader_types::Op;
use regex_lite::Regex;
use std::time::Instant;
use std::vec;

const DEFAULT_SEED: u32 = 0xfeed_face;

fn main() {
    simple_bell_pair();
    bell_at_scale();
    scale_teleport();
    test_pauli_noise();
    test_simple_rotation_and_entanglement();
    test_2q_pauli_noise();
    test_move_noise();
    test_benzene();
    test_cx_various_state();
    scaled_ising();
    scaled_grover();
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

fn has_no_errors(error_codes: &[u32]) -> bool {
    error_codes.iter().all(|&code| code == 0)
}

fn simple_bell_pair() {
    let ops: Vec<Op> = vec![
        Op::new_h_gate(9),           // 5, 9
        Op::new_cx_gate(9, 11),      // 15, 9, 11
        Op::new_mresetz_gate(9, 0),  // 22, 9, 0
        Op::new_mresetz_gate(11, 1), // 22, 11, 1
    ];
    let start = Instant::now();
    let results = run_parallel_shots(12, 2, ops, 10, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();

    let (results, error_codes) = split_results(2, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    println!("[GPU Runner]: Simple Bell Pair on 12 qubits for 10 shots: {results:?}");
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
    let results = run_parallel_shots(3, 3, ops, 100, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();

    let (results, error_codes) = split_results(3, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );

    let num_flipped = results.iter().flatten().filter(|&&x| x == 0).count();
    assert!(
        (140..=160).contains(&num_flipped),
        "Expected 140-160 results to be flipped to 0, got {num_flipped}"
    );

    println!("[GPU Runner]: Run 100 shots of X with pauli noise of {x_noise}: {results:?}");
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
    let results = run_parallel_shots(3, 3, ops, 50000, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();

    let (results, error_codes) = split_results(3, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );

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
    let ops: Vec<Op> = vec![
        Op::new_h_gate(0),
        Op::new_cx_gate(0, 1),
        Op::new_mresetz_gate(0, 0),
        Op::new_mresetz_gate(1, 1),
    ];
    let start = Instant::now();
    let results = run_parallel_shots(2, 2, ops, 60000, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    println!(
        "[GPU Runner]: 60,000 shots of Bell Pair completed, results length: {}",
        results.len()
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
    let results = run_parallel_shots(24, 3, ops, 8, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, error_codes) = split_results(3, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    println!("[GPU Runner]: Results of GHZ state for 8 shots on 24 qubits: {results:?}");
    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
}

fn test_2q_pauli_noise() {
    let ops: Vec<Op> = vec![
        Op::new_h_gate(0),
        Op::new_cx_gate(0, 1),
        Op::new_pauli_noise_2q(0, 1, 0.1, 0.1, 0.1),
        Op::new_cx_gate(1, 2),
        Op::new_pauli_noise_2q(1, 2, 0.1, 0.1, 0.1),
        Op::new_cx_gate(2, 3),
        Op::new_pauli_noise_2q(2, 3, 0.1, 0.1, 0.1),
        Op::new_cx_gate(3, 4),
        Op::new_pauli_noise_2q(3, 4, 0.1, 0.1, 0.1),
        Op::new_cx_gate(4, 5),
        Op::new_pauli_noise_2q(4, 5, 0.3, 0.1, 0.1),
        Op::new_cx_gate(5, 6),
        Op::new_pauli_noise_2q(5, 6, 0.3, 0.1, 0.1),
        Op::new_cx_gate(6, 7),
        Op::new_pauli_noise_2q(6, 7, 0.3, 0.1, 0.1),
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
    let results = run_parallel_shots(8, 8, ops, 20, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, error_codes) = split_results(8, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    // Check the results: The first 3 qubits should always agree, the 4th usually with the first 3,
    // and after that it gets messy.
    println!("[GPU Runner]: Results of 2q Pauli noise: {results:?}");
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
    let results = run_parallel_shots(1, 1, ops, 100, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, error_codes) = split_results(1, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    println!("[GPU Runner]: Results of move op: {results:?}");
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
        Op::new_pauli_noise_2q(0, 7, 0.000166, 0.000166, 0.000166),
        Op::new_cx_gate(0, 6),
        Op::new_pauli_noise_2q(0, 6, 0.000166, 0.000166, 0.000166),
        Op::new_cx_gate(0, 1),
        Op::new_pauli_noise_2q(0, 1, 0.000166, 0.000166, 0.000166),
        Op::new_x_gate(3),
        Op::new_cx_gate(2, 3),
        Op::new_pauli_noise_2q(2, 3, 0.000166, 0.000166, 0.000166),
        Op::new_cx_gate(2, 8),
        Op::new_pauli_noise_2q(2, 8, 0.000166, 0.000166, 0.000166),
        Op::new_cx_gate(3, 9),
        Op::new_pauli_noise_2q(3, 9, 0.000166, 0.000166, 0.000166),
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
    let results = run_parallel_shots(10, 4, ops, 1024, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, error_codes) = split_results(4, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    // TODO: Check that buckets for 1101 & 1110 are over 10%, but 1100 & 1111 are less than 1%
    assert!(
        results[4] == vec![1, 1, 0, 1],
        "Expected first result to be [0001], got {:?}",
        results[1]
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
    let results = run_parallel_shots(10, 3, ops, 1, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();

    let (results, error_codes) = split_results(3, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    // TODO: Check results 0 & 2 are in the 1 state, and result 1 is ~50/50
    println!("[GPU Runner]: CX Various State on 2 qubits for 10 shots: {results:?}");
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
    let results = run_parallel_shots(25, 25, ops, 10, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, error_codes) = split_results(25, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    println!("[GPU Runner]: Scaled Ising (5x5) results for 10 shots:");
    for res in &results {
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
    let results = run_parallel_shots(24, 20, ops, 4, DEFAULT_SEED).expect("GPU shots failed");
    let elapsed = start.elapsed();
    let (results, error_codes) = split_results(20, &results);
    assert!(
        has_no_errors(&error_codes),
        "Error codes from GPU: {error_codes:?}"
    );
    for res in &results {
        assert!(
            res == &vec![0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0],
            "Expected result to be [01010 x4], got {res:?}",
        );
    }
    println!("[GPU Runner]: Scaled Grover (2344 ops on 24 qubits) results for 4 shots:");
    for res in &results {
        println!("  {res:?}");
    }

    println!("[GPU Runner]: Elapsed time: {elapsed:.2?}");
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
