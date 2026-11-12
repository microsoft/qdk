// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Moving the orthogonality centre: how the engine brings the one site that
//! holds the state's norm to where an operation needs it.
//!
//! In the chain's canonical form ([`chain`](crate::chain)), every site but one
//! is orthonormal and drops out of any read, and the one left, the
//! orthogonality centre, holds the state's norm. A read of a qubit, or a gate
//! on two neighbours, needs only its own sites once the centre is among them;
//! operations land anywhere on the chain, so before each read of a site and
//! each two-site gate the engine brings the centre there.
//!
//! Moving the centre changes the tensors, not the state (to rounding), and
//! keeps the form; it discards nothing: no cutoff, no capacity. It costs one
//! thin QR decomposition and one product per site crossed, and touches no other
//! site. A bond it crosses never grows: its new size is the smaller of its old
//! size and twice the bond on the crossed site's other side.
//!
//! [`Engine::move_centre`] moves the centre one site at a time. Rightward, it
//! factors the centre site, read as a matrix whose rows are (left bond, value),
//! as `Q·R` with `Q`'s columns orthonormal: `Q` stays, now left-orthonormal,
//! and `R` is multiplied into the next site, the new centre. Leftward, it does
//! the same with the adjoint of the site read as a matrix whose columns are
//! (value, right bond). [`Engine::centre_on`] first splits a pair a gate left
//! contracted ([`split`](super::split)), since the pair's two sites are stale
//! until then.

use super::Engine;
use crate::{
    CpuMpsError,
    chain::{Chain, Site},
    linear_algebra::{LinearAlgebra, adjoint},
};

impl<A: LinearAlgebra> Engine<A> {
    pub(super) fn move_centre(&self, chain: &mut Chain, to: usize) -> Result<(), CpuMpsError> {
        while chain.centre < to {
            self.shift_centre_right(chain)?;
        }
        while chain.centre > to {
            self.shift_centre_left(chain)?;
        }
        Ok(())
    }

    /// Splits the contracted pair, if any, and moves the centre to `site`,
    /// for a read of that site alone.
    pub(super) fn centre_on(&self, chain: &mut Chain, site: usize) -> Result<(), CpuMpsError> {
        self.split_pair_toward(chain, site)?;
        self.move_centre(chain, site)
    }

    /// Moves the centre one site right: `T_c = Q·R`, keep `Q` on `c`, multiply
    /// `R` into `c + 1`.
    pub(super) fn shift_centre_right(&self, chain: &mut Chain) -> Result<(), CpuMpsError> {
        let c = chain.centre;
        let qr = self.algebra.qr(chain.sites[c].rows_left())?;
        let bond = qr.q.cols();
        let next = &chain.sites[c + 1];
        let merged = self.algebra.multiply(qr.r.view(), next.cols_right());
        let right = next.right;
        chain.sites[c + 1] = Site {
            left: bond,
            right,
            elements: merged.into_elements(),
        };
        let site = &mut chain.sites[c];
        site.right = bond;
        site.elements = qr.q.into_elements();
        chain.centre = c + 1;
        Ok(())
    }

    /// Moves the centre one site left: `T_c = L·Q` from the QR of `T_c†`,
    /// keep `Q` on `c`, multiply `L` into `c − 1`.
    pub(super) fn shift_centre_left(&self, chain: &mut Chain) -> Result<(), CpuMpsError> {
        let c = chain.centre;
        let qr = self
            .algebra
            .qr(adjoint(chain.sites[c].cols_right()).view())?;
        let bond = qr.q.cols();
        let previous = &chain.sites[c - 1];
        let merged = self
            .algebra
            .multiply(previous.rows_left(), adjoint(qr.r.view()).view());
        let left = previous.left;
        chain.sites[c - 1] = Site {
            left,
            right: bond,
            elements: merged.into_elements(),
        };
        let site = &mut chain.sites[c];
        site.left = bond;
        site.elements = adjoint(qr.q.view()).into_elements();
        chain.centre = c - 1;
        Ok(())
    }
}
