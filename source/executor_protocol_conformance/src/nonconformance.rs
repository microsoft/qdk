// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a check returns when a part does not keep its rule:
//! [`Nonconformance`], which names the part, each rule it broke
//! ([`BrokenRule`]) and the places where the check found it broken
//! ([`Fault`], at a [`Place`]; which places, each check's Errors section
//! says), or why the check could not judge the part, and the trial that
//! found it, to replay.

use std::fmt;

use executor_protocol::{RunError, RunRefusal};

use crate::trials::{Trial, Trials};

mod broken_rule;
mod fault;
mod place;

pub use broken_rule::BrokenRule;
pub use fault::Fault;
pub(crate) use fault::shot_list;
pub use place::Place;

/// Why a check does not pass: the part broke rules, or the check could not
/// judge it with what it was given; each in the first trial that found it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Nonconformance {
    /// The part broke these rules: the check's own, and any other the check
    /// saw broken on the way, such as a batch's readouts' shape.
    Broken {
        /// The part's name, as its `name` gives it.
        part: String,
        /// The trial that found them: its places are that trial's shots,
        /// batches and `execute`s.
        trial: Trial,
        /// Each rule broken, with the places where the check found it so.
        rules: Vec<BrokenRule>,
    },
    /// The check could not judge the part: what it was given does not let
    /// a broken rule show (too few shots, nothing read), or a run did not
    /// end as the check needs. Says what to change.
    CannotJudge {
        /// The trial that could not judge it.
        trial: Trial,
        /// Why, and what to change.
        why: String,
    },
}

impl fmt::Display for Nonconformance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Nonconformance::Broken { part, trial, rules } => {
                let s = if rules.len() == 1 { "" } else { "s" };
                write!(f, "`{part}` breaks {} rule{s}, in {trial}:", rules.len())?;
                for rule in rules {
                    write!(f, "\n- {}:", rule.rule)?;
                    for fault in &rule.faults {
                        write!(f, "\n  - {fault}")?;
                    }
                }
                replay(f, *trial)
            }
            Nonconformance::CannotJudge { trial, why } => {
                write!(f, "the check cannot judge the part, in {trial}: {why}")?;
                replay(f, *trial)
            }
        }
    }
}

/// How to replay `trial`, if it is a drawn one: trial 0 runs whatever the
/// trials. The environment's form runs the trials before it too, which
/// passed, so it ends with the same report.
fn replay(f: &mut fmt::Formatter<'_>, trial: Trial) -> fmt::Result {
    if trial.is_fixed() {
        return Ok(());
    }
    let Trial { seed, index } = trial;
    write!(
        f,
        "\nReplay it with `Trials::replaying(Trial {{ seed: {seed:#x}, index: {index} }})`, or \
         rerun the trials up to it with {}={seed:#x} and {}={index} for checks given \
         `Trials::from_env()`.",
        Trials::SEED_VARIABLE,
        Trials::COUNT_VARIABLE
    )
}

impl std::error::Error for Nonconformance {}

/// Why a check cannot use a run of `part` that failed, naming the part,
/// and what to change: a run the chain refused ran nothing, so it says
/// why, from the refusal, with the advice that fits it (a seedable build
/// of a part that is not seedable, fewer shots for a part that offers
/// fewer, a run again once a part that was not ready, a busy device say,
/// is ready).
pub(crate) fn failed_run(part: &str, failure: &RunError) -> String {
    match failure {
        RunError::Refused(RunRefusal::NotSeedable) => format!(
            "the chain refused a run ({failure}), so it ran nothing: `{part}` is not seedable, \
             and every run of a check fixes its seed, so that the check can compare runs and \
             replay a trial; check a seedable build of the part, if it has one"
        ),
        RunError::Refused(RunRefusal::TooManyShots { most, .. }) => format!(
            "the chain refused a run ({failure}), so it ran nothing: `{part}` offers runs of \
             at most {most} shots; give the check at most {most} shots"
        ),
        RunError::Refused(RunRefusal::PartNotReady { .. }) => format!(
            "the chain refused a run ({failure}), so it ran nothing; run the check again once \
             `{part}` is ready"
        ),
        RunError::Refused(_) => format!(
            "the chain refused a run of `{part}` ({failure}), so it ran nothing; the check \
             needs runs that end"
        ),
        _ => format!("a run of `{part}` failed ({failure}); the check needs runs that end"),
    }
}

/// The composition's faults, as why the kit cannot run its chain.
pub(crate) fn uncomposed(faults: &impl std::fmt::Display) -> String {
    format!("the kit's chain around the part does not compose: {faults}")
}
