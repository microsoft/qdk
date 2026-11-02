// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an executor reports: the outcomes of each shot's readouts, their
//! values, and the check that within a batch a tag names one readout of
//! each shot.

#[cfg(doc)]
use crate::{ExecutorReport, LogP, Outcomes};
use crate::{Rejected, ShotConditions, ShotId};

/// One readout of one shot, as an executor reports it: its shot, its tag,
/// its value, and the shot's conditions so far.
///
/// The tag is the batch's name for the readout (for example a result id), so
/// that each readout is matched to the instruction that asked for it, in
/// whatever order a shot's readouts arrive: the executors `executor_fn`
/// makes give them in the shot's measurement order, but an adapter may give
/// them in another (a decoder delivers a logical readout when its last
/// physical readout arrives; `docs/replies.md`, §Replies), so a consumer
/// matches readouts by tag, not by their place in the reply.
///
/// No probability: a shot's log probability is reported once, at close
/// ([`ExecutorReport::log_p`], whose [`LogP`] says why), not per readout.
///
/// It is non-exhaustive so that a field may be added without breaking the
/// crates that make one, by [`ShotOutcome::new`], or match one, by
/// `ShotOutcome { shot, tag, value, .. }`, as [`conditions`](Self::conditions)
/// was added. Another crate makes one with [`ShotOutcome::new`], with no
/// condition, sets its conditions with
/// [`with_conditions`](Self::with_conditions), and matches it with `..`:
///
/// ```
/// use executor_protocol::{ShotConditions, ShotOutcome, Value};
///
/// let outcome = ShotOutcome::new(3, 0, Value::One);
/// assert!(outcome.conditions.is_empty());
/// let rejected = outcome.with_conditions(ShotConditions::REJECTED);
/// let ShotOutcome {
///     shot, conditions, ..
/// } = rejected;
/// assert_eq!((shot, conditions), (3, ShotConditions::REJECTED));
/// ```
///
/// but not by naming its fields: the struct is non-exhaustive, so a
/// literal from another crate does not compile (`E0639`), which is what
/// keeps a field added later from breaking such call sites:
///
/// ```compile_fail,E0639
/// use executor_protocol::{ShotConditions, ShotOutcome, Value};
///
/// let outcome = ShotOutcome {
///     shot: 3,
///     tag: 0,
///     value: Value::One,
///     conditions: ShotConditions::NONE,
/// };
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ShotOutcome {
    /// The shot that produced the outcome.
    pub shot: ShotId,
    /// The batch's name for the readout.
    ///
    /// Within a batch, a tag names one of the readouts the batch asks for,
    /// which each shot reports once, so that the shot and the tag name one
    /// outcome: two readouts of a shot under one tag could not be told
    /// apart, since they need not arrive in the order read. A later batch
    /// may use the tag again, for another readout. An executor that reports
    /// its readouts by tag refuses a batch that repeats a tag
    /// ([`check_unique_tags`]). A program whose names repeat within a batch
    /// needs a readout register that gives each readout a fresh tag and
    /// keeps the later value of a slot: planned, with no milestone yet, for
    /// cases such as a runtime's result slots (a measurement writes a slot,
    /// a later write wins), or a runtime that defers measurements so that a
    /// batch holds several readouts (`docs/replies.md`, §Readout tags).
    pub tag: u32,
    /// What was read.
    pub value: Value,
    /// The shot's conditions, as the executor that gave the outcome knew
    /// them: those raised on the shot, by it or below it, merged by union.
    /// They are the shot's, not the readout's: the shot's outcomes in later
    /// calls carry them too, with those raised since (the conformance
    /// kit's `keeps_each_shots_conditions_from_one_call_to_the_next`), so a
    /// caller takes a shot's conditions as the union of those its outcomes
    /// carry. A condition raised in a call that gives the shot no outcome
    /// reaches no caller in that call. An executor that offers `feedback`
    /// must keep its shots' conditions from one call to the next, which
    /// only the kit checks, and only for the conditions that an earlier
    /// outcome of the shot carried. An [`adapter_fn`](fn@crate::adapter_fn)
    /// never sees those that a send's reply gives after its call returned,
    /// so the shot's later outcomes lack them over an executor that breaks
    /// that requirement; over one that keeps it, the outcomes its logic
    /// writes lack them in every call before the first in which the adapter
    /// takes them in from an outcome of the shot read from below, a known
    /// issue (`docs/replies.md`, §Shot conditions).
    pub conditions: ShotConditions,
}

impl ShotOutcome {
    /// The readout `tag` of shot `shot`, which read `value`, with no
    /// condition.
    #[must_use]
    pub const fn new(shot: ShotId, tag: u32, value: Value) -> Self {
        Self {
            shot,
            tag,
            value,
            conditions: ShotConditions::NONE,
        }
    }

    /// The same outcome, with `conditions` in place of its own.
    #[must_use]
    pub const fn with_conditions(self, conditions: ShotConditions) -> Self {
        Self { conditions, ..self }
    }
}

/// One readout without its shot: its tag and its value. The logic of an
/// [`executor_fn`](fn@crate::executor_fn) pushes it
/// ([`Outcomes`](crate::Outcomes)), which names it by the shot the logic
/// runs on, so that the logic cannot name the wrong one. Like a
/// [`ShotOutcome`], it carries no probability.
///
/// It is non-exhaustive, as [`ShotOutcome`] is, so that a field may be
/// added without breaking the crates that use it. Another crate makes one
/// with [`Outcome::new`], with no condition, sets its conditions with
/// [`with_conditions`](Self::with_conditions), and matches it with `..`:
///
/// ```
/// use executor_protocol::{Outcome, ShotConditions, Value};
///
/// let outcome = Outcome::new(0, Value::One)
///     .with_conditions(ShotConditions::UNRESOLVED);
/// let Outcome { tag, conditions, .. } = outcome;
/// assert_eq!((tag, conditions), (0, ShotConditions::UNRESOLVED));
/// ```
///
/// A literal from another crate does not compile (`E0639`):
///
/// ```compile_fail,E0639
/// use executor_protocol::{Outcome, ShotConditions, Value};
///
/// let outcome = Outcome {
///     tag: 0,
///     value: Value::One,
///     conditions: ShotConditions::NONE,
/// };
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Outcome {
    /// The batch's name for the readout ([`ShotOutcome::tag`]).
    pub tag: u32,
    /// What was read.
    pub value: Value,
    /// The conditions the logic raises with it on the shots it answers: the
    /// [`ShotOutcome::conditions`] of each outcome it makes, with those the
    /// logic raised in the call ([`Outcomes::raise_conditions`]).
    pub conditions: ShotConditions,
}

impl Outcome {
    /// The readout `tag`, which read `value`, with no condition.
    #[must_use]
    pub const fn new(tag: u32, value: Value) -> Self {
        Self {
            tag,
            value,
            conditions: ShotConditions::NONE,
        }
    }

    /// The same readout, with `conditions` in place of its own.
    #[must_use]
    pub const fn with_conditions(self, conditions: ShotConditions) -> Self {
        Self { conditions, ..self }
    }
}

/// Checks that no two readouts of a batch share a tag, since within a batch
/// a tag names one readout of each shot ([`ShotOutcome::tag`]). An executor
/// that reports its readouts by tag calls it before it runs anything, so
/// that a batch that repeats a tag runs nothing.
///
/// `readouts` lists each readout the batch asks for: the index of its
/// instruction in the batch, and its tag. An instruction that makes several
/// readouts is listed once for each.
///
/// # Errors
///
/// [`Rejected::Instruction`] at the first instruction, in the batch's order,
/// whose readout repeats the tag of one before it, or of another of its own
/// readouts. Its reason names the earlier instruction and the planned
/// readout register.
pub fn check_unique_tags(readouts: impl IntoIterator<Item = (usize, u32)>) -> Result<(), Rejected> {
    let mut readouts = readouts.into_iter();
    // No allocation for a batch of at most one readout.
    let (Some(first), Some(second)) = (readouts.next(), readouts.next()) else {
        return Ok(());
    };
    let mut by_tag: Vec<(u32, usize)> = [first, second]
        .into_iter()
        .chain(readouts)
        .map(|(at, tag)| (tag, at))
        .collect();
    by_tag.sort_unstable();
    // Within a tag, the readouts are in the batch's order: a pair's second
    // repeats its first, and the earliest second is the first repeat.
    let repeat = by_tag
        .windows(2)
        .filter_map(|pair| match *pair {
            [(tag, earlier), (again, at)] if tag == again => Some((at, tag, earlier)),
            _ => None,
        })
        .min();
    match repeat {
        None => Ok(()),
        Some((at, tag, earlier)) => {
            let named = if earlier == at {
                "another readout of this instruction".to_string()
            } else {
                format!("the readout of instruction {earlier}")
            };
            Err(Rejected::Instruction {
                at,
                reason: format!(
                    "tag {tag} already names {named}: within a batch a tag names one \
                     readout of each shot (docs/replies.md, \"Readout tags\"). A program that repeats a \
                     tag within a batch is the first case for the planned readout \
                     register: time to implement it"
                ),
            })
        }
    }
}

/// The value of a readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// The qubit was found in |0⟩.
    Zero,
    /// The qubit was found in |1⟩.
    One,
    /// The readout has no value, for the reason given; it does not end the
    /// run, so the shot's other readouts stand.
    Error(ErrorKind),
}

impl From<bool> for Value {
    fn from(one: bool) -> Self {
        if one { Value::One } else { Value::Zero }
    }
}

/// Why a readout has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The qubit was lost (for example, an atom left its trap).
    Loss,
    /// Any other reason; the kind carries no detail.
    Other,
}
