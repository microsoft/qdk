// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unit_arg)]

use criterion::{Criterion, criterion_group, criterion_main};
use qdk_simulators::{
    QubitID, Simulator as _,
    noise_config::{
        CumulativeNoiseConfig, IdleNoiseParams, NoiseConfig, NoiseTable, const_empty_hash_map,
    },
    stabilizer_simulator::{
        StabilizerSimulator,
        noise::Fault,
        operation::{Operation, cz, h, id, mov, mz, s, x, y, z},
    },
};
use rand::{SeedableRng, distributions::Uniform, prelude::Distribution, rngs::StdRng};
use std::{hint::black_box, sync::Arc};

const SEED: u32 = 1000;
const NUM_QUBITS: usize = 1_224;
const NOISE_CONFIG: NoiseConfig<f64, f64> = NoiseConfig {
    idle: IdleNoiseParams {
        s_probability: 0.01,
    },
    i: NoiseTable::noiseless(1),
    x: NoiseTable::noiseless(1),
    y: NoiseTable::noiseless(1),
    z: NoiseTable::noiseless(1),
    h: NoiseTable::noiseless(1),
    s: NoiseTable::noiseless(1),
    s_adj: NoiseTable::noiseless(1),
    t: NoiseTable::noiseless(1),
    t_adj: NoiseTable::noiseless(1),
    sx: NoiseTable::noiseless(1),
    sx_adj: NoiseTable::noiseless(1),
    rx: NoiseTable::noiseless(1),
    ry: NoiseTable::noiseless(1),
    rz: NoiseTable::noiseless(1),
    cx: NoiseTable::noiseless(2),
    cz: NoiseTable::noiseless(2),
    rxx: NoiseTable::noiseless(2),
    ryy: NoiseTable::noiseless(2),
    rzz: NoiseTable::noiseless(2),
    swap: NoiseTable::noiseless(2),
    mov: NoiseTable::noiseless(1),
    mresetz: NoiseTable::noiseless(1),
    intrinsics: const_empty_hash_map(),
};

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
    let mut rng = StdRng::seed_from_u64(u64::from(SEED));
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
    let noise: Arc<CumulativeNoiseConfig<Fault>> = Arc::new(NOISE_CONFIG.into());
    c.bench_function("1k gates", |b| {
        b.iter(|| {
            let mut simulator =
                StabilizerSimulator::new(NUM_QUBITS, NUM_QUBITS, SEED, noise.clone());
            black_box(simulator.apply_gates(black_box(&gates)));
        });
    });
}

fn sim_20k_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 20_000;
    let gates = random_gates(NUM_GATES);
    let noise: Arc<CumulativeNoiseConfig<Fault>> = Arc::new(NOISE_CONFIG.into());
    c.bench_function("20k gates", |b| {
        b.iter(|| {
            let mut simulator =
                StabilizerSimulator::new(NUM_QUBITS, NUM_QUBITS, SEED, noise.clone());
            black_box(simulator.apply_gates(black_box(&gates)));
        });
    });
}

fn sim_1m_gates(c: &mut Criterion) {
    const NUM_GATES: usize = 1_000_000;
    let gates = random_gates(NUM_GATES);
    let noise: Arc<CumulativeNoiseConfig<Fault>> = Arc::new(NOISE_CONFIG.into());
    c.bench_function("1m gates", |b| {
        b.iter(|| {
            let mut simulator =
                StabilizerSimulator::new(NUM_QUBITS, NUM_QUBITS, SEED, noise.clone());
            black_box(simulator.apply_gates(black_box(&gates)));
        });
    });
}

criterion_group!(benches, sim_1k_gates, sim_20k_gates, sim_1m_gates);
criterion_main!(benches);
