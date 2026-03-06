// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    fmt::{self, Display, Formatter},
    sync::Arc,
};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Complex {
    pub real: f64,
    pub imaginary: f64,
}

impl Complex {
    pub fn new(real: f64, imaginary: f64) -> Self {
        Self { real, imaginary }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Type {
    Angle,
    Bool,
    BigInt,
    Complex,
    Int,
    Double,
    Qubit,
    Result,
    Tuple(Vec<Type>),
    Range,
    BoolArray(ArrayDimensions),
    BigIntArray(ArrayDimensions),
    IntArray(ArrayDimensions),
    DoubleArray(ArrayDimensions),
    ComplexArray(ArrayDimensions),
    AngleArray(ArrayDimensions),
    QubitArray(ArrayDimensions),
    ResultArray(ArrayDimensions),
    /// # cargs, # qargs
    Gate(u32, u32),
    /// kind, args, return ty
    Callable(CallableKind, Arc<[Type]>, Arc<Type>),
    #[default]
    Err,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallableKind {
    /// A function.
    Function,
    /// An operation.
    Operation,
}

impl Display for CallableKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CallableKind::Function => write!(f, "Function"),
            CallableKind::Operation => write!(f, "Operation"),
        }
    }
}

/// QASM supports up to seven dimensions.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ArrayDimensions {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
}

impl From<ArrayDimensions> for u32 {
    fn from(value: ArrayDimensions) -> Self {
        value as u32
    }
}

impl From<u32> for ArrayDimensions {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            4 => Self::Four,
            5 => Self::Five,
            6 => Self::Six,
            7 => Self::Seven,
            _ => unreachable!("we validate that num_dims is between 1 and 7 when generating them"),
        }
    }
}

impl From<&qsc_openqasm_parser::semantic::types::ArrayDimensions> for ArrayDimensions {
    fn from(value: &qsc_openqasm_parser::semantic::types::ArrayDimensions) -> Self {
        match value {
            qsc_openqasm_parser::semantic::types::ArrayDimensions::One(..) => Self::One,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Two(..) => Self::Two,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Three(..) => Self::Three,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Four(..) => Self::Four,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Five(..) => Self::Five,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Six(..) => Self::Six,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Seven(..) => Self::Seven,
            qsc_openqasm_parser::semantic::types::ArrayDimensions::Err => {
                unimplemented!("Array dimensions greater than seven are not supported.")
            }
        }
    }
}

impl From<qsc_openqasm_parser::semantic::types::Dims> for ArrayDimensions {
    fn from(value: qsc_openqasm_parser::semantic::types::Dims) -> Self {
        match value {
            qsc_openqasm_parser::semantic::types::Dims::One => Self::One,
            qsc_openqasm_parser::semantic::types::Dims::Two => Self::Two,
            qsc_openqasm_parser::semantic::types::Dims::Three => Self::Three,
            qsc_openqasm_parser::semantic::types::Dims::Four => Self::Four,
            qsc_openqasm_parser::semantic::types::Dims::Five => Self::Five,
            qsc_openqasm_parser::semantic::types::Dims::Six => Self::Six,
            qsc_openqasm_parser::semantic::types::Dims::Seven => Self::Seven,
            qsc_openqasm_parser::semantic::types::Dims::Err => {
                unimplemented!("Array dimensions greater than seven are not supported.")
            }
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Type::Angle => write!(f, "Angle"),
            Type::Bool => write!(f, "bool"),
            Type::BigInt => write!(f, "BigInt"),
            Type::Complex => write!(f, "Complex"),
            Type::Int => write!(f, "Int"),
            Type::Double => write!(f, "Double"),
            Type::Qubit => write!(f, "Qubit"),
            Type::Range => write!(f, "Range"),
            Type::Result => write!(f, "Result"),
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ")")
            }
            Type::BoolArray(dim) => write!(f, "bool{dim}"),
            Type::BigIntArray(dim) => write!(f, "BigInt{dim}"),
            Type::IntArray(dim) => write!(f, "Int{dim}"),
            Type::DoubleArray(dim) => write!(f, "Double{dim}"),
            Type::ComplexArray(dim) => write!(f, "Complex{dim}"),
            Type::AngleArray(dim) => write!(f, "Angle{dim}"),
            Type::QubitArray(dim) => write!(f, "Qubit{dim}"),
            Type::ResultArray(dim) => write!(f, "Result{dim}"),
            Type::Callable(kind, args, return_type) => {
                write!(f, "Callable({kind}, {args:?}, {return_type})")
            }
            Type::Gate(cargs, qargs) => {
                write!(f, "Gate({cargs}, {qargs})")
            }
            Type::Err => write!(f, "Err"),
        }
    }
}

impl Display for ArrayDimensions {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::One => write!(f, "[]"),
            Self::Two => write!(f, "[][]"),
            Self::Three => write!(f, "[][][]"),
            Self::Four => write!(f, "[][][][]"),
            Self::Five => write!(f, "[][][][][]"),
            Self::Six => write!(f, "[][][][][][]"),
            Self::Seven => write!(f, "[][][][][][][]"),
        }
    }
}
