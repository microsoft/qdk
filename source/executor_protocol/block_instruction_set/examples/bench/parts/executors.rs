// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bench's executors, each counting what it gets: on this thread, as
//! cheaply as reading every instruction allows, or from a worker thread.

use std::future::poll_fn;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};
use std::thread::{self, JoinHandle};

use block_instruction_set::{BlockCapabilities, BlockInstruction, BlockInstructionSet, Opcode};
use executor_protocol_core::prelude::*;
use executor_protocol_core::{
    Batch, BatchReadouts, Capabilities, ExecuteError, ExecutorLimits, Features, Resources, Seed,
    ShotSet, StatesLimits,
};

use super::MixGates;
use crate::workload::MIX;

/// An executor of `MixGates` that counts each opcode, the highest qubit and
/// the kinds of batches: as cheap an executor as one that reads every
/// instruction can be.
pub(crate) struct Profiler {
    accepts: BlockInstructionSet,
    provides: Capabilities<BlockInstructionSet>,
    pub(crate) profile: Profile,
}

/// What a [`Profiler`] counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Profile {
    /// Each gate's count, in `MIX`'s order.
    counts: [u64; MIX.len()],
    top_qubit: Option<u32>,
    kinds: BatchKinds,
}

/// Whether a part received checked batches, unchecked ones, or both: what
/// shows that a run sent what its flag asks, so a row labelled checked
/// timed checked batches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BatchKinds {
    checked: bool,
    unchecked: bool,
}

impl BatchKinds {
    fn record(&mut self, checked: bool) {
        if checked {
            self.checked = true;
        } else {
            self.unchecked = true;
        }
    }
}

impl Profiler {
    /// A profiler accepting `MixGates` and providing `resources`.
    pub(crate) fn new(resources: Resources) -> StatesLimits<Self> {
        Self {
            accepts: MixGates::set(),
            provides: Capabilities::new(
                resources,
                Features::unconstrained(),
                BlockCapabilities::NONE,
            ),
            profile: Profile::default(),
        }
        .stating_limits()
    }
}

impl Profile {
    /// What a profiler counts of `program`, sent in batches each
    /// [`checked`](Batch::checked) if `checked`.
    pub(crate) fn of(program: &[BlockInstruction], checked: bool) -> Self {
        let mut profile = Self::default();
        profile.record(program, checked);
        profile
    }

    /// Counts a batch of `instructions`, checked if `checked`.
    fn record(&mut self, instructions: &[BlockInstruction], checked: bool) {
        self.kinds.record(checked);
        for instruction in instructions {
            self.counts[gate_index(instruction.opcode)] += 1;
            if let Some(&top) = instruction.blocks.iter().max() {
                self.top_qubit = Some(self.top_qubit.map_or(top, |was| was.max(top)));
            }
        }
    }
}

/// A gate's place in `MIX`. A match, since an opcode's index is the
/// protocol's own, and a search of `MIX` would compare each opcode in turn
/// on the profiler's hot path.
pub(crate) const fn gate_index(opcode: Opcode) -> usize {
    match opcode {
        MixGates::CX => 0,
        MixGates::H => 1,
        MixGates::RZ => 2,
        MixGates::T => 3,
        MixGates::T_ADJ => 4,
        MixGates::S => 5,
        MixGates::X => 6,
        MixGates::CZ => 7,
        MixGates::CCX => 8,
        MixGates::RESET => 9,
        MixGates::SWAP => 10,
        MixGates::X_IF => 11,
        _ => panic!("the driver sends only instructions of MixGates"),
    }
}

// `gate_index` gives each gate of `MIX` its place there, checked when the
// bench is built.
const _: () = {
    let mut place = 0;
    while place < MIX.len() {
        assert!(gate_index(MIX[place].0) == place);
        place += 1;
    }
};

impl Executor for Profiler {
    type Accepts = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.accepts
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, BlockInstructionSet>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts<BlockInstructionSet>],
    ) -> Result<(), ExecuteError> {
        // Its gates read nothing: each batch's readouts come shaped with no
        // slot.
        for batch in batches {
            self.profile.record(batch.instructions, batch.is_checked());
        }
        Ok(())
    }
}

impl ExecutorLimits for Profiler {
    fn provides(&self) -> Option<&Capabilities<BlockInstructionSet>> {
        Some(&self.provides)
    }
}

/// An executor of `MixGates` whose worker thread counts each batch, from a
/// copy, and hands its readouts back, waking the run's waker.
pub(crate) struct FromWorker {
    accepts: BlockInstructionSet,
    /// Each `execute`'s batches, copied, to the worker; closed by `finish`.
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<Profile>>,
    handover: Arc<Mutex<Handover>>,
}

/// An `execute`'s batches, copied: each one's shots, instructions and
/// whether it is checked.
pub(crate) type Job = Vec<(ShotSet, Vec<BlockInstruction>, bool)>;

/// The worker's readouts, once ready, and the waker to wake then.
#[derive(Default)]
pub(crate) struct Handover {
    readouts: Option<Vec<BatchReadouts<BlockInstructionSet>>>,
    waker: Option<Waker>,
}

impl FromWorker {
    pub(crate) fn spawn() -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let handover = Arc::new(Mutex::new(Handover::default()));
        let worker = {
            let handover = Arc::clone(&handover);
            thread::spawn(move || {
                let mut profile = Profile::default();
                for job in inbox {
                    let readouts = job
                        .iter()
                        .map(|(shots, instructions, checked)| {
                            profile.record(instructions, *checked);
                            BatchReadouts::new(shots.len(), 0)
                        })
                        .collect();
                    let mut handover = handover.lock().expect("no panic while held");
                    handover.readouts = Some(readouts);
                    if let Some(waker) = handover.waker.take() {
                        waker.wake();
                    }
                }
                profile
            })
        };
        Self {
            accepts: MixGates::set(),
            jobs: Some(jobs),
            worker: Some(worker),
            handover,
        }
    }

    /// Stops the worker and returns what it counted.
    pub(crate) fn finish(mut self) -> Profile {
        drop(self.jobs.take());
        let worker = self.worker.take().expect("finished once");
        worker.join().expect("the worker ends")
    }
}

impl Executor for FromWorker {
    type Accepts = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.accepts
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, BlockInstructionSet>],
        _seed: Seed,
        readouts: &mut [BatchReadouts<BlockInstructionSet>],
    ) -> Result<(), ExecuteError> {
        let job = batches
            .iter()
            .map(|batch| {
                (
                    batch.shots.clone(),
                    batch.instructions.to_vec(),
                    batch.is_checked(),
                )
            })
            .collect();
        let jobs = self.jobs.as_ref().expect("the worker runs until `finish`");
        jobs.send(job)
            .expect("the worker takes jobs until `finish`");
        let worker_readouts = poll_fn(|context| {
            let mut handover = self.handover.lock().expect("no panic while held");
            if let Some(ready) = handover.readouts.take() {
                Poll::Ready(ready)
            } else {
                handover.waker = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await;
        // The worker's readouts are its own: copied into the caller's.
        for (theirs, ours) in worker_readouts.iter().zip(readouts) {
            ours.concat_from(std::slice::from_ref(theirs));
        }
        Ok(())
    }
}
