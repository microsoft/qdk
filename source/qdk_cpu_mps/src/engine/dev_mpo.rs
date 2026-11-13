// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): applying an operator on
//! a few sites, possibly far apart, as a matrix product operator (MPO)
//! across the span of its sites, then compressing the span by one QR sweep
//! and one SVD sweep, instead of routing it by SWAPs.
//!
//! The operator, dense on its `k` sites, is decomposed into an MPO by `k − 1`
//! SVDs (a controlled rotation or a CNOT has operator Schmidt rank 2, so a
//! bond of 2); the sites between its sites carry the identity on the bond.
//! Each site of the span is multiplied by its MPO tensor, which multiplies
//! the bonds inside the span by the MPO's; a QR sweep from the span's first
//! site to its last then makes the span left-orthonormal, and an SVD sweep
//! back truncates each cut of the span, by the cutoff and the capacity, as
//! a split at rest does. So a gate on sites `d` apart takes `d` QRs and `d`
//! SVDs, against `2(d − 1) + 1` SVDs routed, and no SWAP's truncation.

use super::Engine;
use crate::{
    CpuMpsError,
    chain::{Chain, Site},
    linear_algebra::{LinearAlgebra, MatrixRef},
};
use num_complex::Complex64;
use std::time::Instant;

/// The singular values of an operator's decomposition below this fraction
/// of the largest are dropped: rounding's.
const OPERATOR_RANK_CUTOFF: f64 = 1e-12;

/// One site's MPO tensor `W[kl, p, kr]`, `p = 2·out + in`, at
/// `kl + left·(p + 4·kr)`.
struct MpoSite {
    left: usize,
    right: usize,
    w: Vec<Complex64>,
}

impl MpoSite {
    fn identity(bond: usize) -> Self {
        let mut w = vec![Complex64::ZERO; bond * 4 * bond];
        for k in 0..bond {
            for p in [0, 3] {
                w[k + bond * (p + 4 * k)] = Complex64::ONE;
            }
        }
        Self {
            left: bond,
            right: bond,
            w,
        }
    }
}

impl<A: LinearAlgebra> Engine<A> {
    /// Applies `matrix` (row-major, `sites[0]` the most significant bit) to
    /// `sites` (ascending, at least two) as an MPO, then compresses the span.
    pub(crate) fn dev_apply_operator(
        &self,
        chain: &mut Chain,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CpuMpsError> {
        let started = Instant::now();
        let k = sites.len();
        assert!(
            k >= 2 && sites.windows(2).all(|pair| pair[0] < pair[1]),
            "at least two ascending sites"
        );
        assert_eq!(matrix.len(), 1 << (2 * k), "a dense operator on the sites");
        let (lo, hi) = (sites[0], sites[k - 1]);
        let mut work = 0.0;
        let mut factors = self.operator_mpo(matrix, k, &mut work)?.into_iter();
        let mut span = Vec::with_capacity(hi - lo + 1);
        let mut bond = 1;
        for s in lo..=hi {
            let site = if sites.contains(&s) {
                factors.next().expect("one factor per site")
            } else {
                MpoSite::identity(bond)
            };
            bond = site.right;
            span.push(site);
        }
        self.split_pair_toward(chain, lo)?;
        let mut qrs = chain.centre.abs_diff(lo) as u64;
        self.move_centre(chain, lo)?;
        for (s, w) in (lo..=hi).zip(&span) {
            multiply_site(&mut chain.sites[s], w);
        }
        while chain.centre < hi {
            let site = &chain.sites[chain.centre];
            work += crate::dev_trace::svd_work(2 * site.left, site.right);
            self.shift_centre_right(chain)?;
            qrs += 1;
        }
        let mut svds = 0;
        for c in (lo + 1..=hi).rev() {
            let site = &chain.sites[c];
            work += crate::dev_trace::svd_work(site.left, 2 * site.right);
            self.dev_split_left(chain, c)?;
            svds += 1;
        }
        crate::dev_trace::record_mpo(svds, qrs, work, started.elapsed().as_secs_f64());
        Ok(())
    }

    /// The MPO of a dense operator on `k` sites, one tensor per site.
    fn operator_mpo(
        &self,
        matrix: &[Complex64],
        k: usize,
        work: &mut f64,
    ) -> Result<Vec<MpoSite>, CpuMpsError> {
        let dim = 1usize << k;
        // Index Σ_j p_j·4^j, p_j = 2·out_j + in_j, site 0 the fastest.
        let mut rest = vec![Complex64::ZERO; dim * dim];
        for row in 0..dim {
            for col in 0..dim {
                let mut index = 0;
                let mut scale = 1;
                for j in 0..k {
                    let out = (row >> (k - 1 - j)) & 1;
                    let input = (col >> (k - 1 - j)) & 1;
                    index += (2 * out + input) * scale;
                    scale *= 4;
                }
                rest[index] = matrix[row * dim + col];
            }
        }
        let mut factors = Vec::with_capacity(k);
        let mut left = 1;
        for _ in 0..k - 1 {
            let rows = 4 * left;
            let cols = rest.len() / rows;
            *work += crate::dev_trace::svd_work(rows, cols);
            let svd = self.algebra.svd(MatrixRef::new(rows, cols, &rest))?;
            let largest = svd.singular_values[0];
            let rank = svd.singular_values.len();
            let kept = svd
                .singular_values
                .iter()
                .take_while(|&&s| s > OPERATOR_RANK_CUTOFF * largest)
                .count()
                .max(1);
            let mut u = svd.u.into_elements();
            u.truncate(rows * kept);
            factors.push(MpoSite {
                left,
                right: kept,
                w: u,
            });
            let v_adjoint = svd.v_adjoint.into_elements();
            let mut next = Vec::with_capacity(kept * cols);
            for column in v_adjoint.chunks_exact(rank) {
                for (x, s) in column[..kept].iter().zip(&svd.singular_values) {
                    next.push(x * s);
                }
            }
            rest = next;
            left = kept;
        }
        factors.push(MpoSite {
            left,
            right: 1,
            w: rest,
        });
        Ok(factors)
    }

    /// Splits the centre site `c` by an SVD of its `left × 2·right` matrix,
    /// truncated as a split at rest at cut `c − 1`: `V†` stays on `c`, and
    /// `U·S` is multiplied into `c − 1`, the new centre.
    fn dev_split_left(&self, chain: &mut Chain, c: usize) -> Result<(), CpuMpsError> {
        let (left, right) = (chain.sites[c].left, chain.sites[c].right);
        let svd = self.algebra.svd(chain.sites[c].cols_right())?;
        let capacity = chain.capacity_bonds[c - 1];
        let need = self.kept(&svd.singular_values, usize::MAX);
        let bond = need.min(capacity);
        crate::dev_trace::record(&svd.singular_values, need, bond, capacity, c - 1);
        let rescale = Self::kept_rescale(chain, &svd.singular_values, bond)?;
        chain.statistics.splits += 1;
        chain.statistics.largest_bond_at_rest = chain.statistics.largest_bond_at_rest.max(bond);
        let rank = svd.singular_values.len();
        let v_adjoint = svd.v_adjoint.into_elements();
        let mut kept_rows = Vec::with_capacity(bond * 2 * right);
        for column in v_adjoint.chunks_exact(rank) {
            kept_rows.extend_from_slice(&column[..bond]);
        }
        let mut us = svd.u.into_elements();
        us.truncate(left * bond);
        for (column, value) in us.chunks_exact_mut(left).zip(&svd.singular_values) {
            for x in column {
                *x *= value * rescale;
            }
        }
        let previous = &chain.sites[c - 1];
        let merged = self
            .algebra
            .multiply(previous.rows_left(), MatrixRef::new(left, bond, &us));
        let previous_left = previous.left;
        chain.sites[c - 1] = Site {
            left: previous_left,
            right: bond,
            elements: merged.into_elements(),
        };
        chain.sites[c] = Site {
            left: bond,
            right,
            elements: kept_rows,
        };
        chain.centre = c - 1;
        Ok(())
    }
}

/// `T'[(a, kl), out, (b, kr)] = Σ_in W[kl, (out, in), kr]·T[a, in, b]`.
fn multiply_site(site: &mut Site, w: &MpoSite) {
    let (l, r) = (site.left, site.right);
    let (dl, dr) = (w.left, w.right);
    let new_left = l * dl;
    let mut elements = vec![Complex64::ZERO; new_left * 2 * r * dr];
    for kr in 0..dr {
        for kl in 0..dl {
            for out in 0..2 {
                for input in 0..2 {
                    let value = w.w[kl + dl * (2 * out + input + 4 * kr)];
                    if value == Complex64::ZERO {
                        continue;
                    }
                    for b in 0..r {
                        let from = l * (input + 2 * b);
                        let to = l * kl + new_left * (out + 2 * (b + r * kr));
                        for a in 0..l {
                            elements[to + a] += value * site.elements[from + a];
                        }
                    }
                }
            }
        }
    }
    *site = Site {
        left: new_left,
        right: r * dr,
        elements,
    };
}
