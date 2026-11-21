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
//! Each join checks that what the part above requires fits what the part
//! below provides ([`BoundaryCompatible::fits`]), and keeps the
//! [`Boundary`](crate::Boundary) where they meet, or the
//! mismatch. The join that makes a runner then checks the whole chain, as
//! its parts declare themselves (`declared_parts`): each requirement
//! against the part below, and against each part further below it reaches,
//! down to the first that serves it, and reports every fault at once
//! ([`CompositionFaults`]). A joined part that check cannot reach, inside
//! a part of the user's that forwards only its role's public methods,
//! refuses every run at `ready` with its mismatch instead. A chain that
//! joins is then a runner, which gets every part ready on each run, and
//! the protocol shapes the readouts crossing its boundaries, checks every
//! batch's count of instructions, and each batch its sender marked
//! [`checked`](crate::Batch::checked), so a part writes only its own
//! logic. The call across each boundary (`boundary_crossing`) and the
//! catching of a part's panics (`part_panics`) belong to composition too.
//! What names a part ([`PartId`](crate::PartId)) is at the crate's root:
//! the roles' failures and the capabilities' boundaries name parts too.
//!
//! The joins, and the role each gives, are in the crate's
//! `docs/composing-and-running.md`.
//!
//! [`BoundaryCompatible::fits`]: crate::BoundaryCompatible::fits

mod adapter_joins;
mod boundary_crossing;
mod composed;
mod composition_faults;
mod declared_parts;
mod driver_joins;
mod part_panics;
pub(crate) mod runner;
mod unended_shots;

pub use adapter_joins::AdapterJoins;
pub use composition_faults::{CompositionFault, CompositionFaults};
pub use declared_parts::DeclaredParts;
pub use driver_joins::DriverJoins;
pub use runner::{RunError, RunRefusal, Runner};
