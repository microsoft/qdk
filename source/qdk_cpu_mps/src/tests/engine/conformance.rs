// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine passes the contract's conformance kit, with and without
//! fusion, and agrees with the exact reference on a long circuit when
//! nothing is truncated.

use super::*;

/// The engine as `default()` makes it, fusing the gates on a pair, passes
/// every case of the contract's conformance kit; a failure names its case.
#[test]
fn conforms_with_fusion() {
    let failures = tensornet::conformance::check(&CpuMpsEngine::default());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The engine without fusion, splitting after each gate, passes every case
/// of the contract's conformance kit; a failure names its case.
#[test]
fn conforms_without_fusion() {
    let failures = tensornet::conformance::check(&CpuMpsEngine::default().with_fusion(false));
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A pseudo-random circuit on 8 sites, mostly on sites far apart, with
/// projections on the way: every read agrees with the reference. The engine
/// must not truncate: a cutoff of `1e-14` over its hundreds of splits moves
/// a probability by about `1e-9`.
#[test]
fn a_long_circuit_agrees_with_the_reference_without_truncation() {
    let sites = 8;
    let reference = ExactReference::new();
    for (engine, name) in both_fusions(0.0) {
        let mut state = engine.prepare(&chain(sites, 64)).expect("prepare");
        let mut exact_state = reference.prepare(&exact(sites)).expect("prepare");
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = |below: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            usize::try_from(seed % u64::try_from(below).expect("small")).expect("small")
        };
        let agree = |state: &CanonicalMps, exact_state: &ReferenceState| {
            assert_reads_agree(&engine, state, exact_state, 1e-10);
            let value = engine.expectation(state, &mixed_string()).expect("value");
            let expected = reference
                .expectation(exact_state, &mixed_string())
                .expect("value");
            assert!(
                (value - expected).norm() < 1e-10,
                "{name}: {value} vs {expected}"
            );
        };
        for step in 0..120 {
            let x = f64::from(u32::try_from(step).expect("small"));
            let a = next(sites);
            let b = (a + 1 + next(sites - 1)) % sites;
            let gate = if step % 3 == 0 {
                cnot()
            } else {
                entangler(0.3 + 0.05 * x)
            };
            engine.apply(&mut state, &[a, b], &gate).expect("apply");
            reference
                .apply(&mut exact_state, &[a, b], &gate)
                .expect("apply");
            let turn = ry(0.2 + 0.01 * x);
            engine.apply(&mut state, &[a], &turn).expect("apply");
            reference
                .apply(&mut exact_state, &[a], &turn)
                .expect("apply");
            if step % 40 == 39 {
                agree(&state, &exact_state);
                let site = next(sites);
                let p1 = reference
                    .probability_of_one(&exact_state, site)
                    .expect("P(One)");
                let one = p1 > 0.5;
                engine.project(&mut state, site, one).expect("project");
                reference
                    .project(&mut exact_state, site, one)
                    .expect("project");
            }
        }
        agree(&state, &exact_state);
        let shape = engine.shape(&state).expect("shape");
        assert!(shape.fits_within(&chain(sites, 64)), "{name}: {shape:?}");
    }
}
