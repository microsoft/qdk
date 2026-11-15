// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks at each boundary of a chain, wherever the brackets go: a
//! refusal, a shape fault or a panic at a boundary inside a composed part
//! ends the run as it does at the executor's, and a checked batch is
//! checked at every boundary it crosses, beside unchecked ones of its
//! `execute`.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use smallvec::smallvec;

use super::{SEED, failed, runner};
use crate::tests::fake_parts::{
    Bits, ForwardsParity, Log, Misbehaving, Misbehaviour, Misshape, Misshapen, NamesBelow,
    ParityGates, Profiler, Program, Renamed, in_each_bracketing,
};
use crate::{
    Adapt, Adapter, Argument, Batch, BlockType, Capabilities, ChainPart, CompositionFaults, Drive,
    DriveError, Driver, Execute, ExecuteError, ExecuteRefusal, Executor, Failure, Instruction,
    InstructionDef, InstructionFault, InstructionSet, Needs, OpcodeMap, ReadoutValue, Readouts,
    RunError, Runner, Seed, ShapeFault, ShotConditions, ShotId, ShotSet,
};

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
/// of it, whoever sends it: the driver, to the executor or to an adapter,
/// or an adapter, in both bracketings. A row per fault and sender.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
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
    let too_long = |max, got| ExecuteRefusal::TooManyInstructions {
        shot: ShotId::new(0),
        batch: 0,
        max,
        got,
    };
    let parity_refused = ExecuteRefusal::Instruction {
        shot: ShotId::new(5),
        batch: 0,
        at: 1,
        fault: InstructionFault::ParityNotProvisioned {
            mnemonic: "x_if".to_owned(),
            parameter: "c".to_owned(),
        },
    };
    let most_instructions = |most| {
        Capabilities::unlimited()
            .max_qubits(64)
            .max_instructions(most)
    };
    let triples = || Adapter::new(Misbehaving::new(Misbehaviour::TriplesEachBatch));
    let wide = InstructionSet::new(
        "Wide",
        [(BlockType::qubit(), 1)],
        [InstructionDef::new("mz")
            .acts_on([BlockType::qubit()])
            .outcomes(1 << 31)],
    )
    .expect("a set");
    let mz = Instruction {
        opcode: wide.opcode("mz").expect("its `mz`"),
        blocks: smallvec![0],
        arguments: smallvec![],
        select: None,
    };
    let rows: [(&str, Vec<Failed>, ExecuteRefusal); 10] = [
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
        // A batch longer than the executor takes.
        (
            "the driver's batch longer than the executor takes",
            failed_with_no_adapter(&ShotSet::ids(0..2), |log| {
                (
                    flip_and_measure(),
                    Executor::new(Bits::new(log).offering(most_instructions(1))),
                )
            })
            .into(),
            too_long(1, 2),
        ),
        // A driver's batch of two instructions, through a profiler, above
        // an executor taking one at most: refused at the driver's boundary,
        // before the profiler counts any.
        (
            "the driver's batch longer than the offer below an adapter",
            failed_in_both_brackets(&ShotSet::ids(0..2), |log, counted| {
                (
                    flip_and_measure(),
                    Adapter::new(Profiler::new(counted, log)),
                    Executor::new(Bits::new(log).offering(most_instructions(1))),
                )
            })
            .into(),
            too_long(1, 2),
        ),
        // A driver's batch of two instructions, tripled by an adapter,
        // above an executor taking 4 at most, with a profiler between them
        // or not: the adapter's batch of 6 is refused, and nothing below
        // the adapter runs.
        (
            "an adapter's batch longer than the executor takes",
            failed_in_both_brackets(&ShotSet::ids(0..2), |log, _| {
                (
                    flip_and_measure(),
                    triples(),
                    Executor::new(Bits::new(log).offering(most_instructions(4))),
                )
            })
            .into(),
            too_long(4, 6),
        ),
        (
            "an adapter's batch longer than the executor takes, through a profiler",
            failed_in_both_brackets(&ShotSet::ids(0..2), |log, counted| {
                (
                    flip_and_measure(),
                    triples() >> Adapter::new(Profiler::new(counted, log)),
                    Executor::new(Bits::new(log).offering(most_instructions(4))),
                )
            })
            .into(),
            too_long(4, 6),
        ),
        // A driver reading qubit 0 twice with an `mz` of 2^31 outcomes,
        // through a profiler, over bits: the batch reads more readout slots
        // than a `u32` counts, so it is refused at the driver's boundary,
        // naming its second instruction.
        (
            "the driver's batch reading more readout slots than a u32 counts",
            failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, counted| {
                (
                    Driver::new(
                        Program::sending(
                            wide.clone(),
                            vec![mz.clone(), mz.clone()],
                            Needs::qubits(1),
                        )
                        .with_checked_batches(),
                    ),
                    Adapter::new(Profiler::on(wide.clone(), counted, log)),
                    Executor::new(Bits::accepting(wide.clone(), log)),
                )
            })
            .into(),
            ExecuteRefusal::TooManyReadoutSlots {
                shot: ShotId::new(5),
                batch: 0,
                at: 1,
            },
        ),
        // A driver sending a readout parity while its needs use none,
        // through a profiler, above an executor that resolves them: refused
        // at the driver's boundary, since the admission never checked that
        // the parts below resolve them.
        (
            "the driver's readout parity its needs do not use",
            failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, counted| {
                (
                    flip_on_a_parity(Needs::qubits(1)),
                    Adapter::new(Profiler::on(ParityGates::set(), counted, log)),
                    Executor::new(
                        Bits::accepting(ParityGates::set(), log)
                            .offering(Capabilities::unlimited().max_qubits(64)),
                    ),
                )
            })
            .into(),
            parity_refused.clone(),
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
                    .offering(Capabilities::unlimited().max_qubits(64)),
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
/// whose `accepts` answers [`ParityGates`] once provisioned, in which
/// `x` is `x_if` and `mz` is past the set's end, in a checked batch,
/// directly and through an adapter in both bracketings: each run checks
/// the batch against the set read before the bits were provisioned, so it
/// runs and reads 1, and the adapter reads that set as the one below it
/// accepts.
#[test]
fn every_batch_is_checked_against_the_set_read_before_provision() {
    let log = Log::default();
    let names_below = || Adapter::new(NamesBelow::new(&log));
    let bits = || Executor::new(Bits::new(&log).accepting_once_provisioned(ParityGates::set()));
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
    assert_eq!(log.count("below accepts: CoinGates"), 2);
}

/// An executor answering a row fewer than the batch's two shots, below a
/// profiler: a failure naming the batch's first shot and the shape fault.
#[test]
fn a_misshapen_answer_below_an_adapter_is_a_failure() {
    for run in failed_in_both_brackets(&ShotSet::ids([6, 1]), |log, counted| {
        (
            Driver::new(Program::new(&[("mz", 0)])),
            Adapter::new(Profiler::new(counted, log)),
            Executor::new(Misshapen(Misshape::MissingRow)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = run;
        assert_eq!(error.shot, ShotId::new(6), "{brackets}");
        let Failure::Part(fault) = error.failure else {
            panic!("{brackets}: the part's error, not {:?}", error.failure);
        };
        assert_eq!(
            fault.downcast_ref::<ShapeFault>(),
            Some(&ShapeFault::Rows {
                batch: 0,
                expected: 2,
                got: 1
            }),
            "{brackets}"
        );
    }
}

/// Two decoders of one name, a profiler above an adapter that panics, in
/// each of the five bracketings of four parts: every failure names the
/// lower one, part 2, so its position tells it from the other.
#[test]
fn a_panic_tells_two_parts_of_one_name_apart_by_position() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = Driver::new(Program::new(&[("mz", 0)]));
            let a = Adapter::new(Renamed::new(Profiler::new(&counted, &log), "decoder"));
            let b = Adapter::new(Renamed::new(
                Misbehaving::new(Misbehaviour::PanicsWithAString),
                "decoder",
            ));
            let e = Executor::new(Bits::new(&log));
        },
        |chain, brackets| {
            let error = failed(runner(chain).run(&ShotSet::ids([3, 1]), SEED));
            assert!(
                matches!(
                    &error.failure,
                    Failure::Panicked { part, .. } if *part == ChainPart::new(2, "decoder")
                ),
                "{brackets}: {error:?}"
            );
        }
    );
}

/// An adapter that reads the name of the part below it, over an executor
/// or over an adapter, each named as a part built at run time is,
/// bracketed both ways: it reads that part's name, though the protocol
/// checks each answer to the `execute`s it sends.
#[test]
fn an_adapter_reads_the_name_of_the_part_below() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let program = || Driver::new(Program::new(&[("mz", 0)]));
    let names_below = || Adapter::new(NamesBelow::new(&log));
    let runtime = || Executor::new(Renamed::new(Bits::new(&log), "qodec runtime"));
    let profiler = || Adapter::new(Renamed::new(Profiler::new(&counted, &log), "profiler"));
    let shots = ShotSet::ids([3, 1]);
    let run = |result: Result<_, RunError>| result.expect("a run");
    run(runner((program() >> names_below()) >> runtime()).run(&shots, SEED));
    run(runner(program() >> (names_below() >> runtime())).run(&shots, SEED));
    run(runner((program() >> names_below()) >> (profiler() >> runtime())).run(&shots, SEED));
    run(runner(program() >> (names_below() >> (profiler() >> runtime()))).run(&shots, SEED));
    let read = log
        .entries()
        .into_iter()
        .filter(|entry| entry.starts_with("below: "))
        .collect::<Vec<_>>();
    assert_eq!(
        read,
        [
            "below: qodec runtime",
            "below: qodec runtime",
            "below: profiler",
            "below: profiler"
        ]
    );
}

/// An adapter of a chain whose one faulty part sends an instruction
/// beyond the needs: a profiler, or, if it is that part, an adapter
/// adding an `x` on qubit 9 beyond the one qubit it passes down.
enum Stage {
    Counts(Profiler),
    Faulty(Misbehaving),
}

impl Adapt for Stage {
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

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut Readouts,
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
        let bits = ChainPart::new(adapters + 1, type_name::<Bits>());
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

/// How a run of [`each_batch_of_an_execute_is_checked_as_its_sender_chose`]
/// ends.
enum Outcome {
    /// It gives a record per shot.
    Runs,
    /// The `execute` is refused.
    Refused(ExecuteRefusal),
    /// The answer is misshapen, a failure of shot 9.
    Misshapen(ShapeFault),
}

/// Two batches of one `execute`, on shots 4 and 2 a measure of qubit 0,
/// on shots 9 and 7 a flip and a measure, one checked and the other not,
/// to bits that take one instruction a batch, or to an executor answering
/// a value more in each row of the second batch: each batch's count of
/// instructions is checked whatever its sender chose, and its answer's
/// width as its sender chose. The unchecked batch's width goes unreported,
/// and the run gives a record per shot; every other fault names batch 1
/// and its first shot, 9.
#[test]
fn each_batch_of_an_execute_is_checked_as_its_sender_chose() {
    let shots = ShotSet::ids([4, 2, 9, 7]);
    let program = |first, second| {
        Driver::new(
            Program::halves(&[("mz", 0)], &[("x", 0), ("mz", 0)])
                .with_checked_halves(first, second),
        )
    };
    let log = Log::default();
    let one_instruction = || {
        Executor::new(
            Bits::new(&log).offering(Capabilities::unlimited().max_qubits(64).max_instructions(1)),
        )
    };
    let wider = || Executor::new(Misshapen(Misshape::ExtraValue));
    let records = |run: Result<Vec<_>, RunError>| run.map(|records| records.len());
    let too_many = || {
        Outcome::Refused(ExecuteRefusal::TooManyInstructions {
            shot: ShotId::new(9),
            batch: 1,
            max: 1,
            got: 2,
        })
    };
    let rows: [(&str, Result<usize, RunError>, Outcome); 4] = [
        (
            "a batch longer than the bits take, unchecked, after a checked one",
            records(runner(program(true, false) >> one_instruction()).run(&shots, SEED)),
            too_many(),
        ),
        (
            "a batch longer than the bits take, checked, after an unchecked one",
            records(runner(program(false, true) >> one_instruction()).run(&shots, SEED)),
            too_many(),
        ),
        (
            "a value more in each row of the unchecked batch, after a checked one",
            records(runner(program(true, false) >> wider()).run(&shots, SEED)),
            Outcome::Runs,
        ),
        (
            "a value more in each row of the checked batch, after an unchecked one",
            records(runner(program(false, true) >> wider()).run(&shots, SEED)),
            Outcome::Misshapen(ShapeFault::Slots {
                batch: 1,
                expected: 1,
                got: 2,
            }),
        ),
    ];
    for (row, run, outcome) in rows {
        match outcome {
            Outcome::Runs => {
                assert_eq!(run.expect(row), 4, "{row}: a record per shot");
            }
            Outcome::Refused(refusal) => {
                let error = failed(run);
                assert_eq!(error.shot, refusal.shot(), "{row}");
                assert!(
                    matches!(&error.failure, Failure::Refused(got) if *got == refusal),
                    "{row}: {error:?}"
                );
            }
            Outcome::Misshapen(fault) => {
                let error = failed(run);
                assert_eq!(error.shot, ShotId::new(9), "{row}");
                let Failure::Part(got) = &error.failure else {
                    panic!("{row}: the part's error, not {:?}", error.failure);
                };
                assert_eq!(got.downcast_ref::<ShapeFault>(), Some(&fault), "{row}");
            }
        }
    }
}
