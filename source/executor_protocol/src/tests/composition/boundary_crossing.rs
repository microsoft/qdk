// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Crossing a boundary, wherever the brackets go: a refusal or a
//! panic at a boundary inside a composition ends the run as it does at
//! the executor's; a checked batch is checked at every boundary it
//! crosses, beside unchecked ones of its `execute`, plain or `_each`; an
//! `execute` of no batches does not reach the receiver; a refused
//! `execute_each` gives no row; a boundary calls its receiver
//! only once its own `execute` is polled; and it shapes the readouts a
//! caller passes: each batch's to its batch, with the readout slots its
//! sender stated, prefilled, whatever they held, before the executor
//! fills them; again when an adapter sends an `execute` again after a
//! failure; a panic in the caller's `execute`, once polled, when they are
//! not one per batch; and, of unchecked batches, into readouts that held
//! readouts as large, no allocation. The checks of one batch are in
//! `batch_limits.rs`.

mod batch_limits;

use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use smallvec::smallvec;

use super::runner::{SEED, failed, runner};
use crate::tests::allocations;
use crate::tests::fake_parts::{
    Bits, ForwardsParity, Log, Misbehaving, Misbehaviour, NamesBelow, OutputsAsRowsCome,
    ParityGates, Profiler, Program, Renamed, RowsGiven, SeesPieces, Sending, coin_gate, coin_gates,
    in_each_bracketing, rows_logged,
};
use crate::{
    Adapt, Adapter, AdapterSetup, Argument, Batch, BatchReadouts, CompositionFaults, Drive,
    DriveError, Driver, DriverSetup, Execute, ExecuteError, ExecuteRefusal, Executor,
    ExecutorSetup, Failure, FinalRow, Instruction, InstructionFault, InstructionSet,
    InstructionStream, Needs, NotReady, Offer, OpcodeMap, PartId, ReadoutValue, Readouts, Runner,
    Seed, ShotConditions, ShotId, ShotSet,
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
fn failed_in_both_brackets<D: Drive<Output: std::fmt::Debug>, A: Adapt, E: Execute>(
    shots: &ShotSet,
    parts: impl Fn(&Log, &Rc<Cell<usize>>) -> (Driver<D>, Adapter<A>, Executor<E>),
) -> [Failed; 2] {
    let new = || (Log::default(), Rc::new(Cell::new(0)));
    let (log, counted) = new();
    let (driver, adapter, executor) = parts(&log, &counted);
    let left = Failed {
        brackets: "the adapter with the driver",
        error: failed_run((driver >> adapter) >> executor, shots),
        log,
        counted,
    };
    let (log, counted) = new();
    let (driver, adapter, executor) = parts(&log, &counted);
    let right = Failed {
        brackets: "the adapter with the executor",
        error: failed_run(driver >> (adapter >> executor), shots),
        log,
        counted,
    };
    [left, right]
}

/// The failure of the run of `shots` on the chain `composed` makes.
fn failed_run<D: Drive<Output: std::fmt::Debug>, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> DriveError {
    failed(runner(composed).run(shots, SEED))
}

/// Measures qubit 0 after flipping it: one checked batch of two
/// instructions, needing one qubit.
fn flip_and_measure() -> Driver<Program> {
    Driver::new(Program::new(&[("x", 0), ("mz", 0)]).with_checked_batches())
}

/// The failure of the run of `shots` on the chain `parts` makes, a driver
/// over an executor: a [`Failed`] with no adapter, so with no profiler,
/// whose count stays 0.
fn failed_with_no_adapter<D: Drive<Output: std::fmt::Debug>, E: Execute>(
    shots: &ShotSet,
    parts: impl Fn(&Log) -> (Driver<D>, Executor<E>),
) -> [Failed; 1] {
    let log = Log::default();
    let (driver, executor) = parts(&log);
    [Failed {
        brackets: "no adapter",
        error: failed_run(driver >> executor, shots),
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
/// a part writes them on its hot path, in one checked batch, with shots
/// needing `needs`.
fn flip_on_a_parity(needs: Needs) -> Driver<Program> {
    let on_qubit_0 = |opcode, arguments| Instruction {
        opcode,
        blocks: smallvec![0],
        arguments,
        select: None,
    };
    let mz = on_qubit_0(ParityGates::MZ, smallvec![]);
    let x_if = on_qubit_0(ParityGates::X_IF, smallvec![Argument::recent(1)]);
    Driver::new(
        Program::sending(ParityGates::set(), vec![mz.clone(), x_if, mz], needs)
            .with_checked_batches(),
    )
}

/// Each boundary refuses a checked batch its receiver does not take,
/// typed and naming the batch's first shot, before the receiver runs any
/// of it or of the other batches of its `execute`, whoever sends it: the
/// driver, to the executor or to an adapter, or an adapter, in both
/// bracketings. A row per sender, and one where an adapter's needs below
/// differ from the driver's, as the needs its batches are checked
/// against; the faults a batch may hold are in `batch_limits.rs`.
#[test]
fn a_batch_the_part_below_does_not_take_is_refused_before_it_runs() {
    let beyond_the_needs = |shot, at, block, needs| ExecuteRefusal::Instruction {
        shot: ShotId::new(shot),
        batch: 0,
        at,
        fault: InstructionFault::BlockBeyondNeeds {
            mnemonic: "x".to_owned(),
            block,
            needs,
        },
    };
    let parity_refused = ExecuteRefusal::Instruction {
        shot: ShotId::new(5),
        batch: 0,
        at: 1,
        fault: InstructionFault::ParityNotNeeded {
            mnemonic: "x_if".to_owned(),
            parameter: "c".to_owned(),
        },
    };
    let rows: [(&str, Vec<Failed>, ExecuteRefusal); 4] = [
        // An instruction on a qubit beyond the program's needs.
        (
            "the driver's instruction beyond its needs, to the executor",
            failed_with_no_adapter(&ShotSet::ids([4, 2]), |log| {
                (
                    Driver::new(
                        Program::new(&[("mz", 0), ("x", 5)])
                            .needing(Needs::qubits(2))
                            .with_checked_batches(),
                    ),
                    Executor::new(Bits::new(log)),
                )
            })
            .into(),
            beyond_the_needs(4, 1, 5, 2),
        ),
        // A driver flipping qubit 1 while needing one qubit, through a
        // profiler: refused at the driver's boundary, before the profiler
        // counts any.
        (
            "the driver's instruction beyond its needs, to an adapter",
            failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, counted| {
                (
                    Driver::new(
                        Program::new(&[("mz", 0), ("x", 1)])
                            .needing(Needs::qubits(1))
                            .with_checked_batches(),
                    ),
                    Adapter::new(Profiler::new(counted, log)),
                    Executor::new(Bits::new(log)),
                )
            })
            .into(),
            beyond_the_needs(5, 1, 1, 1),
        ),
        // A driver flipping and measuring qubit 0, under an adapter that
        // adds an `x` on qubit 9 while passing the driver's need of one
        // qubit down: the adapter's batch is refused, naming its third
        // instruction.
        (
            "an adapter's instruction beyond the needs",
            failed_in_both_brackets(&ShotSet::ids([4, 2]), |log, _| {
                (
                    flip_and_measure(),
                    Adapter::new(Misbehaving::new(Misbehaviour::SendsBeyondTheNeeds)),
                    Executor::new(Bits::new(log)),
                )
            })
            .into(),
            beyond_the_needs(4, 2, 9, 1),
        ),
        // A driver whose needs use readout parities, above an adapter that
        // says it resolves them, so that its needs below use none, but
        // passes them down, over an executor that resolves none: the
        // adapter's batch is refused.
        (
            "an adapter's readout parity its needs below do not use",
            failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, _| {
                (
                    flip_on_a_parity(Needs::qubits(1).with_readout_parity()),
                    Adapter::new(ForwardsParity::on(ParityGates::set(), log)),
                    Executor::new(Bits::accepting(ParityGates::set(), log)),
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

/// A driver whose needs use readout parities, through two profilers, over
/// an executor that resolves them, in each of the five bracketings of
/// four parts: every boundary passes the parity, and the executor runs it.
#[test]
fn a_readout_parity_the_needs_use_crosses_every_boundary() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = flip_on_a_parity(Needs::qubits(1).with_readout_parity());
            let a = Adapter::new(Profiler::on(ParityGates::set(), &counted, &log));
            let b = Adapter::new(Profiler::on(ParityGates::set(), &counted, &log));
            let e = Executor::new(
                Bits::accepting(ParityGates::set(), &log)
                    .offering(Offer::unlimited().max_qubits(64)),
            );
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
/// whose `accepts` answers [`ParityGates`] once ready, in which `x` is
/// `x_if` and `mz` is past the set's end, in a checked batch, directly and
/// through an adapter in both bracketings: each run checks the batch
/// against the set the `>>` that makes the runner read, so it runs and
/// reads 1.
#[test]
fn every_batch_is_checked_against_the_set_read_at_composition() {
    let log = Log::default();
    let names_below = || Adapter::new(NamesBelow::new(&log));
    let bits = || Executor::new(Bits::new(&log).accepting_once_ready(ParityGates::set()));
    let shots = ShotSet::ids([5, 3]);
    let runs = [
        runner(flip_and_measure() >> bits()).run(&shots, SEED),
        runner((flip_and_measure() >> names_below()) >> bits()).run(&shots, SEED),
        runner(flip_and_measure() >> (names_below() >> bits())).run(&shots, SEED),
    ];
    for (i, run) in runs.into_iter().enumerate() {
        let records = run.unwrap_or_else(|error| panic!("bracketing {i}: {error:?}"));
        let ones = (vec![ReadoutValue::One], ShotConditions::NONE);
        assert_eq!(records, [ones.clone(), ones], "bracketing {i}");
    }
    assert_eq!(log.count("Bits execute"), 3, "each bracketing ran it");
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
            let d = Driver::new(Program::new(&[("mz", 0)]));
            let n = Adapter::new(NamesBelow::new(&log));
            let e = Executor::new(Renamed::new(Bits::new(&log), "qodec runtime"));
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
            let d = Driver::new(Program::new(&[("mz", 0)]));
            let n = Adapter::new(NamesBelow::new(&log));
            let p = Adapter::new(Renamed::new(Profiler::new(&counted, &log), "profiler"));
            let e = Executor::new(Renamed::new(Bits::new(&log), "qodec runtime"));
        },
        |chain, brackets| {
            runner(chain).run(&shots, SEED).expect("a run");
            assert_eq!(read(&log), ["below: profiler"], "{brackets}");
        }
    );
}

/// An adapter of a chain whose one faulty part sends an instruction
/// beyond the needs: a profiler, or, if it is that part, an adapter
/// adding an `x` on qubit 9 beyond the one qubit it passes down.
enum Stage {
    Counts(Profiler),
    Faulty(Misbehaving),
}

impl AdapterSetup for Stage {
    fn accepts(&self) -> &InstructionSet {
        match self {
            Self::Counts(profiler) => profiler.accepts(),
            Self::Faulty(faulty) => faulty.accepts(),
        }
    }

    fn sends(&self) -> &InstructionSet {
        self.accepts()
    }

    fn bind(&mut self, below: OpcodeMap) {
        match self {
            Self::Counts(profiler) => profiler.bind(below),
            Self::Faulty(faulty) => faulty.bind(below),
        }
    }
}

impl Adapt for Stage {
    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        match self {
            Self::Counts(profiler) => profiler.adapt(batches, seed, below, readouts).await,
            Self::Faulty(faulty) => faulty.adapt(batches, seed, below, readouts).await,
        }
    }
}

/// The parts of a chain of a driver, up to two adapters and bits, of
/// which one sends an instruction beyond the needs: the driver, an `x` on
/// qubit 5 while needing two qubits, if `faulty` is 0, or else the
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

    fn d(&self) -> Driver<Program> {
        let program = if self.faulty == 0 {
            Program::new(&[("mz", 0), ("x", 5)]).needing(Needs::qubits(2))
        } else {
            Program::new(&[("x", 0), ("mz", 0)])
        };
        Driver::new(if self.checked {
            program.with_checked_batches()
        } else {
            program
        })
    }

    /// The adapter at `position`, 1 or 2 from the top.
    fn p(&self, position: usize) -> Adapter<Stage> {
        Adapter::new(if self.faulty == position {
            Stage::Faulty(Misbehaving::new(Misbehaviour::SendsBeyondTheNeeds))
        } else {
            Stage::Counts(Profiler::new(&self.counted[position - 1], &self.log))
        })
    }

    fn e(&self) -> Executor<Bits> {
        Executor::new(Bits::new(&self.log))
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
fn each_boundary_checks_its_batch<D: Drive<Output: std::fmt::Debug>, E: Execute>(
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
                Failure::Refused(ExecuteRefusal::Instruction {
                    fault: InstructionFault::BlockBeyondNeeds { .. },
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
/// part above a boundary sending an instruction beyond the needs in turn:
/// if the driver's batches are checked, the adapters' are too, and the
/// faulty batch is refused at the boundary below that part, wherever the
/// brackets go; batches are unchecked by default.
#[test]
fn a_checked_batch_is_checked_at_every_boundary_wherever_the_brackets_go() {
    each_boundary_checks_its_batch("d>>e", 0, |c| c.d() >> c.e());
    each_boundary_checks_its_batch("(d>>p)>>e", 1, |c| (c.d() >> c.p(1)) >> c.e());
    each_boundary_checks_its_batch("d>>(p>>e)", 1, |c| c.d() >> (c.p(1) >> c.e()));
    each_boundary_checks_its_batch("((d>>p)>>p)>>e", 2, |c| {
        ((c.d() >> c.p(1)) >> c.p(2)) >> c.e()
    });
    each_boundary_checks_its_batch("d>>(p>>(p>>e))", 2, |c| {
        c.d() >> (c.p(1) >> (c.p(2) >> c.e()))
    });
    each_boundary_checks_its_batch("(d>>(p>>p))>>e", 2, |c| {
        (c.d() >> (c.p(1) >> c.p(2))) >> c.e()
    });
    each_boundary_checks_its_batch("d>>((p>>p)>>e)", 2, |c| {
        c.d() >> ((c.p(1) >> c.p(2)) >> c.e())
    });
    each_boundary_checks_its_batch("(d>>p)>>(p>>e)", 2, |c| {
        (c.d() >> c.p(1)) >> (c.p(2) >> c.e())
    });
}

/// A driver sending, by `execute_each`, a checked batch with an
/// instruction beyond its needs, through a profiler, wherever the brackets
/// go: the boundary refuses it before the bits run anything, and the driver
/// is given no row.
#[test]
fn a_refused_execute_each_gives_no_row() {
    let counted = Rc::new(Cell::new(0));
    in_each_bracketing!([d, a, e] from {
        let (given, log) = (RowsGiven::default(), Log::default());
        let d = Driver::new(OutputsAsRowsCome::new(&given, &log).beyond_its_needs());
        let a = Adapter::new(Profiler::new(&counted, &log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let error = failed(runner(chain).run(&ShotSet::ids(0..4), SEED));
        assert!(
            matches!(
                &error.failure,
                Failure::Refused(ExecuteRefusal::Instruction {
                    fault: InstructionFault::BlockBeyondNeeds { .. },
                    ..
                })
            ),
            "{brackets}: {error:?}"
        );
        assert!(given.borrow().is_empty(), "{brackets}: no row given");
        assert_eq!(rows_logged(&log), Vec::<String>::new(), "{brackets}");
    });
}

/// Two batches of one `execute`, plain or `_each`, on shots 4 and 2 a
/// measure of qubit 0, on shots 9 and 7 a flip of qubit 5 beyond the
/// program's 2 qubits, one checked and the other not: each batch is
/// checked as its sender chose, in both forms. Checked, the second batch
/// is refused, naming batch 1 and its first shot, 9; unchecked, after a
/// checked first batch, it reaches the bits, which panic on it.
#[test]
fn each_batch_of_an_execute_is_checked_as_its_sender_chose() {
    let shots = ShotSet::ids([4, 2, 9, 7]);
    let log = Log::default();
    for (by, sending) in [("execute", Sending::Whole), ("execute_each", Sending::Each)] {
        let run = |first, second| {
            runner(
                Driver::new(
                    Program::halves(&[("mz", 0)], &[("x", 5)])
                        .needing(Needs::qubits(2))
                        .with_checked_halves(first, second)
                        .sending_as(sending.clone()),
                ) >> Executor::new(Bits::new(&log)),
            )
            .run(&shots, SEED)
        };
        let error = failed(run(false, true));
        assert!(
            matches!(
                error.failure,
                Failure::Refused(ExecuteRefusal::Instruction {
                    shot,
                    batch: 1,
                    at: 0,
                    fault: InstructionFault::BlockBeyondNeeds { .. },
                }) if shot == ShotId::new(9)
            ),
            "the checked second batch, by {by}: {error:?}"
        );
        let error = failed(run(true, false));
        let bits = PartId::new(1, type_name::<Bits>());
        assert!(
            matches!(&error.failure, Failure::Panicked { part, .. } if *part == bits),
            "the unchecked second batch, by {by}: {error:?}"
        );
    }
}

/// A driver sending one stream of 6 instructions on qubit 0, checked if
/// `checked`, needing 1 qubit, with a flip of qubit 3 beyond its needs at
/// `beyond`; it keeps the error of its `execute_stream`, rather than
/// failing, so that a test reads it whole.
struct StreamsBeyondItsNeeds {
    instructions: Vec<Instruction>,
    checked: bool,
    error: Rc<RefCell<Option<ExecuteError>>>,
}

impl StreamsBeyondItsNeeds {
    fn new(beyond: usize, checked: bool, error: &Rc<RefCell<Option<ExecuteError>>>) -> Self {
        let instructions = (0..6)
            .map(|at| match at {
                at if at == beyond => coin_gate("x", 3),
                at if at % 2 == 0 => coin_gate("x", 0),
                _ => coin_gate("mz", 0),
            })
            .collect();
        Self {
            instructions,
            checked,
            error: Rc::clone(error),
        }
    }
}

impl DriverSetup for StreamsBeyondItsNeeds {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in &mut self.instructions {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for StreamsBeyondItsNeeds {
    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<()>, DriveError> {
        let stream = InstructionStream::new(shots, self.instructions.iter().cloned())
            .checked_if(self.checked);
        let mut readouts = BatchReadouts::new(0, 0);
        let answer = below.execute_stream(stream, seed, &mut readouts).await;
        *self.error.borrow_mut() = answer.err();
        below.end_shots(shots);
        Ok(vec![(); shots.len()])
    }
}

/// A stream on shots 4 and 2, with a flip beyond its needs, to bits
/// taking at most 2 instructions per batch, which run streams themselves
/// or cut them by the default, and refuse such a flip themselves when it
/// reaches them. A refusal ends the stream where it is found, naming batch
/// 0 and the flip by its position in the stream: in the first piece, as a
/// refusal, nothing having run; in a later piece, as a failure, the
/// pieces before it having run, since their shots' states have changed.
/// A checked stream is refused by the boundary, a piece of 2 at a time,
/// each before any of it reaches the bits, the first before the bits are
/// called, even those that run streams themselves; an unchecked one by
/// the bits, at the piece the default sends them.
#[test]
fn a_refusal_ends_a_stream_as_refused_in_its_first_piece_and_failed_after() {
    let shots = ShotSet::ids([4, 2]);
    let capped = |log: &Log| {
        let offer = Offer::unlimited().max_qubits(64).without_readout_parity();
        Bits::new(log)
            .offering(offer.max_instructions(2))
            .checking_blocks()
    };
    let beyond = |at| ExecuteRefusal::Instruction {
        shot: ShotId::new(4),
        batch: 0,
        at,
        fault: InstructionFault::BlockBeyondNeeds {
            mnemonic: "x".to_owned(),
            block: 3,
            needs: 1,
        },
    };
    for (checked, beyond_at, pieces_ran) in
        [(true, 0, 0), (true, 4, 2), (false, 1, 0), (false, 3, 1)]
    {
        let refusal = beyond(beyond_at);
        let expected = |error: Option<&ExecuteError>| match error {
            Some(ExecuteError::Refused(refused)) => pieces_ran == 0 && *refused == refusal,
            Some(ExecuteError::Failed {
                shot,
                failure: Failure::Refused(refused),
            }) => pieces_ran > 0 && *shot == ShotId::new(4) && *refused == refusal,
            _ => false,
        };
        let case = format!("a flip beyond the needs at {beyond_at}, checked: {checked}");
        let (error, log, produced) = (Rc::default(), Log::default(), Rc::new(Cell::new(0)));
        runner(
            Driver::new(StreamsBeyondItsNeeds::new(beyond_at, checked, &error))
                >> Executor::new(SeesPieces::new(capped(&log), &produced, &log)),
        )
        .run(&shots, SEED)
        .expect("the driver keeps the error");
        let error = error.take();
        assert!(
            expected(error.as_ref()),
            "{case}, by the default: {error:?}"
        );
        assert_eq!(
            log.count("Bits execute"),
            pieces_ran,
            "{case}: the pieces before it ran, {:?}",
            log.entries()
        );
        if !checked {
            // The bits' own `execute_stream` does not check its blocks.
            continue;
        }
        let (error, log) = (Rc::default(), Log::default());
        runner(
            Driver::new(StreamsBeyondItsNeeds::new(beyond_at, checked, &error))
                >> Executor::new(capped(&log)),
        )
        .run(&shots, SEED)
        .expect("the driver keeps the error");
        let error = error.take();
        assert!(
            expected(error.as_ref()),
            "{case}, by the bits' own: {error:?}"
        );
        assert_eq!(
            log.count("Bits execute_stream"),
            usize::from(pieces_ran > 0),
            "{case}: the bits' own are called only past the first piece, {:?}",
            log.entries()
        );
    }
}

/// A driver making one `execute` of no batches, with no readouts, plain
/// or `_each`.
struct SendsNothing {
    through_each: bool,
}

impl DriverSetup for SendsNothing {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for SendsNothing {
    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<()>, DriveError> {
        let mut kept = Readouts::new();
        let readouts = kept.readouts_for(&[]);
        if self.through_each {
            below
                .execute_each(&[], seed, readouts, &mut |_| panic!("no row to give"))
                .await?;
        } else {
            below.execute(&[], seed, readouts).await?;
        }
        Ok(vec![(); shots.len()])
    }
}

/// A driver making one `execute` of no batches, plain or `_each`,
/// directly over an executor that counts its calls, or through an adapter
/// that counts its own, in each bracketing: the run succeeds and no part
/// below is called, so an executor never has to answer an `execute` of no
/// batches.
#[test]
fn an_execute_of_no_batches_reaches_no_part_below() {
    for through_each in [false, true] {
        let executed = Rc::new(Cell::new(0));
        runner(
            Driver::new(SendsNothing { through_each })
                >> Executor::new(AnswersWhenCalled(Rc::clone(&executed))),
        )
        .run(&ShotSet::ids(0..2), SEED)
        .expect("the run succeeds");
        assert_eq!(
            executed.get(),
            0,
            "no adapter, through_each: {through_each}"
        );

        let adapted = Rc::new(Cell::new(0));
        in_each_bracketing!([d, a, e] from {
            let d = Driver::new(SendsNothing { through_each });
            let a = Adapter::new(ForwardsWhenCalled(Rc::clone(&adapted)));
            let e = Executor::new(AnswersWhenCalled(Rc::clone(&executed)));
        }, |chain, brackets| {
            runner(chain)
                .run(&ShotSet::ids(0..2), SEED)
                .expect("the run succeeds");
            assert_eq!(
                (adapted.get(), executed.get()),
                (0, 0),
                "{brackets}, through_each: {through_each}"
            );
        });
    }
}

/// A driver that makes one `execute` of an empty batch below, and awaits
/// it, or drops it before its first poll.
struct MakesAnExecute {
    awaits: bool,
}

impl DriverSetup for MakesAnExecute {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for MakesAnExecute {
    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
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

impl ExecutorSetup for AnswersWhenCalled {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
    }
}

impl Execute for AnswersWhenCalled {
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

impl AdapterSetup for ForwardsWhenCalled {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}
}

impl Adapt for ForwardsWhenCalled {
    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
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
        runner(
            Driver::new(MakesAnExecute { awaits })
                >> Executor::new(AnswersWhenCalled(Rc::clone(&called))),
        )
        .run(&ShotSet::ids(0..2), SEED)
        .expect("the run succeeds");
        assert_eq!(called.get(), calls, "awaits: {awaits}");

        let adapted = Rc::new(Cell::new(0));
        let executed = Rc::new(Cell::new(0));
        in_each_bracketing!([d, a, e] from {
            let d = Driver::new(MakesAnExecute { awaits });
            let a = Adapter::new(ForwardsWhenCalled(Rc::clone(&adapted)));
            let e = Executor::new(AnswersWhenCalled(Rc::clone(&executed)));
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
    /// Whether it calls `execute_each` rather than `execute`.
    through_each: bool,
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
            through_each: false,
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

    /// This driver, calling `execute_each`, its rows ignored.
    fn through_each(mut self) -> Self {
        self.through_each = true;
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

impl DriverSetup for TwoBatches {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.instructions.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for TwoBatches {
    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
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
        if self.through_each {
            below
                .execute_each(sent, seed, passed, &mut |_: FinalRow<'_>| {})
                .await?;
        } else {
            below.execute(sent, seed, passed).await?;
        }
        *self.read.borrow_mut() = Some(readouts.to_vec());
        Ok(vec![(); shots.len()])
    }
}

/// What a [`TwoBatches`] read on the chain `below` composes it into.
fn read_on<D: Drive, E: Execute>(
    below: impl FnOnce(Driver<TwoBatches>) -> Result<Runner<D, E>, CompositionFaults>,
) -> Vec<BatchReadouts> {
    read_stating([2, 0], below)
}

/// What a [`TwoBatches`] stating `stated` readout slots read on the chain
/// `below` composes it into.
fn read_stating<D: Drive, E: Execute>(
    stated: [u32; 2],
    below: impl FnOnce(Driver<TwoBatches>) -> Result<Runner<D, E>, CompositionFaults>,
) -> Vec<BatchReadouts> {
    let read = Rc::new(RefCell::new(None));
    runner(below(Driver::new(TwoBatches::new(&read).stating(stated))))
        .run(&ShotSet::ids(0..3), SEED)
        .expect("a run");
    read.take().expect("the driver read its readouts")
}

/// An executor that records the readouts it is passed, as it finds them,
/// then lets [`Bits`] fill them, or, if it has none, leaves them as they
/// are.
struct RecordsWhatItFinds(Option<Bits>, Rc<RefCell<Vec<BatchReadouts>>>);

impl ExecutorSetup for RecordsWhatItFinds {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn offers(&self) -> Offer {
        Offer::unlimited().max_qubits(64)
    }
}

impl Execute for RecordsWhatItFinds {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        match &mut self.0 {
            Some(bits) => bits.ready(needs).await,
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
        Executor::new(RecordsWhatItFinds(bits, Rc::clone(&found[row])))
    };
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    for (chain, read, found, shaped, expected) in [
        (
            "directly under the driver",
            read_on(|driver| driver >> executor(0, true)),
            &found[0],
            &shaped,
            &filled,
        ),
        (
            "directly under the driver, writing nothing",
            read_on(|driver| driver >> executor(1, false)),
            &found[1],
            &shaped,
            &shaped,
        ),
        (
            "below a profiler with the driver",
            read_on(|driver| (driver >> profiler()) >> executor(2, true)),
            &found[2],
            &shaped,
            &filled,
        ),
        (
            "below a profiler with the executor",
            read_on(|driver| driver >> (profiler() >> executor(3, true))),
            &found[3],
            &shaped,
            &filled,
        ),
        (
            "stating 1 readout slot of 2 read, writing nothing",
            read_stating([1, 0], |driver| driver >> executor(4, false)),
            &found[4],
            &stating_fewer,
            &stating_fewer,
        ),
        (
            "stating 3 readout slots of 2 read, writing nothing",
            read_stating([3, 0], |driver| driver >> executor(5, false)),
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

impl ExecutorSetup for FailsFirst {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn offers(&self) -> Offer {
        self.0.offers()
    }
}

impl Execute for FailsFirst {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.0.ready(needs).await
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

impl AdapterSetup for RetriesAfterAFailure {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }
}

impl Adapt for RetriesAfterAFailure {
    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
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
            >> Adapter::new(RetriesAfterAFailure(Profiler::new(&counted, &log)))
            >> Executor::new(FailsFirst(Bits::new(&log), true))
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

impl AdapterSetup for PassesTooFewReadouts {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }
}

impl Adapt for PassesTooFewReadouts {
    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, &mut readouts[1..]).await
    }
}

/// A driver passing one `BatchReadouts` for its two batches, or for no
/// batch, by `execute` or `execute_each`, or an adapter passing down one
/// fewer than its batches, in each bracketing: the protocol panics in the
/// caller's `execute`, once polled, so the run fails naming the caller,
/// before the executor runs.
#[test]
fn readouts_not_one_per_batch_fail_the_run_naming_the_caller() {
    let log = Log::default();
    let counted = Rc::default();
    let read = Rc::new(RefCell::new(None));
    for (row, run, part) in [
        (
            "the driver, for two batches",
            runner(
                Driver::new(TwoBatches::new(&read).passing(Passes::FewerThanBatches))
                    >> Executor::new(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(0, type_name::<TwoBatches>()),
        ),
        (
            "the driver, for no batch",
            runner(
                Driver::new(TwoBatches::new(&read).passing(Passes::WithoutBatches))
                    >> Executor::new(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(0, type_name::<TwoBatches>()),
        ),
        (
            "the driver, for two batches, by `execute_each`",
            runner(
                Driver::new(
                    TwoBatches::new(&read)
                        .passing(Passes::FewerThanBatches)
                        .through_each(),
                ) >> Executor::new(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(0, type_name::<TwoBatches>()),
        ),
        (
            "the driver, for no batch, by `execute_each`",
            runner(
                Driver::new(
                    TwoBatches::new(&read)
                        .passing(Passes::WithoutBatches)
                        .through_each(),
                ) >> Executor::new(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(0, type_name::<TwoBatches>()),
        ),
        (
            "an adapter, in (d >> a) >> e",
            runner(
                Driver::new(TwoBatches::new(&read))
                    >> Adapter::new(PassesTooFewReadouts(Profiler::new(&counted, &log)))
                    >> Executor::new(Bits::new(&log)),
            )
            .run(&ShotSet::ids(0..3), SEED),
            PartId::new(1, type_name::<PassesTooFewReadouts>()),
        ),
        (
            "an adapter, in d >> (a >> e)",
            runner(
                Driver::new(TwoBatches::new(&read))
                    >> (Adapter::new(PassesTooFewReadouts(Profiler::new(&counted, &log)))
                        >> Executor::new(Bits::new(&log))),
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

impl DriverSetup for RoundsIntoItsReadouts {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in &mut self.instructions {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for RoundsIntoItsReadouts {
    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
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

impl ExecutorSetup for Zeros {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
    }
}

impl Execute for Zeros {
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
        Driver::new(RoundsIntoItsReadouts {
            instructions: vec![coin_gate("mz", 0)],
            allocated: Rc::clone(&allocated),
        }) >> Executor::new(Zeros),
    )
    .run(&ShotSet::ids(0..5), SEED)
    .expect("a run");
    assert_eq!(allocated.get(), Some(0));
}
