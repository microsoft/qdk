use qdk_simulators::run_gpu_shots;
use qdk_simulators::shader_types::Op;

fn main() {
    let ops: Vec<Op> = vec![{ Op::new_h_gate(0) }, { Op::new_cx_gate(0, 1) }];
    let _ = run_gpu_shots(5, 5, ops, 10);
    println!("GPU Runner");
}
