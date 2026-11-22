// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Crossing a boundary, wherever the brackets go: a refusal or a
//! panic at a boundary inside a composition ends the run as it does at
//! the executor's; a checked batch is checked at every boundary it
//! crosses, beside unchecked ones of its `execute`; an `execute` of no
//! batches does not reach the receiver; a boundary calls its receiver
//! only once its own `execute` is polled; and it shapes the readouts a
//! caller passes: each batch's to its batch, with the readout slots its
//! sender stated, prefilled, whatever they held, before the executor
//! fills them; again when an adapter sends an `execute` again after a
//! failure; a panic in the caller's `execute`, once polled, when they are
//! not one per batch; and, of unchecked batches, into readouts that held
//! readouts as large, no allocation. The checks of one batch are in
//! `capabilities/boundary/batch_limits.rs`.

use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use smallvec::smallvec;

use super::runner::{SEED, failed, runner};
use crate::prelude::*;
use crate::tests::allocations;
use crate::tests::fake_parts::{
    Bits, ForwardsParity, Log, Misbehaving, Misbehaviour, NamesBelow, ParityGates, Profiler,
    Program, Renamed, coin_gate, coin_gates, in_each_bracketing, qubits,
};
use crate::{
    Argument, Batch, BatchFault, BatchReadouts, Capabilities, CompositionFaults, DriveError,
    ExecuteError, ExecuteRefusal, Failure, Features, Instruction, InstructionFault, InstructionSet,
    NotReady, OpcodeMap, PartId, ReadoutValue, Readouts, Runner, Seed, ShotConditions, ShotId,
    ShotSet,
};

use ReadoutValue::{One, Zero};

/// A run of a chain in one bracketing: how it failed, what its parts
/// logged, and how many instructions its profiler counted.
struct Failed {
    brackets: &'static str,
    error: DriveError,
    log: Log,
    counted: Rc<Cell<usize>>,
}

/// The failures of the run of `shots` on the chain `parts` makes, with the
/// adapter bracketed with the driver, then with the executor; each
/// bracketing gets new parts, a new log and a new count.
fn failed_in_both_brackets<D: Driver<Output: std::fmt::Debug>, A: Adapter, E: Executor>(
    shots: &ShotSet,
    parts: impl Fn(&Log, &Rc<Cell<usize>>) -> (D, A, E),
) -> [Failed; 2] {
    let new = || (Log::default(), Rc::new(Cell::new(0)));
    let (log, counted) = new();
    let (driver, adapter, executor) = parts(&log, &counted);
    let left = Failed {
        brackets: "the adapter with the driver",
        error: failed_run(driver.through(adapter).on(executor), shots),
        log,
        counted,
    };
    let (log, counted) = new();
    let (driver, adapter, executor) = parts(&log, &counted);
    let right = Failed {
        brackets: "the adapter with the executor",
        error: failed_run(driver.on(adapter.on(executor)), shots),
        log,
        counted,
    };
    [left, right]
}

/// The failure of the run of `shots` on the chain `composed` makes.
fn failed_run<D: Driver<Output: std::fmt::Debug>, E: Executor>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> DriveError {
    failed(runner(composed).run(shots, SEED))
}

/// Measures qubit 0 after flipping it: one checked batch of two
/// instructions, needing one qubit.
fn flip_and_measure() -> Program {
    Program::new(&[("x", 0), ("mz", 0)]).with_checked_batches()
}

/// The failure of the run of `shots` on the chain `parts` makes, a driver
/// over an executor: a [`Failed`] with no adapter, so with no profiler,
/// whose count stays 0.
fn failed_with_no_adapter<D: Driver<Output: std::fmt::Debug>, E: Executor>(
    shots: &ShotSet,
    parts: impl Fn(&Log) -> (D, E),
) -> [Failed; 1] {
    let log = Log::default();
    let (driver, executor) = parts(&log);
    [Failed {
        brackets: "no adapter",
        error: failed_run(driver.on(executor), shots),
        log,
        counted: Rc::new(Cell::new(0)),
    }]
}

/// Checks the row `row` of
/// [`a_batch_the_part_below_does_not_take_is_refused_before_it_runs`]:
/// each of `runs` failed with `refusal`, naming its shot, and before any
/// profiler counted an instruction or the bits ran one.
fn refused_before_it_runs(row: &str, runs: &[Failed], refusal: &ExecuteRefusal) {
    for run in runs {
        let Failed {
            brackets, error, ..
        } = run;
        let Failure::Refused(got) = &error.failure else {
            panic!("{row}, {brackets}: a refusal, not {:?}", error.failure);
        };
        assert_eq!(got, refusal, "{row}, {brackets}");
        assert_eq!(error.shot, refusal.shot(), "{row}, {brackets}");
        assert_eq!(run.counted.get(), 0, "{row}, {brackets}: no count");
        assert_eq!(
            run.log.count("Bits execute"),
            0,
            "{row}, {brackets}: nothing ran"
        );
    }
}

/// Reads qubit 0, flips it if the readout parity of that read is odd,
/// and reads it again, in [`ParityGates`], written as struct literals, as
/// a part writes them on its hot path, in one checked batch, requiring
/// one qubit and readout parities.
fn flip_on_a_parity() -> Program {
    let on_qubit_0 = |opcode, arguments| Instruction {
        opcode,
        blocks: smallvec![0],
        arguments,
        select: None,
    };
    let mz = on_qubit_0(ParityGates::MZ, smallvec![]);
    let x_if = on_qubit_0(ParityGates::X_IF, smallvec![Argument::recent(1)]);
    Program::sending(vec![mz.clone(), x_if, mz], ParityGates::set())
        .with_resources(qubits(1))
        .with_features(Features::unconstrained().with_readout_parity(true))
        .with_checked_batches()
}

/// Each boundary refuses a checked batch its receiver does not take,
/// typed and naming the batch's first shot, before the receiver runs any
/// of it or of the other batches of its `execute`, whoever sends it: the
/// driver, to the executor or to an adapter, or an adapter, in both
/// bracketings. A row per sender, and one where an adapter's requirement
/// differs from the driver's, as the requirement its batches are checked
/// against; the faults a batch may hold are in
/// `capabilities/boundary/batch_limits.rs`.
#[test]
fn a_batch_the_part_below_does_not_take_is_refused_before_it_runs() {
    let beyond_the_required = |shot, at, block, required| ExecuteRefusal::Batch {
        shot: ShotId::new(shot),
        batch: 0,
        fault: BatchFault::Instruction {
            at,
            fault: InstructionFault::BlockBeyondRequired {
                mnemonic: "x".to_owned(),
                block,
                required,
            },
        },
    };
    let parity_refused = ExecuteRefusal::Batch {
        shot: ShotId::new(5),
        batch: 0,
        fault: BatchFault::Instruction {
            at: 1,
            fault: InstructionFault::ParityNotRequired {
                mnemonic: "x_if".to_owned(),
                parameter: "c".to_owned(),
            },
        },
    };
    let rows: [(&str, Vec<Failed>, ExecuteRefusal); 4] = [
        // An instruction on a qubit beyond the program's qubits.
        (
            "the driver's instruction beyond its qubits, to the executor",
            failed_with_no_adapter(&ShotSet::ids([4, 2]), |log| {
                (
                    Program::new(&[("mz", 0), ("x", 5)])
                        .with_resources(qubits(2))
                        .with_checked_batches(),
                    Bits::new(log),
                )
            })
            .into(),
            beyond_the_required(4, 1, 5, 2),
        ),
        // A driver flipping qubit 1 while requiring one qubit, through a
        // profiler: refused at the driver's boundary, before the profiler
        // counts any.
        (
            "the driver's instruction beyond its qubits, to an adapter",
            failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, counted| {
                (
                    Program::new(&[("mz", 0), ("x", 1)])
                        .with_resources(qubits(1))
                        .with_checked_batches(),
                    Profiler::new(counted, log),
                    Bits::new(log),
                )
            })
            .into(),
            beyond_the_required(5, 1, 1, 1),
        ),
        // A driver flipping and measuring qubit 0, under an adapter that
        // adds an `x` on qubit 99 while forwarding the driver's one qubit,
        // which it does not state: the adapter's batch is refused, naming
        // its third instruction.
        (
            "an adapter's instruction beyond the qubits it forwards",
            failed_in_both_brackets(&ShotSet::ids([4, 2]), |log, _| {
                (
                    flip_and_measure(),
                    Misbehaving::new(Misbehaviour::SendsBeyondItsQubits),
                    Bits::new(log),
                )
            })
            .into(),
            beyond_the_required(4, 2, 99, 1),
        ),
        // A driver requiring readout parities, above an adapter that says
        // it resolves them, so that it requires none below, but passes
        // them down, over an executor that resolves none: the adapter's
        // batch is refused.
        (
            "an adapter's readout parity it does not require",
            failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, _| {
                (
                    flip_on_a_parity(),
                    ForwardsParity::on(ParityGates::set(), log),
                    Bits::accepting(ParityGates::set(), log),
                )
            })
            .into(),
            parity_refused,
        ),
    ];
    for (row, runs, refusal) in &rows {
        refused_before_it_runs(row, runs, refusal);
    }
}

/// A driver requiring readout parities, through two profilers, over an
/// executor that resolves them, in each of the five bracketings of four
/// parts: every boundary passes the parity, and the executor runs it.
#[test]
fn a_required_readout_parity_crosses_every_boundary() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = flip_on_a_parity();
            let a = Profiler::on(ParityGates::set(), &counted, &log);
            let b = Profiler::on(ParityGates::set(), &counted, &log);
            let e = Bits::accepting(ParityGates::set(), &log).resolving_parity();
        },
        |chain, brackets| {
            runner(chain)
                .run(&ShotSet::ids([5, 3]), SEED)
                .unwrap_or_else(|error| panic!("{brackets}: {error:?}"));
            assert_eq!(log.count("Bits execute"), 1, "{brackets}");
        }
    );
}

/// A driver flipping and measuring qubit 0 in `CoinGates`, over bits
/// whose `provides` answers [`ParityGates`] once ready, in which `x` is
/// `x_if` and `mz` is past the set's end, in a checked batch, directly and
/// through an adapter in both bracketings: each run checks the batch
/// against the set the `on` that makes the runner read, so it runs and
/// reads 1, and the adapter, reading the set of the part below, is told
/// that set too.
#[test]
fn every_batch_is_checked_against_the_set_read_at_composition() {
    let log = Log::default();
    let names_below = || NamesBelow::new(&log);
    let bits = || Bits::new(&log).accepting_once_ready(ParityGates::set());
    let shots = ShotSet::ids([5, 3]);
    let runs = [
        runner(flip_and_measure().on(bits())).run(&shots, SEED),
        runner(flip_and_measure().through(names_below()).on(bits())).run(&shots, SEED),
        runner(flip_and_measure().on(names_below().on(bits()))).run(&shots, SEED),
    ];
    for (i, run) in runs.into_iter().enumerate() {
        let records = run.unwrap_or_else(|error| panic!("bracketing {i}: {error:?}"));
        let ones = (vec![ReadoutValue::One], ShotConditions::NONE);
        assert_eq!(records, [ones.clone(), ones], "bracketing {i}");
    }
    assert_eq!(log.count("Bits execute"), 3, "each bracketing ran it");
    let told: Vec<_> = log
        .entries()
        .into_iter()
        .filter(|entry| entry.starts_with("below accepts: "))
        .collect();
    let read_at_composition = format!("below accepts: {}", coin_gates().name());
    assert_eq!(told, [read_at_composition.clone(), read_at_composition]);
}

/// An adapter that reads the name of the part below it, over an executor
/// or over an adapter, each named as a part built at run time is, in
/// every bracketing: it reads that part's name, though the protocol
/// shapes the readouts of the `execute`s it sends.
#[test]
fn an_adapter_reads_the_name_of_the_part_below() {
    let read = |log: &Log| {
        log.entries()
            .into_iter()
            .filter(|entry| entry.starts_with("below: "))
            .collect::<Vec<_>>()
    };
    let shots = ShotSet::ids([3, 1]);
    in_each_bracketing!(
        [d, n, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]);
            let n = NamesBelow::new(&log);
            let e = Renamed::new(Bits::new(&log), "qodec runtime");
        },
        |chain, brackets| {
            runner(chain).run(&shots, SEED).expect("a run");
            assert_eq!(read(&log), ["below: qodec runtime"], "{brackets}");
        }
    );
    in_each_bracketing!(
        [d, n, p, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = Program::new(&[("mz", 0)]);
            let n = NamesBelow::new(&log);
            let p = Renamed::new(Profiler::new(&counted, &log), "profiler");
            let e = Renamed::new(Bits::new(&log), "qodec runtime");
        },
        |chain, brackets| {
            runner(chain).run(&shots, SEED).expect("a run");
            assert_eq!(read(&log), ["below: profiler"], "{brackets}");
        }
    );
}

/// An adapter of a chain whose one faulty part sends an instruction
/// beyond the qubits required: a profiler, or, if it is that part, an
/// adapter adding an `x` on qubit 99 beyond the one qubit it forwards.
enum Stage {
    Counts(Profiler),
    Faulty(Misbehaving),
}

impl Adapter for Stage {
    fn accepts(&self) -> &InstructionSet {
        match self {
            Self::Counts(profiler) => Adapter::accepts(profiler),
            Self::Faulty(faulty) => Adapter::accepts(faulty),
        }
    }

    fn sends(&self) -> &InstructionSet {
        match self {
            Self::Counts(profiler) => profiler.sends(),
            Self::Faulty(faulty) => faulty.sends(),
        }
    }

    fn requires(&self, above: &Capabilities) -> Capabilities {
        match self {
            Self::Counts(profiler) => Adapter::requires(profiler, above),
            Self::Faulty(faulty) => Adapter::requires(faulty, above),
        }
    }

    fn bind(&mut self, below: OpcodeMap) {
        match self {
            Self::Counts(profiler) => profiler.bind(below),
            Self::Faulty(faulty) => faulty.bind(below),
        }
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        match self {
            Self::Counts(profiler) => profiler.adapt(batches, seed, below, readouts).await,
            Self::Faulty(faulty) => faulty.adapt(batches, seed, below, readouts).await,
        }
    }
}

/// The parts of a chain of a driver, up to two adapters and bits, of
/// which one sends an instruction beyond the qubits required: the driver,
/// an `x` on qubit 99 while requiring two qubits, if `faulty` is 0, or the
/// adapter at that position. The driver's batches are checked if
/// `checked`, and each adapter's are if the batch it received is. Each
/// other adapter counts the instructions it receives, the first into
/// `counted[0]`.
struct Parts {
    faulty: usize,
    checked: bool,
    counted: [Rc<Cell<usize>>; 2],
    log: Log,
}

impl Parts {
    fn new(faulty: usize, checked: bool) -> Self {
        Self {
            faulty,
            checked,
            counted: [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))],
            log: Log::default(),
        }
    }

    fn d(&self) -> Program {
        let program = if self.faulty == 0 {
            Program::new(&[("mz", 0), ("x", 99)]).with_resources(qubits(2))
        } else {
            Program::new(&[("x", 0), ("mz", 0)])
        };
        if self.checked {
            program.with_checked_batches()
        } else {
            program
        }
    }

    /// The adapter at `position`, 1 or 2 from the top.
    fn p(&self, position: usize) -> Stage {
        if self.faulty == position {
            Stage::Faulty(Misbehaving::new(Misbehaviour::SendsBeyondItsQubits))
        } else {
            Stage::Counts(Profiler::new(&self.counted[position - 1], &self.log))
        }
    }

    fn e(&self) -> Bits {
        Bits::new(&self.log)
    }

    /// What the part below the faulty one received: the instructions the
    /// adapter below counted, or the bits' `execute`s.
    fn received_below(&self, adapters: usize) -> usize {
        if self.faulty < adapters {
            self.counted[self.faulty].get()
        } else {
            self.log.count("Bits execute")
        }
    }
}

/// The chain `chain` makes, of `adapters` adapters, with each of its
/// parts above a boundary faulty in turn: if the driver's batches are
/// checked, the faulty batch is refused, and the part below receives
/// nothing; if not, as by default, it reaches the bits, which panic on
/// it.
fn each_boundary_checks_its_batch<D: Driver<Output: std::fmt::Debug>, E: Executor>(
    brackets: &str,
    adapters: usize,
    chain: impl Fn(&Parts) -> Result<Runner<D, E>, CompositionFaults>,
) {
    let shots = ShotSet::ids(0..2);
    for faulty in 0..=adapters {
        let parts = Parts::new(faulty, true);
        let error = failed(runner(chain(&parts)).run(&shots, SEED));
        assert!(
            matches!(
                error.failure,
                Failure::Refused(ExecuteRefusal::Batch {
                    fault: BatchFault::Instruction {
                        fault: InstructionFault::BlockBeyondRequired { .. },
                        ..
                    },
                    ..
                })
            ),
            "{brackets}, part {faulty} faulty, checked: {error:?}"
        );
        assert_eq!(
            parts.received_below(adapters),
            0,
            "{brackets}, part {faulty} faulty, checked: the part below received nothing"
        );
        let parts = Parts::new(faulty, false);
        let error = failed(runner(chain(&parts)).run(&shots, SEED));
        let bits = PartId::new(adapters + 1, type_name::<Bits>());
        assert!(
            matches!(&error.failure, Failure::Panicked { part, .. } if *part == bits),
            "{brackets}, part {faulty} faulty, unchecked: {error:?}"
        );
        assert!(
            parts.received_below(adapters) > 0,
            "{brackets}, part {faulty} faulty, unchecked: the part below received it"
        );
    }
}

/// A driver, two adapters at most and bits, in every bracketing, with each
/// part above a boundary sending an instruction beyond its qubits in turn:
/// if the driver's batches are checked, the adapters' are too, and the
/// faulty batch is refused at the boundary below that part, wherever the
/// brackets go; batches are unchecked by default.
#[test]
fn a_checked_batch_is_checked_at_every_boundary_wherever_the_brackets_go() {
    each_boundary_checks_its_batch("d.on(e)", 0, |c| c.d().on(c.e()));
    each_boundary_checks_its_batch("d.through(p).on(e)", 1, |c| c.d().through(c.p(1)).on(c.e()));
    each_boundary_checks_its_batch("d.on(p.on(e))", 1, |c| c.d().on(c.p(1).on(c.e())));
    each_boundary_checks_its_batch("d.through(p).through(p).on(e)", 2, |c| {
        c.d().through(c.p(1)).through(c.p(2)).on(c.e())
    });
    each_boundary_checks_its_batch("d.on(p.on(p.on(e)))", 2, |c| {
        c.d().on(c.p(1).on(c.p(2).on(c.e())))
    });
    each_boundary_checks_its_batch("d.through(p.through(p)).on(e)", 2, |c| {
        c.d().through(c.p(1).through(c.p(2))).on(c.e())
    });
    each_boundary_checks_its_batch("d.on(p.through(p).on(e))", 2, |c| {
        c.d().on(c.p(1).through(c.p(2)).on(c.e()))
    });
    each_boundary_checks_its_batch("d.through(p).on(p.on(e))", 2, |c| {
        c.d().through(c.p(1)).on(c.p(2).on(c.e()))
    });
}

/// Two batches of one `execute`, on shots 4 and 2 a measure of qubit 0,
/// on shots 9 and 7 a flip of qubit 99 beyond the program's 2 qubits and
/// the bits' 64, one
/// checked and the other not: each batch is checked as its sender chose.
/// Checked, the second batch is refused, naming batch 1 and its first
/// shot, 9; unchecked, after a checked first batch, it reaches the bits,
/// which panic on it.
#[test]
fn each_batch_of_an_execute_is_checked_as_its_sender_chose() {
    let shots = ShotSet::ids([4, 2, 9, 7]);
    let log = Log::default();
    let run = |first, second| {
        runner(
            (Program::halves(&[("mz", 0)], &[("x", 99)])
                .with_resources(qubits(2))
                .with_checked_halves(first, second))
            .on(Bits::new(&log)),
        )
        .run(&shots, SEED)
    };
    let error = failed(run(false, true));
    assert!(
        matches!(
            error.failure,
            Failure::Refused(ExecuteRefusal::Batch {
                shot,
                batch: 1,
                fault: BatchFault::Instruction {
                    at: 0,
                    fault: InstructionFault::BlockBeyondRequired { .. },
                },
            }) if shot == ShotId::new(9)
        ),
        "the checked second batch: {error:?}"
    );
    let error = failed(run(true, false));
    let bits = PartId::new(1, type_name::<Bits>());
    assert!(
        matches!(&error.failure, Failure::Panicked { part, .. } if *part == bits),
        "the unchecked second batch: {error:?}"
    );
}

/// A driver making one `execute` of no batches, with no readouts.
struct SendsNothing;

impl Driver for SendsNothing {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<()>, DriveError> {
        let mut kept = Readouts::new();
        below.execute(&[], seed, kept.readouts_for(&[])).await?;
        Ok(vec![(); shots.len()])
    }
}

/// A driver making one `execute` of no batches, directly over an executor
/// that counts its calls, or through an adapter that counts its own, in
/// each bracketing: the run succeeds and no part below is called, so an
/// executor never has to answer an `execute` of no batches.
#[test]
fn an_execute_of_no_batches_reaches_no_part_below() {
    let executed = Rc::new(Cell::new(0));
    runner(SendsNothing.on(AnswersWhenCalled(Rc::clone(&executed))))
        .run(&ShotSet::ids(0..2), SEED)
        .expect("the run succeeds");
    assert_eq!(executed.get(), 0, "no adapter");

    let adapted = Rc::new(Cell::new(0));
    in_each_bracketing!([d, a, e] from {
        let d = SendsNothing;
        let a = ForwardsWhenCalled(Rc::clone(&adapted));
        let e = AnswersWhenCalled(Rc::clone(&executed));
    }, |chain, brackets| {
        runner(chain)
            .run(&ShotSet::ids(0..2), SEED)
            .expect("the run succeeds");
        assert_eq!((adapted.get(), executed.get()), (0, 0), "{brackets}");
    });
}

/// A driver that makes one `execute` of an empty batch below, and awaits
/// it, or drops it before its first poll.
struct MakesAnExecute {
    awaits: bool,
}

impl Driver for MakesAnExecute {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<()>, DriveError> {
        let batches = [Batch::new(shots, &[], 0)];
        let mut kept = Readouts::new();
        let execute = below.execute(&batches, seed, kept.readouts_for(&batches));
        if self.awaits {
            execute.await?;
        }
        Ok(vec![(); shots.len()])
    }
}

/// An executor whose `execute` answers when it is called, against the
/// rule that a future works only once polled, so that a call shows: it
/// counts its calls.
struct AnswersWhenCalled(Rc<Cell<usize>>);

impl Executor for AnswersWhenCalled {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.0.set(self.0.get() + 1);
        std::future::ready(Ok(()))
    }
}

/// An adapter whose `adapt` sends below when it is called, against the
/// rule that a future works only once polled, so that a call shows: it
/// counts its calls.
struct ForwardsWhenCalled(Rc<Cell<usize>>);

impl Adapter for ForwardsWhenCalled {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.0.set(self.0.get() + 1);
        below.execute(batches, seed, readouts)
    }
}

/// An `execute` that a driver makes and drops before its first poll calls
/// nothing below, which each boundary calls only once its own future is
/// polled, even when the part below works when called: the executor under
/// the runner's boundary, or an adapter and the executor below it, under
/// the composed driver's boundary or the runner's, by bracketing; awaited,
/// it calls each part below once.
#[test]
fn a_boundary_calls_its_receiver_only_once_its_execute_is_polled() {
    for (awaits, calls) in [(false, 0), (true, 1)] {
        let called = Rc::new(Cell::new(0));
        runner((MakesAnExecute { awaits }).on(AnswersWhenCalled(Rc::clone(&called))))
            .run(&ShotSet::ids(0..2), SEED)
            .expect("the run succeeds");
        assert_eq!(called.get(), calls, "awaits: {awaits}");

        let adapted = Rc::new(Cell::new(0));
        let executed = Rc::new(Cell::new(0));
        in_each_bracketing!([d, a, e] from {
            let d = MakesAnExecute { awaits };
            let a = ForwardsWhenCalled(Rc::clone(&adapted));
            let e = AnswersWhenCalled(Rc::clone(&executed));
        }, |chain, brackets| {
            adapted.set(0);
            executed.set(0);
            runner(chain)
                .run(&ShotSet::ids(0..2), SEED)
                .expect("the run succeeds");
            assert_eq!(
                (adapted.get(), executed.get()),
                (calls, calls),
                "awaits: {awaits}, {brackets}"
            );
        });
    }
}

/// A driver sending, on 3 shots, two batches in one `execute`: shots 0 and
/// 1 flip qubit 0 and read it twice, 2 readout slots; shot 2 flips it, no
/// readout slot. Its readouts held, before, 3 batches': 1 row of 1 value
/// `One`, then 4 rows of 3, each shot rejected, so that it passes readouts
/// of other shapes, smaller and larger, holding values and conditions. It
/// records the readouts it reads.
struct TwoBatches {
    instructions: [Vec<Instruction>; 2],
    /// The readout slots it states for each batch.
    stated: [u32; 2],
    passes: Passes,
    read: Rc<RefCell<Option<Vec<BatchReadouts>>>>,
}

/// What a [`TwoBatches`] passes below.
#[derive(Clone, Copy)]
enum Passes {
    /// Its two batches, with one `BatchReadouts` each.
    OnePerBatch,
    /// Its two batches, with one `BatchReadouts`.
    FewerThanBatches,
    /// No batch, with one `BatchReadouts`.
    WithoutBatches,
}

impl TwoBatches {
    fn new(read: &Rc<RefCell<Option<Vec<BatchReadouts>>>>) -> Self {
        Self {
            instructions: [
                vec![coin_gate("x", 0), coin_gate("mz", 0), coin_gate("mz", 0)],
                vec![coin_gate("x", 0)],
            ],
            stated: [2, 0],
            passes: Passes::OnePerBatch,
            read: Rc::clone(read),
        }
    }

    /// This driver, stating `stated` readout slots for its batches, unchecked.
    fn stating(mut self, stated: [u32; 2]) -> Self {
        self.stated = stated;
        self
    }

    /// This driver, passing below as `passes` says.
    fn passing(mut self, passes: Passes) -> Self {
        self.passes = passes;
        self
    }
}

/// Readouts that held, for a [`TwoBatches`], 3 batches' of values `One`,
/// each shot rejected: the first of 1 row of 1 value, smaller than the
/// first batch's, the others of 4 rows of 3, larger than either batch's.
fn readouts_of_other_shapes() -> Readouts {
    let mut readouts = Readouts::new();
    let shot = ShotSet::ids([0]);
    let three = [0; 3].map(|_| Batch::new(&shot, &[], 0));
    for (index, batch_readouts) in readouts.readouts_for(&three).iter_mut().enumerate() {
        let (rows, slots) = if index == 0 { (1, 1) } else { (4, 3) };
        *batch_readouts = BatchReadouts::new(rows, slots);
        for position in 0..rows {
            batch_readouts.row_mut(position).fill(One);
            batch_readouts.set_conditions(position, ShotConditions::REJECTED);
        }
    }
    readouts
}

impl Driver for TwoBatches {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.instructions.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<()>, DriveError> {
        let parts = [
            shots.select(&[0, 1]).expect("shots 0 and 1"),
            shots.select(&[2]).expect("shot 2"),
        ];
        let batches = [
            Batch::new(&parts[0], &self.instructions[0], self.stated[0]),
            Batch::new(&parts[1], &self.instructions[1], self.stated[1]),
        ];
        let mut kept = readouts_of_other_shapes();
        let readouts = kept.readouts_for(&batches);
        let (sent, passed): (&[Batch<'_>], &mut [BatchReadouts]) = match self.passes {
            Passes::OnePerBatch => (&batches, &mut readouts[..]),
            Passes::FewerThanBatches => (&batches, &mut readouts[..1]),
            Passes::WithoutBatches => (&[], &mut readouts[..1]),
        };
        below.execute(sent, seed, passed).await?;
        *self.read.borrow_mut() = Some(readouts.to_vec());
        Ok(vec![(); shots.len()])
    }
}

/// What a [`TwoBatches`] read on the chain `below` composes it into.
fn read_on<D: Driver, E: Executor>(
    below: impl FnOnce(TwoBatches) -> Result<Runner<D, E>, CompositionFaults>,
) -> Vec<BatchReadouts> {
    read_stating([2, 0], below)
}

/// What a [`TwoBatches`] stating `stated` readout slots read on the chain
/// `below` composes it into.
fn read_stating<D: Driver, E: Executor>(
    stated: [u32; 2],
    below: impl FnOnce(TwoBatches) -> Result<Runner<D, E>, CompositionFaults>,
) -> Vec<BatchReadouts> {
    let read = Rc::new(RefCell::new(None));
    runner(below(TwoBatches::new(&read).stating(stated)))
        .run(&ShotSet::ids(0..3), SEED)
        .expect("a run");
    read.take().expect("the driver read its readouts")
}

/// An executor that records the readouts it is passed, as it finds them,
/// then lets [`Bits`] fill them, or, if it has none, leaves them as they
/// are.
struct RecordsWhatItFinds(Option<Bits>, Rc<RefCell<Vec<BatchReadouts>>>);

impl Executor for RecordsWhatItFinds {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    async fn ready(&mut self, above: &Capabilities) -> Result<(), NotReady> {
        match &mut self.0 {
            Some(bits) => bits.ready(above).await,
            None => Ok(()),
        }
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        *self.1.borrow_mut() = readouts.to_vec();
        match &mut self.0 {
            Some(bits) => bits.execute(batches, seed, readouts).await,
            None => Ok(()),
        }
    }
}

/// A driver whose readouts held readouts of other shapes, with values and
/// conditions, sends two batches of different shapes in one `execute`, to
/// an executor, directly or below a profiler, which passes its readouts
/// down, in both bracketings: the executor finds each batch's readouts
/// shaped as the batch, whether they grow (the first) or shrink (the
/// second), a row per shot of a value per readout slot its
/// sender stated (an unchecked batch's instructions unread, so fewer or
/// more than they read), each value `Missing(ReadoutFailure::Other)` and
/// each shot clear, and the driver reads what the executor wrote; what it
/// did not write stays as found.
#[test]
fn each_batchs_readouts_come_shaped_as_the_batch_prefilled() {
    let log = Log::default();
    let counted = Rc::default();
    let shaped = vec![BatchReadouts::new(2, 2), BatchReadouts::new(1, 0)];
    let mut filled = shaped.clone();
    for position in 0..2 {
        filled[0].row_mut(position).copy_from_slice(&[One, One]);
    }
    let stating_fewer = vec![BatchReadouts::new(2, 1), BatchReadouts::new(1, 0)];
    let stating_more = vec![BatchReadouts::new(2, 3), BatchReadouts::new(1, 0)];
    let found: [Rc<RefCell<Vec<BatchReadouts>>>; 6] = Default::default();
    let executor = |row: usize, writes: bool| {
        let bits = writes.then(|| Bits::new(&log));
        RecordsWhatItFinds(bits, Rc::clone(&found[row]))
    };
    let profiler = || Profiler::new(&counted, &log);
    for (chain, read, found, shaped, expected) in [
        (
            "directly under the driver",
            read_on(|driver| driver.on(executor(0, true))),
            &found[0],
            &shaped,
            &filled,
        ),
        (
            "directly under the driver, writing nothing",
            read_on(|driver| driver.on(executor(1, false))),
            &found[1],
            &shaped,
            &shaped,
        ),
        (
            "below a profiler with the driver",
            read_on(|driver| driver.through(profiler()).on(executor(2, true))),
            &found[2],
            &shaped,
            &filled,
        ),
        (
            "below a profiler with the executor",
            read_on(|driver| driver.on(profiler().on(executor(3, true)))),
            &found[3],
            &shaped,
            &filled,
        ),
        (
            "stating 1 readout slot of 2 read, writing nothing",
            read_stating([1, 0], |driver| driver.on(executor(4, false))),
            &found[4],
            &stating_fewer,
            &stating_fewer,
        ),
        (
            "stating 3 readout slots of 2 read, writing nothing",
            read_stating([3, 0], |driver| driver.on(executor(5, false))),
            &found[5],
            &stating_more,
            &stating_more,
        ),
    ] {
        assert_eq!(*found.borrow(), *shaped, "{chain}: what the executor found");
        assert_eq!(&read, expected, "{chain}: what the driver read");
    }
}

/// [`Bits`] that, at its first `execute`, fills the readouts, then rejects
/// every shot and fails.
struct FailsFirst(Bits, bool);

impl Executor for FailsFirst {
    fn accepts(&self) -> &InstructionSet {
        Executor::accepts(&self.0)
    }

    fn provides(&self) -> &Capabilities {
        Executor::provides(&self.0)
    }

    async fn ready(&mut self, above: &Capabilities) -> Result<(), NotReady> {
        self.0.ready(above).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.0.execute(batches, seed, readouts).await?;
        if std::mem::replace(&mut self.1, false) {
            for batch_readouts in readouts.iter_mut() {
                for position in 0..batch_readouts.shots() {
                    batch_readouts.set_conditions(position, ShotConditions::REJECTED);
                }
            }
            return Err(ExecuteError::Failed {
                shot: ShotId::new(0),
                failure: Failure::Part("the device failed".into()),
            });
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.0.end_shots(shots);
    }
}

/// A [`Profiler`] that, when the `execute` below fails, sends it again,
/// with the same readouts, as an adapter recovering from a failure would.
struct RetriesAfterAFailure(Profiler);

impl Adapter for RetriesAfterAFailure {
    fn accepts(&self) -> &InstructionSet {
        Adapter::accepts(&self.0)
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn provides(&self) -> &Capabilities {
        Adapter::provides(&self.0)
    }

    fn requires(&self, above: &Capabilities) -> Capabilities {
        Adapter::requires(&self.0, above)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        if self.0.adapt(batches, seed, below, readouts).await.is_ok() {
            return Ok(());
        }
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that sends again, with the same readouts, an `execute` whose
/// executor filled them, rejected every shot, then failed: the driver
/// reads only the readouts of the `execute` sent again, its shots clear,
/// since the protocol shapes the readouts again for it.
#[test]
fn an_execute_sent_again_after_a_failure_gets_its_readouts_shaped_again() {
    let log = Log::default();
    let counted = Rc::default();
    let read = read_on(|driver| {
        driver
            .through(RetriesAfterAFailure(Profiler::new(&counted, &log)))
            .on(FailsFirst(Bits::new(&log), true))
    });
    let mut expected = vec![BatchReadouts::new(2, 2), BatchReadouts::new(1, 0)];
    for position in 0..2 {
        expected[0].row_mut(position).copy_from_slice(&[Zero, Zero]);
    }
    assert_eq!(read, expected, "each shot's second `x` reads `Zero`");
}

/// A [`Profiler`] that passes down readouts for all of its batches but the
/// first.
struct PassesTooFewReadouts(Profiler);

impl Adapter for PassesTooFewReadouts {
    fn accepts(&self) -> &InstructionSet {
        Adapter::accepts(&self.0)
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn provides(&self) -> &Capabilities {
        Adapter::provides(&self.0)
    }

    fn requires(&self, above: &Capabilities) -> Capabilities {
        Adapter::requires(&self.0, above)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, &mut readouts[1..]).await
    }
}

/// A driver passing one `BatchReadouts` for its two batches, or for no
/// batch, or an adapter passing down one fewer than its batches, in each
/// bracketing: the protocol panics in the caller's `execute`, once polled,
/// so the run fails naming the caller, before the executor runs.
#[test]
fn readouts_not_one_per_batch_fail_the_run_naming_the_caller() {
    let log = Log::default();
    let counted = Rc::default();
    let read = Rc::new(RefCell::new(None));
    for (row, run, part) in [
        (
            "the driver, for two batches",
            runner(
                TwoBatches::new(&read)
                    .passing(Passes::FewerThanBatches)
                    .on(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(0, type_name::<TwoBatches>()),
        ),
        (
            "the driver, for no batch",
            runner(
                TwoBatches::new(&read)
                    .passing(Passes::WithoutBatches)
                    .on(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(0, type_name::<TwoBatches>()),
        ),
        (
            "an adapter, in d.through(a).on(e)",
            runner(
                TwoBatches::new(&read)
                    .through(PassesTooFewReadouts(Profiler::new(&counted, &log)))
                    .on(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(1, type_name::<PassesTooFewReadouts>()),
        ),
        (
            "an adapter, in d.on(a.on(e))",
            runner(
                TwoBatches::new(&read)
                    .on(PassesTooFewReadouts(Profiler::new(&counted, &log)).on(Bits::new(&log))),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(1, type_name::<PassesTooFewReadouts>()),
        ),
    ] {
        let error = failed(run);
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part: named, message: Some(message) }
                    if *named == part && message.contains("one `BatchReadouts` per batch")
            ),
            "{row}: {error:?}"
        );
        assert_eq!(
            log.count("Bits execute"),
            0,
            "{row}: the executor never ran"
        );
    }
}

/// A driver sending a round at a time into the readouts it keeps: one
/// `execute` of an unchecked batch per shot, each measuring qubit 0, then
/// the same `execute` again, into the same readouts, whose allocations it
/// records as it polls it.
struct RoundsIntoItsReadouts {
    /// One `mz`, of one readout slot.
    instructions: Vec<Instruction>,
    allocated: Rc<Cell<Option<usize>>>,
}

impl Driver for RoundsIntoItsReadouts {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in &mut self.instructions {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<()>, DriveError> {
        let each = (0..shots.len())
            .map(|at| shots.select(&[at]).expect("a shot of the run"))
            .collect::<Vec<_>>();
        let batches = each
            .iter()
            .map(|shot| Batch::new(shot, &self.instructions, 1))
            .collect::<Vec<_>>();
        let mut kept = Readouts::new();
        below
            .execute(&batches, seed, kept.readouts_for(&batches))
            .await?;
        let mut execute = pin!(below.execute(&batches, seed, kept.readouts_for(&batches)));
        let (count, answered) = poll_fn(|context| {
            let mut answered = Poll::Pending;
            let count = allocations(|| answered = execute.as_mut().poll(context));
            Poll::Ready((count, answered))
        })
        .await;
        let Poll::Ready(ran) = answered else {
            panic!("the `execute` answers at its first poll");
        };
        ran?;
        self.allocated.set(Some(count));
        Ok(vec![(); shots.len()])
    }
}

/// An executor of [`CoinGates`](crate::tests::fake_parts::CoinGates)
/// writing `Zero` in every slot of every row, and allocating nothing.
struct Zeros;

impl Executor for Zeros {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    async fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        for batch_readouts in readouts {
            for position in 0..batch_readouts.shots() {
                batch_readouts.row_mut(position).fill(Zero);
            }
        }
        Ok(())
    }
}

/// A driver sending a round at a time, five unchecked batches in one
/// `execute`, into the readouts it keeps, over an executor that only fills
/// them: the second round, of the size of the first, allocates nothing,
/// though the protocol shapes each batch's readouts.
#[test]
fn a_round_of_unchecked_batches_into_readouts_kept_allocates_nothing() {
    let allocated = Rc::new(Cell::new(None));
    runner(
        (RoundsIntoItsReadouts {
            instructions: vec![coin_gate("mz", 0)],
            allocated: Rc::clone(&allocated),
        })
        .on(Zeros),
    )
    .run(&ShotSet::ids(0..5), SEED)
    .expect("a run");
    assert_eq!(allocated.get(), Some(0));
}
