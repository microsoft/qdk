// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Parity with M0's ④ (L10). M0 cannot run here (it needs cuTensorNet and
//! its own branch), so its seed derivation is copied verbatim, and the
//! sampling it did is recomputed directly on the engine: the stack must
//! give the same values, value for value.

use std::rc::Rc;

use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use tensornet::{MpsEngine, reference::ExactReference};

use super::{
    Bytecode,
    cases::{bell_samples_are_correlated, bits, trotter_matches_m0, x_pattern_reads_back},
};
use crate::{MpsStack, mps_capacity, program::plan, sampler_seed};

/// M0's `derive_sampler_seed` for `Some(seed)`, verbatim
/// (`cutensornet_mps/execution.rs`).
fn derive_sampler_seed(seed: Option<u32>) -> i32 {
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_rng(&mut rand::rng())
    };
    rng.random_range(1..=i32::MAX)
}

#[test]
fn the_sampler_seed_is_m0s() {
    for seed in (0..2000).chain([u32::MAX - 1, u32::MAX, 1 << 31, 0xDEAD_BEEF]) {
        let m0 = derive_sampler_seed(Some(seed));
        let m1 = sampler_seed(u64::from(seed)).expect("in range");
        assert_eq!(i64::from(m1.get()), i64::from(m0), "seed {seed}");
    }
}

#[test]
fn the_stack_samples_what_m0_sampled() {
    // M0: one state from the region, the distinct measured qubits in
    // first-measurement order as the sampled sites, one `sample` of N shots
    // with the derived seed; each shot's records from its row.
    let program = super::cases::scrambled(4, 3)
        .mz(2, 0)
        .mz(0, 1)
        .mz(3, 2)
        .mz(0, 3)
        .record(3)
        .record(0)
        .record(1)
        .record(2)
        .build();
    let engine = ExactReference::new();
    let base = plan(&program).expect("a Base program");
    assert_eq!(base.columns, [2, 0, 3]);
    for seed in [0, 1, 42, 4_000_000_000] {
        let shots = 300;
        let mut state = engine
            .prepare(&mps_capacity(4, 128).expect("a chain"))
            .expect("prepares");
        for operation in base.operations() {
            crate::lowering::apply_operation(&engine, &mut state, operation).expect("applies");
        }
        let flat = engine
            .sample(
                &state,
                &base.columns,
                shots,
                sampler_seed(u64::from(seed)).expect("in range"),
            )
            .expect("samples");
        // Records: result 3 (qubit 0), result 0 (qubit 2), 1 (qubit 0), 2 (qubit 3).
        let expected: Vec<Vec<bool>> = flat
            .chunks(3)
            .map(|row| vec![row[1], row[0], row[1], row[2]])
            .collect();
        let stack = MpsStack::shared(Rc::new(engine));
        let sampled = bits(
            &stack
                .sample(&program, u32::try_from(shots).expect("small"), Some(seed))
                .expect("samples"),
        );
        assert_eq!(sampled, expected, "seed {seed}");
    }
}

#[test]
fn the_same_seed_gives_the_same_shots_and_another_seed_others() {
    let stack = MpsStack::new(ExactReference::new());
    let program = super::cases::scrambled(4, 8)
        .measure_all(&[0, 1, 2, 3])
        .build();
    let run = |seed| bits(&stack.sample(&program, 200, seed).expect("samples"));
    assert_eq!(run(Some(5)), run(Some(5)));
    assert_ne!(run(Some(5)), run(Some(6)));
    // Without a seed, one is drawn: two runs differ (at 2⁻²⁰⁰-ish odds).
    assert_eq!(run(None).len(), 200);
}

#[test]
fn deterministic_programs_give_their_records() {
    let stack = MpsStack::new(ExactReference::new());
    for width in [2, 3, 12] {
        x_pattern_reads_back(&stack, width);
    }
    let program = Bytecode::new(3)
        .x(0)
        .x(2)
        .cx(2, 1)
        .measure_all(&[0, 1, 2])
        .build();
    let rows = bits(&stack.sample(&program, 5, Some(9)).expect("samples"));
    assert_eq!(rows, vec![vec![true, true, true]; 5]);
}

#[test]
fn bell_frequencies() {
    bell_samples_are_correlated(&MpsStack::new(ExactReference::new()), 4000);
}

#[test]
fn trotter_expectations_are_m0s_pinned_values() {
    // M0's B2 values, from cuTensorNet at χ = 128 and its base policy; the
    // reference is exact, and agrees to M0's relative 1e-9.
    let stack = MpsStack::new(ExactReference::new());
    trotter_matches_m0(&stack, 12, 8, 4.332_869_154_633, 1e-9);
    trotter_matches_m0(&stack, 16, 8, 6.347_012_657_087, 1e-9);
}
