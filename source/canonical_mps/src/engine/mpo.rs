// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A window applied as a matrix product operator (MPO) across its span.
//!
//! An MPO is to an operator what the chain is to a state. It holds the
//! operator as one small tensor per site, neighbours sharing an *operator
//! bond*. So it can be multiplied into the chain one site at a time. The
//! *span* is the sites from the window's first to its last. The crate
//! README's Concepts define the others (site, bond, cut, capacity,
//! isometry, centre, SVD, truncation, split, move, host).
//!
//! A window on sites that are not neighbours, or on three sites, cannot be
//! one split: its operator acts on more than a pair. The MPO costs one
//! split per cut of its span, and truncates only at the caller's cuts. The
//! crate's `docs/engine.md` compares it with routing by SWAPs.
//!
//! [`apply_mpo`](CanonicalMpsEngine::apply_mpo) runs three steps:
//!
//! 1. **Factor** the window's operator into one MPO tensor per window site
//!    ([`OperatorMpo::of`]).
//! 2. **Multiply** each site of the span by its MPO tensor, the sites
//!    between the window's by the identity
//!    ([`multiplied_span`](CanonicalMpsEngine::multiplied_span)). Each bond
//!    of the span grows to the chain's bond times the operator's.
//! 3. **Compress** the span back to the capacity
//!    ([`compress_span`](CanonicalMpsEngine::compress_span)). The centre
//!    ends on the span's end the caller names.
//!
//! On a CNOT on sites 1 and 4 of six, the chain's bonds `χ`, at the
//! capacity, the next read on site 4:
//!
//! ```text
//!  site:           0       1       2       3       4       5
//!  chain:          A₀ ─χ── A₁ ─χ── A₂ ─χ── A₃ ─χ── A₄ ─χ── A₅
//!                          │       │       │       │
//!  1. MPO:                 W₁ ─2── I  ─2── I  ─2── W₄
//!
//!  2. multiplied:  A₀ ─χ── B₁ ─2χ─ B₂ ─2χ─ B₃ ─2χ─ B₄ ─χ── A₅
//!
//!  3. compressed:  A₀ ─χ── Q₁ ─χ── Q₂ ─χ── Q₃ ─χ── C₄ ─χ── A₅
//!
//!  W: an MPO tensor, I: the identity, A: a site of the chain,
//!  B: a multiplied site, Q: an isometry, C: the centre
//! ```
//!
//! Every backend call is one the engine already makes elsewhere: moves and
//! splits as for a gate, reads and uploads as for a projection.
//!
//! # Failures
//!
//! After a failure the state is unspecified, and the caller drops it
//! ([`CanonicalMpsEngine`]'s Failures). Still, the span's sites are
//! replaced only once every multiplied site is uploaded. So a failed read
//! or upload leaves the span's sites unmultiplied, and neighbours still
//! share each bond. A failure in the compression may leave the span half
//! compressed.
//!
//! # Further reading
//!
//! - U. Schollwöck, "The density-matrix renormalization group in the age
//!   of matrix product states", arXiv:1008.3477
//!   (<https://arxiv.org/abs/1008.3477>), 2011, published in Ann. Phys.
//!   326(1):96–192.
//!   - §4.1.3, "Decomposition of arbitrary quantum states into MPS",
//!     covers the successive SVDs that split a tensor into one tensor per
//!     site, as step 1 factors the operator.
//!   - §5.1, "Applying an MPO to an MPS", and §5.3, "Compressing MPOs and
//!     MPO-MPS products", cover steps 2 and 3.

mod mpo_site;
mod operator_mpo;

use super::{
    CanonicalMpsEngine, CanonicalMpsError,
    chain::{Chain, Site},
    host_site::HostSite,
};
use crate::site_algebra::{CentreSide, SiteAlgebra};
use mpo_site::MpoSite;
use num_complex::Complex64;
use operator_mpo::OperatorMpo;
use std::ops::{Range, RangeInclusive};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies the pending window (its operator row-major, its first site
    /// the most significant bit) as an MPO, and compresses the span. Leaves
    /// the centre on `end`, the span's first or last site.
    pub(super) fn apply_mpo(
        &self,
        chain: &mut Chain<A::Tensor>,
        end: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let window = chain.window.as_ref().expect("a pending window");
        // `Window` keeps its sites ascending, and `apply_window` sends a
        // window on one pair of neighbours to one split instead.
        debug_assert!(
            window.sites.len() >= 2 && window.sites.is_sorted(),
            "an MPO's sites are at least two, ascending"
        );
        // Factored before the window leaves the chain, for robustness only.
        // The state after a failure is unspecified all the same.
        let mpo = OperatorMpo::of(&window.operator, window.sites.len())
            .map_err(CanonicalMpsError::OperatorDecomposition)?;
        let window_sites = chain.window.take().expect("the window factored").sites;
        let span = Span::of(&window_sites);
        self.bring_centre_into(chain, span)?;
        let multiplied =
            self.multiplied_span(chain, span, mpo.across(span.sites(), &window_sites))?;
        chain.commit_span(span, multiplied);
        self.compress_span(chain, span, end)?;
        chain.statistics.mpo_applications += 1;
        Ok(())
    }

    /// Moves the centre to the site of `span` nearest it. The sites outside
    /// the span are then isometries towards it, as compressing the span
    /// needs.
    ///
    /// ```text
    ///  site        0   1   2   3   4   5
    ///                      ├───────┤          the span
    ///  before      C   Q   Q   Q   Q   Q
    ///  after       Q   Q   C   Q   Q   Q      a centre inside it stays
    /// ```
    fn bring_centre_into(
        &self,
        chain: &mut Chain<A::Tensor>,
        span: Span,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let nearest = chain.centre.clamp(span.low, span.high);
        self.move_centre(chain, nearest)
    }

    /// The sites of `span`, each read, its pending gate applied, multiplied
    /// on the host by its tensor of `mpo_sites`, and uploaded as a new
    /// tensor. The chain is left as it was, so a failed read or upload
    /// leaves its neighbours sharing each bond (the module's Failures).
    /// [`Chain::commit_span`] stores the span once every upload succeeded.
    fn multiplied_span(
        &self,
        chain: &Chain<A::Tensor>,
        span: Span,
        mpo_sites: impl Iterator<Item = MpoSite>,
    ) -> Result<Vec<Site<A::Tensor>>, CanonicalMpsError<A::Error>> {
        let mut multiplied = Vec::with_capacity(span.len());
        for (at, mpo_site) in span.sites().zip(mpo_sites) {
            let site = self.host_site(chain, at)?.multiplied_by(&mpo_site);
            multiplied.push(self.uploaded(&site)?);
        }
        Ok(multiplied)
    }

    /// `site` uploaded as a new tensor.
    fn uploaded(
        &self,
        site: &HostSite<'_>,
    ) -> Result<Site<A::Tensor>, CanonicalMpsError<A::Error>> {
        let tensor = self
            .algebra
            .upload(&site.elements)
            .map_err(CanonicalMpsError::Algebra)?;
        Ok(Site {
            left: site.left,
            right: site.right,
            tensor,
        })
    }

    /// Compresses the multiplied `span` back to the capacity. Leaves the
    /// centre on `end`, its first or last site: the near end.
    ///
    /// The multiplied sites are no longer isometries, while the sites
    /// outside the span still are, towards it. Exact moves of the centre
    /// ([`qr_move`](SiteAlgebra::qr_move)) run from the near end to the site
    /// next to the far end, making every site they leave an isometry. Then
    /// one split per cut, the identity as gate, runs from the far end back
    /// to the near end, each truncating its cut as a gate's split does. A
    /// span of `d` cuts thus takes `d − 1` moves and `d` splits.
    fn compress_span(
        &self,
        chain: &mut Chain<A::Tensor>,
        span: Span,
        end: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        debug_assert!(
            end == span.low || end == span.high,
            "the centre ends on the span's first or last site"
        );
        if end == span.high {
            self.compress_toward_high_end(chain, span)
        } else {
            self.compress_toward_low_end(chain, span)
        }
    }

    /// Compresses `span` with the centre ending on its high end. Moves run
    /// from the high end down to the site after the low end, then splits
    /// run back up.
    ///
    /// ```text
    ///  site              1    2    3    4    B: multiplied, Q: an isometry,
    ///  multiplied        B    B    B    B    C: the centre
    ///  centre named      B    B    B    C
    ///  moves 4 → 2       B    C    Q    Q    exact, d − 1 = 2 moves
    ///  split at cut 1    Q    C    Q    Q    d = 3 splits, each truncating
    ///  split at cut 2    Q    Q    C    Q    its cut to the capacity
    ///  split at cut 3    Q    Q    Q    C    the centre on the high end
    /// ```
    fn compress_toward_high_end(
        &self,
        chain: &mut Chain<A::Tensor>,
        span: Span,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        // The multiplied span holds no isometry, so any of its sites may be
        // named the centre.
        chain.centre = span.high;
        self.move_centre(chain, span.low + 1)?;
        for cut in span.cuts() {
            self.truncate_cut(chain, cut, CentreSide::Right)?;
        }
        Ok(())
    }

    /// Compresses `span` with the centre ending on its low end: the mirror
    /// image of [`compress_toward_high_end`](Self::compress_toward_high_end).
    ///
    /// ```text
    ///  site              1    2    3    4
    ///  multiplied        B    B    B    B
    ///  centre named      C    B    B    B
    ///  moves 1 → 3       Q    Q    C    B
    ///  split at cut 3    Q    Q    C    Q
    ///  split at cut 2    Q    C    Q    Q
    ///  split at cut 1    C    Q    Q    Q    the centre on the low end
    /// ```
    fn compress_toward_low_end(
        &self,
        chain: &mut Chain<A::Tensor>,
        span: Span,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        chain.centre = span.low;
        self.move_centre(chain, span.high - 1)?;
        for cut in span.cuts().rev() {
            self.truncate_cut(chain, cut, CentreSide::Left)?;
        }
        Ok(())
    }

    /// Truncates the bond at `cut` as a gate's split does, by a split with
    /// the identity as gate, and leaves the centre on `side`.
    ///
    /// ```text
    ///  before:   ─ A ─b── B ─      the centre on A or B
    ///  after:    ─ Q ─b'─ C ─      side Right (Left: ─ C ─b'─ Q ─)
    ///
    ///  b' ≤ b, within the capacity: the identity changes nothing,
    ///  so the truncation alone acts
    /// ```
    fn truncate_cut(
        &self,
        chain: &mut Chain<A::Tensor>,
        cut: usize,
        side: CentreSide,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        self.split(chain, cut, TWO_SITE_IDENTITY, side)
    }
}

/// The identity on two sites: the gate of the splits that only truncate,
/// those compressing the span.
const TWO_SITE_IDENTITY: [Complex64; 16] = {
    let mut identity = [Complex64::ZERO; 16];
    let mut at = 0;
    while at < 16 {
        identity[at] = Complex64::ONE;
        at += 5;
    }
    identity
};

impl<T> Chain<T> {
    /// Stores the `multiplied` sites of `span`, whose pending gates they
    /// applied. Records the bonds at its cuts as
    /// [transient](super::CanonicalMpsStatistics::largest_transient_bond):
    /// they may exceed the capacity until compressing the span shrinks them.
    fn commit_span(&mut self, span: Span, multiplied: Vec<Site<T>>) {
        for (at, site) in span.sites().zip(multiplied) {
            if at < span.high {
                let largest = &mut self.statistics.largest_transient_bond;
                *largest = (*largest).max(site.right);
            }
            self.sites[at] = site;
            self.pending[at] = None;
        }
    }
}

/// The sites from a window's first, `low`, to its last, `high`.
#[derive(Clone, Copy, Debug)]
struct Span {
    low: usize,
    high: usize,
}

impl Span {
    /// The span of a window's `sites`, ascending.
    fn of(sites: &[usize]) -> Self {
        Self {
            low: sites[0],
            high: sites[sites.len() - 1],
        }
    }

    /// The span's sites, ascending.
    fn sites(self) -> RangeInclusive<usize> {
        self.low..=self.high
    }

    /// The number of the span's sites.
    fn len(self) -> usize {
        self.high - self.low + 1
    }

    /// The cuts between the span's sites, `cut` between `cut` and
    /// `cut + 1`.
    fn cuts(self) -> Range<usize> {
        self.low..self.high
    }
}

#[cfg(test)]
mod tests;
