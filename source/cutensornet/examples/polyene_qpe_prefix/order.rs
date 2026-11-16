// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the chain orders and the `seq3`
//! fusion of `simulators`' `polyene_swaps_dev`, copied for the GPU engines
//! (`--order`, `--variant seq3-mpo` in `main.rs`). `positions` keeps two of
//! its orders, the CPU's baseline and its winner; `seq3` is its `fuse` with
//! `Fusion::Seq(3)`.

use crate::inputs::{Gate, Hamiltonian, QpePrefix, Reference, Res};
use num_complex::Complex64;

/// The site of each qubit for the order `name`: `identity` (qubit `i` on
/// site `i`) or `interleave-centre` (α and β modes alternate, the ancilla
/// in the middle).
pub fn positions(name: &str, hamiltonian: &Hamiltonian, qubits: usize) -> Res<Vec<usize>> {
    let order: Vec<usize> = match name {
        "identity" => (0..qubits).collect(),
        "interleave-centre" => {
            let mut order: Vec<usize> = hamiltonian
                .alpha
                .iter()
                .zip(&hamiltonian.beta)
                .flat_map(|(&a, &b)| [a, b])
                .collect();
            order.insert(order.len() / 2, hamiltonian.ancilla);
            order
        }
        other => return Err(format!("--order {other}: identity or interleave-centre").into()),
    };
    let mut position = vec![usize::MAX; qubits];
    for (site, &qubit) in order.iter().enumerate() {
        if qubit >= qubits || position[qubit] != usize::MAX {
            return Err(format!("--order {name}: not a permutation of 0..{qubits}").into());
        }
        position[qubit] = site;
    }
    if order.len() != qubits {
        return Err(format!("--order {name}: not a permutation of 0..{qubits}").into());
    }
    Ok(position)
}

/// Moves every qubit `q` of the case to site `position[q]`.
pub fn relabel(
    position: &[usize],
    prefix: &mut QpePrefix,
    hamiltonian: &mut Hamiltonian,
    reference: Option<&mut Reference>,
) {
    for gate in prefix.trial.iter_mut().chain(&mut prefix.controlled_step) {
        for qubit in &mut gate.qubits {
            *qubit = position[*qubit];
        }
    }
    for mode in hamiltonian.alpha.iter_mut().chain(&mut hamiltonian.beta) {
        *mode = position[*mode];
    }
    hamiltonian.ancilla = position[hamiltonian.ancilla];
    for (_, factors) in &mut hamiltonian.terms {
        for (qubit, _) in factors {
            *qubit = position[*qubit];
        }
    }
    if let Some(reference) = reference {
        for (qubit, _) in &mut reference.probabilities_of_one {
            *qubit = position[*qubit];
        }
        for (_, factors) in &mut reference.pauli_strings {
            for (qubit, _) in factors {
                *qubit = position[*qubit];
            }
        }
    }
}

/// `gates` fused in order: the open block takes the next gate while their
/// sites are at most three, and never moves a gate past another. Each
/// operator is on ascending sites.
pub fn seq3(gates: &[Gate]) -> Vec<Gate> {
    let mut blocks: Vec<Gate> = Vec::new();
    let mut open: Option<Gate> = None;
    for gate in gates {
        let mut sites = gate.qubits.clone();
        sites.sort_unstable();
        if let Some(block) = open.as_mut() {
            let mut union = block.qubits.clone();
            union.extend(&sites);
            union.sort_unstable();
            union.dedup();
            if union.len() <= 3 {
                if union != block.qubits {
                    *block = widened(block, union);
                }
                then(block, &gate.qubits, &gate.matrix);
                continue;
            }
            blocks.extend(open.take());
        }
        let mut block = identity(sites);
        then(&mut block, &gate.qubits, &gate.matrix);
        open = Some(block);
    }
    blocks.extend(open);
    blocks
}

fn identity(sites: Vec<usize>) -> Gate {
    let dim = 1 << sites.len();
    let mut matrix = vec![Complex64::ZERO; dim * dim];
    for i in 0..dim {
        matrix[i * dim + i] = Complex64::ONE;
    }
    Gate {
        qubits: sites,
        matrix,
    }
}

/// `gate` (on `qubits`, a subset of the block's sites, in any order), as a
/// matrix on the block's sites.
fn embed(block: &Gate, qubits: &[usize], gate: &[Complex64]) -> Vec<Complex64> {
    let k = block.qubits.len();
    let dim = 1 << k;
    let bit = |site: usize| {
        let at = block
            .qubits
            .iter()
            .position(|&s| s == site)
            .expect("a site of the block");
        k - 1 - at
    };
    let bits: Vec<usize> = qubits.iter().map(|&q| bit(q)).collect();
    let mask: usize = bits.iter().map(|&b| 1 << b).sum();
    let index = |x: usize| bits.iter().fold(0, |acc, &b| (acc << 1) | ((x >> b) & 1));
    let gdim = 1 << qubits.len();
    let mut out = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for col in 0..dim {
            if row & !mask == col & !mask {
                out[row * dim + col] = gate[index(row) * gdim + index(col)];
            }
        }
    }
    out
}

/// Applies `gate` after the block; `qubits` must be among its sites.
fn then(block: &mut Gate, qubits: &[usize], gate: &[Complex64]) {
    let g = embed(block, qubits, gate);
    let dim = 1 << block.qubits.len();
    let mut product = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for mid in 0..dim {
            let x = g[row * dim + mid];
            if x == Complex64::ZERO {
                continue;
            }
            for col in 0..dim {
                product[row * dim + col] += x * block.matrix[mid * dim + col];
            }
        }
    }
    block.matrix = product;
}

/// The block extended to `sites` (a superset, ascending).
fn widened(block: &Gate, sites: Vec<usize>) -> Gate {
    let mut wide = identity(sites);
    then(&mut wide, &block.qubits, &block.matrix);
    wide
}
