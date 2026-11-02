// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor written by hand in a crate other than the protocol's, as a
//! simulator whose shots share one state is
//! ([An executor written by hand](../docs/executors.md#an-executor-written-by-hand-executorlifecycle)),
//! and what its crate writes so that a chain ending at it is written with
//! `>>` all along. The protocol's crate cannot give that executor `>>`, so
//! its crate does: one impl for each of the four public types that may be
//! on its left (`ChainHead`, `ComposedAdapters`, `AdapterFn` and
//! `OnePerShotAdapter`), each through `Compose<_, WithExecutor>`. The
//! chain then groups either way.

use std::collections::BTreeMap;
use std::ops::Shr;

use executor_protocol::{
    AdapterFn, AdapterLogic, Below, CallInput, Capabilities, ChainHead, Close, Compose,
    ComposedAdapters, Describe, Executor, ExecutorLifecycle, ImmediateReply, InstructionBatch,
    InstructionBatchRef, MakeState, Needs, OnePerShot, OnePerShotAdapter, Outcomes, Rejected,
    ShotId, ShotOutcome, StaticProgram, TypedInstruction, Value, WithExecutor, adapter_fn,
    check_unique_tags, compose, driver,
};

/// A bit per shot.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    /// Flips the bit.
    Flip,
    /// Reads the bit, under a tag.
    Read(u32),
}

impl TypedInstruction for Op {}

/// The executor written by hand: the bits of its shots, kept from one
/// call to the next, and the rules of failure and close kept by its
/// lifecycle.
#[derive(Default)]
struct Bits {
    bits: BTreeMap<ShotId, bool>,
    lifecycle: ExecutorLifecycle,
}

impl Executor<InstructionBatch<Op>> for Bits {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        self.lifecycle.admit()?;
        let readouts = batch.instructions.iter().enumerate();
        check_unique_tags(readouts.filter_map(|(at, op)| match op {
            Op::Read(tag) => Some((at, *tag)),
            Op::Flip => None,
        }))?;
        let mut reply = ImmediateReply::default();
        for &shot in batch.shots.as_slice() {
            let bit = self.bits.entry(shot).or_default();
            for op in batch.instructions {
                match *op {
                    Op::Flip => *bit = !*bit,
                    Op::Read(tag) => reply.push(ShotOutcome::new(shot, tag, (*bit).into())),
                }
            }
        }
        Ok(reply)
    }
}

impl Describe for Bits {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_resumable_shots(true)
    }
}

impl Close for Bits {
    type Report = ();

    fn close(&mut self) {
        self.lifecycle.close();
    }
}

/// `>>` after a chain's head, as `Bits`'s crate writes it; the three
/// impls below are its like.
impl<T, L> Shr<Bits> for ChainHead<T, L>
where
    Self: Compose<Bits, WithExecutor>,
{
    type Output = <Self as Compose<Bits, WithExecutor>>::Output;

    fn shr(self, rhs: Bits) -> Self::Output {
        self.compose(rhs)
    }
}

impl<X, Y> Shr<Bits> for ComposedAdapters<X, Y>
where
    Self: Compose<Bits, WithExecutor>,
{
    type Output = <Self as Compose<Bits, WithExecutor>>::Output;

    fn shr(self, rhs: Bits) -> Self::Output {
        self.compose(rhs)
    }
}

impl<I, O, S, M, L, C, R> Shr<Bits> for AdapterFn<I, O, S, M, L, C, R>
where
    Self: Compose<Bits, WithExecutor>,
{
    type Output = <Self as Compose<Bits, WithExecutor>>::Output;

    fn shr(self, rhs: Bits) -> Self::Output {
        self.compose(rhs)
    }
}

impl<I, O, S, M, L, C, R> Shr<Bits> for OnePerShotAdapter<I, O, S, M, L, C, R>
where
    Self: Compose<Bits, WithExecutor>,
{
    type Output = <Self as Compose<Bits, WithExecutor>>::Output;

    fn shr(self, rhs: Bits) -> Self::Output {
        self.compose(rhs)
    }
}

/// An adapter that flips before it sends each call on, one state.
fn flip_first() -> AdapterFn<Op, Op, (), impl MakeState<()>, impl AdapterLogic<Op, Op, ()>> {
    adapter_fn(
        |_| Ok(()),
        |(): &mut (),
         input: &CallInput<'_, Op>,
         below: &mut Below<'_, Op>,
         _: &mut Outcomes<'_>| {
            below.send(&[Op::Flip])?;
            below.send(input.instructions())
        },
    )
}

/// An adapter that flips after it sends each call on, a state per shot.
fn flip_after() -> OnePerShotAdapter<Op, Op, (), impl MakeState<()>, impl AdapterLogic<Op, Op, ()>>
{
    adapter_fn(
        |_| Ok(()),
        |(): &mut (),
         input: &CallInput<'_, Op>,
         below: &mut Below<'_, Op>,
         _: &mut Outcomes<'_>| {
            below.send(input.instructions())?;
            below.send(&[Op::Flip])
        },
    )
    .shots(OnePerShot)
}

/// The shots each chain runs.
const SHOTS: [ShotId; 4] = [0, 1, 2, 3];

/// What `chain`, once built, reads when it runs a flip and a read on
/// [`SHOTS`], by shot.
macro_rules! read_by {
    ($chain:expr) => {{
        let program = StaticProgram::new(Needs::default(), vec![vec![Op::Flip, Op::Read(0)]]);
        let chain = ($chain).expect("the adapters send what `Bits` takes");
        let job = chain.run(program, SHOTS).expect("the program fits");
        let (outcomes, _) = job.wait();
        let mut outcomes = outcomes.expect("`Bits` runs every call");
        outcomes.sort_by_key(|outcome| outcome.shot);
        outcomes
    }};
}

/// A chain of the crate's driver, an adapter that flips first, one that
/// flips after, and `Bits`, written with `>>` in each grouping that puts
/// one of the four public left-hand types beside `Bits`: each reads what
/// the same chain composed with `compose` reads: 1 on each shot, after
/// the program's flip and the two the adapters send before its read.
/// `AdapterFn` is beside `Bits` only in the chain without the adapter that
/// flips after, so that chain is checked against its own.
#[test]
fn an_executor_written_by_hand_is_composed_with_shr_after_each_public_left_type() {
    let by_compose = read_by!(compose(
        compose(compose(driver(), flip_first()), flip_after()),
        Bits::default()
    ));
    let ones: Vec<_> = SHOTS
        .iter()
        .map(|&shot| ShotOutcome::new(shot, 0, Value::One))
        .collect();
    assert_eq!(by_compose, ones, "three flips before the read");
    let after_a_head = read_by!(driver() >> flip_first() >> flip_after() >> Bits::default());
    let built = "the adapters send what `Bits` takes";
    let after_composed_adapters =
        read_by!(driver() >> ((flip_first() >> flip_after()) >> Bits::default()).expect(built));
    let below = (flip_after() >> Bits::default()).expect(built);
    let after_one_per_shot = read_by!(driver() >> (flip_first() >> below).expect(built));
    let after_an_adapter_fn = read_by!(driver() >> (flip_first() >> Bits::default()).expect(built));
    let flip_first_alone = read_by!(compose(compose(driver(), flip_first()), Bits::default()));
    for (grouping, read) in [
        ("d >> a >> b >> e", after_a_head),
        ("d >> ((a >> b) >> e)?", after_composed_adapters),
        ("d >> (a >> (b >> e)?)?", after_one_per_shot),
    ] {
        assert_eq!(read, by_compose, "{grouping}");
    }
    assert_eq!(after_an_adapter_fn, flip_first_alone, "d >> (a >> e)?");
}
