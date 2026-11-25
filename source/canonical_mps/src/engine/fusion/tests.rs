// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fusion: the window, which opens on its gate's sites ascending, which a
//! two-site gate joins while their sites number at most three, neighbours
//! or not, and whose operator is its gates' product in the caller's order;
//! the engine's pending window, which the first operation it does not serve
//! applies at once; the end of its span it leaves the centre on; which
//! site each operation names for it; and every read sees its gates.

use super::Window;
use crate::engine::conformance::{
    assert_reads_agree, capacity, cnot, entangler, full_capacity, mixed_string, real, ry,
};
use crate::engine::tests::{State, both_fusions, cz, phase, z_string, zz};
use crate::{CpuAlgebra, CpuMpsEngine as Engine, DEFAULT_CUTOFF};
use num_complex::Complex64;
use tensornet::MpsEngine;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::sampling::SplitMix64;

/// A caller that takes the engine by `new`: two CNOTs on one pair,
/// then a read, take one split and no matrix product operator.
#[test]
fn an_engine_fuses_by_default() {
    let engine = Engine::new(CpuAlgebra);
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.7)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.probability_of_one(&state, 1).expect("P(One)");
    let statistics = state.statistics();
    assert_eq!((statistics.splits(), statistics.mpo_applications()), (1, 0));
}

/// Grouped shots fork a state whose last gates left a window pending: the
/// window is applied once, in the state forked, so neither that state nor
/// its forks apply it again when read.
#[test]
fn a_fork_applies_the_pending_window_once() {
    let engine = Engine::new(CpuAlgebra);
    let mut state = engine.prepare(&capacity(3, 4)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.7)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
    assert_eq!(state.statistics().splits(), 0, "a pending window");
    let forks = [
        engine.fork(&state).expect("fork"),
        engine.fork(&state).expect("fork"),
    ];
    for read in forks.iter().chain([&state]) {
        assert_eq!(read.statistics().splits(), 2, "after the forks");
        engine.probability_of_one(read, 1).expect("P(One)");
        assert_eq!(read.statistics().splits(), 2, "after a read");
    }
}

/// A gate left pending, then a read of one site: the read applies the
/// window toward its site, so the centre ends on the window's end nearer
/// the read and makes no move it would not make to reach the read.
///
/// - A gate on sites 1 and 2 of 4, applied by one split: with the centre
///   on site 0, a read of site 2; with the centre on site 3, where a read
///   of site 3 had left it, a read of site 1. The read moves the centre
///   onto the pair's site nearer it, one move, and splits the pair toward
///   the site it reads, which leaves the centre there, so the centre makes
///   no other move. With the centre already on site 1, where a read of
///   site 1 had left it, a read of site 2 makes no move before the split.
///   A conditional read with no conditions forks nothing and reads as
///   `probability_of_one` does, with the same moves; its rows are the two
///   where a flush toward the centre would take one move more.
/// - A CNOT on sites 0 and 4 of 6, every site turned, applied as a matrix
///   product operator from the centre on site 0, then a read of site 5 or
///   of site 0. The operator's splits leave the centre on the span's end
///   nearer the read, site 4 or site 0. So the read of site 5 takes one
///   move more than the span's 3, and the read of site 0 none.
/// - A gate on sites 2 and 3 of 6 with the centre on site 5, then a gate
///   on sites 0 and 1, which does not fit, then a read of site 0. The
///   window on 2 and 3 takes two moves to reach site 3, then is applied
///   toward the second gate's lower site, which leaves the centre on
///   site 2. The read takes one move to reach the new pair. Applied toward
///   the centre, the first window would leave it on site 3, one move more.
#[test]
fn a_read_applies_the_pending_window_toward_its_site() {
    struct Row {
        read_of: SiteRead,
        read_name: &'static str,
        sites: usize,
        bond: usize,
        turned: bool,
        gates: &'static [[usize; 2]],
        first_read: Option<usize>,
        read: usize,
        splits: u64,
        moves: u64,
    }
    let pair = |read_of, read_name, first_read, read, moves| Row {
        read_of,
        read_name,
        sites: 4,
        bond: 2,
        turned: false,
        gates: &[[1, 2]],
        first_read,
        read,
        splits: 1,
        moves,
    };
    let span = |read, moves| Row {
        read_of: probability_of_one,
        read_name: "P(One)",
        sites: 6,
        bond: 8,
        turned: true,
        gates: &[[0, 4]],
        first_read: None,
        read,
        splits: 4,
        moves,
    };
    let rows = [
        pair(probability_of_one as SiteRead, "P(One)", None, 2, 1),
        pair(probability_of_one, "P(One)", Some(3), 1, 1),
        pair(probability_of_one, "P(One)", Some(1), 2, 0),
        pair(unconditional_read, "an unconditional read", Some(3), 1, 1),
        pair(unconditional_read, "an unconditional read", Some(1), 2, 0),
        span(5, 4),
        span(0, 3),
        Row {
            read_of: probability_of_one,
            read_name: "P(One)",
            sites: 6,
            bond: 2,
            turned: false,
            gates: &[[2, 3], [0, 1]],
            first_read: Some(5),
            read: 0,
            splits: 2,
            moves: 3,
        },
    ];
    for row in rows {
        let case = format!(
            "gates on {:?} of {}, centre left by a read of {:?}, {} of site {}",
            row.gates, row.sites, row.first_read, row.read_name, row.read
        );
        let engine = Engine::new(CpuAlgebra);
        let mut state = engine
            .prepare(&capacity(row.sites, row.bond))
            .expect("prepare");
        if row.turned {
            for site in 0..row.sites {
                engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
            }
        }
        if let Some(site) = row.first_read {
            engine.probability_of_one(&state, site).expect("P(One)");
        }
        let moves = state.statistics().centre_moves();
        for gate_sites in row.gates {
            engine
                .apply(&mut state, gate_sites, &cnot())
                .expect("apply");
        }
        (row.read_of)(&engine, &state, row.read);
        let statistics = state.statistics();
        assert_eq!(
            statistics.splits(),
            row.splits,
            "precondition, the windows' splits: {case}"
        );
        assert_eq!(statistics.centre_moves() - moves, row.moves, "{case}");
    }
}

/// A read of one site of a state.
type SiteRead = fn(&Engine, &State, usize);

/// P(One) of `site`.
fn probability_of_one(engine: &Engine, state: &State, site: usize) {
    engine.probability_of_one(state, site).expect("P(One)");
}

/// P(One) of `site` given no conditions.
fn unconditional_read(engine: &Engine, state: &State, site: usize) {
    engine
        .conditional_probability_of_one(state, site, &[])
        .expect("P(One | nothing)");
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

/// The shape of the chain's bonds.
fn shape(engine: &Engine, state: &State) {
    engine.shape(state).expect("a shape");
}

/// The weight the truncations kept.
fn retained_weight(engine: &Engine, state: &State) {
    engine.retained_weight(state).expect("a retained weight");
}

/// A CNOT on sites 1 and 2 of 4, with the centre on site 3, where a read
/// had left it, then an operation that names no site: an expectation, a
/// conditional read with a condition (which forks), a shape or the
/// retained weight. Or, without fusion, the gate alone, applied at once.
/// Each applies the window toward the centre, which leaves it on site 2.
/// Applied toward site 0, toward the gate's lower site, or, for the
/// conditional read, toward the site read, it would leave it on site 1.
/// So a read of site 2 after the operation makes no move.
#[test]
fn every_operation_that_names_no_site_applies_the_window_toward_the_centre() {
    let rows: [(&str, bool, Operation); 5] = [
        ("an expectation", true, expectation),
        ("a conditional read", true, conditional_read),
        ("a shape", true, shape),
        ("the retained weight", true, retained_weight),
        ("an unfused gate", false, |_, _| {}),
    ];
    for (name, fuses, operation) in rows {
        let engine = Engine::new(CpuAlgebra).with_fusion(fuses);
        let mut state = engine.prepare(&capacity(4, 2)).expect("prepare");
        engine.probability_of_one(&state, 3).expect("P(One)");
        engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
        operation(&engine, &state);
        let moves = state.statistics().centre_moves();
        engine.probability_of_one(&state, 2).expect("P(One)");
        let statistics = state.statistics();
        assert_eq!(
            statistics.splits(),
            1,
            "precondition, the window's split: {name}"
        );
        assert_eq!(statistics.centre_moves(), moves, "{name}");
    }
}

/// A CNOT with site 1 the control and site 0 the target opens a window on
/// sites 0 and 1, ascending, the gate laid out on them, site 0 the most
/// significant bit: it sends `[a, b]` to `[a ⊕ b, b]`, so the basis states
/// 0, 1, 2 and 3 to 0, 3, 2 and 1. The engine applies a window's operator
/// on its sites in that order, so a window that kept its operands' order
/// would act on the wrong sites.
#[test]
fn a_window_opens_on_its_gates_sites_ascending() {
    let window = Window::of(&cnot(), &[1, 0]);
    assert_eq!(window.sites, [0, 1]);
    let mut expected = vec![real(0.0); 16];
    for (column, row) in [(0, 0), (1, 3), (2, 2), (3, 1)] {
        expected[4 * row + column] = real(1.0);
    }
    assert_eq!(window.operator, expected);
}

/// The window of a CNOT on each pair of `gates` in turn: the first opens
/// it, and each later one must join it.
fn window_of_cnots(gates: &[[usize; 2]]) -> Window {
    let (first, later) = gates.split_first().expect("a gate to open the window");
    let mut window = Window::of(&cnot(), first);
    for gate_sites in later {
        assert!(
            window.absorb_if_it_fits(&cnot(), gate_sites),
            "precondition: the CNOT on {gate_sites:?} joins the window"
        );
    }
    window
}

/// One gate offered to a window: the CNOTs that made the window, the
/// offered CNOT's sites, and the window's sites after it joins (`None`
/// when it does not).
struct Offer {
    window: &'static [[usize; 2]],
    gate: [usize; 2],
    joined: Option<&'static [usize]>,
}

/// A program's run of CNOTs, each offered to the window the earlier ones
/// opened. A two-site gate joins while the window's sites and its own
/// number at most `WINDOW_SITES`, neighbours or not, whatever its operand
/// order: the window then holds their sites, ascending. Otherwise it does
/// not join, and the window keeps its sites and operator.
#[test]
fn a_two_site_gate_joins_the_window_while_their_sites_number_at_most_three() {
    let offers = [
        // The window's own sites, in the other order.
        Offer {
            window: &[[0, 1]],
            gate: [1, 0],
            joined: Some(&[0, 1]),
        },
        // A third site between the window's two, which are not neighbours.
        Offer {
            window: &[[0, 2]],
            gate: [2, 1],
            joined: Some(&[0, 1, 2]),
        },
        // A third site apart from the window's.
        Offer {
            window: &[[0, 1]],
            gate: [3, 1],
            joined: Some(&[0, 1, 3]),
        },
        // A full window of three sites, the gate on two of them.
        Offer {
            window: &[[0, 1], [1, 2]],
            gate: [2, 0],
            joined: Some(&[0, 1, 2]),
        },
        // A fourth site.
        Offer {
            window: &[[0, 1], [1, 2]],
            gate: [2, 3],
            joined: None,
        },
        // Two new sites at once, a fourth among them.
        Offer {
            window: &[[0, 1]],
            gate: [2, 3],
            joined: None,
        },
    ];
    for Offer {
        window: gates,
        gate,
        joined,
    } in offers
    {
        let case = format!("the window of {gates:?}, a gate on {gate:?}");
        let mut window = window_of_cnots(gates);
        let before = window.clone();
        let fits = window.absorb_if_it_fits(&cnot(), &gate);
        assert_eq!(fits, joined.is_some(), "{case}");
        if let Some(sites) = joined {
            assert_eq!(window.sites, sites, "{case}");
        } else {
            assert_eq!(window.sites, before.sites, "{case}");
            assert_eq!(window.operator, before.operator, "{case}");
        }
    }
}

/// The operator on three sites that sends each basis state `[a, b, c]`
/// (site 0's value `a` the most significant bit) to `map([a, b, c])`.
fn permutation_on_three_sites(map: fn([usize; 3]) -> [usize; 3]) -> Vec<Complex64> {
    let mut operator = vec![real(0.0); 64];
    for column in 0..8 {
        let [a, b, c] = map([(column >> 2) & 1, (column >> 1) & 1, column & 1]);
        operator[8 * ((a << 2) | (b << 1) | c) + column] = real(1.0);
    }
    operator
}

/// A run of three gates on sites 0, 1 and 2: a CNOT with site 2 the
/// control and site 0 the target, which opens the window; a CNOT with site
/// 0 the control and site 1 the target, which joins it; an X on site 1,
/// which multiplies into it. The window's operator, on its sites ascending,
/// is the product of the gates in the caller's order. Each gate permutes
/// the basis states, so that product is exactly the classical map
///
/// ```text
///  [a, b, c] → [a ⊕ c, b, c]               the CNOT on (2, 0)
///            → [a ⊕ c, b ⊕ a ⊕ c, c]       the CNOT on (0, 1)
///            → [a ⊕ c, ¬(b ⊕ a ⊕ c), c]    the X on 1
/// ```
///
/// In the reverse order, the gates would map `[0, 0, 1]` to `[1, 1, 1]`,
/// not to `[1, 0, 1]`.
#[test]
fn the_windows_operator_is_its_gates_product_in_the_callers_order() {
    let mut window = Window::of(&cnot(), &[2, 0]);
    assert!(
        window.absorb_if_it_fits(&cnot(), &[0, 1]),
        "precondition: the CNOT on [0, 1] joins the window"
    );
    window.absorb(&[real(0.0), real(1.0), real(1.0), real(0.0)], &[1]);
    assert_eq!(window.sites, [0, 1, 2]);
    let expected = permutation_on_three_sites(|[a, b, c]| [a ^ c, 1 ^ b ^ a ^ c, c]);
    assert_eq!(window.operator, expected);
}

/// The sites of the CNOTs that make a window, as [`window_of_cnots`]
/// takes them.
type Cnots = &'static [[usize; 2]];

/// A window applied for an operation that names a site leaves the centre
/// on the end of its span nearer that site, the lower end on a tie. So
/// the centre then moves no further than the span to reach the site. An
/// operation that names no site names the centre: a window on sites 2
/// and 3 then leaves it on site 2 when it came from site 0, and on site 3
/// when it came from site 5. Each row: the CNOTs that made the window, the
/// site named, and the end the centre is left on.
#[test]
fn a_window_leaves_the_centre_on_its_end_nearer_the_site_named() {
    let rows: [(Cnots, usize, usize); 10] = [
        (&[[2, 3]], 0, 2),
        (&[[2, 3]], 2, 2),
        (&[[2, 3]], 3, 3),
        (&[[2, 3]], 5, 3),
        (&[[1, 4]], 2, 1),
        (&[[1, 4]], 5, 4),
        // Site 2 is as near site 1 as site 3.
        (&[[1, 3]], 2, 1),
        (&[[1, 3]], 3, 3),
        // Site 1 is as near site 0 as site 2.
        (&[[0, 1], [1, 2]], 1, 0),
        (&[[0, 1], [1, 2]], 2, 2),
    ];
    for (gates, site, end) in rows {
        let window = window_of_cnots(gates);
        assert_eq!(
            window.centre_end_toward(site),
            end,
            "the window of {gates:?}, site {site}"
        );
    }
}

/// A two-site gate that does not fit applies the window toward the gate's
/// lower site, whichever operand comes first and wherever the centre is.
/// Each row: the CNOTs that made the window, the gate's sites, and the end
/// the centre is left on.
///
/// - A window on sites 1 and 4, a gate on sites 5 and 2, then on 2 and 5:
///   site 1, the end nearer site 2. Toward site 5, the first operand of
///   one and the higher site of both, it would be site 4.
/// - A window on sites 2 and 4, a gate on sites 5 and 0: site 2, the end
///   nearer site 0. Toward site 5, the gate's site nearer the window, it
///   would be site 4.
#[test]
fn a_gate_that_does_not_fit_applies_the_window_toward_its_lower_site() {
    let rows: [(Cnots, [usize; 2], usize); 3] = [
        (&[[1, 4]], [5, 2], 1),
        (&[[1, 4]], [2, 5], 1),
        (&[[2, 4]], [5, 0], 2),
    ];
    for (gates, gate, end) in rows {
        let window = window_of_cnots(gates);
        assert!(
            !window.clone().absorb_if_it_fits(&cnot(), &gate),
            "precondition: the gate on {gate:?} does not fit"
        );
        assert_eq!(
            window.centre_end_displaced_by(&gate),
            end,
            "the window of {gates:?}, a gate on {gate:?}"
        );
    }
}

/// A read of a state and of the exact reference's, which checks they
/// agree.
type Read = fn(&Engine, &mut State, &ExactReference, &mut ReferenceState);

/// A window on sites 1, 3 and 4 of 6, left pending, with a turn of site 2
/// between its sites and one-site gates before and after its gates. Then
/// each read, on a state of its own. Every read agrees with the exact
/// reference. So each sees every gate, the window's and those waiting on
/// their sites, in the caller's order.
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
            let mut state = engine.prepare(&capacity(6, 8)).expect("prepare");
            let mut exact_state = reference.prepare(&full_capacity(6)).expect("prepare");
            for (on, gate) in &gates {
                engine.apply(&mut state, on, gate).expect("apply");
                reference.apply(&mut exact_state, on, gate).expect("apply");
            }
            if fusion == "fused" {
                assert_eq!(
                    state.statistics().splits(),
                    0,
                    "precondition: a pending window, before {name}"
                );
            }
            read(&engine, &mut state, &reference, &mut exact_state);
        }
    }
}

/// A generator of the random circuits below: `tensornet`'s [`SplitMix64`],
/// so that each seed gives the same circuit on every platform.
struct Draws(SplitMix64);

impl Draws {
    fn new(seed: u64) -> Self {
        Self(SplitMix64::new(seed))
    }

    fn below(&mut self, n: usize) -> usize {
        let bound = u64::try_from(n).expect("a small bound");
        usize::try_from(self.0.next_u64() % bound).expect("below a usize")
    }

    fn angle(&mut self) -> f64 {
        f64::from(u32::try_from(self.0.next_u64() % 10_000).expect("a small draw")) * 6.283e-4
    }
}

/// Random circuits of 7 qubits, at a bond that holds every state: one- and
/// two-site gates on any sites in either order, with reads, expectations,
/// projections on the likelier value and forks between them. Every read
/// agrees with the exact reference, fused and unfused, so fusion and the matrix
/// product operators change nothing but rounding when nothing is
/// truncated.
#[test]
fn fusion_changes_no_read_of_an_untruncated_circuit() {
    const SITES: usize = 7;
    let reference = ExactReference::new();
    let mut operators = [0, 0];
    for seed in 0..12 {
        for (engine, name) in both_fusions(0.0) {
            let mut draws = Draws::new(seed);
            let mut state = engine.prepare(&full_capacity(SITES)).expect("prepare");
            let mut exact_state = reference.prepare(&full_capacity(SITES)).expect("prepare");
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
            operators[usize::from(name == "fused")] += state.statistics().mpo_applications();
        }
    }
    assert!(
        operators.iter().all(|&count| count > 20),
        "precondition: the circuits apply operators, {operators:?}"
    );
}
