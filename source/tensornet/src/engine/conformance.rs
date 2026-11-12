// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conformance kit: checks an [`MpsEngine`] against [`ExactReference`].
//!
//! Each case runs the same operations on the engine and on the reference,
//! on chains of at most five sites, and compares what the engine reads
//! back: its answers must agree within [`TOLERANCE`], and each call with
//! one fault must be refused with the reference's [`Refusal`], the state
//! unchanged. An engine's tests run the kit with [`check`], which returns
//! every failure, each naming its case and what differed:
//!
//! ```no_run
//! let engine = tensornet::reference::ExactReference::new();
//! let failures = tensornet::conformance::check(&engine);
//! assert!(failures.is_empty(), "{failures:#?}");
//! ```
//!
//! The cases, by the part of the contract they check; each case's function
//! says what it checks and the defects it catches:
//!
//! - `reads.rs`: the reads of states no truncation touched, after gates on
//!   neighbours (`bell`, `layout`) and on distant sites (`distant sites`);
//! - `conditionals.rs`: `conditional` probabilities, and the rounding an
//!   engine may leave on a condition impossible in exact arithmetic;
//! - `projection.rs`: `project` and `fork`;
//! - `sampling.rs`: `sample`, within the bounds `statistics.rs` derives;
//! - `capacity.rs`: the chain and its truncation (`shape`, `retained
//!   weight`, `truncated reads`, `truncated chain`, `uneven capacity`,
//!   `distant gate within capacity`, `wide gate truncation`), and a `lone
//!   site`;
//! - `refusals.rs`: `refusals`, each call with one fault refused exactly.
//!
//! An engine that truncates must keep, at each cut whose Schmidt rank
//! exceeds its bond, the largest Schmidt values, as a singular value
//! decomposition of the chain in canonical form at the cut does, and every
//! squared Schmidt value of at least 1e-9 of its cut's weight;
//! `capped_twin.rs` says how the kit computes the state such an engine
//! must hold.
//!
//! The sampling checks are statistical: a correct engine fails them with
//! probability below 1e-8 in all (at most 262 frequencies at 1e-11 each,
//! one independence check at 4e-9, and one repeat check at about
//! 2.5e-30), and a seed check fails it only when two seeds select the same
//! values, of probability 2⁻¹²⁸ (or about 2⁻³¹ a pair, 2⁻²⁹ for the four
//! pairs, for an engine that hashes the seed into a narrower range, which
//! keeps the sum, about 8.5e-9, below 1e-8).
//!
//! No fault of the engine makes the kit panic, so a failing engine reports
//! every case it fails. The kit needs the `conformance` feature, so it
//! costs nothing in a build that does not test an engine.

use crate::MpsEngine;
use std::fmt;

#[cfg(doc)]
use crate::{MpsShape, Refusal, reference::ExactReference};

mod capacity;
mod capped_twin;
mod conditionals;
mod gates;
mod observables;
mod projection;
mod reads;
mod refusals;
mod sampling;
mod schmidt;
pub(crate) mod statistics;
mod twin;

#[cfg(test)]
pub(crate) use conditionals::condition_probability;
pub(crate) use gates::{cnot, controlled_y, fused_gates, kron, product, ry, s_h};
pub(crate) use schmidt::{normalized, truncated};

/// How far an engine's probability or expectation may be from the
/// reference's. An fp64 engine that does not truncate is far closer.
pub const TOLERANCE: f64 = 1e-9;

/// The largest probability, in the normalized state, either side of zero,
/// the kit takes for rounding left on a condition that is impossible in
/// exact arithmetic, where an engine may answer the condition instead of
/// refusing it: 10⁻²⁰, an amplitude of 10⁻¹⁰, about 4.5·10⁵ ε. An fp64
/// engine's factorizations (a QR or a singular value decomposition of a
/// matrix of side at most 2χ as a read or a projection moves along the
/// chain) leave a value they zeroed an amplitude of about n·(2χ)²·ε at most
/// on a chain of n sites, 320 ε on the kit's largest (five sites, χ at most
/// 4): over a thousand times below the bound; one engine measured
/// 3.6·10⁻³², about 0.7 ε². A defect whose projections, or whose
/// conditional probabilities along the condition, leave 10⁻¹⁹ of either
/// sign is caught.
pub const IMPOSSIBLE_OUTCOME_ROUNDING: f64 = 1e-20;

/// The certainty allowance the kit grants: 10⁻¹², about 4500 ε.
///
/// Where a condition is impossible and an engine answers it, the kit reads the
/// engine's probability of the condition along its conditional path (the
/// product of its P(vᵢ | the values before vᵢ) over the condition), taking
/// P(Zero) as one minus its P(One). Near one, fp64 resolves that difference
/// only to a few ε, so a P(One) within the allowance of one, either side,
/// counts as a certain One: the step's P(Zero) counts as zero. The allowance
/// forgives that rounding and nothing else: a P(One) that falls 10⁻¹⁰ short of
/// one where the value One is certain (a leak of 10⁻¹⁰ on the value Zero) is
/// still caught, as is any leak on a step whose value is One, which the
/// allowance does not touch. The allowance is about 2300 times the rounding
/// measured: the trait's default conditional probability, by projections of a
/// fork, answered 2 ε (4.4·10⁻¹⁶) below one on the kit's cases. An engine
/// whose P(One) near one rounds further fails the kit there; whether such an
/// engine needs a larger allowance is a question for that engine.
pub const DEFAULT_CERTAINTY_ALLOWANCE: f64 = 1e-12;

/// One way an engine disagreed with the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The case that failed.
    pub case: &'static str,
    /// What differed.
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.case, self.message)
    }
}

/// A case of the kit: the name its [`Failure`] gives it, and the function
/// that runs it on the engine.
type Case<E> = (&'static str, fn(&E) -> Result<(), String>);

/// The kit's cases, in the order [`check`] runs them.
fn cases<E: MpsEngine>() -> [Case<E>; 16] {
    [
        ("bell", reads::bell),
        ("layout", reads::layout),
        ("distant sites", reads::distant_sites),
        ("conditional", conditionals::conditional),
        ("project", projection::project),
        ("fork", projection::fork),
        ("sample", sampling::sample),
        ("shape", capacity::shape),
        ("retained weight", capacity::retained_weight),
        ("truncated reads", capacity::truncated_reads),
        ("truncated chain", capacity::truncated_chain),
        ("uneven capacity", capacity::uneven_capacity),
        (
            "distant gate within capacity",
            capacity::distant_gate_within_capacity,
        ),
        ("wide gate truncation", capacity::wide_gate_truncation),
        ("lone site", capacity::lone_site),
        ("refusals", refusals::refusals),
    ]
}

/// Runs every case against `engine` and returns the failures, empty when
/// the engine conforms.
pub fn check<E: MpsEngine>(engine: &E) -> Vec<Failure> {
    cases()
        .into_iter()
        .filter_map(|(case, run)| run(engine).err().map(|message| Failure { case, message }))
        .collect()
}
