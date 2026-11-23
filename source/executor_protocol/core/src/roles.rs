// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The three roles a part takes in a chain, one trait each, which an
//! implementer writes: [`Driver`], [`Adapter`] (or, for one that sends
//! another set type than it receives, [`TranslatingAdapter`]) and
//! [`Executor`]. Each
//! trait's methods come in two groups: its declarations, which the joins
//! read before any run, with what it is handed back, once, before any
//! run; and what the chain calls on every run. And the errors the run's
//! methods return, with the [`Failure`] an `execute` and a `drive` share.
//!
//! A chain is a driver, any number of adapters, and an executor. The driver
//! sends batches of instructions down; each adapter answers each batch it
//! receives by sending batches of its own to the part below; the executor
//! runs them. Users join parts with `through` and `on`, which check at each
//! join that the instructions the part above sends are ones the part below
//! accepts and, once the chain is whole, that what arrives at each part
//! fits what it provides, and make a runner; so a part writes only its
//! own logic.
//!
//! Each error holds only what can happen where it is returned: a `ready`
//! says the part is not ready ([`NotReady`]); an `execute` is refused or
//! fails ([`ExecuteError`]); a `drive` only fails ([`DriveError`]), since a
//! refusal from below once a run started is a failure of the run.
//!
//! The crate's `docs/writing-a-part.md` gives each role's methods, phase
//! by phase, and the rules a part keeps that the protocol does not check;
//! its `docs/capabilities.md` gives the two rules a `requires` keeps.

mod adapter;
mod driver;
mod executor;
mod failure;
mod not_ready;
mod translating_adapter;

pub use adapter::{Adapter, AdapterLimits};
pub use driver::{DriveError, Driver, DriverReadings, DriverRequirements};
pub use executor::{ExecuteError, ExecuteRefusal, Executor, ExecutorLimits, ExecutorReadings};
pub use failure::Failure;
pub use not_ready::NotReady;
pub use translating_adapter::TranslatingAdapter;
pub(crate) use translating_adapter::lent_translating_adapter;

/// What the roles' hidden hooks that no other parameter seals take, so
/// that only the crate calls or overrides them: the hooks through which
/// the protocol reads a part's limits.
///
/// It is public, because the hooks name it, but no public path reaches
/// it, and only the crate makes one.
pub struct Sealed(pub(crate) ());
