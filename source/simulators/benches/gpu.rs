#![allow(clippy::unit_arg)]

use criterion::{Criterion, criterion_group, criterion_main};
use qdk_simulators::run_gpu_simulator;
use qdk_simulators::shader_types::Op;
use qdk_simulators::shader_types::ops;
use rand::{SeedableRng, distributions::Uniform, prelude::Distribution, rngs::StdRng};
use std::hint::black_box;

const SEED: u64 = 1000;
const NUM_QUBITS: u32 = 10;

fn random_qubit(rng: &mut StdRng) -> u32 {
    let distr = Uniform::new(0, u32::MAX);
    distr.sample(rng) % NUM_QUBITS
}

fn gate_op(id: u32, q1: u32, q2: u32, q3: u32, angle: f32) -> Op {
    Op {
        id,
        q1,
        q2,
        q3,
        angle,
        padding: [0; 204],
        _00r: 0.0,
        _00i: 0.0,
        _01r: 0.0,
        _01i: 0.0,
        _10r: 0.0,
        _10i: 0.0,
        _11r: 0.0,
        _11i: 0.0,
    }
}

fn m_every_z() -> Op {
    gate_op(ops::MEVERYZ, 0, 0, 0, 0.0)
}

fn one_qubit_gate(id: u32, qubit: u32) -> Op {
    gate_op(id, qubit, 0, 0, 0.0)
}

fn gate(rng: &mut StdRng) -> Op {
    let distr = Uniform::new(0, usize::MAX);
    let gate = distr.sample(rng) % 11;

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
        _ => unreachable!(),
    }
}

fn random_gates(num_gates: usize) -> Vec<Op> {
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut gates: Vec<Op> = Vec::with_capacity(num_gates);
    for _ in 0..num_gates {
        gates.push(gate(&mut rng));
    }
    gates.push(m_every_z());

    gates
}

fn sim_1k_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("1k gates", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

fn sim_5k_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 5_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("5k gates", |b| {
        b.iter(|| black_box(run_gpu_simulator(NUM_QUBITS, gates.clone())));
    });
}

criterion_group!(benches, sim_1k_gates, sim_5k_gates);
criterion_main!(benches);
