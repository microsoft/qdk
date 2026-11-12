// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rewrite contract of `resumer/rewrites.rs`, one test per clause, each
//! run on every rewrite: a new rewrite is tested by being added to
//! `every_rewrite`. What only gate fusion does is in `rewrites/fusion.rs`.

mod fusion;

use num_complex::Complex64;

use super::super::below;
use crate::mps_simulator::GateFusion;
use crate::mps_simulator::resumer::lowering::{Operator, Step};
use crate::mps_simulator::resumer::rewrites::{Rewrite, rewrite_runs};
use crate::mps_simulator::resumer::shot_stream::SplitMix64;
use crate::qir::QirGate;

/// The sites of the runs here: few enough for dense products.
const SITES: usize = 6;

/// Every rewrite, by name: today the two settings of gate fusion.
fn every_rewrite() -> [(&'static str, &'static dyn Rewrite); 2] {
    [
        ("Unfused", &GateFusion::Unfused),
        ("Pairs", &GateFusion::Pairs),
    ]
}

/// `gate` at `angle` on `sites` (the second unused for a one-qubit gate).
fn at(gate: QirGate, angle: Option<f64>, sites: [usize; 2]) -> Operator {
    Operator {
        sites,
        matrix: gate.matrix_at(angle),
    }
}

/// `gate`, which takes no angle, on `sites`.
fn on(gate: QirGate, sites: [usize; 2]) -> Operator {
    at(gate, None, sites)
}

/// A random gate of `stream` on `SITES` sites.
fn random_gate(stream: &mut SplitMix64) -> Operator {
    let gates = [
        QirGate::H,
        QirGate::Rx,
        QirGate::Ry,
        QirGate::T,
        QirGate::Cx,
        QirGate::Cy,
        QirGate::Rzz,
        QirGate::Swap,
    ];
    let gate = gates[below(stream, gates.len())];
    let angle = matches!(gate, QirGate::Rx | QirGate::Ry | QirGate::Rzz)
        .then(|| stream.next_uniform() * 6.0);
    let first = below(stream, SITES);
    let second = (first + 1 + below(stream, SITES - 1)) % SITES;
    at(gate, angle, [first, second])
}

/// Runs that stress a rewrite: the empty run, one gate, overlapping and
/// disjoint pairs, one-site gates before and after pairs, distant sites,
/// random runs and a long one.
fn runs() -> Vec<Vec<Operator>> {
    use QirGate::{Cx, Cy, Cz, H, Rx, Rzz, S, Swap, T};
    let mut runs = vec![
        vec![],
        vec![on(H, [2, 0])],
        // Each pair shares a site with the one before.
        vec![
            on(Cx, [0, 1]),
            on(Cz, [1, 2]),
            at(Rzz, Some(0.4), [0, 1]),
            on(Cx, [2, 1]),
            on(Cy, [3, 2]),
            on(Swap, [1, 2]),
        ],
        // Disjoint pairs, then pairs across them.
        vec![
            on(Cx, [0, 1]),
            on(Cx, [2, 3]),
            on(Cx, [4, 5]),
            on(Cz, [1, 2]),
            on(Cy, [3, 4]),
            on(Cx, [1, 0]),
        ],
        // One-site gates before and after pairs.
        vec![
            on(H, [0, 0]),
            on(T, [1, 0]),
            on(Cx, [0, 1]),
            on(S, [0, 0]),
            at(Rx, Some(1.3), [1, 0]),
            on(H, [2, 0]),
            on(Cz, [1, 2]),
            on(T, [2, 0]),
        ],
        // Pairs on distant sites.
        vec![
            on(Cx, [0, 5]),
            on(H, [5, 0]),
            on(Cz, [5, 2]),
            at(Rzz, Some(2.1), [1, 4]),
            on(Cx, [4, 0]),
        ],
    ];
    let mut stream = SplitMix64::new(11);
    for length in [40; 10].into_iter().chain([300]) {
        runs.push((0..length).map(|_| random_gate(&mut stream)).collect());
    }
    runs
}

/// The product of `operators`, applied in order on `SITES` sites: a dense
/// matrix, row-major, site 0 the most significant bit. Equal products give
/// the same state from every state.
fn product(operators: &[Operator]) -> Vec<Complex64> {
    let dimension = 1 << SITES;
    let mut product = vec![Complex64::ZERO; dimension * dimension];
    for i in 0..dimension {
        product[i * dimension + i] = Complex64::ONE;
    }
    let bit = |site: usize| 1usize << (SITES - 1 - site);
    for operator in operators {
        let sites = operator.sites();
        let k = sites.len();
        // The offset of each basis state of the operator's sites, its first
        // site the most significant.
        let offsets: Vec<usize> = (0..1usize << k)
            .map(|basis| {
                (0..k)
                    .filter(|j| basis >> (k - 1 - j) & 1 == 1)
                    .fold(0, |offset, j| offset | bit(sites[j]))
            })
            .collect();
        let mask = sites.iter().fold(0, |mask, &site| mask | bit(site));
        let matrix = operator.matrix.row_major();
        for column in 0..dimension {
            for base in (0..dimension).filter(|row| row & mask == 0) {
                let before: Vec<Complex64> = offsets
                    .iter()
                    .map(|&offset| product[(base | offset) * dimension + column])
                    .collect();
                for (row, &offset) in matrix.chunks_exact(1 << k).zip(&offsets) {
                    product[(base | offset) * dimension + column] =
                        row.iter().zip(&before).map(|(m, v)| m * v).sum();
                }
            }
        }
    }
    product
}

/// Panics, naming `what`, unless the products of `expected` and `actual`
/// are the same, to rounding.
fn assert_same_product(expected: &[Operator], actual: &[Operator], what: &str) {
    for (index, (e, a)) in product(expected).iter().zip(product(actual)).enumerate() {
        let difference = (e - a).norm();
        assert!(
            difference < 1e-10,
            "{what}: entry {index} of the products differs by {difference}"
        );
    }
}

/// The draws of `steps` in order, and the runs of operators before, between
/// and after them: one more run than draws, some of them empty.
fn split_at_draws(steps: &[Step]) -> (Vec<Step>, Vec<Vec<Operator>>) {
    let (mut draws, mut runs) = (Vec::new(), vec![Vec::new()]);
    for step in steps {
        match step {
            Step::Apply(operator) => runs.last_mut().expect("one run").push(*operator),
            draw => {
                draws.push(draw.clone());
                runs.push(Vec::new());
            }
        }
    }
    (draws, runs)
}

/// Clause 1, same state: on every run, every rewrite's operators have the
/// product of the run's, so they give the same state from every state.
#[test]
fn every_rewrite_gives_the_same_state() {
    for (name, rewrite) in every_rewrite() {
        for (index, run) in runs().into_iter().enumerate() {
            let rewritten = rewrite.rewrite(run.clone());
            assert_same_product(&run, &rewritten, &format!("{name}, run {index}"));
        }
    }
}

/// Clause 2, engine-ready: on every run, every operator a rewrite gives acts
/// on one site or two distinct sites.
#[test]
fn every_rewrite_puts_each_operator_on_one_or_two_distinct_sites() {
    for (name, rewrite) in every_rewrite() {
        for run in runs() {
            for operator in rewrite.rewrite(run) {
                let sites = operator.sites();
                assert!(
                    matches!(sites, [_] | [_, _]) && sites.first() != sites.get(1),
                    "{name}: an operator on {sites:?}"
                );
            }
        }
    }
}

/// Clause 3, measurements and resets are barriers: in a batch of runs
/// between draws (a reset first, a measurement and a reset with no gate
/// between, a measurement that resets, a measurement last), every rewrite
/// keeps the draws, in order, and each run between them keeps its product,
/// so no operator moves across a draw.
#[test]
fn measurements_and_resets_are_barriers_to_every_rewrite() {
    let runs = runs();
    let measure = |site, result, reset| Step::Measure {
        site,
        result,
        reset,
    };
    let apply = |run: &Vec<Operator>| run.iter().copied().map(Step::Apply).collect::<Vec<_>>();
    let batches = [
        [
            vec![Step::Reset { site: 0 }],
            apply(&runs[2]),
            vec![measure(1, 0, false), measure(2, 1, true)],
            apply(&runs[4]),
            vec![Step::Reset { site: 1 }],
            apply(&runs[6]),
        ]
        .concat(),
        [
            apply(&runs[3]),
            vec![measure(0, 0, true)],
            apply(&runs[7]),
            vec![measure(5, 1, false)],
        ]
        .concat(),
    ];
    for (name, rewrite) in every_rewrite() {
        for batch in &batches {
            let (draws, runs) = split_at_draws(batch);
            let (rewritten_draws, rewritten_runs) =
                split_at_draws(&rewrite_runs(batch.clone(), rewrite));
            assert_eq!(rewritten_draws, draws, "{name}: the draws, in order");
            for (place, (run, rewritten)) in runs.iter().zip(&rewritten_runs).enumerate() {
                assert_same_product(
                    run,
                    rewritten,
                    &format!("{name}, the run before draw {place}"),
                );
            }
        }
    }
}
