// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Construction throughput of the store's native paths.
//!
//! This measures the Rust ceiling. The target derived in the construction
//! benchmark is a Python-level number, so these figures are not a like-for-like
//! comparison: they bound what any Python surface over this store can reach.
//! The comparable measurement arrives with the Python binding.

use std::time::Instant;

use qsc_circuit_store::{CircuitStore, Qubit, StandardGate};

const GATES: usize = 1_000_000;
const SAMPLES: usize = 5;

/// Throughput at the fastest observed run, since benchmark noise only ever
/// makes a run slower.
fn best_gates_per_second<F: FnMut() -> CircuitStore>(mut build: F) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let store = build();
        let elapsed = start.elapsed().as_secs_f64();
        assert_eq!(store.len(), GATES, "benchmark must build the full circuit");
        best = best.min(elapsed);
    }
    #[allow(clippy::cast_precision_loss)]
    let gates = GATES as f64;
    gates / best
}

fn per_gate_single_qubit() -> CircuitStore {
    let mut store = CircuitStore::new(32, 0);
    store.reserve(GATES);
    for index in 0..GATES {
        let qubit = Qubit(u32::try_from(index % 32).expect("within range"));
        store
            .push_standard(StandardGate::H, &[qubit])
            .expect("gate should append");
    }
    store
}

fn per_gate_repeated_pair() -> CircuitStore {
    let mut store = CircuitStore::new(32, 0);
    store.reserve(GATES);
    for _ in 0..GATES {
        store
            .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
            .expect("gate should append");
    }
    store
}

fn bulk_repeated_pair() -> CircuitStore {
    let operands = [Qubit(0), Qubit(1)];
    let batch: Vec<&[Qubit]> = (0..GATES).map(|_| operands.as_slice()).collect();
    let mut store = CircuitStore::new(32, 0);
    store.reserve(GATES);
    store
        .extend_standard(StandardGate::Cx, batch.iter().copied())
        .expect("batch should append");
    store
}

fn main() {
    println!("Store construction throughput, {GATES} gates, best of {SAMPLES} samples.");
    println!();
    println!("{:<28} {:>18}", "path", "gates/second");
    println!("{}", "-".repeat(48));
    for (name, throughput) in [
        (
            "per-gate single_qubit",
            best_gates_per_second(per_gate_single_qubit),
        ),
        (
            "per-gate repeated_pair",
            best_gates_per_second(per_gate_repeated_pair),
        ),
        (
            "bulk repeated_pair",
            best_gates_per_second(bulk_repeated_pair),
        ),
    ] {
        println!("{name:<28} {throughput:>18.0}");
    }
    println!();
    println!("These are native Rust figures and bound what a Python surface can reach.");
}
