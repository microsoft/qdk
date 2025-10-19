// Used for local testing of the GPU runner (e.g. profiling in Xcode)

// Run with: cargo run --bin gpu-runner [--release]
// Build with: cargo build --bin gpu-runner [--release]

use qdk_simulators::run_gpu_shots;
use qdk_simulators::shader_types::Op;

fn main() {
    // Reset all qubits and rng with seed
    let mut init_op = Op::new_reset_gate(u32::MAX);
    init_op.q2 = 0xdead_beef;

    let ops: Vec<Op> = vec![
        init_op,                  // 1, 0xFFFFFFFF, 0xDEADBEEF
        Op::new_h_gate(0),        // 5, 0
        Op::new_cx_gate(0, 1),    // 15, 0, 1
        Op::new_sample_gate(0.0), // 27
    ];
    let _ = run_gpu_shots(5, 5, ops, 10);
    println!("GPU Runner");
}
