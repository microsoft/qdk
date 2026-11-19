// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol's conformance kit: checks, run in an implementer's
//! tests, of the rules a part keeps that the protocol states but does not
//! check as it runs, such as writing every readout value. Each check runs
//! the part in a real chain, between the kit's own driver and executor,
//! and returns `Ok(())` or a [`Nonconformance`].
//!
//! The checks are in a module per role: [`executor`], [`adapter`] and
//! [`driver`]. The crate's `README.md` says what the kit is for, what each
//! role's checks cover and how to call one; its `docs/` folder lists every
//! check, how to replay a report, and what the kit does not check.
//!
//! Not on wasm32: the checks wait for each run on the calling thread,
//! which wasm32 cannot do (`Runner::run` is not built there).

// On wasm32 the crate is empty, so its docs' links to its items resolve to
// nothing. This comes before the `cfg`, which drops what follows it there.
#![cfg_attr(target_arch = "wasm32", allow(rustdoc::broken_intra_doc_links))]
#![cfg(not(target_arch = "wasm32"))]

pub mod adapter;
pub mod driver;
pub mod executor;

mod caller_faults;
mod chain;
mod findings;
mod nonconformance;
mod readout_prefills;
mod rule;
mod script;
mod shared_run;
mod shot_records;
mod splitmix64;
mod trial_patterns;
mod trials;

#[cfg(test)]
mod tests;

pub use nonconformance::{BrokenRule, Fault, Nonconformance, Place};
pub use rule::Rule;
pub use script::Script;
pub use trials::{Trial, Trials};
