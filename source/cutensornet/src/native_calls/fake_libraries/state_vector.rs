// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The state vector the fake libraries compute with, and the truncation
//! a bond imposes on it at a cut.

use crate::engine::policy::DISCARDED_WEIGHT_CUTOFF;
use num_complex::Complex64;

/// A state vector of qubits, site `k` the bit `k` of the index, that the
/// fake computes with. Unlike the contract's reference, it applies any
/// matrix: the projections, the truncations and the observables' factors.
#[derive(Clone)]
pub(super) struct StateVector {
    sites: usize,
    amplitudes: Vec<Complex64>,
}

impl StateVector {
    pub(super) fn zero(sites: usize) -> Self {
        let mut amplitudes = vec![Complex64::new(0.0, 0.0); 1 << sites];
        amplitudes[0] = Complex64::new(1.0, 0.0);
        Self { sites, amplitudes }
    }

    /// ψ ← (M ⊗ I)ψ for the row-major `matrix` on `sites`, `sites[0]` the
    /// most significant bit of its index, as `MpsEngine::apply` reads it.
    pub(super) fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) {
        let (count, dim) = (sites.len(), 1 << sites.len());
        let mask: usize = sites.iter().map(|&site| 1 << site).sum();
        let index = |base: usize, local: usize| {
            sites.iter().enumerate().fold(base, |index, (at, &site)| {
                if (local >> (count - 1 - at)) & 1 == 1 {
                    index | (1 << site)
                } else {
                    index
                }
            })
        };
        for base in (0..self.amplitudes.len()).filter(|base| base & mask == 0) {
            let old: Vec<Complex64> = (0..dim).map(|l| self.amplitudes[index(base, l)]).collect();
            for out in 0..dim {
                self.amplitudes[index(base, out)] =
                    (0..dim).map(|l| matrix[out * dim + l] * old[l]).sum();
            }
        }
    }

    pub(super) fn inner(&self, ket: &Self) -> Complex64 {
        self.amplitudes
            .iter()
            .zip(&ket.amplitudes)
            .map(|(bra, ket)| bra.conj() * ket)
            .sum()
    }

    /// The reduced density matrix of sites `0..=cut`, row-major:
    /// `ρ[a][b] = Σ_c ψ(a + c·2^(cut+1))·conj(ψ(b + c·2^(cut+1)))`.
    fn left_density(&self, cut: usize) -> (Vec<Complex64>, usize) {
        let left = 1_usize << (cut + 1);
        let right = 1_usize << (self.sites - cut - 1);
        let mut density = vec![Complex64::new(0.0, 0.0); left * left];
        for a in 0..left {
            for b in 0..left {
                density[a * left + b] = (0..right)
                    .map(|c| self.amplitudes[a + c * left] * self.amplitudes[b + c * left].conj())
                    .sum();
            }
        }
        (density, left)
    }

    /// How many squared Schmidt values at the cut after site `cut` exceed
    /// `fraction` of their total.
    pub(super) fn rank_above(&self, cut: usize, fraction: f64) -> usize {
        let (density, left) = self.left_density(cut);
        let (values, _) = hermitian_eigen(density, left);
        let total: f64 = values.iter().sum();
        values
            .iter()
            .filter(|&&value| value > fraction * total)
            .count()
    }

    /// How many Schmidt values cuTensorNet keeps at the cut after site
    /// `cut`, before the bond: all but the smallest whose squares sum to at
    /// most the engine's `DISCARDED_WEIGHT_CUTOFF` of their total, which
    /// also drops the rounding a projection leaves.
    pub(super) fn kept_rank(&self, cut: usize) -> usize {
        let (density, left) = self.left_density(cut);
        let (mut values, _) = hermitian_eigen(density, left);
        values.sort_by(f64::total_cmp);
        let total: f64 = values.iter().sum();
        let mut discarded = 0.0;
        let dropped = values
            .iter()
            .take_while(|&&value| {
                discarded += value;
                discarded <= DISCARDED_WEIGHT_CUTOFF * total
            })
            .count();
        (left - dropped).max(1)
    }
}

/// Keeps the `bond` largest Schmidt values of `state` across the cut after
/// site `cut`: projects sites `0..=cut` on the eigenvectors of their reduced
/// density matrix `ρ[a][b] = Σ_c ψ(a + c·2^(cut+1))·conj(ψ(b + c·2^(cut+1)))`
/// with the `bond` largest eigenvalues, without renormalizing.
pub(super) fn truncate(state: &mut StateVector, cut: usize, bond: usize) {
    let (density, left) = state.left_density(cut);
    let (values, vectors) = hermitian_eigen(density, left);
    let mut order: Vec<usize> = (0..left).collect();
    order.sort_by(|&x, &y| values[y].total_cmp(&values[x]));
    let kept = &order[..bond.min(left)];
    let projector: Vec<Complex64> = (0..left * left)
        .map(|at| {
            let (a, b) = (at / left, at % left);
            kept.iter()
                .map(|&j| vectors[a * left + j] * vectors[b * left + j].conj())
                .sum()
        })
        .collect();
    // `apply` reads its first site as the most significant bit of the
    // local index, so site `k` is bit `k` with the sites from `cut` down.
    let sites: Vec<usize> = (0..=cut).rev().collect();
    state.apply(&sites, &projector);
}

/// The eigenvalues of the Hermitian `order`×`order` row-major `matrix` and its
/// eigenvectors, as the columns of a row-major matrix, by cyclic Jacobi
/// rotations: each rotation zeroes one off-diagonal element, and sweeps
/// repeat until the off-diagonal weight is rounding.
fn hermitian_eigen(mut matrix: Vec<Complex64>, order: usize) -> (Vec<f64>, Vec<Complex64>) {
    let zero = Complex64::new(0.0, 0.0);
    let mut vectors: Vec<Complex64> = (0..order * order)
        .map(|at| {
            if at / order == at % order {
                Complex64::new(1.0, 0.0)
            } else {
                zero
            }
        })
        .collect();
    let scale: f64 = matrix.iter().map(Complex64::norm_sqr).sum();
    for _ in 0..64 {
        let off: f64 = (0..order * order)
            .filter(|at| at / order != at % order)
            .map(|at| matrix[at].norm_sqr())
            .sum();
        if off <= 1e-30 * scale.max(f64::MIN_POSITIVE) {
            break;
        }
        for p in 0..order {
            for q in p + 1..order {
                let element = matrix[p * order + q];
                let magnitude = element.norm();
                if magnitude == 0.0 {
                    continue;
                }
                // G = diag(1, conj(phase)) · [[cos, sin], [−sin, cos]] on
                // (p, q) makes the element real, then zeroes it (Numerical
                // Recipes' rotation, ch. 11.1).
                let phase = element / magnitude;
                let tau = (matrix[q * order + q].re - matrix[p * order + p].re) / (2.0 * magnitude);
                let tangent = tau.signum() / (tau.abs() + tau.mul_add(tau, 1.0).sqrt());
                let cosine = 1.0 / tangent.mul_add(tangent, 1.0).sqrt();
                let sine = tangent * cosine;
                let (gpq, gqp, gqq) = (
                    Complex64::new(sine, 0.0),
                    -phase.conj() * sine,
                    phase.conj() * cosine,
                );
                for k in 0..order {
                    for m in [&mut matrix, &mut vectors] {
                        let (kp, kq) = (m[k * order + p], m[k * order + q]);
                        m[k * order + p] = kp * cosine + kq * gqp;
                        m[k * order + q] = kp * gpq + kq * gqq;
                    }
                }
                for k in 0..order {
                    let (pk, qk) = (matrix[p * order + k], matrix[q * order + k]);
                    matrix[p * order + k] = pk * cosine + qk * gqp.conj();
                    matrix[q * order + k] = pk * gpq.conj() + qk * gqq.conj();
                }
            }
        }
    }
    (
        (0..order).map(|i| matrix[i * order + i].re).collect(),
        vectors,
    )
}
