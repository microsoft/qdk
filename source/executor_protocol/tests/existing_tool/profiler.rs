// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A profiler: an executor that counts the instructions its shots run, and
//! reads random bits drawn from each shot's seed. It is written with
//! [`executor_fn`], a state per shot ([`OnePerShot`]), so it holds only
//! that logic: each shot's state keeps its own counts, which it reports
//! when the profiler closes (`ExecutorFn::state_report`), and the
//! facility folds them over the shots into the profiler's report.

use std::collections::BTreeMap;
use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, Close, Describe, Executor, ExecutorReport, InstructionBatch,
    OnePerShot, Outcome, Outcomes, Rejected, ShotContext, check_unique_tags, executor_fn,
};

use crate::toy::{Op, readouts, toy_set};

/// A profile: how many of each instruction ran. It folds as maps of
/// integers do, adding count by count, so it needs no fold of its own.
pub type Counts = BTreeMap<&'static str, u64>;

/// The profiler of the toy's instructions whose shots draw from `seed`: an
/// executor that counts the instructions each shot runs, and reads each
/// readout as a random bit drawn from the shot's seed. It promises the
/// toy's instruction set, and no qubit count: it counts any of them, on
/// any qubit. Each shot's state keeps its counts and its draws from one
/// call to the next, and raises no shot condition, so it has none to keep:
/// it promises resumable shots. A shot draws from its own seed alone, so it
/// reads the same bits alone or among others: it promises replayable
/// shots. Its report holds the counts of every shot that ran, folded
/// ([`ExecutorReport::states`]).
pub fn toy_profiler(
    seed: u64,
) -> impl Executor<InstructionBatch<Op>> + Describe + Close<Report = ExecutorReport<Counts>> {
    let promise = Capabilities::default()
        .with_instruction_set(Some(Arc::new(toy_set())))
        .with_resumable_shots(true)
        .with_replayable(true);
    executor_fn(promise, ShotProfile::start, ShotProfile::run)
        .check(|input: &CallInput<'_, Op>| check_unique_tags(readouts(input.instructions())))
        .state_report(|shot: &ShotProfile| shot.counts.clone())
        .seed(seed)
        .shots(OnePerShot)
}

/// One shot's state in the profiler: the instructions it ran, and its
/// random state.
struct ShotProfile {
    counts: Counts,
    rng: u64,
}

impl ShotProfile {
    /// The profile of the shot of `ctx`, which has counted nothing and
    /// draws from the last executor's seed in the shot
    /// ([`ShotContext::seed`]).
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the form of a state closure, passed by name"
    )]
    fn start(ctx: ShotContext) -> Result<Self, Rejected> {
        Ok(Self {
            counts: Counts::default(),
            rng: ctx.seed(),
        })
    }

    /// Counts each instruction of `input` once, and reads each readout as
    /// a random bit of the shot.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the form of a logic, passed by name; the profiler fails no batch"
    )]
    fn run(
        &mut self,
        input: &CallInput<'_, Op>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        for &op in input.instructions() {
            if let Some(tag) = op.tag() {
                let value = self.random_bit().into();
                outcomes.push(Outcome::new(tag, value));
            }
            *self.counts.entry(mnemonic(op)).or_default() += 1;
        }
        Ok(())
    }

    /// One step of `SplitMix64`; its low bit.
    fn random_bit(&mut self) -> bool {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) & 1 == 1
    }
}

/// The toy's mnemonic for `op`.
fn mnemonic(op: Op) -> &'static str {
    match op {
        Op::Coin(_) => "coin",
        Op::X(_) => "x",
        Op::Cx(..) => "cx",
        Op::Measure { .. } => "measure",
        Op::Reset(_) => "reset",
    }
}
