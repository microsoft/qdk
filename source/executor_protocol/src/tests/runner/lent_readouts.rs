// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` through the protocol does with the readouts its
//! caller lends: it appends its answer after those already there, cannot
//! clear those, and leaves them as they were when it fails or is dropped.

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
        let batch = Batch {
            shots,
            instructions: &self.instructions,
        };
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
            Unruliness::ClearsFirst | Unruliness::ReplacesThem => Ok(()),
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
    let failed = [
        seen(
            Waits::ForTheAnswer,
            Unruly(Bits::new(&log), Unruliness::FailsAfterAppending),
        ),
        seen(
            Waits::ForTheAnswer,
            Unruly(Bits::new(&log), Unruliness::PanicsAfterAppending),
        ),
        seen(Waits::ForTheAnswer, Misshapen(Misshape::ExtraRow)),
    ];
    for (
        case,
        Seen {
            before,
            result,
            after,
        },
    ) in failed.iter().enumerate()
    {
        assert!(
            matches!(result, Some(Err(ExecuteError::Failed { .. }))),
            "case {case}: {result:?}"
        );
        assert_eq!(after, before, "case {case}");
    }
    let dropped = seen(
        Waits::ForOnePoll,
        Unruly(Bits::new(&log), Unruliness::WaitsAfterAppending),
    );
    assert_eq!(dropped.after, dropped.before, "the dropped execute");
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
    let cleared = [
        seen(
            Waits::ForTheAnswer,
            Unruly(Bits::new(&log), Unruliness::ClearsFirst),
        ),
        seen_on(Waits::ForTheAnswer, |lender| {
            lender
                >> Adapter::new(Retries(Profiler::new(&counted, &log)))
                >> Executor::new(Bits::new(&log))
        }),
    ];
    for (
        case,
        Seen {
            before,
            result,
            mut after,
        },
    ) in cleared.into_iter().enumerate()
    {
        assert!(matches!(result, Some(Ok(()))), "case {case}: {result:?}");
        assert_eq!(after.len(), 2, "case {case}");
        assert_eq!(after[0], before[0], "case {case}: the driver's batch");
        assert_eq!(after[1].shots(), 3, "case {case}: the answer");
        after.clear();
        assert!(after.is_empty(), "case {case}: the driver's clear");
    }
}

/// An executor that replaces the readouts it is lent with new, empty ones,
/// which no executor may do: a failure, since it appended none after the
/// readouts the driver had, and the driver's readouts stay readable.
#[test]
fn an_execute_replacing_the_lent_readouts_with_fewer_is_a_failure() {
    let log = Log::default();
    let Seen { result, after, .. } = seen(
        Waits::ForTheAnswer,
        Unruly(Bits::new(&log), Unruliness::ReplacesThem),
    );
    let Some(Err(ExecuteError::Failed {
        failure: Failure::Part(why),
        ..
    })) = result
    else {
        panic!("a failure of the part: {result:?}");
    };
    assert_eq!(
        why.downcast_ref::<ShapeFault>(),
        Some(&ShapeFault::Batches {
            expected: 1,
            got: 0
        })
    );
    assert!(after.is_empty());
}
