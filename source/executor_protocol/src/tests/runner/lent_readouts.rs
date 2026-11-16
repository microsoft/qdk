// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` through the protocol does with the readouts its
//! caller lends: it appends its answer after those already there, and
//! leaves them as they were when it fails or is dropped, also when it
//! points the view it is lent at a buffer of its own.

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
    Adapt, Adapter, AppendOnlyReadouts, Batch, Capabilities, CompositionFaults, Drive, DriveError,
    Driver, Execute, ExecuteError, Executor, Failure, Instruction, InstructionSet, Needs,
    OpcodeMap, ReadoutValue, Readouts, Runner, Seed, ShotConditions, ShotId, ShotSet,
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

/// A driver that lends readouts, `found`, to one `execute` of `x` and `mz`
/// on qubit 0 for every shot, waits for it as [`Waits`] says, and records
/// what it saw; it ends the run with no error whatever the `execute` gave.
struct Lender {
    instructions: Vec<Instruction>,
    found: Readouts,
    waits: Waits,
    seen: Rc<RefCell<Option<Seen>>>,
}

impl Lender {
    fn new(found: Readouts, waits: Waits, seen: &Rc<RefCell<Option<Seen>>>) -> Self {
        Self {
            instructions: vec![coin_gate("x", 0), coin_gate("mz", 0)],
            found,
            waits,
            seen: Rc::clone(seen),
        }
    }
}

/// Readouts holding a batch of a [`Lender`]'s own, of one row
/// `[Zero, One]`.
fn a_batch_of_its_own() -> Readouts {
    let mut readouts = Readouts::new();
    readouts
        .push_with_capacity(1, 2)
        .push_row(
            &[ReadoutValue::Zero, ReadoutValue::One],
            ShotConditions::NONE,
        )
        .expect("a row of two values");
    readouts
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
        let mut readouts = std::mem::take(&mut self.found);
        let before = readouts.clone();
        let batch = Batch::new(shots, &self.instructions);
        let result = {
            let mut lent = readouts.append_only();
            let mut execute = pin!(below.execute(std::slice::from_ref(&batch), seed, &mut lent));
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
    /// As `FailsAfterAppending` at its first `execute`, then obeys.
    FailsAfterAppendingOnce,
    /// Appends its answer, then panics.
    PanicsAfterAppending,
    /// Appends its answer, then waits a poll before it answers.
    WaitsAfterAppending,
    /// Points the view it is lent at a buffer of its own, holding
    /// [`readouts_from_elsewhere`], then appends its answer there.
    SwapsInAViewOfItsOwn,
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        if let Unruliness::SwapsInAViewOfItsOwn = self.1 {
            *readouts = Box::leak(Box::new(readouts_from_elsewhere())).append_only();
        }
        self.0.execute(batches, seed, readouts).await?;
        match self.1 {
            Unruliness::FailsAfterAppending | Unruliness::FailsAfterAppendingOnce => {
                if let Unruliness::FailsAfterAppendingOnce = self.1 {
                    self.1 = Unruliness::Obeys;
                }
                Err(ExecuteError::Failed {
                    shot: ShotId::new(0),
                    failure: Failure::Part("the device failed".into()),
                })
            }
            Unruliness::PanicsAfterAppending => panic!("the device broke"),
            Unruliness::WaitsAfterAppending => {
                YieldNow::default().await;
                Ok(())
            }
            Unruliness::SwapsInAViewOfItsOwn | Unruliness::Obeys => Ok(()),
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.0.end_shots(shots);
    }
}

/// A [`Profiler`] that, when the `execute` below fails, sends it again,
/// through the readouts it is lent, as an adapter recovering from a
/// failure would.
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        if self.0.adapt(batches, seed, below, readouts).await.is_ok() {
            return Ok(());
        }
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// What a [`Lender`] lending a batch of its own and waiting as `waits`
/// saw, on 3 shots, on the chain `below` composes it into.
fn seen_on<D: Drive, E: Execute>(
    waits: Waits,
    below: impl FnOnce(Driver<Lender>) -> Result<Runner<D, E>, CompositionFaults>,
) -> Seen {
    let seen = Rc::new(RefCell::new(None));
    runner(below(Driver::new(Lender::new(
        a_batch_of_its_own(),
        waits,
        &seen,
    ))))
    .run(&ShotSet::ids(0..3), SEED)
    .expect("the lender ends the run with no error");
    seen.take().expect("the lender ran")
}

/// What a [`Lender`] lending a batch of its own and waiting as `waits`
/// saw, on 3 shots above `executor`.
fn seen<E: Execute>(waits: Waits, executor: E) -> Seen {
    seen_on(waits, |lender| lender >> Executor::new(executor))
}

/// A driver lending readouts that hold a batch of its own: the answer of
/// its `execute` of one batch comes after it, a row per shot, and its own
/// batch is unchanged; also when an adapter between sends the `execute`
/// again, through the readouts it is lent, after the part below failed
/// having appended an answer: the driver then reads only the answer sent
/// again, where each shot's second `x` gave `Zero`.
#[test]
fn an_execute_appends_its_answer_after_the_readouts_already_there() {
    let log = Log::default();
    let counted = Rc::default();
    for (scene, seen, value) in [
        (
            "an executor",
            seen(Waits::ForTheAnswer, Bits::new(&log)),
            ReadoutValue::One,
        ),
        (
            "an adapter sending again after a failure below",
            seen_on(Waits::ForTheAnswer, |lender| {
                lender
                    >> Adapter::new(RetriesAfterAFailure(Profiler::new(&counted, &log)))
                    >> Executor::new(Unruly(Bits::new(&log), Unruliness::FailsAfterAppendingOnce))
            }),
            ReadoutValue::Zero,
        ),
    ] {
        let Seen {
            before,
            result,
            after,
        } = seen;
        result
            .expect("the lender waited")
            .unwrap_or_else(|error| panic!("{scene}: {error:?}"));
        assert_eq!(after.len(), 2, "{scene}");
        assert_eq!(after[0], before[0], "{scene}: the lender's own batch");
        assert_eq!(after[1].shots(), 3, "{scene}");
        for shot in 0..3 {
            assert_eq!(after[1].row(shot), [value], "{scene}: shot {shot}");
        }
    }
}

/// A driver lending readouts that hold a batch of its own to an executor
/// that appends its answer, then fails, panics or answers the wrong shape,
/// to one that points the view it is lent at a buffer of its own and
/// answers there, or to one it stops waiting for: the readouts are as they
/// were, so the driver never reads part of an answer.
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
            "an executor answering in a buffer of its own",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::SwapsInAViewOfItsOwn),
            ),
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
