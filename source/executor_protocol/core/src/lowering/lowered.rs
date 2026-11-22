// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Lowered`]: any [`Lowering`] as an adapter.
//!
//! Each `adapt` lowers its batches, instruction by instruction, into
//! batches of the set below, sends them below in one `execute`, and reads
//! each instruction's readouts back from those of its lowering, in a batch
//! whose lowered readout slots fit in a `u32`. Beyond that, a checked
//! batch is refused below
//! ([`TooManyReadoutSlots`](crate::BatchFault::TooManyReadoutSlots)), and
//! an unchecked one is sent stating `u32::MAX` slots: an instruction whose
//! lowering's slots lie past them reads fewer of them back, or none.
//!
//! So that a lowering's readouts are found without reading its
//! instructions again, it notes, as it lowers, where each instruction's
//! readout slots lie in the batch's rows above and below, summing the
//! slots of its lowering ([`InstructionSet::readout_slots`]) for the slots
//! it states below. It sums them as the receiver's set reads them, not as
//! the lowering's [`sends`](Lowering::sends) does: its lowering is encoded
//! as the binding to the receiver's set says, which may number the gates
//! otherwise.

use std::ops::Range;

use super::Lowering;
use crate::roles::lent_translating_adapter;
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, ExecuteError, Executor, InstructionSet, NotReady,
    ReadoutSlots, Readouts, Seed, TranslatingAdapter,
};

/// The adapter that runs the lowering `L`: it accepts `L::From`, sends
/// `L::To`, provides, requires below and gets ready as `L` does, and runs
/// each batch as `L` lowers it, each sent checked if the batch it lowers
/// arrived checked, as [`TranslatingAdapter::adapt`]'s rule asks. It
/// passes a refusal from below up unchanged, so the driver reads it in the
/// set below's terms: a [`BatchFault`](crate::BatchFault)'s `at` is a
/// position in the lowered batch, and its fault is `L::To`'s.
///
/// It keeps its lowered batches and their readouts from one `adapt` to the
/// next, so that, once it has lowered batches as large, an `adapt`
/// allocates, of its own memory, only the list of the batches it sends;
/// what its lowering allocates, and what the instructions it lowers to
/// hold, are the lowering's.
pub struct Lowered<L: Lowering> {
    lowering: L,
    /// One per batch of the last `adapt`, then any kept, with their
    /// memory, from an earlier `adapt` of more batches.
    batches: Vec<LoweredBatch<L::To>>,
    /// The readouts of the batches sent below.
    readouts: Readouts<L::To>,
}

impl<L: Lowering> Lowered<L> {
    /// The adapter of `lowering`, with no memory yet.
    #[must_use]
    pub fn new(lowering: L) -> Self {
        Self {
            lowering,
            batches: Vec::new(),
            readouts: Readouts::new(),
        }
    }
}

impl<L: Lowering> TranslatingAdapter for Lowered<L> {
    type Accepts = L::From;
    type Sends = L::To;

    fn accepts(&self) -> &L::From {
        self.lowering.accepts()
    }

    fn sends(&self) -> &L::To {
        self.lowering.sends()
    }

    fn provides(&self) -> &Capabilities<L::From> {
        self.lowering.provides()
    }

    fn requires(&self, above: &Capabilities<L::From>) -> Capabilities<L::To> {
        self.lowering.requires(above)
    }

    fn ready(
        &mut self,
        above: &Capabilities<L::From>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        self.lowering.ready(above)
    }

    fn bind(&mut self, below: Binding<L::To>) {
        self.lowering.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, L::From>],
        seed: Seed,
        below: &mut impl Executor<Accepts = L::To>,
        readouts: &mut [BatchReadouts<L::From>],
    ) -> Result<(), ExecuteError> {
        let Self {
            lowering,
            batches: lowered,
            readouts: lowered_readouts,
        } = self;
        if lowered.len() < batches.len() {
            lowered.resize_with(batches.len(), LoweredBatch::default);
        }
        for (batch, lowered) in batches.iter().zip(lowered.iter_mut()) {
            lowered.lower(lowering, below.accepts(), batch.instructions);
        }
        let sent: Vec<Batch<'_, L::To>> = batches
            .iter()
            .zip(lowered.iter())
            .map(|(batch, lowered)| {
                Batch::new(batch.shots, &lowered.instructions, lowered.readout_slots)
                    .checked_if(batch.is_checked())
            })
            .collect();
        let sent_readouts = lowered_readouts.readouts_for(&sent);
        below.execute(&sent, seed, sent_readouts).await?;
        for (((batch, lowered), below), above) in batches
            .iter()
            .zip(lowered.iter())
            .zip(sent_readouts.iter())
            .zip(readouts)
        {
            lowered.read(lowering, batch.instructions, below, above);
        }
        Ok(())
    }
}

lent_translating_adapter!([L: Lowering] Lowered<L>);

/// A batch as lowered: its instructions below, and where each instruction
/// with readout slots finds its readouts.
struct LoweredBatch<S: InstructionSet> {
    instructions: Vec<S::Instruction>,
    /// One per instruction above with a readout slot, in their order.
    reads: Vec<InstructionReadouts>,
    /// The readout slots of `instructions`, summed.
    readout_slots: ReadoutSlots,
}

/// Where an instruction above finds its readouts: its slots in a row above,
/// and those of its lowering in a row below.
struct InstructionReadouts {
    /// The instruction's position in its batch.
    at: usize,
    above: Range<usize>,
    below: Range<usize>,
}

impl<S: InstructionSet> Default for LoweredBatch<S> {
    fn default() -> Self {
        Self {
            instructions: Vec::new(),
            reads: Vec::new(),
            readout_slots: 0,
        }
    }
}

impl<S: InstructionSet> LoweredBatch<S> {
    /// Lowers `instructions` with `lowering`, in place of what it held,
    /// counting the readout slots of their lowering as `receiver`, the set
    /// below they are encoded in, reads them.
    fn lower<L: Lowering<To = S>>(
        &mut self,
        lowering: &L,
        receiver: &S,
        instructions: &[<L::From as InstructionSet>::Instruction],
    ) {
        let above_set = lowering.accepts();
        self.instructions.clear();
        self.reads.clear();
        let mut above: usize = 0;
        let mut below: ReadoutSlots = 0;
        for (at, instruction) in instructions.iter().enumerate() {
            let start = self.instructions.len();
            lowering.lower(instruction, &mut self.instructions);
            // Saturating, so that no sum overflows on any target: a sum
            // beyond a `u32` is stated as `u32::MAX`, which the boundary
            // below refuses in a checked batch, as it refuses a batch of
            // that many slots sent unlowered; in an unchecked one, the
            // ranges below end at `u32::MAX`. Above, a checked batch's
            // slots fit in a `u32`, as the boundary above checked.
            let below_slots = self.instructions[start..]
                .iter()
                .map(|lowered| receiver.readout_slots(lowered))
                .fold(0, u32::saturating_add);
            let below_end = below.saturating_add(below_slots);
            let above_slots = above_set.readout_slots(instruction) as usize;
            let above_end = above.saturating_add(above_slots);
            if above_slots > 0 {
                self.reads.push(InstructionReadouts {
                    at,
                    above: above..above_end,
                    below: below as usize..below_end as usize,
                });
            }
            above = above_end;
            below = below_end;
        }
        self.readout_slots = below;
    }

    /// Writes the readouts above, `above`, of `instructions`, which it
    /// lowered, from those below, `below`, row by row, each row with its
    /// shot's conditions read as `lowering` reads them.
    fn read<L: Lowering<To = S>>(
        &self,
        lowering: &L,
        instructions: &[<L::From as InstructionSet>::Instruction],
        below: &BatchReadouts<S>,
        above: &mut BatchReadouts<L::From>,
    ) {
        for shot in 0..below.shots() {
            let (row_below, row_above) = (below.row(shot), above.row_mut(shot));
            for read in &self.reads {
                lowering.read(
                    &instructions[read.at],
                    &row_below[read.below.clone()],
                    &mut row_above[read.above.clone()],
                );
            }
            above.set_conditions(shot, lowering.read_conditions(below.conditions(shot)));
        }
    }
}
