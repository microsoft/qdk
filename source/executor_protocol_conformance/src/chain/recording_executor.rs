// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`RecordingExecutor`]: the executor below an adapter or a driver under test,
//! which answers every batch it gets with values drawn from each shot's
//! seed, raises the conditions the check chose, and logs each batch.

use rustc_hash::FxHashMap;

use executor_protocol::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, InstructionSet, Offer, Seed,
    ShotId, ShotSet,
};

use crate::shared_run::{BatchBelow, RunSetup, SharedRun};
use crate::splitmix64::SplitMix64;

/// Answers each batch, as an executor that keeps every rule would: each
/// shot's values drawn from a stream made at its first batch from
/// `seed.shot(id)` and kept until its end, one draw per instruction filling
/// that instruction's readout slots; each row's conditions those the setup
/// raises on its shot in that call of that `execute`.
pub(crate) struct RecordingExecutor {
    run: SharedRun,
    accepts: InstructionSet,
    streams: FxHashMap<ShotId, SplitMix64>,
    /// Whether each call is an `execute` of the part under test, as for a
    /// driver's check, rather than a call below one.
    counts_executes: bool,
}

impl RecordingExecutor {
    /// An executor below an adapter, that accepts what the adapter sends.
    pub(crate) fn below_an_adapter(run: SharedRun, accepts: InstructionSet) -> Self {
        Self {
            run,
            accepts,
            streams: FxHashMap::default(),
            counts_executes: false,
        }
    }

    /// An executor below a driver, that accepts what the driver sends.
    pub(crate) fn below_a_driver(run: SharedRun, accepts: InstructionSet) -> Self {
        Self {
            counts_executes: true,
            ..Self::below_an_adapter(run, accepts)
        }
    }

    /// Answers `batch` in `readouts`, and logs it.
    fn answer(
        &mut self,
        (execute, call, index): (usize, usize, usize),
        batch: &Batch<'_>,
        seed: Seed,
        readouts: &mut BatchReadouts,
        setup: &RunSetup,
    ) {
        self.run.write(|log| {
            log.below.push(BatchBelow {
                execute,
                call,
                batch: index,
                shots: batch.shots.iter().collect(),
                stated: batch.readout_slots(),
                counted: self.accepts.readout_slots_of(batch.instructions),
                checked: batch.is_checked(),
                seed: seed.shot(ShotId::new(0)),
            });
        });
        for (position, shot) in batch.shots.iter().enumerate() {
            let stream = self
                .streams
                .entry(shot)
                .or_insert_with(|| SplitMix64::new(seed.shot(shot)));
            let row = readouts.row_mut(position);
            let mut slots = row.iter_mut();
            for instruction in batch.instructions {
                let draw = stream.next_u64();
                let count = self.accepts.readout_slots(instruction.opcode).unwrap_or(0);
                for (bit, slot) in (0..count).zip(slots.by_ref()) {
                    *slot = (draw >> (bit % 64) & 1 == 1).into();
                }
            }
            // Slots a sender states beyond its instructions' count, which
            // `states_readout_slots` reports: filled too, as an
            // executor writes every slot it is given.
            for slot in slots {
                *slot = (stream.next_u64() & 1 == 1).into();
            }
            readouts.set_conditions(position, setup.raised(execute, call, shot));
        }
    }
}

impl ExecutorSetup for RecordingExecutor {
    fn accepts(&self) -> &InstructionSet {
        &self.accepts
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "a part that never waits keeps the trait's `async fn` form"
)]
impl Execute for RecordingExecutor {
    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        if self.counts_executes {
            self.run.begin_execute();
        }
        let (execute, call) = self.run.next_call_below();
        let setup = self.run.setup();
        for (index, (batch, readouts)) in batches.iter().zip(readouts).enumerate() {
            self.answer((execute, call, index), batch, seed, readouts, &setup);
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for shot in shots.iter() {
            self.streams.remove(&shot);
        }
    }
}
