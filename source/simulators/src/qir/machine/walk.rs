// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The walk of a [`QirMachine`]: the loop that runs a context's bytecode,
//! instruction by instruction, from its position to its next stop
//! ([`Stop`]). It runs each classical instruction (what the arithmetic and
//! the comparisons compute is in `classical_ops.rs`), and appends each
//! quantum instruction's quantum calls to the context's batch: its
//! prebuilt calls, or the call the context builds from its registers
//! (`shot_context.rs`). Function calls push and pop the context's frames.

use super::super::program::prepared_quantum_calls::CallSource;
#[cfg(doc)]
use super::super::program::prepared_quantum_calls::PreparedQuantumCalls;
use super::super::quantum_instruction_encoding::Register;
use super::super::{QirReadout, QirTrap};
use super::classical_ops::{binary, bit, float_compare, int_compare, signed, unary, unsigned};
use super::shot_context::Frame;
use super::{QirMachine, QirShotContext, block_start};
use crate::bytecode::encoding::{
    OP_ADD, OP_ALLOCA, OP_AND, OP_ASHR, OP_BRANCH, OP_CALL, OP_CALL_RETURN, OP_CONST, OP_FADD,
    OP_FCMP, OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM, OP_FSUB,
    OP_GEP, OP_ICMP, OP_INTTOPTR, OP_JUMP, OP_LOAD, OP_LSHR, OP_MEASURE, OP_MOV, OP_MUL, OP_OR,
    OP_PEEK_LOSS, OP_PHI, OP_QUANTUM_GATE, OP_READ_LOSS, OP_READ_RESULT, OP_READOUT_NOISE,
    OP_RECORD_OUTPUT, OP_RESET, OP_RET, OP_SDIV, OP_SELECT, OP_SEXT, OP_SHL, OP_SITOFP, OP_SREM,
    OP_STORE, OP_SUB, OP_SWITCH, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_WRITE_RESULT, OP_XOR,
    OP_ZEXT, VOID_RETURN,
};
use crate::{MeasurementResult, OutputRecord};

/// Where a walk stopped.
pub(super) enum Stop {
    Batch,
    Ended,
}

impl QirMachine<'_> {
    /// Runs `context` from its position to its next batch or its end,
    /// reading each bytecode instruction as it runs it: its opcode, and
    /// the immediate flag of each operand it reads. A quantum instruction
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
                OP_CALL => {
                    let function = &bytecode.function_table[instruction.aux0 as usize];
                    context.frames.push(Frame {
                        return_pc: next(pc),
                        caller_block: context.block,
                        keep: (instruction.dst != VOID_RETURN).then_some(dst),
                        memory: context.memory.len(),
                    });
                    let saved = context.saved.len();
                    context.saved.extend_from_slice(&context.registers);
                    // Read from the saved copy, so that each parameter
                    // takes its argument's value at the call, though it
                    // may be an argument of a later one.
                    let arguments = &bytecode.call_args[instruction.aux2 as usize..]
                        [..instruction.aux1 as usize];
                    for (param, argument) in (function.param_base_reg as usize..).zip(arguments) {
                        context.registers[param] = context.saved[saved + *argument as usize];
                    }
                    context.block = function.entry_block_id;
                    pc = block_start(bytecode, function.entry_block_id);
                }
                OP_CALL_RETURN => {
                    let frame = context
                        .frames
                        .pop()
                        .expect("the check refused a return reached with no call");
                    // Read from the function's registers, before the
                    // caller's come back.
                    let kept = frame.keep.map(|keep| (keep, context.src0(instruction)));
                    let saved = context.saved.len() - context.registers.len();
                    context.registers.copy_from_slice(&context.saved[saved..]);
                    context.saved.truncate(saved);
                    context.memory.truncate(frame.memory);
                    if let Some((keep, value)) = kept {
                        context.set(keep, value);
                    }
                    context.block = frame.caller_block;
                    pc = frame.return_pc as usize;
                }
                OP_QUANTUM_GATE | OP_MEASURE | OP_RESET | OP_PEEK_LOSS | OP_READOUT_NOISE => {
                    let source = self
                        .prepared_quantum_calls
                        .source(pc)
                        .expect("every quantum instruction has a prepared source");
                    match source {
                        CallSource::Prebuilt { first, end, result } => {
                            let calls = self.prepared_quantum_calls.prebuilt(pc, *first, *end);
                            let (end, result) = (*end as usize, *result);
                            if let Some(result) = result {
                                if context.pending[result as usize] {
                                    // The last call writes a result a call
                                    // of the batch writes: it starts the
                                    // next batch.
                                    let (before, _) = calls.split_at(calls.len() - 1);
                                    context.batch.extend_from_slice(before);
                                    pc = end - 1;
                                    break Stop::Batch;
                                }
                                context.pending[result as usize] = true;
                                context.batch_results.push(result);
                            }
                            context.batch.extend_from_slice(calls);
                            // To the run's last call: the `pc += 1` below
                            // goes past it.
                            pc = end - 1;
                        }
                        CallSource::BuiltFromRegisters(operands) => {
                            let call = match context.call(bytecode, pc, operands) {
                                Ok(resolved) => resolved,
                                // The stop comes before the trap: the batch
                                // runs first, as `run_shot` measures before
                                // it panics.
                                Err(_)
                                    if context
                                        .written(operands)
                                        .is_some_and(|result| context.is_pending(result)) =>
                                {
                                    break Stop::Batch;
                                }
                                Err(trap) => return Err(trap),
                            };
                            if let Some(result) = call.result() {
                                if context.pending[result as usize] {
                                    break Stop::Batch;
                                }
                                context.pending[result as usize] = true;
                                context.batch_results.push(result);
                            }
                            context.batch.push(call);
                        }
                        CallSource::CorrelatedNoise { table } => {
                            let call = context.correlated_noise_call(
                                bytecode,
                                pc,
                                *table,
                                context.aux1(instruction),
                                context.aux2(instruction),
                            )?;
                            context.batch.push(call);
                        }
                    }
                    pc += 1;
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
                    let result = context.src1(instruction);
                    // Before the value's check, so that the stop comes
                    // before the trap, as for a call.
                    if context.is_pending(result) {
                        break Stop::Batch;
                    }
                    // The value is checked before the result, as `run_shot`
                    // does, but which fault is reported is not promised
                    // (see `QirTrap`), so no test pins it.
                    let written = if bit(pc, context.src0(instruction))? {
                        QirReadout::One
                    } else {
                        QirReadout::Zero
                    };
                    let index = context.result_index(bytecode, pc, result)?;
                    context.results[index] = written;
                    pc += 1;
                }
                OP_RECORD_OUTPUT => {
                    match instruction.aux1 {
                        0 => {
                            let result = context.src0(instruction);
                            if context.is_pending(result) {
                                break Stop::Batch;
                            }
                            let recorded = context.result(bytecode, pc, result)?;
                            context.records.push(OutputRecord::Result(recorded));
                        }
                        3 => {
                            let value = bit(pc, context.src0(instruction))?;
                            context.records.push(OutputRecord::Bool(value));
                        }
                        4 => {
                            let value = signed(context.src0(instruction));
                            context.records.push(OutputRecord::Int(value));
                        }
                        5 => {
                            let value = f64::from_bits(context.src0(instruction));
                            context.records.push(OutputRecord::Double(value));
                        }
                        // Records of an array or a tuple only mark the
                        // output's shape.
                        _ => {}
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
                    let address = context.src0(instruction);
                    let value = usize::try_from(address)
                        .ok()
                        .and_then(|index| context.memory.get(index))
                        .copied()
                        .ok_or(QirTrap::LoadPastMemory {
                            instruction: pc,
                            address,
                            words: context.memory.len(),
                        })?;
                    context.set(dst, value);
                    pc += 1;
                }
                OP_STORE => {
                    // A store past the context's memory traps, writing
                    // nothing, as a load does: growing the memory up to
                    // the address, as `run_shot` does, would hide a
                    // producer's error (a store outside every allocation)
                    // and let a huge address abort the process. The GPU
                    // shader, too, stops a shot at a store
                    // past its fixed memory, but writes below it unchecked,
                    // over whatever is there (other allocations, and the
                    // registers a call saves). See `QirMachine`'s doc.
                    let value = context.src0(instruction);
                    let address = context.src1(instruction);
                    let words = context.memory.len();
                    let word = usize::try_from(address)
                        .ok()
                        .and_then(|index| context.memory.get_mut(index))
                        .ok_or(QirTrap::StorePastMemory {
                            instruction: pc,
                            address,
                            words,
                        })?;
                    *word = value;
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
}

/// The instruction after `pc`.
fn next(pc: usize) -> u32 {
    u32::try_from(pc + 1).expect("a program holds fewer than 2^32 instructions")
}
