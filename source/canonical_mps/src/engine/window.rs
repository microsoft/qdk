// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fusion: the pending window, the gates on up to three sites gathered
//! into one operator that the backend applies at once.
//!
//! A program's two-site gates come in runs on few qubits: a controlled
//! rotation's CNOTs, a Trotter step's ladder. Applying each gate by its
//! own split costs a factorisation per gate; gathering the gates of a run
//! on up to [`WINDOW_SITES`] sites into one operator costs one
//! application for the run. The rules, stated for callers on
//! [`CanonicalMpsEngine`] (Fusion):
//!
//! - **What joins.** A two-site gate joins the window when the window's
//!   sites and its own number at most [`WINDOW_SITES`]; otherwise the
//!   window is applied and the gate opens a new one. A one-site gate on a
//!   site of the window multiplies into it; on another site it waits on
//!   its site, as before any window (`chain`), and a gate that brings the
//!   site into the window applies after it.
//! - **How it is applied.** A window on two neighbours is one
//!   [`gate_split`](SiteAlgebra::gate_split) ([`split`](super::split)),
//!   with the centre moved onto the pair first; any other window, on
//!   sites further apart or on three sites, is applied as a matrix product
//!   operator across its span ([`mpo`](super::mpo)).
//! - **When.** The first operation the window does not serve applies it: a
//!   two-site gate that does not fit; a read, a sample only if it draws
//!   values; a projection, fork, shape or retained weight. It leaves the
//!   centre on the end of the window's span nearer the site that operation
//!   names (a gate's lower site; a read's site, but for a conditional read
//!   with conditions, which forks; or the centre, for an expectation, a
//!   fork, a shape or the retained weight), the lower end on a tie, so the
//!   operation finds it there or close.
//!
//! Without fusion (the tests' comparison), each two-site gate is its own
//! window, applied at once, and leaves the centre on the side of its sites
//! nearer where the centre was.

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain, gates};
use crate::algebra::{CentreSide, SiteAlgebra};
use num_complex::Complex64;

/// The most sites a window gathers. Three serves the runs of the H₂ and
/// polyene phase estimations (a controlled rotation's ancilla and two
/// sites); a fourth site would make an operator of 256 elements and an
/// operator bond of up to 16.
pub(super) const WINDOW_SITES: usize = 3;

/// The gates applied to a few sites since the window opened, as one
/// operator: the pending window.
#[derive(Clone, Debug)]
pub(super) struct Window {
    /// The window's sites, ascending.
    pub(super) sites: Vec<usize>,
    /// The product of the window's gates, row-major, `sites[0]` the most
    /// significant bit.
    pub(super) operator: Vec<Complex64>,
}

impl Window {
    /// A window of `gate` on `on` alone.
    fn of(gate: &[Complex64], on: &[usize]) -> Self {
        let mut sites = on.to_vec();
        sites.sort_unstable();
        Self {
            operator: gates::widen(gate, on, &sites),
            sites,
        }
    }

    /// Whether the window holds `site`.
    pub(super) fn holds(&self, site: usize) -> bool {
        self.sites.contains(&site)
    }

    /// Absorbs `gate`, on the sites `on` of the window, into the operator.
    pub(super) fn absorb(&mut self, gate: &[Complex64], on: &[usize]) {
        self.operator = gates::multiply(&gates::widen(gate, on, &self.sites), &self.operator);
    }

    /// The window's sites and those of `on`, ascending, when they number at
    /// most [`WINDOW_SITES`].
    fn joined(&self, on: &[usize]) -> Option<Vec<usize>> {
        let mut sites = self.sites.clone();
        sites.extend(on.iter().filter(|site| !self.holds(**site)));
        sites.sort_unstable();
        (sites.len() <= WINDOW_SITES).then_some(sites)
    }
}

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `gate` (row-major, checked, `on[0]` the most significant
    /// bit) to the two sites `on`, in the pending window.
    pub(super) fn apply_two(
        &self,
        chain: &mut Chain<A::Tensor>,
        on: &[usize],
        gate: &[Complex64],
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let joined = chain.window.as_ref().and_then(|window| window.joined(on));
        if let Some(sites) = joined {
            let window = chain.window.as_mut().expect("the window the gate joins");
            window.operator = gates::widen(&window.operator, &window.sites, &sites);
            window.sites = sites;
            window.absorb(gate, on);
        } else {
            // The lower site took the fewest moves of the centre on H₂, or
            // the least time on the polyene, against the first operand
            // and the gate's site nearer the window.
            let toward = on[0].min(on[1]);
            self.flush_window(chain, toward)?;
            chain.window = Some(Window::of(gate, on));
        }
        if self.fuses {
            Ok(())
        } else {
            self.flush_window_toward_centre(chain)
        }
    }

    /// Applies the pending window, if any, toward the centre: the rule of
    /// the operations that name no site (an expectation, a fork, a shape,
    /// the retained weight, and an unfused gate), so the centre moves no
    /// further than the window's span.
    pub(super) fn flush_window_toward_centre(
        &self,
        chain: &mut Chain<A::Tensor>,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let centre = chain.centre;
        self.flush_window(chain, centre)
    }

    /// Applies the pending window, if any, and leaves the centre on the end
    /// of its span nearer `toward`, the lower end on a tie.
    pub(super) fn flush_window(
        &self,
        chain: &mut Chain<A::Tensor>,
        toward: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let (low, high) = match chain.window.as_ref().map(|window| &*window.sites) {
            None => return Ok(()),
            Some(&[low, high]) if high == low + 1 => (low, high),
            Some(_) => return self.apply_mpo(chain, toward),
        };
        let window = chain.window.take().expect("the window on two neighbours");
        let nearest = if chain.centre <= low { low } else { high };
        self.move_centre(chain, nearest)?;
        let first = chain.pending[low].take().unwrap_or(gates::IDENTITY);
        let second = chain.pending[high].take().unwrap_or(gates::IDENTITY);
        let pending = gates::multiply(
            &gates::widen(&first, &[low], &window.sites),
            &gates::widen(&second, &[high], &window.sites),
        );
        let operator = gates::multiply(&window.operator, &pending);
        let side = if toward <= low {
            CentreSide::Left
        } else {
            CentreSide::Right
        };
        let gate = std::array::from_fn(|at| operator[at]);
        self.split(chain, low, gate, side)
    }
}
