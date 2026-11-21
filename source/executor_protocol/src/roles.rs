// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The three roles a part takes in a chain, one trait each, which an
//! implementer writes: [`Driver`], [`Adapter`] and [`Executor`]. Each
//! trait's methods come in two groups: its declarations, which the joins
//! read, and what it is handed back, once, before any run; and what the
//! chain calls on every run. And the errors the run's methods return, with
//! the [`Failure`] an `execute` and a `drive` share.
//!
//! A chain is a driver, any number of adapters, and an executor. The driver
//! sends batches of instructions down; each adapter answers each batch it
//! receives by sending batches of its own to the part below; the executor
//! runs them. Users join parts with `through` and `on`, which check at each
//! join that what the part above requires fits what the part below
//! provides and, once the chain is whole, that each requirement fits every
//! part it reaches, and make a runner; so a part writes only its own
//! logic.
//!
//! Each error holds only what can happen where it is returned: a `ready`
//! says the part is not ready ([`NotReady`]); an `execute` is refused or
//! fails ([`ExecuteError`]); a `drive` only fails ([`DriveError`]), since a
//! refusal from below once a run started is a failure of the run.
//!
//! The crate's `docs/writing-a-part.md` gives each role's methods, phase
//! by phase, and the rules a part keeps that the protocol does not check.

mod adapter;
mod driver;
mod executor;
mod failure;
mod not_ready;

pub use adapter::Adapter;
pub use driver::{DriveError, Driver};
pub use executor::{ExecuteError, ExecuteRefusal, Executor};
pub use failure::Failure;
pub use not_ready::NotReady;
