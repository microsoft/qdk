// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Composing parts into a chain: the role types users hold ([`Driver`],
//! [`Adapter`], [`Executor`]), [`Compose`], whose operator is `>>`, and
//! what a join gives: a [`Composed`] part or, for a driver with an
//! executor, a chain, the [`Runner`] that runs its shots.
//!
//! Users compose parts, and implementers write one part, as a trait. Rust
//! lets the protocol write `>>` only for its own types, so each role has a
//! role type, a thin wrapper of a part, that users compose.
//!
//! Each join checks that the instructions sent fit those accepted, and
//! keeps the faults of each boundary that does not, so that the join that
//! makes a runner reports every fault of the chain at once: each
//! boundary's, and the admission's, at each boundary the needs of the part
//! above against the offer of the part below ([`CompositionFaults`]). A
//! chain that composes is then a runner, which gets every part ready for
//! its needs on each run, and the protocol shapes the readouts crossing
//! its boundaries, checks every batch's count of instructions, and each
//! batch its sender marked [`checked`](crate::Batch::checked), so a part
//! writes only its own logic. A part has a place only in a chain, so what
//! names it there ([`PartId`]), the call across each boundary
//! (`boundary_crossing`) and the catching of a part's panics
//! (`part_panics`) belong to composition too.
//!
//! The joins `>>` allows, and the role each gives, are in the crate's
//! `docs/composing-and-running.md` ("Joining parts with `>>`").

mod adapter;
pub(crate) mod admission;
mod boundary_crossing;
mod composed;
mod composition_faults;
mod driver;
mod executor;
mod part_id;
pub(crate) mod part_panics;
pub(crate) mod runner;

use crate::{FitFaults, InstructionSet, OpcodeMap};

pub use adapter::Adapter;
pub use admission::{AdmissionFaults, ShortOffer};
pub use composed::Composed;
pub use composition_faults::CompositionFaults;
pub use driver::Driver;
pub use executor::Executor;
pub use part_id::PartId;
pub use runner::{RunError, RunRefusal, Runner};

/// Joins two parts into one, in the role their positions give it: checks
/// that the instructions the upper part sends fit those the lower part
/// accepts, binds them where they fit, and keeps every fit fault of both
/// parts and of their boundary, top to bottom. The join of a driver with an
/// executor also checks the admission, and returns every fault of the
/// chain, or gives a [`Runner`](crate::Runner).
///
/// Its operator is `>>`: `a >> b` is `a.compose(b)`. Rust has no
/// overloading, so one name for every pair of roles is a trait.
pub trait Compose<Rhs> {
    /// The part, or the runner, the join gives.
    type Output;

    /// Joins `self`, above, with `rhs`, below.
    fn compose(self, rhs: Rhs) -> Self::Output;
}

/// The receiver's opcode for each instruction `sent`, or the boundary's
/// fault, naming both sets.
fn fit(sent: &InstructionSet, accepted: &InstructionSet) -> Result<OpcodeMap, FitFaults> {
    sent.fits(accepted).map_err(|mismatches| FitFaults {
        sent: sent.name().to_owned(),
        accepted: accepted.name().to_owned(),
        mismatches,
    })
}

/// The fit faults of a join, top to bottom: the upper part's, the
/// boundary's, the lower part's.
fn faults(
    mut upper: Vec<FitFaults>,
    boundary: Option<FitFaults>,
    lower: Vec<FitFaults>,
) -> Vec<FitFaults> {
    upper.extend(boundary);
    upper.extend(lower);
    upper
}

/// `Ok` when there is no fault in `fit`.
fn check(fit: &[FitFaults]) -> Result<(), &[FitFaults]> {
    if fit.is_empty() { Ok(()) } else { Err(fit) }
}
