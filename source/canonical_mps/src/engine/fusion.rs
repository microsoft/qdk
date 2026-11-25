// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fusion: the pending window, the gates on up to three sites gathered
//! into one operator that the backend applies at once.
//!
//! A program's two-site gates come in runs on few qubits: a controlled
//! rotation's CNOTs, a Trotter step's ladder. Applying each gate by its own
//! split costs a factorisation per gate. Gathering the gates of a run on up
//! to [`WINDOW_SITES`] sites into one operator costs one application for
//! the run. [`CanonicalMpsEngine`]'s Fusion says what callers rely on. The
//! rules:
//!
//! - **Which gates join.** A two-site gate joins the window while the
//!   window's sites and its own number at most [`WINDOW_SITES`], neighbours
//!   or not. A one-site gate on a site of the window multiplies into its
//!   operator. A one-site gate on another site waits on its site, as its
//!   pending gate.
//! - **Which operations apply the window.** The first one the window does
//!   not serve: a two-site gate that does not fit; a read (a probability or
//!   an expectation); a sample, only if it draws values; a projection, a
//!   fork, a shape or the retained weight.
//! - **Where it leaves the centre.** On the end of the window's span nearer
//!   the site the operation names, the lower end on a tie: a two-site
//!   gate's lower site, or a read's site. The operations that name no site
//!   name the centre: an expectation, a fork (a conditional read with
//!   conditions forks), a shape and the retained weight. So the centre
//!   moves no further than the span. The crate's `docs/engine.md` gives
//!   where each method leaves it.
//! - **When a waiting one-site gate is applied.** The next split or move of
//!   the centre across its site applies it, as does a projection of the
//!   site, or a window whose span holds it. A read sees it applied.
//!
//! A run of gates, and the windows it makes:
//!
//! ```text
//!  call               window after it   what happens
//!  CNOT on 0, 1       {0, 1}            opens the window
//!  Rz on 1            {0, 1}            joins: site 1 is the window's
//!  H on 3             {0, 1}            waits on site 3, outside it
//!  CNOT on 1, 2       {0, 1, 2}         joins: three sites fit
//!  CNOT on 2, 3       {2, 3}            {0, 1, 2, 3} does not fit:
//!                                       {0, 1, 2} is applied toward 2
//!                                       (an MPO), then {2, 3} opens
//!  read of site 0     none              {2, 3} is applied toward 0
//!                                       (one split)
//! ```
//!
//! A window on two neighbours is applied by one split
//! ([`splits`](super::splits)), any other as a matrix product operator
//! across its span ([`mpo`](super::mpo)); [`apply_window`] chooses.
//! Where the centre ends is decided apart, by [`Window::centre_end_toward`]
//! and [`Window::centre_end_displaced_by`], and the window's application
//! takes that end.
//!
//! [`apply_window`]: CanonicalMpsEngine::apply_window

use super::{
    CanonicalMpsEngine, CanonicalMpsError, chain::Chain, gate_products,
    gate_products::ONE_SITE_IDENTITY,
};
use crate::site_algebra::{CentreSide, SiteAlgebra};
use num_complex::Complex64;

/// The most sites a window gathers. Three serves the runs of the H₂ and
/// polyene phase estimations: a controlled rotation's ancilla and two
/// sites. A fourth site would make an operator of 256 elements and an
/// operator bond of up to 16.
pub(super) const WINDOW_SITES: usize = 3;

/// The pending window: the gates applied to a few sites since the window
/// opened, as one operator.
#[derive(Clone, Debug)]
pub(super) struct Window {
    /// The window's sites, ascending.
    pub(super) sites: Vec<usize>,
    /// The product of the window's gates, row-major, `sites[0]` the most
    /// significant bit.
    pub(super) operator: Vec<Complex64>,
}

impl Window {
    /// A window of `gate` alone, on `gate_sites`.
    fn of(gate: &[Complex64], gate_sites: &[usize]) -> Self {
        let mut sites = gate_sites.to_vec();
        sites.sort_unstable();
        Self {
            operator: gate_products::widen(gate, gate_sites, &sites),
            sites,
        }
    }

    /// Whether the window holds `site`.
    pub(super) fn holds(&self, site: usize) -> bool {
        self.sites.contains(&site)
    }

    /// Absorbs `gate`, on `gate_sites`, which the window holds, into the
    /// operator.
    pub(super) fn absorb(&mut self, gate: &[Complex64], gate_sites: &[usize]) {
        self.operator = gate_products::product_on_same_sites(
            &gate_products::widen(gate, gate_sites, &self.sites),
            &self.operator,
        );
    }

    /// Absorbs `gate`, on `gate_sites`, when the window's sites and the
    /// gate's number at most [`WINDOW_SITES`]. Returns whether it did.
    fn absorb_if_it_fits(&mut self, gate: &[Complex64], gate_sites: &[usize]) -> bool {
        let Some(sites) = self.joined(gate_sites) else {
            return false;
        };
        self.grow_to(sites);
        self.absorb(gate, gate_sites);
        true
    }

    /// The window's sites and `gate_sites`, ascending, when they number at
    /// most [`WINDOW_SITES`].
    fn joined(&self, gate_sites: &[usize]) -> Option<Vec<usize>> {
        let mut sites = self.sites.clone();
        sites.extend(gate_sites.iter().filter(|site| !self.holds(**site)));
        sites.sort_unstable();
        (sites.len() <= WINDOW_SITES).then_some(sites)
    }

    /// Extends the window to `sites`, which hold its own, ascending. The
    /// operator acts as the identity on the new sites.
    fn grow_to(&mut self, sites: Vec<usize>) {
        self.operator = gate_products::widen(&self.operator, &self.sites, &sites);
        self.sites = sites;
    }

    /// The end of the window's span that applying the window leaves the
    /// centre on, for an operation that names `site`: the end nearer
    /// `site`, the lower end on a tie. So the centre then moves no further
    /// than the span to reach `site`.
    ///
    /// ```text
    ///  window      the centre left on
    ///  {2, 3}      2 if site ≤ 2, else 3
    ///  {1, 3}      1 if site ≤ 2 (2: a tie), else 3
    ///  {0, 1, 2}   0 if site ≤ 1 (1: a tie), else 2
    /// ```
    fn centre_end_toward(&self, site: usize) -> usize {
        let (low, high) = (self.sites[0], self.sites[self.sites.len() - 1]);
        if site.abs_diff(high) < site.abs_diff(low) {
            high
        } else {
            low
        }
    }

    /// The end of the window's span that applying the window leaves the
    /// centre on, when a two-site gate on `gate_sites` does not fit in it:
    /// the end nearer the gate's lower site, whichever operand comes first
    /// and wherever the centre is.
    fn centre_end_displaced_by(&self, gate_sites: &[usize]) -> usize {
        // Against the first operand and the gate's site nearer the window,
        // the lower site took the fewest moves of the centre on H₂, and the
        // least time on the polyene.
        self.centre_end_toward(gate_sites[0].min(gate_sites[1]))
    }

    /// The window's two sites, when they are neighbours.
    fn neighbours(&self) -> Option<(usize, usize)> {
        match *self.sites {
            [low, high] if high == low + 1 => Some((low, high)),
            _ => None,
        }
    }
}

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `gate` (row-major, checked, `gate_sites[0]` the most
    /// significant bit) to its two sites `gate_sites`, in the pending window.
    pub(super) fn apply_two(
        &self,
        chain: &mut Chain<A::Tensor>,
        gate_sites: &[usize],
        gate: &[Complex64],
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let joined = chain
            .window
            .as_mut()
            .is_some_and(|window| window.absorb_if_it_fits(gate, gate_sites));
        if !joined {
            self.replace_window(chain, gate_sites, gate)?;
        }
        if self.fuses {
            Ok(())
        } else {
            self.flush_window_toward_centre(chain)
        }
    }

    /// Applies the pending window, if any, toward the lower of
    /// `gate_sites` ([`Window::centre_end_displaced_by`]), and opens a new
    /// window with `gate` alone.
    ///
    /// ```text
    ///  window {0, 1}, then a CNOT on 3, 2: {0, 1, 2, 3} does not fit
    ///  flush toward 2    {0, 1} applied by one split, the centre on 1
    ///  open              window {2, 3}, the CNOT alone
    /// ```
    fn replace_window(
        &self,
        chain: &mut Chain<A::Tensor>,
        gate_sites: &[usize],
        gate: &[Complex64],
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        if let Some(window) = chain.window.as_ref() {
            let end = window.centre_end_displaced_by(gate_sites);
            self.apply_window(chain, end)?;
        }
        chain.window = Some(Window::of(gate, gate_sites));
        Ok(())
    }

    /// Applies the pending window, if any, toward the centre. The
    /// operations that name no site follow this rule: an expectation, a
    /// fork, a shape, the retained weight, and an unfused gate. So the
    /// centre moves no further than the window's span.
    pub(super) fn flush_window_toward_centre(
        &self,
        chain: &mut Chain<A::Tensor>,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let centre = chain.centre;
        self.flush_window(chain, centre)
    }

    /// Applies the pending window, if any, and leaves the centre on the end
    /// of its span nearer `toward`, the lower end on a tie
    /// ([`Window::centre_end_toward`]).
    pub(super) fn flush_window(
        &self,
        chain: &mut Chain<A::Tensor>,
        toward: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let Some(window) = chain.window.as_ref() else {
            return Ok(());
        };
        let end = window.centre_end_toward(toward);
        self.apply_window(chain, end)
    }

    /// Applies the pending window, and leaves the centre on `end`, the end
    /// of its span a `Window::centre_end_…` decision chose.
    ///
    /// ```text
    ///  window      applied by
    ///  {2, 3}      one split
    ///  {1, 3}      an MPO on sites 1-3
    ///  {0, 1, 2}   an MPO on sites 0-2
    /// ```
    fn apply_window(
        &self,
        chain: &mut Chain<A::Tensor>,
        end: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let window = chain.window.as_ref().expect("a pending window");
        match window.neighbours() {
            Some((low, high)) => self.apply_window_by_one_split(chain, low, high, end),
            None => self.apply_mpo(chain, end),
        }
    }

    /// Applies the pending window, on the neighbours `low` and `high`, by
    /// one split. It first moves the centre onto the pair, and folds the
    /// pair's pending one-site gates into the split's gate
    /// ([`gate_including_pending_gates`]). Leaves the centre on `end`,
    /// `low` or `high`.
    fn apply_window_by_one_split(
        &self,
        chain: &mut Chain<A::Tensor>,
        low: usize,
        high: usize,
        end: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        debug_assert!(end == low || end == high, "the centre ends on the pair");
        let window = chain.window.take().expect("the window on two neighbours");
        let nearest = if chain.centre <= low { low } else { high };
        self.move_centre(chain, nearest)?;
        let gate = gate_including_pending_gates(chain, &window);
        let side = if end == low {
            CentreSide::Left
        } else {
            CentreSide::Right
        };
        self.split(chain, low, gate, side)
    }
}

/// The gate the split of a window on two neighbours applies: the window's
/// operator after the pair's pending one-site gates. It takes those gates
/// from the chain, since the split's gate now carries them.
fn gate_including_pending_gates<T>(chain: &mut Chain<T>, window: &Window) -> [Complex64; 16] {
    let (low, high) = (window.sites[0], window.sites[1]);
    let first = chain.pending[low].take().unwrap_or(ONE_SITE_IDENTITY);
    let second = chain.pending[high].take().unwrap_or(ONE_SITE_IDENTITY);
    // The two pending gates act on different sites, so they commute:
    // which one is passed as `later` does not matter.
    let pending = gate_products::product_on_same_sites(
        &gate_products::widen(&first, &[low], &window.sites),
        &gate_products::widen(&second, &[high], &window.sites),
    );
    let operator = gate_products::product_on_same_sites(&window.operator, &pending);
    std::array::from_fn(|at| operator[at])
}

#[cfg(test)]
mod tests;
