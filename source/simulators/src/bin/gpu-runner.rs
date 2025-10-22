// Used for local testing of the GPU runner (e.g. profiling in Xcode)

// Run with: cargo run --bin gpu-runner [--release]
// Build with: cargo build --bin gpu-runner [--release]

use qdk_simulators::run_gpu_shots;
use qdk_simulators::shader_types::Op;

fn main() {
    simple_bell_pair();
    test_simple_rotation_and_entanglement();
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
    let results = run_gpu_shots(12, 2, ops, 10);
    println!("[GPU Runner]: Results: {results:?}");
}

fn test_simple_rotation_and_entanglement() {
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;

    let ops: Vec<Op> = vec![
        init_op, // 1, 0xFFFFFFFF, 0xDEADBEEF
        Op::new_rx_gate(2.25, 1),
        Op::new_cx_gate(1, 11),
        Op::new_cx_gate(11, 22),
        Op::new_mresetz_gate(1, 0),
        Op::new_mresetz_gate(11, 1),
        Op::new_mresetz_gate(22, 2),
    ];
    // At 22 qubits, 32 shots fits into 1GB of GPU memory.
    // At 25 qubits, 4 shots fits into 1GB of GPU memory.
    let results = run_gpu_shots(25, 3, ops, 4);
    println!("[GPU Runner]: Results: {results:?}");
}
