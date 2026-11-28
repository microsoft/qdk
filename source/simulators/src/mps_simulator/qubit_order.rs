// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubit order ([`QubitOrder`]): which site of the chain holds each
//! qubit of a program. Lowering reads it to put each call on its qubits'
//! sites. [`QubitOrder::for_program`] chooses it at the start of each run,
//! from the program alone and before any shot, so every run of a program
//! starts with the same order.
//!
//! An MPS is a chain, so its cost depends on where the qubits sit. An
//! engine applies a gate on distant sites at a cost that grows with their
//! distance (for instance, as a matrix product operator, an MPO, across the
//! sites between them). The bonds grow with the entanglement across each
//! cut, which depends on which qubits fall on each side.
//!
//! Up to rounding, every order holds the same state on an engine that does
//! not truncate ([`MpsResumer`](super::MpsResumer) says what rounding
//! allows). On one that truncates, each order truncates its own way. The
//! bonds, the time and the weight discarded depend on the order, so the
//! order is the MPS simulation method's own: it never reaches the caller.
//!
//! [`local_search_order`] gives how the order is chosen.

use std::cmp::Reverse;
use std::collections::{BTreeMap, VecDeque};

use crate::qir::{QirProgram, TwoQubitGate};

/// The site of each qubit: a permutation of `0..qubits`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct QubitOrder {
    /// The site of qubit `q` at index `q`.
    sites: Vec<usize>,
}

impl QubitOrder {
    /// Qubit `q` on site `q`.
    fn identity(qubits: usize) -> Self {
        Self {
            sites: (0..qubits).collect(),
        }
    }

    /// The order for `program`: [`local_search_order`] over its two-qubit
    /// gates whose qubits it knows before any run
    /// ([`TwoQubitGate::Known`]). It does not weigh a gate whose qubit is
    /// in a register, nor a gate on one qubit twice, which lowering
    /// refuses.
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
}

/// How often each pair of a program's qubits meets in a two-qubit gate,
/// counting each gate of the program once, whether a shot runs it many
/// times (a loop) or never (a branch no shot takes).
struct InteractionWeights {
    /// How many qubits the program has; every pair's qubits are below it.
    qubits: usize,
    /// The gates on each pair of qubits, the lower qubit first.
    pairs: BTreeMap<(usize, usize), u64>,
}

impl InteractionWeights {
    /// The weights of `gates`, each on two distinct qubits below `qubits`.
    fn from_gates(qubits: usize, gates: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut pairs = BTreeMap::new();
        for (a, b) in gates {
            debug_assert!(a != b && a.max(b) < qubits, "a gate on two of the qubits");
            *pairs.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        Self { qubits, pairs }
    }

    /// Σ gates · |site(a) − site(b)| over the pairs: a proxy for what the
    /// gates cost on the chain under `order`, as an engine applies a gate on
    /// distant sites at a cost that grows with their distance.
    fn span(&self, order: &QubitOrder) -> u64 {
        self.pairs
            .iter()
            .map(|(&(a, b), &gates)| gates * order.sites[a].abs_diff(order.sites[b]) as u64)
            .sum()
    }

    /// Each qubit's neighbours, with their gates, ascending.
    fn neighbours(&self) -> Neighbours {
        let mut neighbours = vec![Vec::new(); self.qubits];
        for (&(a, b), &gates) in &self.pairs {
            neighbours[a].push((b, gates));
            neighbours[b].push((a, gates));
        }
        for list in &mut neighbours {
            list.sort_unstable();
        }
        neighbours
    }
}

/// Each qubit's neighbours in the interaction graph, each with the gates
/// they share.
type Neighbours = Vec<Vec<(usize, u64)>>;

/// The order of least span that a local search finds. It never has a span
/// above the identity's, no swap of two qubits' sites lowers its span, and
/// the same weights give the same order.
///
/// The search seeks a good order, not the best. It is a local search for
/// the minimum linear arrangement of the program's interaction graph, a
/// problem that is NP-complete (M. R. Garey, D. S. Johnson and L.
/// Stockmeyer, "Some simplified NP-complete graph problems", Theoretical
/// Computer Science 1(3), 1976).
///
/// It searches from two starts: the identity, and reverse Cuthill–McKee
/// (the order of "Reducing the bandwidth of sparse symmetric matrices",
/// Proc. 24th ACM National Conference, 1969, reversed as A. George
/// proposed, doctoral thesis, Stanford, 1971). From each, it swaps the
/// sites of pairs of qubits while that lowers the span, until no swap
/// does. This is a pairwise-interchange search (compared with other
/// heuristics by J. Petit, "Experiments on the minimum linear arrangement
/// problem", ACM Journal of Experimental Algorithmics 8, 2003). It keeps
/// the order of least span, the identity's when they tie.
///
/// Neither the QDK nor its dependencies have this search. The `sprs`
/// crate has reverse Cuthill–McKee alone, which ignores how many gates
/// each pair of qubits shares.
///
/// On H₂'s iterative phase estimation (29 qubits, about 89,000 two-qubit
/// gates), the search lowered the weighted span from 628,526 to 216,950.
/// The tests check the search's rules, not these figures.
fn local_search_order(weights: &InteractionWeights) -> QubitOrder {
    let from_identity = local_search(QubitOrder::identity(weights.qubits), weights);
    let from_rcm = local_search(reverse_cuthill_mckee(weights), weights);
    if weights.span(&from_rcm) < weights.span(&from_identity) {
        from_rcm
    } else {
        from_identity
    }
}

/// Reverse Cuthill–McKee on the interaction graph: the qubits in the order
/// of [`breadth_first_visits`], reversed, so that qubits that share gates
/// sit on nearby sites.
fn reverse_cuthill_mckee(weights: &InteractionWeights) -> QubitOrder {
    let visits = breadth_first_visits(weights);
    let mut sites = vec![0; weights.qubits];
    for (site, &qubit) in visits.iter().rev().enumerate() {
        sites[qubit] = site;
    }
    QubitOrder { sites }
}

/// Every qubit, in the order a breadth-first search visits them: each
/// component from its qubit of least weighted degree (then least index),
/// each qubit's neighbours by weight down, degree up, index up.
fn breadth_first_visits(weights: &InteractionWeights) -> Vec<usize> {
    let neighbours = weights.neighbours();
    let degrees = weighted_degrees(&neighbours);
    let mut seen = vec![false; weights.qubits];
    let mut visits = Vec::with_capacity(weights.qubits);
    while let Some(start) = least_degree_unseen(&degrees, &seen) {
        visit_component(start, &neighbours, &degrees, &mut seen, &mut visits);
    }
    visits
}

/// Each qubit's weighted degree: the gates it shares with any other.
fn weighted_degrees(neighbours: &Neighbours) -> Vec<u64> {
    neighbours
        .iter()
        .map(|list| list.iter().map(|&(_, gates)| gates).sum())
        .collect()
}

/// The unseen qubit of least weighted degree, then least index: where the
/// next component's visit starts.
fn least_degree_unseen(degrees: &[u64], seen: &[bool]) -> Option<usize> {
    (0..seen.len())
        .filter(|&qubit| !seen[qubit])
        .min_by_key(|&qubit| (degrees[qubit], qubit))
}

/// Appends to `visits` the qubits of `start`'s component, breadth first,
/// each qubit's unseen neighbours by gates down, degree up, index up.
fn visit_component(
    start: usize,
    neighbours: &Neighbours,
    degrees: &[u64],
    seen: &mut [bool],
    visits: &mut Vec<usize>,
) {
    seen[start] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(qubit) = queue.pop_front() {
        visits.push(qubit);
        let mut next = neighbours[qubit].clone();
        next.sort_by_key(|&(neighbour, gates)| (Reverse(gates), degrees[neighbour], neighbour));
        for (neighbour, _) in next {
            if !seen[neighbour] {
                seen[neighbour] = true;
                queue.push_back(neighbour);
            }
        }
    }
}

/// The local search from `start`: sweeps over the pairs of qubits until a
/// sweep swaps nothing ([`sweep`]). A swap changes only the spans of the
/// two qubits' pairs, so each swap is judged on those
/// ([`span_touching`]).
fn local_search(start: QubitOrder, weights: &InteractionWeights) -> QubitOrder {
    let neighbours = weights.neighbours();
    let mut sites = start.sites;
    while sweep(&mut sites, &neighbours) {}
    QubitOrder { sites }
}

/// One sweep of the local search over every pair of qubits `a < b`, `a`
/// ascending, then `b`. Keeps each swap of their `sites` that lowers the
/// span of the pairs they touch. Answers whether it kept a swap.
///
/// For instance, q0 and q2 share 5 gates, q1 and q2 share 1:
///
/// ```text
///  site      0    1    2
///  before    q0   q1   q2    span: 5 × |0 − 2| + 1 × |1 − 2| = 11
///  after     q1   q0   q2    span: 5 × |1 − 2| + 1 × |0 − 2| = 7, kept
/// ```
fn sweep(sites: &mut [usize], neighbours: &Neighbours) -> bool {
    let mut swapped = false;
    for a in 0..sites.len() {
        for b in a + 1..sites.len() {
            let before = span_touching(sites, neighbours, a, b);
            sites.swap(a, b);
            if span_touching(sites, neighbours, a, b) < before {
                swapped = true;
            } else {
                sites.swap(a, b);
            }
        }
    }
    swapped
}

/// The span of the pairs that qubit `a` or `b` is in, under `sites`: all
/// that a swap of their sites changes. A pair of `a` and `b` counts twice,
/// which leaves the comparison of a swap fair.
#[inline]
fn span_touching(sites: &[usize], neighbours: &Neighbours, a: usize, b: usize) -> u64 {
    [a, b]
        .iter()
        .flat_map(|&qubit| {
            neighbours[qubit]
                .iter()
                .map(move |&(neighbour, gates)| (qubit, neighbour, gates))
        })
        .map(|(qubit, neighbour, gates)| gates * sites[qubit].abs_diff(sites[neighbour]) as u64)
        .sum()
}

/// What the tests need of an order beyond what a run uses: to place each
/// qubit where a test chooses.
#[cfg(test)]
impl QubitOrder {
    /// Qubit `q` on site `sites[q]`; `sites` is a permutation of
    /// `0..sites.len()`.
    pub(super) fn from_sites(sites: Vec<usize>) -> Self {
        let mut sorted = sites.clone();
        sorted.sort_unstable();
        assert!(
            sorted.into_iter().eq(0..sites.len()),
            "the sites {sites:?} are a permutation"
        );
        Self { sites }
    }
}

#[cfg(test)]
mod tests;
