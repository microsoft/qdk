// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The walk of a [`QirMachine`]: the loop that runs a context's bytecode,
//! instruction by instruction, from its position to its next stop
//! ([`Stop`]). It dispatches each instruction to its step: a classical
//! instruction computes in place, a quantum instruction appends its calls
//! to the context's batch, and a function call pushes or pops the
//! context's frames. The steps that change only the context are its own
//! ([`QirShotContext`]); those that need the program's prepared quantum
//! calls are here.

use super::super::QirTrap;
#[cfg(doc)]
use super::super::program::prepared_quantum_calls::PreparedQuantumCalls;
use super::super::program::prepared_quantum_calls::{CallSource, RegisterOperands};
use super::super::quantum_instruction_encoding::Register;
use super::classical_ops::{binary, bit, float_compare, int_compare, signed, unary, unsigned};
use super::{QirMachine, QirShotContext};
use crate::MeasurementResult;
use crate::bytecode::Instruction;
use crate::bytecode::encoding::{
    OP_ADD, OP_ALLOCA, OP_AND, OP_ASHR, OP_BRANCH, OP_CALL, OP_CALL_RETURN, OP_CONST, OP_FADD,
    OP_FCMP, OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM, OP_FSUB,
    OP_GEP, OP_ICMP, OP_INTTOPTR, OP_JUMP, OP_LOAD, OP_LSHR, OP_MEASURE, OP_MOV, OP_MUL, OP_OR,
    OP_PEEK_LOSS, OP_PHI, OP_QUANTUM_GATE, OP_READ_LOSS, OP_READ_RESULT, OP_READOUT_NOISE,
    OP_RECORD_OUTPUT, OP_RESET, OP_RET, OP_SDIV, OP_SELECT, OP_SEXT, OP_SHL, OP_SITOFP, OP_SREM,
    OP_STORE, OP_SUB, OP_SWITCH, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_WRITE_RESULT, OP_XOR,
    OP_ZEXT,
};
use std::ops::ControlFlow;

/// Where a walk stopped.
pub(super) enum Stop {
    Batch,
    Ended,
}

impl QirMachine<'_> {
    /// Runs `context` from its position to its next batch or its end,
    /// dispatching each instruction to its step. A quantum instruction
    /// appends its prepared calls ([`PreparedQuantumCalls`]).
    // The bytecode's words index its tables and registers: the check
    // refused every index past them, so none is truncated.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub(super) fn walk(self, context: &mut QirShotContext) -> Result<Stop, QirTrap> {
        let bytecode = self.bytecode;
        let mut pc = context.pc;
        let stop = loop {
            let instruction = &bytecode.instructions[pc];
            let dst = instruction.dst as Register;
            let opcode = instruction.primary_opcode();
            match opcode {
                OP_RET => {
                    // No frame is left: the check refused a `RET` inside a
                    // function.
                    if !context.batch.is_empty() {
                        break Stop::Batch;
                    }
                    context.finish(self.program.records_output(), pc)?;
                    break Stop::Ended;
                }
                OP_JUMP => pc = context.enter(bytecode, instruction.dst),
                OP_BRANCH => {
                    let block = if bit(pc, context.src0(instruction))? {
                        instruction.aux0
                    } else {
                        instruction.aux1
                    };
                    pc = context.enter(bytecode, block);
                }
                OP_SWITCH => {
                    let value = context.src0(instruction);
                    let block = bytecode.switch_cases[instruction.aux1 as usize..]
                        [..instruction.aux2 as usize]
                        .iter()
                        .find(|case| case.case_val == value)
                        .map_or(instruction.aux0, |case| case.target_block);
                    pc = context.enter(bytecode, block);
                }
                OP_CALL => pc = context.call_function(bytecode, instruction, pc),
                OP_CALL_RETURN => pc = context.return_from_function(instruction),
                OP_QUANTUM_GATE | OP_MEASURE | OP_RESET | OP_PEEK_LOSS | OP_READOUT_NOISE => {
                    match self.append_quantum_calls(context, instruction, pc)? {
                        ControlFlow::Continue(next) => pc = next,
                        ControlFlow::Break(resume_at) => {
                            pc = resume_at;
                            break Stop::Batch;
                        }
                    }
                }
                OP_READ_RESULT => {
                    let result = context.src0(instruction);
                    if context.is_pending(result) {
                        break Stop::Batch;
                    }
                    let one = context.result(bytecode, pc, result)? == MeasurementResult::One;
                    context.set(dst, u64::from(one));
                    pc += 1;
                }
                OP_READ_LOSS => {
                    let result = context.src0(instruction);
                    if context.is_pending(result) {
                        break Stop::Batch;
                    }
                    let loss = context.result(bytecode, pc, result)? == MeasurementResult::Loss;
                    context.set(dst, u64::from(loss));
                    pc += 1;
                }
                OP_WRITE_RESULT => {
                    if context.write_result(bytecode, instruction, pc)?.is_break() {
                        break Stop::Batch;
                    }
                    pc += 1;
                }
                OP_RECORD_OUTPUT => {
                    if context.record_output(bytecode, instruction, pc)?.is_break() {
                        break Stop::Batch;
                    }
                    pc += 1;
                }
                OP_ADD | OP_SUB | OP_MUL | OP_UDIV | OP_UREM | OP_SDIV | OP_SREM | OP_AND
                | OP_OR | OP_XOR | OP_SHL | OP_LSHR | OP_ASHR | OP_FADD | OP_FSUB | OP_FMUL
                | OP_FDIV | OP_FREM => {
                    let value =
                        binary(opcode, context.src0(instruction), context.src1(instruction))
                            .ok_or(QirTrap::DivisionByZero { instruction: pc })?;
                    context.set(dst, value);
                    pc += 1;
                }
                OP_ICMP => {
                    let value = int_compare(
                        instruction.sub_opcode(),
                        context.src0(instruction),
                        context.src1(instruction),
                    );
                    context.set(dst, u64::from(value));
                    pc += 1;
                }
                OP_FCMP => {
                    let value = float_compare(
                        instruction.sub_opcode(),
                        f64::from_bits(context.src0(instruction)),
                        f64::from_bits(context.src1(instruction)),
                    );
                    context.set(dst, u64::from(value));
                    pc += 1;
                }
                OP_ZEXT | OP_TRUNC | OP_INTTOPTR | OP_MOV | OP_FPEXT | OP_FPTRUNC | OP_FPTOSI
                | OP_FPTOUI | OP_SITOFP | OP_UITOFP => {
                    context.set(dst, unary(opcode, context.src0(instruction)));
                    pc += 1;
                }
                OP_CONST => {
                    context.set(dst, instruction.src0);
                    pc += 1;
                }
                OP_SEXT => {
                    // A run reads the width in `aux0` as is, whatever its
                    // flag.
                    let bits = instruction.aux0 as u32;
                    let a = context.src0(instruction);
                    let value = if bits > 0 && bits < 64 {
                        let shift = 64 - bits;
                        unsigned(signed(a).wrapping_shl(shift).wrapping_shr(shift))
                    } else {
                        a
                    };
                    context.set(dst, value);
                    pc += 1;
                }
                OP_SELECT => {
                    let value = if bit(pc, context.src0(instruction))? {
                        context.aux0(instruction)
                    } else {
                        context.aux1(instruction)
                    };
                    context.set(dst, value);
                    pc += 1;
                }
                OP_PHI => {
                    // Only `PHI`s and `MOV`s of an immediate come before it
                    // in its block (the check refused any other), so the
                    // last edge is the one into its block, and the check
                    // refused a `PHI` with no entry for it.
                    let entry = bytecode.phi_entries[instruction.aux0 as usize..]
                        [..instruction.aux1 as usize]
                        .iter()
                        .find(|entry| entry.block_id == context.previous_block)
                        .expect("the check refused a `PHI` with no entry for a predecessor");
                    context.set(dst, context.registers[entry.val_reg as usize]);
                    pc += 1;
                }
                OP_ALLOCA => {
                    let address = context.memory.len() as u64;
                    context.grow_memory(address.saturating_add(context.src0(instruction)));
                    context.set(dst, address);
                    pc += 1;
                }
                OP_LOAD => {
                    let value = context.load(context.src0(instruction), pc)?;
                    context.set(dst, value);
                    pc += 1;
                }
                OP_STORE => {
                    context.store(context.src1(instruction), context.src0(instruction), pc)?;
                    pc += 1;
                }
                OP_GEP => {
                    let address = context.src0(instruction).wrapping_add(
                        context
                            .src1(instruction)
                            .wrapping_mul(context.aux0(instruction)),
                    );
                    context.set(dst, address);
                    pc += 1;
                }
                // `NOP`; every other opcode was refused by the check.
                _ => pc += 1,
            }
        };
        context.pc = pc;
        Ok(stop)
    }

    /// Appends the calls of the quantum instruction at `pc` to the batch,
    /// from its prepared source ([`CallSource`]). `Continue` holds the
    /// instruction to run next. `Break` stops the batch at a call that
    /// writes a result the batch writes already
    /// ([`QirShotContext::mark_pending`]), and holds that call's
    /// instruction, which the context resumes from.
    fn append_quantum_calls(
        self,
        context: &mut QirShotContext,
        instruction: &Instruction<u64>,
        pc: usize,
    ) -> Result<ControlFlow<usize, usize>, QirTrap> {
        let source = self
            .prepared_quantum_calls
            .source(pc)
            .expect("every quantum instruction has a prepared source");
        match source {
            CallSource::Prebuilt { first, end, result } => {
                Ok(self.append_prebuilt_calls(context, pc, *first, *end, *result))
            }
            CallSource::BuiltFromRegisters(operands) => {
                self.append_call_built_from_registers(context, pc, operands)
            }
            CallSource::CorrelatedNoise { table } => {
                let call = context.correlated_noise_call(
                    self.bytecode,
                    pc,
                    *table,
                    context.aux1(instruction),
                    context.aux2(instruction),
                )?;
                context.batch.push(call);
                Ok(ControlFlow::Continue(pc + 1))
            }
        }
    }

    /// Appends the prebuilt calls of the run of quantum instructions from
    /// `pc` to `end`, as one slice. Only the run's last call may write a
    /// result, `result`: when the batch writes it already, the calls before
    /// it are appended, and the batch stops at it.
    fn append_prebuilt_calls(
        self,
        context: &mut QirShotContext,
        pc: usize,
        first: u32,
        end: u32,
        result: Option<u32>,
    ) -> ControlFlow<usize, usize> {
        let calls = self.prepared_quantum_calls.prebuilt(pc, first, end);
        let end = end as usize;
        if let Some(result) = result
            && !context.mark_pending(result)
        {
            let (before, _) = calls.split_at(calls.len() - 1);
            context.batch.extend_from_slice(before);
            return ControlFlow::Break(end - 1);
        }
        context.batch.extend_from_slice(calls);
        ControlFlow::Continue(end)
    }

    /// Appends the call of the quantum instruction at `pc`, built from the
    /// registers that `operands` name; or stops the batch at it, when the
    /// batch writes its result already.
    fn append_call_built_from_registers(
        self,
        context: &mut QirShotContext,
        pc: usize,
        operands: &RegisterOperands,
    ) -> Result<ControlFlow<usize, usize>, QirTrap> {
        let call = match context.call(self.bytecode, pc, operands) {
            Ok(call) => call,
            // The stop comes before the trap: the batch runs first, as
            // `run_shot` measures before it panics.
            Err(_)
                if context
                    .written(operands)
                    .is_some_and(|result| context.is_pending(result)) =>
            {
                return Ok(ControlFlow::Break(pc));
            }
            Err(trap) => return Err(trap),
        };
        if let Some(result) = call.result()
            && !context.mark_pending(result)
        {
            return Ok(ControlFlow::Break(pc));
        }
        context.batch.push(call);
        Ok(ControlFlow::Continue(pc + 1))
    }
}
