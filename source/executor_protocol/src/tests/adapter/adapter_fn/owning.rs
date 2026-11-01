// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An `owning_adapter_fn`'s states own their way below (`OwnedBelow`), as a
//! code's runtime owns the backend it is made over: through it, their
//! logic reaches below by the rules of a lent `Below`, on its own call's
//! shots, or its own shot's; it reaches nothing outside its state's logic,
//! nor inside another owning state's logic that this logic runs, and says
//! why.
//!
//! Where a test runs one logic through a lent way below and through one
//! its state owns, it pins the rule for both adapters: so are the
//! per-shot rules for a panic below that a shot's logic catches
//! (`a_shots_kept_sends_go_below_once_though_one_panics_going_alone` and
//! the three tests after it), which hold for `adapter_fn` under
//! `OnePerShot` as for `owning_adapter_fn`.

use std::cell::{Cell, RefCell};
use std::ops::Shr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;

use super::{Pulse, no_state, pass, pulse_device};
use crate::adapter::below::{INSIDE_ANOTHER_LOGIC, NOT_SERVING, ON_ANOTHER_THREAD};
use crate::tests::executor_fn::panic_text;
use crate::tests::test_double::{run, step};
use crate::tests::{shots, values};
use crate::{
    AdapterFn, Below, Capabilities, Close, Compose, Describe, Executor, ExecutorLogic,
    ImmediateReply, InstructionBatch, InstructionBatchRef, OnePerShot, OnePerShotAdapter, Outcome,
    Outcomes, OwnedBelow, Owning, ProgramInput, Rejected, ShotContext, ShotId, ShotOutcome,
    ShotSet, Value, WithExecutor, adapter_fn, compose, executor_fn, owning_adapter_fn,
};

/// The shots of the target chain's run: not a range.
const SHOTS: [ShotId; 4] = [2, 5, 6, 11];

/// What happened below, in order.
#[derive(Clone, Debug, PartialEq)]
enum Event {
    /// A call, with its shots and its instructions.
    Call(Vec<ShotId>, Vec<bool>),
    /// A state dropped, with its way below.
    Dropped,
    /// The executor below closed.
    Close,
    /// The executor below sent through a way below it was handed, inside
    /// a call, with what it got.
    Reentered(Result<(), String>),
}

/// What happened below, shared with the test; `Send`, so that an owning
/// adapter over the executor that logs it can move to another thread.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<Event>>>);

impl Log {
    fn push(&self, event: Event) {
        (self.0.lock().unwrap_or_else(PoisonError::into_inner)).push(event);
    }

    fn events(&self) -> Vec<Event> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The calls below, without the other events.
    fn calls(&self) -> Vec<Event> {
        let calls = self.events().into_iter();
        calls.filter(|e| matches!(e, Event::Call(..))).collect()
    }
}

/// A `Send` executor of reads, which logs what it does: each `true` of a
/// call reads, for each of its shots, the parity of the shot, tagged with
/// its index; it refuses a call with a `false` when `refuses_false`, fails
/// the calls on shot `failing`, after their readouts, and, after its next
/// `passing` calls, panics at each of its next `panics` calls; inside each
/// call, it sends a read through each way below it was `handed`.
#[derive(Clone, Default)]
struct Downstream {
    log: Log,
    refuses_false: bool,
    failing: Option<ShotId>,
    passing: Arc<AtomicUsize>,
    panics: Arc<AtomicUsize>,
    handed: Arc<Mutex<Vec<OwnedBelow<bool>>>>,
}

impl Downstream {
    /// An executor below that panics at its next `calls` calls.
    fn panicking(calls: usize) -> Self {
        Self::panicking_after(0, calls)
    }

    /// An executor below that panics at `calls` calls after its next
    /// `passing` calls.
    fn panicking_after(passing: usize, calls: usize) -> Self {
        Self {
            passing: Arc::new(AtomicUsize::new(passing)),
            panics: Arc::new(AtomicUsize::new(calls)),
            ..Self::default()
        }
    }
}

impl Executor<InstructionBatch<bool>> for Downstream {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let passes = (self.passing).fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
            left.checked_sub(1)
        });
        let panics = passes.is_err()
            && (self.panics)
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
        assert!(!panics, "the executor below has a bug");
        let skip = batch.instructions.iter().position(|&read| !read);
        if let Some(at) = skip.filter(|_| self.refuses_false) {
            let reason = "not a read".to_owned();
            return Err(Rejected::Instruction { at, reason });
        }
        let call = Event::Call(batch.shots.to_vec(), batch.instructions.to_vec());
        self.log.push(call);
        let mut handed = self.handed.lock().unwrap_or_else(PoisonError::into_inner);
        for below in handed.iter_mut() {
            self.log.push(Event::Reentered(below.send(&[true])));
        }
        let mut read = Vec::new();
        for &shot in batch.shots {
            for (tag, _) in batch.instructions.iter().enumerate().filter(|(_, r)| **r) {
                let tag = u32::try_from(tag).expect("a batch of the tests is short");
                let value = Value::from(shot % 2 == 1);
                read.push(ShotOutcome { shot, tag, value });
            }
        }
        if self.failing.is_some_and(|j| batch.shots.contains(&j)) {
            return Ok(ImmediateReply::failed(read, "lost".to_owned()));
        }
        Ok(ImmediateReply::done(read))
    }
}

impl Describe for Downstream {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for Downstream {
    type Report = ();

    fn close(&mut self) {
        self.log.push(Event::Close);
    }
}

/// `>>` with [`Downstream`] on the right, as an executor's crate
/// implements it.
macro_rules! downstream_by_shr {
    ($adapter:ident) => {
        impl<I, O, S, M, L, C> Shr<Downstream> for $adapter<I, O, S, M, L, C>
        where
            Self: Compose<Downstream, WithExecutor>,
        {
            type Output = <Self as Compose<Downstream, WithExecutor>>::Output;

            fn shr(self, executor: Downstream) -> Self::Output {
                self.compose(executor)
            }
        }
    };
}

downstream_by_shr!(AdapterFn);
downstream_by_shr!(OnePerShotAdapter);

/// The state of an adapter that keeps its way below, and nothing else.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn its_way_below(_: ShotContext, below: OwnedBelow<bool>) -> Result<OwnedBelow<bool>, Rejected> {
    Ok(below)
}

/// The logic of an observer that sends each call down as it is, through
/// its state's way below.
fn pass_owned(
    below: &mut OwnedBelow<bool>,
    input: &ProgramInput<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    below.send(input.instructions())
}

/// The logic of a decoder that runs each call below and writes what it
/// reads, through its state's way below.
fn run_each_owned(
    below: &mut OwnedBelow<bool>,
    input: &ProgramInput<'_, bool>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    outcomes.extend(below.run(input.instructions())?);
    Ok(())
}

/// A shared place where a test keeps the ways below its states give up.
type Kept = Rc<RefCell<Vec<OwnedBelow<bool>>>>;

/// A state closure that gives its way below up to `kept`, and keeps its
/// shot.
fn giving_up(kept: &Kept) -> impl FnMut(ShotContext, OwnedBelow<bool>) -> Result<ShotId, Rejected> {
    let kept = Rc::clone(kept);
    move |context, below| {
        kept.borrow_mut().push(below);
        Ok(context.shot())
    }
}

/// A state closure that gives shot 2's way below up to `kept`, and keeps
/// its shot.
fn giving_up_2s(
    kept: &Kept,
) -> impl FnMut(ShotContext, OwnedBelow<bool>) -> Result<ShotId, Rejected> + use<> {
    let kept = Rc::clone(kept);
    move |context, below| {
        if context.shot() == 2 {
            kept.borrow_mut().push(below);
        }
        Ok(context.shot())
    }
}

/// An observer with one state, on the target chain's four shots, two calls:
/// what its state sends through the way below it owns goes below as what
/// a lent one sends, call for call, on the call's shots, and gives the
/// same readouts.
#[test]
fn owned_sends_go_below_as_lent_ones_do() {
    let lent = Downstream::default();
    let owned = Downstream::default();
    let mut lending = (adapter_fn(no_state, pass) >> lent.clone()).expect("bools fit");
    let mut owning =
        (owning_adapter_fn(its_way_below, pass_owned) >> owned.clone()).expect("bools fit");
    for batch in [&[true, true][..], &[false, true]] {
        let (through_lent, end) =
            run(&mut lending, &shots(&SHOTS), batch).expect("the call is accepted");
        end.expect("the lent call ends well");
        let (through_owned, end) =
            run(&mut owning, &shots(&SHOTS), batch).expect("the call is accepted");
        end.expect("the owned call ends well");
        assert!(!through_lent.is_empty(), "the batch reads");
        assert_eq!(values(&through_owned), values(&through_lent));
    }
    assert_eq!(owned.log.calls(), lent.log.calls());
}

/// A lowering with a state per shot whose even shots read, then skip a
/// bit and read, and odd ones the other way round, on the target chain's
/// four shots: through the ways below their states own, each shot's first
/// sends, then its second, go below merged over the shots that sent them
/// alike, as lent ones do.
#[test]
fn owned_sends_of_each_shot_merge_as_lent_ones_do() {
    const READ: &[bool] = &[true];
    const SKIP_READ: &[bool] = &[false, true];
    fn by_parity(shot: ShotId) -> [&'static [bool]; 2] {
        if shot.is_multiple_of(2) {
            [READ, SKIP_READ]
        } else {
            [SKIP_READ, READ]
        }
    }
    fn send_each(below: &mut Below<'_, bool>, shot: ShotId) -> Result<(), String> {
        by_parity(shot)
            .into_iter()
            .try_for_each(|batch| below.send(batch))
    }
    let lent = Downstream::default();
    let owned = Downstream::default();
    let lowering = adapter_fn(
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId,
         _: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| send_each(below, *shot),
    );
    let owning = owning_adapter_fn(
        |context: ShotContext, below| Ok::<_, Rejected>((context.shot(), below)),
        |(shot, below): &mut (ShotId, OwnedBelow<bool>),
         _: &ProgramInput<'_, bool>,
         _: &mut Outcomes<'_>| {
            let [first, second] = by_parity(*shot);
            below.send(first)?;
            below.send(second)
        },
    );
    let mut lending = (lowering.shots(OnePerShot) >> lent.clone()).expect("bools fit");
    let mut owning = (owning.shots(OnePerShot) >> owned.clone()).expect("bools fit");
    step(&mut lending, &shots(&SHOTS), &[true], "the lent call runs");
    step(&mut owning, &shots(&SHOTS), &[true], "the owned call runs");
    let merged = [
        Event::Call(vec![2, 6], READ.to_vec()),
        Event::Call(vec![5, 11], SKIP_READ.to_vec()),
        Event::Call(vec![2, 6], SKIP_READ.to_vec()),
        Event::Call(vec![5, 11], READ.to_vec()),
    ];
    assert_eq!(lent.log.calls(), merged, "the lent sends merge");
    assert_eq!(owned.log.calls(), merged);
}

/// A decoder with a state per shot, on the target chain's four shots: what
/// each shot's state runs through the way below it owns goes below at
/// once, as a call on that shot alone.
#[test]
fn an_owned_run_goes_below_on_its_shot_alone() {
    let downstream = Downstream::default();
    let decoding = owning_adapter_fn(its_way_below, run_each_owned).shots(OnePerShot);
    let mut executor = (decoding >> downstream.clone()).expect("bools fit");
    step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
    let alone: Vec<_> = (SHOTS.iter())
        .map(|&j| Event::Call(vec![j], vec![true]))
        .collect();
    assert_eq!(downstream.log.calls(), alone);
}

/// A decoder with one state, on the target chain's four shots: what its
/// state runs through the way below it owns goes below as one call on the
/// call's shots, and its logic gets their readouts.
#[test]
fn an_owned_run_with_one_state_goes_below_on_the_calls_shots() {
    let got = Rc::new(RefCell::new(Vec::new()));
    let decoding = {
        let got = Rc::clone(&got);
        owning_adapter_fn(
            its_way_below,
            move |below: &mut OwnedBelow<bool>,
                  input: &ProgramInput<'_, bool>,
                  _: &mut Outcomes<'_>| {
                got.borrow_mut().extend(below.run(input.instructions())?);
                Ok(())
            },
        )
    };
    let downstream = Downstream::default();
    let mut executor = (decoding >> downstream.clone()).expect("bools fit");
    step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
    assert_eq!(
        downstream.log.calls(),
        [Event::Call(SHOTS.to_vec(), vec![true])]
    );
    let parities: Vec<_> = (SHOTS.iter())
        .map(|&j| (j, 0, Value::from(j % 2 == 1)))
        .collect();
    assert_eq!(values(&got.borrow()), parities, "the logic got them");
}

/// A decoder with one state, owning its way below, over a simulator below
/// that reads tags 29 down to 0, on shots 2, 0 and 1: its run gives the
/// readouts by shot, each shot's in the order it read them, as the shot's
/// reply keeps them, as a lent way below's run does, not by tag. The
/// readouts are many, 90, as for the lent way below: the standard
/// library's unstable sort sorts a few by insertion, which keeps their
/// order, so with 6 a sort that is not stable would not be seen.
#[test]
fn an_owned_run_gives_its_readouts_by_shot_each_in_the_order_read() {
    let backwards = executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
            for tag in (0..30).rev() {
                outcomes.push(Outcome::new(tag, Value::One));
            }
            Ok(())
        },
    )
    .shots(OnePerShot);
    let read = Rc::new(RefCell::new(Vec::new()));
    let decoding = {
        let read = Rc::clone(&read);
        owning_adapter_fn(
            its_way_below,
            move |below: &mut OwnedBelow<bool>,
                  input: &ProgramInput<'_, bool>,
                  _: &mut Outcomes<'_>| {
                read.borrow_mut().extend(below.run(input.instructions())?);
                Ok(())
            },
        )
    };
    let mut executor = compose(decoding, backwards).expect("bools fit");
    step(&mut executor, &shots(&[2, 0, 1]), &[true], "the call runs");
    let by_shot: Vec<_> = [0, 1, 2]
        .into_iter()
        .flat_map(|shot| (0..30).rev().map(move |tag| (shot, tag, Value::One)))
        .collect();
    assert_eq!(values(&read.borrow()), by_shot);
}

/// A debugger with a state per shot whose shot 2 gives its way below up,
/// and whose every shot's logic sends a read through it, on the target
/// chain's four shots: the way below reaches below in shot 2's logic only.
#[test]
fn a_shots_way_below_reaches_nothing_in_another_shots_logic() {
    let kept = Kept::default();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let debugging = {
        let (kept, sent) = (Rc::clone(&kept), Rc::clone(&sent));
        owning_adapter_fn(
            giving_up_2s(&kept),
            move |shot: &mut ShotId, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
                let mut kept = kept.borrow_mut();
                let shot_2s = kept.first_mut().expect("shot 2's state gave it up");
                sent.borrow_mut().push((*shot, shot_2s.send(&[true])));
                Ok(())
            },
        )
    };
    let downstream = Downstream::default();
    let mut executor = (debugging.shots(OnePerShot) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &shots(&SHOTS), &[true], "the call runs");
    let refused = Err(NOT_SERVING.to_owned());
    let expected = [
        (2, Ok(())),
        (5, refused.clone()),
        (6, refused.clone()),
        (11, refused),
    ];
    assert_eq!(*sent.borrow(), expected);
    assert_eq!(downstream.log.calls(), [Event::Call(vec![2], vec![true])]);
}

/// A debugger with a state per shot whose shot 2 gives its way below up,
/// over a tracer with one state whose logic sends a read through it, then
/// the call, on shots 2 and 5: inside the run of shot 5's state, the
/// tracer's logic serves, not shot 2's, so shot 2's way below reaches
/// nothing there, and only the tracer's call goes below.
#[test]
fn a_way_below_reaches_nothing_in_the_logic_of_an_adapter_below() {
    let kept = Kept::default();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let debugging = {
        let kept = Rc::clone(&kept);
        owning_adapter_fn(
            move |context: ShotContext, below| {
                if context.shot() == 2 {
                    kept.borrow_mut().push(below);
                    return Ok::<_, Rejected>(None);
                }
                Ok(Some(below))
            },
            |below: &mut Option<OwnedBelow<bool>>,
             input: &ProgramInput<'_, bool>,
             outcomes: &mut Outcomes<'_>| match below {
                Some(below) => run_each_owned(below, input, outcomes),
                None => Ok(()),
            },
        )
    };
    let tracing = {
        let (kept, sent) = (Rc::clone(&kept), Rc::clone(&sent));
        owning_adapter_fn(
            its_way_below,
            move |below: &mut OwnedBelow<bool>,
                  input: &ProgramInput<'_, bool>,
                  outcomes: &mut Outcomes<'_>| {
                let mut kept = kept.borrow_mut();
                let shot_2s = kept.first_mut().expect("shot 2's state gave it up");
                sent.borrow_mut().push(shot_2s.send(&[true]));
                run_each_owned(below, input, outcomes)
            },
        )
    };
    let downstream = Downstream::default();
    let mut executor = compose(
        compose(debugging.shots(OnePerShot), tracing),
        downstream.clone(),
    )
    .expect("bools fit");
    step(&mut executor, &shots(&[2, 5]), &[true], "the call runs");
    assert_eq!(*sent.borrow(), [Err(NOT_SERVING.to_owned())]);
    assert_eq!(downstream.log.calls(), [Event::Call(vec![5], vec![true])]);
}

/// A runtime with one state that gives its way below up, and whose logic
/// consults a chain of its own, an owning tracer over another executor,
/// whose logic sends a read through the runtime's way below: inside the
/// tracer's logic, the tracer's state serves, not the runtime's, so the
/// runtime's way below reaches nothing there, and says so; nothing goes
/// below either chain but the tracer's call.
#[test]
fn a_way_below_reaches_nothing_in_another_chains_logic_its_logic_runs() {
    let kept = Kept::default();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let consulted = Downstream::default();
    let tracer = {
        let (kept, sent) = (Rc::clone(&kept), Rc::clone(&sent));
        owning_adapter_fn(
            its_way_below,
            move |below: &mut OwnedBelow<bool>,
                  input: &ProgramInput<'_, bool>,
                  outcomes: &mut Outcomes<'_>| {
                let mut kept = kept.borrow_mut();
                let runtime_below = kept.first_mut().expect("the runtime's state gave it up");
                sent.borrow_mut().push(runtime_below.send(&[true]));
                run_each_owned(below, input, outcomes)
            },
        )
    };
    let tracing = Rc::new(RefCell::new(
        (tracer >> consulted.clone()).expect("bools fit"),
    ));
    let consulting = {
        let tracing = Rc::clone(&tracing);
        move |_: &mut ShotId, input: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let (_, end) = run(
                &mut *tracing.borrow_mut(),
                &ShotSet::one(0),
                input.instructions(),
            )
            .map_err(|refused| refused.to_string())?;
            end
        }
    };
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(giving_up(&kept), consulting) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(3), &[true], "the call runs");
    assert_eq!(*sent.borrow(), [Err(INSIDE_ANOTHER_LOGIC.to_owned())]);
    assert!(downstream.log.calls().is_empty(), "nothing went below");
    assert_eq!(consulted.log.calls(), [Event::Call(vec![0], vec![true])]);
}

/// A runtime with one state hands its way below to the executor below a
/// chain its logic consults, an owning tracer with a state per shot that
/// sends each call down: the tracer's sends merge into one, made after its
/// logics return, so the executor below it sends through the runtime's
/// way below outside them, and reaches below.
#[test]
fn a_way_below_reaches_below_from_a_merged_send_its_logic_runs() {
    let consulted = Downstream::default();
    let handed = Arc::clone(&consulted.handed);
    let tracer = owning_adapter_fn(its_way_below, pass_owned).shots(OnePerShot);
    let mut tracing = (tracer >> consulted.clone()).expect("bools fit");
    let consulting = move |(): &mut (), input: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
        let (_, end) = run(&mut tracing, &shots(&[0, 1]), input.instructions())
            .map_err(|refused| refused.to_string())?;
        end
    };
    let handing = move |_: ShotContext, below: OwnedBelow<bool>| {
        (handed.lock().unwrap_or_else(PoisonError::into_inner)).push(below);
        Ok::<_, Rejected>(())
    };
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(handing, consulting) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(3), &[true], "the call runs");
    assert_eq!(
        consulted.log.events(),
        [
            Event::Call(vec![0, 1], vec![true]),
            Event::Reentered(Ok(()))
        ]
    );
    assert_eq!(downstream.log.calls(), [Event::Call(vec![3], vec![true])]);
}

/// The executor below refuses a skip, and a decoder with one state runs
/// one first: its run is refused, and the call is refused with the
/// refusal below, nothing having gone down.
#[test]
fn an_owned_run_refused_first_refuses_the_call() {
    let downstream = Downstream {
        refuses_false: true,
        ..Downstream::default()
    };
    let mut decoded =
        (owning_adapter_fn(its_way_below, run_each_owned) >> downstream).expect("bools fit");
    let refused = run(&mut decoded, &ShotSet::one(0), &[false]).expect_err("refused below");
    let reason = "not a read".to_owned();
    assert_eq!(refused, Rejected::Instruction { at: 0, reason });
}

/// The executor below refuses a skip, and a decoder with one state sends a
/// read, then runs a skip: the read stands, so the call is not refused but
/// fails, and so does the adapter.
#[test]
fn an_owned_run_refused_after_a_send_fails_the_call_and_the_adapter() {
    let downstream = Downstream {
        refuses_false: true,
        ..Downstream::default()
    };
    let read_then_skip =
        |below: &mut OwnedBelow<bool>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            below.send(&[true])?;
            below.run(&[false]).map(drop)
        };
    let mut decoded =
        (owning_adapter_fn(its_way_below, read_then_skip) >> downstream).expect("bools fit");
    let (read, end) = run(&mut decoded, &ShotSet::one(0), &[true]).expect("the read went down");
    assert_eq!(read.len(), 1, "the read stands: {read:?}");
    end.expect_err("the run was refused after the read");
    let later = run(&mut decoded, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// The executor below refuses a skip, and a decoder with one state sends
/// one: the send gives its logic the refusal below, as a lent one does, so
/// that the logic need not go on.
#[test]
fn an_owned_send_refused_below_gives_its_logic_the_refusal() {
    let downstream = Downstream {
        refuses_false: true,
        ..Downstream::default()
    };
    let seen = Rc::new(RefCell::new(Vec::new()));
    let skipping = {
        let seen = Rc::clone(&seen);
        move |below: &mut OwnedBelow<bool>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            seen.borrow_mut().push(below.send(&[false]));
            Ok(())
        }
    };
    let mut decoded =
        (owning_adapter_fn(its_way_below, skipping) >> downstream).expect("bools fit");
    run(&mut decoded, &ShotSet::one(0), &[true]).expect_err("the skip is refused below");
    let refusal = Rejected::Instruction {
        at: 0,
        reason: "not a read".to_owned(),
    };
    assert_eq!(*seen.borrow(), [Err(refusal.to_string())]);
}

/// The executor below refuses a skip, and a debugger with a state per shot
/// runs one, ignores the refusal and sends a read: the send gives its
/// logic the refusal its run got, as a lent one does.
#[test]
fn a_shots_owned_send_after_its_refused_run_gives_its_logic_the_refusal() {
    let downstream = Downstream {
        refuses_false: true,
        ..Downstream::default()
    };
    let seen = Rc::new(RefCell::new(Vec::new()));
    let going_on = {
        let seen = Rc::clone(&seen);
        move |below: &mut OwnedBelow<bool>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            below.run(&[false]).expect_err("the skip is refused below");
            seen.borrow_mut().push(below.send(&[true]));
            Ok(())
        }
    };
    let mut debugged = (owning_adapter_fn(its_way_below, going_on).shots(OnePerShot) >> downstream)
        .expect("bools fit");
    run(&mut debugged, &ShotSet::one(0), &[true]).expect_err("the skip is refused below");
    let refusal = Rejected::Instruction {
        at: 0,
        reason: "not a read".to_owned(),
    };
    assert_eq!(*seen.borrow(), [Err(refusal.to_string())]);
}

/// Shot 0's calls fail below, and a decoder with one state ignores its
/// run's failure: the failure reaches its logic, the call fails with it,
/// and so does the adapter.
#[test]
fn an_owned_run_that_fails_below_fails_the_call_and_the_adapter() {
    let downstream = Downstream {
        failing: Some(0),
        ..Downstream::default()
    };
    let seen = Rc::new(RefCell::new(Vec::new()));
    let ignoring = {
        let seen = Rc::clone(&seen);
        move |below: &mut OwnedBelow<bool>, input: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            seen.borrow_mut()
                .push(below.run(input.instructions()).map(drop));
            Ok(())
        }
    };
    let mut decoded =
        (owning_adapter_fn(its_way_below, ignoring) >> downstream).expect("bools fit");
    let (_, end) = run(&mut decoded, &ShotSet::one(0), &[true]).expect("the call is accepted");
    assert_eq!(*seen.borrow(), [Err("lost".to_owned())], "the logic saw it");
    assert_eq!(end, Err("lost".to_owned()));
    let later = run(&mut decoded, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A debugger with a state per shot whose shot 2 gives its way below up,
/// and whose every shot's logic sends a read through it and returns what
/// it gets, on shots 2 and 5: shot 5's logic returns the refusal, which
/// fails the call and the adapter, as any `Err` of a logic does.
#[test]
fn a_per_shot_logic_that_returns_a_refusal_fails_the_call_and_the_adapter() {
    let kept = Kept::default();
    let debugging = {
        let kept = Rc::clone(&kept);
        owning_adapter_fn(
            giving_up_2s(&kept),
            move |_: &mut ShotId, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
                let mut kept = kept.borrow_mut();
                let shot_2s = kept.first_mut().expect("shot 2's state gave it up");
                shot_2s.send(&[true])
            },
        )
    };
    let mut executor = (debugging.shots(OnePerShot) >> Downstream::default()).expect("bools fit");
    let (_, end) = run(&mut executor, &shots(&[2, 5]), &[true]).expect("the call is accepted");
    assert_eq!(end, Err(NOT_SERVING.to_owned()));
    let later = run(&mut executor, &ShotSet::one(2), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A runtime with one state whose logic hands its way below to a thread of
/// its own, which sends the call through it, and returns what the thread
/// got: the logic returns the refusal, which fails the call and the
/// adapter, as any `Err` of a logic does.
#[test]
fn a_one_state_logic_that_returns_a_refusal_fails_the_call_and_the_adapter() {
    let handing_off =
        |below: &mut OwnedBelow<bool>, input: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let instructions = input.instructions();
            thread::scope(|scope| scope.spawn(|| below.send(instructions)).join())
                .expect("the thread ends well")
        };
    let mut executor = (owning_adapter_fn(its_way_below, handing_off) >> Downstream::default())
        .expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("the call is accepted");
    assert_eq!(end, Err(ON_ANOTHER_THREAD.to_owned()));
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A state that owns its way below and logs when it is dropped, as a
/// runtime that flushes its backend at its end would.
struct Logged {
    below: OwnedBelow<bool>,
    log: Log,
}

impl Drop for Logged {
    fn drop(&mut self) {
        self.log.push(Event::Dropped);
    }
}

/// A logged state's logic: sends each call down.
fn pass_logged(
    state: &mut Logged,
    input: &ProgramInput<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    state.below.send(input.instructions())
}

/// A runtime with one state that owns its way below, closed after a call:
/// the state, with its way below, is dropped before the executor below
/// closes.
#[test]
fn closing_drops_the_state_and_its_way_below_before_closing_below() {
    let downstream = Downstream::default();
    let log = downstream.log.clone();
    let runtime = owning_adapter_fn(
        move |_: ShotContext, below| {
            let log = log.clone();
            Ok::<_, Rejected>(Logged { below, log })
        },
        pass_logged,
    );
    let mut executor = (runtime >> downstream.clone()).expect("bools fit");
    step(&mut executor, &shots(&[0, 1]), &[true], "the call runs");
    executor.close();
    let events = downstream.log.events();
    assert_eq!(events[1..], [Event::Dropped, Event::Close], "{events:?}");
}

/// A runtime with a state per shot, each owning its way below, closed
/// after a call on two shots: both states, with their ways below, are
/// dropped before the executor below closes.
#[test]
fn closing_drops_each_shots_state_and_its_way_below_before_closing_below() {
    let downstream = Downstream::default();
    let log = downstream.log.clone();
    let runtime = owning_adapter_fn(
        move |_: ShotContext, below| {
            let log = log.clone();
            Ok::<_, Rejected>(Logged { below, log })
        },
        pass_logged,
    );
    let mut executor = (runtime.shots(OnePerShot) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &shots(&[0, 1]), &[true], "the call runs");
    executor.close();
    let events = downstream.log.events();
    let closing = [Event::Dropped, Event::Dropped, Event::Close];
    assert_eq!(events[1..], closing, "{events:?}");
}

/// A state closure that sends a read through the way below it is given,
/// before its state serves any call: the send reaches nothing, and says
/// that the state serves no call; nothing goes below.
#[test]
fn a_way_below_reaches_nothing_from_its_state_closure() {
    let sent = Rc::new(RefCell::new(Vec::new()));
    let eager = {
        let sent = Rc::clone(&sent);
        move |_: ShotContext, mut below: OwnedBelow<bool>| {
            sent.borrow_mut().push(below.send(&[true]));
            Ok::<_, Rejected>(below)
        }
    };
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(eager, pass_owned) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[], "the call runs");
    assert_eq!(*sent.borrow(), [Err(NOT_SERVING.to_owned())]);
    assert_eq!(downstream.log.calls(), [Event::Call(vec![0], vec![])]);
}

/// The logic of an adapter that does nothing below.
#[expect(clippy::unnecessary_wraps, reason = "the signature a logic has")]
fn idle(_: &mut ShotId, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>) -> Result<(), String> {
    Ok(())
}

/// A state that gives its way below up to the test, which sends a read
/// through it between two calls: the send reaches nothing, and nothing
/// goes below.
#[test]
fn a_way_below_kept_between_calls_reaches_nothing() {
    let kept = Kept::default();
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(giving_up(&kept), idle) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
    let between = kept.borrow_mut()[0].send(&[true]);
    assert_eq!(between, Err(NOT_SERVING.to_owned()));
    assert!(downstream.log.calls().is_empty(), "nothing went below");
}

/// A state that gives its way below up to the test, which sends a read
/// through it from a thread of its own between two calls: its state
/// serves no call, so the send says that, not that it is on another
/// thread, and nothing goes below.
#[test]
fn a_way_below_kept_between_calls_reaches_nothing_from_another_thread() {
    let kept = Kept::default();
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(giving_up(&kept), idle) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
    let mut below = (kept.borrow_mut().pop()).expect("the state gave its way below up");
    let between = thread::scope(|scope| scope.spawn(|| below.send(&[true])).join())
        .expect("the thread ends well");
    assert_eq!(between, Err(NOT_SERVING.to_owned()));
    assert!(downstream.log.calls().is_empty(), "nothing went below");
}

/// A decoder with one state whose first call is refused below, so that its
/// state is dropped, and whose state gives its way below up: at the next
/// call, the logic of the new state sends a read through the dropped
/// state's way below, which reaches nothing, though a state serves.
#[test]
fn a_way_below_of_a_state_dropped_with_a_refused_call_reaches_nothing() {
    let kept = Kept::default();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let decoder = {
        let (kept, sent) = (Rc::clone(&kept), Rc::clone(&sent));
        move |_: &mut ShotId, input: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let mut kept = kept.borrow_mut();
            if let [dropped, _] = &mut kept[..] {
                sent.borrow_mut().push(dropped.send(&[true]));
            }
            let own = kept.last_mut().expect("the serving state gave it up");
            own.run(input.instructions()).map(drop)
        }
    };
    let downstream = Downstream {
        refuses_false: true,
        ..Downstream::default()
    };
    let mut executor =
        (owning_adapter_fn(giving_up(&kept), decoder) >> downstream.clone()).expect("bools fit");
    run(&mut executor, &ShotSet::one(0), &[false]).expect_err("the first call is refused");
    step(
        &mut executor,
        &ShotSet::one(0),
        &[true],
        "the next call runs",
    );
    assert_eq!(kept.borrow().len(), 2, "a state was made for each call");
    assert_eq!(*sent.borrow(), [Err(NOT_SERVING.to_owned())]);
    assert_eq!(downstream.log.calls(), [Event::Call(vec![0], vec![true])]);
}

/// A decoder with one state that gives its way below up, and whose call
/// fails below, so that the adapter fails and drops the state: the test
/// then sends a read through the dropped state's way below, which reaches
/// nothing, and nothing more goes below.
#[test]
fn a_way_below_of_a_state_dropped_with_a_failed_call_reaches_nothing() {
    let kept = Kept::default();
    let decoder = {
        let kept = Rc::clone(&kept);
        move |_: &mut ShotId, input: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let mut kept = kept.borrow_mut();
            let own = kept.first_mut().expect("the serving state gave it up");
            own.run(input.instructions()).map(drop)
        }
    };
    let downstream = Downstream {
        failing: Some(0),
        ..Downstream::default()
    };
    let mut executor =
        (owning_adapter_fn(giving_up(&kept), decoder) >> downstream.clone()).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("the call is accepted");
    end.expect_err("the call fails below");
    let after = kept.borrow_mut()[0].send(&[true]);
    assert_eq!(after, Err(NOT_SERVING.to_owned()));
    assert_eq!(downstream.log.calls(), [Event::Call(vec![0], vec![true])]);
}

/// A runtime with one state that gives its way below up, closed after a
/// call: the test then sends a read through the way below of the state
/// dropped at close, which reaches nothing, and nothing goes below after
/// the close.
#[test]
fn a_way_below_kept_past_close_reaches_nothing() {
    let kept = Kept::default();
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(giving_up(&kept), idle) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
    executor.close();
    let after = kept.borrow_mut()[0].send(&[true]);
    assert_eq!(after, Err(NOT_SERVING.to_owned()));
    assert_eq!(downstream.log.events().last(), Some(&Event::Close));
}

/// A runtime that flushes a read below at its end, as one that buffers
/// would.
struct FlushingAtItsEnd {
    below: OwnedBelow<bool>,
    flushed: Rc<RefCell<Vec<Result<(), String>>>>,
}

impl Drop for FlushingAtItsEnd {
    fn drop(&mut self) {
        let flushed = self.below.send(&[true]);
        self.flushed.borrow_mut().push(flushed);
    }
}

/// A runtime with one state that flushes at its end, closed after a call:
/// its state's `Drop` is past the state's life, so the flush reaches
/// nothing, and only the call goes below.
#[test]
fn a_states_drop_reaches_nothing_below() {
    let flushed = Rc::new(RefCell::new(Vec::new()));
    let runtimes = {
        let flushed = Rc::clone(&flushed);
        move |_: ShotContext, below| {
            let flushed = Rc::clone(&flushed);
            Ok::<_, Rejected>(FlushingAtItsEnd { below, flushed })
        }
    };
    let pass = |runtime: &mut FlushingAtItsEnd,
                input: &ProgramInput<'_, bool>,
                _: &mut Outcomes<'_>| runtime.below.send(input.instructions());
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(runtimes, pass) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[false], "the call runs");
    executor.close();
    assert_eq!(*flushed.borrow(), [Err(NOT_SERVING.to_owned())]);
    assert_eq!(downstream.log.calls(), [Event::Call(vec![0], vec![false])]);
}

/// A debugger with a state per shot whose shot 2 hands its way below to the
/// executor below, which has a bug: it sends through it inside the call
/// shot 5's state runs through its own. The way below reaches nothing
/// there, and does not wait: the call ends well.
#[test]
fn a_way_below_reaches_nothing_from_inside_a_call_below() {
    let downstream = Downstream::default();
    let handed = Arc::clone(&downstream.handed);
    let debugging = owning_adapter_fn(
        move |context: ShotContext, below: OwnedBelow<bool>| {
            if context.shot() == 2 {
                (handed.lock().unwrap_or_else(PoisonError::into_inner)).push(below);
                return Ok::<_, Rejected>(None);
            }
            Ok(Some(below))
        },
        |below: &mut Option<OwnedBelow<bool>>,
         input: &ProgramInput<'_, bool>,
         outcomes: &mut Outcomes<'_>| {
            if let Some(below) = below {
                outcomes.extend(below.run(input.instructions())?);
            }
            Ok(())
        },
    );
    let mut executor = (debugging.shots(OnePerShot) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &shots(&[2, 5]), &[true], "the call runs");
    let reentered: Vec<_> = (downstream.log.events().into_iter())
        .filter(|event| matches!(event, Event::Reentered(_)))
        .collect();
    assert_eq!(reentered, [Event::Reentered(Err(NOT_SERVING.to_owned()))]);
}

/// A lowering to pulses, a state per shot owning its way below, on shots
/// 2 and 5: shot 2 sends two pulses down through it, kept to be merged
/// with the other shots' sends, as lent ones are, then shot 5's logic
/// panics (a bug). The logic's panic reaches the caller once the call
/// freed both pulses' copies, each in a catch of its own, rather than
/// abort the process; dropped, the adapter raises no panic.
#[test]
fn a_failed_calls_owned_sends_kept_are_freed_raising_no_panic_of_their_drops() {
    let released = Rc::new(Cell::new(0));
    let mut lowered = owned_lowering_failing_at_5(&released);
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut lowered, &shots(&[2, 5]), &[true])
    }))
    .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the lowering has a bug"));
    assert_eq!(released.get(), 2, "both copies, freed by the call");
    catch_unwind(AssertUnwindSafe(|| drop(lowered))).expect("a drop raises no panic");
}

/// As
/// [`a_failed_calls_owned_sends_kept_are_freed_raising_no_panic_of_their_drops`],
/// with the adapter dropped as the logic's panic unwinds, as a chain's run
/// drops its executor: the logic's panic reaches the caller, and the
/// process does not abort. The call already freed the copies as the panic
/// left it, so the adapter's drop during the unwind finds none.
#[test]
fn dropped_as_a_panic_unwinds_it_frees_the_owned_sends_kept_raising_none() {
    let released = Rc::new(Cell::new(0));
    let mut lowered = owned_lowering_failing_at_5(&released);
    let panicked = catch_unwind(AssertUnwindSafe(move || {
        run(&mut lowered, &shots(&[2, 5]), &[true])
    }))
    .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the lowering has a bug"));
    assert_eq!(released.get(), 2);
}

/// A lowering to pulses whose states own their way below, whose shot 2
/// sends two pulses down, and whose shot 5's logic panics (a bug), over a
/// [`pulse_device`]; `released` counts the copies' drops.
fn owned_lowering_failing_at_5(
    released: &Rc<Cell<usize>>,
) -> impl Executor<InstructionBatch<bool>> + use<> {
    let pulses = [Pulse::on(0, released), Pulse::on(1, released)];
    let lowering = owning_adapter_fn(
        |context: ShotContext, below: OwnedBelow<Pulse>| Ok((context.shot(), below)),
        move |(shot, below): &mut (ShotId, OwnedBelow<Pulse>),
              _: &ProgramInput<'_, bool>,
              _: &mut Outcomes<'_>| {
            assert!(*shot != 5, "the lowering has a bug");
            below.send(&pulses)
        },
    )
    .shots(OnePerShot);
    compose(lowering, pulse_device()).expect("pulses fit")
}

/// A lowering to pulses, a state per shot owning its way below, sends two
/// pulses down through it in each call, whose copies the adapter keeps
/// until it sends them below: each call frees both once sent, each in a
/// catch of its own, absorbing their drops' panics, as with lent sends;
/// its reply is unharmed, the adapter goes on serving, and its drop
/// raises no panic.
#[test]
fn an_owned_sends_drop_that_panics_after_its_call_ran_leaves_the_adapter_serving() {
    let released = Rc::new(Cell::new(0));
    let pulses = [Pulse::on(0, &released), Pulse::on(1, &released)];
    let lowering = owning_adapter_fn(
        |_: ShotContext, below: OwnedBelow<Pulse>| Ok(below),
        move |below: &mut OwnedBelow<Pulse>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            below.send(&pulses)
        },
    )
    .shots(OnePerShot);
    let mut lowered = compose(lowering, pulse_device()).expect("pulses fit");
    step(&mut lowered, &shots(&[0]), &[true], "the call runs");
    assert_eq!(released.get(), 2, "both copies, freed by the call");
    step(&mut lowered, &shots(&[0]), &[true], "the next call runs");
    assert_eq!(released.get(), 4);
    catch_unwind(AssertUnwindSafe(|| drop(lowered))).expect("a drop raises no panic");
}

/// A logic with a bug sends a read, then panics: the panic reaches the
/// caller, and the executor below is still there to close.
#[test]
fn closing_after_a_logic_panicked_closes_below() {
    let buggy = |below: &mut OwnedBelow<bool>,
                 input: &ProgramInput<'_, bool>,
                 _: &mut Outcomes<'_>|
     -> Result<(), String> {
        below.send(input.instructions())?;
        panic!("the logic has a bug")
    };
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(its_way_below, buggy) >> downstream.clone()).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| {
        run(&mut executor, &ShotSet::one(0), &[true])
    }))
    .expect_err("the panic reaches the caller");
    executor.close();
    assert_eq!(downstream.log.events().last(), Some(&Event::Close));
}

/// The executor below has a bug, and panics at a call a state sends through
/// its way below: the panic reaches the caller, and the executor below is
/// still there to close.
#[test]
fn closing_after_a_panic_below_closes_below() {
    let downstream = Downstream::panicking(1);
    let mut executor =
        (owning_adapter_fn(its_way_below, pass_owned) >> downstream.clone()).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| {
        run(&mut executor, &ShotSet::one(0), &[true])
    }))
    .expect_err("the panic reaches the caller");
    executor.close();
    assert_eq!(downstream.log.events().last(), Some(&Event::Close));
}

/// The executor below has a bug, and panics at the first call; a decoder
/// with one state catches the panic of its first run, then runs again.
/// Through the way below its state owns, as through a lent one, the second
/// run reaches below and gives its readouts, and the call ends well.
#[test]
fn a_way_below_reaches_below_after_its_logic_caught_a_panic_there() {
    fn after_a_caught_panic(
        mut run: impl FnMut() -> Result<Vec<ShotOutcome>, String>,
    ) -> Result<Vec<ShotOutcome>, String> {
        catch_unwind(AssertUnwindSafe(&mut run)).expect_err("the first run below panics");
        run()
    }
    let lent = adapter_fn(
        no_state,
        |(): &mut (),
         input: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         outcomes: &mut Outcomes<'_>| {
            outcomes.extend(after_a_caught_panic(|| below.run(input.instructions()))?);
            Ok(())
        },
    );
    let owned = owning_adapter_fn(
        its_way_below,
        |below: &mut OwnedBelow<bool>,
         input: &ProgramInput<'_, bool>,
         outcomes: &mut Outcomes<'_>| {
            outcomes.extend(after_a_caught_panic(|| below.run(input.instructions()))?);
            Ok(())
        },
    );
    let mut lending = (lent >> Downstream::panicking(1)).expect("bools fit");
    let mut owning = (owned >> Downstream::panicking(1)).expect("bools fit");
    let (through_lent, end) =
        run(&mut lending, &shots(&SHOTS), &[true]).expect("the lent call is accepted");
    end.expect("the lent call ends well");
    let (through_owned, end) =
        run(&mut owning, &shots(&SHOTS), &[true]).expect("the call is accepted");
    end.expect("the call ends well");
    assert_eq!(through_lent.len(), SHOTS.len(), "the lent run read");
    assert_eq!(values(&through_owned), values(&through_lent));
}

/// The executor below has a bug, and panics at its second call; a
/// debugger with a state per shot, on shots 5 and 2: shot 5 sends a read
/// to be merged; shot 2 sends one, runs another and catches its panic,
/// after its send went below alone, then sends and runs again. Through the
/// ways below its states own, as through lent ones, shot 2's later calls
/// go below on it alone, and shot 5's send goes below last, on shot 5.
#[test]
fn a_shots_way_below_reaches_below_after_its_logic_caught_a_panic_there() {
    const SENT_BY_5: &[bool] = &[false];
    const SENT_BY_2: &[bool] = &[true];
    const PANICS: &[bool] = &[true, true];
    const SENT_AFTER: &[bool] = &[true, false];
    const RUN_AFTER: &[bool] = &[false, true];
    let lent = adapter_fn(
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId,
         _: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| {
            if *shot == 5 {
                return below.send(SENT_BY_5);
            }
            below.send(SENT_BY_2)?;
            catch_unwind(AssertUnwindSafe(|| below.run(PANICS))).expect_err("the run panics");
            below.send(SENT_AFTER)?;
            below.run(RUN_AFTER).map(drop)
        },
    );
    let owned = owning_adapter_fn(
        |context: ShotContext, below| Ok::<_, Rejected>((context.shot(), below)),
        |(shot, below): &mut (ShotId, OwnedBelow<bool>),
         _: &ProgramInput<'_, bool>,
         _: &mut Outcomes<'_>| {
            if *shot == 5 {
                return below.send(SENT_BY_5);
            }
            below.send(SENT_BY_2)?;
            catch_unwind(AssertUnwindSafe(|| below.run(PANICS))).expect_err("the run panics");
            below.send(SENT_AFTER)?;
            below.run(RUN_AFTER).map(drop)
        },
    );
    let (lent_below, owned_below) = (
        Downstream::panicking_after(1, 1),
        Downstream::panicking_after(1, 1),
    );
    let mut lending = (lent.shots(OnePerShot) >> lent_below.clone()).expect("bools fit");
    let mut owning = (owned.shots(OnePerShot) >> owned_below.clone()).expect("bools fit");
    step(&mut lending, &shots(&[5, 2]), &[true], "the lent call runs");
    step(&mut owning, &shots(&[5, 2]), &[true], "the call runs");
    let on = |shot, sent: &[bool]| Event::Call(vec![shot], sent.to_vec());
    let lent_calls = [
        on(2, SENT_BY_2),
        on(2, SENT_AFTER),
        on(2, RUN_AFTER),
        on(5, SENT_BY_5),
    ];
    assert_eq!(lent_below.log.calls(), lent_calls, "the lent calls below");
    assert_eq!(owned_below.log.calls(), lent_calls);
}

/// The executor below has a bug, and panics at its second call; a
/// debugger with a state per shot, on shots 5 and 2: shot 5 sends a read
/// to be merged; shot 2 sends two, then runs, and catches the panic of its
/// second send as it goes below alone, then sends and runs again. Through
/// lent ways below, and ways its states own, shot 2's first send went
/// below once, before the panic, and goes no more: its later calls below
/// are its later send and run, then shot 5's send goes below.
#[test]
fn a_shots_kept_sends_go_below_once_though_one_panics_going_alone() {
    const SENT_BY_5: &[bool] = &[false];
    const FIRST: &[bool] = &[true];
    const PANICS: &[bool] = &[true, true];
    const RUN: &[bool] = &[true, true, true];
    const SENT_AFTER: &[bool] = &[true, false];
    const RUN_AFTER: &[bool] = &[false, true];
    let lent = adapter_fn(
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId,
         _: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| {
            if *shot == 5 {
                return below.send(SENT_BY_5);
            }
            below.send(FIRST)?;
            below.send(PANICS)?;
            catch_unwind(AssertUnwindSafe(|| below.run(RUN))).expect_err("a send alone panics");
            below.send(SENT_AFTER)?;
            below.run(RUN_AFTER).map(drop)
        },
    );
    let owned = owning_adapter_fn(
        |context: ShotContext, below| Ok::<_, Rejected>((context.shot(), below)),
        |(shot, below): &mut (ShotId, OwnedBelow<bool>),
         _: &ProgramInput<'_, bool>,
         _: &mut Outcomes<'_>| {
            if *shot == 5 {
                return below.send(SENT_BY_5);
            }
            below.send(FIRST)?;
            below.send(PANICS)?;
            catch_unwind(AssertUnwindSafe(|| below.run(RUN))).expect_err("a send alone panics");
            below.send(SENT_AFTER)?;
            below.run(RUN_AFTER).map(drop)
        },
    );
    let (lent_below, owned_below) = (
        Downstream::panicking_after(1, 1),
        Downstream::panicking_after(1, 1),
    );
    let mut lending = (lent.shots(OnePerShot) >> lent_below.clone()).expect("bools fit");
    let mut owning = (owned.shots(OnePerShot) >> owned_below.clone()).expect("bools fit");
    step(&mut lending, &shots(&[5, 2]), &[true], "the lent call runs");
    step(&mut owning, &shots(&[5, 2]), &[true], "the call runs");
    let on = |shot, sent: &[bool]| Event::Call(vec![shot], sent.to_vec());
    let lent_calls = [
        on(2, FIRST),
        on(2, SENT_AFTER),
        on(2, RUN_AFTER),
        on(5, SENT_BY_5),
    ];
    assert_eq!(lent_below.log.calls(), lent_calls, "the lent calls below");
    assert_eq!(owned_below.log.calls(), lent_calls);
}

/// The executor below has a bug, and panics at its second call; a
/// debugger with a state per shot, on shot 2, sends a read, then sends
/// another in a catch, then sends and runs again. Through lent ways below,
/// and ways its states own, the send in the catch is kept, and its panic
/// below comes at the run, which the catch does not hold: the call
/// panics, after the first send went below.
#[test]
fn a_kept_sends_panic_below_comes_at_its_shots_run_not_at_the_send() {
    fn a_catch_around_a_send(
        mut send: impl FnMut(&[bool]) -> Result<(), String>,
        mut run: impl FnMut(&[bool]) -> Result<(), String>,
    ) -> Result<(), String> {
        send(&[true])?;
        catch_unwind(AssertUnwindSafe(|| send(&[true, true])))
            .expect("the send is kept, so nothing panics there")?;
        send(&[true, false])?;
        run(&[false, true])
    }
    let lent = adapter_fn(
        no_state,
        |(): &mut (),
         _: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| {
            let below = RefCell::new(below);
            a_catch_around_a_send(
                |sent| below.borrow_mut().send(sent),
                |ran| below.borrow_mut().run(ran).map(drop),
            )
        },
    );
    let owned = owning_adapter_fn(
        its_way_below,
        |below: &mut OwnedBelow<bool>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let below = RefCell::new(below);
            a_catch_around_a_send(
                |sent| below.borrow_mut().send(sent),
                |ran| below.borrow_mut().run(ran).map(drop),
            )
        },
    );
    let (lent_below, owned_below) = (
        Downstream::panicking_after(1, 1),
        Downstream::panicking_after(1, 1),
    );
    let mut lending = (lent.shots(OnePerShot) >> lent_below.clone()).expect("bools fit");
    let mut owning = (owned.shots(OnePerShot) >> owned_below.clone()).expect("bools fit");
    let through_lent = catch_unwind(AssertUnwindSafe(|| {
        step(&mut lending, &shots(&[2]), &[true], "the lent call");
    }))
    .expect_err("the lent run panics");
    let through_owned = catch_unwind(AssertUnwindSafe(|| {
        step(&mut owning, &shots(&[2]), &[true], "the call");
    }))
    .expect_err("the run panics");
    assert_eq!(
        (panic_text(&*through_lent), panic_text(&*through_owned)),
        (
            Some("the executor below has a bug"),
            Some("the executor below has a bug")
        ),
        "the panics raised"
    );
    let calls = [Event::Call(vec![2], vec![true])];
    assert_eq!(lent_below.log.calls(), calls, "the lent calls below");
    assert_eq!(owned_below.log.calls(), calls);
}

/// The executor below has a bug, and panics at its first call; a debugger
/// with a state per shot, on shots 5 and 2, whose shots each send a read,
/// and whose shot 2 then runs, and catches the panic of its send as it
/// goes below alone: through lent ways below, and ways its states own,
/// the call's end sends the read below on shot 5 alone, not again on
/// shot 2.
#[test]
fn a_shots_send_that_panicked_going_alone_is_not_sent_as_the_call_ends() {
    const SENT: &[bool] = &[true];
    const RUN: &[bool] = &[true, true];
    let lent = adapter_fn(
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId,
         _: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| {
            below.send(SENT)?;
            if *shot == 2 {
                catch_unwind(AssertUnwindSafe(|| below.run(RUN))).expect_err("the send panics");
            }
            Ok(())
        },
    );
    let owned = owning_adapter_fn(
        |context: ShotContext, below| Ok::<_, Rejected>((context.shot(), below)),
        |(shot, below): &mut (ShotId, OwnedBelow<bool>),
         _: &ProgramInput<'_, bool>,
         _: &mut Outcomes<'_>| {
            below.send(SENT)?;
            if *shot == 2 {
                catch_unwind(AssertUnwindSafe(|| below.run(RUN))).expect_err("the send panics");
            }
            Ok(())
        },
    );
    let (lent_below, owned_below) = (Downstream::panicking(1), Downstream::panicking(1));
    let mut lending = (lent.shots(OnePerShot) >> lent_below.clone()).expect("bools fit");
    let mut owning = (owned.shots(OnePerShot) >> owned_below.clone()).expect("bools fit");
    step(&mut lending, &shots(&[5, 2]), &[true], "the lent call runs");
    step(&mut owning, &shots(&[5, 2]), &[true], "the call runs");
    let lent_calls = [Event::Call(vec![5], SENT.to_vec())];
    assert_eq!(lent_below.log.calls(), lent_calls, "the lent calls below");
    assert_eq!(owned_below.log.calls(), lent_calls);
}

/// The executor below has a bug, and panics at its second call; a
/// debugger, on shot 2, sends a read, then, in a catch, sends two more
/// and runs, then sends and runs again. With one state, its sends go below
/// at once: the first send, then the second panics, so the third is never
/// made. With a state per shot, through lent ways below and ways its
/// states own, the calls below are the same: the third send, kept when
/// the second panicked going alone, is dropped unsent.
#[test]
fn a_shots_calls_below_around_a_caught_panic_are_a_one_state_adapters() {
    fn sends_around_a_caught_panic(
        mut send: impl FnMut(&[bool]) -> Result<(), String>,
        mut run: impl FnMut(&[bool]) -> Result<(), String>,
    ) -> Result<(), String> {
        send(&[true])?;
        catch_unwind(AssertUnwindSafe(|| {
            send(&[true, true])?;
            send(&[true, true, true, true])?;
            run(&[true, true, true])
        }))
        .expect_err("the second send panics below");
        send(&[true, false])?;
        run(&[false, true])
    }
    // Both ways below are needed by both closures, through one borrow.
    let lent_logic = |below: &mut Below<'_, bool>| {
        let below = RefCell::new(below);
        sends_around_a_caught_panic(
            |sent| below.borrow_mut().send(sent),
            |ran| below.borrow_mut().run(ran).map(drop),
        )
    };
    let one_state = adapter_fn(
        no_state,
        move |(): &mut (),
              _: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| lent_logic(below),
    );
    let lent = adapter_fn(
        no_state,
        move |(): &mut (),
              _: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| lent_logic(below),
    );
    let owned = owning_adapter_fn(
        its_way_below,
        |below: &mut OwnedBelow<bool>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let below = RefCell::new(below);
            sends_around_a_caught_panic(
                |sent| below.borrow_mut().send(sent),
                |ran| below.borrow_mut().run(ran).map(drop),
            )
        },
    );
    let below = || Downstream::panicking_after(1, 1);
    let (one_state_below, lent_below, owned_below) = (below(), below(), below());
    let mut with_one = (one_state >> one_state_below.clone()).expect("bools fit");
    let mut lending = (lent.shots(OnePerShot) >> lent_below.clone()).expect("bools fit");
    let mut owning = (owned.shots(OnePerShot) >> owned_below.clone()).expect("bools fit");
    step(
        &mut with_one,
        &shots(&[2]),
        &[true],
        "the one-state call runs",
    );
    step(&mut lending, &shots(&[2]), &[true], "the lent call runs");
    step(&mut owning, &shots(&[2]), &[true], "the call runs");
    let on_2 = |sent: &[bool]| Event::Call(vec![2], sent.to_vec());
    let one_state_calls = [on_2(&[true]), on_2(&[true, false]), on_2(&[false, true])];
    assert_eq!(
        one_state_below.log.calls(),
        one_state_calls,
        "with one state"
    );
    assert_eq!(
        lent_below.log.calls(),
        one_state_calls,
        "the lent calls below"
    );
    assert_eq!(owned_below.log.calls(), one_state_calls);
}

/// A runtime whose logic hands the way below its state owns to a thread of
/// its own, which sends a read through it while the logic waits: the way
/// below reaches nothing there, and says that its state serves on another
/// thread; nothing goes below.
#[test]
fn a_way_below_reaches_nothing_from_another_thread_in_its_logic() {
    let sent = Rc::new(RefCell::new(Vec::new()));
    let handing_off = {
        let sent = Rc::clone(&sent);
        move |below: &mut OwnedBelow<bool>, _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| {
            let elsewhere = thread::scope(|scope| scope.spawn(|| below.send(&[true])).join());
            sent.borrow_mut()
                .push(elsewhere.expect("the thread ends well"));
            Ok(())
        }
    };
    let downstream = Downstream::default();
    let mut executor =
        (owning_adapter_fn(its_way_below, handing_off) >> downstream.clone()).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
    assert_eq!(*sent.borrow(), [Err(ON_ANOTHER_THREAD.to_owned())]);
    assert!(downstream.log.calls().is_empty(), "nothing went below");
}

/// A runtime that leaves its way below in a thread-local of its thread,
/// whose destructor sends a read through it when the thread ends, after
/// the crate's own thread-locals may be gone: the send reaches nothing,
/// and is refused with an `Err`, as past its state's life, not a panic.
#[test]
fn a_way_below_reaches_nothing_from_a_thread_locals_destructor() {
    struct Left {
        below: OwnedBelow<bool>,
        sent: mpsc::Sender<Option<Result<(), String>>>,
    }
    impl Drop for Left {
        fn drop(&mut self) {
            let sent = catch_unwind(AssertUnwindSafe(|| self.below.send(&[true])));
            self.sent
                .send(sent.ok())
                .expect("the test waits for the send");
        }
    }
    thread_local! {
        static LEFT: RefCell<Option<Left>> = const { RefCell::new(None) };
    }
    let (sender, sent) = mpsc::channel();
    let downstream = Downstream::default();
    let below = downstream.clone();
    thread::spawn(move || {
        // Reached first, so that its destructor runs last where the
        // platform runs them in reverse.
        LEFT.with_borrow_mut(|left| *left = None);
        let leaving = move |_: ShotContext, below| {
            let sent = sender.clone();
            LEFT.with_borrow_mut(|left| *left = Some(Left { below, sent }));
            Ok::<_, Rejected>(())
        };
        let idle = |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(());
        let mut executor = (owning_adapter_fn(leaving, idle) >> below).expect("bools fit");
        step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
    })
    .join()
    .expect("the thread ends well");
    let sent = sent.recv().expect("the destructor sends");
    assert_eq!(sent, Some(Err(NOT_SERVING.to_owned())), "None: it panicked");
    assert!(downstream.log.calls().is_empty(), "nothing went below");
}

/// An observer's executor kept by a thread-local of its thread, whose
/// destructor calls it, then closes it, when the thread ends, after the
/// crate's own thread-locals may be gone: the call may panic, as a
/// thread-local it needs is gone, but the executor keeps the executor
/// below, and its close closes it.
#[test]
fn closing_after_a_call_from_a_thread_locals_destructor_closes_below() {
    struct Ending(Option<Box<dyn FnOnce()>>);
    impl Drop for Ending {
        fn drop(&mut self) {
            if let Some(end) = self.0.take() {
                end();
            }
        }
    }
    thread_local! {
        static ENDING: RefCell<Ending> = const { RefCell::new(Ending(None)) };
    }
    let (sender, closed) = mpsc::channel();
    let downstream = Downstream::default();
    let below = downstream.clone();
    thread::spawn(move || {
        // Reached first, so that its destructor runs last where the
        // platform runs them in reverse.
        ENDING.with_borrow_mut(|_| {});
        let mut executor =
            (owning_adapter_fn(its_way_below, pass_owned) >> below).expect("bools fit");
        step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
        let end = move || {
            let call = AssertUnwindSafe(|| run(&mut executor, &ShotSet::one(1), &[true]).map(drop));
            drop(catch_unwind(call));
            let closed = catch_unwind(AssertUnwindSafe(|| executor.close()));
            sender
                .send(closed.is_ok())
                .expect("the test waits for the close");
        };
        ENDING.with_borrow_mut(|ending| ending.0 = Some(Box::new(end)));
    })
    .join()
    .expect("the thread ends well");
    assert!(
        closed.recv().expect("the destructor closes"),
        "the close panicked"
    );
    assert_eq!(downstream.log.events().last(), Some(&Event::Close));
}

/// A shot's runtime, as a code's is: a plan its shots share by `Rc`, so
/// that it is not `Send`, and the way below it owns.
struct Runtime {
    plan: Rc<[bool]>,
    below: OwnedBelow<bool>,
}

/// A runtime's logic: runs its plan, then the call, below, and writes what
/// they read.
fn run_planned(
    runtime: &mut Runtime,
    input: &ProgramInput<'_, bool>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    let lowered: Vec<bool> = (runtime.plan.iter().chain(input.instructions()))
        .copied()
        .collect();
    outcomes.extend(runtime.below.run(&lowered)?);
    Ok(())
}

/// An adapter of runtimes with a state per shot, as a code's crate makes
/// one, in a function of its own: it returns the facility's type, each
/// closure opaque in an `Owning`, the logic named by its role. Its shots'
/// runtimes share `plan`.
#[expect(
    clippy::type_complexity,
    reason = "the state closure is named by its signature: no role trait names it"
)]
fn code_adapter(
    plan: &[bool],
) -> OnePerShotAdapter<
    bool,
    bool,
    Runtime,
    Owning<impl FnMut(ShotContext, OwnedBelow<bool>) -> Result<Runtime, Rejected> + use<>>,
    Owning<impl ExecutorLogic<bool, Runtime> + use<>>,
> {
    let plan: Rc<[bool]> = plan.into();
    let runtimes = move |_, below| {
        let plan = Rc::clone(&plan);
        Ok(Runtime { plan, below })
    };
    owning_adapter_fn(runtimes, run_planned).shots(OnePerShot)
}

/// Two adapters of runtimes that are not `Send`, a state per shot, each
/// runtime owning its way below, as two codes stacked are, on the target
/// chain's four shots: grouped either way, the stack reaches below with
/// the same calls and reads the same.
#[test]
fn owning_adapters_whose_states_are_not_send_stack_either_way() {
    let top = || code_adapter(&[true]);
    let middle = || code_adapter(&[false]);
    let (left, right) = (Downstream::default(), Downstream::default());
    let mut grouped_left = compose(compose(top(), middle()), left.clone()).expect("bools fit");
    let mut grouped_right =
        compose(top(), compose(middle(), right.clone()).expect("bools fit")).expect("bools fit");
    let (read_left, end) =
        run(&mut grouped_left, &shots(&SHOTS), &[true]).expect("the call is accepted");
    end.expect("the call ends well grouped left");
    let (read_right, end) =
        run(&mut grouped_right, &shots(&SHOTS), &[true]).expect("the call is accepted");
    end.expect("the call ends well grouped right");
    let below: Vec<_> = (SHOTS.iter())
        .map(|&j| Event::Call(vec![j], vec![false, true, true]))
        .collect();
    assert_eq!(left.log.calls(), below, "each shot ran both plans below");
    assert_eq!(right.log.calls(), below);
    assert_eq!(read_left.len(), 2 * SHOTS.len(), "two reads per shot");
    assert_eq!(by_shot(&read_right), by_shot(&read_left));
}

/// A tracer whose state is its way below, over an adapter of runtimes that
/// share a plan by `Rc`, so that what is below is not `Send`: the way
/// below the tracer's state is given is `Send` and `Sync` all the same,
/// as it holds no part of what is below, and can be handed to another
/// thread or shared with one.
#[test]
fn a_way_below_is_send_and_sync_whatever_is_below() {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    let tracing = owning_adapter_fn(
        |_: ShotContext, below: OwnedBelow<bool>| {
            assert_send_sync(&below);
            Ok::<_, Rejected>(below)
        },
        run_each_owned,
    );
    let below = compose(code_adapter(&[false]), Downstream::default()).expect("bools fit");
    let mut executor = compose(tracing, below).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
}

/// A state's way below, which the state owns: no copy of it can be made,
/// to reach below for another, as it is not `Clone`.
#[test]
fn a_way_below_cannot_be_cloned() {
    // `checked` names one function when the type is not `Clone`, and two,
    // which does not compile, when it is (a `compile_fail` doc example
    // would run only under `cargo test --doc`, not in CI's `cargo test`).
    trait NotClone<Which> {
        fn checked() {}
    }
    impl<T> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    <OwnedBelow<bool> as NotClone<_>>::checked();
}

/// `outcomes` by shot, then tag: shots served one by one come up in the
/// order their replies end.
fn by_shot(outcomes: &[ShotOutcome]) -> Vec<(u32, u32, Value)> {
    let mut values = values(outcomes);
    values.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
    values
}

/// A decoder with a state per shot, each owning its way below, over an
/// executor that is `Send`: it is `Send` too, and moved to another thread
/// after a call that made two shots' states here, it serves the next call
/// there, those states included, and reads what it reads here.
#[test]
fn an_owning_adapter_over_a_send_executor_serves_on_another_thread() {
    fn assert_send<T: Send>(_: &T) {}
    let decoded = || {
        let decoding = owning_adapter_fn(its_way_below, run_each_owned).shots(OnePerShot);
        (decoding >> Downstream::default()).expect("bools fit")
    };
    let (mut staying, mut moving) = (decoded(), decoded());
    step(
        &mut staying,
        &shots(&[2, 5]),
        &[true],
        "the first call runs",
    );
    step(&mut moving, &shots(&[2, 5]), &[true], "the first call runs");
    let (expected, end) = run(&mut staying, &shots(&SHOTS), &[true]).expect("the call is accepted");
    end.expect("the call ends well here");
    assert_send(&moving);
    let elsewhere = thread::spawn(move || run(&mut moving, &shots(&SHOTS), &[true]));
    let (read, end) = (elsewhere.join())
        .expect("the thread ends well")
        .expect("the moved call is accepted");
    end.expect("the call ends well there");
    assert_eq!(by_shot(&read), by_shot(&expected));
}
