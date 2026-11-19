// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol's conformance kit: checks, run in an implementer's
//! tests, of the rules a part keeps that the protocol states but does not
//! check as it runs, such as writing every readout value.
//!
//! The parts above a part index its readouts with no check of their own,
//! and the protocol checks only what it can check cheaply at its
//! boundaries; so a part that breaks one of these rules gives wrong results
//! to the parts above it, with no error. Each check runs the part in a real
//! chain, between the kit's own driver and executor, and reports every
//! place where the part breaks the rule, in a [`Nonconformance`].
//!
//! - [`executor`]: the rules an executor keeps.
//! - [`adapter`]: the rules an adapter keeps, an executor's included except
//!   keeping its `accepts` and `offers`, checked on the adapter over the
//!   kit's executor.
//! - [`driver`]: the rules a driver keeps.
//!
//! The checks share these types: a [`Script`], what the kit's driver sends
//! an executor or an adapter; the [`Trials`] a check runs, each a
//! [`Trial`]; a [`Nonconformance`], why a check does not pass, which holds
//! each [`BrokenRule`] with its [`Fault`]s, each at a [`Place`]; and a
//! [`Rule`], each rule a report names.
//!
//! Each check takes `make`, which builds the part afresh, the same each
//! time (each run on a new part, except in the executor's and the
//! adapter's `runs_as_new_after_a_failed_run`),
//! and what to run: a [`Script`]; or shots and two lists of instructions;
//! or, for a driver, its shots, and, to check merged conditions, how to
//! read them in an output. Last, it takes the [`Trials`] to run: trial 0
//! runs what it is given, on fixed patterns; each later trial runs the
//! same instructions on patterns drawn from a seed (such as the shots'
//! ids, order, batches and groups, where a run fails, and the conditions
//! raised below), so that a part fitted to the fixed patterns still shows.
//! A report names its trial, which [`Trials::replaying`] runs again. Every
//! run of a trial has the trial's seed, so a part under an executor's or
//! an adapter's check must be seedable. A test that gives a check
//! [`Trials::from_env`] runs the default trials, and more seeds or trials
//! when its environment variables are set, with no change to the test.
//!
//! Each check's documentation says why its rule matters to the parts
//! around the part, how the check runs it, an example of a fault with the
//! report it gives, and what the check does not see. The crate's
//! `docs/checks.md` lists every check with its rule, how to read a report
//! and how to add a rule; `docs/trials.md` says how to replay a report and
//! run more trials; and `docs/limits.md` lists what the kit does not check.
//!
//! The kit checks one part at a time: a composed chain's parts get ready
//! for the needs that cross the chain's own boundaries, which `>>` keeps
//! through hooks a check's wrapper cannot forward. Check each part of a
//! chain on its own.
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
mod nonconformance;
mod readout_prefills;
mod rule;
mod script;
mod shared_run;
mod shot_records;
mod splitmix64;
mod trials;

#[cfg(test)]
mod tests;

pub use nonconformance::{BrokenRule, Fault, Nonconformance, Place};
pub use rule::Rule;
pub use script::Script;
pub use trials::{Trial, Trials};
