// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks of the rules that the type system cannot enforce, for
//! implementers to run in their own tests: the compiler checks that a type
//! has `call`, `close` and `fold`, and that a call is an
//! [`InstructionCall`], not that they keep the rules that callers rely on.
//! They check the rules of [`Executor`] (a refusal for the call's own
//! fault leaves the executor usable, a failure is final, and ends the
//! later calls still in flight once it is recorded as the executor's: the
//! check polls the failing call's reply first), of [`Reply`] (a reply
//! appends its events, keeping what the caller held, checked wherever a
//! check reads readouts), of [`Close`], [`Fold`], described instruction sets
//! and readout tags, and the promises of
//! [`Capabilities`](crate::Capabilities) (`replayable`; `resumable_shots`, with
//! its rule that a call changes no shot it does not name), and of shot
//! conditions (a shot keeps from one call to the next those its outcomes
//! carried; the checks that compare a shot's readouts over the same calls
//! compare the conditions they carry too, since a shot's conditions are
//! its own, as its readouts are). They also
//! check that a new executor starts its shots as made, whatever an earlier
//! one left, closed or dropped, a rule every check that builds several
//! executors relies on.
//! Built with the `conformance` feature, so a crate that implements the
//! traits enables it as a dev-dependency:
//!
//! ```toml
//! [dev-dependencies.executor_protocol]
//! path = "../executor_protocol"
//! features = ["conformance"]
//! ```
//!
//! Each check panics, naming the rule broken, when the implementation does
//! not keep it, and names what it needs when its inputs do not let it check
//! the rule. A check of a rule or a need on each shot names every shot that
//! breaks it, so that one run reports every fault.
//!
//! The checks, by the family of rules they check:
//!
//! - `lifecycle`: [`rejects_calls_once_closed`],
//!   [`accepts_calls_after_a_refusal`], [`refuses_calls_after_a_failure`]
//!   and [`ends_calls_in_flight_at_an_earlier_failure`].
//! - `reports`: [`reports_once`], [`default_is_empty`] and
//!   [`folds_in_any_order`].
//! - `instruction_sets`: [`refuses_calls_outside_its_set`].
//! - `readout_tags`: [`refuses_a_tag_repeated_within_a_batch`] and
//!   [`refuses_a_repeated_tag_at_the_index_of_its_instruction`].
//! - `shots`: [`gives_each_shot_alone_what_it_gives_among_others`],
//!   [`continues_a_shot_from_one_call_to_the_next`],
//!   [`changes_no_shot_a_call_does_not_name`],
//!   [`starts_its_shots_as_made_whatever_an_earlier_executor_left`] and
//!   [`keeps_each_shots_conditions_from_one_call_to_the_next`].
//! - `readouts`: how the checks read what an executor gives (a call's
//!   outcomes, a run's readouts by shot), and a check on each shot.

// Known issue, open (blocks neither this commit nor a later one): no check
// here of the rule that a wrapper never forgets a refusal (`Executor`'s
// contract): an executor that wraps another, or is made of one, keeps
// refusing once the other is closed or failed. It would need a function
// that wraps the executor under test, a new shape for a check; whether to
// add one is a design question held for the user. Only the crate's own
// wrappers are tested (`.run_on(..)` and `.shots(..)`).

#[cfg(doc)]
use crate::{Close, Executor, Fold, InstructionCall, Reply};

mod instruction_sets;
mod lifecycle;
mod readout_tags;
mod readouts;
mod reports;
mod shots;

pub use instruction_sets::refuses_calls_outside_its_set;
pub use lifecycle::{
    accepts_calls_after_a_refusal, ends_calls_in_flight_at_an_earlier_failure,
    refuses_calls_after_a_failure, rejects_calls_once_closed,
};
pub use readout_tags::{
    refuses_a_repeated_tag_at_the_index_of_its_instruction, refuses_a_tag_repeated_within_a_batch,
};
pub use reports::{default_is_empty, folds_in_any_order, reports_once};
pub use shots::{
    changes_no_shot_a_call_does_not_name, continues_a_shot_from_one_call_to_the_next,
    gives_each_shot_alone_what_it_gives_among_others,
    keeps_each_shots_conditions_from_one_call_to_the_next,
    starts_its_shots_as_made_whatever_an_earlier_executor_left,
};
