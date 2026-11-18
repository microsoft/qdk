// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine agrees with the exact reference on a long circuit when
//! nothing is truncated, with and without fusion, and stays within the
//! distance its truncations allow when they drop weight.

use super::*;

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
        let mut next = xorshift(0x2545_F491_4F6C_DD1D);
        let agree = |state: &State, exact_state: &ReferenceState| {
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
            apply_to_both(&engine, &mut state, &mut exact_state, &[a, b], &gate);
            let turn = ry(0.2 + 0.01 * x);
            apply_to_both(&engine, &mut state, &mut exact_state, &[a], &turn);
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

/// A circuit of weak entangling gates on 8 sites, mostly far apart, at a
/// bond of 4, which truncates: the distribution of all eight qubits stays
/// within the distance the splits' kept shares allow of the reference's.
///
/// The bound holds on any backend that keeps the contract. A split of a
/// normalised state `ψ` that drops a share `ε` of its squared Schmidt
/// values and renormalises maps it to `Pψ / ‖Pψ‖`, at distance
/// `√(2 − 2√(1 − ε)) ≤ √(2ε)`; gates and moves of the centre keep
/// distances, so after splits dropping `ε₁, ε₂, …` the state is within
/// `Σ √(2εᵢ)` of the exact one, up to a global phase, and the total
/// variation distance of its distribution is at most that. The retained
/// weight is `W = Π (1 − εᵢ)`, so `Σ εᵢ ≤ −ln W`, and over at most `k`
/// splits, the statistics' count, `Σ √(2εᵢ) ≤ √(2k · (−ln W))`. The circuit
/// projects nothing: a projection divides by a probability, which can
/// enlarge the distance.
#[test]
fn a_truncated_circuit_stays_within_the_distance_its_splits_allow() {
    let sites = 8;
    let engine = Engine::new(CpuAlgebra).with_cutoff(0.0);
    let reference = ExactReference::new();
    let mut state = engine.prepare(&chain(sites, 4)).expect("prepare");
    let mut exact_state = reference.prepare(&exact(sites)).expect("prepare");
    let mut next = xorshift(0x2545_F491_4F6C_DD1D);
    for step in 0..20 {
        let x = f64::from(u32::try_from(step).expect("small"));
        let a = next(sites);
        let b = (a + 1 + next(sites - 1)) % sites;
        let gate = entangler(0.05 + 0.01 * x);
        apply_to_both(&engine, &mut state, &mut exact_state, &[a, b], &gate);
        let turn = ry(0.2 + 0.01 * x);
        apply_to_both(&engine, &mut state, &mut exact_state, &[a], &turn);
    }
    let weight = engine.retained_weight(&state).expect("weight");
    let splits = state.split_statistics().splits();
    let bound = (2.0 * f64::from(u32::try_from(splits).expect("few splits")) * -weight.ln()).sqrt();
    let tvd: f64 = distribution(&engine, &state, sites)
        .iter()
        .zip(distribution(&reference, &exact_state, sites))
        .map(|(p, q)| 0.5 * (p - q).abs())
        .sum();
    assert!(
        tvd > 1e-5,
        "precondition: the cap truncates, so the distributions differ \
         (TVD {tvd:e})"
    );
    assert!(
        bound < 0.2,
        "precondition: the bound is far below 1, so the check is not \
         vacuous (bound {bound:e})"
    );
    assert!(tvd <= bound, "TVD {tvd:e} above the bound {bound:e}");
}

/// The probability of every outcome of `sites` qubits, the outcome's bit
/// `q` the value of qubit `q`: the product of the probabilities of its
/// values, each projected on a fork in turn.
fn distribution<E: MpsEngine>(engine: &E, state: &E::State, sites: usize) -> Vec<f64> {
    let mut out = vec![0.0; 1 << sites];
    let fork = engine.fork(state).expect("fork");
    branch(engine, &fork, 0, 0, 1.0, &mut out);
    out
}

/// Fills `out` with the outcomes that extend `prefix`, the values of the
/// qubits below `site`, which `state` is projected on with probability `p`.
fn branch<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    site: usize,
    prefix: usize,
    p: f64,
    out: &mut [f64],
) {
    if 1 << site == out.len() {
        out[prefix] = p;
        return;
    }
    let p_one = engine.probability_of_one(state, site).expect("P(One)");
    for (one, possible) in [(false, p_one < 1.0), (true, p_one > 0.0)] {
        if possible {
            let mut fork = engine.fork(state).expect("fork");
            let q = engine.project(&mut fork, site, one).expect("project");
            branch(
                engine,
                &fork,
                site + 1,
                prefix | usize::from(one) << site,
                p * q,
                out,
            );
        }
    }
}

/// Applies `gate` on `qubits` to the engine's state and to the reference's.
fn apply_to_both(
    engine: &Engine,
    state: &mut State,
    exact_state: &mut ReferenceState,
    qubits: &[usize],
    gate: &[Complex64],
) {
    engine.apply(state, qubits, gate).expect("apply");
    ExactReference::new()
        .apply(exact_state, qubits, gate)
        .expect("apply");
}

/// A xorshift generator from `seed`: each call gives a number below its
/// argument, the same sequence on every run.
fn xorshift(mut seed: u64) -> impl FnMut(usize) -> usize {
    move |below| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % u64::try_from(below).expect("small")).expect("small")
    }
}
