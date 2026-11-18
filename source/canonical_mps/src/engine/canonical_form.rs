// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Moving the orthogonality centre, by one exact
//! [`qr_move`](SiteAlgebra::qr_move) per site crossed.
//!
//! A read of a site and a split of two neighbours need the centre on their
//! sites: there, the rest of the chain drops out of the read, and the
//! split is the state's Schmidt decomposition. A move truncates nothing,
//! so it changes the tensors but not the state. A window applied as a
//! matrix product operator ([`mpo`](super::mpo)) uses the same moves to
//! make the sites of its span isometries again.
//!
//! The site the centre leaves must become an isometry with its pending
//! one-site gate applied, so the move carries that gate into its QR; the
//! pending gate of the site the centre reaches stays pending.

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain, chain::Site};
use crate::algebra::{CentreSide, QrMoveCall, SiteAlgebra};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies the pending window, if any, toward `site`, and moves the
    /// centre to `site`: what a read of that site alone needs.
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
            let (cut, leaving, centre) = if chain.centre < to {
                (chain.centre, chain.centre, CentreSide::Right)
            } else {
                (chain.centre - 1, chain.centre, CentreSide::Left)
            };
            let gate = chain.pending[leaving].take();
            let (left, right) = (&chain.sites[cut], &chain.sites[cut + 1]);
            let (left_extent, right_extent) = (left.left, right.right);
            let outcome = self
                .algebra
                .qr_move(&QrMoveCall {
                    left: &left.tensor,
                    right: &right.tensor,
                    left_extent,
                    bond: left.right,
                    right_extent,
                    gate,
                    centre,
                })
                .map_err(CanonicalMpsError::Algebra)?;
            chain.sites[cut] = Site {
                left: left_extent,
                right: outcome.bond,
                tensor: outcome.left,
            };
            chain.sites[cut + 1] = Site {
                left: outcome.bond,
                right: right_extent,
                tensor: outcome.right,
            };
            chain.centre = match centre {
                CentreSide::Left => cut,
                CentreSide::Right => cut + 1,
            };
            chain.statistics.centre_moves += 1;
        }
        Ok(())
    }
}
