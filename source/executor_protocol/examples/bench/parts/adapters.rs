// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bench's adapters: one that forwards, and a toy lowering.

use executor_protocol::prelude::*;
use executor_protocol::{
    Arguments, Batch, BatchReadouts, BlockType, Capabilities, ExecuteError, Features, Instruction,
    InstructionSet, Opcode, OpcodeMap, ReadoutValue, Readouts, Resources, Seed,
};

use super::{LogicalGates, MixGates, PhysicalGates};

/// An adapter that forwards every batch as it is: the cost of a boundary.
/// It states only its instructions: it neither needs nor limits the rest.
pub(crate) struct Forward {
    set: InstructionSet,
    identity: OpcodeMap,
}

impl Forward {
    pub(crate) fn new() -> Self {
        let set = MixGates::set();
        let identity = set.fits(&set).expect("a set fits itself");
        Self { set, identity }
    }
}

impl Adapter for Forward {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        assert!(
            below == self.identity,
            "the receiver's set is the adapter's"
        );
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}

/// A toy lowering: the repetition code of three, each logical qubit `q` the
/// physical qubits `3q`, `3q + 1` and `3q + 2`, each logical instruction the
/// same physical one on each, and a logical readout the majority of three.
/// Built for a count of logical qubits, it takes at most that many, and
/// requires three physical qubits for each logical qubit that arrives.
pub(crate) struct Repetition3 {
    accepts: InstructionSet,
    sends: InstructionSet,
    provides: Capabilities,
    /// The physical opcodes, in the receiver's, from `bind`.
    opcodes: [Opcode; 4],
    /// Each batch's physical instructions, kept to reuse their memory.
    lowered: Vec<Vec<Instruction>>,
    /// The physical readouts, kept to reuse their memory.
    physical_readouts: Readouts,
}

impl Repetition3 {
    /// The lowering of `logical_qubits` logical qubits.
    pub(crate) fn new(logical_qubits: u32) -> Self {
        Self {
            accepts: LogicalGates::set(),
            sends: PhysicalGates::set(),
            provides: Capabilities::new(
                Resources::unconstrained().with_qubits(logical_qubits),
                Features::unconstrained(),
            ),
            opcodes: [
                PhysicalGates::RESET,
                PhysicalGates::X,
                PhysicalGates::CX,
                PhysicalGates::MZ,
            ],
            lowered: Vec::new(),
            physical_readouts: Readouts::new(),
        }
    }
}

impl Adapter for Repetition3 {
    fn accepts(&self) -> &InstructionSet {
        &self.accepts
    }

    fn sends(&self) -> &InstructionSet {
        &self.sends
    }

    fn provides(&self) -> &Capabilities {
        &self.provides
    }

    /// What arrives, on three physical qubits for each logical one.
    fn requires(&self, above: &Capabilities) -> Capabilities {
        let logical = (above.resources()).blocks(&BlockType::qubit());
        (above.clone()).map_resources(|resources| match logical {
            Some(logical) => resources.with_qubits(3 * logical),
            None => resources,
        })
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.opcodes = self.opcodes.map(|opcode| below.map(opcode));
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.lowered.resize_with(batches.len(), Vec::new);
        for (batch, lowered) in batches.iter().zip(&mut self.lowered) {
            lowered.clear();
            for instruction in batch.instructions {
                lower(instruction, self.opcodes, lowered);
            }
        }
        // Each checked if the batch it lowers was, as `Adapter`'s rule says,
        // reading three times its readout slots: each readout is lowered to
        // three.
        let sent: Vec<Batch<'_>> = batches
            .iter()
            .zip(&self.lowered)
            .map(|(batch, lowered)| {
                Batch::new(batch.shots, lowered, 3 * batch.readout_slots())
                    .checked_if(batch.is_checked())
            })
            .collect();
        let physical = self.physical_readouts.readouts_for(&sent);
        below.execute(&sent, seed, physical).await?;
        for (physical, logical) in physical.iter().zip(readouts) {
            read_logical(physical, logical);
        }
        Ok(())
    }
}

/// Appends `instruction`'s three physical instructions to `lowered`.
/// Always inlined, so that the lowering section times the protocol, not
/// whether LLVM inlines the workload into a given build.
#[expect(
    clippy::inline_always,
    reason = "a bench's workload, timed alike in every build"
)]
#[inline(always)]
pub(crate) fn lower(
    instruction: &Instruction,
    opcodes: [Opcode; 4],
    lowered: &mut Vec<Instruction>,
) {
    let [reset, x, cx, mz] = opcodes;
    let opcode = match instruction.opcode {
        LogicalGates::PREPARE_Z => reset,
        LogicalGates::X => x,
        LogicalGates::CX => cx,
        LogicalGates::MEASURE_Z => mz,
        _ => unreachable!("the driver sends only instructions of LogicalGates"),
    };
    for i in 0..3 {
        lowered.push(Instruction {
            opcode,
            blocks: instruction.blocks.iter().map(|q| 3 * q + i).collect(),
            arguments: Arguments::new(),
            select: None,
        });
    }
}

/// Fills a batch's logical readouts, shaped with a third of the physical
/// readout slots, from its physical ones: each three's majority.
pub(crate) fn read_logical(physical: &BatchReadouts, logical: &mut BatchReadouts) {
    for shot in 0..physical.shots() {
        let threes = physical.row(shot).chunks(3);
        for (value, three) in logical.row_mut(shot).iter_mut().zip(threes) {
            *value = majority(three);
        }
        logical.set_conditions(shot, physical.conditions(shot));
    }
}

/// The majority of three readout values.
pub(crate) fn majority(three: &[ReadoutValue]) -> ReadoutValue {
    let ones = three.iter().filter(|&&v| v == ReadoutValue::One).count();
    if 2 * ones > three.len() {
        ReadoutValue::One
    } else {
        ReadoutValue::Zero
    }
}
