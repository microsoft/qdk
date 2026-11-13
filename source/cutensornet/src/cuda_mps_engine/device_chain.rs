// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the state [`DeviceMps`] holds, a
//! chain of site tensors wherever [`TensorCalls`] keeps them, in canonical
//! form around one orthogonality centre, as `qdk_cpu_mps`'s `Chain`.
//!
//! What it adds to the CPU's chain lives on the host, so that a gate costs
//! no device call until a split needs one:
//! - one pending one-site gate per site, multiplied into the gate of the
//!   next `GateSplit` on the site, or applied to a downloaded copy by a
//!   read (a unitary on the physical index keeps the canonical form);
//! - the fused pair as a 4×4 gate, not as a contracted block: its two sites
//!   stay as they are until the split contracts them, gate and all.

use super::{CudaMpsError, TensorCalls};
use num_complex::Complex64;
use std::cell::RefCell;

/// The state of a [`CudaMpsEngine`](super::CudaMpsEngine): the chain, in a
/// `RefCell` because reads take `&State` and may split a fused pair or
/// move the centre, as the CPU engine's do.
pub struct DeviceMps<B> {
    pub(super) chain: RefCell<DeviceChain<B>>,
}

/// The chain: its sites, the centre, the capacity, and what gates left
/// pending on the host.
pub(super) struct DeviceChain<B> {
    pub(super) sites: Vec<DeviceSite<B>>,
    /// The one-site gate (row-major) each site's tensor still owes, if any.
    pub(super) pending: Vec<Option<[Complex64; 4]>>,
    pub(super) centre: usize,
    /// The capacity's bond at each cut.
    pub(super) capacity_bonds: Vec<usize>,
    pub(super) fused: Option<FusedGate>,
    /// The product of the fractions of their weight the truncating splits
    /// kept.
    pub(super) retained_weight: f64,
}

/// A site tensor `T[a, s, b]`, of extents `[left, 2, right]`.
pub(super) struct DeviceSite<B> {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) buffer: B,
}

impl<B> DeviceSite<B> {
    pub(super) fn len(&self) -> usize {
        2 * self.left * self.right
    }
}

/// The gates applied to the sites `cut` and `cut + 1` since their last
/// split, as one 4×4 gate (row-major, `cut` the most significant bit),
/// and what their split discards.
#[derive(Clone, Copy)]
pub(super) struct FusedGate {
    pub(super) cut: usize,
    pub(super) gate: [Complex64; 16],
    pub(super) truncation: Truncation,
}

/// What a split discards, as `qdk_cpu_mps`'s `Truncation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Truncation {
    /// A split at rest, after a gate or a return SWAP: the cutoff, then
    /// the capacity's bond.
    CutoffAndCapacity,
    /// A split after a route's outbound SWAP: the cutoff alone.
    CutoffOnly,
}

impl<B> DeviceChain<B> {
    /// `|0…0⟩` on `capacity_bonds.len() + 1` sites, the centre on site 0.
    pub(super) fn zero<T: TensorCalls<Buffer = B>>(
        tensors: &T,
        capacity_bonds: Vec<usize>,
    ) -> Result<Self, CudaMpsError> {
        let n = capacity_bonds.len() + 1;
        let sites = (0..n)
            .map(|_| {
                Ok(DeviceSite {
                    left: 1,
                    right: 1,
                    buffer: tensors.upload(&[c(1.0), c(0.0)])?,
                })
            })
            .collect::<Result<_, CudaMpsError>>()?;
        Ok(Self {
            sites,
            pending: vec![None; n],
            centre: 0,
            capacity_bonds,
            fused: None,
            retained_weight: 1.0,
        })
    }

    /// A copy whose sites are new buffers.
    pub(super) fn duplicate<T: TensorCalls<Buffer = B>>(
        &self,
        tensors: &T,
    ) -> Result<Self, CudaMpsError> {
        let sites = self
            .sites
            .iter()
            .map(|site| {
                Ok(DeviceSite {
                    left: site.left,
                    right: site.right,
                    buffer: tensors.duplicate(&site.buffer, site.len())?,
                })
            })
            .collect::<Result<_, CudaMpsError>>()?;
        Ok(Self {
            sites,
            pending: self.pending.clone(),
            centre: self.centre,
            capacity_bonds: self.capacity_bonds.clone(),
            fused: self.fused,
            retained_weight: self.retained_weight,
        })
    }

    /// Applies the one-site gate `m` (row-major) to `site`: into the fused
    /// gate when `site` is one of its pair's, as `qdk_cpu_mps`'s
    /// `Chain::apply_one`; otherwise into the site's pending gate.
    pub(super) fn apply_one(&mut self, site: usize, m: &[Complex64]) {
        let m: [Complex64; 4] = std::array::from_fn(|at| m[at]);
        match self.fused.as_mut() {
            Some(fused) if fused.cut == site || fused.cut + 1 == site => {
                let gate = if fused.cut == site {
                    kron(&m, &IDENTITY)
                } else {
                    kron(&IDENTITY, &m)
                };
                fused.gate = multiply4(&gate, &fused.gate);
            }
            _ => {
                let pending = &mut self.pending[site];
                *pending = Some(match pending {
                    Some(earlier) => multiply2(&m, earlier),
                    None => m,
                });
            }
        }
    }

    /// The pending gates of `cut` and `cut + 1` as one 4×4 gate, cleared:
    /// what the next `GateSplit` on the pair applies first.
    pub(super) fn take_pending_pair(&mut self, cut: usize) -> [Complex64; 16] {
        let left = self.pending[cut].take().unwrap_or(IDENTITY);
        let right = self.pending[cut + 1].take().unwrap_or(IDENTITY);
        kron(&left, &right)
    }
}

pub(super) const fn c(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

pub(super) const IDENTITY: [Complex64; 4] = [c(1.0), c(0.0), c(0.0), c(1.0)];

/// The SWAP gate, row-major: a route's step.
pub(super) const SWAP: [Complex64; 16] = [
    c(1.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(1.0),
    c(0.0),
    c(0.0),
    c(1.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(1.0),
];

/// `a ⊗ b` of two one-site gates (row-major), `a` on the most significant
/// bit.
pub(super) fn kron(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 4, at % 4);
        a[2 * (row >> 1) + (col >> 1)] * b[2 * (row & 1) + (col & 1)]
    })
}

/// `a·b` of two row-major 4×4 matrices.
pub(super) fn multiply4(a: &[Complex64; 16], b: &[Complex64; 16]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 4, at % 4);
        (0..4).map(|k| a[4 * row + k] * b[4 * k + col]).sum()
    })
}

/// `a·b` of two row-major 2×2 matrices.
fn multiply2(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 4] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 2, at % 2);
        (0..2).map(|k| a[2 * row + k] * b[2 * k + col]).sum()
    })
}

/// A two-site gate on `[b, a]` rewritten for `[a, b]`, as `qdk_cpu_mps`'s
/// `swap_operands`.
pub(super) fn swap_operands(m: &[Complex64]) -> [Complex64; 16] {
    let flip = |index: usize| ((index & 1) << 1) | (index >> 1);
    std::array::from_fn(|at| m[4 * flip(at / 4) + flip(at % 4)])
}

/// A site's elements on the host, `T[a, s, b]` at `a + left·(s + 2b)`,
/// with the reads `qdk_cpu_mps`'s `Site` makes.
pub(super) struct HostSite {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) elements: Vec<Complex64>,
}

impl HostSite {
    /// The squared norm of the elements whose physical index is `level`.
    pub(super) fn weight_of(&self, level: usize) -> f64 {
        let l = self.left;
        (0..self.right)
            .map(|b| {
                let start = l * (level + 2 * b);
                self.elements[start..start + l]
                    .iter()
                    .map(Complex64::norm_sqr)
                    .sum::<f64>()
            })
            .sum()
    }

    pub(super) fn weight(&self) -> f64 {
        self.elements.iter().map(Complex64::norm_sqr).sum()
    }

    /// Applies the one-site gate `m` (row-major) to the physical index.
    pub(super) fn apply(&mut self, m: &[Complex64]) {
        let l = self.left;
        for b in 0..self.right {
            for a in 0..l {
                let (i0, i1) = (a + 2 * l * b, a + l + 2 * l * b);
                let (v0, v1) = (self.elements[i0], self.elements[i1]);
                self.elements[i0] = m[0] * v0 + m[1] * v1;
                self.elements[i1] = m[2] * v0 + m[3] * v1;
            }
        }
    }

    /// Keeps the elements whose physical index is `level` and divides the
    /// site by their norm, computed scaled, as `qdk_cpu_mps`'s
    /// `Site::project`; the kept elements are not all zero.
    pub(super) fn project(&mut self, level: usize) {
        let l = self.left;
        for (at, x) in self.elements.iter_mut().enumerate() {
            if (at / l) % 2 != level {
                *x = c(0.0);
            }
        }
        let largest = self.elements.iter().map(|x| x.norm()).fold(0.0, f64::max);
        let scaled: f64 = self.elements.iter().map(|x| (x / largest).norm_sqr()).sum();
        let norm = scaled.sqrt();
        for x in &mut self.elements {
            *x = *x / largest / norm;
        }
    }
}
