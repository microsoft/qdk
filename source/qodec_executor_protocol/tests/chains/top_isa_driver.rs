// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TopIsaDriver`]: a program of a qodec's top ISA, sent to every shot,
//! each of its `execute`s as one batch.

use std::sync::Arc;

use executor_protocol::{
    Argument, Batch, BatchReadouts, BlockType, Drive, DriveError, DriverSetup, Execute,
    ExecuteError, ExecuteRefusal, Failure, Instruction, InstructionSet, Needs, OpcodeMap,
    ReadoutValue, Seed, ShotConditions, ShotSet,
};
use qodec_executor_protocol::QodecInstructionSet;
use qodec_runtime_recipes::{Fault, Isa};

use crate::programs::{Program, ShotReadouts};

/// Sends `program` to every shot, and gives each shot's stream of the run's
/// seed (to drive Qodec's runtime directly with) and readouts. By default
/// an `execute`'s error ends the run; a driver
/// [`going_on_after_errors`](TopIsaDriver::going_on_after_errors) sends the
/// next `execute` anyway, as a debugger may, and gives how each ended.
pub struct TopIsaDriver {
    set: QodecInstructionSet,
    /// Each `execute`'s instructions, in the driver's set, then in the set
    /// below once `>>` binds them, and their readout slots.
    executes: Vec<(Vec<Instruction>, u32)>,
    needs: Needs,
    checked: bool,
    going_on: bool,
}

/// A shot's stream of the run's seed, its readouts, and how each `execute`
/// ended.
pub struct ShotRun {
    pub stream: u64,
    pub readouts: ShotReadouts,
    pub ends: Vec<ExecuteEnd>,
}

/// How an `execute` ended.
#[derive(Clone, Debug)]
pub enum ExecuteEnd {
    Ran,
    /// It failed: the failure's message, and the Qodec fault it is, if one.
    Failed {
        #[allow(dead_code, reason = "shown by a failing test's `Debug`")]
        message: String,
        fault: Option<Fault>,
    },
    Refused(ExecuteRefusal),
}

impl ExecuteEnd {
    fn of(error: ExecuteError) -> Self {
        match error {
            ExecuteError::Refused(refusal) => Self::Refused(refusal),
            ExecuteError::Failed { failure, .. } => {
                let fault = match &failure {
                    Failure::Part(error) => error.downcast_ref::<Fault>().cloned(),
                    _ => None,
                };
                Self::Failed {
                    message: failure.to_string(),
                    fault,
                }
            }
            _ => unreachable!("an execute fails or is refused"),
        }
    }
}

impl TopIsaDriver {
    /// `program`, of `top`, in unchecked batches.
    pub fn new(top: &Arc<Isa>, program: &Program) -> Self {
        let set = QodecInstructionSet::new(top).expect("the example's ISA is a protocol set");
        let mut needs = Needs::default();
        let executes = program
            .executes
            .iter()
            .map(|steps| {
                let instructions: Vec<Instruction> = steps
                    .iter()
                    .map(|step| {
                        add_needs(top, &step.mnemonic, &step.blocks, &mut needs);
                        let arguments: Vec<(&str, Argument)> = step
                            .numbers
                            .iter()
                            .map(|(name, value)| (name.as_str(), Argument::Number(*value)))
                            .collect();
                        set.instruction_set()
                            .instruction(&step.mnemonic, step.blocks.iter().copied(), &arguments)
                            .unwrap_or_else(|fault| panic!("`{}`: {fault}", step.mnemonic))
                    })
                    .collect();
                let slots = set
                    .instruction_set()
                    .readout_slots_of(&instructions)
                    .expect("fewer than 2^32 readout slots");
                (instructions, slots)
            })
            .collect();
        Self {
            set,
            executes,
            needs,
            checked: false,
            going_on: false,
        }
    }

    /// This driver, with instruction `at` of `execute` given one block more
    /// than it takes: a malformed instruction, which only the boundary of a
    /// checked batch refuses.
    pub fn with_a_block_too_many(mut self, execute: usize, at: usize) -> Self {
        let blocks = &mut self.executes[execute].0[at].blocks;
        let next = blocks.iter().max().map_or(0, |&block| block + 1);
        blocks.push(next);
        self
    }

    /// This driver, with instruction `at` of `execute` sent without its
    /// arguments: a malformed instruction, which only the boundary of a
    /// checked batch refuses.
    pub fn without_its_arguments(mut self, execute: usize, at: usize) -> Self {
        self.executes[execute].0[at].arguments.clear();
        self
    }

    /// This driver, sending each `execute` whatever the last gave.
    pub fn going_on_after_errors(mut self) -> Self {
        self.going_on = true;
        self
    }

    /// This driver, sending checked batches.
    pub fn checked(mut self) -> Self {
        self.checked = true;
        self
    }
}

/// Adds to `needs` the blocks `mnemonic` of `isa` acts on at `blocks`:
/// operand `j` is of the type of its `in[j]`, or else its `out[j]`.
fn add_needs(isa: &Isa, mnemonic: &str, blocks: &[u32], needs: &mut Needs) {
    let signature = isa
        .instructions
        .iter()
        .find(|signature| signature.mnemonic == mnemonic)
        .unwrap_or_else(|| panic!("`{mnemonic}` is in `{}`", isa.name));
    for (j, &block) in blocks.iter().enumerate() {
        let operand = signature
            .inputs
            .get(j)
            .or_else(|| signature.outputs.get(j))
            .expect("an operand of the instruction");
        let block_type = BlockType::new(isa.types.names[operand.block].clone());
        let held = needs.blocks.entry(block_type).or_insert(0);
        *held = (*held).max(block + 1);
    }
}

impl DriverSetup for TopIsaDriver {
    fn sends(&self) -> &InstructionSet {
        self.set.instruction_set()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for (instructions, _) in &mut self.executes {
            for instruction in instructions {
                instruction.opcode = below.map(instruction.opcode);
            }
        }
    }

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

impl Drive for TopIsaDriver {
    type Output = ShotRun;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<ShotRun>, DriveError> {
        let mut runs: Vec<ShotRun> = shots
            .iter()
            .map(|shot| ShotRun {
                stream: seed.shot(shot),
                readouts: ShotReadouts {
                    values: Vec::new(),
                    conditions: ShotConditions::NONE,
                },
                ends: Vec::new(),
            })
            .collect();
        for (instructions, slots) in &self.executes {
            let mut readouts = [BatchReadouts::new(0, 0)];
            let batch = Batch::new(shots, instructions, *slots).checked_if(self.checked);
            let end = match below.execute(&[batch], seed, &mut readouts).await {
                Ok(()) => ExecuteEnd::Ran,
                Err(error) if self.going_on => ExecuteEnd::of(error),
                Err(error) => return Err(error.into()),
            };
            for (position, run) in runs.iter_mut().enumerate() {
                if matches!(end, ExecuteEnd::Ran) {
                    let row = readouts[0].row(position);
                    run.readouts
                        .values
                        .extend(row.iter().map(|value| match value {
                            ReadoutValue::Zero => Some(false),
                            ReadoutValue::One => Some(true),
                            _ => None,
                        }));
                    run.readouts.conditions |= readouts[0].conditions(position);
                }
                run.ends.push(end.clone());
            }
        }
        Ok(runs)
    }
}
