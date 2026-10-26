// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gate fusion: a region's gates as the operators an engine applies.
//!
//! A region ([`QuantumEvolutionRegion`]) is a run of unitary gates with no
//! measurement, reset or branch inside it, so the terminal may group its
//! gates as it likes, as long as the product is the same. The engine's cost
//! grows with the number of operators it contracts (one per
//! [`MpsEngine::apply`](tensornet::MpsEngine::apply)), so fewer, larger
//! operators may cost less; whether they do is measured on the A100
//! (`h2_qpe_on_the_a100` with `QDK_H2_FUSION`), not assumed.
//!
//! What the terminal needs of a fusion is [`GateFusion::operators`]: the
//! region's gates in, the operators to apply in order out, each a
//! [`SiteOperator`] in the layout `apply` takes. How the gates are grouped
//! stays behind it.

use std::collections::BTreeMap;

use num_complex::Complex64;
use qdk_simulators::{
    QubitID,
    execution::{UnitaryOperation, unitary_matrix},
};

/// One operator for [`MpsEngine::apply`](tensornet::MpsEngine::apply): a
/// unitary matrix on distinct sites, row-major `M[out][in]` over the sites'
/// joint index with `sites()[0]` the most significant bit, so `4^k`
/// elements on `k` sites.
#[derive(Clone, Debug, PartialEq)]
pub struct SiteOperator {
    sites: Vec<QubitID>,
    matrix: Vec<Complex64>,
}

impl SiteOperator {
    /// The sites it acts on, distinct; the first is the matrix's most
    /// significant bit.
    #[must_use]
    pub fn sites(&self) -> &[QubitID] {
        &self.sites
    }

    /// The matrix, row-major `M[out][in]`.
    #[must_use]
    pub fn matrix(&self) -> &[Complex64] {
        &self.matrix
    }
}

/// How a terminal turns a region's gates into the engine's operators.
///
/// Every fusion is exact: the product of its operators, in order, is the
/// product of the region's gates (to rounding, a few ulps per gate
/// multiplied in). Under truncation (a bond cap below the state's Schmidt
/// rank) the engine truncates after each operator, so fused and unfused
/// runs may differ within the truncation error; without truncation they
/// agree to rounding.
///
/// Fusion never crosses a region: a region ends at each measurement, reset
/// and branch, so no operator spans one.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GateFusion {
    /// One operator per gate, on its qubits in operand order, with the
    /// matrix of `qdk_simulators`' shared table; `I` is dropped. M0's ④
    /// applied gates so.
    Unfused,
    /// Operators on at most two sites: single-qubit gates join the
    /// two-qubit block open on their qubit (after it), or the next one that
    /// opens on it (before it); consecutive gates on the same pair multiply
    /// into one 4×4; a run of single-qubit gates on a qubit that no pair
    /// claims multiplies into one 2×2. A block's sites are in ascending
    /// order. Two sites is what every engine applies, cuTensorNet's
    /// included, so this is the largest grouping that needs nothing more of
    /// the engine.
    ///
    /// The default: on the A100, H₂'s first 100 measurements take 131 s
    /// instead of 176 s unfused, with 4.5× fewer operators. The gain is
    /// smaller than the operator count because the time goes to the
    /// two-qubit gates, which fusion keeps.
    #[default]
    Pairs,
}

impl GateFusion {
    /// The operators that apply `operations` (a region's, in order), in the
    /// order to apply them.
    ///
    /// The operations must be the ones a terminal checked
    /// (`lowering::check_operation`): qubits distinct within each.
    #[must_use]
    pub fn operators(self, operations: &[UnitaryOperation]) -> Vec<SiteOperator> {
        match self {
            Self::Unfused => operations.iter().filter_map(|&op| unfused(op)).collect(),
            Self::Pairs => blocks(operations, 2),
        }
    }
}

/// `operation` alone, in operand order, or `None` for `I`.
fn unfused(operation: UnitaryOperation) -> Option<SiteOperator> {
    let matrix = unitary_matrix(operation)?;
    let sites = match operation.qubits() {
        (first, None) => vec![first],
        (first, Some(second)) => vec![first, second],
    };
    Some(SiteOperator {
        sites,
        matrix: matrix.row_major().to_vec(),
    })
}

/// A block of gates being multiplied: its sites, ascending, and the product
/// so far, row-major on them.
struct Block {
    sites: Vec<QubitID>,
    matrix: Vec<Complex64>,
}

impl Block {
    /// The identity on `sites` (ascending).
    fn identity(sites: Vec<QubitID>) -> Self {
        let dimension = 1 << sites.len();
        let mut matrix = vec![Complex64::new(0.0, 0.0); dimension * dimension];
        for i in 0..dimension {
            matrix[i * dimension + i] = Complex64::new(1.0, 0.0);
        }
        Self { sites, matrix }
    }

    /// Multiplies `gate`, on `on` (a subset of the block's sites, in the
    /// gate's own order, the first its most significant bit), after the
    /// product so far: M ← G·M, with G embedded on the block's sites.
    fn multiply(&mut self, on: &[QubitID], gate: &[Complex64]) {
        let n = self.sites.len();
        let dimension = 1usize << n;
        // The bit of each of the gate's sites in the block's index, whose
        // first site is the most significant.
        let bits: Vec<usize> = on
            .iter()
            .map(|site| {
                let position = self.sites.iter().position(|s| s == site).unwrap_or(0);
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
        let mut column = vec![Complex64::new(0.0, 0.0); gate_dimension];
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

    fn into_operator(self) -> SiteOperator {
        SiteOperator {
            sites: self.sites,
            matrix: self.matrix,
        }
    }
}

/// Fuses `operations` into blocks of at most `max_sites` sites (at least
/// two, the widest gate's).
///
/// Each qubit belongs to at most one open block. A gate joins the open
/// blocks on its qubits, smallest first, while their union fits
/// `max_sites`; the blocks that do not fit are closed and emitted, and the
/// gate opens a new block with those that fit. At the end the open blocks
/// are emitted, oldest first.
///
/// The order is exact: a block is emitted when a later gate on one of its
/// qubits closes it, before the block that gate opens, so on each qubit the
/// blocks apply in the order of their gates; blocks emitted out of the
/// gates' order (an old block left open while newer ones close) share no
/// qubit with the blocks between, so they commute with them.
pub(crate) fn blocks(operations: &[UnitaryOperation], max_sites: usize) -> Vec<SiteOperator> {
    let mut emitted = Vec::new();
    // Open blocks by age; a closed or merged block's slot is `None`.
    let mut open: Vec<Option<Block>> = Vec::new();
    let mut owner: BTreeMap<QubitID, usize> = BTreeMap::new();
    for &operation in operations {
        let Some(gate) = unitary_matrix(operation) else {
            continue;
        };
        let on: Vec<QubitID> = match operation.qubits() {
            (first, None) => vec![first],
            (first, Some(second)) => vec![first, second],
        };
        let mut candidates: Vec<usize> = on.iter().filter_map(|q| owner.get(q).copied()).collect();
        candidates.sort_unstable();
        candidates.dedup();
        // Already one block that holds every qubit of the gate: multiply in
        // place.
        if let [id] = candidates.as_slice()
            && let Some(block) = open[*id].as_mut()
            && on.iter().all(|q| block.sites.contains(q))
        {
            block.multiply(&on, gate.row_major());
            continue;
        }
        candidates.sort_by_key(|&id| (open[id].as_ref().map_or(0, |b| b.sites.len()), id));
        let mut union: Vec<QubitID> = on.clone();
        let mut kept = Vec::new();
        for id in candidates {
            let Some(block) = open[id].take() else {
                continue;
            };
            let extra = block.sites.iter().filter(|q| !union.contains(q)).count();
            if union.len() + extra <= max_sites {
                union.extend(block.sites.iter().filter(|q| !on.contains(q)));
                union.sort_unstable();
                union.dedup();
                kept.push(block);
            } else {
                for q in &block.sites {
                    owner.remove(q);
                }
                emitted.push(block.into_operator());
            }
        }
        union.sort_unstable();
        let mut block = Block::identity(union);
        // The kept blocks are on disjoint qubits, so their order does not
        // matter; all of them precede the gate.
        for kept in kept {
            block.multiply(&kept.sites, &kept.matrix);
        }
        block.multiply(&on, gate.row_major());
        let id = open.len();
        for &q in &block.sites {
            owner.insert(q, id);
        }
        open.push(Some(block));
    }
    emitted.extend(open.into_iter().flatten().map(Block::into_operator));
    emitted
}
