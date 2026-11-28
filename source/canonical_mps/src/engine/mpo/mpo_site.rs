// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One site's MPO tensor, and its product with a site of the chain.

use crate::engine::host_site::HostSite;
use num_complex::Complex64;
use std::borrow::Cow;

/// One site's MPO tensor `W[kl, p, kr]`, `p = 2·out + in`, at
/// `kl + left·(p + 4·kr)`. `kl` and `kr` index the operator bonds on the
/// site's left and right. `out` and `in` are the qubit's value after and
/// before the operator.
pub(super) struct MpoSite {
    /// The operator bond on the site's left.
    pub(super) left: usize,
    /// The operator bond on the site's right.
    pub(super) right: usize,
    /// The entries of `W`, in the order above.
    pub(super) elements: Vec<Complex64>,
}

impl MpoSite {
    /// The identity on a site between the window's, carrying a bond of
    /// `bond`.
    pub(super) fn identity(bond: usize) -> Self {
        let mut elements = vec![Complex64::ZERO; bond * 4 * bond];
        for k in 0..bond {
            for p in [0, 3] {
                elements[k + bond * (p + 4 * k)] = Complex64::ONE;
            }
        }
        Self {
            left: bond,
            right: bond,
            elements,
        }
    }

    /// The entry `W[kl, p, kr]`, or `None` where it is zero.
    #[inline]
    fn nonzero_entry(&self, kl: usize, p: usize, kr: usize) -> Option<MpoEntry> {
        let value = self.elements[kl + self.left * (p + 4 * kr)];
        (value != Complex64::ZERO).then_some(MpoEntry {
            left_bond: kl,
            out: p / 2,
            input: p % 2,
            right_bond: kr,
            value,
        })
    }
}

/// One entry `W[kl, (out, in), kr] = value` of an MPO tensor.
struct MpoEntry {
    /// `kl`, the index on the operator bond on the site's left.
    left_bond: usize,
    /// The qubit's value after the operator.
    out: usize,
    /// The qubit's value before the operator.
    input: usize,
    /// `kr`, the index on the operator bond on the site's right.
    right_bond: usize,
    /// The entry's value, nonzero.
    value: Complex64,
}

impl HostSite<'_> {
    /// The site multiplied by its MPO tensor `mpo_site` (`W` below). The
    /// operator acts on the site's qubit. Each of the site's bonds is
    /// paired with the operator bond on the same side, so the new bonds are
    /// their products:
    ///
    /// ```text
    ///         a ─ T ─ b
    ///             │ in              T'[(a, kl), out, (b, kr)]
    ///        kl ─ W ─ kr              = Σ_in W[kl, (out, in), kr]·T[a, in, b]
    ///             │ out
    ///  new bonds: (a, kl) and (b, kr), `a` faster than `kl`, `b` than `kr`
    /// ```
    ///
    /// The entries of `W` are visited `kr` the slowest, then `kl` and `p`,
    /// skipping the zeros.
    pub(super) fn multiplied_by(&self, mpo_site: &MpoSite) -> HostSite<'static> {
        let new_left = self.left * mpo_site.left;
        let new_right = self.right * mpo_site.right;
        let mut product = vec![Complex64::ZERO; new_left * 2 * new_right];
        for kr in 0..mpo_site.right {
            for kl in 0..mpo_site.left {
                for p in 0..4 {
                    if let Some(entry) = mpo_site.nonzero_entry(kl, p, kr) {
                        self.add_entry_times_site(&mut product, &entry, new_left);
                    }
                }
            }
        }
        HostSite {
            left: new_left,
            right: new_right,
            elements: Cow::Owned(product),
        }
    }

    /// Adds `entry`'s value times the site's slice of qubit value `in` to
    /// `product`'s slice of bond indices `kl`, `kr` and qubit value `out`.
    /// `new_left` is the product's left bond.
    #[inline]
    fn add_entry_times_site(&self, product: &mut [Complex64], entry: &MpoEntry, new_left: usize) {
        let (l, r) = (self.left, self.right);
        for b in 0..r {
            let from = l * (entry.input + 2 * b);
            let to = l * entry.left_bond + new_left * (entry.out + 2 * (b + r * entry.right_bond));
            for a in 0..l {
                product[to + a] += entry.value * self.elements[from + a];
            }
        }
    }
}
