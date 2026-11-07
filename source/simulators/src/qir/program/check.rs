// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a bytecode program, run once when a [`QirProgram`] is
//! built.
//!
//! The bytecode comes from a producer the machine does not control (the
//! Python pass, or a test writing it by hand). A fault met during a shot
//! would stop that shot only, after work was spent on it, and show one
//! fault at a time. So the check refuses, before any shot runs, the faults
//! the bytecode shows, and reports every one ([`QirFaults`]), so that
//! whoever emits the bytecode can fix them all at once. The machine then
//! relies on it: it reads the indices the check accepted without checking
//! them again. Checking a quantum instruction also decodes it, as a call
//! of a [`QirInstruction`] ([`CheckedCall`]), from which the program
//! prepares its calls.
//!
//! The check follows what a run reads. Every instruction is checked, the
//! untaken branches' too, and each operand as a run reads it, as a
//! register or as a constant. A value known only during a run (a qubit or
//! a divisor held in a register, the call arguments of correlated noise
//! whose count or offset is held in one) is left to the run, which traps
//! on a wrong one. The rules, one per variant, are [`QirError`]'s.

use super::super::QirInstruction;
use super::super::fault::{QirError, QirFault, QirFaultSite, QirFaults, SideTable};
use super::super::quantum_instruction_encoding::{
    self, CallOperand, CallOperands, Encoding, QIR_INSTRUCTIONS, Shape,
};
#[cfg(doc)]
use super::QirProgram;
use crate::bytecode::encoding::{
    FCMP_TRUE, FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM,
    FLAG_SRC1_IMM, ICMP_UGE, OP_ADD, OP_ALLOCA, OP_AND, OP_ASHR, OP_BRANCH, OP_CALL,
    OP_CALL_RETURN, OP_CONST, OP_FADD, OP_FCMP, OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI,
    OP_FPTRUNC, OP_FREM, OP_FSUB, OP_GEP, OP_ICMP, OP_INTTOPTR, OP_JUMP, OP_LOAD, OP_LSHR,
    OP_MEASURE, OP_MOV, OP_MUL, OP_NOP, OP_OR, OP_PEEK_LOSS, OP_PHI, OP_QUANTUM_GATE, OP_READ_LOSS,
    OP_READ_RESULT, OP_READOUT_NOISE, OP_RECORD_OUTPUT, OP_RESET, OP_RET, OP_SDIV, OP_SELECT,
    OP_SEXT, OP_SHL, OP_SITOFP, OP_SREM, OP_STORE, OP_SUB, OP_SWITCH, OP_TRUNC, OP_UDIV, OP_UITOFP,
    OP_UREM, OP_WRITE_RESULT, OP_XOR, OP_ZEXT, OPID_CORRELATED_NOISE, VOID_RETURN,
};
use crate::bytecode::{AdaptiveProgram, Function, Instruction};
use std::ops::Range;

/// A quantum instruction of the bytecode, as a call of a [`QirInstruction`],
/// with its operands as building the program gives them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CheckedCall {
    /// A call of `instruction`, any but correlated noise.
    Call {
        instruction: QirInstruction,
        operands: CallOperands,
    },
    /// Correlated noise, drawn from the noise table `table`, on the
    /// qubits held in the registers that the `count` call arguments from
    /// `offset` name. When both are constants, the call arguments are
    /// checked to exist and to name registers; else a run checks them.
    CorrelatedNoise {
        table: u32,
        count: Source,
        offset: Source,
    },
}

impl CheckedCall {
    /// The instruction it calls.
    pub(super) fn instruction(&self) -> QirInstruction {
        match self {
            Self::Call { instruction, .. } => *instruction,
            Self::CorrelatedNoise { .. } => QirInstruction::CorrelatedNoise,
        }
    }
}

/// Checks `bytecode`, and decodes its quantum instructions: one entry per
/// instruction, `Some` for a quantum one. Every fault is reported: those
/// of the program's entry block, then of each block and each function, in
/// their order, then those of the instructions, in theirs.
pub(super) fn check(
    bytecode: &AdaptiveProgram<u64>,
) -> Result<Vec<Option<CheckedCall>>, QirFaults> {
    let mut check = Check::new(bytecode);
    let calls = (0..bytecode.instructions.len())
        .map(|index| check.instruction(index))
        .collect();
    if check.faults.is_empty() {
        Ok(calls)
    } else {
        Err(QirFaults(check.faults))
    }
}

/// Whether a call of `bytecode` keeps its function's value, so that a run
/// reads the value a `CALL_RETURN` names: the check checks that value, and
/// a [`QirProgram`] decodes it, only then.
fn keeps_return(bytecode: &AdaptiveProgram<u64>) -> bool {
    bytecode.instructions.iter().any(|instruction| {
        instruction.primary_opcode() == OP_CALL && instruction.dst != VOID_RETURN
    })
}

/// The first and last of the registers a call with `arguments` arguments
/// copies them into, from `function`'s first parameter register on, when
/// it has any: a call with none names no register, whatever the base. The
/// check checks the last, and a [`QirProgram`] decodes the first, only
/// then.
fn parameter_registers(function: &Function<u64>, arguments: u64) -> Option<(u64, u64)> {
    let first = function.param_base_reg;
    (arguments > 0).then(|| (first, first.saturating_add(arguments - 1)))
}

/// The check of one program, as it goes.
struct Check<'a> {
    bytecode: &'a AdaptiveProgram<u64>,
    faults: Vec<QirFault>,
    /// The instructions of each block; `None` when its range is refused.
    blocks: Vec<Option<Range<usize>>>,
    /// The block each instruction belongs to (the first, when blocks
    /// overlap).
    owners: Vec<Option<usize>>,
    /// The blocks a run reaches from the entry block through jumps,
    /// branches and switches alone, so with no call to return from.
    outside_functions: Vec<bool>,
    /// The blocks a run reaches from the entry block of a function that a
    /// `CALL` names, through jumps, branches and switches alone, so inside
    /// a call (some may be outside functions too). A function no `CALL`
    /// names never runs: the Python bindings give a program with no
    /// function a function at block 0, as they fill each empty table with
    /// a default entry.
    inside_functions: Vec<bool>,
    /// The predecessors of each block: the blocks whose terminator (a
    /// jump, a branch, or a switch's default or case) goes to it, each
    /// once, in ascending order; every such edge counts, also from a block
    /// no run reaches.
    predecessors: Vec<Vec<usize>>,
    /// The blocks a run enters by no edge: the program's entry block, and
    /// the entry blocks of the functions a `CALL` names (an edge into one
    /// is refused).
    entry_blocks: Vec<bool>,
    /// Whether a call keeps its function's value, so that a run reads
    /// the value a `CALL_RETURN` names.
    keeps_return: bool,
}

/// An operand field of an instruction.
#[derive(Clone, Copy)]
enum Field {
    Dst,
    Src0,
    Src1,
    Aux0,
    Aux1,
    Aux2,
}

impl Field {
    /// The field's word in `instruction`.
    fn word(self, instruction: &Instruction<u64>) -> u64 {
        match self {
            Field::Dst => instruction.dst,
            Field::Src0 => instruction.src0,
            Field::Src1 => instruction.src1,
            Field::Aux0 => instruction.aux0,
            Field::Aux1 => instruction.aux1,
            Field::Aux2 => instruction.aux2,
        }
    }

    /// The flag that makes the field a constant rather than a register.
    fn constant_flag(self) -> u64 {
        match self {
            Field::Dst => FLAG_DST_IMM,
            Field::Src0 => FLAG_SRC0_IMM,
            Field::Src1 => FLAG_SRC1_IMM,
            Field::Aux0 => FLAG_AUX0_IMM,
            Field::Aux1 => FLAG_AUX1_IMM,
            Field::Aux2 => FLAG_AUX2_IMM,
        }
    }
}

/// What an operand field gives a run: a constant, or a register's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Source {
    Constant(u64),
    Register(u32),
}

/// Whether an instruction with `opcode` ends its block.
fn is_terminator(opcode: u8) -> bool {
    matches!(
        opcode,
        OP_RET | OP_JUMP | OP_BRANCH | OP_SWITCH | OP_CALL_RETURN
    )
}

/// Whether `instruction` may come before a `PHI` in its block: another
/// `PHI`, or a `MOV` of an immediate, which the adaptive pass puts there
/// to load a `PHI`'s constant value into a register.
fn may_precede_phi(instruction: &Instruction<u64>) -> bool {
    match instruction.primary_opcode() {
        OP_PHI => true,
        OP_MOV => instruction.has_flag(FLAG_SRC0_IMM),
        _ => false,
    }
}

impl<'a> Check<'a> {
    /// The check of `bytecode`, with its entry block, blocks and functions
    /// checked.
    fn new(bytecode: &'a AdaptiveProgram<u64>) -> Self {
        let mut check = Check {
            bytecode,
            faults: Vec::new(),
            blocks: Vec::new(),
            owners: vec![None; bytecode.instructions.len()],
            outside_functions: Vec::new(),
            inside_functions: Vec::new(),
            predecessors: Vec::new(),
            entry_blocks: Vec::new(),
            keeps_return: keeps_return(bytecode),
        };
        check.block(QirFaultSite::Program, bytecode.entry_block);
        for index in 0..bytecode.block_table.len() {
            let range = check.block_range(index);
            check.blocks.push(range);
        }
        for (index, function) in bytecode.function_table.iter().enumerate() {
            check.block(QirFaultSite::Function(index), function.entry_block_id);
        }
        check.outside_functions = check.reached([bytecode.entry_block]);
        let called: Vec<u64> = bytecode
            .instructions
            .iter()
            .filter(|instruction| instruction.primary_opcode() == OP_CALL)
            .filter_map(|instruction| {
                let function = usize::try_from(instruction.aux0).ok()?;
                Some(bytecode.function_table.get(function)?.entry_block_id)
            })
            .collect();
        check.inside_functions = check.reached(called.iter().copied());
        check.entry_blocks = check.marked([bytecode.entry_block].into_iter().chain(called));
        check.predecessors = vec![Vec::new(); check.blocks.len()];
        for block in 0..check.blocks.len() {
            for target in check.successors(block) {
                if let Some(target) = check.index(target) {
                    // Blocks go in ascending order, so a block already
                    // pushed for a target is its last.
                    if check.predecessors[target].last() != Some(&block) {
                        check.predecessors[target].push(block);
                    }
                }
            }
        }
        check
    }

    /// The index of `block` in the block table, when it is in range.
    fn index(&self, block: u64) -> Option<usize> {
        usize::try_from(block)
            .ok()
            .filter(|block| *block < self.blocks.len())
    }

    /// The blocks `blocks` marked, by index in the block table; blocks out
    /// of range are skipped: they are refused anyway.
    fn marked(&self, blocks: impl IntoIterator<Item = u64>) -> Vec<bool> {
        let mut marked = vec![false; self.blocks.len()];
        for block in blocks {
            if let Some(block) = self.index(block) {
                marked[block] = true;
            }
        }
        marked
    }

    /// The blocks the terminator of the block at `index` goes to: a jump's
    /// target, a branch's two, a switch's default and cases (when its
    /// cases' range is in its table), in that order. None for a return, or
    /// when the block's range is refused or it ends with no terminator.
    fn successors(&self, index: usize) -> Vec<u64> {
        let Some(range) = &self.blocks[index] else {
            return Vec::new();
        };
        let last = &self.bytecode.instructions[range.end - 1];
        match last.primary_opcode() {
            OP_JUMP => vec![last.dst],
            OP_BRANCH => vec![last.aux0, last.aux1],
            OP_SWITCH => {
                let mut targets = vec![last.aux0];
                if let Some(cases) =
                    self.side_table_range(SideTable::SwitchCases, last.aux1, last.aux2)
                {
                    targets.extend(
                        self.bytecode.switch_cases[cases]
                            .iter()
                            .map(|case| case.target_block),
                    );
                }
                targets
            }
            _ => Vec::new(),
        }
    }

    fn fault(&mut self, at: QirFaultSite, error: QirError) {
        self.faults.push(QirFault { at, error });
    }

    /// Checks the block at `index` of the block table: in range, not empty,
    /// sharing no instruction with an earlier block, ending with a
    /// terminator. Gives its instructions, when in range.
    fn block_range(&mut self, index: usize) -> Option<Range<usize>> {
        let at = QirFaultSite::Block(index);
        let block = self.bytecode.block_table[index];
        let instructions = self.bytecode.instructions.len();
        if block.instr_count == 0 {
            self.fault(at, QirError::EmptyBlock);
            return None;
        }
        let range = usize::try_from(block.instr_offset)
            .ok()
            .zip(usize::try_from(block.instr_count).ok())
            .and_then(|(offset, count)| Some(offset..offset.checked_add(count)?))
            .filter(|range| range.end <= instructions);
        let Some(range) = range else {
            self.fault(
                at,
                QirError::BlockPastInstructions {
                    offset: block.instr_offset,
                    count: block.instr_count,
                    instructions,
                },
            );
            return None;
        };
        let mut earlier = None;
        for owner in &mut self.owners[range.clone()] {
            match owner {
                Some(block) => earlier = earlier.or(Some(*block)),
                None => *owner = Some(index),
            }
        }
        if let Some(earlier) = earlier {
            self.fault(at, QirError::BlocksOverlap { earlier });
        }
        let last = &self.bytecode.instructions[range.end - 1];
        if !is_terminator(last.primary_opcode()) {
            self.fault(at, QirError::NoTerminator);
        }
        Some(range)
    }

    /// The blocks a run reaches from the blocks `from` with no call or
    /// return: those blocks, and the targets of their jumps, branches and
    /// switches, by index in the block table. Blocks out of range are
    /// skipped: they are refused anyway.
    fn reached(&self, from: impl IntoIterator<Item = u64>) -> Vec<bool> {
        let mut marked = vec![false; self.blocks.len()];
        let mut pending: Vec<u64> = from.into_iter().collect();
        while let Some(block) = pending.pop() {
            let Some(block) = self.index(block) else {
                continue;
            };
            if marked[block] {
                continue;
            }
            marked[block] = true;
            pending.extend(self.successors(block));
        }
        marked
    }

    /// The range of `table` from `offset` holding `count` entries, when the
    /// table holds them.
    fn side_table_range(&self, table: SideTable, offset: u64, count: u64) -> Option<Range<usize>> {
        let len = self.side_table_len(table);
        let offset = usize::try_from(offset).ok()?;
        let end = offset.checked_add(usize::try_from(count).ok()?)?;
        (end <= len).then_some(offset..end)
    }

    fn side_table_len(&self, table: SideTable) -> usize {
        match table {
            SideTable::PhiEntries => self.bytecode.phi_entries.len(),
            SideTable::SwitchCases => self.bytecode.switch_cases.len(),
            SideTable::CallArgs => self.bytecode.call_args.len(),
        }
    }

    /// Checks that `table` holds `count` entries from `offset`, and gives
    /// their range.
    fn side_table(
        &mut self,
        at: QirFaultSite,
        table: SideTable,
        offset: u64,
        count: u64,
    ) -> Option<Range<usize>> {
        let range = self.side_table_range(table, offset, count);
        if range.is_none() {
            let len = self.side_table_len(table);
            self.fault(
                at,
                QirError::PastSideTable {
                    table,
                    offset,
                    count,
                    len,
                },
            );
        }
        range
    }

    /// Checks that the call arguments hold `count` entries from `offset`,
    /// each naming a register.
    fn call_arguments(&mut self, at: QirFaultSite, offset: u64, count: u64) {
        if let Some(arguments) = self.side_table(at, SideTable::CallArgs, offset, count) {
            for argument in arguments {
                self.register(at, self.bytecode.call_args[argument]);
            }
        }
    }

    /// Checks that `block` is in the block table.
    fn block(&mut self, at: QirFaultSite, block: u64) {
        let blocks = self.bytecode.block_table.len();
        if !usize::try_from(block).is_ok_and(|block| block < blocks) {
            self.fault(at, QirError::NoSuchBlock { block, blocks });
        }
    }

    /// Checks that `register` is one of the program's, and gives it.
    fn register(&mut self, at: QirFaultSite, register: u64) -> Option<u32> {
        let registers = self.bytecode.num_registers;
        let checked = u32::try_from(register)
            .ok()
            .filter(|register| *register < registers);
        if checked.is_none() {
            self.fault(
                at,
                QirError::NoSuchRegister {
                    register,
                    registers,
                },
            );
        }
        checked
    }

    /// What `field` of `instruction` gives a run, its register checked.
    fn source(
        &mut self,
        at: QirFaultSite,
        instruction: &Instruction<u64>,
        field: Field,
    ) -> Option<Source> {
        let word = field.word(instruction);
        if instruction.has_flag(field.constant_flag()) {
            Some(Source::Constant(word))
        } else {
            self.register(at, word).map(Source::Register)
        }
    }

    /// Checks the fields a run reads from `instruction`.
    fn reads(&mut self, at: QirFaultSite, instruction: &Instruction<u64>, fields: &[Field]) {
        for field in fields {
            self.source(at, instruction, *field);
        }
    }

    /// Checks the bit that `field` of `instruction` gives a run (a
    /// condition, a bool recorded or a result written): a constant other
    /// than 0 and 1 is refused, as a run would trap on it.
    fn bit(&mut self, at: QirFaultSite, instruction: &Instruction<u64>, field: Field) {
        if let Some(Source::Constant(value)) = self.source(at, instruction, field)
            && value > 1
        {
            self.fault(at, QirError::NotABit { value });
        }
    }

    /// The qubit a call acts on, from `source`.
    fn qubit(&mut self, at: QirFaultSite, source: Option<Source>) -> Option<CallOperand<u32>> {
        match source? {
            Source::Register(register) => Some(CallOperand::InRegister(register)),
            Source::Constant(qubit) => {
                let qubits = self.bytecode.num_qubits;
                let checked = u32::try_from(qubit).ok().filter(|qubit| *qubit < qubits);
                if checked.is_none() {
                    self.fault(at, QirError::NoSuchQubit { qubit, qubits });
                }
                checked.map(CallOperand::Known)
            }
        }
    }

    /// The result an instruction or a call uses, from `source`.
    fn result(&mut self, at: QirFaultSite, source: Option<Source>) -> Option<CallOperand<u32>> {
        match source? {
            Source::Register(register) => Some(CallOperand::InRegister(register)),
            Source::Constant(result) => {
                let results = self.bytecode.num_results;
                let checked = u32::try_from(result)
                    .ok()
                    .filter(|result| *result < results);
                if checked.is_none() {
                    self.fault(at, QirError::NoSuchResult { result, results });
                }
                checked.map(CallOperand::Known)
            }
        }
    }

    /// The real number a call takes, from `source`: a constant holds its
    /// bits.
    fn real(source: Option<Source>) -> Option<CallOperand<f64>> {
        Some(match source? {
            Source::Register(register) => CallOperand::InRegister(register),
            Source::Constant(bits) => CallOperand::Known(f64::from_bits(bits)),
        })
    }

    /// Checks where the instruction at `index`, with `opcode`, lies: in a
    /// block; a terminator at its block's end; a return from a function
    /// inside one, and the program's end outside them.
    fn placement(&mut self, at: QirFaultSite, index: usize, opcode: u8) {
        let Some(block) = self.owners[index] else {
            self.fault(at, QirError::OutsideBlocks);
            return;
        };
        let end = self.blocks[block].as_ref().map(|range| range.end);
        if is_terminator(opcode) && end != Some(index + 1) {
            self.fault(at, QirError::TerminatorInsideBlock);
        }
        if opcode == OP_CALL_RETURN && self.outside_functions[block] {
            self.fault(at, QirError::ReturnOutsideFunction);
        }
        if opcode == OP_RET && self.inside_functions[block] {
            self.fault(at, QirError::ProgramEndInsideFunction);
        }
    }

    /// Checks the instruction at `index`, and decodes it when it is a
    /// quantum one.
    fn instruction(&mut self, index: usize) -> Option<CheckedCall> {
        use Field::{Aux0, Aux1, Dst, Src0, Src1};
        let at = QirFaultSite::Instruction(index);
        let instruction = self.bytecode.instructions[index];
        let opcode = instruction.primary_opcode();
        self.placement(at, index, opcode);
        match opcode {
            OP_NOP => {}
            OP_RET => self.reads(at, &instruction, &[Dst]),
            OP_JUMP => self.edges(at, &[instruction.dst]),
            OP_BRANCH => {
                self.bit(at, &instruction, Src0);
                self.edges(at, &[instruction.aux0, instruction.aux1]);
            }
            OP_SWITCH => self.switch(at, &instruction),
            OP_CALL => self.call(at, &instruction),
            OP_CALL_RETURN => {
                if self.keeps_return {
                    self.reads(at, &instruction, &[Src0]);
                }
            }
            OP_QUANTUM_GATE | OP_MEASURE | OP_RESET | OP_PEEK_LOSS | OP_READOUT_NOISE => {
                return self.quantum(at, &instruction);
            }
            OP_READ_RESULT | OP_READ_LOSS => {
                // A constant result past the program's is refused, though
                // `run_shot` reads it as `Zero`: no write can ever reach it,
                // so reading it is a producer's bug that would give
                // plausible output (see `QirMachine`'s doc).
                let result = self.source(at, &instruction, Src0);
                self.result(at, result);
                self.register(at, instruction.dst);
            }
            OP_WRITE_RESULT => {
                self.bit(at, &instruction, Src0);
                let result = self.source(at, &instruction, Src1);
                self.result(at, result);
            }
            OP_RECORD_OUTPUT => match instruction.aux1 {
                // A result, refused past the program's as for a read.
                0 => {
                    let result = self.source(at, &instruction, Src0);
                    self.result(at, result);
                }
                3 => self.bit(at, &instruction, Src0),
                4 | 5 => self.reads(at, &instruction, &[Src0]),
                // Kinds 1 and 2 (array, tuple) only mark the output's shape.
                _ => {}
            },
            OP_ADD | OP_SUB | OP_MUL | OP_AND | OP_OR | OP_XOR | OP_SHL | OP_LSHR | OP_ASHR
            | OP_FADD | OP_FSUB | OP_FMUL | OP_FDIV | OP_FREM => {
                self.computes(at, &instruction, &[Src0, Src1]);
            }
            OP_UDIV | OP_SDIV | OP_UREM | OP_SREM => {
                self.computes(at, &instruction, &[Src0, Src1]);
                if instruction.has_flag(FLAG_SRC1_IMM) && instruction.src1 == 0 {
                    self.fault(at, QirError::DivisionByZero);
                }
            }
            OP_ICMP | OP_FCMP => {
                let predicate = instruction.sub_opcode();
                let last = if opcode == OP_ICMP {
                    ICMP_UGE
                } else {
                    FCMP_TRUE
                };
                if predicate > last {
                    self.fault(at, QirError::UnknownPredicate { predicate });
                }
                self.computes(at, &instruction, &[Src0, Src1]);
            }
            // An `ALLOCA` reads its words, not the address in `src1`: a
            // run allocates past the context's memory.
            OP_ZEXT | OP_SEXT | OP_TRUNC | OP_FPEXT | OP_FPTRUNC | OP_INTTOPTR | OP_FPTOSI
            | OP_SITOFP | OP_FPTOUI | OP_UITOFP | OP_MOV | OP_LOAD | OP_ALLOCA => {
                self.computes(at, &instruction, &[Src0]);
            }
            OP_CONST => self.computes(at, &instruction, &[]),
            OP_SELECT => {
                self.bit(at, &instruction, Src0);
                self.computes(at, &instruction, &[Aux0, Aux1]);
            }
            OP_STORE => self.reads(at, &instruction, &[Src0, Src1]),
            OP_GEP => self.computes(at, &instruction, &[Src0, Src1, Aux0]),
            OP_PHI => {
                let entries = self.side_table(
                    at,
                    SideTable::PhiEntries,
                    instruction.aux0,
                    instruction.aux1,
                );
                if let Some(entries) = entries.clone() {
                    for entry in entries {
                        self.register(at, self.bytecode.phi_entries[entry].val_reg);
                    }
                }
                self.register(at, instruction.dst);
                if let Some(block) = self.owners[index] {
                    self.phi_placement(at, block, index);
                    self.phi_edges(at, block, entries);
                }
            }
            _ => self.fault(at, QirError::UnknownOpcode { opcode }),
        }
        None
    }

    /// Checks an instruction that reads `fields` and writes its `dst`.
    fn computes(&mut self, at: QirFaultSite, instruction: &Instruction<u64>, fields: &[Field]) {
        self.reads(at, instruction, fields);
        self.register(at, instruction.dst);
    }

    /// Checks that the `PHI` at `index`, in `block`, follows in its block
    /// only `PHI`s and `MOV`s of an immediate, naming the last instruction
    /// before it that is neither.
    fn phi_placement(&mut self, at: QirFaultSite, block: usize, index: usize) {
        // In valid QIR, `phi`s come first in their block, as LLVM requires
        // (`QirError::PhiAfterInstruction` quotes the rule). So in a
        // bytecode the pass makes from it (`QirError` says why), only
        // other `PHI`s and the `MOV`s below come before a `PHI`. Another
        // instruction (a `CALL`, say) is refused, though `run_shot` runs
        // it.
        //
        // DISCUSS COMPATIBILITY: a `MOV` of an immediate is accepted
        // before a `PHI`, though the QIR's rule refuses it. The QIR's
        // `phi`s come first, and may take constant values; the
        // adaptive pass loads each constant value of a `phi` into a fresh
        // register with such a `MOV`, in the `PHI`'s own block, as the
        // `PHI` entries hold registers. Such a `MOV` leaves the last edge
        // as it is, so the `PHI` still takes the entry for the edge into
        // its block. We accept them meanwhile, and recommend keeping this
        // acceptance until the pass puts these `MOV`s in the predecessor
        // blocks, then refusing a `PHI` after any instruction but a `PHI`,
        // as LLVM does.
        let start = self.blocks[block]
            .as_ref()
            .map(|range| range.start)
            .expect("an instruction's owner is a block of an accepted range");
        if let Some(offset) = self.bytecode.instructions[start..index]
            .iter()
            .rposition(|instruction| !may_precede_phi(instruction))
        {
            let instruction = start + offset;
            self.fault(at, QirError::PhiAfterInstruction { instruction });
        }
    }

    /// Checks that a `PHI` of `block`, with the entries `entries` when
    /// their range is in their table, has a value on every edge into
    /// `block`, as a bytecode the pass makes from valid QIR does
    /// ([`QirError`] says why): `block` is no entry block, and an entry
    /// names each of its predecessors.
    fn phi_edges(&mut self, at: QirFaultSite, block: usize, entries: Option<Range<usize>>) {
        // The other QDK runtimes (`run_shot`, the GPU shader, `_qodec`)
        // accept a `PHI` in a function's entry block: a `CALL` keeps the
        // caller's last edge, so the `PHI` takes the entry for the
        // caller's last block, or writes nothing with no such entry (at
        // the program's entry, the previous block starts as 0). The value
        // then depends on the caller's control flow. No valid QIR gives
        // it (`QirError::PhiInEntryBlock` quotes LLVM's rule).
        if self.entry_blocks[block] {
            self.fault(at, QirError::PhiInEntryBlock { block });
        }
        let Some(entries) = entries else {
            return;
        };
        // The other QDK runtimes write nothing on an edge a `PHI` has no
        // entry for, so its register keeps its earlier value, silently. No
        // valid QIR gives such a `PHI` (`QirError::MissingPhiEntry` quotes
        // LLVM's rule), so every edge counts, also one no run takes. A
        // block with no predecessor needs no entry: a run enters it only
        // as an entry block, refused above, or never. An entry for a block
        // that is no predecessor is accepted: no run reads it.
        let entries = &self.bytecode.phi_entries[entries];
        let missing: Vec<usize> = self.predecessors[block]
            .iter()
            .copied()
            .filter(|predecessor| {
                !entries
                    .iter()
                    .any(|entry| entry.block_id == *predecessor as u64)
            })
            .collect();
        for predecessor in missing {
            self.fault(at, QirError::MissingPhiEntry { block, predecessor });
        }
    }

    fn switch(&mut self, at: QirFaultSite, instruction: &Instruction<u64>) {
        self.reads(at, instruction, &[Field::Src0]);
        let mut targets = vec![instruction.aux0];
        if let Some(cases) = self.side_table(
            at,
            SideTable::SwitchCases,
            instruction.aux1,
            instruction.aux2,
        ) {
            targets.extend(
                self.bytecode.switch_cases[cases]
                    .iter()
                    .map(|case| case.target_block),
            );
        }
        self.edges(at, &targets);
    }

    /// Checks the edges with which a jump, a branch or a switch ends its
    /// block: each of `targets`, the blocks it goes to, is in the block
    /// table, and none is an entry block (one fault per entry block, in the
    /// order of `targets`).
    fn edges(&mut self, at: QirFaultSite, targets: &[u64]) {
        for target in targets {
            self.block(at, *target);
        }
        // The other QDK runtimes take an edge into an entry block as any
        // other. No valid QIR gives one (`QirError::EdgeToEntryBlock`
        // quotes LLVM's rule), so a run enters an entry block by no edge.
        let mut entered: Vec<usize> = Vec::new();
        for target in targets {
            if let Some(block) = self.index(*target)
                && self.entry_blocks[block]
                && !entered.contains(&block)
            {
                entered.push(block);
            }
        }
        for block in entered {
            self.fault(at, QirError::EdgeToEntryBlock { block });
        }
    }

    fn call(&mut self, at: QirFaultSite, instruction: &Instruction<u64>) {
        let functions = self.bytecode.function_table.len();
        let function = usize::try_from(instruction.aux0)
            .ok()
            .and_then(|function| self.bytecode.function_table.get(function).copied());
        if function.is_none() {
            self.fault(
                at,
                QirError::NoSuchFunction {
                    function: instruction.aux0,
                    functions,
                },
            );
        }
        let count = instruction.aux1;
        self.call_arguments(at, instruction.aux2, count);
        // The last register the arguments are copied into must exist.
        if let Some((_, last)) = function.and_then(|function| parameter_registers(&function, count))
        {
            self.register(at, last);
        }
        if instruction.dst != VOID_RETURN {
            self.register(at, instruction.dst);
        }
    }

    /// Checks a quantum instruction, and decodes it as a call.
    fn quantum(&mut self, at: QirFaultSite, instruction: &Instruction<u64>) -> Option<CheckedCall> {
        use Field::{Aux0, Aux1, Aux2, Src0};
        let (encoding, fields): (_, &[Field]) = match instruction.primary_opcode() {
            OP_PEEK_LOSS => (Encoding::PeekLoss, &[Aux0, Aux1]),
            OP_READOUT_NOISE => (Encoding::ReadoutNoise, &[Aux0, Aux1, Aux2]),
            opcode => {
                let op_id = self.operation(at, instruction.aux0)?;
                let (encoding, fields): (_, &[Field]) = match opcode {
                    // A run reads the count and offset of its qubits, and
                    // no angle.
                    OP_QUANTUM_GATE if op_id == OPID_CORRELATED_NOISE => {
                        (Encoding::Gate(op_id), &[Aux1, Aux2])
                    }
                    // A run reads all three fields, whatever the gate.
                    OP_QUANTUM_GATE => (Encoding::Gate(op_id), &[Aux1, Aux2, Src0]),
                    OP_MEASURE => (Encoding::Measure(op_id), &[Aux1, Aux2]),
                    _ => (Encoding::Reset(op_id), &[Aux1]),
                };
                if quantum_instruction_encoding::position(encoding).is_none() {
                    self.fault(at, QirError::UnsupportedOperation { op_id });
                    return None;
                }
                (encoding, fields)
            }
        };
        let (_, called) = QIR_INSTRUCTIONS[quantum_instruction_encoding::position(encoding)?];
        let sources: Vec<Option<Source>> = fields
            .iter()
            .map(|field| self.source(at, instruction, *field))
            .collect();
        let mut operands = CallOperands::NONE;
        // Each operand is checked before any is dropped, so that every
        // fault is reported; an operand is `None` only where one was.
        let complete = match quantum_instruction_encoding::shape(called) {
            Shape::Qubit => self.qubits(at, &mut operands, &sources[..1]),
            Shape::TwoQubits => self.qubits(at, &mut operands, &sources[..2]),
            Shape::QubitAngle => {
                let qubit = self.qubits(at, &mut operands, &sources[..1]);
                Self::reals(&mut operands, &sources[2..3]) && qubit
            }
            Shape::TwoQubitsAngle => {
                let qubits = self.qubits(at, &mut operands, &sources[..2]);
                Self::reals(&mut operands, &sources[2..3]) && qubits
            }
            Shape::QubitResult => {
                let qubit = self.qubits(at, &mut operands, &sources[..1]);
                operands.result = self.result(at, sources[1]);
                operands.result.is_some() && qubit
            }
            Shape::ReadoutNoise => {
                let reals = Self::reals(&mut operands, &sources[..2]);
                operands.result = self.result(at, sources[2]);
                operands.result.is_some() && reals
            }
            Shape::CorrelatedNoise => {
                let (count, offset) = (sources[0]?, sources[1]?);
                if let (Source::Constant(count), Source::Constant(offset)) = (count, offset) {
                    self.call_arguments(at, offset, count);
                }
                return Some(CheckedCall::CorrelatedNoise {
                    table: self.noise_table(instruction),
                    count,
                    offset,
                });
            }
        };
        complete.then_some(CheckedCall::Call {
            instruction: called,
            operands,
        })
    }

    /// Checks the qubits a call acts on, from `sources`, into `operands`'s
    /// first qubits; whether each is known or held in a register.
    fn qubits(
        &mut self,
        at: QirFaultSite,
        operands: &mut CallOperands,
        sources: &[Option<Source>],
    ) -> bool {
        let mut complete = true;
        for (slot, source) in operands.qubits.iter_mut().zip(sources) {
            match self.qubit(at, *source) {
                Some(qubit) => *slot = qubit,
                None => complete = false,
            }
        }
        complete
    }

    /// The real numbers a call takes, from `sources`, into `operands`'s
    /// first reals; whether each is known or held in a register.
    fn reals(operands: &mut CallOperands, sources: &[Option<Source>]) -> bool {
        let mut complete = true;
        for (slot, source) in operands.reals.iter_mut().zip(sources) {
            match Self::real(*source) {
                Some(real) => *slot = real,
                None => complete = false,
            }
        }
        complete
    }

    /// The noise table that correlated noise `instruction` draws from: its
    /// operation's `q1`, cut to 32 bits as `run_shot` cuts it.
    fn noise_table(&self, instruction: &Instruction<u64>) -> u32 {
        let operation = usize::try_from(instruction.aux0)
            .ok()
            .and_then(|index| self.bytecode.quantum_ops.get(index))
            .expect("the instruction's operation is in the table, as checked");
        #[allow(clippy::cast_possible_truncation)]
        let table = operation.q1 as u32;
        table
    }

    /// Checks that the quantum operation table holds `operation`, and gives
    /// its operation id.
    fn operation(&mut self, at: QirFaultSite, operation: u64) -> Option<u64> {
        let operations = &self.bytecode.quantum_ops;
        let op_id = usize::try_from(operation)
            .ok()
            .and_then(|index| operations.get(index))
            .map(|operation| operation.op_id);
        if op_id.is_none() {
            let operations = operations.len();
            self.fault(
                at,
                QirError::NoSuchOperation {
                    operation,
                    operations,
                },
            );
        }
        op_id
    }
}
