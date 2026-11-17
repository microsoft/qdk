// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A window applied as a matrix product operator (MPO) across its span:
//! the window's operator factored into one tensor per site, the sites
//! between carrying the identity, multiplied into the sites, and the span
//! compressed back to the capacity by a sweep of splits.
//!
//! A window on sites that are not neighbours, or on three sites, cannot be
//! one split: its operator acts on more than a pair. Bringing its sites
//! together by SWAPs and back (routing) costs two splits per site
//! crossed, each a truncation; the MPO, once the centre is in its span,
//! costs one split per cut of the span and one QR move fewer, and
//! truncates only at the caller's cuts (the crate's
//! `docs/distant_sites.md` compares them). The steps:
//!
//! 1. **Factor.** The operator on `k` sites becomes `k` tensors by `k − 1`
//!    thin SVDs on the host ([`operator_mpo`]), dropping only rounding's
//!    values ([`OPERATOR_RANK_CUTOFF`]): a CNOT has an operator bond of 2,
//!    a general two-site gate at most 4.
//! 2. **Multiply.** With the centre moved into the span, each site of the
//!    span is read, its pending gate applied, multiplied by its MPO tensor
//!    on the host, and uploaded: the bond at each cut of the span grows by
//!    the operator's bond there, a transient bond
//!    ([`SplitStatistics::largest_transient_bond`]).
//! 3. **Sweep.** The span is now any tensors, while the sites outside it
//!    are still isometries towards it. Exact moves of the centre
//!    ([`qr_move`](SiteAlgebra::qr_move)) from the span's near end (the
//!    end nearer the site the caller names) to the site next to the far
//!    end make every site they leave an isometry; then one
//!    [`gate_split`](SiteAlgebra::gate_split) per cut, the identity as
//!    gate, from the far end back to the near end, truncates each cut as a
//!    gate's split at rest, and leaves the centre at the near end.
//!
//! So a window on a span of `d + 1` sites costs the moves that bring the
//! centre into the span, then `d − 1` QR moves and `d` splits. Every call
//! is one the algebra already makes for a gate or a move of the centre;
//! only the bonds inside the span are larger, up to the capacity's times
//! the operator's bond.
//!
//! [`SplitStatistics::largest_transient_bond`]: super::SplitStatistics::largest_transient_bond

use super::{
    CanonicalMpsEngine, CanonicalMpsError,
    chain::{Chain, Site},
    policy::OPERATOR_RANK_CUTOFF,
    reads::HostSite,
};
use crate::algebra::{CentreSide, SiteAlgebra};
use dense_linalg::{MatrixRef, Svd};
use num_complex::Complex64;
use std::borrow::Cow;

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `operator` (row-major, `sites[0]` the most significant bit)
    /// to `sites` (ascending, at least two) as an MPO, compresses the span,
    /// and leaves the centre on the span's end nearer `toward`.
    pub(super) fn apply_mpo(
        &self,
        chain: &mut Chain<A::Tensor>,
        sites: &[usize],
        operator: &[Complex64],
        toward: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let (low, high) = (sites[0], sites[sites.len() - 1]);
        let mut factors = operator_mpo(operator, sites.len())
            .map_err(CanonicalMpsError::OperatorDecomposition)?
            .into_iter();
        let centre = chain.centre.clamp(low, high);
        self.move_centre(chain, centre)?;
        // The span's sites are committed only once every upload succeeded,
        // so a failed upload leaves the chain's neighbours sharing each
        // bond, as before the window.
        let mut multiplied_span = Vec::with_capacity(high - low + 1);
        let mut bond = 1;
        for at in low..=high {
            let factor = if sites.contains(&at) {
                factors.next().expect("one factor per site of the window")
            } else {
                MpoSite::identity(bond)
            };
            bond = factor.right;
            let multiplied = self.host_site(chain, at)?.multiplied_by(&factor);
            let tensor = self
                .algebra
                .upload(&multiplied.elements)
                .map_err(CanonicalMpsError::Algebra)?;
            multiplied_span.push(Site {
                left: multiplied.left,
                right: multiplied.right,
                tensor,
            });
        }
        for (at, site) in (low..=high).zip(multiplied_span) {
            if at < high {
                let largest = &mut chain.statistics.largest_transient_bond;
                *largest = (*largest).max(site.right);
            }
            chain.sites[at] = site;
            chain.pending[at] = None;
        }
        let identity = std::array::from_fn(|at| {
            if at % 5 == 0 {
                Complex64::ONE
            } else {
                Complex64::ZERO
            }
        });
        // The multiplied span holds no isometry, so the moves may start at
        // either end: they start at the near one, so that the splits, which
        // come back, leave the centre there.
        if toward.abs_diff(high) < toward.abs_diff(low) {
            chain.centre = high;
            self.move_centre(chain, low + 1)?;
            for cut in low..high {
                self.split(chain, cut, identity, CentreSide::Right)?;
            }
        } else {
            chain.centre = low;
            self.move_centre(chain, high - 1)?;
            for cut in (low..high).rev() {
                self.split(chain, cut, identity, CentreSide::Left)?;
            }
        }
        chain.statistics.mpo_applications += 1;
        Ok(())
    }
}

/// One site's MPO tensor `W[kl, p, kr]`, `p = 2·out + in`, at
/// `kl + left·(p + 4·kr)`.
pub(super) struct MpoSite {
    left: usize,
    right: usize,
    w: Vec<Complex64>,
}

impl MpoSite {
    /// The identity on a site between the window's, carrying a bond of
    /// `bond`.
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

/// The MPO of `operator` (row-major, the first site the most significant
/// bit) on `k` sites, one tensor per site, by `k − 1` thin SVDs, each
/// keeping the values above [`OPERATOR_RANK_CUTOFF`] times the largest.
pub(super) fn operator_mpo(
    operator: &[Complex64],
    k: usize,
) -> Result<Vec<MpoSite>, dense_linalg::SvdError> {
    let dim = 1usize << k;
    debug_assert_eq!(operator.len(), dim * dim, "an operator on `k` sites");
    // Index Σ_j p_j·4^j, p_j = 2·out_j + in_j, site 0 the fastest: as a
    // column-major matrix of `4·left` rows, the first site's index and the
    // bond on its left are the rows.
    let mut rest = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for col in 0..dim {
            let (mut index, mut scale) = (0, 1);
            for j in 0..k {
                let out = (row >> (k - 1 - j)) & 1;
                let input = (col >> (k - 1 - j)) & 1;
                index += (2 * out + input) * scale;
                scale *= 4;
            }
            rest[index] = operator[row * dim + col];
        }
    }
    let mut factors = Vec::with_capacity(k);
    let mut left = 1;
    for _ in 0..k - 1 {
        let rows = 4 * left;
        let cols = rest.len() / rows;
        let svd = Svd::of(MatrixRef::new(rows, cols, &rest))?;
        let values = &svd.singular_values;
        let kept = values
            .iter()
            .take_while(|&&s| s > OPERATOR_RANK_CUTOFF * values[0])
            .count()
            .max(1);
        factors.push(MpoSite {
            left,
            right: kept,
            w: svd.u.elements()[..rows * kept].to_vec(),
        });
        // `S·V†`, column-major `kept × cols`.
        let v_adjoint = svd.v_adjoint.elements();
        let v_rows = svd.v_adjoint.rows();
        rest = (0..cols)
            .flat_map(|col| (0..kept).map(move |kr| (col, kr)))
            .map(|(col, kr)| v_adjoint[kr + v_rows * col] * values[kr])
            .collect();
        left = kept;
    }
    factors.push(MpoSite {
        left,
        right: 1,
        w: rest,
    });
    Ok(factors)
}

impl HostSite<'_> {
    /// `T'[(a, kl), out, (b, kr)] = Σ_in W[kl, (out, in), kr]·T[a, in, b]`:
    /// the site multiplied by its MPO tensor `w`.
    fn multiplied_by(&self, w: &MpoSite) -> HostSite<'static> {
        let (l, r) = (self.left, self.right);
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
                                elements[to + a] += value * self.elements[from + a];
                            }
                        }
                    }
                }
            }
        }
        HostSite {
            left: new_left,
            right: r * dr,
            elements: Cow::Owned(elements),
        }
    }
}
