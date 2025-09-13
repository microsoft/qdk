#![allow(clippy::unit_arg)]

use criterion::{Criterion, criterion_group, criterion_main};
use rand::{SeedableRng, distributions::Uniform, prelude::Distribution, rngs::StdRng};
use stabilizer_simulator::{QubitID, Simulator, noise_config::NoiseConfig, operation::*};
use std::hint::black_box;

const SEED: u64 = 1000;
const NUM_QUBITS: usize = 1_224;

fn random_qubit(rng: &mut StdRng) -> QubitID {
    let distr = Uniform::new(0, usize::MAX);
    distr.sample(rng) % NUM_QUBITS
}

fn gate(rng: &mut StdRng) -> Operation {
    let distr = Uniform::new(0, usize::MAX);
    let gate = distr.sample(rng) % 8;

    match gate {
        0 => id(random_qubit(rng)),
        1 => x(random_qubit(rng)),
        2 => y(random_qubit(rng)),
        3 => z(random_qubit(rng)),
        4 => h(random_qubit(rng)),
        5 => s(random_qubit(rng)),
        6 => cz(random_qubit(rng), random_qubit(rng)),
        7 => mov(random_qubit(rng)),
        _ => unreachable!(),
    }
}

fn random_gates(num_gates: usize) -> Vec<Operation> {
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut gates: Vec<Operation> = Vec::with_capacity(num_gates);
    for _ in 0..num_gates {
        gates.push(gate(&mut rng));
    }
    for q in 0..NUM_QUBITS {
        gates.push(mz(q));
    }
    gates
}

fn sim_1k_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("1k gates", |b| {
        b.iter(|| {
            let mut simulator = Simulator::new(NUM_QUBITS, NoiseConfig::NOISELESS);
            black_box(simulator.apply_gates(black_box(&gates)))
        })
    });
}

fn sim_20k_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 20_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("20k gates", |b| {
        b.iter(|| {
            let mut simulator = Simulator::new(NUM_QUBITS, NoiseConfig::NOISELESS);
            black_box(simulator.apply_gates(black_box(&gates)))
        })
    });
}

fn sim_1m_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000_000;
    let gates = random_gates(NUM_GATES);
    c.bench_function("1m gates", |b| {
        b.iter(|| {
            let mut simulator = Simulator::new(NUM_QUBITS, NoiseConfig::NOISELESS);
            black_box(simulator.apply_gates(black_box(&gates)))
        })
    });
}

criterion_group!(benches, sim_1k_gates, sim_20k_gates, sim_1m_gates);
criterion_main!(benches);
