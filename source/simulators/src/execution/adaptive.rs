// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adaptive bytecode preparation and command production.

use std::{fmt, ops::Range};

use num_traits::Unsigned;

use crate::{
    MeasurementResult, OutputRecord,
    bytecode::{AdaptiveProgram, Instruction},
};

use super::{
    AdaptiveCommand, AdaptiveResponse, MeasurementKind, MeasurementRequest, OPID_MRESETZ, OPID_MZ,
    QuantumEvolutionRegion, RegionId, UnitaryOperation, resolve_unitary_operation,
};

pub(super) const OP_QUANTUM_GATE: u64 = 0x10;
const OP_RET: u8 = 0x02;
const OP_JUMP: u8 = 0x04;
const OP_BRANCH: u8 = 0x05;
const OP_MEASURE: u8 = 0x11;
const OP_READ_RESULT: u8 = 0x13;
const OP_RECORD_OUTPUT: u8 = 0x14;

const FLAG_SRC0_IMM: u64 = 1 << 16;
const FLAG_AUX1_IMM: u64 = 1 << 20;
const FLAG_AUX2_IMM: u64 = 1 << 21;

fn bytecode_index(value: u64) -> usize {
    usize::try_from(value).expect("adaptive bytecode index should fit in usize")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionSite {
    pub id: RegionId,
    pub block_id: u32,
    pub instruction_range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionPartitionError {
    InvalidBlockRange {
        block_id: usize,
        start: usize,
        end: usize,
    },
    InvalidOperationIndex {
        instruction_index: usize,
        operation_index: usize,
    },
    TooManyRegions {
        maximum: u32,
    },
}

impl fmt::Display for RegionPartitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBlockRange {
                block_id,
                start,
                end,
            } => write!(
                formatter,
                "adaptive block {block_id} has an invalid instruction range {start}..{end}"
            ),
            Self::InvalidOperationIndex {
                instruction_index,
                operation_index,
            } => write!(
                formatter,
                "adaptive instruction {instruction_index} references missing quantum operation {operation_index}"
            ),
            Self::TooManyRegions { maximum } => write!(
                formatter,
                "adaptive program contains more than {maximum} quantum evolution regions"
            ),
        }
    }
}

impl std::error::Error for RegionPartitionError {}

pub fn partition_unitary_regions<Word>(
    program: &AdaptiveProgram<Word>,
) -> Result<Vec<RegionSite>, RegionPartitionError>
where
    Word: Unsigned + Copy + Into<u64>,
{
    let mut regions = Vec::new();
    for (block_id, block) in program.block_table.iter().enumerate() {
        let start = usize::try_from(block.instr_offset.into()).map_err(|_| {
            RegionPartitionError::InvalidBlockRange {
                block_id,
                start: usize::MAX,
                end: usize::MAX,
            }
        })?;
        let count = usize::try_from(block.instr_count.into()).map_err(|_| {
            RegionPartitionError::InvalidBlockRange {
                block_id,
                start,
                end: usize::MAX,
            }
        })?;
        let end = start
            .checked_add(count)
            .ok_or(RegionPartitionError::InvalidBlockRange {
                block_id,
                start,
                end: usize::MAX,
            })?;
        if end > program.instructions.len() {
            return Err(RegionPartitionError::InvalidBlockRange {
                block_id,
                start,
                end,
            });
        }

        let mut region_start = None;
        for instruction_index in start..end {
            if is_unitary_instruction(program, instruction_index)? {
                region_start.get_or_insert(instruction_index);
            } else if let Some(region_start) = region_start.take() {
                push_region(&mut regions, block_id, region_start..instruction_index)?;
            }
        }
        if let Some(region_start) = region_start {
            push_region(&mut regions, block_id, region_start..end)?;
        }
    }
    Ok(regions)
}

#[derive(Debug)]
pub struct PreparedAdaptiveProgram<Word: Unsigned> {
    program: AdaptiveProgram<Word>,
    regions: Box<[RegionSite]>,
}

impl<Word> PreparedAdaptiveProgram<Word>
where
    Word: Unsigned + Copy + Into<u64>,
{
    pub fn new(program: AdaptiveProgram<Word>) -> Result<Self, RegionPartitionError> {
        let regions = partition_unitary_regions(&program)?.into_boxed_slice();
        Ok(Self { program, regions })
    }

    #[must_use]
    pub fn program(&self) -> &AdaptiveProgram<Word> {
        &self.program
    }

    #[must_use]
    pub fn regions(&self) -> &[RegionSite] {
        &self.regions
    }

    #[must_use]
    pub fn into_program(self) -> AdaptiveProgram<Word> {
        self.program
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdaptiveExecutionError {
    MissingResponse,
    UnexpectedResponse,
    UnsupportedInstruction {
        opcode: u8,
        instruction_index: usize,
    },
    UnsupportedMeasurement {
        operation_id: u64,
        instruction_index: usize,
    },
}

impl fmt::Display for AdaptiveExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingResponse => write!(formatter, "execution response is required"),
            Self::UnexpectedResponse => write!(formatter, "execution response was not expected"),
            Self::UnsupportedInstruction {
                opcode,
                instruction_index,
            } => write!(
                formatter,
                "unsupported adaptive opcode 0x{opcode:02X} at instruction {instruction_index}"
            ),
            Self::UnsupportedMeasurement {
                operation_id,
                instruction_index,
            } => write!(
                formatter,
                "unsupported adaptive measurement {operation_id} at instruction {instruction_index}"
            ),
        }
    }
}

impl std::error::Error for AdaptiveExecutionError {}

#[derive(Clone, Copy)]
enum AdaptiveExecutionState {
    Ready,
    AwaitingRegionCompletion,
    AwaitingMeasurementResult { result_id: usize },
    Complete,
}

pub struct AdaptiveExecution<'program> {
    prepared_program: &'program PreparedAdaptiveProgram<u64>,
    instruction_index: usize,
    current_block_id: u64,
    registers: Vec<u64>,
    measurements: Vec<MeasurementResult>,
    records: Vec<OutputRecord>,
    state: AdaptiveExecutionState,
}

impl<'program> AdaptiveExecution<'program> {
    #[must_use]
    pub fn new(prepared_program: &'program PreparedAdaptiveProgram<u64>) -> Self {
        let program = prepared_program.program();
        let current_block_id = program.entry_block;
        let instruction_index =
            bytecode_index(program.block_table[bytecode_index(current_block_id)].instr_offset);
        Self {
            prepared_program,
            instruction_index,
            current_block_id,
            registers: vec![
                0;
                usize::try_from(program.num_registers)
                    .expect("adaptive register count should fit in usize")
            ],
            measurements: vec![
                MeasurementResult::Zero;
                usize::try_from(program.num_results)
                    .expect("adaptive result count should fit in usize")
            ],
            records: Vec::new(),
            state: AdaptiveExecutionState::Ready,
        }
    }

    pub fn next_command(
        &mut self,
        previous_response: Option<AdaptiveResponse>,
    ) -> Result<AdaptiveCommand, AdaptiveExecutionError> {
        self.accept_response(previous_response)?;

        loop {
            if let Some(region) = self.region_starting_here() {
                let region_id = region.id;
                let operations = region
                    .instruction_range
                    .clone()
                    .map(|index| self.resolve_unitary(index))
                    .collect::<Vec<_>>();
                self.instruction_index = region.instruction_range.end;
                self.state = AdaptiveExecutionState::AwaitingRegionCompletion;
                return Ok(AdaptiveCommand::ExecuteRegion {
                    region_id,
                    region: QuantumEvolutionRegion::new(operations),
                });
            }

            let program = self.prepared_program.program();
            let instruction = program.instructions[self.instruction_index];
            match instruction.primary_opcode() {
                OP_RET => {
                    self.state = AdaptiveExecutionState::Complete;
                    return Ok(AdaptiveCommand::Complete(std::mem::take(&mut self.records)));
                }
                OP_JUMP => self.jump(instruction.dst),
                OP_BRANCH => {
                    let condition = self.resolve_u64(instruction.src0, instruction.opcode, 0) != 0;
                    self.jump(if condition {
                        instruction.aux0
                    } else {
                        instruction.aux1
                    });
                }
                OP_MEASURE => return self.measurement_command(instruction),
                OP_READ_RESULT => {
                    let result_id =
                        bytecode_index(self.resolve_u64(instruction.src0, instruction.opcode, 0));
                    self.registers[bytecode_index(instruction.dst)] = u64::from(matches!(
                        self.measurements[result_id],
                        MeasurementResult::One
                    ));
                    self.instruction_index += 1;
                }
                OP_RECORD_OUTPUT => {
                    if instruction.aux1 == 0 {
                        let result_id = bytecode_index(self.resolve_u64(
                            instruction.src0,
                            instruction.opcode,
                            0,
                        ));
                        self.records
                            .push(OutputRecord::Result(self.measurements[result_id]));
                    }
                    self.instruction_index += 1;
                }
                opcode => {
                    return Err(AdaptiveExecutionError::UnsupportedInstruction {
                        opcode,
                        instruction_index: self.instruction_index,
                    });
                }
            }
        }
    }

    fn accept_response(
        &mut self,
        response: Option<AdaptiveResponse>,
    ) -> Result<(), AdaptiveExecutionError> {
        match (self.state, response) {
            (AdaptiveExecutionState::Ready, None) => Ok(()),
            (
                AdaptiveExecutionState::AwaitingRegionCompletion,
                Some(AdaptiveResponse::RegionComplete),
            ) => {
                self.state = AdaptiveExecutionState::Ready;
                Ok(())
            }
            (
                AdaptiveExecutionState::AwaitingMeasurementResult { result_id },
                Some(AdaptiveResponse::Measurement(result)),
            ) => {
                self.measurements[result_id] = result;
                self.state = AdaptiveExecutionState::Ready;
                Ok(())
            }
            (
                AdaptiveExecutionState::AwaitingRegionCompletion
                | AdaptiveExecutionState::AwaitingMeasurementResult { .. },
                None,
            ) => Err(AdaptiveExecutionError::MissingResponse),
            (AdaptiveExecutionState::Complete, _) | (AdaptiveExecutionState::Ready, Some(_)) => {
                Err(AdaptiveExecutionError::UnexpectedResponse)
            }
            _ => Err(AdaptiveExecutionError::UnexpectedResponse),
        }
    }

    fn region_starting_here(&self) -> Option<&RegionSite> {
        self.prepared_program.regions.iter().find(|region| {
            u64::from(region.block_id) == self.current_block_id
                && region.instruction_range.start == self.instruction_index
        })
    }

    fn resolve_unitary(&self, instruction_index: usize) -> UnitaryOperation {
        let program = self.prepared_program.program();
        let instruction = program.instructions[instruction_index];
        let operation_id = program.quantum_ops[bytecode_index(instruction.aux0)].op_id;
        let angle = f64::from_bits(self.resolve_u64(instruction.src0, instruction.opcode, 0));
        let q1 = bytecode_index(self.resolve_u64(instruction.aux1, instruction.opcode, 4));
        let q2 = bytecode_index(self.resolve_u64(instruction.aux2, instruction.opcode, 5));
        resolve_unitary_operation(operation_id, angle, q1, q2)
            .expect("prepared region should contain only supported unitary operations")
    }

    fn measurement_command(
        &mut self,
        instruction: Instruction<u64>,
    ) -> Result<AdaptiveCommand, AdaptiveExecutionError> {
        let operation_id =
            self.prepared_program.program().quantum_ops[bytecode_index(instruction.aux0)].op_id;
        let kind = match operation_id {
            OPID_MZ => MeasurementKind::MeasureZ,
            OPID_MRESETZ => MeasurementKind::MeasureResetZ,
            operation_id => {
                return Err(AdaptiveExecutionError::UnsupportedMeasurement {
                    operation_id,
                    instruction_index: self.instruction_index,
                });
            }
        };
        let request = MeasurementRequest {
            kind,
            qubit: bytecode_index(self.resolve_u64(instruction.aux1, instruction.opcode, 4)),
            result_id: bytecode_index(self.resolve_u64(instruction.aux2, instruction.opcode, 5)),
        };
        self.instruction_index += 1;
        self.state = AdaptiveExecutionState::AwaitingMeasurementResult {
            result_id: request.result_id,
        };
        Ok(AdaptiveCommand::Measure(request))
    }

    fn jump(&mut self, block_id: u64) {
        self.current_block_id = block_id;
        self.instruction_index = bytecode_index(
            self.prepared_program.program().block_table[bytecode_index(block_id)].instr_offset,
        );
    }

    fn resolve_u64(&self, operand: u64, flags: u64, operand_index: u64) -> u64 {
        let immediate_flag = match operand_index {
            0 => FLAG_SRC0_IMM,
            4 => FLAG_AUX1_IMM,
            5 => FLAG_AUX2_IMM,
            _ => 0,
        };
        if flags & immediate_flag != 0 {
            operand
        } else {
            self.registers[bytecode_index(operand)]
        }
    }
}

fn is_unitary_instruction<Word>(
    program: &AdaptiveProgram<Word>,
    instruction_index: usize,
) -> Result<bool, RegionPartitionError>
where
    Word: Unsigned + Copy + Into<u64>,
{
    let instruction = &program.instructions[instruction_index];
    if instruction.opcode.into() & 0xFF != OP_QUANTUM_GATE {
        return Ok(false);
    }
    let operation_index = usize::try_from(instruction.aux0.into()).map_err(|_| {
        RegionPartitionError::InvalidOperationIndex {
            instruction_index,
            operation_index: usize::MAX,
        }
    })?;
    let operation = program.quantum_ops.get(operation_index).ok_or(
        RegionPartitionError::InvalidOperationIndex {
            instruction_index,
            operation_index,
        },
    )?;
    let operation_id = operation.op_id.into();
    Ok(matches!(operation_id, 0 | 2..=19 | 24 | 29))
}

fn push_region(
    regions: &mut Vec<RegionSite>,
    block_id: usize,
    instruction_range: Range<usize>,
) -> Result<(), RegionPartitionError> {
    let id = u32::try_from(regions.len())
        .map_err(|_| RegionPartitionError::TooManyRegions { maximum: u32::MAX })?;
    let block_id = u32::try_from(block_id)
        .map_err(|_| RegionPartitionError::TooManyRegions { maximum: u32::MAX })?;
    regions.push(RegionSite {
        id: RegionId::new(id),
        block_id,
        instruction_range,
    });
    Ok(())
}