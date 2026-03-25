// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! PyO3-friendly instruction and operand types for the `QuantumProgram` trait.
//! These are simple value types with no references to RIR internals, making them
//! suitable for passing across a Python/Rust boundary via `PyO3`.

/// A block identifier (index).
pub type BlockIdx = usize;

/// A variable identifier (index).
pub type VariableIdx = usize;

/// A debug location identifier (index).
pub type DbgLocationIdx = usize;

/// The type of a variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarTy {
    Qubit,
    Result,
    Boolean,
    Integer,
    Double,
    Pointer,
}

/// A typed variable reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Var {
    pub id: VariableIdx,
    pub ty: VarTy,
}

/// A literal value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lit {
    Qubit(u32),
    Result(u32),
    Bool(bool),
    Integer(i64),
    Double(f64),
    Pointer,
    Tag(usize, usize),
}

/// An operand — either a literal value or a variable reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Opr {
    Literal(Lit),
    Variable(Var),
}

/// Integer comparison condition codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpCondition {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
}

/// Floating-point comparison condition codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcmpCondition {
    False,
    OrderedAndEqual,
    OrderedAndGreaterThan,
    OrderedAndGreaterThanOrEqual,
    OrderedAndLessThan,
    OrderedAndLessThanOrEqual,
    OrderedAndNotEqual,
    Ordered,
    UnorderedOrEqual,
    UnorderedOrGreaterThan,
    UnorderedOrGreaterThanOrEqual,
    UnorderedOrLessThan,
    UnorderedOrLessThanOrEqual,
    UnorderedOrNotEqual,
    Unordered,
    True,
}

/// A program instruction — simple value enum with no references to RIR internals.
#[derive(Clone, Debug)]
pub enum Instr {
    /// Call a callable with operands, optionally storing the result in a variable.
    Call {
        callable_name: String,
        args: Vec<Opr>,
        output: Option<Var>,
        dbg_location: Option<DbgLocationIdx>,
    },
    /// Jump unconditionally to a block.
    Jump(BlockIdx),
    /// Branch conditionally to one of two blocks.
    Branch {
        condition: Var,
        true_block: BlockIdx,
        false_block: BlockIdx,
        dbg_location: Option<DbgLocationIdx>,
    },
    /// Return from the program.
    Return,
    /// Integer comparison.
    Icmp(IcmpCondition, Opr, Opr, Var),
    /// Floating-point comparison.
    Fcmp(FcmpCondition, Opr, Opr, Var),
    /// Phi node — merge values from predecessor blocks.
    Phi(Vec<(Opr, BlockIdx)>, Var),
    /// Binary arithmetic/logic operation.
    BinOp(BinOpKind, Opr, Opr, Var),
    /// Logical NOT.
    LogicalNot(Opr, Var),
    /// Type conversion.
    Convert(Opr, Var),
}

/// The kind of binary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Sdiv,
    Srem,
    Shl,
    Ashr,
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
    LogicalAnd,
    LogicalOr,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
}
