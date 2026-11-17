// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conditioned-batch policy of a [`QirMachine`]
//! ([`QirMachine::resume_conditioned`]): a resumer that can run a quantum
//! call only where a condition on outcomes holds lets the machine keep
//! going past a read of a result its batch writes, rather than stop there.
//!
//! What it gives a resumer: fewer, longer batches. A program that measures,
//! reads the result and applies a correction where it reads 1 (a
//! repetition code's decoder, say) stops at every read under
//! [`resume`](QirMachine::resume); under this policy its corrections join
//! the batch as calls conditioned on the batch's own results
//! ([`QirCallCondition`]), and it stops only where a value it cannot carry
//! as a condition is needed.
//!
//! How: a read of a result that a measurement of the batch writes gives its
//! register a *deferred* value, a [`QirCondition`] on the batch's results.
//! The bit instructions combine deferred values into deferred values. A
//! branch on a deferred value whose arms only apply gates and compute bits
//! (a triangle or a diamond of single blocks) runs both arms, their gates
//! conditioned on the branch's condition, and its join's `PHI`s select
//! between the arms' values. Any other use of a deferred value stops the
//! batch before it, as a read of a pending result stops it under `resume`;
//! on the next resume, every deferred value takes its value from the
//! outcomes, and the instruction runs as under `resume`.

use super::super::program::prepared_quantum_calls::CallSource;
use super::super::quantum_instruction_encoding::{CallOperand, Register};
use super::super::{QirCall, QirTrap};
use super::classical_ops::{binary, int_compare, unary};
use super::{QirMachine, QirShotContext, block_start};
use crate::OutputRecord;
use crate::bytecode::Instruction;
use crate::bytecode::encoding::{
    FLAG_SRC0_IMM, ICMP_EQ, ICMP_NE, OP_ADD, OP_ALLOCA, OP_AND, OP_ASHR, OP_BRANCH, OP_CONST,
    OP_FADD, OP_FCMP, OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM,
    OP_FSUB, OP_GEP, OP_ICMP, OP_INTTOPTR, OP_JUMP, OP_LOAD, OP_LSHR, OP_MEASURE, OP_MOV, OP_MUL,
    OP_NOP, OP_OR, OP_PEEK_LOSS, OP_PHI, OP_QUANTUM_GATE, OP_READ_LOSS, OP_READ_RESULT,
    OP_READOUT_NOISE, OP_RECORD_OUTPUT, OP_RESET, OP_RET, OP_SDIV, OP_SELECT, OP_SEXT, OP_SHL,
    OP_SITOFP, OP_SREM, OP_STORE, OP_SUB, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_WRITE_RESULT,
    OP_XOR, OP_ZEXT,
};

/// Where [`QirMachine::resume_conditioned`] stopped a context, lent from
/// the context until it is resumed: as [`QirStop`](super::QirStop), but a
/// batch's calls may carry conditions.
#[derive(Debug, PartialEq)]
pub enum QirConditionedStop<'c> {
    /// The quantum calls to run next, in order, on the program's qubits,
    /// each listed in `conditions` only where its condition holds; the
    /// next resume takes the outcome of each that writes a result
    /// ([`QirCall::result`]), by that result. A conditioned call is a
    /// gate: it writes no result.
    Batch {
        calls: &'c [QirCall],
        conditions: &'c [QirCallCondition],
    },
    /// The program returned: the context's records, as
    /// [`QirStop::Ended`](super::QirStop::Ended) gives them.
    Ended(&'c [OutputRecord]),
}

/// The condition of one call of a batch: `calls[call]` runs only where
/// `condition` holds. A batch lists its conditions in the order of their
/// calls, at most one per call; a call it lists in none runs always.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QirCallCondition {
    /// The index of the call in its batch.
    pub call: usize,
    /// Where it runs.
    pub condition: QirCondition,
}

/// A condition on outcomes of the batch it comes with: a boolean function
/// of 1 to [`MAX_RESULTS`](Self::MAX_RESULTS) results that measurements of
/// that batch write, each read as a `READ_RESULT` reads it (1 for `One`, 0
/// for `Zero` and for `Loss`), given as its truth table. It depends on
/// each of its results, which are in increasing order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QirCondition {
    /// The results it reads, the first `len` of them.
    results: [u32; QirCondition::MAX_RESULTS],
    len: u8,
    /// Bit `i`: its value where result `results[k]` reads bit `k` of `i`.
    table: u32,
}

impl QirCondition {
    /// The most results a condition reads: its truth table fits a `u32`.
    pub const MAX_RESULTS: usize = 5;

    /// The results it reads, in increasing order.
    #[must_use]
    pub fn results(&self) -> &[u32] {
        &self.results[..self.len as usize]
    }

    /// Its truth table: bit `i` is its value where the `k`-th of its
    /// [`results`](Self::results) reads bit `k` of `i`.
    #[must_use]
    pub fn truth_table(&self) -> u32 {
        self.table
    }

    /// Whether it holds where each result `r` reads `one(r)`.
    pub fn holds(&self, mut one: impl FnMut(u32) -> bool) -> bool {
        let index = self
            .results()
            .iter()
            .enumerate()
            .fold(0, |index, (k, result)| {
                index | (u32::from(one(*result)) << k)
            });
        self.table >> index & 1 != 0
    }

    /// The constant `value`: a condition of no result, which only the
    /// machine holds (a batch's conditions read at least one).
    fn constant(value: bool) -> Self {
        Self {
            results: [0; Self::MAX_RESULTS],
            len: 0,
            table: u32::from(value),
        }
    }

    /// Whether `result` reads 1.
    fn result(result: u32) -> Self {
        let mut results = [0; Self::MAX_RESULTS];
        results[0] = result;
        Self {
            results,
            len: 1,
            table: 0b10,
        }
    }

    fn as_constant(&self) -> Option<bool> {
        (self.len == 0).then_some(self.table & 1 != 0)
    }

    /// `function` of `operands`, or `None` when it would read more than
    /// [`MAX_RESULTS`](Self::MAX_RESULTS) results.
    fn combine(operands: &[Self], function: impl Fn(&[bool]) -> bool) -> Option<Self> {
        let mut all = Vec::with_capacity(Self::MAX_RESULTS * operands.len());
        for operand in operands {
            all.extend_from_slice(operand.results());
        }
        all.sort_unstable();
        all.dedup();
        if all.len() > Self::MAX_RESULTS {
            return None;
        }
        let mut values = [false; 3];
        let mut table = 0;
        for index in 0..1u32 << all.len() {
            let one = |result: u32| {
                let k = all.iter().position(|r| *r == result).expect("a union");
                index >> k & 1 != 0
            };
            for (value, operand) in values.iter_mut().zip(operands) {
                *value = operand.holds(one);
            }
            table |= u32::from(function(&values[..operands.len()])) << index;
        }
        let mut condition = Self::constant(false);
        condition.results[..all.len()].copy_from_slice(&all);
        condition.len = u8::try_from(all.len()).expect("at most 5 results");
        condition.table = table;
        condition.drop_unread();
        Some(condition)
    }

    /// Drops each result its value does not depend on.
    fn drop_unread(&mut self) {
        let mut k = 0;
        while k < self.len as usize {
            let size = 1u32 << self.len;
            let depends =
                (0..size).any(|i| (self.table >> i & 1) != (self.table >> (i ^ 1 << k) & 1));
            if depends {
                k += 1;
                continue;
            }
            let mut table = 0;
            for (j, i) in (0..size).filter(|i| i >> k & 1 == 0).enumerate() {
                table |= (self.table >> i & 1) << j;
            }
            self.table = table;
            self.results.copy_within(k + 1..self.len as usize, k);
            self.len -= 1;
        }
    }

    fn not(self) -> Self {
        Self::combine(&[self], |v| !v[0]).expect("as many results")
    }
}

/// What the conditioned step did with an instruction.
pub(super) enum ConditionedStep {
    /// Ran it (and maybe a branch's arms and join): the walk goes on from
    /// here.
    Ran(usize),
    /// It reads no deferred value: the walk runs it as `resume` does.
    Concrete,
    /// It needs a deferred value: the batch ends before it.
    Stop,
}

impl QirShotContext {
    /// The deferred value of the operand field `word`, if it is a register
    /// holding one.
    fn deferred_operand(
        &self,
        instruction: &Instruction<u64>,
        flag: u64,
        word: u64,
    ) -> Option<QirCondition> {
        if instruction.opcode & flag != 0 {
            None
        } else {
            // A field that is no register reads no deferred value.
            usize::try_from(word)
                .ok()
                .and_then(|index| self.deferred.get(index).copied().flatten())
        }
    }

    /// The operand field `word` as a condition: its deferred value, or its
    /// value when that is a bit; `None` for any other value.
    fn as_condition(
        &self,
        instruction: &Instruction<u64>,
        flag: u64,
        word: u64,
    ) -> Option<QirCondition> {
        self.deferred_operand(instruction, flag, word).or_else(|| {
            match self.operand(instruction, flag, word) {
                0 => Some(QirCondition::constant(false)),
                1 => Some(QirCondition::constant(true)),
                _ => None,
            }
        })
    }

    /// Whether the context holds a deferred value.
    pub(super) fn defers(&self) -> bool {
        !self.deferred_registers.is_empty()
    }

    /// Writes `value` to `register`: a concrete bit for a constant, else
    /// a deferred value; logged for [`undo`](Self::undo) when `logged`.
    fn assign(&mut self, register: Register, value: QirCondition, logged: bool) {
        let index = register as usize;
        if logged {
            self.undo_log
                .push((register, self.registers[index], self.deferred[index]));
        }
        if let Some(bit) = value.as_constant() {
            self.registers[index] = u64::from(bit);
            self.deferred[index] = None;
        } else {
            self.deferred[index] = Some(value);
            self.deferred_registers.push(register);
        }
    }

    /// Writes the concrete `value` to `register`, logged.
    fn assign_word(&mut self, register: Register, value: u64) {
        let index = register as usize;
        self.undo_log
            .push((register, self.registers[index], self.deferred[index]));
        self.registers[index] = value;
        self.deferred[index] = None;
    }

    /// Forgets `register`'s deferred value: an instruction writes it.
    pub(super) fn forget(&mut self, register: Register) {
        self.deferred[register as usize] = None;
    }

    /// Appends `call`, run only where `guard` holds.
    fn push_conditioned(&mut self, call: QirCall, guard: QirCondition) {
        match guard.as_constant() {
            Some(false) => {}
            Some(true) => self.batch.push(call),
            None => {
                self.batch.push(call);
                self.conditions.push(QirCallCondition {
                    call: self.batch.len() - 1,
                    condition: guard,
                });
            }
        }
    }

    /// Undoes the logged writes and the calls appended since `mark`.
    fn undo(&mut self, mark: Mark) {
        while let Some((register, value, deferred)) = self.undo_log.pop() {
            self.registers[register as usize] = value;
            self.deferred[register as usize] = deferred;
        }
        self.batch.truncate(mark.calls);
        self.conditions.truncate(mark.conditions);
        self.deferred_registers.truncate(mark.deferred);
    }

    fn mark(&mut self) -> Mark {
        self.undo_log.clear();
        Mark {
            calls: self.batch.len(),
            conditions: self.conditions.len(),
            deferred: self.deferred_registers.len(),
        }
    }

    /// Gives each deferred value its value from the results, once the
    /// batch's outcomes are in; a deferred read of a result with no value
    /// traps at that read, the first in the program's order.
    pub(super) fn resolve_deferred(&mut self) -> Result<(), QirTrap> {
        use super::super::QirReadout;
        if let Some(&(result, pc)) = self
            .deferred_reads
            .iter()
            .find(|(result, _)| self.results[*result as usize] == QirReadout::Missing)
        {
            return Err(QirTrap::MissingValue {
                instruction: pc,
                result,
            });
        }
        for register in self.deferred_registers.drain(..) {
            if let Some(condition) = self.deferred[register as usize].take() {
                let value =
                    condition.holds(|result| self.results[result as usize] == QirReadout::One);
                self.registers[register as usize] = u64::from(value);
            }
        }
        self.deferred_reads.clear();
        self.conditions.clear();
        Ok(())
    }
}

/// Where an if-conversion started, to undo it.
#[derive(Clone, Copy)]
struct Mark {
    calls: usize,
    conditions: usize,
    deferred: usize,
}

impl QirMachine<'_> {
    /// Runs the instruction at `pc` under the conditioned policy, when the
    /// context holds a deferred value or the instruction is a read.
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    pub(super) fn conditioned_step(
        self,
        context: &mut QirShotContext,
        pc: usize,
    ) -> ConditionedStep {
        use crate::bytecode::encoding::{FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_SRC1_IMM};
        let bytecode = self.bytecode;
        let instruction = &bytecode.instructions[pc];
        let dst = instruction.dst as Register;
        let opcode = instruction.primary_opcode();
        let src0 = context.deferred_operand(instruction, FLAG_SRC0_IMM, instruction.src0);
        let src1 = context.deferred_operand(instruction, FLAG_SRC1_IMM, instruction.src1);
        let aux0 = context.deferred_operand(instruction, FLAG_AUX0_IMM, instruction.aux0);
        let aux1 = context.deferred_operand(instruction, FLAG_AUX1_IMM, instruction.aux1);
        let concrete = |context: &mut QirShotContext, writes: bool| {
            if writes {
                context.forget(dst);
            }
            ConditionedStep::Concrete
        };
        match opcode {
            OP_READ_RESULT => {
                if src0.is_some() {
                    return ConditionedStep::Stop;
                }
                let result = context.src0(instruction);
                if !context.is_pending(result) {
                    return concrete(context, true);
                }
                let result = result as u32;
                if !context.deferrable[result as usize] {
                    return ConditionedStep::Stop;
                }
                context.assign(dst, QirCondition::result(result), false);
                context.deferred_reads.push((result, pc));
                ConditionedStep::Ran(pc + 1)
            }
            OP_AND | OP_OR | OP_XOR | OP_ICMP | OP_MOV | OP_ZEXT | OP_TRUNC | OP_SELECT => {
                let reads = match opcode {
                    OP_MOV | OP_ZEXT | OP_TRUNC => src0.is_some(),
                    OP_SELECT => src0.is_some() || aux0.is_some() || aux1.is_some(),
                    _ => src0.is_some() || src1.is_some(),
                };
                if !reads {
                    return concrete(context, true);
                }
                match bit_function(context, instruction) {
                    Some(value) => {
                        context.assign(dst, value, false);
                        ConditionedStep::Ran(pc + 1)
                    }
                    None => ConditionedStep::Stop,
                }
            }
            OP_PHI => {
                let entry = bytecode.phi_entries[instruction.aux0 as usize..]
                    [..instruction.aux1 as usize]
                    .iter()
                    .find(|entry| entry.block_id == context.previous_block)
                    .expect("the check refused a `PHI` with no entry for a predecessor");
                match context.deferred[entry.val_reg as usize] {
                    Some(value) => {
                        context.assign(dst, value, false);
                        ConditionedStep::Ran(pc + 1)
                    }
                    None => concrete(context, true),
                }
            }
            OP_BRANCH => match src0 {
                None => concrete(context, false),
                Some(condition) => match self.if_convert(context, instruction, condition) {
                    Some(next) => ConditionedStep::Ran(next),
                    None => ConditionedStep::Stop,
                },
            },
            OP_QUANTUM_GATE | OP_MEASURE | OP_RESET | OP_PEEK_LOSS | OP_READOUT_NOISE => {
                match self.prepared_quantum_calls.source(pc) {
                    Some(CallSource::Prebuilt { .. }) => concrete(context, false),
                    Some(CallSource::BuiltFromRegisters(operands)) => {
                        let ops = &operands.operands;
                        let deferred = ops
                            .qubits
                            .iter()
                            .map(register_of)
                            .chain(ops.reals.iter().map(register_of))
                            .chain(ops.result.as_ref().map(register_of))
                            .flatten()
                            .any(|register| context.deferred[register as usize].is_some());
                        if deferred {
                            ConditionedStep::Stop
                        } else {
                            concrete(context, false)
                        }
                    }
                    // Its qubits' registers are not checked: stop.
                    _ => ConditionedStep::Stop,
                }
            }
            OP_JUMP | OP_RET | OP_NOP => concrete(context, false),
            OP_CONST => concrete(context, true),
            OP_ADD | OP_SUB | OP_MUL | OP_UDIV | OP_UREM | OP_SDIV | OP_SREM | OP_SHL | OP_LSHR
            | OP_ASHR | OP_FADD | OP_FSUB | OP_FMUL | OP_FDIV | OP_FREM | OP_FCMP | OP_STORE
            | OP_WRITE_RESULT => {
                if src0.is_some() || src1.is_some() {
                    ConditionedStep::Stop
                } else {
                    concrete(context, opcode != OP_STORE && opcode != OP_WRITE_RESULT)
                }
            }
            OP_INTTOPTR | OP_FPEXT | OP_FPTRUNC | OP_FPTOSI | OP_FPTOUI | OP_SITOFP | OP_UITOFP
            | OP_SEXT | OP_ALLOCA | OP_LOAD | OP_READ_LOSS | OP_RECORD_OUTPUT => {
                if src0.is_some() {
                    ConditionedStep::Stop
                } else {
                    concrete(context, opcode != OP_RECORD_OUTPUT)
                }
            }
            OP_GEP => {
                if src0.is_some() || src1.is_some() || aux0.is_some() {
                    ConditionedStep::Stop
                } else {
                    concrete(context, true)
                }
            }
            // `CALL`, `CALL_RETURN` and `SWITCH`: the registers a call
            // saves and restores carry no deferred value, and a switch's
            // cases are not bits.
            _ => ConditionedStep::Stop,
        }
    }

    /// Runs the branch `instruction`, on the deferred `condition`, as
    /// both its arms, their gates conditioned, and its join's `PHI`s as
    /// selects; gives the join's first other instruction. `None`, having
    /// undone everything, where the branch is no triangle or diamond of
    /// single blocks, an arm does more than compute bits and apply gates,
    /// or a value would read too many results.
    fn if_convert(
        self,
        context: &mut QirShotContext,
        instruction: &Instruction<u64>,
        condition: QirCondition,
    ) -> Option<usize> {
        let bytecode = self.bytecode;
        let (yes, no) = (instruction.aux0, instruction.aux1);
        if yes == no {
            return None;
        }
        let here = context.block;
        let jumps_to = |block: u64| -> Option<u64> {
            let entry = &bytecode.block_table[usize::try_from(block).ok()?];
            let count = usize::try_from(entry.instr_count).ok()?;
            let last = usize::try_from(entry.instr_offset).ok()? + count.checked_sub(1)?;
            let jump = &bytecode.instructions[last];
            (jump.primary_opcode() == OP_JUMP).then_some(jump.dst)
        };
        let not = condition.not();
        // The arms to run, each under its guard, and the blocks the join's
        // `PHI`s see on the edges where the condition holds and where not.
        let (arms, count, join, from_yes, from_no) = if jumps_to(yes) == Some(no) {
            ([(yes, condition), (no, not)], 1, no, yes, here)
        } else if jumps_to(no) == Some(yes) {
            ([(no, not), (yes, condition)], 1, yes, here, no)
        } else {
            match (jumps_to(yes), jumps_to(no)) {
                (Some(a), Some(b)) if a == b => ([(yes, condition), (no, not)], 2, a, yes, no),
                _ => return None,
            }
        };
        let mark = context.mark();
        let mut converted = || -> Option<usize> {
            for &(arm, guard) in &arms[..count] {
                self.run_arm(context, arm, guard)?;
            }
            let mut pc = block_start(bytecode, join);
            loop {
                let instruction = &bytecode.instructions[pc];
                match instruction.primary_opcode() {
                    OP_MOV if instruction.opcode & FLAG_SRC0_IMM != 0 => {
                        #[allow(clippy::cast_possible_truncation)]
                        context.assign_word(instruction.dst as Register, instruction.src0);
                    }
                    OP_PHI => {
                        let entries = &bytecode.phi_entries
                            [usize::try_from(instruction.aux0).ok()?..]
                            [..usize::try_from(instruction.aux1).ok()?];
                        let value = |block: u64| {
                            let entry = entries.iter().find(|entry| entry.block_id == block)?;
                            let register = entry.val_reg;
                            context.deferred[usize::try_from(register).ok()?].or_else(|| {
                                match context.registers[usize::try_from(register).ok()?] {
                                    0 => Some(QirCondition::constant(false)),
                                    1 => Some(QirCondition::constant(true)),
                                    _ => None,
                                }
                            })
                        };
                        let (when, otherwise) = (value(from_yes)?, value(from_no)?);
                        let selected = QirCondition::combine(&[condition, when, otherwise], |v| {
                            if v[0] { v[1] } else { v[2] }
                        })?;
                        #[allow(clippy::cast_possible_truncation)]
                        context.assign(instruction.dst as Register, selected, true);
                    }
                    _ => break,
                }
                pc += 1;
            }
            Some(pc)
        };
        let converted = converted();
        if converted.is_some() {
            context.previous_block = from_yes;
            context.block = join;
        } else {
            context.undo(mark);
        }
        converted
    }

    /// Runs the arm `block` but its final jump, its gates conditioned on
    /// `guard`; `None` at an instruction an arm may not hold.
    #[allow(clippy::cast_possible_truncation)]
    fn run_arm(self, context: &mut QirShotContext, block: u64, guard: QirCondition) -> Option<()> {
        let bytecode = self.bytecode;
        let entry = &bytecode.block_table[usize::try_from(block).ok()?];
        let start = usize::try_from(entry.instr_offset).ok()?;
        let end = start + usize::try_from(entry.instr_count).ok()? - 1;
        let mut pc = start;
        while pc < end {
            let instruction = &bytecode.instructions[pc];
            let dst = instruction.dst as Register;
            match instruction.primary_opcode() {
                OP_QUANTUM_GATE => match self.prepared_quantum_calls.source(pc)? {
                    CallSource::Prebuilt {
                        first,
                        end: run_end,
                        result: None,
                    } => {
                        let calls = self.prepared_quantum_calls.prebuilt(pc, *first, *run_end);
                        if calls
                            .iter()
                            .any(|call| !matches!(call, QirCall::Gate { .. }))
                        {
                            return None;
                        }
                        for call in calls {
                            context.push_conditioned(call.clone(), guard);
                        }
                        pc = *run_end as usize;
                        continue;
                    }
                    CallSource::BuiltFromRegisters(operands) => {
                        let ops = &operands.operands;
                        let deferred = ops
                            .qubits
                            .iter()
                            .map(register_of)
                            .chain(ops.reals.iter().map(register_of))
                            .flatten()
                            .any(|register| context.deferred[register as usize].is_some());
                        if deferred || ops.result.is_some() {
                            return None;
                        }
                        let call = context.call(bytecode, pc, operands).ok()?;
                        if !matches!(call, QirCall::Gate { .. }) {
                            return None;
                        }
                        context.push_conditioned(call, guard);
                    }
                    _ => return None,
                },
                OP_AND | OP_OR | OP_XOR | OP_ICMP | OP_MOV | OP_ZEXT | OP_TRUNC | OP_SELECT => {
                    if let Some(value) = bit_function(context, instruction) {
                        context.assign(dst, value, true);
                    } else {
                        // Not bits: run it as is, when it cannot trap.
                        let opcode = instruction.primary_opcode();
                        if opcode == OP_SELECT {
                            return None;
                        }
                        let any_deferred = [
                            context.deferred_operand(instruction, FLAG_SRC0_IMM, instruction.src0),
                            context.deferred_operand(
                                instruction,
                                crate::bytecode::encoding::FLAG_SRC1_IMM,
                                instruction.src1,
                            ),
                        ]
                        .iter()
                        .any(Option::is_some);
                        if any_deferred {
                            return None;
                        }
                        let (a, b) = (context.src0(instruction), context.src1(instruction));
                        let value = match opcode {
                            OP_ICMP => u64::from(int_compare(instruction.sub_opcode(), a, b)),
                            OP_MOV | OP_ZEXT | OP_TRUNC => unary(opcode, a),
                            _ => binary(opcode, a, b)?,
                        };
                        context.assign_word(dst, value);
                    }
                }
                OP_CONST => context.assign_word(dst, instruction.src0),
                OP_NOP => {}
                _ => return None,
            }
            pc += 1;
        }
        Some(())
    }
}

/// Whether `call` is a measurement, whose result a read may defer.
pub(super) fn is_measurement(call: &QirCall) -> bool {
    matches!(call, QirCall::Measure { .. } | QirCall::MeasureReset { .. })
}

/// The register an operand is held in, if any.
fn register_of<T>(operand: &CallOperand<T>) -> Option<Register> {
    match operand {
        CallOperand::Known(_) => None,
        CallOperand::InRegister(register) => Some(*register),
    }
}

/// The bit instruction `instruction` on its operands as conditions;
/// `None` where an operand is no bit, the instruction computes no bit
/// function (an `ICMP` other than `eq` and `ne`), or the value would read
/// too many results.
fn bit_function(context: &QirShotContext, instruction: &Instruction<u64>) -> Option<QirCondition> {
    use crate::bytecode::encoding::{FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_SRC1_IMM};
    let a = || context.as_condition(instruction, FLAG_SRC0_IMM, instruction.src0);
    let b = || context.as_condition(instruction, FLAG_SRC1_IMM, instruction.src1);
    match instruction.primary_opcode() {
        OP_AND => QirCondition::combine(&[a()?, b()?], |v| v[0] && v[1]),
        OP_OR => QirCondition::combine(&[a()?, b()?], |v| v[0] || v[1]),
        OP_XOR => QirCondition::combine(&[a()?, b()?], |v| v[0] != v[1]),
        OP_ICMP => match instruction.sub_opcode() {
            ICMP_EQ => QirCondition::combine(&[a()?, b()?], |v| v[0] == v[1]),
            ICMP_NE => QirCondition::combine(&[a()?, b()?], |v| v[0] != v[1]),
            _ => None,
        },
        OP_MOV | OP_ZEXT | OP_TRUNC => a(),
        OP_SELECT => {
            let x = context.as_condition(instruction, FLAG_AUX0_IMM, instruction.aux0)?;
            let y = context.as_condition(instruction, FLAG_AUX1_IMM, instruction.aux1)?;
            QirCondition::combine(&[a()?, x, y], |v| if v[0] { v[1] } else { v[2] })
        }
        _ => None,
    }
}
