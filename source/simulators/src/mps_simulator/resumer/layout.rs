// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The layout: which site of the chain holds each qubit of a program,
//! chosen at the start of each run, from the program alone and before any
//! shot, so every run of a program starts with the same; lowering reads
//! it to put each call on its qubits' sites. On a chain a layout is an
//! order: its instance is the qubit order ([`QubitOrder`]), chosen from
//! the program's two-qubit gates ([`QubitOrder::for_program`]).
//!
//! An MPS is a chain, so its cost depends on where the qubits sit: a gate
//! on distant sites is routed through swaps, and each bond is the Schmidt
//! rank across its cut, which depends on which qubits fall on each side.
//! Every order holds the same state (to rounding, and within the
//! truncation error of an engine that truncates), so on an exact engine a
//! shot's draws and records do not depend on it; the bonds, the time and
//! the weight a max bond dimension discards do. So the order is the MPS
//! simulation method's own: chosen inside the resumer and applied to every
//! call it hands the engine, it never reaches the caller.
//!
//! The choice is a local search for the minimum linear arrangement of the
//! program's interaction graph. On H₂'s phase estimation it lowered the
//! weighted span from 628,526 to 216,950, in 2 ms on one core (measured
//! during development; the tests check the search's rules, not these
//! figures).

use std::cmp::Reverse;
use std::collections::{BTreeMap, VecDeque};

use crate::qir::{QirProgram, TwoQubitGate};

/// The site of each qubit: a permutation of `0..qubits`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct QubitOrder {
    sites: Vec<usize>,
}

impl QubitOrder {
    /// Qubit `q` on site `q`.
    pub(crate) fn identity(qubits: usize) -> Self {
        Self {
            sites: (0..qubits).collect(),
        }
    }

    /// The order for `program`: the local search of [`local_search_order`]
    /// over its two-qubit gates whose qubits it knows before any run
    /// ([`TwoQubitGate::Known`]); a gate whose qubit is in a register is
    /// not weighed, nor a gate on one qubit twice, which lowering refuses.
    pub(crate) fn for_program(program: &QirProgram) -> Self {
        let qubits = usize::try_from(program.qubits()).expect("a u32 fits a usize");
        let gates = program
            .two_qubit_gates()
            .iter()
            .filter_map(|gate| match gate {
                TwoQubitGate::Known { q1, q2 } if q1 != q2 => Some((*q1 as usize, *q2 as usize)),
                _ => None,
            });
        local_search_order(&InteractionWeights::from_gates(qubits, gates))
    }

    /// The site of `qubit`, which is below the order's qubits.
    pub(crate) fn site(&self, qubit: u32) -> usize {
        self.sites[qubit as usize]
    }

    /// The site of each qubit, qubit 0 first.
    #[cfg(test)]
    pub(crate) fn sites(&self) -> &[usize] {
        &self.sites
    }
}

/// How often each pair of a program's qubits meets in a two-qubit gate,
/// counting each gate of the program once, whether a shot runs it many
/// times (a loop) or never (a branch no shot takes).
pub(crate) struct InteractionWeights {
    qubits: usize,
    /// The gates on each pair of qubits, the lower qubit first.
    pairs: BTreeMap<(usize, usize), u64>,
}

impl InteractionWeights {
    /// The weights of `gates`, each on two distinct qubits below `qubits`.
    pub(crate) fn from_gates(
        qubits: usize,
        gates: impl IntoIterator<Item = (usize, usize)>,
    ) -> Self {
        let mut pairs = BTreeMap::new();
        for (a, b) in gates {
            debug_assert!(a != b && a.max(b) < qubits, "a gate on two of the qubits");
            *pairs.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        Self { qubits, pairs }
    }

    /// Σ gates · |site(a) − site(b)| over the pairs: the swaps the gates
    /// cost on the chain under `order`, within a factor of two.
    pub(crate) fn span(&self, order: &QubitOrder) -> u64 {
        self.pairs
            .iter()
            .map(|(&(a, b), &gates)| gates * order.sites[a].abs_diff(order.sites[b]) as u64)
            .sum()
    }

    /// Each qubit's neighbours, with their gates, ascending.
    fn neighbours(&self) -> Vec<Vec<(usize, u64)>> {
        let mut neighbours = vec![Vec::new(); self.qubits];
        for (&(a, b), &w) in &self.pairs {
            neighbours[a].push((b, w));
            neighbours[b].push((a, w));
        }
        for n in &mut neighbours {
            n.sort_unstable();
        }
        neighbours
    }
}

/// The order of least span that a local search finds: it searches from two
/// starts, the identity and reverse Cuthill–McKee, sweeping over the pairs
/// of qubits and swapping their sites while that lowers the span, and keeps
/// the order of least span, the identity's when they tie. So its span is
/// never above the identity's, and the same weights give the same order.
pub(crate) fn local_search_order(weights: &InteractionWeights) -> QubitOrder {
    let from_identity = local_search(QubitOrder::identity(weights.qubits), weights);
    let from_rcm = local_search(reverse_cuthill_mckee(weights), weights);
    if weights.span(&from_rcm) < weights.span(&from_identity) {
        from_rcm
    } else {
        from_identity
    }
}

/// Reverse Cuthill–McKee on the interaction graph: each component breadth
/// first from its qubit of least weighted degree (then least index),
/// neighbours by weight down, degree up, index up; the visits reversed.
fn reverse_cuthill_mckee(weights: &InteractionWeights) -> QubitOrder {
    let qubits = weights.qubits;
    let neighbours = weights.neighbours();
    let degree: Vec<u64> = neighbours
        .iter()
        .map(|n| n.iter().map(|&(_, w)| w).sum())
        .collect();
    let mut seen = vec![false; qubits];
    let mut visits = Vec::with_capacity(qubits);
    while let Some(start) = (0..qubits)
        .filter(|&q| !seen[q])
        .min_by_key(|&q| (degree[q], q))
    {
        seen[start] = true;
        let mut queue = VecDeque::from([start]);
        while let Some(q) = queue.pop_front() {
            visits.push(q);
            let mut next = neighbours[q].clone();
            next.sort_by_key(|&(r, w)| (Reverse(w), degree[r], r));
            for (r, _) in next {
                if !seen[r] {
                    seen[r] = true;
                    queue.push_back(r);
                }
            }
        }
    }
    let mut sites = vec![0; qubits];
    for (site, &qubit) in visits.iter().rev().enumerate() {
        sites[qubit] = site;
    }
    QubitOrder { sites }
}

/// The local search from `start`: sweeps over the pairs of qubits (the
/// first ascending, then the second), swapping their sites when that lowers
/// the span, until a sweep swaps nothing. A swap changes only the spans of
/// the two qubits' pairs, so each swap is judged on those.
fn local_search(start: QubitOrder, weights: &InteractionWeights) -> QubitOrder {
    let neighbours = weights.neighbours();
    let mut sites = start.sites;
    let touched = |sites: &[usize], a: usize, b: usize| -> u64 {
        [a, b]
            .iter()
            .flat_map(|&q| neighbours[q].iter().map(move |&(r, w)| (q, r, w)))
            .map(|(q, r, w)| w * sites[q].abs_diff(sites[r]) as u64)
            .sum()
    };
    let mut improved = true;
    while improved {
        improved = false;
        for a in 0..sites.len() {
            for b in a + 1..sites.len() {
                let before = touched(&sites, a, b);
                sites.swap(a, b);
                if touched(&sites, a, b) < before {
                    improved = true;
                } else {
                    sites.swap(a, b);
                }
            }
        }
    }
    QubitOrder { sites }
}
