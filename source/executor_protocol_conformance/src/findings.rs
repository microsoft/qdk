// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Findings`]: what a trial of a check found, gathered from its runs'
//! logs, until it gives its result: a [`Nonconformance`], or a pass.

use executor_protocol::RunError;

use crate::nonconformance::{BrokenRule, Fault, Nonconformance, Place, failed_run};
use crate::rule::Rule;
use crate::shared_run::RunLog;
use crate::trials::Trial;

/// What a trial of a check found, gathered as it runs: the part's name,
/// the rules broken with their faults, the first reason it cannot judge
/// the part, and how many of its runs went without the shot their script
/// never names.
pub(crate) struct Findings {
    trial: Trial,
    part: String,
    rules: Vec<BrokenRule>,
    why_cannot_judge: Option<String>,
    runs: usize,
    runs_without_unsent: usize,
}

impl Findings {
    /// Nothing found yet, in `trial`.
    pub(crate) fn new(trial: Trial) -> Self {
        Self {
            trial,
            part: String::new(),
            rules: Vec::new(),
            why_cannot_judge: None,
            runs: 0,
            runs_without_unsent: 0,
        }
    }

    /// The seed of every run of the trial.
    pub(crate) fn run_seed(&self) -> u64 {
        self.trial.run_seed()
    }

    /// Notes the part's name.
    pub(crate) fn note_part_name(&mut self, name: &str) {
        name.clone_into(&mut self.part);
    }

    /// Adds a fault of `rule` at `place`, unless one with the same place
    /// and text is there already, as when several runs find the same.
    pub(crate) fn fault(&mut self, rule: Rule, place: Place, found: impl Into<String>) {
        let fault = Fault {
            place,
            found: found.into(),
        };
        let index = self
            .rules
            .iter()
            .position(|broken| broken.rule == rule)
            .unwrap_or_else(|| {
                self.rules.push(BrokenRule {
                    rule,
                    faults: Vec::new(),
                });
                self.rules.len() - 1
            });
        let faults = &mut self.rules[index].faults;
        if !faults.contains(&fault) {
            faults.push(fault);
        }
    }

    /// Adds `faults` of `rule`.
    pub(crate) fn faults(&mut self, rule: Rule, faults: impl IntoIterator<Item = Fault>) {
        for fault in faults {
            self.fault(rule, fault.place, fault.found);
        }
    }

    /// Notes that the check cannot judge the part, and why, unless an
    /// earlier reason was noted. [`Findings::finish`] gives it only when
    /// no rule was found broken, so a check notes it whatever it found.
    pub(crate) fn cannot_judge(&mut self, why: impl Into<String>) {
        self.why_cannot_judge.get_or_insert_with(|| why.into());
    }

    /// Notes that the trial's patterns cannot show a broken rule, for
    /// `why`: in trial 0, whose patterns are the ones the implementer
    /// chose, the check cannot judge the part, as for
    /// [`Findings::cannot_judge`]; a drawn trial whose patterns happen not to
    /// show one passes, unless it found a rule broken, as the other
    /// trials judge the part.
    pub(crate) fn undetectable(&mut self, why: impl Into<String>) {
        if self.trial.is_fixed() {
            self.cannot_judge(why);
        }
    }

    /// Notes that the check did not judge the part on `rule`, which does
    /// not bind it, for `why`: in trial 0, writes so ([`unexercised`]),
    /// since the check then passes with nothing to show it judged nothing.
    pub(crate) fn not_judged(&self, rule: Rule, why: &str) {
        if self.trial.is_fixed() {
            unexercised(&format!(
                "{}: the check did not judge {rule:?}, since {why}",
                self.part
            ));
        }
    }

    /// Notes that the check did not judge whether the trial's patterns can
    /// show `rule` broken, for `why`: in trial 0, writes so
    /// ([`unexercised`]), since the check then judges the part without
    /// knowing that its patterns could show a fault.
    pub(crate) fn detectability_not_judged(&self, rule: Rule, why: &str) {
        if self.trial.is_fixed() {
            unexercised(&format!(
                "{}: the check did not judge whether its patterns can show {rule:?} broken, \
                 since {why}",
                self.part
            ));
        }
    }

    /// Notes what a run's log shows of every check's rules: the part's
    /// name, each batch answered in another shape, each `end_shots` that
    /// panicked on shots the part did not hold, and whether the run went
    /// without the shot its script never names.
    pub(crate) fn note_run(&mut self, log: &RunLog) {
        self.note_part_name(&log.part);
        self.runs += 1;
        self.runs_without_unsent += usize::from(log.without_unsent);
        self.faults(Rule::KeepsEachBatchsShape, log.shape_faults.iter().cloned());
        self.faults(
            Rule::IgnoresEndsOfShotsItDoesNotHold,
            log.end_faults.iter().cloned(),
        );
    }

    /// The check's result once a run it needs failed, or could not be
    /// built, for `why`: the rules found broken so far, if any, as
    /// [`Findings::finish`] reports them; else the first reason noted not to
    /// judge the part, an earlier one or else `why`, as
    /// [`Findings::cannot_judge`] keeps it.
    pub(crate) fn failed(&mut self, why: impl Into<String>) -> Nonconformance {
        self.cannot_judge(why);
        let taken = std::mem::replace(self, Self::new(self.trial));
        taken
            .finish()
            .expect_err("a check with a reason not to judge the part does not pass")
    }

    /// The check's result once a run failed for `failure`, as
    /// [`Findings::failed`] gives it, saying why the check cannot use the
    /// run, and naming the part.
    pub(crate) fn run_failed(&mut self, failure: &RunError) -> Nonconformance {
        let why = failed_run(&self.part, failure);
        self.failed(why)
    }

    /// The trial's result: the broken rules if any, else the reason it
    /// could not judge the part if any, else a pass. In trial 0, first
    /// writes what its runs did not exercise ([`unexercised`]), whatever
    /// the result.
    pub(crate) fn finish(self) -> Result<(), Nonconformance> {
        let trial = self.trial;
        if trial.is_fixed() && self.runs_without_unsent > 0 {
            unexercised(&format!(
                "{}: {} of the {} runs of trial 0 held only the shots the check was given, none \
                 the kit never sends ({:?}): the chain refused a run of one shot more, or \
                 shot {} is among those given. To give each run a shot the kit never sends, \
                 give fewer shots than the chain's `max_shots`, with ids below {}.",
                self.part,
                self.runs_without_unsent,
                self.runs,
                Rule::IgnoresEndsOfShotsItDoesNotHold,
                u32::MAX,
                u32::MAX,
            ));
        }
        if !self.rules.is_empty() {
            return Err(Nonconformance::Broken {
                part: self.part,
                trial,
                rules: self.rules,
            });
        }
        match self.why_cannot_judge {
            Some(why) => Err(Nonconformance::CannotJudge { trial, why }),
            None => Ok(()),
        }
    }
}

/// Writes `note`, on what a check's runs did not exercise, to the standard
/// error, since a passing check returns `Ok(())`, with no report to hold
/// it; the test harness shows it beside a failing test, or with
/// `--nocapture`.
fn unexercised(note: &str) {
    eprintln!("executor_protocol_conformance: not exercised: {note}");
    #[cfg(test)]
    NOTED.with_borrow_mut(|noted| noted.push(note.to_owned()));
}

#[cfg(test)]
thread_local! {
    /// Each note [`unexercised`] wrote on this thread, for the kit's tests.
    static NOTED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The notes [`unexercised`] wrote on this thread since the last call.
#[cfg(test)]
pub(crate) fn take_unexercised() -> Vec<String> {
    NOTED.with_borrow_mut(std::mem::take)
}
