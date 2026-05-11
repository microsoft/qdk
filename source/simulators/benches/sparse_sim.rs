// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_complex::Complex64;
use num_traits::One;
use std::f64::consts::PI;

use criterion::{Criterion, criterion_group, criterion_main};
use qdk_simulators::SparseStateSim;

/// The number of qubits to use for benchmarking single qubit gates. We want enough qubits to have a
/// decent size state vector. The qubit targetted for the gate will be `NUM_QUBITS` + 1.
const NUM_QUBITS: usize = 7;

macro_rules! bench_single_qubit_gate {
    ($c:ident, $qir_gate:expr, $desc:expr) => {
        $c.bench_function($desc, |b| {
            let mut sim = SparseStateSim::default();
            // Allocate additional qubits, apply H to each, and get the state to force the simulator to
            // have a decent size state vector before benchmarking the gate operation.
            let mut last_q = 0;
            for _ in 0..=NUM_QUBITS {
                let q = sim.allocate();
                sim.h(q);
                last_q = q;
            }
            let _ = sim.get_state();
            b.iter(|| {
                $qir_gate(&mut sim, last_q);
                // Force a flush of the operations by allocating a qubit and using a phase gate on it.
                let q = sim.allocate();
                sim.mcphase(&[q], Complex64::one(), last_q);
                sim.release(q);
            })
        });
    };
}

macro_rules! bench_single_qubit_rotation {
    ($c:ident, $qir_gate:expr, $desc:expr) => {
        $c.bench_function($desc, |b| {
            let mut sim = SparseStateSim::default();
            // Allocate additional qubits, apply H to each, and get the state to force the simulator to
            // have a decent size state vector before benchmarking the gate operation.
            let mut last_q = 0;
            for _ in 0..=NUM_QUBITS {
                let q = sim.allocate();
                sim.h(q);
                last_q = q;
            }
            let _ = sim.get_state();
            b.iter(|| {
                $qir_gate(&mut sim, PI / 7.0, last_q);
                // Force a flush of the operations by allocating a qubit and using a phase gate on it.
                let q = sim.allocate();
                sim.mcphase(&[q], Complex64::one(), last_q);
                sim.release(q);
            })
        });
    };
}

pub fn x_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::x, "X Gate");
}

pub fn y_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::y, "Y Gate");
}

pub fn z_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::z, "Z Gate");
}

pub fn h_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::h, "H Gate");
}

pub fn s_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::s, "S Gate");
}

pub fn sadj_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::sadj, "S Adj Gate");
}

pub fn t_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::t, "T Gate");
}

pub fn tadj_gate(c: &mut Criterion) {
    bench_single_qubit_gate!(c, SparseStateSim::tadj, "T Adj Gate");
}

pub fn rx_gate(c: &mut Criterion) {
    bench_single_qubit_rotation!(c, SparseStateSim::rx, "Rx Gate");
}

pub fn ry_gate(c: &mut Criterion) {
    bench_single_qubit_rotation!(c, SparseStateSim::ry, "Ry Gate");
}

pub fn rz_gate(c: &mut Criterion) {
    bench_single_qubit_rotation!(c, SparseStateSim::rz, "Rz Gate");
}

/// Benchmarks large number of qubit allocations and releases.
pub fn allocate_release(c: &mut Criterion) {
    c.bench_function("Allocate-Release 2k qubits", |b| {
        let mut sim = SparseStateSim::default();
        b.iter(|| {
            let mut qubits = Vec::new();
            for _ in 0..2000 {
                qubits.push(sim.allocate());
            }
            for q in qubits {
                sim.release(q);
            }
        });
    });
}

criterion_group!(
    benches,
    x_gate,
    y_gate,
    z_gate,
    h_gate,
    s_gate,
    sadj_gate,
    t_gate,
    tadj_gate,
    rx_gate,
    ry_gate,
    rz_gate,
    allocate_release
);
criterion_main!(benches);
