// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks resumers against the contract between the machine and a
//! resumer.
//!
//! Checks of the rules of [`QirResumer::run`] that the type system cannot
//! enforce, for implementers to run in their own tests: the compiler
//! checks that a resumer has `run`, not that it runs each shot on its own
//! state and reports what callers rely on. Built with the `conformance`
//! feature, so a crate that implements [`QirResumer`] enables it as a
//! dev-dependency:
//!
//! ```toml
//! [dev-dependencies.qdk_simulators]
//! path = "../simulators"
//! features = ["conformance"]
//! ```
//!
//! [`check`] runs every rule on resumers its argument makes, a fresh one
//! per run, and panics naming each rule broken and how, so that one run
//! reports every fault; [`check_with_failing_shots`] also checks the rule
//! on the resumer's own failures, on resumers made to fail for the shots
//! it names. A panic in a run is a fault of the rule whose check met it:
//! the other rules are still checked. The rules:
//!
//! 1. a context per shot: shots that draw random outcomes do not all give
//!    the same records, as they would from one context;
//! 2. a batch's calls run in order;
//! 3. one outcome per call that writes a result, by its result, to its own
//!    shot: a result measured twice, and copied through a branch, is the
//!    same three times;
//! 4. each shot's records at its place in `shots`, listed out of order
//!    and with ids not contiguous;
//! 5. a trap stops its shot, and the error names it, at once and after a
//!    batch;
//! 6. the first failing shot in the order of `shots` is reported, whatever
//!    order the resumer runs them in, traps and the resumer's own failures
//!    alike, and shots that trap in different rounds too;
//! 7. a shot gives the same records alone as among other shots, and in a
//!    later run of the same resumer, after runs of the same shot ids and
//!    of programs of other qubit counts;
//! 8. the edges: no shots, one shot, a program with no quantum call, and
//!    shots that end after one batch beside shots that run a second.
//!
//! The kit runs programs of its own, which use `x`, `h`, `mz` and `reset`,
//! and whose expected records hold on any correct quantum state: so a
//! resumer runs at least those calls. Its checks of rules 1, 4 and 7, and
//! some of those of rules 3, 6 and 8, need random outcomes (`h`, `mz`)
//! that differ among shots and come out both ways; a resumer whose shots
//! draw the same outcomes passes them without exercising them. A resumer
//! that seeds each shot by its id, as rule 7 needs, draws them.

mod programs;

use std::any::Any;
use std::fmt::{self, Debug, Write};
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use super::super::{QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, QirTrap};
use crate::{MeasurementResult, OutputRecord};

/// Checks every rule of [`QirResumer::run`] but the one on the resumer's
/// own failures, on resumers `make` makes, a fresh one per run.
///
/// # Panics
///
/// When the resumers break a rule, naming each rule broken and how.
pub fn check<R: QirResumer>(mut make: impl FnMut() -> R)
where
    R::Error: Debug,
{
    report(&faults(&mut |_: &[QirShotId]| make(), false));
}

/// Checks every rule of [`QirResumer::run`], on resumers `make` makes, a
/// fresh one per run: `make(failing)` makes a resumer whose own work fails
/// ([`QirResumeError::Resumer`]) for each shot of `failing` at its first
/// batch, before running it, and for no other shot.
///
/// # Panics
///
/// When the resumers break a rule, naming each rule broken and how.
pub fn check_with_failing_shots<R: QirResumer>(mut make: impl FnMut(&[QirShotId]) -> R)
where
    R::Error: Debug,
{
    report(&faults(&mut make, true));
}

/// Panics listing `faults`, if any.
fn report(faults: &[Fault]) {
    if faults.is_empty() {
        return;
    }
    let mut message = format!("the resumer breaks {} rule(s):", faults.len());
    for fault in faults {
        write!(message, "\n- {}: {}", fault.rule, fault.detail).expect("a `String` takes any text");
    }
    panic!("{message}");
}

/// A rule of [`QirResumer::run`] that the kit checks, numbered as in the
/// module's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rule {
    ContextPerShot,
    BatchInOrder,
    OutcomesToTheirShot,
    RecordsAtTheirPlace,
    TrapNamesItsShot,
    FirstFailureInOrder,
    IndependentOfOtherShots,
    Edges,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ContextPerShot => "rule 1 (a context per shot)",
            Self::BatchInOrder => "rule 2 (a batch's calls run in order)",
            Self::OutcomesToTheirShot => {
                "rule 3 (one outcome per call that writes a result, by its result, to its own shot)"
            }
            Self::RecordsAtTheirPlace => "rule 4 (each shot's records at its place in `shots`)",
            Self::TrapNamesItsShot => "rule 5 (a trap stops its shot, and the error names it)",
            Self::FirstFailureInOrder => {
                "rule 6 (the first failing shot in the order of `shots` is reported)"
            }
            Self::IndependentOfOtherShots => {
                "rule 7 (a shot gives the same records alone as among other shots, \
                 and in a later run of the same resumer)"
            }
            Self::Edges => "rule 8 (the edges)",
        })
    }
}

/// A rule broken, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    pub(crate) rule: Rule,
    pub(crate) detail: String,
}

/// The faults of resumers `make` makes, every rule checked; the rule on
/// the resumer's own failures only when `failing` (the resumers can fail
/// on purpose, as [`check_with_failing_shots`] says).
pub(crate) fn faults<R: QirResumer>(
    make: &mut dyn FnMut(&[QirShotId]) -> R,
    failing: bool,
) -> Vec<Fault>
where
    R::Error: Debug,
{
    let mut kit = Kit {
        make,
        faults: Vec::new(),
    };
    kit.context_per_shot();
    kit.batch_in_order();
    kit.outcomes_to_their_shot();
    kit.records_at_their_place();
    kit.trap_names_its_shot();
    kit.first_failure_in_order(failing);
    kit.independent_of_other_shots();
    kit.edges();
    kit.faults
}

/// Distinct ids, out of order and not contiguous.
pub(crate) const SCRAMBLED: [u32; 8] = [9, 2, 14, 5, 11, 0, 7, 3];

/// The ids of [`SCRAMBLED`], in ascending order.
fn ascending() -> Vec<QirShotId> {
    let mut shots = ids(&SCRAMBLED);
    shots.sort();
    shots
}

fn ids(ids: &[u32]) -> Vec<QirShotId> {
    ids.iter().copied().map(QirShotId).collect()
}

type Records = Vec<Vec<OutputRecord>>;

/// What a run gave, as the checks read it.
enum Ran {
    Records(Records),
    Trapped { shot: QirShotId, trap: QirTrap },
    Failed { shot: QirShotId, error: String },
    Panicked(String),
}

impl fmt::Display for Ran {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Records(records) => write!(f, "the records {records:?}"),
            Self::Trapped { shot, trap } => write!(f, "{shot} trapped: {trap}"),
            Self::Failed { shot, error } => write!(f, "{shot} failed: {error}"),
            Self::Panicked(message) => write!(f, "a panic: {message}"),
        }
    }
}

/// The kit's state: what makes resumers, and the faults found so far.
struct Kit<'m, R> {
    make: &'m mut dyn FnMut(&[QirShotId]) -> R,
    faults: Vec<Fault>,
}

impl<R: QirResumer> Kit<'_, R>
where
    R::Error: Debug,
{
    /// Runs `shots` of `program` on a fresh resumer that fails for the
    /// shots `failing`, catching a panic.
    fn run(&mut self, program: &QirProgram, shots: &[QirShotId], failing: &[QirShotId]) -> Ran {
        Self::run_on(&mut (self.make)(failing), program, shots)
    }

    /// Runs `shots` of `program` on `resumer`, catching a panic.
    fn run_on(resumer: &mut R, program: &QirProgram, shots: &[QirShotId]) -> Ran {
        match catch_unwind(AssertUnwindSafe(|| {
            block_on(resumer.run(&QirMachine::new(program), shots))
        })) {
            Ok(Ok(records)) => Ran::Records(records),
            Ok(Err(QirResumeError::Trapped { shot, trap })) => Ran::Trapped { shot, trap },
            Ok(Err(QirResumeError::Resumer { shot, error })) => Ran::Failed {
                shot,
                error: format!("{error:?}"),
            },
            Err(panic) => Ran::Panicked(panic_message(&*panic)),
        }
    }

    fn fault(&mut self, rule: Rule, detail: String) {
        self.faults.push(Fault { rule, detail });
    }

    /// The records of a run of `shots`, one per shot; else, a fault of
    /// `rule` naming the run `what`.
    fn records(&mut self, rule: Rule, what: &str, ran: Ran, shots: usize) -> Option<Records> {
        match ran {
            Ran::Records(records) if records.len() == shots => Some(records),
            other => {
                self.fault(
                    rule,
                    format!("{what}: gave {other}, where the records of {shots} shots were due"),
                );
                None
            }
        }
    }

    /// Checks that each of `shots` of `program` records `expected`.
    fn fixed(
        &mut self,
        rule: Rule,
        what: &str,
        program: &QirProgram,
        shots: &[QirShotId],
        expected: &[OutputRecord],
    ) {
        let ran = self.run(program, shots, &[]);
        self.ran_fixed(rule, what, ran, shots, expected);
    }

    /// Checks that each of `shots` recorded `expected` in the run `ran`.
    fn ran_fixed(
        &mut self,
        rule: Rule,
        what: &str,
        ran: Ran,
        shots: &[QirShotId],
        expected: &[OutputRecord],
    ) {
        let Some(records) = self.records(rule, what, ran, shots.len()) else {
            return;
        };
        let wrong = shots
            .iter()
            .zip(&records)
            .filter(|(_, records)| records[..] != *expected)
            .map(|(shot, records)| format!("{shot} recorded {records:?}"))
            .collect::<Vec<_>>();
        if !wrong.is_empty() {
            self.fault(
                rule,
                format!("{what}: {}, where {expected:?} was due", wrong.join(", ")),
            );
        }
    }

    /// Checks that `program`'s run of `shots` fails as `expected` does: a
    /// trap or the resumer's own failure, of the shot it names.
    fn fails(
        &mut self,
        rule: Rule,
        what: &str,
        program: &QirProgram,
        (shots, failing): (&[QirShotId], &[QirShotId]),
        expected: &Ran,
    ) {
        let ran = self.run(program, shots, failing);
        let met = match (&ran, expected) {
            (Ran::Trapped { shot, trap }, Ran::Trapped { shot: s, trap: t }) => {
                shot == s && trap == t
            }
            (Ran::Failed { shot, .. }, Ran::Failed { shot: s, .. }) => shot == s,
            _ => false,
        };
        if !met {
            let expected = match expected {
                Ran::Failed { shot, .. } => format!("{shot} failed"),
                other => other.to_string(),
            };
            self.fault(
                rule,
                format!("{what}: gave {ran}, where {expected} was due"),
            );
        }
    }

    /// Rule 1: shots that draw random outcomes do not all give the same
    /// records.
    fn context_per_shot(&mut self) {
        let what = "16 shots of `h`, `mz` on 4 qubits";
        let shots = QirShotId::first(16);
        let ran = self.run(&programs::random(), &shots, &[]);
        if let Some(records) = self.records(Rule::ContextPerShot, what, ran, shots.len())
            && records.windows(2).all(|pair| pair[0] == pair[1])
        {
            self.fault(
                Rule::ContextPerShot,
                format!(
                    "{what}: every shot recorded {:?}, as from one context",
                    records[0]
                ),
            );
        }
    }

    /// Rule 2: `x`, `mz`, `reset`, `mz` in one batch.
    fn batch_in_order(&mut self) {
        self.fixed(
            Rule::BatchInOrder,
            "`x`, `mz`, `reset`, `mz` in one batch",
            &programs::in_order(),
            &ids(&[4, 1, 6]),
            &[result(true), result(false)],
        );
    }

    /// Rule 3: outcomes into results other than their calls' places, and a
    /// random outcome that must agree with itself measured again and
    /// copied through a branch.
    fn outcomes_to_their_shot(&mut self) {
        let rule = Rule::OutcomesToTheirShot;
        self.fixed(
            rule,
            "`x`, `mz` into result 3, then `mz` of qubits 0 and 1 into results 1 and 0",
            &programs::results_out_of_place(),
            &ids(&[2, 0, 5]),
            &[result(false), result(true), result(true)],
        );
        let what = "16 shots of `h`, `mz`, the outcome measured again and copied through a branch";
        let shots = QirShotId::first(16);
        let ran = self.run(&programs::consistent(), &shots, &[]);
        let Some(records) = self.records(rule, what, ran, shots.len()) else {
            return;
        };
        let wrong = shots
            .iter()
            .zip(&records)
            .filter(|(_, records)| {
                records.len() != 3 || records.iter().any(|record| *record != records[0])
            })
            .map(|(shot, records)| format!("{shot} recorded {records:?}"))
            .collect::<Vec<_>>();
        if !wrong.is_empty() {
            self.fault(
                rule,
                format!(
                    "{what}: {}, where three equal results were due",
                    wrong.join(", ")
                ),
            );
        }
    }

    /// Rule 4: random records of shots listed out of order, each the same
    /// as the shot's in ascending order.
    fn records_at_their_place(&mut self) {
        let rule = Rule::RecordsAtTheirPlace;
        let program = programs::random();
        let scrambled = ids(&SCRAMBLED);
        let ascending = ascending();
        let what = format!("shots {SCRAMBLED:?} of `h`, `mz` on 4 qubits");
        let ran = self.run(&program, &scrambled, &[]);
        let Some(listed) = self.records(rule, &what, ran, scrambled.len()) else {
            return;
        };
        let ran = self.run(&program, &ascending, &[]);
        let Some(sorted) = self.records(rule, "the same shots in ascending order", ran, 8) else {
            return;
        };
        let wrong = scrambled
            .iter()
            .zip(&listed)
            .filter_map(|(shot, records)| {
                let place = ascending.binary_search(shot).expect("the same shots");
                (*records != sorted[place]).then(|| {
                    format!(
                        "{shot} recorded {records:?} at its place, and {:?} in ascending order",
                        sorted[place]
                    )
                })
            })
            .collect::<Vec<_>>();
        if !wrong.is_empty() {
            self.fault(rule, format!("{what}: {}", wrong.join(", ")));
        }
    }

    /// Rule 5: shot 5 traps at once, and after a batch.
    fn trap_names_its_shot(&mut self) {
        let shot = [QirShotId(5)];
        for (what, program) in [
            (
                "shot 5 trapping before any quantum call",
                programs::traps_at_once(),
            ),
            (
                "shot 5 trapping after a batch",
                programs::traps_after_a_batch(),
            ),
        ] {
            let (program, trap) = program;
            self.fails(
                Rule::TrapNamesItsShot,
                what,
                &program,
                (&shot, &[]),
                &Ran::Trapped {
                    shot: shot[0],
                    trap,
                },
            );
        }
    }

    /// Rule 6: shots listed out of order, all trapping; some trapping, as
    /// each does alone; and, when `failing`, the resumer failing for some.
    fn first_failure_in_order(&mut self, failing: bool) {
        let rule = Rule::FirstFailureInOrder;
        let shots = ids(&SCRAMBLED);
        let (program, trap) = programs::traps_after_a_batch();
        self.fails(
            rule,
            &format!("shots {SCRAMBLED:?}, each trapping after a batch"),
            &program,
            (&shots, &[]),
            &Ran::Trapped {
                shot: shots[0],
                trap,
            },
        );
        self.some_trapping(&shots);
        if !failing {
            return;
        }
        let (program, trap) = programs::traps_after_two_batches();
        self.fails(
            rule,
            &format!(
                "shots {SCRAMBLED:?}, each trapping after two batches, the resumer failing for {}",
                shots[2]
            ),
            &program,
            (&shots, &shots[2..3]),
            &Ran::Trapped {
                shot: shots[0],
                trap,
            },
        );
        self.fails(
            rule,
            &format!(
                "shots {SCRAMBLED:?}, each trapping after two batches, the resumer failing for {}",
                shots[0]
            ),
            &program,
            (&shots, &shots[..1]),
            &Ran::Failed {
                shot: shots[0],
                error: String::new(),
            },
        );
        self.fails(
            rule,
            &format!(
                "shots {SCRAMBLED:?}, the resumer failing for {} and {}",
                shots[5], shots[1]
            ),
            &programs::in_order(),
            (&shots, &[shots[5], shots[1]]),
            &Ran::Failed {
                shot: shots[1],
                error: String::new(),
            },
        );
    }

    /// Rule 6: shots that trap or not on a random outcome, together as each
    /// alone, in the order of `shots`; then shots that trap in their first
    /// round or their second on one, those that trap in their second
    /// listed first: the first trapping one in the order of the run
    /// reported, even when a shot listed after it traps in an earlier
    /// round.
    fn some_trapping(&mut self, shots: &[QirShotId]) {
        let how = "trapping on the outcome `One`";
        let program = programs::traps_on_one();
        if let Some(alone) = self.alone(&program, shots, how) {
            let order: Vec<_> = shots.iter().copied().zip(&alone).collect();
            self.together(&program, &order, how);
        }
        let how = "trapping at once on the outcome `One`, after a second batch on `Zero`";
        let (program, late) = programs::traps_sooner_or_later();
        if let Some(alone) = self.alone(&program, shots, how) {
            let (mut order, early): (Vec<_>, Vec<_>) = shots
                .iter()
                .copied()
                .zip(&alone)
                .partition(|(_, alone)| alone.as_ref().err() == Some(&late));
            order.extend(early);
            self.together(&program, &order, how);
        }
    }

    /// The records or the trap of each of `shots` of `program`, run alone;
    /// else, a fault saying how a shot `how` failed otherwise.
    fn alone(
        &mut self,
        program: &QirProgram,
        shots: &[QirShotId],
        how: &str,
    ) -> Option<Vec<Result<Vec<OutputRecord>, QirTrap>>> {
        let mut alone = Vec::new();
        for &shot in shots {
            match self.run(program, &[shot], &[]) {
                Ran::Records(mut records) if records.len() == 1 => {
                    alone.push(Ok(records.remove(0)));
                }
                Ran::Trapped { shot: named, trap } if named == shot => alone.push(Err(trap)),
                other => {
                    self.fault(
                        Rule::FirstFailureInOrder,
                        format!(
                            "{shot} alone, {how}: gave {other}, where its records or its trap \
                             were due"
                        ),
                    );
                    return None;
                }
            }
        }
        Some(alone)
    }

    /// Checks that the shots of `order`, each with what it gave alone, run
    /// together give the trap of the first that trapped, or else every
    /// shot's records.
    fn together(
        &mut self,
        program: &QirProgram,
        order: &[(QirShotId, &Result<Vec<OutputRecord>, QirTrap>)],
        how: &str,
    ) {
        let rule = Rule::FirstFailureInOrder;
        let shots: Vec<QirShotId> = order.iter().map(|&(shot, _)| shot).collect();
        let what = format!(
            "shots {:?}, each {how} as it does alone",
            shots.iter().map(|shot| shot.0).collect::<Vec<_>>()
        );
        let first_trap = order
            .iter()
            .find_map(|(shot, alone)| alone.as_ref().err().map(|trap| (*shot, trap.clone())));
        if let Some((shot, trap)) = first_trap {
            self.fails(
                rule,
                &what,
                program,
                (&shots, &[]),
                &Ran::Trapped { shot, trap },
            );
        } else {
            let expected = order
                .iter()
                .map(|(_, alone)| (*alone).clone().expect("no shot trapped"))
                .collect::<Records>();
            match self.run(program, &shots, &[]) {
                Ran::Records(records) if records == expected => {}
                other => self.fault(
                    rule,
                    format!("{what}: gave {other}, where {expected:?} was due"),
                ),
            }
        }
    }

    /// Rule 7: random records of shots in ascending order, each the same
    /// as the shot's alone. On one resumer, `x` and `mz` of one qubit run
    /// twice on those shots, then each shot alone of `h` and `mz` on 4
    /// qubits: each run gives what a fresh resumer gives, so that no shot
    /// starts from the state an earlier run of its id left, nor runs the
    /// program of an earlier run.
    fn independent_of_other_shots(&mut self) {
        let rule = Rule::IndependentOfOtherShots;
        let random = programs::random();
        let flips = programs::flips_a_qubit();
        let shots = ascending();
        let what = format!("shots {shots:?} of `h`, `mz` on 4 qubits, in ascending order");
        let ran = self.run(&random, &shots, &[]);
        let Some(together) = self.records(rule, &what, ran, shots.len()) else {
            return;
        };
        let mut resumer = (self.make)(&[]);
        for runs in 0..2 {
            let ran = Self::run_on(&mut resumer, &flips, &shots);
            let what = format!("shots {shots:?} of `x`, `mz` after {runs} runs of their resumer");
            self.ran_fixed(rule, &what, ran, &shots, &[result(true)]);
        }
        let mut wrong = Vec::new();
        for (runs, (&shot, together)) in (2..).zip(shots.iter().zip(&together)) {
            let ran = Self::run_on(&mut resumer, &random, &[shot]);
            let what = format!("{shot} alone, after {runs} runs of its resumer");
            let Some(alone) = self.records(rule, &what, ran, 1) else {
                continue;
            };
            if alone[0] != *together {
                wrong.push(format!(
                    "{shot} recorded {together:?}, and {:?} alone after {runs} runs of its \
                     resumer",
                    alone[0]
                ));
            }
        }
        if !wrong.is_empty() {
            self.fault(rule, format!("{what}: {}", wrong.join(", ")));
        }
    }

    /// Rule 8: no shots, one shot, a program with no quantum call, and
    /// shots that end after one batch beside shots that run a second.
    fn edges(&mut self) {
        let rule = Rule::Edges;
        let in_order = programs::in_order();
        let ran = self.run(&in_order, &[], &[]);
        self.records(rule, "no shots", ran, 0);
        self.fixed(
            rule,
            "one shot",
            &in_order,
            &[QirShotId(7)],
            &[result(true), result(false)],
        );
        self.fixed(
            rule,
            "a program with no quantum call",
            &programs::no_quantum_call(),
            &ids(&[3, 1]),
            &[OutputRecord::Int(42)],
        );
        let what = "16 shots of `h`, `mz`, ending when `Zero`, else measuring again";
        let shots = QirShotId::first(16);
        let ran = self.run(&programs::ends_or_continues(), &shots, &[]);
        let Some(records) = self.records(rule, what, ran, shots.len()) else {
            return;
        };
        let wrong = shots
            .iter()
            .zip(&records)
            .filter(|(_, records)| {
                records[..] != [result(false)] && records[..] != [result(true), result(true)]
            })
            .map(|(shot, records)| format!("{shot} recorded {records:?}"))
            .collect::<Vec<_>>();
        if !wrong.is_empty() {
            self.fault(
                rule,
                format!(
                    "{what}: {}, where [Zero] or [One, One] was due",
                    wrong.join(", ")
                ),
            );
        }
    }
}

/// The record of a result: `One` when `one`, else `Zero`.
fn result(one: bool) -> OutputRecord {
    OutputRecord::Result(if one {
        MeasurementResult::One
    } else {
        MeasurementResult::Zero
    })
}

/// The message of a panic's payload.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "with a payload other than a message".to_string())
}

/// Wakes the thread that waits on a future.
struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// The output of `future`, the thread parked while it is pending.
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}
