// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The three roles a part takes in a chain, each two traits an implementer
//! writes, one per time: at composition time, what `>>` reads from the part
//! and hands back to it once, before any run ([`DriverSetup`],
//! [`AdapterSetup`] and [`ExecutorSetup`]); at run time, what the chain
//! calls on every run ([`Drive`], [`Adapt`] and [`Execute`]), each with its
//! setup trait as supertrait. And the errors the run-time methods return,
//! with the [`Failure`] an `execute` and a `drive` share.
//!
//! A chain is a driver, any number of adapters, and an executor. The driver
//! sends batches of instructions down; each adapter answers each batch it
//! receives by sending batches of its own to the part below; the executor
//! runs them. Users compose parts with `>>`, which checks at each join that
//! the parts fit and, once the chain is whole, that each part's offer
//! serves the driver's needs as they reach it, and makes a runner; so a
//! part writes only its own logic.
//!
//! Each error holds only what can happen where it is returned: a `ready`
//! says the part is not ready ([`NotReady`]); an `execute` is refused or
//! fails ([`ExecuteError`]); a `drive` only fails ([`DriveError`]), since a
//! refusal from below once a run started is a failure of the run.
//!
//! The crate's `docs/writing-a-part.md` gives each role's methods, phase
//! by phase, and the rules a part keeps that the protocol does not check.

mod adapt;
mod adapter_setup;
mod drive;
mod driver_setup;
mod execute;
mod executor_setup;
mod failure;
mod not_ready;

pub use adapt::Adapt;
pub use adapter_setup::AdapterSetup;
pub(crate) use drive::EndedRun;
pub use drive::{Drive, DriveError};
pub use driver_setup::DriverSetup;
pub use execute::{Execute, ExecuteError, ExecuteRefusal};
pub use executor_setup::ExecutorSetup;
pub use failure::Failure;
pub use not_ready::NotReady;
