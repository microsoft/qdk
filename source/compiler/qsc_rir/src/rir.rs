// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indenter::{Indented, indented};
use qsc_data_structures::{index_map::IndexMap, target::TargetCapabilityFlags};
use std::fmt::{self, Display, Formatter, Write};

/// The root of the RIR.
#[derive(Default, Clone)]
pub struct Program {
    pub entry: CallableId,
    pub callables: IndexMap<CallableId, Callable>,
    pub blocks: IndexMap<BlockId, Block>,
    pub config: Config,
    pub num_qubits: u32,
    pub num_results: u32,
    pub tags: Vec<String>,
}

impl Display for Program {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Program:",)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nentry: {}", self.entry.0)?;
        write!(indent, "\ncallables:")?;
        indent = set_indentation(indent, 2);
        for (callable_id, callable) in self.callables.iter() {
            write!(indent, "\nCallable {}: {}", callable_id.0, callable)?;
        }
        indent = set_indentation(indent, 1);
        write!(indent, "\nblocks:")?;
        indent = set_indentation(indent, 2);
        for (block_id, block) in self.blocks.iter() {
            write!(indent, "\nBlock {}: {}", block_id.0, block)?;
        }
        indent = set_indentation(indent, 1);
        write!(indent, "\nconfig: {}", self.config)?;
        write!(indent, "\nnum_qubits: {}", self.num_qubits)?;
        write!(indent, "\nnum_results: {}", self.num_results)?;
        writeln!(indent, "\ntags:")?;
        indent = set_indentation(indent, 2);
        for (idx, tag) in self.tags.iter().enumerate() {
            writeln!(indent, "[{idx}]: {tag}")?;
        }
        Ok(())
    }
}

impl Program {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get_callable(&self, id: CallableId) -> &Callable {
        self.callables.get(id).expect("callable should be present")
    }

    #[must_use]
    pub fn get_block(&self, id: BlockId) -> &Block {
        self.blocks.get(id).expect("block should be present")
    }

    #[must_use]
    pub fn get_block_mut(&mut self, id: BlockId) -> &mut Block {
        self.blocks.get_mut(id).expect("block should be present")
    }
}

#[derive(Default, Clone, Copy)]
pub struct Config {
    pub capabilities: TargetCapabilityFlags,
}

impl Display for Config {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Config:",)?;
        indent = set_indentation(indent, 1);
        if self.capabilities.is_empty() {
            write!(indent, "\ncapabilities: Base")?;
        } else {
            write!(indent, "\ncapabilities: {:?}", self.capabilities)?;
        }
        Ok(())
    }
}

impl Config {
    #[must_use]
    pub fn is_base(&self) -> bool {
        self.capabilities == TargetCapabilityFlags::empty()
    }
}

/// A unique identifier for a block in a RIR program.
#[derive(Clone, Copy, Debug, Default, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct BlockId(pub u32);

impl From<BlockId> for usize {
    fn from(id: BlockId) -> usize {
        id.0 as usize
    }
}

impl From<usize> for BlockId {
    fn from(id: usize) -> Self {
        Self(id.try_into().expect("block id should fit into u32"))
    }
}

impl Display for Block {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Block:",)?;
        if self.0.is_empty() {
            write!(indent, " <EMPTY>")?;
        } else {
            indent = set_indentation(indent, 1);
            for instruction in &self.0 {
                write!(indent, "\n{instruction}")?;
            }
        }
        Ok(())
    }
}

impl BlockId {
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

/// A block is a collection of instructions.
#[derive(Default, Clone)]
pub struct Block(pub Vec<Instruction>);

/// A unique identifier for a callable in a RIR program.
#[derive(Clone, Copy, Debug, Default, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct CallableId(pub u32);

impl From<CallableId> for usize {
    fn from(id: CallableId) -> Self {
        id.0 as Self
    }
}

impl From<usize> for CallableId {
    fn from(id: usize) -> Self {
        Self(id.try_into().expect("callable id should fit into u32"))
    }
}

impl CallableId {
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

/// A callable.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Callable {
    /// The name of the callable.
    pub name: String,
    /// The input type of the callable.
    pub input_type: Vec<Ty>,
    /// The output type of the callable.
    pub output_type: Option<Ty>,
    /// The callable body.
    /// N.B. `None` bodys represent an intrinsic.
    pub body: Option<BlockId>,
    /// What type of callable this is.
    pub call_type: CallableType,
}

impl Display for Callable {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Callable:",)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nname: {}", self.name)?;
        write!(indent, "\ncall_type: {}", self.call_type)?;
        write!(indent, "\ninput_type:")?;
        if self.input_type.is_empty() {
            write!(indent, " <VOID>")?;
        } else {
            indent = set_indentation(indent, 2);
            for (index, ty) in self.input_type.iter().enumerate() {
                write!(indent, "\n[{index}]: {ty}")?;
            }
            indent = set_indentation(indent, 1);
        }
        write!(indent, "\noutput_type:")?;
        if let Some(output_type) = &self.output_type {
            write!(indent, " {output_type}")?;
        } else {
            write!(indent, " <VOID>")?;
        }
        write!(indent, "\nbody:")?;
        if let Some(body_block_id) = self.body {
            write!(indent, " {}", body_block_id.0)?;
        } else {
            write!(indent, " <NONE>")?;
        }
        Ok(())
    }
}

/// The type of callable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableType {
    Measurement,
    Reset,
    Readout,
    OutputRecording,
    Regular,
}

impl Display for CallableType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::Measurement => write!(f, "Measurement")?,
            Self::Readout => write!(f, "Readout")?,
            Self::OutputRecording => write!(f, "OutputRecording")?,
            Self::Regular => write!(f, "Regular")?,
            Self::Reset => write!(f, "Reset")?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionCode {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
}

impl Display for ConditionCode {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::Eq => write!(f, "Eq")?,
            Self::Ne => write!(f, "Ne")?,
            Self::Slt => write!(f, "Slt")?,
            Self::Sle => write!(f, "Sle")?,
            Self::Sgt => write!(f, "Sgt")?,
            Self::Sge => write!(f, "Sge")?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcmpConditionCode {
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

impl Display for FcmpConditionCode {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::False => write!(f, "False")?,
            Self::OrderedAndEqual => write!(f, "Oeq")?,
            Self::OrderedAndGreaterThan => write!(f, "Ogt")?,
            Self::OrderedAndGreaterThanOrEqual => write!(f, "Oge")?,
            Self::OrderedAndLessThan => write!(f, "Olt")?,
            Self::OrderedAndLessThanOrEqual => write!(f, "Ole")?,
            Self::OrderedAndNotEqual => write!(f, "One")?,
            Self::Ordered => write!(f, "Ord")?,
            Self::UnorderedOrEqual => write!(f, "Ueq")?,
            Self::UnorderedOrGreaterThan => write!(f, "Ugt")?,
            Self::UnorderedOrGreaterThanOrEqual => write!(f, "Uge")?,
            Self::UnorderedOrLessThan => write!(f, "Ult")?,
            Self::UnorderedOrLessThanOrEqual => write!(f, "Ule")?,
            Self::UnorderedOrNotEqual => write!(f, "Une")?,
            Self::Unordered => write!(f, "Uno")?,
            Self::True => write!(f, "True")?,
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Store(Operand, Variable),
    Call(CallableId, Vec<Operand>, Option<Variable>),
    Jump(BlockId),
    Branch(Variable, BlockId, BlockId),
    Add(Operand, Operand, Variable),
    Sub(Operand, Operand, Variable),
    Mul(Operand, Operand, Variable),
    Sdiv(Operand, Operand, Variable),
    Srem(Operand, Operand, Variable),
    Shl(Operand, Operand, Variable),
    Ashr(Operand, Operand, Variable),
    Fadd(Operand, Operand, Variable),
    Fsub(Operand, Operand, Variable),
    Fmul(Operand, Operand, Variable),
    Fdiv(Operand, Operand, Variable),
    Fcmp(FcmpConditionCode, Operand, Operand, Variable),
    Icmp(ConditionCode, Operand, Operand, Variable),
    LogicalNot(Operand, Variable),
    LogicalAnd(Operand, Operand, Variable),
    LogicalOr(Operand, Operand, Variable),
    BitwiseNot(Operand, Variable),
    BitwiseAnd(Operand, Operand, Variable),
    BitwiseOr(Operand, Operand, Variable),
    BitwiseXor(Operand, Operand, Variable),
    Phi(Vec<(Operand, BlockId)>, Variable),
    Return,
}

impl Display for Instruction {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fn write_binary_instruction(
            f: &mut Formatter,
            instruction: &str,
            lhs: &Operand,
            rhs: &Operand,
            variable: Variable,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "{variable} = {instruction} {lhs}, {rhs}")?;
            Ok(())
        }

        fn write_branch(
            f: &mut Formatter,
            condition: Variable,
            if_true: BlockId,
            if_false: BlockId,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "Branch {condition}, {}, {}", if_true.0, if_false.0)?;
            Ok(())
        }

        fn write_call(
            f: &mut Formatter,
            callable_id: CallableId,
            args: &[Operand],
            variable: Option<Variable>,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            if let Some(variable) = variable {
                write!(indent, "{variable} = ")?;
            }
            write!(indent, "Call id({}), args( ", callable_id.0)?;
            for arg in args {
                write!(indent, "{arg}, ")?;
            }
            write!(indent, ")")?;
            Ok(())
        }

        fn write_unary_instruction(
            f: &mut Formatter,
            instruction: &str,
            value: &Operand,
            variable: Variable,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "{variable} = {instruction} {value}")?;
            Ok(())
        }

        fn write_fcmp_instruction(
            f: &mut Formatter,
            condition: FcmpConditionCode,
            lhs: &Operand,
            rhs: &Operand,
            variable: Variable,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "{variable} = Fcmp {condition}, {lhs}, {rhs}")?;
            Ok(())
        }

        fn write_icmp_instruction(
            f: &mut Formatter,
            condition: ConditionCode,
            lhs: &Operand,
            rhs: &Operand,
            variable: Variable,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "{variable} = Icmp {condition}, {lhs}, {rhs}")?;
            Ok(())
        }

        fn write_phi_instruction(
            f: &mut Formatter,
            args: &[(Operand, BlockId)],
            variable: Variable,
        ) -> fmt::Result {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "{variable} = Phi ( ")?;
            for (val, block_id) in args {
                write!(indent, "[{val}, {}], ", block_id.0)?;
            }
            write!(indent, ")")?;
            Ok(())
        }

        match &self {
            Self::Store(value, variable) => write_unary_instruction(f, "Store", value, *variable)?,
            Self::Jump(block_id) => write!(f, "Jump({})", block_id.0)?,
            Self::Call(callable_id, args, variable) => {
                write_call(f, *callable_id, args, *variable)?;
            }
            Self::Branch(condition, if_true, if_false) => {
                write_branch(f, *condition, *if_true, *if_false)?;
            }
            Self::Add(lhs, rhs, variable) => {
                write_binary_instruction(f, "Add", lhs, rhs, *variable)?;
            }
            Self::Sub(lhs, rhs, variable) => {
                write_binary_instruction(f, "Sub", lhs, rhs, *variable)?;
            }
            Self::Mul(lhs, rhs, variable) => {
                write_binary_instruction(f, "Mul", lhs, rhs, *variable)?;
            }
            Self::Sdiv(lhs, rhs, variable) => {
                write_binary_instruction(f, "Sdiv", lhs, rhs, *variable)?;
            }
            Self::LogicalNot(value, variable) => {
                write_unary_instruction(f, "LogicalNot", value, *variable)?;
            }
            Self::LogicalAnd(lhs, rhs, variable) => {
                write_binary_instruction(f, "LogicalAnd", lhs, rhs, *variable)?;
            }
            Self::LogicalOr(lhs, rhs, variable) => {
                write_binary_instruction(f, "LogicalOr", lhs, rhs, *variable)?;
            }
            Self::BitwiseNot(value, variable) => {
                write_unary_instruction(f, "BitwiseNot", value, *variable)?;
            }
            Self::BitwiseAnd(lhs, rhs, variable) => {
                write_binary_instruction(f, "BitwiseAnd", lhs, rhs, *variable)?;
            }
            Self::BitwiseOr(lhs, rhs, variable) => {
                write_binary_instruction(f, "BitwiseOr", lhs, rhs, *variable)?;
            }
            Self::BitwiseXor(lhs, rhs, variable) => {
                write_binary_instruction(f, "BitwiseXor", lhs, rhs, *variable)?;
            }
            Self::Srem(lhs, rhs, variable) => {
                write_binary_instruction(f, "Srem", lhs, rhs, *variable)?;
            }
            Self::Shl(lhs, rhs, variable) => {
                write_binary_instruction(f, "Shl", lhs, rhs, *variable)?;
            }
            Self::Ashr(lhs, rhs, variable) => {
                write_binary_instruction(f, "Ashr", lhs, rhs, *variable)?;
            }
            Self::Fadd(lhs, rhs, variable) => {
                write_binary_instruction(f, "Fadd", lhs, rhs, *variable)?;
            }
            Self::Fsub(lhs, rhs, variable) => {
                write_binary_instruction(f, "Fsub", lhs, rhs, *variable)?;
            }
            Self::Fmul(lhs, rhs, variable) => {
                write_binary_instruction(f, "Fmul", lhs, rhs, *variable)?;
            }
            Self::Fdiv(lhs, rhs, variable) => {
                write_binary_instruction(f, "Fdiv", lhs, rhs, *variable)?;
            }
            Self::Fcmp(op, lhs, rhs, variable) => {
                write_fcmp_instruction(f, *op, lhs, rhs, *variable)?;
            }
            Self::Icmp(op, lhs, rhs, variable) => {
                write_icmp_instruction(f, *op, lhs, rhs, *variable)?;
            }
            Self::Phi(args, variable) => {
                write_phi_instruction(f, args, *variable)?;
            }
            Self::Return => write!(f, "Return")?,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct VariableId(pub u32);

impl VariableId {
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl From<VariableId> for usize {
    fn from(id: VariableId) -> usize {
        id.0 as usize
    }
}

impl From<usize> for VariableId {
    fn from(id: usize) -> Self {
        Self(id.try_into().expect("variable id should fit into u32"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variable {
    pub variable_id: VariableId,
    pub ty: Ty,
}

impl Display for Variable {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Variable({}, {})", self.variable_id.0, self.ty)?;
        Ok(())
    }
}

impl Variable {
    #[must_use]
    pub fn new_boolean(id: VariableId) -> Self {
        Self {
            variable_id: id,
            ty: Ty::Boolean,
        }
    }

    #[must_use]
    pub fn new_integer(id: VariableId) -> Self {
        Self {
            variable_id: id,
            ty: Ty::Integer,
        }
    }

    #[must_use]
    pub fn new_double(id: VariableId) -> Self {
        Self {
            variable_id: id,
            ty: Ty::Double,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Qubit,
    Result,
    Boolean,
    Integer,
    Double,
    Pointer,
}

impl Display for Ty {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::Qubit => write!(f, "Qubit")?,
            Self::Result => write!(f, "Result")?,
            Self::Boolean => write!(f, "Boolean")?,
            Self::Integer => write!(f, "Integer")?,
            Self::Double => write!(f, "Double")?,
            Self::Pointer => write!(f, "Pointer")?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Literal(Literal),
    Variable(Variable),
}

impl Display for Operand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Literal(literal) => write!(f, "{literal}"),
            Self::Variable(variable) => write!(f, "{variable}"),
        }
    }
}

impl Operand {
    #[must_use]
    pub fn get_type(&self) -> Ty {
        match self {
            Operand::Literal(lit) => match lit {
                Literal::Qubit(_) => Ty::Qubit,
                Literal::Result(_) => Ty::Result,
                Literal::Bool(_) => Ty::Boolean,
                Literal::Integer(_) => Ty::Integer,
                Literal::Double(_) => Ty::Double,
                Literal::Pointer | Literal::Tag(..) | Literal::EmptyTag => Ty::Pointer,
            },
            Operand::Variable(var) => var.ty,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Literal {
    Qubit(u32),
    Result(u32),
    Bool(bool),
    Integer(i64),
    Double(f64),
    Tag(usize, usize),
    EmptyTag,
    Pointer,
}

impl Display for Literal {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::Qubit(id) => write!(f, "Qubit({id})")?,
            Self::Result(id) => write!(f, "Result({id})")?,
            Self::Bool(b) => write!(f, "Bool({b})")?,
            Self::Integer(i) => write!(f, "Integer({i})")?,
            Self::Double(d) => write!(f, "Double({d})")?,
            Self::Tag(idx, len) => write!(f, "Tag({idx}, {len})")?,
            Self::EmptyTag => write!(f, "EmptyTag")?,
            Self::Pointer => write!(f, "Pointer")?,
        }
        Ok(())
    }
}

// The `PartialEq` and `Eq` traits are explicitly implemented for literals to allow assertions on instructions where we
// might need to compare floating point values.
impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Bool(self_bool) => {
                if let Self::Bool(other_bool) = other {
                    self_bool == other_bool
                } else {
                    false
                }
            }
            Self::Double(self_double) => {
                if let Self::Double(other_double) = other {
                    (self_double - other_double).abs() < f64::EPSILON
                } else {
                    false
                }
            }
            Self::Integer(self_integer) => {
                if let Self::Integer(other_integer) = other {
                    self_integer == other_integer
                } else {
                    false
                }
            }
            Self::Pointer => matches!(other, Self::Pointer),
            Self::Qubit(self_qubit) => {
                if let Self::Qubit(other_qubit) = other {
                    self_qubit == other_qubit
                } else {
                    false
                }
            }
            Self::Result(self_result) => {
                if let Self::Result(other_result) = other {
                    self_result == other_result
                } else {
                    false
                }
            }
            Self::Tag(self_tag_idx, self_tag_len) => {
                if let Self::Tag(other_tag_idx, other_tag_len) = other {
                    self_tag_idx == other_tag_idx && self_tag_len == other_tag_len
                } else {
                    false
                }
            }
            Self::EmptyTag => *other == Self::EmptyTag,
        }
    }
}

impl Eq for Literal {}

fn set_indentation<'a, 'b>(
    indent: Indented<'a, Formatter<'b>>,
    level: usize,
) -> Indented<'a, Formatter<'b>> {
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        _ => unimplemented!("indentation level not supported"),
    }
}
