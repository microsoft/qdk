// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What building a [`QirProgram`] reports when its bytecode has faults:
//! every fault ([`QirFaults`]), each ([`QirFault`]) where it is
//! ([`QirFaultSite`]) and what it is ([`QirError`], naming the
//! [`SideTable`] a range reaches past), so that whoever emits the bytecode
//! can fix them all at once.

use std::fmt;

#[cfg(doc)]
use super::{QirInstruction, QirProgram};
#[cfg(doc)]
use crate::bytecode::AdaptiveProgram;

/// Every fault of a bytecode program, so that whoever emits it can fix
/// them all at once: the program's own first, then those of its blocks,
/// of its functions and of its instructions, each in their order. A
/// quantum instruction whose operation is refused has its operands left
/// unchecked, as what they mean depends on the operation. It reads as one
/// message: each fault as its site and its error (`site: error`), joined
/// by `"; "`.
#[derive(Clone, Debug, PartialEq)]
pub struct QirFaults(pub Vec<QirFault>);

impl fmt::Display for QirFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, fault) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{fault}")?;
        }
        Ok(())
    }
}

impl std::error::Error for QirFaults {}

/// One fault of a bytecode program: where it is, and what it is.
#[derive(Clone, Debug, PartialEq)]
pub struct QirFault {
    /// Where it is.
    pub at: QirFaultSite,
    /// What it is.
    pub error: QirError,
}

impl fmt::Display for QirFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.error)
    }
}

/// Where a fault is in a bytecode program, by its index in its table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QirFaultSite {
    /// The program's own fields: its entry block.
    Program,
    /// The block at this index of the block table.
    Block(usize),
    /// The function at this index of the function table.
    Function(usize),
    /// The instruction at this index of the instructions.
    Instruction(usize),
}

impl fmt::Display for QirFaultSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QirFaultSite::Program => f.write_str("the program"),
            QirFaultSite::Block(index) => write!(f, "block {index}"),
            QirFaultSite::Function(index) => write!(f, "function {index}"),
            QirFaultSite::Instruction(index) => write!(f, "instruction {index}"),
        }
    }
}

/// A side table of an [`AdaptiveProgram`]. Each bytecode instruction has a
/// fixed number of operand fields (`dst`, `src0`, `src1`, `aux0` to
/// `aux2`), but some instructions need a list of any length, which does
/// not fit them. So the program keeps those lists in three flat vectors
/// beside its instructions, its side tables, and an instruction names its
/// list in one by an offset and a count. [`QirError::PastSideTable`] names
/// the table whose end such a range passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideTable {
    /// [`AdaptiveProgram::phi_entries`]: the entries of the `PHI`
    /// instructions.
    PhiEntries,
    /// [`AdaptiveProgram::switch_cases`]: the cases of the `SWITCH`
    /// instructions.
    SwitchCases,
    /// [`AdaptiveProgram::call_args`]: the argument registers of the
    /// `CALL` instructions, and the registers holding the qubits of
    /// correlated noise.
    CallArgs,
}

impl fmt::Display for SideTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SideTable::PhiEntries => "phi entries",
            SideTable::SwitchCases => "switch cases",
            SideTable::CallArgs => "call arguments",
        })
    }
}

/// What is wrong at a [`QirFaultSite`], found when the program is built,
/// before any shot runs. Each variant states one rule of building, and why
/// it holds: most refuse content on which a run would stop; a few are
/// stricter than a run needs, and say so; four refuse what no valid QIR
/// gives. A value known only during a run (one held in a register, a
/// memory address) is not checked: a shot that meets a wrong one traps
/// ([`QirTrap`](super::QirTrap)).
///
/// The rules on edges and `PHI`s ([`EdgeToEntryBlock`], [`PhiInEntryBlock`],
/// [`MissingPhiEntry`], [`PhiAfterInstruction`]) come from the QIR, not
/// from the bytecode, which is not LLVM: the adaptive pass keeps the QIR's
/// control flow, giving each basic block one block, each target of a `br`
/// or a `switch` one edge, and each incoming pair of a `phi` one `PHI`
/// entry. So a bytecode that the pass makes from valid QIR follows LLVM's
/// rules on entry blocks and `phi`s, and one that breaks them stands for no
/// QIR program: it comes from a bug in its producer, or was written by
/// hand.
///
/// Variants are added as checks are, hence `#[non_exhaustive]`.
///
/// [`EdgeToEntryBlock`]: Self::EdgeToEntryBlock
/// [`PhiInEntryBlock`]: Self::PhiInEntryBlock
/// [`MissingPhiEntry`]: Self::MissingPhiEntry
/// [`PhiAfterInstruction`]: Self::PhiAfterInstruction
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum QirError {
    /// A block that is not in the block table: the entry block, a
    /// function's entry block, or a jump's, branch's or switch's target.
    NoSuchBlock { block: u64, blocks: usize },
    /// The block's instructions reach past the program's.
    BlockPastInstructions {
        offset: u64,
        count: u64,
        instructions: usize,
    },
    /// The block holds no instruction.
    EmptyBlock,
    /// The block holds instructions that an earlier block also holds. Each
    /// instruction belongs to one block, so that a run leaves a block only
    /// through its terminator.
    BlocksOverlap { earlier: usize },
    /// No block holds the instruction. Each instruction belongs to one
    /// block, so that a run leaves a block only through its terminator.
    OutsideBlocks,
    /// The block does not end with a terminator (`RET`, `JUMP`, `BRANCH`,
    /// `SWITCH`, `CALL_RETURN`), so a run would go on into the next
    /// instruction of the program, whatever block holds it.
    NoTerminator,
    /// A terminator before the end of its block, so that the instructions
    /// after it never run: each block ends with its only terminator.
    TerminatorInsideBlock,
    /// A `CALL_RETURN` that a run reaches from the entry block through
    /// jumps, branches and switches alone, so with no call to return from;
    /// a block that a function reaches too counts as outside.
    ReturnOutsideFunction,
    /// A `RET`, which ends the program, that a run reaches from the entry
    /// block of a function a `CALL` names, through jumps, branches and
    /// switches alone, so inside a call, where it would end the program
    /// before the function returns (with `CALL_RETURN`); a block that the
    /// program's entry block reaches too counts as inside. This is stricter
    /// than a run needs, which could end the program there: a function
    /// returns with `CALL_RETURN`, and the adaptive pass emits `RET` only
    /// in the entry point.
    ProgramEndInsideFunction,
    /// A `PHI` of `block` with no entry for `predecessor`, a block whose
    /// terminator (a jump, a branch, or a switch's default or case) goes to
    /// `block`, whether or not a run takes that edge. No valid QIR gives
    /// one: LLVM's `phi` takes "one pair for each predecessor basic block
    /// of the current block"
    /// ([LangRef](https://llvm.org/docs/LangRef.html#phi-instruction)).
    MissingPhiEntry { block: usize, predecessor: usize },
    /// A `PHI` in `block`, the program's entry block or the entry block of
    /// a function a `CALL` names, which a run enters by no edge. No valid
    /// QIR gives one: LLVM's entry block "is not allowed to have
    /// predecessor basic blocks … Because the block can have no
    /// predecessors, it also cannot have any PHI nodes"
    /// ([LangRef](https://llvm.org/docs/LangRef.html#functions)).
    PhiInEntryBlock { block: usize },
    /// A jump, a branch, or a switch's default or case, that goes to
    /// `block`, the program's entry block or the entry block of a function
    /// a `CALL` names. No valid QIR gives one: LLVM's entry block "is not
    /// allowed to have predecessor basic blocks (i.e., there can not be any
    /// branches to the entry block of a function)"
    /// ([LangRef](https://llvm.org/docs/LangRef.html#functions)), so a run
    /// enters it only when the program starts or a call does.
    EdgeToEntryBlock { block: usize },
    /// A `PHI` that follows, in its block, the instruction at index
    /// `instruction`, which is neither a `PHI` nor a `MOV` of an immediate.
    /// No valid QIR gives one, as LLVM puts `phi`s first: "There must be no
    /// non-phi instructions between the start of a basic block and the PHI
    /// instructions: i.e. PHI instructions must be first in a basic block"
    /// ([LangRef](https://llvm.org/docs/LangRef.html#phi-instruction)). A
    /// `MOV` of an immediate is accepted there, unlike in the QIR, because
    /// the adaptive pass puts one there for each constant value of a `phi`:
    /// the QIR's `phi`s may take constant values, but the bytecode's `PHI`
    /// entries hold registers, so the pass loads each constant value into a
    /// fresh register in the `PHI`'s own block.
    PhiAfterInstruction { instruction: usize },
    /// An opcode the bytecode does not define.
    UnknownOpcode { opcode: u8 },
    /// A comparison predicate the bytecode does not define for this
    /// comparison (`ICMP` or `FCMP`).
    UnknownPredicate { predicate: u8 },
    /// A register past the program's registers, in a field that a run
    /// reads or writes as a register, as the instruction's flags make it
    /// (a field flagged constant is none): an operand, a destination, a
    /// value a terminator reads, a `PHI` entry's value, a call argument,
    /// or the last register a call copies its arguments into. Once any call
    /// keeps its function's value, every `CALL_RETURN`'s value is checked
    /// as a register, also in a function that only calls dropping the
    /// value reach, which is stricter than a run needs.
    NoSuchRegister { register: u64, registers: u32 },
    /// A function that is not in the function table.
    NoSuchFunction { function: u64, functions: usize },
    /// A quantum operation that is not in the quantum operation table.
    NoSuchOperation { operation: u64, operations: usize },
    /// A range of a side table that starts or ends past its end. A run
    /// would read no entry of an empty range, but building refuses one
    /// that starts past the end too, which is stricter than a run needs: no
    /// producer emits it, and every range then obeys one rule. The range
    /// of correlated noise whose count or offset is held in a register is
    /// known only during a run, and not checked.
    PastSideTable {
        table: SideTable,
        offset: u64,
        count: u64,
        len: usize,
    },
    /// An operation id that the instruction's opcode does not run: an
    /// unknown id, an id of another opcode (`mz` in a `QUANTUM_GATE`), the
    /// identity (0), which is no [`QirInstruction`], or `ccx`, which the
    /// Python passes decompose first.
    UnsupportedOperation { op_id: u64 },
    /// A constant qubit past the program's qubits.
    NoSuchQubit { qubit: u64, qubits: u32 },
    /// A constant result past the program's results, which an instruction
    /// uses. For a read, a loss check or a record of the result, this is
    /// stricter than a run needs: no write can reach the result.
    NoSuchResult { result: u64, results: u32 },
    /// A division or remainder by the constant zero.
    DivisionByZero,
    /// A constant other than 0 and 1 where a run reads a bit: a branch's
    /// or a select's condition, a bool recorded, or a result written.
    NotABit { value: u64 },
}

impl fmt::Display for QirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QirError::NoSuchBlock { block, blocks } => {
                write!(f, "block {block} does not exist (the program has {blocks})")
            }
            QirError::BlockPastInstructions {
                offset,
                count,
                instructions,
            } => write!(
                f,
                "its {count} instructions from {offset} reach past the program's {instructions}"
            ),
            QirError::EmptyBlock => f.write_str("it holds no instruction"),
            QirError::BlocksOverlap { earlier } => {
                write!(f, "it holds instructions that block {earlier} holds")
            }
            QirError::OutsideBlocks => f.write_str("no block holds it"),
            QirError::NoTerminator => f.write_str("it does not end with a terminator"),
            QirError::TerminatorInsideBlock => {
                f.write_str("a terminator before the end of its block")
            }
            QirError::ReturnOutsideFunction => {
                f.write_str("a return from a function, reached from the entry block with no call")
            }
            QirError::ProgramEndInsideFunction => {
                f.write_str("an end of the program, reached from a called function's entry block")
            }
            QirError::MissingPhiEntry { block, predecessor } => write!(
                f,
                "a phi of block {block} with no entry for block {predecessor}, which goes to it"
            ),
            QirError::PhiInEntryBlock { block } => write!(
                f,
                "a phi in block {block}, the entry block of the program or of a called function"
            ),
            QirError::EdgeToEntryBlock { block } => write!(
                f,
                "it goes to block {block}, the entry block of the program or of a called function"
            ),
            QirError::PhiAfterInstruction { instruction } => write!(
                f,
                "a phi after instruction {instruction}, neither a phi nor a move of an immediate"
            ),
            QirError::UnknownOpcode { opcode } => write!(f, "unknown opcode 0x{opcode:02X}"),
            QirError::UnknownPredicate { predicate } => {
                write!(f, "unknown comparison predicate {predicate}")
            }
            QirError::NoSuchRegister {
                register,
                registers,
            } => write!(
                f,
                "register {register} does not exist (the program has {registers})"
            ),
            QirError::NoSuchFunction {
                function,
                functions,
            } => write!(
                f,
                "function {function} does not exist (the program has {functions})"
            ),
            QirError::NoSuchOperation {
                operation,
                operations,
            } => write!(
                f,
                "quantum operation {operation} does not exist (the program has {operations})"
            ),
            QirError::PastSideTable {
                table,
                offset,
                count,
                len,
            } => write!(
                f,
                "its {count} {table} from {offset} reach past the program's {len}"
            ),
            QirError::UnsupportedOperation { op_id } => {
                write!(f, "operation id {op_id} is not run by this opcode")
            }
            QirError::NoSuchQubit { qubit, qubits } => {
                write!(f, "qubit {qubit} does not exist (the program has {qubits})")
            }
            QirError::NoSuchResult { result, results } => write!(
                f,
                "result {result} does not exist (the program has {results})"
            ),
            QirError::DivisionByZero => f.write_str("a division by the constant zero"),
            QirError::NotABit { value } => write!(
                f,
                "the constant {value} where a bit is read, neither 0 nor 1"
            ),
        }
    }
}
