// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Moving the orthogonality centre, by one exact
//! [`qr_move`](SiteAlgebra::qr_move) per site crossed.
//!
//! A read of a site and a split of two neighbours need the centre on their
//! sites. There, the rest of the chain drops out of the read, and the split
//! is the state's Schmidt decomposition (the crate README's Concepts). A move
//! truncates nothing, so it changes the tensors but not the state. A window
//! applied as a matrix product operator ([`mpo`](super::mpo)) uses the same
//! moves to make the sites of its span isometries again.
//!
//! ```text
//!  before:   Q → C ← Q ← Q ← Q      C: the centre
//!  after:    Q → Q → Q → Q → C      Q: an isometry, its arrow toward C
//! ```
//!
//! [`move_centre`](CanonicalMpsEngine::move_centre) walks the centre one
//! cut at a time. Each site it leaves becomes an isometry.

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain};
use crate::site_algebra::{CentreSide, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use num_complex::Complex64;

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies the pending window, if any, toward `site`, and moves the
    /// centre to `site`. A read of that site alone needs this.
    pub(super) fn centre_on(
        &self,
        chain: &mut Chain<A::Tensor>,
        site: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        self.flush_window(chain, site)?;
        self.move_centre(chain, site)
    }

    /// Moves the centre to `to`, one site at a time. The chain has no
    /// pending window.
    ///
    /// ```text
    ///  site      0   1   2   3   4      C: the centre
    ///  before    Q   C   Q   Q   Q      Q: an isometry towards it
    ///  cut 1     Q   Q   C   Q   Q      to = 4: one move per cut,
    ///  cut 2     Q   Q   Q   C   Q      in order
    ///  cut 3     Q   Q   Q   Q   C
    /// ```
    pub(super) fn move_centre(
        &self,
        chain: &mut Chain<A::Tensor>,
        to: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        debug_assert!(
            chain.window.is_none(),
            "a move of the centre with a pending window"
        );
        while chain.centre != to {
            let (cut, side) = next_cut_toward(chain.centre, to);
            self.move_across(chain, cut, side)?;
        }
        Ok(())
    }

    /// Moves the centre one site, across `cut`, to the site `side` names.
    /// The site the centre leaves becomes an isometry, its pending gate
    /// applied: an isometry must hold the gates applied to it. The other
    /// takes the norm, and its pending gate stays pending.
    ///
    /// ```text
    ///  across cut 2, side Right:     across cut 2, side Left:
    ///  site     2        3           site     2        3
    ///  before   C(G) ─── Q           before   Q ─── C(G)
    ///  after    Q ──k─── C           after    C ──k─── Q
    ///
    ///  G: the gate pending on the centre, applied into the new Q
    ///  k: the new bond (`pair_with_centre_moved` says when it shrinks)
    /// ```
    fn move_across(
        &self,
        chain: &mut Chain<A::Tensor>,
        cut: usize,
        side: CentreSide,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let gate = chain.pending[chain.centre].take();
        let new_pair = self.pair_with_centre_moved(chain, cut, gate, side)?;
        chain.replace_pair(cut, new_pair.bond, new_pair.left, new_pair.right, side);
        chain.statistics.centre_moves += 1;
        Ok(())
    }

    /// The pair at `cut` with the centre moved to `side` by one
    /// [`qr_move`](SiteAlgebra::qr_move), `gate` applied to the site the
    /// centre leaves. The chain is unchanged; the caller stores the pair.
    ///
    /// ```text
    ///  side Right, A the centre:
    ///    ─ A ─── B ─        A, `gate` applied, is factored
    ///    A = Q · R          Q: an isometry; R: a k × bond matrix,
    ///                       k = min(bond, 2·A's left bond)
    ///    ─ Q ─k─ R·B ─      Q stays on A's site; R moves into B, the centre
    ///
    ///  side Left, the mirror image, B the centre:
    ///    B = L · Q          Q's rows orthonormal; L: a bond × k matrix,
    ///                       k = min(bond, 2·B's right bond)
    ///    ─ A·L ─k─ Q ─      Q stays on B's site; L moves into A, the centre
    /// ```
    ///
    /// The new bond `k` is smaller than the old only where the old exceeded
    /// what the site's other bond and its qubit can carry, as after an MPO's
    /// multiply. [`QrMoveCall`] gives the matrices' indices.
    fn pair_with_centre_moved(
        &self,
        chain: &Chain<A::Tensor>,
        cut: usize,
        gate: Option<[Complex64; 4]>,
        side: CentreSide,
    ) -> Result<QrMoveOutcome<A::Tensor>, CanonicalMpsError<A::Error>> {
        let (left, right) = (&chain.sites[cut], &chain.sites[cut + 1]);
        self.algebra
            .qr_move(&QrMoveCall {
                left: &left.tensor,
                right: &right.tensor,
                left_extent: left.left,
                bond: left.right,
                right_extent: right.right,
                gate,
                centre: side,
            })
            .map_err(CanonicalMpsError::Algebra)
    }
}

/// The cut the centre crosses next on its way from `centre` to `to`, and
/// the side of it the centre reaches.
///
/// ```text
///  centre 1, to 3:   0 ─ C ┼ 2 ─ 3     cut 1, Right
///  centre 3, to 0:   0 ─ 1 ─ 2 ┼ C     cut 2, Left
///
///  C: the centre; ┼: the cut it crosses next
/// ```
fn next_cut_toward(centre: usize, to: usize) -> (usize, CentreSide) {
    if centre < to {
        (centre, CentreSide::Right)
    } else {
        (centre - 1, CentreSide::Left)
    }
}

#[cfg(test)]
mod tests;
