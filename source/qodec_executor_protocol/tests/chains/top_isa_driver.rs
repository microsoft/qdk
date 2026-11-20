// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TopIsaDriver`]: a program of a qodec's top ISA, sent to every shot,
//! or to the shots chosen for an `execute`, each `execute` as one batch or
//! as the batches chosen for it, ending chosen shots after an `execute`.

use std::sync::Arc;

use executor_protocol::{
    Argument, Batch, BatchReadouts, Bit, BlockType, Drive, DriveError, DriverSetup, Execute,
    ExecuteError, ExecuteRefusal, Failure, Instruction, InstructionSet, Needs, Opcode, OpcodeMap,
    ReadoutValue, Seed, ShotConditions, ShotSet,
};
use qodec_executor_protocol::QodecInstructionSet;
use qodec_runtime_recipes::{Fault, Isa, ParamKind};

use crate::programs::{Program, ShotReadouts};

/// Sends `program` to every shot, or each `execute` to the shots chosen for
/// it ([`sending_execute_to`](TopIsaDriver::sending_execute_to)), in one
/// batch or in the batches chosen for it
/// ([`sending_execute_in_batches`](TopIsaDriver::sending_execute_in_batches)),
/// ending the shots chosen after an `execute`
/// ([`ending_shots_after`](TopIsaDriver::ending_shots_after)), and gives
/// each shot's stream of the run's seed (to drive Qodec's runtime directly
/// with) and readouts. By default an `execute`'s error ends the run; a
/// driver
/// [`going_on_after_errors`](TopIsaDriver::going_on_after_errors) sends the
/// next `execute` anyway, as a debugger may, and gives how each ended.
pub struct TopIsaDriver {
    set: QodecInstructionSet,
    /// Each `execute`'s instructions, in the driver's set, then in the set
    /// below once `>>` binds them, and their readout slots.
    executes: Vec<(Vec<Instruction>, u32)>,
    /// The positions, in the run's shots, each batch of each `execute`
    /// goes to: one batch of all of them when `None`.
    targets: Vec<Option<Vec<Vec<usize>>>>,
    /// Opcodes sent in place of those `bind` gives: each `execute`'s,
    /// instruction's place in it, and opcode.
    sent_opcodes: Vec<(usize, usize, Opcode)>,
    /// The shots ended after an `execute`: the `execute`, and the shots'
    /// positions in the run's shots.
    endings: Vec<(usize, Vec<usize>)>,
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
#[derive(Clone, Debug, PartialEq)]
pub enum ExecuteEnd {
    Ran,
    /// It failed: the failure's message, the Qodec fault it is, if one, and
    /// the refusal below it is, if one.
    Failed {
        #[allow(dead_code, reason = "shown by a failing test's `Debug`")]
        message: String,
        fault: Option<Fault>,
        refusal: Option<ExecuteRefusal>,
    },
    /// A part below panicked: the panic's message, if text.
    Panicked {
        #[allow(dead_code, reason = "shown by a failing test's `Debug`")]
        message: Option<String>,
    },
    Refused(ExecuteRefusal),
}

impl ExecuteEnd {
    fn of(error: ExecuteError) -> Self {
        match error {
            ExecuteError::Refused(refusal) => Self::Refused(refusal),
            ExecuteError::Failed {
                failure: Failure::Panicked { message, .. },
                ..
            } => Self::Panicked { message },
            ExecuteError::Failed { failure, .. } => {
                let fault = match &failure {
                    Failure::Part(error) => error.downcast_ref::<Fault>().cloned(),
                    _ => None,
                };
                let refusal = match &failure {
                    Failure::Refused(refusal) => Some(refusal.clone()),
                    _ => None,
                };
                Self::Failed {
                    message: failure.to_string(),
                    fault,
                    refusal,
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
        let executes: Vec<(Vec<Instruction>, u32)> = program
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
                            .map(|(name, value)| {
                                let argument = match parameter_kind(top, &step.mnemonic, name) {
                                    ParamKind::Bit => Argument::Bit(Bit::Value(*value != 0.0)),
                                    _ => Argument::Number(*value),
                                };
                                (name.as_str(), argument)
                            })
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
        let targets = vec![None; executes.len()];
        Self {
            set,
            executes,
            targets,
            sent_opcodes: Vec::new(),
            endings: Vec::new(),
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

    /// This driver, with each `bit` argument of instruction `at` of
    /// `execute` replaced by the parity of the shot's latest readout: an
    /// argument a qodec's call has no form for, which this driver's needs
    /// do not state, so that only the boundary of a checked batch refuses
    /// it.
    pub fn with_a_readout_parity(mut self, execute: usize, at: usize) -> Self {
        for argument in &mut self.executes[execute].0[at].arguments {
            if matches!(argument, Argument::Bit(_)) {
                *argument = Argument::recent(1);
            }
        }
        self
    }

    /// This driver, sending instruction `at` of `execute` with `opcode`, in
    /// place of the opcode the set below has for it: an instruction no
    /// fit gives, which only an unchecked batch can carry.
    pub fn sending_opcode(mut self, execute: usize, at: usize, opcode: Opcode) -> Self {
        self.sent_opcodes.push((execute, at, opcode));
        self
    }

    /// This driver, stating `slots` readout slots for `execute`, whatever
    /// its instructions read: a misstatement only an unchecked batch can
    /// carry.
    pub fn stating_readout_slots(mut self, execute: usize, slots: u32) -> Self {
        self.executes[execute].1 = slots;
        self
    }

    /// This driver, sending `execute` only to the shots at `positions` of
    /// the run's shots: the others' ends and readouts skip it.
    pub fn sending_execute_to(self, execute: usize, positions: &[usize]) -> Self {
        self.sending_execute_in_batches(execute, &[positions])
    }

    /// This driver, sending `execute` as one batch per entry of `batches`,
    /// in order, each to the shots at its positions of the run's shots:
    /// the others' ends and readouts skip it.
    pub fn sending_execute_in_batches(mut self, execute: usize, batches: &[&[usize]]) -> Self {
        self.targets[execute] = Some(batches.iter().map(|positions| positions.to_vec()).collect());
        self
    }

    /// This driver, ending the shots at `positions` of the run's shots once
    /// `execute` ran, as a driver that has read all it wants of them may:
    /// the parts below may then drop what they hold for them.
    pub fn ending_shots_after(mut self, execute: usize, positions: &[usize]) -> Self {
        self.endings.push((execute, positions.to_vec()));
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

/// The kind of parameter `name` of `mnemonic` in `isa`.
fn parameter_kind(isa: &Isa, mnemonic: &str, name: &str) -> ParamKind {
    isa.instructions
        .iter()
        .find(|signature| signature.mnemonic == mnemonic)
        .and_then(|signature| {
            signature
                .parameters
                .iter()
                .find(|parameter| parameter.name == name)
        })
        .unwrap_or_else(|| panic!("`{mnemonic}` of `{}` has a parameter `{name}`", isa.name))
        .kind
}

/// Adds to `needs` the blocks `mnemonic` of `isa` acts on at `blocks`:
/// operand `j` is a block of the type of its `in[j]` and one of its
/// `out[j]`'s, at index `blocks[j]` among each type's, as a merge that
/// changes a block's type keeps its index.
fn add_needs(isa: &Isa, mnemonic: &str, blocks: &[u32], needs: &mut Needs) {
    let signature = isa
        .instructions
        .iter()
        .find(|signature| signature.mnemonic == mnemonic)
        .unwrap_or_else(|| panic!("`{mnemonic}` is in `{}`", isa.name));
    for (j, &block) in blocks.iter().enumerate() {
        let operands = [signature.inputs.get(j), signature.outputs.get(j)];
        assert!(
            operands.iter().any(Option::is_some),
            "`{mnemonic}` has an operand {j}"
        );
        for operand in operands.into_iter().flatten() {
            let block_type = BlockType::new(isa.types.names[operand.block].clone());
            let held = needs.blocks.entry(block_type).or_insert(0);
            *held = (*held).max(block + 1);
        }
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
        for &(execute, at, opcode) in &self.sent_opcodes {
            self.executes[execute].0[at].opcode = opcode;
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
        for (execute, ((instructions, slots), target)) in
            self.executes.iter().zip(&self.targets).enumerate()
        {
            let batch_positions: Vec<Vec<usize>> = match target {
                Some(batch_positions) => batch_positions.clone(),
                None => vec![(0..shots.len()).collect()],
            };
            let targeted: Vec<ShotSet> = batch_positions
                .iter()
                .map(|positions| {
                    shots
                        .select(positions)
                        .expect("the chosen positions are among the run's shots")
                })
                .collect();
            let batches: Vec<Batch<'_>> = targeted
                .iter()
                .map(|targeted| Batch::new(targeted, instructions, *slots).checked_if(self.checked))
                .collect();
            let mut readouts: Vec<BatchReadouts> =
                batches.iter().map(|_| BatchReadouts::new(0, 0)).collect();
            let end = match below.execute(&batches, seed, &mut readouts).await {
                Ok(()) => ExecuteEnd::Ran,
                Err(error) if self.going_on => ExecuteEnd::of(error),
                Err(error) => return Err(error.into()),
            };
            for (positions, batch_readouts) in batch_positions.iter().zip(&readouts) {
                for (row_index, &position) in positions.iter().enumerate() {
                    let run = &mut runs[position];
                    if matches!(end, ExecuteEnd::Ran) {
                        let row = batch_readouts.row(row_index);
                        run.readouts
                            .values
                            .extend(row.iter().map(|value| match value {
                                ReadoutValue::Zero => Some(false),
                                ReadoutValue::One => Some(true),
                                _ => None,
                            }));
                        run.readouts.conditions |= batch_readouts.conditions(row_index);
                    }
                    run.ends.push(end.clone());
                }
            }
            for (_, positions) in self.endings.iter().filter(|(after, _)| *after == execute) {
                below.end_shots(
                    &shots
                        .select(positions)
                        .expect("the chosen positions are among the run's shots"),
                );
            }
        }
        Ok(runs)
    }
}
