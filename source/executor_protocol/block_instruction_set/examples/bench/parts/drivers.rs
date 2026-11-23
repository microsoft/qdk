// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bench's drivers: a program sent in batches, through the chain or
//! past it, and running a chain of them once.

use std::hint::black_box;
use std::time::{Duration, Instant};

use block_instruction_set::{
    BlockCapabilities, BlockInstruction, BlockInstructionSet, OpcodeMap, ReadoutValue,
};
use executor_protocol_core::prelude::*;
use executor_protocol_core::{
    Batch, BatchReadouts, Capabilities, CompositionFaults, DriveError, DriverRequirements,
    ExecuteError, Features, Readouts, Resources, RunRequirement, Runner, Seed, ShotSet,
};

use crate::workload::SEED;

/// A driver that sends its program below, `batch` instructions per
/// `execute`, one shot, each batch [`checked`](Batch::checked) when
/// `checked`, and returns every readout and, when `timed`, the time of each
/// call.
pub(crate) struct ProgramInBatches<'p> {
    /// The set its program is written in.
    sends: BlockInstructionSet,
    /// Its qubits, and readout parities, which the parity-heavy mix's
    /// corrections use: a checked batch's corrections are refused unless
    /// its sender requires them, and the other programs hold none, so
    /// their checks cost the same.
    requires: Capabilities<BlockInstructionSet>,
    /// The map `bind` must get, the set's own: the program goes below as
    /// it is, so every chain gets the same instructions.
    identity: OpcodeMap,
    program: &'p [BlockInstruction],
    /// The readout slots of each batch of `program`, counted by
    /// [`slots_per_batch`] before a timed run, as a sender knows them where
    /// it writes a batch.
    slots: &'p [u32],
    batch: usize,
    checked: bool,
    pub(crate) timed: bool,
}

/// What [`ProgramInBatches`] got back.
pub(crate) struct Received {
    pub(crate) values: Vec<ReadoutValue>,
    pub(crate) call_times: Vec<Duration>,
}

impl<'p> ProgramInBatches<'p> {
    pub(crate) fn new(
        set: BlockInstructionSet,
        program: &'p [BlockInstruction],
        slots: &'p [u32],
        qubits: u32,
        batch: usize,
    ) -> Self {
        let identity = set.fits(&set).expect("a set fits itself");
        assert_eq!(
            slots.len(),
            program.len().div_ceil(batch),
            "a count of slots per batch"
        );
        Self {
            sends: set,
            requires: Capabilities::new(
                Resources::unconstrained(),
                Features::unconstrained(),
                BlockCapabilities::NONE
                    .with_qubits(qubits)
                    .with_readout_parity(true),
            ),
            identity,
            program,
            slots,
            batch,
            checked: false,
            timed: false,
        }
    }

    /// The same driver, sending each batch checked if `on`.
    pub(crate) fn checked_if(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    /// Sends the program to `target`; past the protocol, which shapes
    /// the readouts, if `shapes_itself`: it then shapes them itself, with
    /// the readout slots the batch states, and only where the shape changes.
    async fn send(
        &self,
        target: &mut impl Executor<Accepts = BlockInstructionSet>,
        shots: &ShotSet,
        seed: Seed,
        shapes_itself: bool,
    ) -> Result<Received, ExecuteError> {
        let mut received = Received {
            values: Vec::new(),
            call_times: Vec::new(),
        };
        let mut kept = Readouts::<BlockInstructionSet>::new();
        for (chunk, &slots) in self.program.chunks(self.batch).zip(self.slots) {
            let start = self.timed.then(Instant::now);
            let batches = [Batch::new(shots, black_box(chunk), slots).checked_if(self.checked)];
            let readouts = kept.readouts_for(&batches);
            if shapes_itself {
                let batch_readouts = &mut readouts[0];
                if batch_readouts.shots() != shots.len() || batch_readouts.readout_slots() != slots
                {
                    *batch_readouts = BatchReadouts::new(shots.len(), slots);
                }
            }
            target.execute(&batches, seed, readouts).await?;
            if let Some(start) = start {
                received.call_times.push(start.elapsed());
            }
            received.values.extend_from_slice(readouts[0].row(0));
        }
        Ok(received)
    }
}

impl Driver for ProgramInBatches<'_> {
    type Sends = BlockInstructionSet;

    fn sends(&self) -> &BlockInstructionSet {
        &self.sends
    }

    fn bind(&mut self, below: OpcodeMap) {
        assert!(below == self.identity, "the receiver's set is the driver's");
    }

    type Output = Received;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = BlockInstructionSet>,
    ) -> Result<Vec<Received>, DriveError> {
        let received = self.send(below, shots, seed, false).await?;
        Ok(vec![received])
    }
}

impl DriverRequirements for ProgramInBatches<'_> {
    /// Its qubits and readout parities, whatever the run: it does not pass
    /// the run's one shot and fixed seed below, which every executor of
    /// the bench takes, so the timings that include a run's start leave
    /// out checking them.
    fn requires(&self, _run: &RunRequirement) -> Capabilities<BlockInstructionSet> {
        self.requires.clone()
    }
}

/// The readout slots of each batch of `batch` instructions of `program`,
/// counted before a timed run, as a sender states them where it writes a
/// batch: so that no variant times a pass over the program to count them.
pub(crate) fn slots_per_batch(
    set: &BlockInstructionSet,
    program: &[BlockInstruction],
    batch: usize,
) -> Vec<u32> {
    program
        .chunks(batch)
        .map(|chunk| set.readout_slots_of(chunk).expect("a program of the set"))
        .collect()
}

/// [`ProgramInBatches`] sending to an executor of its own, past the chain
/// it is composed with, awaiting each `execute` itself: the baseline. A
/// driver, because only a run gives a seed to call `execute` with; it
/// requires what its [`ProgramInBatches`] does.
pub(crate) struct Direct<'p, E>(pub(crate) ProgramInBatches<'p>, pub(crate) E);

impl<E: Executor<Accepts = BlockInstructionSet>> Driver for Direct<'_, E> {
    type Sends = BlockInstructionSet;

    fn sends(&self) -> &BlockInstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    type Output = Received;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        _chain: &mut impl Executor<Accepts = BlockInstructionSet>,
    ) -> Result<Vec<Received>, DriveError> {
        let Self(in_batches, executor) = self;
        let received = in_batches.send(executor, shots, seed, true).await?;
        // Its executor is past the chain, so the runner does not end the
        // run's shots on it.
        executor.end_shots(shots);
        Ok(vec![received])
    }
}

impl<E: Executor<Accepts = BlockInstructionSet>> DriverRequirements for Direct<'_, E> {
    fn requires(&self, run: &RunRequirement) -> Capabilities<BlockInstructionSet> {
        self.0.requires(run)
    }
}

/// Runs the composed chain on one shot, and returns its driver's output.
pub(crate) fn run_once<
    D: Driver<Output = Received, Sends = BlockInstructionSet>,
    E: Executor<Accepts = BlockInstructionSet>,
>(
    chain: Result<Runner<D, E>, CompositionFaults>,
) -> Received {
    let mut runner = chain.expect("the chain composes");
    let mut outputs = runner
        .run(&ShotSet::ids([0]), SEED)
        .expect("the chain runs one shot");
    outputs.pop().expect("one shot's output")
}
