// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): an operator on a few
//! sites, possibly far apart, applied as a matrix product operator (MPO)
//! across the span of its sites, as `qdk_cpu_mps`'s `engine/dev_mpo.rs`
//! (`CpuMpsEngine::with_dev_mpo_far`), instead of routed by SWAPs.
//!
//! The steps are the CPU engine's, each split one `GateSplit`:
//! - the dense operator on its `k` sites becomes an MPO by `k − 1` SVDs on
//!   the host (a controlled rotation, or a controlled Pauli rotation of
//!   weight two, has bond 2); the sites between its sites carry the
//!   identity on the bond;
//! - with the centre in the span, each site of the span is downloaded, its
//!   pending gate applied, multiplied by its MPO tensor on the host, and
//!   uploaded: the bonds inside the span grow by the MPO's;
//! - a sweep of exact centre moves (the engine's `GateSplit` with no cutoff,
//!   in place of the CPU's QRs, or the CPU's QR moves with
//!   `with_dev_qr_moves`) from the span's first site to its last but one
//!   makes the span left-orthonormal;
//! - a sweep of truncating splits back (the cutoff, then the capacity's
//!   bond, as a gate's split at rest) compresses each cut of the span.
//!
//! So an operator on a span of `d + 1` sites costs `2d − 1` `GateSplit`s,
//! or `d − 1` QR moves and `d` `GateSplit`s (the CPU: `d` QRs and `d`
//! SVDs): the last move of the CPU's QR sweep is not needed, because a
//! `GateSplit` contracts the pair it splits.

use super::device_chain::{
    DeviceChain, DeviceSite, FusedGate, HostSite, IDENTITY, Truncation, kron,
};
use super::split::CentreSide;
use super::{CudaMpsEngine, CudaMpsError, TensorCalls};
use num_complex::Complex64;
use std::time::Instant;
use t4a_faer::{
    Auto, MatMut, MatRef, Par, ParShape,
    diag::Diag,
    dyn_stack::{MemBuffer, MemStack},
    linalg::svd::{ComputeSvdVectors, SvdParams, svd, svd_scratch},
};

/// The singular values of an operator's decomposition below this fraction
/// of the largest are dropped: rounding's (the CPU engine's).
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

impl<T: TensorCalls> CudaMpsEngine<T> {
    /// Applies `matrix` (row-major, `sites[0]` the most significant bit) to
    /// `sites` (distinct, in any order, at least two) as an MPO, then
    /// compresses the span, as `qdk_cpu_mps`'s `dev_apply_operator`.
    pub(super) fn apply_wide(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CudaMpsError> {
        if sites.windows(2).all(|pair| pair[0] < pair[1]) {
            return self.apply_mpo(chain, sites, matrix);
        }
        let (sorted, permuted) = ascending(sites, matrix);
        self.apply_mpo(chain, &sorted, &permuted)
    }

    /// [`Self::apply_wide`] on ascending sites.
    fn apply_mpo(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CudaMpsError> {
        let started = Instant::now();
        let k = sites.len();
        assert!(
            k >= 2 && sites.windows(2).all(|pair| pair[0] < pair[1]),
            "at least two ascending sites"
        );
        assert_eq!(matrix.len(), 1 << (2 * k), "a dense operator on the sites");
        let (lo, hi) = (sites[0], sites[k - 1]);
        let mut factors = operator_mpo(matrix, k)?.into_iter();
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
        // Any centre in the span leaves the sites left of it
        // left-orthonormal and those right of it right-orthonormal, which
        // is all the sweeps need; the CPU engine moves it to `lo`.
        self.split_pair_toward(chain, lo)?;
        let centre = chain.centre.clamp(lo, hi);
        self.move_centre(chain, centre)?;
        let host_started = Instant::now();
        for (s, w) in (lo..=hi).zip(&span) {
            let mut host = self.host_site(chain, s)?;
            chain.pending[s] = None;
            multiply_site(&mut host, w);
            chain.sites[s] = DeviceSite {
                left: host.left,
                right: host.right,
                buffer: self.tensors.upload(&host.elements)?,
            };
        }
        let host_seconds = host_started.elapsed().as_secs_f64();
        // The span is now any tensors: the sweep starts from its first site.
        chain.centre = lo;
        self.move_centre(chain, hi - 1)?;
        let identity = kron(&IDENTITY, &IDENTITY);
        for cut in (lo..hi).rev() {
            chain.fused = Some(FusedGate {
                cut,
                gate: identity,
                truncation: Truncation::CutoffAndCapacity,
            });
            self.split_pair(chain, CentreSide::Left)?;
        }
        let seconds = started.elapsed().as_secs_f64();
        self.count(|counters| {
            counters.mpo_operators += 1;
            counters.mpo_seconds += seconds;
            counters.mpo_host_seconds += host_seconds;
        });
        Ok(())
    }
}

/// The MPO of a dense operator (row-major, the first site the most
/// significant bit) on `k` sites, one tensor per site, by `k − 1` thin
/// SVDs, as `qdk_cpu_mps`'s `operator_mpo`.
fn operator_mpo(matrix: &[Complex64], k: usize) -> Result<Vec<MpoSite>, CudaMpsError> {
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
        let (u, values, v) = thin_svd(rows, cols, &rest)?;
        let largest = values[0];
        let kept = values
            .iter()
            .take_while(|&&s| s > OPERATOR_RANK_CUTOFF * largest)
            .count()
            .max(1);
        factors.push(MpoSite {
            left,
            right: kept,
            w: u[..rows * kept].to_vec(),
        });
        // `S·V†`, column-major `kept × cols`.
        let mut next = Vec::with_capacity(kept * cols);
        for col in 0..cols {
            for (kr, s) in values[..kept].iter().enumerate() {
                next.push(v[col + cols * kr].conj() * s);
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

/// `T'[(a, kl), out, (b, kr)] = Σ_in W[kl, (out, in), kr]·T[a, in, b]`, as
/// `qdk_cpu_mps`'s `multiply_site`.
fn multiply_site(site: &mut HostSite, w: &MpoSite) {
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
    *site = HostSite {
        left: new_left,
        right: r * dr,
        elements,
    };
}

/// `matrix` on `sites` (row-major, the first site the most significant
/// bit), as the same operator on the sites ascending, as `qdk_cpu_mps`'s
/// `dev_ascending`.
fn ascending(sites: &[usize], matrix: &[Complex64]) -> (Vec<usize>, Vec<Complex64>) {
    let k = sites.len();
    let dim = 1usize << k;
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by_key(|&i| sites[i]);
    let sorted: Vec<usize> = order.iter().map(|&i| sites[i]).collect();
    // The index in the old order of index `new` in the sorted order: the
    // sorted position j holds old position order[j].
    let old_index = |new: usize| {
        (0..k).fold(0, |old, j| {
            let bit = (new >> (k - 1 - j)) & 1;
            old | (bit << (k - 1 - order[j]))
        })
    };
    let map: Vec<usize> = (0..dim).map(old_index).collect();
    let mut permuted = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for column in 0..dim {
            permuted[row * dim + column] = matrix[map[row] * dim + map[column]];
        }
    }
    (sorted, permuted)
}

/// `U`, the singular values and `V` of a thin SVD.
type ThinSvd = (Vec<Complex64>, Vec<f64>, Vec<Complex64>);

/// The thin SVD `W = U·diag(S)·V†` of the column-major `rows × cols` `w`:
/// `U` (`rows × k`), the values nonincreasing, `V` (`cols × k`), with
/// `k = min(rows, cols)`.
fn thin_svd(rows: usize, cols: usize, w: &[Complex64]) -> Result<ThinSvd, CudaMpsError> {
    let k = rows.min(cols);
    let params = <SvdParams as Auto<Complex64>>::auto();
    let mut values = Diag::<Complex64>::zeros(k);
    let mut u = vec![Complex64::ZERO; rows * k];
    let mut v = vec![Complex64::ZERO; cols * k];
    let mut buffer = MemBuffer::new(svd_scratch::<Complex64>(
        rows,
        cols,
        ComputeSvdVectors::Thin,
        ComputeSvdVectors::Thin,
        ParShape::Seq,
        params.into(),
    ));
    svd(
        MatRef::from_column_major_slice(w, rows, cols),
        values.as_mut(),
        Some(MatMut::from_column_major_slice_mut(&mut u, rows, k)),
        Some(MatMut::from_column_major_slice_mut(&mut v, cols, k)),
        Par::Seq,
        MemStack::new(&mut buffer),
        params.into(),
    )
    .map_err(|error| CudaMpsError::TensorCallFailed(format!("faer's SVD: {error:?}")))?;
    let values = values.column_vector();
    Ok((u, (0..k).map(|i| values[i].re).collect(), v))
}
