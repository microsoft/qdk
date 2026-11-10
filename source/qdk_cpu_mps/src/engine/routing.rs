// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Routing`]: how the engine brings a gate's two sites together when they
//! are not neighbours, and the opt-in [`Routing::MeetHalfway`], which leaves
//! them where they met, with the guard that keeps it as accurate as the
//! default `swap-back-2x`.
//!
//! A simulator chooses the routing with
//! [`CpuMpsEngine::with_routing`](crate::CpuMpsEngine::with_routing).
//! `swap-back-2x` ([`gates`](super::gates)) is the default and needs nothing
//! here. `MeetHalfway` takes about half its SWAPs on a gate between distant
//! qubits, and none on a gate that follows on the same pair, at the price of
//! a chain whose order drifts from the caller's. The engine keeps the map
//! ([`Layout`](crate::chain::Layout)), so every call answers in the caller's
//! sites, and this file holds when and how the caller's order comes back:
//!
//! - **The route** ([`Engine::meet_halfway`]): the two sites at `low < high`
//!   move to the pair at `meet = low + (high − low − 1) / 2`, the one on the
//!   centre's side first, so that the centre rides along and crosses the
//!   span once; the gate applies there and the sites stay.
//! - **The deferred cut.** While the chain is out of the caller's order, a
//!   split keeps up to twice the capacity's largest bond
//!   ([`Truncation::CutoffAndTwiceCapacity`]), not the capacity: a cut of a
//!   drifted order is not a cut of the caller's, and the capacity describes
//!   the caller's. The split records its cut when it keeps more than the
//!   capacity's bond there ([`split`](super::split)), and the first
//!   two-qubit gate or read after it restores the order
//!   ([`Engine::restore_if_over_capacity`],
//!   [`Engine::restore_if_over_capacity_for_read`]; a fork restores the
//!   state it forks, once for all its forks). The restore
//!   ([`Engine::restore_order`]) moves every site home by SWAPs at twice the
//!   capacity, then compresses the cuts over the capacity to it by one sweep
//!   of SVDs, in the caller's order. So the capacity cuts the state only at
//!   the caller's cuts, as with `swap-back-2x`.
//! - **The fallback.** That restore also ends the drift for good: a bond
//!   over the capacity says the capacity binds, and each later route would
//!   pay a restore. A gate that leaves the layout drift above
//!   [`FALLBACK_DRIFT`] restores the order and falls back too: the order is
//!   then nearer reversed than the caller's, where routes grow long. A state
//!   that fell back routes by `swap-back-2x` for the rest of its life, and
//!   its forks inherit it.
//!
//! Why twice the capacity: the first route from the caller's order moves
//! each site across cuts whose sides then differ from the caller's by that
//! one site, so each of its bonds is at most twice a bond the caller's order
//! holds, itself within the capacity. A later route starts from a drifted
//! order, where that bound no longer holds: a split there that needs more
//! than twice the capacity's largest bond truncates at a cut that is not the
//! caller's. The capacity fallback ends the drift at the first bond over the
//! capacity, which keeps such splits rare; how rare was not measured.
//!
//! **Fusion and the deferred cut.** With fusion, the last pair a route
//! leaves unsplit is split by the next operation. A read splits it first, so
//! a read restores the order when that pair's bond exceeds the capacity. A
//! gate on another pair splits it while it starts, after its own check, so
//! the restore waits for the operation after that gate: the capacity still
//! cuts only at the caller's cuts, one gate later.

use super::{Engine, split::CentreSide};
use crate::{
    CpuMpsError,
    chain::{Chain, Truncation, c},
    linear_algebra::LinearAlgebra,
};
use num_complex::Complex64;

/// How the engine routes a gate on two qubits whose sites are not
/// neighbours in the chain: [`SwapBack`](Routing::SwapBack), the default,
/// or [`MeetHalfway`](Routing::MeetHalfway), an option for programs with
/// many gates between distant qubits. Choose it with
/// [`CpuMpsEngine::with_routing`](crate::CpuMpsEngine::with_routing).
///
/// Both answer every call in the caller's sites, keep the contract, and
/// cut the state to the capacity only at the caller's cuts, so they agree to
/// the cutoff's truncations (to rounding at a cutoff of zero) when the
/// capacity never binds, and lose the same weight when it does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Routing {
    /// `swap-back-2x`, the default: the site at `high` comes down next to
    /// the one at `low` by SWAPs, the gate applies, and the site goes back
    /// up, so the chain is in the caller's order after every call. A gate on
    /// sites `d` apart takes `2(d − 1)` SWAPs. See Routing on
    /// [`CpuMpsEngine`](crate::CpuMpsEngine).
    #[default]
    SwapBack,
    /// Both sites of a gate move halfway toward each other by SWAPs, the
    /// gate applies where they meet, and they stay there: a gate on sites
    /// `d` apart takes `d − 1` SWAPs, and a gate that follows on the same
    /// two qubits takes none. The chain's order drifts from the caller's;
    /// the engine maps every site a call names to where it sits, and
    /// [`MpsEngine::shape`](tensornet::MpsEngine::shape) restores the
    /// caller's order first, so no answer shows the drift.
    ///
    /// It is guarded, so that it truncates where `SwapBack` does:
    ///
    /// - **The deferred cut.** While the chain is out of the caller's order,
    ///   a split keeps up to twice the capacity's largest bond. The first
    ///   two-qubit gate or read after a split that kept more than the
    ///   capacity's bond at its cut restores the caller's order and cuts the
    ///   bonds over the capacity to it there (with fusion, a gate that
    ///   splits the pair a route left may come first: the restore then
    ///   waits for the operation after it). So the capacity cuts the state
    ///   only at the caller's cuts.
    /// - **The fallback.** Once a bond has exceeded the capacity, or once a
    ///   gate leaves the order's drift above one half (the pairs of qubits
    ///   the chain holds in the other order than the caller's, over all
    ///   pairs: 0 in the caller's order, 1 reversed), the state restores the
    ///   caller's order and routes by `SwapBack` for the rest of its life;
    ///   its forks inherit it. Where the capacity binds, each route would
    ///   pay a restore; and past one half, the order is nearer reversed than
    ///   the caller's, where routes grow long.
    ///
    /// **Why an option.** It saves most where gates between distant qubits
    /// dominate and the capacity rarely binds: on a recorded H₂ shot at
    /// `χ` 128, 0.20× `SwapBack`'s time with the same readout distribution,
    /// and 0.12× on a random circuit of 20 qubits at depth 2. On the other
    /// cases measured it took 0.88× to 1.2× `SwapBack`'s time, at the same
    /// accuracy: the fallback took over (crate's `docs/routing.md`). It is
    /// a hypothesis being evaluated, not the default: the threshold of one
    /// half and the route were chosen on six cases and checked on four more.
    ///
    /// **Its price.** Between restores, a site tensor may hold bonds of up
    /// to twice the capacity's largest bond `χ`, as during a `SwapBack`
    /// route, but for longer; [`SplitStatistics`](crate::SplitStatistics)
    /// reports them as transient. And a split in a drifted order that needs
    /// more than `2χ` truncates at a cut that is not the caller's (see the
    /// crate's `docs/routing.md`).
    MeetHalfway,
}

/// The layout drift above which a gate makes `MeetHalfway` fall back to
/// `swap-back-2x`: one half, where the chain's order is as far from the
/// caller's as from its reverse. Measured on QFT of 20 qubits at `χ` 128,
/// whose drift grows to the reverse, it took `MeetHalfway` from 2.8× to
/// 0.88× `swap-back-2x`'s time; the cases where `MeetHalfway` pays (H₂, at
/// most 0.13, and random circuits) stay below. A constant, not a setting:
/// one value served every case measured.
const FALLBACK_DRIFT: f64 = 0.5;

impl<A: LinearAlgebra> Engine<A> {
    /// Applies `gate` (row-major, the site at position `low` the most
    /// significant bit) to the positions `low` and `high`, at least two
    /// apart, by `MeetHalfway`'s route, then restores the order and falls
    /// back if the drift exceeds [`FALLBACK_DRIFT`].
    pub(super) fn meet_halfway(
        &self,
        chain: &mut Chain,
        low: usize,
        high: usize,
        gate: &[Complex64],
    ) -> Result<(), CpuMpsError> {
        let meet = low + (high - low - 1) / 2;
        let deferred = Truncation::CutoffAndTwiceCapacity;
        // After the SWAP at `cut`, the site moving up sits right of it and
        // the one moving down left of it: the centre goes with each.
        if chain.centre <= low.midpoint(high) {
            for cut in low..meet {
                self.swap(chain, cut, CentreSide::Right, deferred)?;
            }
            for cut in (meet + 1..high).rev() {
                self.swap(chain, cut, CentreSide::Left, deferred)?;
            }
        } else {
            for cut in (meet + 1..high).rev() {
                self.swap(chain, cut, CentreSide::Left, deferred)?;
            }
            for cut in low..meet {
                self.swap(chain, cut, CentreSide::Right, deferred)?;
            }
        }
        let side = Self::side_nearer_centre(chain, meet);
        let truncation = Self::truncation_at_rest(chain);
        self.apply_adjacent(chain, meet, gate, side, truncation)?;
        if chain.layout.drift() > FALLBACK_DRIFT {
            chain.fell_back = true;
            self.restore_order(chain)?;
        }
        Ok(())
    }

    /// Before a gate: restores the caller's order, which falls back, when a
    /// split since the last restore kept more than the capacity's bond at
    /// its cut. A pair left unsplit is split by the gate itself (see Fusion
    /// and the deferred cut, in this module).
    pub(super) fn restore_if_over_capacity(&self, chain: &mut Chain) -> Result<(), CpuMpsError> {
        if chain.over_capacity_cuts.is_some() {
            self.restore_order(chain)?;
        }
        Ok(())
    }

    /// Before a read, a fork or `retained_weight`: splits first a pair left
    /// unsplit by a split out of the caller's order, so that its bond is
    /// checked too, then restores as [`Engine::restore_if_over_capacity`].
    pub(super) fn restore_if_over_capacity_for_read(
        &self,
        chain: &mut Chain,
    ) -> Result<(), CpuMpsError> {
        if chain
            .pair
            .as_ref()
            .is_some_and(|pair| pair.truncation == Truncation::CutoffAndTwiceCapacity)
        {
            let centre = chain.centre;
            self.split_pair_toward(chain, centre)?;
        }
        self.restore_if_over_capacity(chain)
    }

    /// Moves every site back to the caller's order, then cuts the bonds
    /// over the capacity to it. Site 0 comes down to position 0, then site
    /// 1, and so on, by SWAPs at twice the capacity's largest bond (the
    /// order is not the caller's until the last one). Their last pair is
    /// split, so that its bond is checked too; then, if any split since the
    /// last restore kept more than the capacity's bond at its cut, the
    /// state falls back and [`Engine::compress`] cuts those bonds.
    pub(super) fn restore_order(&self, chain: &mut Chain) -> Result<(), CpuMpsError> {
        for site in 0..chain.sites.len() {
            for cut in (site..chain.layout.position(site)).rev() {
                self.swap(
                    chain,
                    cut,
                    CentreSide::Left,
                    Truncation::CutoffAndTwiceCapacity,
                )?;
            }
        }
        if chain
            .pair
            .as_ref()
            .is_some_and(|pair| pair.truncation == Truncation::CutoffAndTwiceCapacity)
        {
            let centre = chain.centre;
            self.split_pair_toward(chain, centre)?;
        }
        if let Some(cuts) = chain.over_capacity_cuts.take() {
            chain.fell_back = true;
            self.compress(chain, cuts)?;
        }
        Ok(())
    }

    /// One sweep of SVDs over the cuts `low..=high`, in the caller's order,
    /// from the highest down: each contracts its pair with the centre in it
    /// and splits it at the cutoff and the capacity, which leaves every bond
    /// of the range within the capacity, each cut at its best (the centre in
    /// the pair).
    fn compress(&self, chain: &mut Chain, (low, high): (usize, usize)) -> Result<(), CpuMpsError> {
        let identity: [Complex64; 16] =
            std::array::from_fn(|at| c(if at % 5 == 0 { 1.0 } else { 0.0 }));
        self.split_pair_toward(chain, high + 1)?;
        self.move_centre(chain, high + 1)?;
        for cut in (low..=high).rev() {
            self.apply_adjacent(
                chain,
                cut,
                &identity,
                CentreSide::Left,
                Truncation::CutoffAndCapacity,
            )?;
            self.split_pair(chain, CentreSide::Left)?;
        }
        Ok(())
    }
}
