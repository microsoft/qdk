// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` through the protocol does with the readouts its
//! caller lends: it appends its answer after those already there, cannot
//! clear those, fails if it replaces them, and otherwise leaves them as
//! they were when it fails or is dropped.

use std::cell::RefCell;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::task::Poll;

use super::{SEED, runner};
use crate::tests::fake_parts::{
    Bits, CoinGates, Log, Misshape, Misshapen, Profiler, YieldNow, coin_gate,
};
use crate::{
    Adapt, Adapter, Batch, Capabilities, CompositionFaults, Drive, DriveError, Driver, Execute,
    ExecuteError, Executor, Failure, Instruction, InstructionSet, Needs, OpcodeMap, ReadoutValue,
    Readouts, Runner, Seed, ShapeFault, ShotConditions, ShotId, ShotSet,
};

/// How a [`Lender`] waits for its `execute`.
#[derive(Clone, Copy)]
enum Waits {
    ForTheAnswer,
    /// Polls it once, then drops it.
    ForOnePoll,
}

/// What a [`Lender`] saw: its readouts before its `execute`, what the
/// `execute` gave, if it waited for it, and its readouts after.
struct Seen {
    before: Readouts,
    result: Option<Result<(), ExecuteError>>,
    after: Readouts,
}

/// A driver that lends readouts already holding a batch of its own to one
/// `execute` of `x` and `mz` on qubit 0 for every shot, waits for it as
/// [`Waits`] says, and records what it saw; it ends the run with no error
/// whatever the `execute` gave.
struct Lender {
    instructions: Vec<Instruction>,
    waits: Waits,
    seen: Rc<RefCell<Option<Seen>>>,
}

impl Lender {
    fn new(waits: Waits, seen: &Rc<RefCell<Option<Seen>>>) -> Self {
        Self {
            instructions: vec![coin_gate("x", 0), coin_gate("mz", 0)],
            waits,
            seen: Rc::clone(seen),
        }
    }
}

impl Drive for Lender {
    type Output = ();

    fn sends(&self) -> &InstructionSet {
        static SET: OnceLock<InstructionSet> = OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in &mut self.instructions {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<()>, DriveError> {
        let mut readouts = Readouts::new();
        readouts
            .push_with_capacity(1, 2)
            .push_row(
                &[ReadoutValue::Zero, ReadoutValue::One],
                ShotConditions::NONE,
            )
            .expect("a row of two values");
        let before = readouts.clone();
        let batch = Batch::new(shots, &self.instructions);
        let result = {
            let mut execute =
                pin!(below.execute(std::slice::from_ref(&batch), seed, &mut readouts));
            match self.waits {
                Waits::ForTheAnswer => Some(execute.await),
                Waits::ForOnePoll => {
                    poll_fn(|context| {
                        assert!(
                            execute.as_mut().poll(context).is_pending(),
                            "the `execute` waits past its first poll"
                        );
                        Poll::Ready(())
                    })
                    .await;
                    None
                }
            }
        };
        *self.seen.borrow_mut() = Some(Seen {
            before,
            result,
            after: readouts,
        });
        Ok(vec![(); shots.len()])
    }
}

/// How an [`Unruly`] executor breaks the rules of the readouts it is lent.
#[derive(Clone, Copy, Debug)]
enum Unruliness {
    /// Appends its answer, then fails.
    FailsAfterAppending,
    /// Appends its answer, then panics.
    PanicsAfterAppending,
    /// Appends its answer, then waits a poll before it answers.
    WaitsAfterAppending,
    /// Clears the readouts it is lent, then appends its answer.
    ClearsFirst,
    /// Replaces the readouts it is lent with new, empty ones, and appends
    /// nothing.
    ReplacesThem,
    /// Replaces the readouts it is lent with new ones holding copies of
    /// those it found, then appends its answer.
    ReplacesThemWithCopies,
    /// Swaps in [`readouts_from_elsewhere`] for the readouts it is lent,
    /// as many, then appends its answer.
    SwapsInOthers,
    /// As `SwapsInOthers` at its first `execute`, then obeys.
    SwapsInOthersOnce,
    /// Breaks no rule: appends its answer.
    Obeys,
}

/// Readouts of one batch, of one row `[One, One]`, which no lender holds.
fn readouts_from_elsewhere() -> Readouts {
    let mut readouts = Readouts::new();
    readouts
        .push_with_capacity(1, 2)
        .push_row(
            &[ReadoutValue::One, ReadoutValue::One],
            ShotConditions::NONE,
        )
        .expect("a row of two values");
    readouts
}

/// [`Bits`] breaking the rules of the readouts it is lent, as
/// [`Unruliness`] says.
struct Unruly(Bits, Unruliness);

impl Execute for Unruly {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.0.offers()
    }

    fn provision(&mut self, needs: Needs) {
        self.0.provision(needs);
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        match self.1 {
            Unruliness::ClearsFirst => readouts.clear(),
            Unruliness::ReplacesThem => {
                *readouts = Readouts::new();
                return Ok(());
            }
            Unruliness::ReplacesThemWithCopies => {
                let found = readouts.to_vec();
                *readouts = Readouts::new();
                for batch in found {
                    readouts.push(batch);
                }
            }
            Unruliness::SwapsInOthers => {
                let mut others = readouts_from_elsewhere();
                std::mem::swap(readouts, &mut others);
            }
            Unruliness::SwapsInOthersOnce => {
                let mut others = readouts_from_elsewhere();
                std::mem::swap(readouts, &mut others);
                self.1 = Unruliness::Obeys;
            }
            _ => {}
        }
        self.0.execute(batches, seed, readouts).await?;
        match self.1 {
            Unruliness::FailsAfterAppending => Err(ExecuteError::Failed {
                shot: ShotId::new(0),
                failure: Failure::Part("the device failed".into()),
            }),
            Unruliness::PanicsAfterAppending => panic!("the device broke"),
            Unruliness::WaitsAfterAppending => {
                YieldNow::default().await;
                Ok(())
            }
            Unruliness::ClearsFirst
            | Unruliness::ReplacesThem
            | Unruliness::ReplacesThemWithCopies
            | Unruliness::SwapsInOthers
            | Unruliness::SwapsInOthersOnce
            | Unruliness::Obeys => Ok(()),
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.0.end_shots(shots);
    }
}

/// A [`Profiler`] that sends each `execute` below twice, through the
/// readouts it is lent, clearing the first answer before the second, as an
/// adapter retrying an attempt would.
struct Retries(Profiler);

impl Adapt for Retries {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await?;
        readouts.clear();
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// A [`Profiler`] that, when the `execute` below fails, clears its answer
/// and sends it again, as an adapter recovering from a failure would.
struct RetriesAfterAFailure(Profiler);

impl Adapt for RetriesAfterAFailure {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        if self.0.adapt(batches, seed, below, readouts).await.is_ok() {
            return Ok(());
        }
        readouts.clear();
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// What a [`Lender`] waiting as `waits` saw, on 3 shots, on the chain
/// `below` composes it into.
fn seen_on<D: Drive, E: Execute>(
    waits: Waits,
    below: impl FnOnce(Driver<Lender>) -> Result<Runner<D, E>, CompositionFaults>,
) -> Seen {
    let seen = Rc::new(RefCell::new(None));
    runner(below(Driver::new(Lender::new(waits, &seen))))
        .run(&ShotSet::ids(0..3), SEED)
        .expect("the lender ends the run with no error");
    seen.take().expect("the lender ran")
}

/// What a [`Lender`] waiting as `waits` saw, on 3 shots above `executor`.
fn seen<E: Execute>(waits: Waits, executor: E) -> Seen {
    seen_on(waits, |lender| lender >> Executor::new(executor))
}

/// A driver lending readouts that hold a batch of its own: the answer of
/// its `execute` of one batch comes after it, a row per shot, and its own
/// batch is unchanged.
#[test]
fn an_execute_appends_its_answer_after_the_readouts_already_there() {
    let log = Log::default();
    let Seen {
        before,
        result,
        after,
    } = seen(Waits::ForTheAnswer, Bits::new(&log));
    result
        .expect("the lender waited")
        .expect("the execute succeeds");
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0], "the lender's own batch is unchanged");
    assert_eq!(after[1].shots(), 3);
    for shot in 0..3 {
        assert_eq!(after[1].row(shot), [ReadoutValue::One], "shot {shot}");
    }
}

/// A driver lending readouts that hold a batch of its own to an executor
/// that appends its answer, then fails, panics or answers the wrong shape,
/// or to one it stops waiting for: the readouts are as they were, so the
/// driver never reads part of an answer.
#[test]
fn a_failed_or_dropped_execute_leaves_the_lent_readouts_as_they_were() {
    let log = Log::default();
    for (
        row,
        Seen {
            before,
            result,
            after,
        },
        fails,
    ) in [
        (
            "an executor failing after appending",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::FailsAfterAppending),
            ),
            true,
        ),
        (
            "an executor panicking after appending",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::PanicsAfterAppending),
            ),
            true,
        ),
        (
            "an executor answering a row too many",
            seen(Waits::ForTheAnswer, Misshapen(Misshape::ExtraRow)),
            true,
        ),
        (
            "an execute dropped after appending",
            seen(
                Waits::ForOnePoll,
                Unruly(Bits::new(&log), Unruliness::WaitsAfterAppending),
            ),
            false,
        ),
    ] {
        if fails {
            assert!(
                matches!(result, Some(Err(ExecuteError::Failed { .. }))),
                "{row}: {result:?}"
            );
        }
        assert_eq!(after, before, "{row}");
    }
}

/// An executor that clears the readouts it is lent, as if they were its
/// own, before it answers, and an adapter that clears the answer of the
/// part below to send its `execute` again: each `clear` removes only what
/// its `execute` added, so the driver's batch stays, followed by the
/// answer; the driver's own `clear`, after, removes it all.
#[test]
fn clearing_the_lent_readouts_keeps_those_the_execute_found() {
    let log = Log::default();
    let counted = Rc::default();
    for (
        scene,
        Seen {
            before,
            result,
            mut after,
        },
    ) in [
        (
            "an executor clearing what it is lent",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::ClearsFirst),
            ),
        ),
        (
            "an adapter clearing the answer below to send again",
            seen_on(Waits::ForTheAnswer, |lender| {
                lender
                    >> Adapter::new(Retries(Profiler::new(&counted, &log)))
                    >> Executor::new(Bits::new(&log))
            }),
        ),
    ] {
        assert!(matches!(result, Some(Ok(()))), "{scene}: {result:?}");
        assert_eq!(after.len(), 2, "{scene}");
        assert_eq!(after[0], before[0], "{scene}: the driver's batch");
        assert_eq!(after[1].shots(), 3, "{scene}: the answer");
        after.clear();
        assert!(after.is_empty(), "{scene}: the driver's clear");
    }
}

/// Readouts of one batch, a row `[value]` per shot of a [`Lender`]'s.
fn a_row_per_shot_of(value: ReadoutValue) -> Readouts {
    let mut readouts = Readouts::new();
    let batch = readouts.push_with_capacity(3, 1);
    for _ in 0..3 {
        batch
            .push_row(&[value], ShotConditions::NONE)
            .expect("a row of one value");
    }
    readouts
}

/// An executor that replaces the readouts it is lent, which no executor
/// may do: with new, empty ones; with new ones holding copies of those it
/// found, and its answer after them, so that their count is right; with
/// others, as many, swapped in; or swapping in others once, below an
/// adapter that recovers from the failure by clearing and sending again,
/// and answers. Each is a failure, named by the batch's first shot, and
/// its answer is removed: the driver then holds the replacement's readouts
/// as far as those it had, its own if copied.
#[test]
fn an_execute_replacing_the_lent_readouts_is_a_failure() {
    /// The readouts the driver holds after, from those it held before.
    type Held = fn(&Readouts) -> Readouts;
    let log = Log::default();
    let counted = Rc::default();
    let unruly = |unruliness| seen(Waits::ForTheAnswer, Unruly(Bits::new(&log), unruliness));
    let rows: [(&str, Seen, Held); 4] = [
        ("empty ones", unruly(Unruliness::ReplacesThem), |_| {
            Readouts::new()
        }),
        (
            "copies, then the answer",
            unruly(Unruliness::ReplacesThemWithCopies),
            Readouts::clone,
        ),
        (
            "others, swapped in, then the answer",
            unruly(Unruliness::SwapsInOthers),
            |_| readouts_from_elsewhere(),
        ),
        (
            "others, swapped in, below an adapter that recovers",
            seen_on(Waits::ForTheAnswer, |lender| {
                lender
                    >> Adapter::new(RetriesAfterAFailure(Profiler::new(&counted, &log)))
                    >> Executor::new(Unruly(Bits::new(&log), Unruliness::SwapsInOthersOnce))
            }),
            // The recovery's `clear` emptied the replacement, so the driver
            // holds the answer sent again, where each shot's second `x`
            // gave `Zero`.
            |_| a_row_per_shot_of(ReadoutValue::Zero),
        ),
    ];
    for (
        scene,
        Seen {
            before,
            result,
            after,
        },
        held,
    ) in rows
    {
        let Some(Err(ExecuteError::Failed {
            shot,
            failure: Failure::Part(why),
        })) = result
        else {
            panic!("{scene}: a failure of the part: {result:?}");
        };
        assert_eq!(shot, ShotId::new(0), "{scene}");
        assert_eq!(
            why.downcast_ref::<ShapeFault>(),
            Some(&ShapeFault::ReadoutsReplaced),
            "{scene}"
        );
        assert_eq!(after, held(&before), "{scene}: the readouts held after");
    }
}
