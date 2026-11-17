// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fusion: the two-site gates on up to three sites, neighbours or not,
//! gather into one pending window, which the first operation it does not
//! serve applies at once; every read sees its gates; and an unfused gate
//! leaves the centre where it came from.

use super::*;

/// A caller that takes the engine by `new`: two CNOTs on one pair,
/// then a read, take one split and no matrix product operator.
#[test]
fn new_fuses() {
    let engine = Engine::new(CpuAlgebra);
    let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.7)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.probability_of_one(&state, 1).expect("P(One)");
    let statistics = state.split_statistics();
    assert_eq!((statistics.splits(), statistics.mpo_applications()), (1, 0));
}

/// Grouped shots fork a state whose last gates left a window pending: the
/// window is applied once, in the state forked, so neither that state nor
/// its forks apply it again when read.
#[test]
fn a_fork_applies_the_pending_window_once() {
    let engine = Engine::new(CpuAlgebra);
    let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.7)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
    assert_eq!(state.split_statistics().splits(), 0, "a pending window");
    let forks = [
        engine.fork(&state).expect("fork"),
        engine.fork(&state).expect("fork"),
    ];
    for read in forks.iter().chain([&state]) {
        assert_eq!(read.split_statistics().splits(), 2, "after the forks");
        engine.probability_of_one(read, 1).expect("P(One)");
        assert_eq!(read.split_statistics().splits(), 2, "after a read");
    }
}

/// A gate on sites 1 and 2 of 4, left pending, then a read of either of
/// its sites: with the centre on site 0, a read of site 2; with the centre
/// on site 3, where a read of site 3 had left it, a read of site 1. The
/// read moves the centre onto the pair's site nearer it, one move, and
/// splits the pair toward the site it reads, which leaves the centre
/// there, so the centre makes no other move. With the centre already on
/// site 1, where a read of site 1 had left it, a read of site 2 makes no
/// move before the split.
#[test]
fn a_read_applies_the_pending_window_toward_its_site() {
    for (first_read, read, expected) in [(None, 2, 1), (Some(3), 1, 1), (Some(1), 2, 0)] {
        let engine = Engine::new(CpuAlgebra);
        let mut state = engine.prepare(&chain(4, 2)).expect("prepare");
        if let Some(site) = first_read {
            engine.probability_of_one(&state, site).expect("P(One)");
        }
        let moves = state.split_statistics().centre_moves();
        engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
        engine.probability_of_one(&state, read).expect("P(One)");
        let statistics = state.split_statistics();
        assert_eq!(statistics.splits(), 1, "precondition, the pair's split");
        assert_eq!(
            statistics.centre_moves() - moves,
            expected,
            "a read of site {read}"
        );
    }
}

/// An operation on a state that reads it.
type Operation = fn(&Engine, &State);

/// The expectation of Z on site 0.
fn expectation(engine: &Engine, state: &State) {
    engine
        .expectation(state, &z_string(&[0]))
        .expect("an expectation");
}

/// P(One) of site 1 given site 0 found Zero, on a fork.
fn conditional_read(engine: &Engine, state: &State) {
    engine
        .conditional_probability_of_one(state, 1, &[(0, false)])
        .expect("P(One | given)");
}

/// A gate left pending with the centre where a read had left it, then an
/// expectation or a conditional read with a condition, which forks: each
/// applies the window toward the centre. A gate on sites 1 and 2 of 4 with
/// the centre on site 3 ends it on site 2, where a conditional read of
/// site 1 would end it on site 1 had it applied the window toward the site
/// it reads; a gate on sites 0 and 2 with the centre on site 1, as near
/// one end of the span as the other, ends it on the lower end, site 0. So
/// a read of that site after the operation makes no move.
#[test]
fn an_expectation_or_a_fork_applies_the_pending_window_toward_the_centre() {
    let rows: [(Operation, [usize; 2], usize, usize, u64); 3] = [
        (expectation, [1, 2], 3, 2, 1),
        (expectation, [0, 2], 1, 0, 2),
        (conditional_read, [1, 2], 3, 2, 1),
    ];
    for (operation, on, centre, end, splits) in rows {
        let engine = Engine::new(CpuAlgebra);
        let mut state = engine.prepare(&chain(4, 2)).expect("prepare");
        engine.probability_of_one(&state, centre).expect("P(One)");
        engine.apply(&mut state, &on, &cnot()).expect("apply");
        operation(&engine, &state);
        let moves = state.split_statistics().centre_moves();
        engine.probability_of_one(&state, end).expect("P(One)");
        let statistics = state.split_statistics();
        assert_eq!(
            statistics.splits(),
            splits,
            "precondition, the window's splits"
        );
        assert_eq!(statistics.centre_moves(), moves, "a gate on {on:?}");
    }
}

/// The state, splits and matrix product operators of `gates` (on 5 sites
/// of bond 8, after a turn of each site), with and without fusion: each
/// site's P(One), then the two counts.
fn fused_and_unfused(gates: &[(&[usize], Vec<Complex64>)]) -> [(Vec<f64>, (u64, u64)); 2] {
    [false, true].map(|fuses| {
        let engine = Engine::new(CpuAlgebra).with_fusion(fuses);
        let mut state = engine.prepare(&chain(5, 8)).expect("prepare");
        for (site, angle) in [0.3, 0.4, 0.5, 0.6, 0.7].into_iter().enumerate() {
            engine
                .apply(&mut state, &[site], &ry(angle))
                .expect("apply");
        }
        for (on, gate) in gates {
            engine.apply(&mut state, on, gate).expect("apply");
        }
        let p = (0..5)
            .map(|site| engine.probability_of_one(&state, site).expect("P(One)"))
            .collect();
        let statistics = state.split_statistics();
        (p, (statistics.splits(), statistics.mpo_applications()))
    })
}

/// The counts of the unfused and the fused run of `gates`, checked to reach
/// the same state.
fn counts(gates: &[(&[usize], Vec<Complex64>)]) -> [(u64, u64); 2] {
    let [(p0, unfused), (p1, fused)] = fused_and_unfused(gates);
    assert!(
        p0.iter().zip(&p1).all(|(a, b)| (a - b).abs() < 1e-12),
        "precondition: the fused and unfused runs reach the same state"
    );
    [unfused, fused]
}

/// CNOT, a phase on its target, a turn of its control, CNOT again, all on
/// one pair: one split with fusion, two without, for the same state.
#[test]
fn fuses_the_gates_on_one_pair_into_one_split() {
    let gates: &[(&[usize], Vec<Complex64>)] = &[
        (&[1, 2], cnot()),
        (&[2], phase(0.3)),
        (&[1], ry(0.2)),
        (&[1, 2], cnot()),
    ];
    assert_eq!(counts(gates), [(2, 0), (1, 0)]);
}

/// A CNOT ladder on sites 0, 1 and 2, a turn of site 1 and a CNOT on sites
/// 2 and 0: one window, applied as one matrix product operator across the
/// three sites, two splits; unfused, a split per gate on neighbours and an
/// operator of two splits for the gate on sites 0 and 2.
#[test]
fn fuses_the_gates_on_three_sites_into_one_window() {
    let gates: &[(&[usize], Vec<Complex64>)] = &[
        (&[0, 1], cnot()),
        (&[1, 2], entangler(0.4)),
        (&[1], ry(0.9)),
        (&[2, 0], cnot()),
    ];
    assert_eq!(counts(gates), [(4, 1), (2, 1)]);
}

/// Gates on sites (0, 1), (1, 2), then (1, 3): the third would make the
/// window's fourth site, so the window on sites 0 to 2 is applied, two
/// splits, and the third gate opens a window of its own, on sites 1 and 3,
/// another two.
#[test]
fn a_gate_that_would_make_a_fourth_site_opens_a_new_window() {
    let gates: &[(&[usize], Vec<Complex64>)] = &[
        (&[0, 1], cnot()),
        (&[1, 2], entangler(0.4)),
        (&[1, 3], entangler(-0.7)),
    ];
    assert_eq!(counts(gates), [(4, 1), (4, 2)]);
}

/// A window on sites 1 and 4 of 6, then a gate on sites 2 and 5, which
/// does not fit: the window is applied toward the gate's lower site, 2,
/// which leaves the centre on site 1, so the new window on sites 2 to 5
/// takes one move to reach its span. Applied toward site 5, the centre
/// would be on site 4, inside the new span, one move fewer. So the gate's
/// operands in either order, and the centre on site 0 or, after a read,
/// on site 5, all take the same moves: the direction is neither an
/// operand's place nor the centre's. The lower site was chosen by the H₂
/// and polyene runs, where it took the fewest moves or the least time.
#[test]
fn a_gate_that_does_not_fit_applies_the_window_toward_its_lower_site() {
    // Each window: the moves into its span, then 2 across it. Toward site
    // 2, 1 + 2 for the first and 1 + 2 for the second; toward site 5,
    // 1 + 2 and 0 + 2. With the centre on site 5, the first window's
    // moves into its span start from there.
    for (first_read, on) in [(None, [5, 2]), (None, [2, 5]), (Some(5), [2, 5])] {
        let (algebra, calls) = SpyAlgebra::new();
        let engine = CanonicalMpsEngine::new(algebra).with_fusion(true);
        let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        if let Some(site) = first_read {
            engine.probability_of_one(&state, site).expect("P(One)");
        }
        let before = calls.qr_moves.get();
        engine.apply(&mut state, &[1, 4], &cnot()).expect("apply");
        assert_eq!(
            calls.qr_moves.get(),
            before,
            "precondition: the window is pending"
        );
        engine
            .apply(&mut state, &on, &entangler(0.4))
            .expect("apply");
        engine.retained_weight(&state).expect("weight");
        assert_eq!(
            calls.qr_moves.get() - before,
            6,
            "a gate on {on:?}, the centre first on {first_read:?}"
        );
        assert_eq!(
            state.split_statistics().mpo_applications(),
            2,
            "precondition"
        );
    }
}

/// A read of a state and of the reference's, which checks they agree.
type Read = fn(&Engine, &mut State, &ExactReference, &mut ReferenceState);

/// A window on sites 1, 3 and 4 of 6, with a turn of site 2 between its
/// sites and one-site gates before and after its gates, left pending; then
/// each read, on a state of its own: every read agrees with the reference,
/// so each sees every gate, the window's and those waiting on their sites,
/// in the caller's order.
#[test]
fn every_read_sees_the_pending_windows_gates() {
    let reference = ExactReference::new();
    let gates: [(&[usize], Vec<Complex64>); 9] = [
        (&[1], ry(0.8)),
        (&[3], ry(-1.1)),
        (&[4], ry(0.5)),
        (&[3, 1], entangler(0.7)),
        (&[2], ry(1.2)),
        (&[3], phase(0.4)),
        (&[4, 3], cnot()),
        (&[1], ry(0.6)),
        (&[3, 4], entangler(-0.3)),
    ];
    let reads: [(&str, Read); 5] = [
        ("P(One)", |engine, state, reference, exact_state| {
            assert_reads_agree(engine, state, exact_state, 1e-12);
            let _ = reference;
        }),
        ("conditional", |engine, state, reference, exact_state| {
            let given = [(4, true), (1, false)];
            let p = engine
                .conditional_probability_of_one(state, 3, &given)
                .expect("P(One | given)");
            let q = reference
                .conditional_probability_of_one(exact_state, 3, &given)
                .expect("P(One | given)");
            assert!((p - q).abs() < 1e-12, "{p} vs {q}");
        }),
        ("expectation", |engine, state, reference, exact_state| {
            let value = engine
                .expectation(state, &mixed_string())
                .expect("expectation");
            let expected = reference
                .expectation(exact_state, &mixed_string())
                .expect("expectation");
            assert!((value - expected).norm() < 1e-12, "{value} vs {expected}");
        }),
        ("projection", |engine, state, reference, exact_state| {
            let p = engine.project(state, 3, true).expect("project");
            let q = reference.project(exact_state, 3, true).expect("project");
            assert!((p - q).abs() < 1e-12, "{p} vs {q}");
            assert_reads_agree(engine, state, exact_state, 1e-12);
        }),
        ("fork", |engine, state, _, exact_state| {
            let fork = engine.fork(state).expect("fork");
            assert_reads_agree(engine, &fork, exact_state, 1e-12);
        }),
    ];
    for (name, read) in reads {
        for (engine, fusion) in both_fusions(DEFAULT_CUTOFF) {
            let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
            let mut exact_state = reference.prepare(&exact(6)).expect("prepare");
            for (on, gate) in &gates {
                engine.apply(&mut state, on, gate).expect("apply");
                reference.apply(&mut exact_state, on, gate).expect("apply");
            }
            if fusion == "fused" {
                assert_eq!(
                    state.split_statistics().splits(),
                    0,
                    "precondition: a pending window, before {name}"
                );
            }
            read(&engine, &mut state, &reference, &mut exact_state);
        }
    }
}

/// Without fusion, a gate on sites 2 and 3 of 6 splits its pair and leaves
/// the centre on the site nearer where it came from, or on the site it was
/// when it was in the pair: on site 2 after a read of site 0, so a read of
/// site 0 again takes two QRs; on site 3 after a read of site 5, so a read
/// of site 5 again takes two; on site 2 or 3 after a read of that site, so
/// a read of it again takes none.
#[test]
fn an_unfused_gate_leaves_the_centre_on_the_side_it_came_from() {
    for (end, expected) in [(0, 2), (5, 2), (2, 0), (3, 0)] {
        let (algebra, calls) = SpyAlgebra::new();
        let engine = CanonicalMpsEngine::new(algebra).with_fusion(false);
        let mut state = engine.prepare(&chain(6, 4)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.probability_of_one(&state, end).expect("P(One)");
        engine.apply(&mut state, &[2, 3], &cnot()).expect("apply");
        let before = calls.qr_moves.get();
        engine.probability_of_one(&state, end).expect("P(One)");
        assert_eq!(
            calls.qr_moves.get() - before,
            expected,
            "back to site {end}"
        );
    }
}

/// A generator of the random circuits below: a linear congruential
/// generator, so that each seed gives the same circuit on every platform.
struct Draws(u64);

impl Draws {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next()).expect("a 31-bit draw") % n
    }

    fn angle(&mut self) -> f64 {
        f64::from(u32::try_from(self.next() % 10_000).expect("a small draw")) * 6.283e-4
    }
}

/// Random circuits of 7 qubits, at a bond that holds every state: one- and
/// two-site gates on any sites in either order, with reads, expectations,
/// projections on the likelier value and forks between them. Every read
/// agrees with the reference, fused and unfused, so fusion and the matrix
/// product operators change nothing but rounding when nothing is
/// truncated.
#[test]
fn random_circuits_agree_with_the_reference() {
    const SITES: usize = 7;
    let reference = ExactReference::new();
    let mut operators = [0, 0];
    for seed in 0..12 {
        for (engine, name) in both_fusions(0.0) {
            let mut draws = Draws(seed);
            let mut state = engine.prepare(&exact(SITES)).expect("prepare");
            let mut exact_state = reference.prepare(&exact(SITES)).expect("prepare");
            for step in 0..80 {
                let context = format!("{name}, seed {seed}, step {step}");
                match draws.below(20) {
                    0..=5 => {
                        let site = draws.below(SITES);
                        let gate = if draws.below(2) == 0 {
                            ry(draws.angle())
                        } else {
                            phase(draws.angle())
                        };
                        engine.apply(&mut state, &[site], &gate).expect("apply");
                        reference
                            .apply(&mut exact_state, &[site], &gate)
                            .expect("apply");
                    }
                    6..=14 => {
                        let first = draws.below(SITES);
                        let second = (first + 1 + draws.below(SITES - 1)) % SITES;
                        let gate = match draws.below(4) {
                            0 => cnot(),
                            1 => cz(),
                            2 => zz(),
                            _ => entangler(draws.angle()),
                        };
                        engine
                            .apply(&mut state, &[first, second], &gate)
                            .expect("apply");
                        reference
                            .apply(&mut exact_state, &[first, second], &gate)
                            .expect("apply");
                    }
                    15 | 16 => {
                        let site = draws.below(SITES);
                        let p = engine.probability_of_one(&state, site).expect("P(One)");
                        let q = reference
                            .probability_of_one(&exact_state, site)
                            .expect("P(One)");
                        assert!((p - q).abs() < 1e-10, "{context}: {p} vs {q}");
                    }
                    17 => {
                        let site = draws.below(SITES);
                        let q = reference
                            .probability_of_one(&exact_state, site)
                            .expect("P(One)");
                        let one = q >= 0.5;
                        let p = engine.project(&mut state, site, one).expect("project");
                        reference
                            .project(&mut exact_state, site, one)
                            .expect("project");
                        let q = if one { q } else { 1.0 - q };
                        assert!((p - q).abs() < 1e-10, "{context}: {p} vs {q}");
                    }
                    18 => {
                        let first = draws.below(SITES);
                        let second = (first + 1 + draws.below(SITES - 1)) % SITES;
                        let observable = z_string(&[first, second]);
                        let value = engine
                            .expectation(&state, &observable)
                            .expect("expectation");
                        let expected = reference
                            .expectation(&exact_state, &observable)
                            .expect("expectation");
                        assert!(
                            (value - expected).norm() < 1e-10,
                            "{context}: {value} vs {expected}"
                        );
                    }
                    _ => state = engine.fork(&state).expect("fork"),
                }
            }
            assert_reads_agree(&engine, &state, &exact_state, 1e-10);
            operators[usize::from(name == "fused")] += state.split_statistics().mpo_applications();
        }
    }
    assert!(
        operators.iter().all(|&count| count > 20),
        "precondition: the circuits apply operators, {operators:?}"
    );
}
