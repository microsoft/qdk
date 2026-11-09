// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GateFusion`]: how the method groups a batch's gates into the operators
//! the engine applies.
//!
//! An engine's time goes to its operators (one per
//! [`MpsEngine::apply`](tensornet::MpsEngine::apply)), so fewer, larger
//! operators may cost less: on the A100, H₂'s first 100 measurements took
//! 131 s with gates fused in pairs and 176 s unfused, with 4.5× fewer
//! operators. Fusion is exact: the product of a run's operators, in order,
//! is the product of its gates, to rounding. Under truncation an engine
//! truncates after each operator, so fused and unfused runs may differ
//! within the truncation error.
//!
//! A run of gates ends at each measurement and reset, so no operator spans
//! one; each run is fused on its own.

use std::collections::BTreeMap;

use crate::qir::OperatorMatrix;
use num_complex::Complex64;

use crate::mps_simulator::resumer::lowering::{Operator, Step};

/// How the method groups the gates of each run between measurements and
/// resets into the engine's operators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GateFusion {
    /// One operator per gate, its own matrix on its qubits.
    Unfused,
    /// Operators on at most two sites, the most every engine applies: a
    /// one-qubit gate joins the two-site block open on its site (after it),
    /// or the next one that opens on it (before it); consecutive gates on
    /// the same two sites multiply into one 4×4; a run of one-qubit gates on
    /// a site no pair claims multiplies into one 2×2. The default.
    #[default]
    Pairs,
}

impl GateFusion {
    /// `steps` with each run of [`Step::Apply`] grouped as this fusion
    /// says; measurements and resets kept in place.
    pub(crate) fn fuse(self, steps: Vec<Step>) -> Vec<Step> {
        match self {
            Self::Unfused => steps,
            Self::Pairs => {
                let mut fused = Vec::with_capacity(steps.len());
                let mut run = Vec::new();
                for step in steps {
                    match step {
                        Step::Apply(operator) => run.push(operator),
                        other => {
                            fused.extend(pairs(&run).into_iter().map(Step::Apply));
                            run.clear();
                            fused.push(other);
                        }
                    }
                }
                fused.extend(pairs(&run).into_iter().map(Step::Apply));
                fused
            }
        }
    }
}

/// A block of gates being multiplied: its sites, ascending, and the product
/// so far, row-major on them, the first site the most significant.
struct Block {
    sites: Vec<usize>,
    matrix: Vec<Complex64>,
}

impl Block {
    /// The identity on `sites` (ascending).
    fn identity(sites: Vec<usize>) -> Self {
        let dimension = 1 << sites.len();
        let mut matrix = vec![Complex64::ZERO; dimension * dimension];
        for i in 0..dimension {
            matrix[i * dimension + i] = Complex64::ONE;
        }
        Self { sites, matrix }
    }

    /// Multiplies `gate`, on `on` (some of the block's sites, in the gate's
    /// order, the first its most significant bit), after the product so
    /// far: M ← G·M, with G embedded on the block's sites.
    fn multiply(&mut self, on: &[usize], gate: &[Complex64]) {
        let n = self.sites.len();
        let dimension = 1usize << n;
        // The bit of each of the gate's sites in the block's index.
        let bits: Vec<usize> = on
            .iter()
            .map(|site| {
                let position = self
                    .sites
                    .iter()
                    .position(|s| s == site)
                    .expect("a gate's sites are the block's");
                1 << (n - 1 - position)
            })
            .collect();
        let k = on.len();
        let gate_dimension = 1usize << k;
        let offsets: Vec<usize> = (0..gate_dimension)
            .map(|c| {
                bits.iter()
                    .enumerate()
                    .filter(|&(j, _)| c >> (k - 1 - j) & 1 == 1)
                    .fold(0, |offset, (_, &bit)| offset | bit)
            })
            .collect();
        let mask = bits.iter().fold(0, |mask, &bit| mask | bit);
        let mut column = vec![Complex64::ZERO; gate_dimension];
        // Each column of M is a vector on the block's sites; G acts on the
        // rows that differ in the gate's bits only.
        for input in 0..dimension {
            for base in (0..dimension).filter(|row| row & mask == 0) {
                for (value, &offset) in column.iter_mut().zip(&offsets) {
                    *value = self.matrix[(base | offset) * dimension + input];
                }
                for (row, &offset) in gate.chunks_exact(gate_dimension).zip(&offsets) {
                    self.matrix[(base | offset) * dimension + input] =
                        row.iter().zip(&column).map(|(g, v)| g * v).sum();
                }
            }
        }
    }

    fn into_operator(self) -> Operator {
        let matrix = match self.sites.len() {
            1 => OperatorMatrix::One(self.matrix.try_into().expect("a 2×2 block")),
            _ => OperatorMatrix::Two(self.matrix.try_into().expect("a 4×4 block")),
        };
        Operator {
            sites: [self.sites[0], self.sites.get(1).copied().unwrap_or(0)],
            matrix,
        }
    }
}

/// `run`'s gates fused into blocks of at most two sites.
///
/// Each site belongs to at most one open block. A gate joins the open
/// blocks on its sites, smallest first, while their union fits two sites;
/// the blocks that do not fit are closed and emitted, and the gate opens a
/// new block with those that fit. At the end the open blocks are emitted;
/// they share no site, so their order does not matter (oldest first).
///
/// The order is exact: a block is emitted when a later gate on one of its
/// sites closes it, before the block that gate opens, so on each site the
/// blocks apply in the order of their gates; blocks emitted out of the
/// gates' order share no site with the blocks between, so they commute
/// with them.
fn pairs(run: &[Operator]) -> Vec<Operator> {
    const MAX_SITES: usize = 2;
    let mut emitted = Vec::new();
    // Open blocks by age; a closed or merged block's slot is `None`.
    let mut open: Vec<Option<Block>> = Vec::new();
    let mut owner: BTreeMap<usize, usize> = BTreeMap::new();
    for gate in run {
        let on = gate.sites();
        let mut candidates: Vec<usize> = on.iter().filter_map(|s| owner.get(s).copied()).collect();
        candidates.sort_unstable();
        candidates.dedup();
        // One open block already holds every site of the gate.
        if let [id] = candidates.as_slice()
            && let Some(block) = open[*id].as_mut()
            && on.iter().all(|s| block.sites.contains(s))
        {
            block.multiply(on, gate.matrix.row_major());
            continue;
        }
        candidates.sort_by_key(|&id| (open[id].as_ref().map_or(0, |b| b.sites.len()), id));
        let mut union: Vec<usize> = on.to_vec();
        let mut kept = Vec::new();
        for id in candidates {
            let Some(block) = open[id].take() else {
                continue;
            };
            let extra = block.sites.iter().filter(|s| !union.contains(s)).count();
            if union.len() + extra <= MAX_SITES {
                union.extend(block.sites.iter().filter(|s| !on.contains(s)));
                union.sort_unstable();
                union.dedup();
                kept.push(block);
            } else {
                for s in &block.sites {
                    owner.remove(s);
                }
                emitted.push(block.into_operator());
            }
        }
        union.sort_unstable();
        let mut block = Block::identity(union);
        // The kept blocks are on disjoint sites, so their order does not
        // matter; all of them precede the gate.
        for kept in kept {
            block.multiply(&kept.sites, &kept.matrix);
        }
        block.multiply(on, gate.matrix.row_major());
        let id = open.len();
        for &s in &block.sites {
            owner.insert(s, id);
        }
        open.push(Some(block));
    }
    emitted.extend(open.into_iter().flatten().map(Block::into_operator));
    emitted
}
