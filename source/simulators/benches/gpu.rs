#![allow(clippy::unit_arg)]

use criterion::{Criterion, criterion_group, criterion_main};
use qdk_simulators::run_gpu_simulator;
use qdk_simulators::shader_types::Op;
use rand::{Rng, SeedableRng, distributions::Uniform, prelude::Distribution, rngs::StdRng};
use std::hint::black_box;

const SEED: u64 = 1000;
const NUM_QUBITS: u32 = 10;

fn random_qubit(rng: &mut StdRng) -> u32 {
    let distr = Uniform::new(0, u32::MAX);
    distr.sample(rng) % NUM_QUBITS
}

fn random_qubit_pair(rng: &mut StdRng) -> (u32, u32) {
    let q1 = random_qubit(rng);
    let mut q2 = random_qubit(rng);

    // Ensure different qubits
    while q1 == q2 {
        q2 = random_qubit(rng);
    }

    (q1, q2)
}

fn gate(rng: &mut StdRng) -> Op {
    let distr = Uniform::new(0, usize::MAX);
    let gate = distr.sample(rng) % 12;

    match gate {
        0 => Op::new_id_gate(random_qubit(rng)),
        1 => Op::new_x_gate(random_qubit(rng)),
        2 => Op::new_y_gate(random_qubit(rng)),
        3 => Op::new_z_gate(random_qubit(rng)),
        4 => Op::new_h_gate(random_qubit(rng)),
        5 => Op::new_s_gate(random_qubit(rng)),
        6 => Op::new_s_adj_gate(random_qubit(rng)),
        7 => Op::new_sx_gate(random_qubit(rng)),
        8 => Op::new_sx_adj_gate(random_qubit(rng)),
        9 => Op::new_t_gate(random_qubit(rng)),
        10 => Op::new_t_adj_gate(random_qubit(rng)),
        11 => Op::new_matrix_gate(
            random_qubit(rng),
            (1.0, 0.0),
            (0.0, 0.0), // Identity matrix for testing
            (0.0, 0.0),
            (1.0, 0.0),
        ),
        _ => unreachable!(),
    }
}

fn random_gates(num_gates: usize) -> Vec<Op> {
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut gates: Vec<Op> = Vec::with_capacity(num_gates);
    for _ in 0..num_gates {
        gates.push(gate(&mut rng));
    }
    gates.push(Op::new_m_every_z_gate());

    gates
}

fn gates(num_gates: usize, mut f: impl FnMut(&mut StdRng) -> Op) -> Vec<Op> {
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut gates: Vec<Op> = Vec::with_capacity(num_gates);
    for _ in 0..num_gates {
        gates.push(f(&mut rng));
    }
    gates.push(Op::new_m_every_z_gate());

    gates
}

fn sim_id_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_id_gate(random_qubit(rng)));
    c.bench_function("id gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_x_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_x_gate(random_qubit(rng)));
    c.bench_function("x gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_y_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_y_gate(random_qubit(rng)));
    c.bench_function("y gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_z_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_z_gate(random_qubit(rng)));
    c.bench_function("z gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_h_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_h_gate(random_qubit(rng)));
    c.bench_function("h gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_s_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_s_gate(random_qubit(rng)));
    c.bench_function("s gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_s_adj_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_s_adj_gate(random_qubit(rng)));
    c.bench_function("s adj gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_sx_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_sx_gate(random_qubit(rng)));
    c.bench_function("sx gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_sx_adj_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_sx_adj_gate(random_qubit(rng)));
    c.bench_function("sx adj gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_t_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_t_gate(random_qubit(rng)));
    c.bench_function("t gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_t_adj_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| Op::new_t_adj_gate(random_qubit(rng)));
    c.bench_function("t adj gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_1q_matrix_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        Op::new_matrix_gate(
            random_qubit(rng),
            (1.0, 0.0),
            (0.0, 0.0), // Identity matrix for benchmark
            (0.0, 0.0),
            (1.0, 0.0),
        )
    });
    c.bench_function("1q matrix gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_rx_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
        Op::new_rx_gate(angle, random_qubit(rng))
    });
    c.bench_function("rx gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_ry_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
        Op::new_ry_gate(angle, random_qubit(rng))
    });
    c.bench_function("ry gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_rz_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
        Op::new_rz_gate(angle, random_qubit(rng))
    });
    c.bench_function("rz gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

// 2-qubit gate benchmarks

fn sim_cx_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let (control, target) = random_qubit_pair(rng);
        Op::new_cx_gate(control, target)
    });
    c.bench_function("cx gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_cz_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let (control, target) = random_qubit_pair(rng);
        Op::new_cz_gate(control, target)
    });
    c.bench_function("cz gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_rxx_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
        let (q1, q2) = random_qubit_pair(rng);
        Op::new_rxx_gate(angle, q1, q2)
    });
    c.bench_function("rxx gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_ryy_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
        let (q1, q2) = random_qubit_pair(rng);
        Op::new_ryy_gate(angle, q1, q2)
    });
    c.bench_function("ryy gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_rzz_gate(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, |rng| {
        let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
        let (q1, q2) = random_qubit_pair(rng);
        Op::new_rzz_gate(angle, q1, q2)
    });
    c.bench_function("rzz gate", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn random_2q_gate(rng: &mut StdRng) -> Op {
    let distr = Uniform::new(0, usize::MAX);
    let gate = distr.sample(rng) % 5;
    let (q1, q2) = random_qubit_pair(rng);

    match gate {
        0 => Op::new_cx_gate(q1, q2),
        1 => Op::new_cz_gate(q1, q2),
        2 => {
            let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
            Op::new_rxx_gate(angle, q1, q2)
        }
        3 => {
            let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
            Op::new_ryy_gate(angle, q1, q2)
        }
        4 => {
            let angle = rng.r#gen::<f32>() * std::f32::consts::TAU;
            Op::new_rzz_gate(angle, q1, q2)
        }
        _ => unreachable!(),
    }
}

fn sim_1k_2q_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = gates(NUM_GATES, random_2q_gate);
    c.bench_function("1k 2q gates", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_1k_1q_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("1k gates", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_5k_1q_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 5_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("5k gates", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

criterion_group!(
    benches_1q,
    sim_x_gate,
    sim_y_gate,
    sim_z_gate,
    sim_h_gate,
    sim_s_gate,
    sim_s_adj_gate,
    sim_sx_gate,
    sim_sx_adj_gate,
    sim_t_gate,
    sim_t_adj_gate,
    sim_1q_matrix_gate,
    sim_rx_gate,
    sim_ry_gate,
    sim_rz_gate,
    sim_id_gate
);

criterion_group!(
    benches_2q,
    sim_cx_gate,
    sim_cz_gate,
    sim_rxx_gate,
    sim_ryy_gate,
    sim_rzz_gate
);

criterion_group!(benches_rand_1q, sim_1k_1q_gates, sim_5k_1q_gates);
criterion_group!(benches_rand_2q, sim_1k_2q_gates);
criterion_main!(benches_1q, benches_2q, benches_rand_1q, benches_rand_2q);
