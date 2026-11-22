// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bench's adapters: one that forwards, and a toy lowering, which runs
//! as an adapter through the protocol's `Lowered`.

use block_instruction_set::{
    Arguments, BlockCapabilities, BlockInstruction, BlockInstructionSet, BlockType, Opcode,
    OpcodeMap, ReadoutFailure, ReadoutValue, ShotConditions,
};
use executor_protocol_core::prelude::*;
use executor_protocol_core::{
    Batch, BatchReadouts, Capabilities, ExecuteError, Features, Resources, Seed,
};

use super::{LogicalGates, MixGates, PhysicalGates};

/// An adapter that forwards every batch as it is: the cost of a boundary.
/// It states only its instructions: it neither needs nor limits the rest.
pub(crate) struct Forward {
    set: BlockInstructionSet,
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
    type Set = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.set
    }

    fn sends(&self) -> &BlockInstructionSet {
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
        batches: &[Batch<'_, BlockInstructionSet>],
        seed: Seed,
        below: &mut impl Executor<Accepts = BlockInstructionSet>,
        readouts: &mut [BatchReadouts<BlockInstructionSet>],
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
    accepts: BlockInstructionSet,
    sends: BlockInstructionSet,
    provides: Capabilities<BlockInstructionSet>,
    /// The physical opcodes, in the receiver's, from `bind`.
    opcodes: [Opcode; 4],
}

impl Repetition3 {
    /// The lowering of `logical_qubits` logical qubits.
    pub(crate) fn new(logical_qubits: u32) -> Self {
        Self {
            accepts: LogicalGates::set(),
            sends: PhysicalGates::set(),
            provides: Capabilities::new(
                Resources::unconstrained(),
                Features::unconstrained(),
                BlockCapabilities::NONE.with_qubits(logical_qubits),
            ),
            opcodes: [
                PhysicalGates::RESET,
                PhysicalGates::X,
                PhysicalGates::CX,
                PhysicalGates::MZ,
            ],
        }
    }
}

impl Lowering for Repetition3 {
    type From = BlockInstructionSet;
    type To = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.accepts
    }

    fn sends(&self) -> &BlockInstructionSet {
        &self.sends
    }

    fn provides(&self) -> &Capabilities<BlockInstructionSet> {
        &self.provides
    }

    /// What arrives, on three physical qubits for each logical one.
    fn requires(
        &self,
        above: &Capabilities<BlockInstructionSet>,
    ) -> Capabilities<BlockInstructionSet> {
        let blocks = match above.set_capabilities().blocks(&BlockType::qubit()) {
            Some(logical) => BlockCapabilities::NONE.with_qubits(3 * logical),
            None => BlockCapabilities::NONE,
        };
        Capabilities::new(*above.resources(), *above.features(), blocks)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.opcodes = self.opcodes.map(|opcode| below.map(opcode));
    }

    #[inline]
    fn lower(&self, instruction: &BlockInstruction, into: &mut Vec<BlockInstruction>) {
        lower(instruction, self.opcodes, into);
    }

    #[inline]
    fn read(
        &self,
        _instruction: &BlockInstruction,
        lowered: &[ReadoutValue],
        into: &mut [ReadoutValue],
    ) {
        into[0] = majority(lowered);
    }

    #[inline]
    fn read_conditions(&self, lowered: ShotConditions) -> ShotConditions {
        lowered
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
fn lower(
    instruction: &BlockInstruction,
    opcodes: [Opcode; 4],
    lowered: &mut Vec<BlockInstruction>,
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
        lowered.push(BlockInstruction {
            opcode,
            blocks: instruction.blocks.iter().map(|q| 3 * q + i).collect(),
            arguments: Arguments::new(),
            select: None,
        });
    }
}

/// The value at least two of three readouts gave. A lost readout
/// (`Missing`) is no vote, so where no value has two, the logical readout
/// is lost too, with the first lost one's reason, rather than a bit made
/// up for it.
pub(crate) fn majority(three: &[ReadoutValue]) -> ReadoutValue {
    let votes = |value| three.iter().filter(|&&v| v == value).count();
    for value in [ReadoutValue::One, ReadoutValue::Zero] {
        if 2 * votes(value) > three.len() {
            return value;
        }
    }
    three
        .iter()
        .copied()
        .find(|v| matches!(v, ReadoutValue::Missing(_)))
        .unwrap_or(ReadoutValue::Missing(ReadoutFailure::Other))
}

#[cfg(test)]
mod tests {
    use block_instruction_set::{
        BlockCapabilities, BlockInstruction, BlockInstructionSet, ReadoutFailure, ReadoutValue,
        instruction_set,
    };
    use executor_protocol_core::{
        BoundaryCompatible as _, Capabilities, Features, Lowering, Resources,
    };

    use super::Repetition3;
    use crate::parts::LogicalGates;

    instruction_set! {
        /// The physical gates as a receiver numbers them, in another
        /// order than the lowering's: of the lowering's set's name, so
        /// they fit by mnemonic.
        PhysicalGates {
            MZ = mz(qubit) -> 1 outcome,
            CX = cx(qubit, qubit),
            RESET = reset(qubit),
            X = x(qubit),
        }
    }

    /// A logical instruction of the code, given to a receiver whose
    /// opcodes differ from the lowering's.
    fn lowered(mnemonic: &str, qubits: &[u32]) -> Vec<BlockInstruction> {
        let mut repetition = Repetition3::new(2);
        let binding = repetition
            .sends()
            .fits(&PhysicalGates::set())
            .expect("the physical gates fit the receiver's");
        repetition.bind(binding);
        let instruction = LogicalGates::set()
            .instruction(mnemonic, qubits.iter().copied(), &[])
            .expect("an instruction of LogicalGates");
        let mut into = Vec::new();
        repetition.lower(&instruction, &mut into);
        into
    }

    /// The receiver runs `cx` on logical qubits 0 and 1 as three `cx`, one
    /// on each pair of their physical qubits, in its own opcode.
    #[test]
    fn each_logical_instruction_is_three_physical_ones_on_its_qubits_blocks() {
        let receiver: BlockInstructionSet = PhysicalGates::set();
        for (mnemonic, physical, qubits, expected) in [
            (
                "prepare_z",
                PhysicalGates::RESET,
                vec![1],
                vec![vec![3], vec![4], vec![5]],
            ),
            (
                "x",
                PhysicalGates::X,
                vec![0],
                vec![vec![0], vec![1], vec![2]],
            ),
            (
                "cx",
                PhysicalGates::CX,
                vec![0, 1],
                vec![vec![0, 3], vec![1, 4], vec![2, 5]],
            ),
            (
                "measure_z",
                PhysicalGates::MZ,
                vec![1],
                vec![vec![3], vec![4], vec![5]],
            ),
        ] {
            let lowered = lowered(mnemonic, &qubits);
            let blocks: Vec<Vec<u32>> = lowered.iter().map(|i| i.blocks.to_vec()).collect();
            assert_eq!(blocks, expected, "{mnemonic}'s physical qubits");
            assert!(
                lowered.iter().all(|i| i.opcode == physical),
                "{mnemonic} lowered to {:?}, in the receiver's opcodes",
                lowered
                    .iter()
                    .map(|i| receiver.mnemonic(i.opcode))
                    .collect::<Vec<_>>()
            );
        }
    }

    /// A logical measurement reads as the value at least two of its three
    /// physical measurements gave; where lost readouts leave no value two,
    /// it is lost too, never a bit made up.
    #[test]
    fn a_logical_readout_is_the_majority_of_its_three_physical_ones() {
        use ReadoutValue::{Missing, One, Zero};
        let lost = Missing(ReadoutFailure::Loss);
        let repetition = Repetition3::new(1);
        let measure = LogicalGates::set()
            .instruction("measure_z", [0], &[])
            .expect("an instruction of LogicalGates");
        for (physical, logical) in [
            ([Zero, Zero, Zero], Zero),
            ([One, Zero, Zero], Zero),
            ([Zero, One, One], One),
            ([One, Zero, One], One),
            ([One, One, One], One),
            ([lost, One, One], One),
            ([Zero, lost, Zero], Zero),
            ([lost, Zero, One], lost),
            ([lost, lost, One], lost),
            ([lost, Missing(ReadoutFailure::Other), Zero], lost),
        ] {
            let mut into = [Zero];
            if logical == Zero {
                into = [One];
            }
            repetition.read(&measure, &physical, &mut into);
            assert_eq!(into, [logical], "the majority of {physical:?}");
        }
    }

    /// Built for 2 logical qubits, the code takes at most 2, and the shots
    /// above it reaching 2 require 6 physical qubits below; shots stating
    /// no qubits require none.
    #[test]
    fn each_logical_qubit_requires_three_physical_ones() {
        let repetition = Repetition3::new(2);
        let shots = |blocks| {
            Capabilities::new(
                Resources::unconstrained(),
                Features::unconstrained(),
                blocks,
            )
        };
        assert_eq!(
            repetition.provides().set_capabilities(),
            &BlockCapabilities::NONE.with_qubits(2),
            "what the code takes"
        );
        for (scene, above, below) in [
            (
                "2 logical qubits",
                BlockCapabilities::NONE.with_qubits(2),
                BlockCapabilities::NONE.with_qubits(6),
            ),
            (
                "no qubits stated",
                BlockCapabilities::NONE,
                BlockCapabilities::NONE,
            ),
        ] {
            assert_eq!(
                repetition.requires(&shots(above)).set_capabilities(),
                &below,
                "{scene}"
            );
        }
    }
}
