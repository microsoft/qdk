// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Joining parts into a chain: the joins each part heads
//! ([`DriverJoins`] and [`AdapterJoins`], whose methods are `through` and
//! `on`), and what a join gives: a joined part, in the role its position
//! gives it, or, for a driver on an executor, a chain, the [`Runner`] that
//! runs its shots.
//!
//! Implementers write one part, as one role's trait; users join parts by
//! calling the joins on them: `driver.through(adapter).on(executor)`. A
//! joined part's type cannot be named: it is used by its role alone.
//!
//! Each join checks that the instructions sent fit those accepted, and
//! keeps the faults of each boundary that does not, so that the join that
//! makes a runner, checking the whole chain (`chain_check`), reports every
//! fault of the chain at once: each boundary's, and the admission's, at
//! each boundary the needs of the part above against the offer of the part
//! below ([`CompositionFaults`]). A joined part that check cannot reach,
//! inside a part of the user's that forwards only its role's public
//! methods, refuses every run at `ready` with its faults instead. A
//! chain that joins is then a runner, which gets every part ready for
//! its needs on each run, and the protocol shapes the readouts crossing
//! its boundaries, checks every batch's count of instructions, and each
//! batch its sender marked [`checked`](crate::Batch::checked), so a part
//! writes only its own logic. A part has a place only in a chain, so what
//! names it there ([`PartId`]), the call across each boundary
//! (`boundary_crossing`) and the catching of a part's panics
//! (`part_panics`) belong to composition too.
//!
//! The joins, and the role each gives, are in the crate's
//! `docs/composing-and-running.md`.

mod adapter_joins;
mod boundary_crossing;
pub(crate) mod chain_check;
mod composed;
mod driver_joins;
mod part_id;
mod part_panics;
pub(crate) mod runner;
mod unended_shots;

use crate::{FitFaults, InstructionSet, OpcodeMap};

pub use adapter_joins::AdapterJoins;
pub use chain_check::{AdmissionFaults, CompositionFaults, ShortOffer};
pub use driver_joins::DriverJoins;
pub use part_id::PartId;
pub use runner::{RunError, RunRefusal, Runner};

/// The receiver's opcode for each instruction `sent`, or the boundary's
/// fault, naming both sets.
fn fit(sent: &InstructionSet, accepted: &InstructionSet) -> Result<OpcodeMap, FitFaults> {
    sent.fits(accepted).map_err(|mismatches| FitFaults {
        sent: sent.name().to_owned(),
        accepted: accepted.name().to_owned(),
        mismatches,
    })
}
