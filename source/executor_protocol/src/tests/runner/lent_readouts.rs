// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` through the protocol does with the readouts its
//! caller lends: it appends its answer after those already there, and
//! leaves them as they were when it fails or is dropped.

use std::cell::RefCell;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::task::Poll;

use super::{SEED, runner};
use crate::tests::fake_parts::{Bits, CoinGates, Log, Misshape, Misshapen, YieldNow, coin_gate};
use crate::{
    Batch, Capabilities, Drive, DriveError, Driver, Execute, ExecuteError, Executor, Failure,
    Instruction, InstructionSet, Needs, OpcodeMap, ReadoutValue, Readouts, Seed, ShapeFault,
    ShotConditions, ShotId, ShotSet,
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
            .push_batch(1, 2)
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
        if let Unruliness::ClearsFirst = self.1 {
            readouts.clear();
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
            Unruliness::ClearsFirst => Ok(()),
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.0.end_shots(shots);
    }
}

/// What a [`Lender`] waiting as `waits` saw, on 3 shots above `executor`.
fn seen<E: Execute>(waits: Waits, executor: E) -> Seen {
    let seen = Rc::new(RefCell::new(None));
    runner(Driver::new(Lender::new(waits, &seen)) >> Executor::new(executor))
        .run(&ShotSet::ids(0..3), SEED)
        .expect("the lender ends the run with no error");
    seen.take().expect("the lender ran")
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

/// An executor that clears the readouts it is lent, holding the driver's
/// batch, then appends its answer for the one batch: a failure, since it
/// appended none after the readouts it found.
#[test]
fn an_answer_removing_readouts_it_found_is_a_failure() {
    let log = Log::default();
    let Seen { result, .. } = seen(
        Waits::ForTheAnswer,
        Unruly(Bits::new(&log), Unruliness::ClearsFirst),
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
}
