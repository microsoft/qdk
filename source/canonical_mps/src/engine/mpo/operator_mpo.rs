// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A window's operator factored into one MPO tensor per window site.

use super::mpo_site::MpoSite;
use crate::engine::cutoffs::OPERATOR_RANK_CUTOFF;
use dense_linalg::{MatrixRef, Svd, SvdError};
use num_complex::Complex64;
use std::ops::RangeInclusive;

/// An operator on a few sites as one MPO tensor per site, in site order.
pub(super) struct OperatorMpo {
    sites: Vec<MpoSite>,
}

impl OperatorMpo {
    /// The MPO of `operator` (row-major, the first site the most
    /// significant bit) on `site_count` sites. The operator is rearranged
    /// with one index per site, then factored, one site at a time from the
    /// first, by `site_count − 1` thin SVDs on the host.
    ///
    /// Each SVD keeps, as the operator bond, as many singular values as the
    /// operator needs. Each already leaves out the values its trim finds to
    /// be rounding errors (`dense_linalg`'s SVD). The factoring also drops
    /// the values at or below [`OPERATOR_RANK_CUTOFF`] times the largest.
    /// What it drops is rounding the trim kept, or a part of the operator of
    /// at most that relative size. The retained weight does not count it. A
    /// CNOT has an operator bond of 2, a general two-site gate at most 4.
    ///
    /// ```text
    ///  a CNOT on sites 1 and 4, one index per site:
    ///      ┌────────────┐
    ///      │  operator  │          p = 2·out + in on each site
    ///      └──┬──────┬──┘
    ///         p₁     p₄
    ///  factor off site 1:  operator = U·S·V†   →   W₁ ─2─ rest
    ///                      (U is W₁, S·V† the rest, 2 values kept)
    ///  the last site:      the rest is W₄      →   W₁ ─2─ W₄
    /// ```
    pub(super) fn of(operator: &[Complex64], site_count: usize) -> Result<Self, SvdError> {
        let mut rest = one_index_per_site(operator, site_count);
        let mut sites = Vec::with_capacity(site_count);
        let mut left = 1;
        for _ in 0..site_count - 1 {
            let (site, remainder) = factor_off_next_site(&rest, left)?;
            left = site.right;
            sites.push(site);
            rest = remainder;
        }
        sites.push(MpoSite {
            left,
            right: 1,
            elements: rest,
        });
        Ok(Self { sites })
    }

    /// One MPO tensor per site of `span`, the sites from the window's
    /// first to its last, in order: the next of the operator's on each
    /// site of `window_sites`, the identity on the sites between, carrying
    /// the operator bond across.
    pub(super) fn across(
        self,
        span: RangeInclusive<usize>,
        window_sites: &[usize],
    ) -> impl Iterator<Item = MpoSite> + '_ {
        let mut operator_sites = self.sites.into_iter();
        let mut bond = 1;
        span.map(move |at| {
            let site = if window_sites.contains(&at) {
                operator_sites
                    .next()
                    .expect("one MPO tensor per site of the window")
            } else {
                MpoSite::identity(bond)
            };
            bond = site.right;
            site
        })
    }
}

/// The operator on `site_count` sites as one tensor with one index per
/// site, `p_j = 2·out_j + in_j`, at `Σ_j p_j·4^j`, site 0 the fastest. Read
/// as a column-major matrix of 4 rows, the first site's index is the rows
/// and the other sites' the columns. That is what [`factor_off_next_site`]
/// factors.
fn one_index_per_site(operator: &[Complex64], site_count: usize) -> Vec<Complex64> {
    let dim = 1usize << site_count;
    debug_assert_eq!(
        operator.len(),
        dim * dim,
        "an operator on `site_count` sites"
    );
    let mut tensor = vec![Complex64::ZERO; dim * dim];
    for row in 0..dim {
        for col in 0..dim {
            let position = position_with_one_index_per_site(row, col, site_count);
            tensor[position] = operator[row * dim + col];
        }
    }
    tensor
}

/// Where the operator's entry at `row` and `col` goes in
/// [`one_index_per_site`]'s tensor: `Σ_j p_j·4^j`, with `p_j` the site
/// `j`'s bits of `row` (out) and `col` (in), site 0 the most significant
/// bit of each.
#[inline]
fn position_with_one_index_per_site(row: usize, col: usize, site_count: usize) -> usize {
    let (mut position, mut scale) = (0, 1);
    for j in 0..site_count {
        let out = (row >> (site_count - 1 - j)) & 1;
        let input = (col >> (site_count - 1 - j)) & 1;
        position += (2 * out + input) * scale;
        scale *= 4;
    }
    position
}

/// Factors the next site's MPO tensor off `rest`, the part of the
/// operator not yet factored. `left` is its operator bond on the left.
/// Read `rest` as a column-major matrix of `4·left` rows (that bond and the
/// site's index). Its thin SVD `U·S·V†`, truncated to the operator bond
/// kept ([`kept_operator_bond`]), gives the site's tensor `U` and the new
/// rest `S·V†`, column-major `kept × cols`.
fn factor_off_next_site(
    rest: &[Complex64],
    left: usize,
) -> Result<(MpoSite, Vec<Complex64>), SvdError> {
    let rows = 4 * left;
    let cols = rest.len() / rows;
    let svd = Svd::of(MatrixRef::new(rows, cols, rest))?;
    let kept = kept_operator_bond(&svd.singular_values);
    let Svd {
        u,
        singular_values,
        mut v_adjoint,
    } = svd.truncated(kept);
    v_adjoint.multiply_rows_by(&singular_values);
    let site = MpoSite {
        left,
        right: kept,
        elements: u.into_elements(),
    };
    Ok((site, v_adjoint.into_elements()))
}

/// The operator bond on a site's right: the number of singular `values`
/// above [`OPERATOR_RANK_CUTOFF`] times the largest, at least one.
fn kept_operator_bond(values: &[f64]) -> usize {
    values
        .iter()
        .take_while(|&&s| s > OPERATOR_RANK_CUTOFF * values[0])
        .count()
        .max(1)
}

#[cfg(test)]
mod tests;
