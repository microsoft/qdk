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
    OpcodeMap, ReadoutValue, Readouts, Runner, Seed, ShapeFault, ShotConditions, ShotId, ShotSet,
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

/// A driver lending readouts that hold a batch of its own to an executor:
/// the answer of its `execute` of one batch comes after it, a row per
/// shot, and its own batch is unchanged.
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
        .expect("the executor answers");
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0], "the lender's own batch");
    assert_eq!(after[1].shots(), 3);
    for shot in 0..3 {
        assert_eq!(after[1].row(shot), [ReadoutValue::One], "shot {shot}");
    }
}

/// What a [`Lender`]'s `execute` gives, in a row of
/// [`a_failed_or_dropped_execute_leaves_the_lent_readouts_as_they_were`].
enum Gives {
    /// The executor's own failure, "the device failed".
    ItsFailure,
    /// A failure for the executor's panic, "the device broke".
    ItsPanic,
    /// A failure for this fault in the answer's shape, naming shot 0.
    Fault(ShapeFault),
    /// The answer to an adapter's resend: each shot's second `x` gives
    /// `Zero`.
    TheAnswerSentAgain,
    /// Nothing: the lender stopped waiting.
    Nothing,
}

impl Gives {
    /// Whether `result` is what this says.
    fn is(&self, result: Option<&Result<(), ExecuteError>>) -> bool {
        match self {
            Self::ItsFailure => matches!(
                result,
                Some(Err(ExecuteError::Failed { failure: Failure::Part(error), .. }))
                    if error.to_string() == "the device failed"
            ),
            Self::ItsPanic => matches!(
                result,
                Some(Err(ExecuteError::Failed { failure: Failure::Panicked { message, .. }, .. }))
                    if message.as_deref() == Some("the device broke")
            ),
            Self::Fault(fault) => matches!(
                result,
                Some(Err(ExecuteError::Failed { shot, failure: Failure::Part(error) }))
                    if *shot == ShotId::new(0)
                        && error.downcast_ref::<ShapeFault>() == Some(fault)
            ),
            Self::TheAnswerSentAgain => matches!(result, Some(Ok(()))),
            Self::Nothing => result.is_none(),
        }
    }
}

/// A driver lending readouts that hold a batch of its own to an executor
/// that appends its answer, then fails, panics or answers the wrong shape,
/// to one that points the view it is lent at a buffer of its own and
/// answers there, or to one it stops waiting for: the readouts are as they
/// were, so the driver never reads part of an answer. Also at a boundary
/// below an adapter: when the executor fails after appending and the
/// adapter sends the `execute` again, through the readouts it is lent, the
/// driver reads only the answer sent again.
#[test]
fn a_failed_or_dropped_execute_leaves_the_lent_readouts_as_they_were() {
    let log = Log::default();
    let counted = Rc::default();
    for (
        row,
        Seen {
            before,
            result,
            after,
        },
        gives,
    ) in [
        (
            "an executor failing after appending",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::FailsAfterAppending),
            ),
            Gives::ItsFailure,
        ),
        (
            "an executor panicking after appending",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::PanicsAfterAppending),
            ),
            Gives::ItsPanic,
        ),
        (
            "an executor answering a row too many",
            seen(Waits::ForTheAnswer, Misshapen(Misshape::ExtraRow)),
            Gives::Fault(ShapeFault::Rows {
                batch: 0,
                expected: 3,
                got: 4,
            }),
        ),
        (
            "an executor answering in a buffer of its own, as readouts missing",
            seen(
                Waits::ForTheAnswer,
                Unruly(Bits::new(&log), Unruliness::SwapsInAViewOfItsOwn),
            ),
            Gives::Fault(ShapeFault::Batches {
                expected: 1,
                got: 0,
            }),
        ),
        (
            "an execute dropped after appending",
            seen(
                Waits::ForOnePoll,
                Unruly(Bits::new(&log), Unruliness::WaitsAfterAppending),
            ),
            Gives::Nothing,
        ),
        (
            "below an adapter sending again, an executor failing after appending",
            seen_on(Waits::ForTheAnswer, |lender| {
                lender
                    >> Adapter::new(RetriesAfterAFailure(Profiler::new(&counted, &log)))
                    >> Executor::new(Unruly(Bits::new(&log), Unruliness::FailsAfterAppendingOnce))
            }),
            Gives::TheAnswerSentAgain,
        ),
    ] {
        assert!(gives.is(result.as_ref()), "{row}: {result:?}");
        let mut expected = before.clone();
        if let Gives::TheAnswerSentAgain = gives {
            let answer = expected.push_with_capacity(3, 1);
            for _ in 0..3 {
                answer
                    .push_row(&[ReadoutValue::Zero], ShotConditions::NONE)
                    .expect("a row of one value");
            }
        }
        assert_eq!(after, expected, "{row}");
    }
}
