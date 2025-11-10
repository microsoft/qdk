// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc_data_structures::span::Span;

use super::ast::{BinOp, Expr, Index, LiteralKind, Range};
use crate::parser::ast as syntax;
use crate::semantic::{Lowerer, SemanticErrorKind};
use core::fmt;
use std::fmt::{Display, Formatter};
use std::{cmp::max, sync::Arc};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum Type {
    // scalar types
    Bit(bool),
    Bool(bool),
    // Duration is always const, but we keep the bool to match the other types
    Duration(bool),
    Stretch(bool),

    Angle(Option<u32>, bool),
    Complex(Option<u32>, bool),
    Float(Option<u32>, bool),
    Int(Option<u32>, bool),
    UInt(Option<u32>, bool),

    // quantum
    Qubit,
    HardwareQubit,

    // magic arrays
    BitArray(u32, bool),
    QubitArray(u32),

    // proper arrays
    Array(ArrayType),

    /// Dynamic array references.
    /// These are array references declared with the `#dim = const expr` syntax.
    /// E.g.: `readonly array[int, #dim = 3] arr`.
    DynArrayRef(DynArrayRefType),

    /// Static array references.
    /// These are array references where all dimension lengths are declared explicitly.
    /// E.g.: `readonly array[int, 2, 3, 4] arr`.
    StaticArrayRef(StaticArrayRefType),

    // realistically the sizes could be u3
    Gate(u32, u32),
    Function(Arc<[Type]>, Arc<Type>),
    Range,
    Set,
    Void,
    #[default]
    Err,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ArrayBaseType {
    Bool,
    Duration,
    Angle(Option<u32>),
    Complex(Option<u32>),
    Float(Option<u32>),
    Int(Option<u32>),
    UInt(Option<u32>),
}

impl Display for ArrayBaseType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let ty: Type = self.clone().into();
        write!(f, "{ty}")
    }
}

impl From<ArrayBaseType> for Type {
    fn from(value: ArrayBaseType) -> Self {
        match value {
            ArrayBaseType::Bool => Self::Bool(false),
            ArrayBaseType::Duration => Self::Duration(false),
            ArrayBaseType::Angle(width) => Self::Angle(width, false),
            ArrayBaseType::Complex(width) => Self::Complex(width, false),
            ArrayBaseType::Float(width) => Self::Float(width, false),
            ArrayBaseType::Int(width) => Self::Int(width, false),
            ArrayBaseType::UInt(width) => Self::UInt(width, false),
        }
    }
}

impl TryFrom<Type> for ArrayBaseType {
    type Error = ();

    fn try_from(value: Type) -> Result<Self, ()> {
        match value {
            Type::Bool(false) => Ok(Self::Bool),
            Type::Duration(false) => Ok(Self::Duration),
            Type::Angle(width, false) => Ok(Self::Angle(width)),
            Type::Complex(width, false) => Ok(Self::Complex(width)),
            Type::Float(width, false) => Ok(Self::Float(width)),
            Type::Int(width, false) => Ok(Self::Int(width)),
            Type::UInt(width, false) => Ok(Self::UInt(width)),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArrayType {
    pub base_ty: ArrayBaseType,
    pub dims: ArrayDimensions,
}

impl Display for ArrayType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let base_ty: Type = self.base_ty.clone().into();
        write!(f, "array[{base_ty}, ")?;
        write_array_dimensions(f, &self.dims)?;
        write!(f, "]")
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StaticArrayRefType {
    pub base_ty: ArrayBaseType,
    pub dims: ArrayDimensions,
    pub is_mutable: bool,
}

impl Display for StaticArrayRefType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_mutable {
            write!(f, "mutable ")?;
        } else {
            write!(f, "readonly ")?;
        }

        let base_ty: Type = self.base_ty.clone().into();
        write!(f, "array[{base_ty}, ")?;
        write_array_dimensions(f, &self.dims)?;
        write!(f, "]")
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DynArrayRefType {
    pub base_ty: ArrayBaseType,
    pub dims: Dims,
    pub is_mutable: bool,
}

impl Display for DynArrayRefType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_mutable {
            write!(f, "mutable ")?;
        } else {
            write!(f, "readonly ")?;
        }

        let base_ty: Type = self.base_ty.clone().into();
        write!(f, "array[{}, #dim = {}]", base_ty, self.dims)
    }
}

fn write_always_const_ty_omitting_mutabilty(f: &mut Formatter<'_>, name: &str) -> std::fmt::Result {
    write_ty_with_designator_and_const(f, false, None, name)
}

fn write_ty_with_const(f: &mut Formatter<'_>, is_const: bool, name: &str) -> std::fmt::Result {
    write_ty_with_designator_and_const(f, is_const, None, name)
}

fn write_ty_with_designator(
    f: &mut Formatter<'_>,
    width: Option<u32>,
    name: &str,
) -> std::fmt::Result {
    write_ty_with_designator_and_const(f, false, width, name)
}

fn write_ty_with_designator_and_const(
    f: &mut Formatter<'_>,
    is_const: bool,
    width: Option<u32>,
    name: &str,
) -> std::fmt::Result {
    if is_const {
        write!(f, "const ")?;
    }
    if let Some(width) = width {
        write!(f, "{name}[{width}]")
    } else {
        write!(f, "{name}")
    }
}

fn write_complex_ty(f: &mut Formatter<'_>, is_const: bool, width: Option<u32>) -> std::fmt::Result {
    if is_const {
        write!(f, "const ")?;
    }
    if let Some(width) = width {
        write!(f, "complex[float[{width}]]")
    } else {
        write!(f, "complex[float]")
    }
}

fn write_array_dimensions(f: &mut Formatter<'_>, dims: &ArrayDimensions) -> std::fmt::Result {
    match dims {
        ArrayDimensions::One(one) => write!(f, "{one}"),
        ArrayDimensions::Two(one, two) => write!(f, "{one}, {two}"),
        ArrayDimensions::Three(one, two, three) => write!(f, "{one}, {two}, {three}"),
        ArrayDimensions::Four(one, two, three, four) => {
            write!(f, "{one}, {two}, {three}, {four}")
        }
        ArrayDimensions::Five(one, two, three, four, five) => {
            write!(f, "{one}, {two}, {three}, {four}, {five}")
        }
        ArrayDimensions::Six(one, two, three, four, five, six) => {
            write!(f, "{one}, {two}, {three}, {four}, {five}, {six}")
        }
        ArrayDimensions::Seven(one, two, three, four, five, six, seven) => {
            write!(f, "{one}, {two}, {three}, {four}, {five}, {six}, {seven}")
        }
        ArrayDimensions::Err => write!(f, "unknown"),
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Bit(is_const) => write_ty_with_const(f, *is_const, "bit"),
            Type::Bool(is_const) => write_ty_with_const(f, *is_const, "bool"),
            Type::Duration(is_const) => write_ty_with_const(f, *is_const, "duration"),
            Type::Stretch(_) => write_always_const_ty_omitting_mutabilty(f, "stretch"),
            Type::Angle(width, is_const) => {
                write_ty_with_designator_and_const(f, *is_const, *width, "angle")
            }
            Type::Complex(width, is_const) => write_complex_ty(f, *is_const, *width),
            Type::Float(width, is_const) => {
                write_ty_with_designator_and_const(f, *is_const, *width, "float")
            }
            Type::Int(width, is_const) => {
                write_ty_with_designator_and_const(f, *is_const, *width, "int")
            }
            Type::UInt(width, is_const) => {
                write_ty_with_designator_and_const(f, *is_const, *width, "uint")
            }
            Type::Qubit => write!(f, "qubit"),
            Type::HardwareQubit => write!(f, "hardware qubit"),
            Type::BitArray(width, is_const) => {
                write_ty_with_designator_and_const(f, *is_const, Some(*width), "bit")
            }
            Type::QubitArray(width) => write_ty_with_designator(f, Some(*width), "qubit"),
            Type::Array(array) => write!(f, "{array}"),
            Type::StaticArrayRef(array_ref) => write!(f, "{array_ref}"),
            Type::DynArrayRef(array_ref) => write!(f, "{array_ref}"),
            Type::Gate(cargs, qargs) => write!(f, "gate({cargs}, {qargs})"),
            Type::Function(params_ty, return_ty) => {
                let params_ty_str = params_ty
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "def ({params_ty_str}) -> {return_ty}")
            }
            Type::Range => write!(f, "range"),
            Type::Set => write!(f, "set"),
            Type::Void => write!(f, "void"),
            Type::Err => write!(f, "unknown"),
        }
    }
}

impl Type {
    pub(crate) fn is_array(&self) -> bool {
        matches!(
            self,
            Type::BitArray(..)
                | Type::QubitArray(..)
                | Type::Array(..)
                | Type::DynArrayRef(..)
                | Type::StaticArrayRef(..)
        )
    }

    pub(crate) fn array_dims(&self) -> Option<ArrayDimensions> {
        match self {
            Self::Array(array) => Some(array.dims.clone()),
            Self::StaticArrayRef(array) => Some(array.dims.clone()),
            _ => None,
        }
    }

    pub(crate) fn has_zero_size(&self) -> bool {
        match self {
            Type::BitArray(size, _) | Type::QubitArray(size) => *size == 0,
            Type::Array(array) => array
                .dims
                .clone()
                .into_iter()
                .any(|dim_length| dim_length == 0),
            Type::StaticArrayRef(array) => array
                .dims
                .clone()
                .into_iter()
                .any(|dim_length| dim_length == 0),
            _ => false,
        }
    }

    pub(crate) fn make_array_ty(dims: &[u32], base_ty: &Self) -> Self {
        let dims = dims.into();

        if let Ok(base_ty) = base_ty.clone().try_into() {
            Self::Array(ArrayType { base_ty, dims })
        } else {
            Self::Err
        }
    }

    pub(crate) fn make_static_array_ref_ty(dims: &[u32], base_ty: &Self, is_mutable: bool) -> Self {
        let dims = dims.into();

        if let Ok(base_ty) = base_ty.clone().try_into() {
            Self::StaticArrayRef(StaticArrayRefType {
                base_ty,
                dims,
                is_mutable,
            })
        } else {
            Self::Err
        }
    }

    pub(crate) fn make_dyn_array_ref_ty(num_dims: Dims, base_ty: &Self, is_mutable: bool) -> Self {
        if let Ok(base_ty) = base_ty.clone().try_into() {
            Self::DynArrayRef(DynArrayRefType {
                base_ty,
                dims: num_dims,
                is_mutable,
            })
        } else {
            Self::Err
        }
    }

    #[must_use]
    pub fn is_const(&self) -> bool {
        match self {
            Type::BitArray(_, is_const)
            | Type::Bit(is_const)
            | Type::Bool(is_const)
            | Type::Duration(is_const)
            | Type::Angle(_, is_const)
            | Type::Complex(_, is_const)
            | Type::Float(_, is_const)
            | Type::Int(_, is_const)
            | Type::UInt(_, is_const) => *is_const,
            Type::Stretch(_) => true,
            _ => false,
        }
    }

    #[must_use]
    pub fn is_readonly_array_ref(&self) -> bool {
        match self {
            Type::StaticArrayRef(array_ref) => !array_ref.is_mutable,
            Type::DynArrayRef(array_ref) => !array_ref.is_mutable,
            _ => false,
        }
    }

    #[must_use]
    pub fn is_err(&self) -> bool {
        matches!(self, Type::Err)
    }

    #[must_use]
    pub fn width(&self) -> Option<u32> {
        match self {
            Type::Angle(w, _)
            | Type::Complex(w, _)
            | Type::Float(w, _)
            | Type::Int(w, _)
            | Type::UInt(w, _) => *w,
            _ => None,
        }
    }

    #[must_use]
    pub fn is_inferred_output_type(&self) -> bool {
        match self {
            Type::Bit(_)
            | Type::Int(..)
            | Type::UInt(..)
            | Type::Float(..)
            | Type::Angle(..)
            | Type::Complex(..)
            | Type::Bool(..)
            | Type::BitArray(..) => true,
            Type::Array(array) => {
                Into::<Self>::into(array.base_ty.clone()).is_inferred_output_type()
            }
            _ => false,
        }
    }

    /// Get the indexed type of a type given a list of indices.
    /// For example, if the type is `Int[2][3]`, the indexed type is `Int[2]`.
    /// If the type is `Int[2]`, the indexed type is `Int`.
    /// If the type is `Int`, the indexed type is `None`.
    ///
    /// This is useful for determining the type of an array element.
    pub(crate) fn get_indexed_type(
        &self,
        ctx: &mut Lowerer,
        index: &super::ast::Index,
        span: Span,
    ) -> Self {
        match self {
            Type::Angle(Some(size), constness)
            | Type::Int(Some(size), constness)
            | Type::UInt(Some(size), constness)
            | Type::BitArray(size, constness) => indexed_type_builder(
                ctx,
                || Type::Bit(*constness),
                |d| {
                    let ArrayDimensions::One(size) = d else {
                        unreachable!("dims was hardcoded to have one dimension")
                    };
                    Type::BitArray(size, *constness)
                },
                &ArrayDimensions::One(*size),
                index,
            ),
            Type::QubitArray(size) => indexed_type_builder(
                ctx,
                || Type::Qubit,
                |d| {
                    let ArrayDimensions::One(size) = d else {
                        unreachable!("dims was hardcoded to have one dimension")
                    };
                    Type::QubitArray(size)
                },
                &ArrayDimensions::One(*size),
                index,
            ),
            Type::Array(array) => indexed_type_builder(
                ctx,
                || array.base_ty.clone().into(),
                |dims| {
                    Type::Array(ArrayType {
                        base_ty: array.base_ty.clone(),
                        dims: dims.clone(),
                    })
                },
                &array.dims,
                index,
            ),
            Type::StaticArrayRef(array) => indexed_type_builder(
                ctx,
                || array.base_ty.clone().into(),
                |dims| {
                    Type::StaticArrayRef(StaticArrayRefType {
                        base_ty: array.base_ty.clone(),
                        dims: dims.clone(),
                        is_mutable: array.is_mutable,
                    })
                },
                &array.dims,
                index,
            ),
            Type::DynArrayRef(array) => {
                // In this case we only care about the number of dimensions and not about
                // the size of the dimensions. So, we create a dummy `ArrayDimensions`
                // encoding the num_dims to be able to use the same infrastructure we
                // use for the other array types.
                let dummy_dims: ArrayDimensions = array.dims.into();
                indexed_type_builder(
                    ctx,
                    || array.base_ty.clone().into(),
                    |dims| {
                        Type::DynArrayRef(DynArrayRefType {
                            base_ty: array.base_ty.clone(),
                            dims: dims.into(),
                            is_mutable: array.is_mutable,
                        })
                    },
                    &dummy_dims,
                    index,
                )
            }
            _ => {
                let kind = super::error::SemanticErrorKind::CannotIndexType(self.to_string(), span);
                ctx.push_semantic_error(kind);
                super::types::Type::Err
            }
        }
    }

    pub(crate) fn as_const(&self) -> Type {
        match self {
            Type::Bit(_) => Self::Bit(true),
            Type::Bool(_) => Self::Bool(true),
            Type::Angle(w, _) => Self::Angle(*w, true),
            Type::Complex(w, _) => Self::Complex(*w, true),
            Type::Float(w, _) => Self::Float(*w, true),
            Type::Int(w, _) => Self::Int(*w, true),
            Type::UInt(w, _) => Self::UInt(*w, true),
            Type::BitArray(size, _) => Self::BitArray(*size, true),
            _ => self.clone(),
        }
    }

    pub(crate) fn as_non_const(&self) -> Type {
        match self {
            Type::Bit(_) => Self::Bit(false),
            Type::Bool(_) => Self::Bool(false),
            Type::Angle(w, _) => Self::Angle(*w, false),
            Type::Complex(w, _) => Self::Complex(*w, false),
            Type::Float(w, _) => Self::Float(*w, false),
            Type::Int(w, _) => Self::Int(*w, false),
            Type::UInt(w, _) => Self::UInt(*w, false),
            Type::BitArray(size, _) => Self::BitArray(*size, false),
            _ => self.clone(),
        }
    }

    pub(crate) fn is_quantum(&self) -> bool {
        matches!(
            self,
            Type::HardwareQubit | Type::Qubit | Type::QubitArray(_)
        )
    }
}

/// This function builds the indexed type of a given type.
///
/// Its first argument is a function that builds the base type of an array,
/// which is only used if the array is fully indexed, and the result of the
/// indexing operation is a scalar type.
///
/// Its second argument is a function that builds the array type given the
/// new dims of the array after indexing, and the base type of the array.
///
/// Finally it takes the dimensions of the array being indexed and the indices
/// used to index it. The function returns the type of the result after indexing.
fn indexed_type_builder(
    ctx: &mut Lowerer,
    base_ty_builder: impl Fn() -> Type,
    array_ty_builder: impl Fn(ArrayDimensions) -> Type,
    dims: &ArrayDimensions,
    index: &super::ast::Index,
) -> Type {
    if matches!(dims, ArrayDimensions::Err) {
        return Type::Err;
    }

    let mut dims = dims.clone().into_iter();
    let first_dim = dims.next().expect("there is at least one dimension");

    let dims_vec = match index {
        Index::Expr(..) => dims.collect::<Vec<_>>(),

        // If we have a range we need to compute the size of the slice
        Index::Range(range) => {
            if let Some(slice_size) = compute_slice_size(ctx, range, first_dim) {
                [slice_size].into_iter().chain(dims).collect::<Vec<_>>()
            } else {
                return Type::Err;
            }
        }
    };

    if dims_vec.is_empty() {
        base_ty_builder()
    } else {
        array_ty_builder((&dims_vec[..]).into())
    }
}

/// The spec says: "Indexing of arrays is n-based i.e., negative indices are allowed."
/// <https://openqasm.com/language/types.html#arrays>
///
/// We interpret this as allowing indexing in the range [-N, N-1], where N is the size
/// of the array, which matches Python and Q# behavior.
pub(crate) fn wrap_index_value(
    ctx: &mut Lowerer,
    idx: i64,
    dimension_size: i64,
    span: Span,
) -> Option<i64> {
    if (-dimension_size..dimension_size).contains(&idx) {
        Some(if idx < 0 { idx + dimension_size } else { idx })
    } else {
        ctx.push_semantic_error(SemanticErrorKind::IndexOutOfBounds(
            -dimension_size,
            dimension_size - 1,
            idx,
            span,
        ));
        None
    }
}

/// Computes the slice's start, step, and end.
pub(crate) fn compute_slice_components(
    ctx: &mut Lowerer,
    range: &Range,
    dimension_size: u32,
) -> Option<(i64, i64, i64)> {
    // Helper function to extract the literal value from the start,
    // step, and end components of the range. These expressions are
    // guaranteed to be of literals of type `int` by this point.
    let unwrap_lit_or_default = |expr: Option<&Expr>, default: i64| {
        if let Some(expr) = expr {
            // slices must be const int exprs. If we failed to const eval the range,
            // the lowering of the range would have failed. Since we are here,
            // we know lowering succeeded and the const value is available.
            // If this is ever false, we have a compiler bug.
            if let Some(LiteralKind::Int(val)) = expr.get_const_value() {
                val
            } else {
                unreachable!("range components are guaranteed to be int literals")
            }
        } else {
            default
        }
    };
    let start = unwrap_lit_or_default(range.start.as_ref(), 0);
    let step = unwrap_lit_or_default(range.step.as_ref(), 1);
    let end = unwrap_lit_or_default(range.end.as_ref(), i64::from(dimension_size) - 1);

    let start = wrap_index_value(ctx, start, i64::from(dimension_size), range.span)?;
    let end = wrap_index_value(ctx, end, i64::from(dimension_size), range.span)?;

    Some((start, step, end))
}

/// This function returns `None` if the range step is zero.
fn compute_slice_size(ctx: &mut Lowerer, range: &Range, dimension_size: u32) -> Option<u32> {
    // If the dimension is zero, the slice will also have size zero.
    // If the dimension size is zero, the slice will always be empty.
    // So we return Some(0) as the size of the slice.
    if dimension_size == 0 {
        return Some(0);
    }

    let (start, step, end) = compute_slice_components(ctx, range, dimension_size)?;

    // <https://openqasm.com/language/types.html#register-concatenation-and-slicing>
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // The range corresponds to the set
    // {start, start + 1 * step, start + 2 * step, ..., start + m * step}
    //   (Note that the range has m + 1 elements)
    // where m is the largest integer such that:
    //
    // if step > 0,
    //   start + m * step <= end
    //
    // solving for m we have,
    //   m * step <= (end - start)
    //   m <= (end - start) / step
    //
    // the largest integer m satisfying this inequality is,
    //   m = floor((end - start) / step)
    //
    // when start <= end. Since rust's integer division matches
    // this behavior we don't need to take the floor.
    // When start > end, the slice is empty and m = 0.
    //
    // --
    //
    // If the step < 0,
    //   start + m * step >= end
    //
    // solving for m we have,
    //   m * step >= end - start

    // since step is negative, when we divide both sides
    // of the inequality by it, the inequality sign changes.
    //   m <= (end - start) / step
    //
    // Note that we get the same expression that in the case
    // when step > 0, however here is expected that end <= start.
    let slice_size = if step > 0 {
        if start <= end {
            ((end - start) / step) as u32 + 1
        } else {
            0
        }
    } else if end <= start {
        ((end - start) / step) as u32 + 1
    } else {
        0
    };

    Some(slice_size)
}

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq)]
pub enum ArrayDimensions {
    One(u32),
    Two(u32, u32),
    Three(u32, u32, u32),
    Four(u32, u32, u32, u32),
    Five(u32, u32, u32, u32, u32),
    Six(u32, u32, u32, u32, u32, u32),
    Seven(u32, u32, u32, u32, u32, u32, u32),
    #[default]
    Err,
}

impl From<u32> for ArrayDimensions {
    fn from(value: u32) -> Self {
        Self::One(value)
    }
}

impl Display for ArrayDimensions {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ArrayDimensions::One(..) => write!(f, "[]"),
            ArrayDimensions::Two(..) => write!(f, "[][]"),
            ArrayDimensions::Three(..) => write!(f, "[][][]"),
            ArrayDimensions::Four(..) => write!(f, "[][][][]"),
            ArrayDimensions::Five(..) => write!(f, "[][][][][]"),
            ArrayDimensions::Six(..) => write!(f, "[][][][][][]"),
            ArrayDimensions::Seven(..) => write!(f, "[][][][][][][]"),
            ArrayDimensions::Err => write!(f, "Invalid array dimensions"),
        }
    }
}

impl ArrayDimensions {
    #[must_use]
    pub fn num_dims(&self) -> u32 {
        match self {
            ArrayDimensions::One(_) => 1,
            ArrayDimensions::Two(_, _) => 2,
            ArrayDimensions::Three(_, _, _) => 3,
            ArrayDimensions::Four(_, _, _, _) => 4,
            ArrayDimensions::Five(_, _, _, _, _) => 5,
            ArrayDimensions::Six(_, _, _, _, _, _) => 6,
            ArrayDimensions::Seven(_, _, _, _, _, _, _) => 7,
            ArrayDimensions::Err => 0,
        }
    }

    #[must_use]
    pub fn indexed_dim_size(&self) -> Option<u32> {
        match self {
            ArrayDimensions::One(d)
            | ArrayDimensions::Two(d, _)
            | ArrayDimensions::Three(d, _, _)
            | ArrayDimensions::Four(d, _, _, _)
            | ArrayDimensions::Five(d, _, _, _, _)
            | ArrayDimensions::Six(d, _, _, _, _, _)
            | ArrayDimensions::Seven(d, _, _, _, _, _, _) => Some(*d),
            ArrayDimensions::Err => None,
        }
    }
}

impl IntoIterator for ArrayDimensions {
    type Item = u32;
    type IntoIter = std::vec::IntoIter<u32>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ArrayDimensions::One(d1) => vec![d1].into_iter(),
            ArrayDimensions::Two(d1, d2) => vec![d1, d2].into_iter(),
            ArrayDimensions::Three(d1, d2, d3) => vec![d1, d2, d3].into_iter(),
            ArrayDimensions::Four(d1, d2, d3, d4) => vec![d1, d2, d3, d4].into_iter(),
            ArrayDimensions::Five(d1, d2, d3, d4, d5) => vec![d1, d2, d3, d4, d5].into_iter(),
            ArrayDimensions::Six(d1, d2, d3, d4, d5, d6) => {
                vec![d1, d2, d3, d4, d5, d6].into_iter()
            }
            ArrayDimensions::Seven(d1, d2, d3, d4, d5, d6, d7) => {
                vec![d1, d2, d3, d4, d5, d6, d7].into_iter()
            }
            ArrayDimensions::Err => vec![].into_iter(),
        }
    }
}

impl From<&[u32]> for ArrayDimensions {
    fn from(dims: &[u32]) -> Self {
        let dims = dims.to_vec();
        match dims.len() {
            1 => ArrayDimensions::One(dims[0]),
            2 => ArrayDimensions::Two(dims[0], dims[1]),
            3 => ArrayDimensions::Three(dims[0], dims[1], dims[2]),
            4 => ArrayDimensions::Four(dims[0], dims[1], dims[2], dims[3]),
            5 => ArrayDimensions::Five(dims[0], dims[1], dims[2], dims[3], dims[4]),
            6 => ArrayDimensions::Six(dims[0], dims[1], dims[2], dims[3], dims[4], dims[5]),
            7 => ArrayDimensions::Seven(
                dims[0], dims[1], dims[2], dims[3], dims[4], dims[5], dims[6],
            ),
            _ => ArrayDimensions::Err,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Dims {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Err = 0,
}

impl Display for Dims {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Dims::One => write!(f, "1"),
            Dims::Two => write!(f, "2"),
            Dims::Three => write!(f, "3"),
            Dims::Four => write!(f, "4"),
            Dims::Five => write!(f, "5"),
            Dims::Six => write!(f, "6"),
            Dims::Seven => write!(f, "7"),
            Dims::Err => write!(f, "Err"),
        }
    }
}

impl From<Dims> for u32 {
    fn from(value: Dims) -> Self {
        value as u32
    }
}

impl From<u32> for Dims {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            4 => Self::Four,
            5 => Self::Five,
            6 => Self::Six,
            7 => Self::Seven,
            _ => Self::Err,
        }
    }
}

impl From<ArrayDimensions> for Dims {
    fn from(value: ArrayDimensions) -> Self {
        match value {
            ArrayDimensions::One(..) => Self::One,
            ArrayDimensions::Two(..) => Self::Two,
            ArrayDimensions::Three(..) => Self::Three,
            ArrayDimensions::Four(..) => Self::Four,
            ArrayDimensions::Five(..) => Self::Five,
            ArrayDimensions::Six(..) => Self::Six,
            ArrayDimensions::Seven(..) => Self::Seven,
            ArrayDimensions::Err => Self::Err,
        }
    }
}

impl From<Dims> for ArrayDimensions {
    /// This implementation is only meant to be used as a helper method
    /// for [`Type::get_indexed_type`].
    fn from(value: Dims) -> Self {
        match value {
            Dims::One => Self::One(0),
            Dims::Two => Self::Two(0, 0),
            Dims::Three => Self::Three(0, 0, 0),
            Dims::Four => Self::Four(0, 0, 0, 0),
            Dims::Five => Self::Five(0, 0, 0, 0, 0),
            Dims::Six => Self::Six(0, 0, 0, 0, 0, 0),
            Dims::Seven => Self::Seven(0, 0, 0, 0, 0, 0, 0),
            Dims::Err => Self::Err,
        }
    }
}

/// When two types are combined, the result is a type that can represent both.
/// For constness, the result is const iff both types are const.
#[must_use]
pub fn relax_constness(lhs_ty: &Type, rhs_ty: &Type) -> bool {
    lhs_ty.is_const() && rhs_ty.is_const()
}

/// Having no width means that the type is not a fixed-width type
/// and can hold any explicit width. If both types have a width,
/// the result is the maximum of the two. Otherwise, the result
/// is a type without a width.
#[must_use]
pub fn promote_width(lhs_ty: &Type, rhs_ty: &Type) -> Option<u32> {
    match (lhs_ty.width(), rhs_ty.width()) {
        (Some(w1), Some(w2)) => Some(max(w1, w2)),
        (Some(_) | None, None) | (None, Some(_)) => None,
    }
}

fn get_effective_width(lhs_ty: &Type, rhs_ty: &Type) -> Option<u32> {
    match (lhs_ty.width(), rhs_ty.width()) {
        (Some(w1), Some(w2)) => Some(max(w1, w2)),
        (Some(w), None) | (None, Some(w)) => Some(w),
        (None, None) => None,
    }
}

/// If both can be promoted to a common type, the result is that type.
/// If the types are not compatible, the result is `Type::Void`.
#[must_use]
pub fn promote_types(lhs_ty: &Type, rhs_ty: &Type) -> Type {
    if *lhs_ty == *rhs_ty {
        return lhs_ty.clone();
    }
    if types_equal_except_const(lhs_ty, rhs_ty) {
        // If one of the types is non-const, we return the type as non-const.
        return lhs_ty.as_non_const();
    }
    let ty = promote_types_symmetric(lhs_ty, rhs_ty);
    if ty != Type::Void {
        return ty;
    }
    let ty = promote_types_asymmetric(lhs_ty, rhs_ty);
    if ty == Type::Void {
        return promote_types_asymmetric(rhs_ty, lhs_ty);
    }
    ty
}

pub(crate) fn promote_to_uint_ty(
    lhs_ty: &Type,
    rhs_ty: &Type,
) -> (Option<Type>, Option<Type>, Option<Type>) {
    let is_const = relax_constness(lhs_ty, rhs_ty);
    let lhs_ty = get_uint_ty(lhs_ty);
    let rhs_ty = get_uint_ty(rhs_ty);
    match (lhs_ty, rhs_ty) {
        (Some(lhs_ty), Some(rhs_ty)) => {
            let width = get_effective_width(&lhs_ty, &rhs_ty);
            (
                Some(Type::UInt(width, is_const)),
                Some(lhs_ty),
                Some(rhs_ty),
            )
        }
        (Some(lhs_ty), None) => (None, Some(lhs_ty), None),
        (None, Some(rhs_ty)) => (None, None, Some(rhs_ty)),
        (None, None) => (None, None, None),
    }
}

fn get_uint_ty(ty: &Type) -> Option<Type> {
    if matches!(ty, Type::Int(..) | Type::UInt(..) | Type::Angle(..)) {
        Some(Type::UInt(ty.width(), ty.is_const()))
    } else if matches!(ty, Type::Bool(..) | Type::Bit(..)) {
        Some(Type::UInt(Some(1), ty.is_const()))
    } else if let Type::BitArray(size, _) = ty {
        Some(Type::UInt(Some(*size), ty.is_const()))
    } else {
        None
    }
}

/// Promotes two types if they share a common base type with
/// their constness relaxed, and their width promoted.
/// If the types are not compatible, the result is `Type::Void`.
fn promote_types_symmetric(lhs_ty: &Type, rhs_ty: &Type) -> Type {
    let is_const = relax_constness(lhs_ty, rhs_ty);
    match (lhs_ty, rhs_ty) {
        (Type::Bit(..), Type::Bit(..)) => Type::Bit(is_const),
        (Type::Bool(..), Type::Bool(..)) => Type::Bool(is_const),
        (Type::Int(..), Type::Int(..)) => Type::Int(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::UInt(..), Type::UInt(..)) => Type::UInt(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::Angle(..), Type::Angle(..)) => Type::Angle(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::Float(..), Type::Float(..)) => Type::Float(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::Complex(..), Type::Complex(..)) => {
            Type::Complex(promote_width(lhs_ty, rhs_ty), is_const)
        }
        _ => Type::Void,
    }
}

/// Promotion follows casting rules. We only match one way, as the
/// both combinations are covered by calling this function twice
/// with the arguments swapped.
///
/// If the types are not compatible, the result is `Type::Void`.
///
/// The left-hand side is the type to promote from, and the right-hand
/// side is the type to promote to. So any promotion goes from lesser
/// type to greater type.
///
/// This is more complicated as we have C99 promotion for simple types,
/// but complex types like `Complex`, and `Angle` don't follow those rules.
fn promote_types_asymmetric(lhs_ty: &Type, rhs_ty: &Type) -> Type {
    let is_const = relax_constness(lhs_ty, rhs_ty);
    #[allow(clippy::match_same_arms)]
    match (lhs_ty, rhs_ty) {
        (Type::Bit(..), Type::Bool(..)) => Type::Bool(is_const),
        (Type::Bit(..), Type::Int(w, _)) => Type::Int(*w, is_const),
        (Type::Bit(..), Type::UInt(w, _)) => Type::UInt(*w, is_const),

        (Type::Bit(..), Type::Angle(w, _)) => Type::Angle(*w, is_const),

        (Type::Bool(..), Type::Int(w, _)) => Type::Int(*w, is_const),
        (Type::Bool(..), Type::UInt(w, _)) => Type::UInt(*w, is_const),
        (Type::Bool(..), Type::Float(w, _)) => Type::Float(*w, is_const),
        (Type::Bool(..), Type::Complex(w, _)) => Type::Complex(*w, is_const),

        (Type::UInt(..), Type::Int(..)) => Type::Int(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::UInt(..), Type::Float(..)) => Type::Float(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::UInt(..), Type::Complex(..)) => {
            Type::Complex(promote_width(lhs_ty, rhs_ty), is_const)
        }

        (Type::Int(..), Type::Float(..)) => Type::Float(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::Int(..), Type::Complex(..)) => {
            Type::Complex(promote_width(lhs_ty, rhs_ty), is_const)
        }
        (Type::Angle(..), Type::Float(..)) => Type::Float(promote_width(lhs_ty, rhs_ty), is_const),
        (Type::Float(..), Type::Complex(..)) => {
            Type::Complex(promote_width(lhs_ty, rhs_ty), is_const)
        }
        _ => Type::Void,
    }
}

/// Compares two types for equality, ignoring constness.
pub(crate) fn types_equal_except_const(lhs: &Type, rhs: &Type) -> bool {
    match (lhs, rhs) {
        (Type::Bit(_), Type::Bit(_))
        | (Type::Qubit, Type::Qubit)
        | (Type::HardwareQubit, Type::HardwareQubit)
        | (Type::Bool(_), Type::Bool(_))
        | (Type::Duration(_), Type::Duration(_))
        | (Type::Stretch(_), Type::Stretch(_))
        | (Type::Range, Type::Range)
        | (Type::Set, Type::Set)
        | (Type::Void, Type::Void)
        | (Type::Err, Type::Err) => true,
        (Type::Int(lhs_width, _), Type::Int(rhs_width, _))
        | (Type::UInt(lhs_width, _), Type::UInt(rhs_width, _))
        | (Type::Float(lhs_width, _), Type::Float(rhs_width, _))
        | (Type::Angle(lhs_width, _), Type::Angle(rhs_width, _))
        | (Type::Complex(lhs_width, _), Type::Complex(rhs_width, _)) => lhs_width == rhs_width,
        (Type::BitArray(lhs_size, _), Type::BitArray(rhs_size, _))
        | (Type::QubitArray(lhs_size), Type::QubitArray(rhs_size)) => lhs_size == rhs_size,
        (Type::Array(lhs), Type::Array(rhs)) => {
            types_equal_except_const(&lhs.base_ty.clone().into(), &rhs.base_ty.clone().into())
                && lhs.dims == rhs.dims
        }
        (Type::StaticArrayRef(lhs), Type::StaticArrayRef(rhs)) => {
            types_equal_except_const(&lhs.base_ty.clone().into(), &rhs.base_ty.clone().into())
                && lhs.dims == rhs.dims
        }
        (Type::DynArrayRef(lhs), Type::DynArrayRef(rhs)) => {
            types_equal_except_const(&lhs.base_ty.clone().into(), &rhs.base_ty.clone().into())
                && lhs.dims == rhs.dims
        }
        (Type::Gate(lhs_cargs, lhs_qargs), Type::Gate(rhs_cargs, rhs_qargs)) => {
            lhs_cargs == rhs_cargs && lhs_qargs == rhs_qargs
        }
        _ => false,
    }
}

/// Compares two types for equality, ignoring constness and width.
/// arrays are equal if their dimensions are equal.
pub(crate) fn base_types_equal(lhs: &Type, rhs: &Type) -> bool {
    match (lhs, rhs) {
        (Type::Bit(_), Type::Bit(_))
        | (Type::Qubit, Type::Qubit)
        | (Type::HardwareQubit, Type::HardwareQubit)
        | (Type::Bool(_), Type::Bool(_))
        | (Type::Duration(_), Type::Duration(_))
        | (Type::Stretch(_), Type::Stretch(_))
        | (Type::Range, Type::Range)
        | (Type::Set, Type::Set)
        | (Type::Void, Type::Void)
        | (Type::Err, Type::Err)
        | (Type::Int(_, _), Type::Int(_, _))
        | (Type::UInt(_, _), Type::UInt(_, _))
        | (Type::Float(_, _), Type::Float(_, _))
        | (Type::Angle(_, _), Type::Angle(_, _))
        | (Type::Complex(_, _), Type::Complex(_, _))
        | (Type::Gate(_, _), Type::Gate(_, _)) => true,
        (Type::BitArray(lhs_size, _), Type::BitArray(rhs_size, _))
        | (Type::QubitArray(lhs_size), Type::QubitArray(rhs_size)) => lhs_size == rhs_size,
        (Type::Array(lhs), Type::Array(rhs)) => {
            base_types_equal(&lhs.base_ty.clone().into(), &rhs.base_ty.clone().into())
                && lhs.dims == rhs.dims
        }
        (Type::StaticArrayRef(lhs), Type::StaticArrayRef(rhs)) => {
            base_types_equal(&lhs.base_ty.clone().into(), &rhs.base_ty.clone().into())
                && lhs.dims == rhs.dims
        }
        (Type::DynArrayRef(lhs), Type::DynArrayRef(rhs)) => {
            base_types_equal(&lhs.base_ty.clone().into(), &rhs.base_ty.clone().into())
                && lhs.dims == rhs.dims
        }
        _ => false,
    }
}

#[must_use]
pub fn can_cast_literal(lhs_ty: &Type, ty_lit: &Type) -> bool {
    // todo: not sure if this top case is still needed after parser changes
    if matches!(lhs_ty, Type::Int(..)) && matches!(ty_lit, Type::UInt(..)) {
        return true;
    }
    // todo: not sure if this case is still needed after parser changes
    if matches!(lhs_ty, Type::UInt(..)) {
        return matches!(ty_lit, Type::Complex(..));
    }

    base_types_equal(lhs_ty, ty_lit)
        || matches!((lhs_ty, ty_lit), (Type::Angle(_, _), Type::Float(_, _)))
        || matches!((lhs_ty, ty_lit), (Type::Bit(..), Type::Angle(..)))
        || matches!(
            (lhs_ty, ty_lit),
            (
                Type::Float(_, _) | Type::Complex(_, _),
                Type::Int(_, _) | Type::UInt(_, _)
            ) | (Type::Complex(_, _), Type::Float(_, _))
        )
        || {
            matches!(lhs_ty, Type::Bit(..) | Type::Bool(..))
                && matches!(ty_lit, Type::Bit(..) | Type::Bool(..))
        }
        || {
            match lhs_ty {
                Type::BitArray(size, _) => {
                    matches!(ty_lit, Type::Int(_, _) | Type::UInt(_, _))
                        || matches!(ty_lit, Type::Angle(width, _) if Some(*size) == *width)
                }
                Type::Angle(width, _) => {
                    matches!(ty_lit, Type::BitArray(size, _) if Some(*size) == *width)
                }
                _ => false,
            }
        }
}

/// some literals can be cast to a specific type if the value is known
/// This is useful to avoid generating a cast expression in the AST
pub(crate) fn can_cast_literal_with_value_knowledge(lhs_ty: &Type, kind: &LiteralKind) -> bool {
    if matches!(lhs_ty, &Type::Bit(_)) {
        if let LiteralKind::Int(value) = kind {
            return *value == 0 || *value == 1;
        }
    }
    if matches!(lhs_ty, &Type::UInt(..)) {
        if let LiteralKind::Int(value) = kind {
            return *value >= 0;
        }
    }
    // Much existing OpenQASM code uses 0 as a literal for angles
    // and Qiskit generates this code. While this is not allowed
    // in the spec, we allow it for compatibility.
    if matches!(lhs_ty, &Type::Angle(..)) {
        if let LiteralKind::Int(value) = kind {
            return *value == 0;
        }
    }
    false
}

// https://openqasm.com/language/classical.html
pub(crate) fn unary_op_can_be_applied_to_type(op: syntax::UnaryOp, ty: &Type) -> bool {
    match op {
        syntax::UnaryOp::NotB => matches!(
            ty,
            Type::Bit(_) | Type::UInt(_, _) | Type::Angle(_, _) | Type::BitArray(_, _)
        ),
        syntax::UnaryOp::NotL => matches!(ty, Type::Bool(_)),
        syntax::UnaryOp::Neg => {
            matches!(ty, Type::Int(_, _) | Type::Float(_, _) | Type::Angle(_, _))
        }
    }
}

pub(crate) fn binop_requires_asymmetric_angle_op(
    op: syntax::BinOp,
    lhs: &Type,
    rhs: &Type,
) -> bool {
    match op {
        // Design Decision: This branch isn't Spec compliant. It is an exception
        //                  to support existing code in the Qiskit ecosystem which
        //                  compiles to qasm that adds and subtracts floats and angles.
        syntax::BinOp::Add | syntax::BinOp::Sub => {
            matches!(
                (lhs, rhs),
                (Type::Float(..), Type::Angle(..)) | (Type::Angle(..), Type::Float(..))
            )
        }
        syntax::BinOp::Div => {
            matches!(
                (lhs, rhs),
                (
                    Type::Angle(_, _),
                    Type::Int(_, _) | Type::UInt(_, _) | Type::Angle(_, _)
                )
            )
        }
        syntax::BinOp::Mul => {
            matches!(
                (lhs, rhs),
                (Type::Angle(_, _), Type::Int(_, _) | Type::UInt(_, _))
            ) || matches!(
                (lhs, rhs),
                (Type::Int(_, _) | Type::UInt(_, _), Type::Angle(_, _))
            )
        }
        _ => false,
    }
}

/// Bit arrays can be compared, but need to be converted to int first
pub(crate) fn binop_requires_int_conversion_for_type(
    op: syntax::BinOp,
    lhs: &Type,
    rhs: &Type,
) -> bool {
    match op {
        syntax::BinOp::Eq
        | syntax::BinOp::Gt
        | syntax::BinOp::Gte
        | syntax::BinOp::Lt
        | syntax::BinOp::Lte
        | syntax::BinOp::Neq => match (lhs, rhs) {
            (Type::BitArray(lhs_size, _), Type::BitArray(rhs_size, _)) => lhs_size == rhs_size,
            _ => false,
        },
        _ => false,
    }
}

/// Symmetric arithmetic conversions are applied to:
/// binary arithmetic *, /, %, +, -
/// relational operators <, >, <=, >=, ==, !=
/// binary bitwise arithmetic &, ^, |,
pub(crate) fn requires_symmetric_conversion(op: syntax::BinOp) -> bool {
    match op {
        syntax::BinOp::Add
        | syntax::BinOp::AndB
        | syntax::BinOp::AndL
        | syntax::BinOp::Div
        | syntax::BinOp::Eq
        | syntax::BinOp::Exp
        | syntax::BinOp::Gt
        | syntax::BinOp::Gte
        | syntax::BinOp::Lt
        | syntax::BinOp::Lte
        | syntax::BinOp::Mod
        | syntax::BinOp::Mul
        | syntax::BinOp::Neq
        | syntax::BinOp::OrB
        | syntax::BinOp::OrL
        | syntax::BinOp::Sub
        | syntax::BinOp::XorB => true,
        syntax::BinOp::Shl | syntax::BinOp::Shr => false,
    }
}

pub(crate) fn try_promote_with_casting(left_type: &Type, right_type: &Type) -> Type {
    let promoted_type = promote_types(left_type, right_type);

    if promoted_type != Type::Void {
        return promoted_type;
    }
    if let Some(value) = try_promote_bitarray_to_int(left_type, right_type) {
        return value;
    }
    // simple promotion failed, try a lossless cast
    // each side to double
    let promoted_rhs = promote_types(&Type::Float(None, right_type.is_const()), right_type);
    let promoted_lhs = promote_types(left_type, &Type::Float(None, left_type.is_const()));

    match (promoted_lhs, promoted_rhs) {
        (Type::Void, Type::Void) => Type::Float(None, false),
        (Type::Void, promoted_rhs) => promoted_rhs,
        (promoted_lhs, Type::Void) => promoted_lhs,
        (promoted_lhs, promoted_rhs) => {
            // return the greater of the two promoted types
            if matches!(promoted_lhs, Type::Complex(..)) {
                promoted_lhs
            } else if matches!(promoted_rhs, Type::Complex(..)) {
                promoted_rhs
            } else if matches!(promoted_lhs, Type::Float(..)) {
                promoted_lhs
            } else if matches!(promoted_rhs, Type::Float(..)) {
                promoted_rhs
            } else {
                Type::Float(None, false)
            }
        }
    }
}

fn try_promote_bitarray_to_int(left_type: &Type, right_type: &Type) -> Option<Type> {
    if matches!(
        (left_type, right_type),
        (Type::Int(..) | Type::UInt(..), Type::BitArray(..))
    ) {
        let Type::BitArray(size, _) = right_type else {
            return None;
        };

        if left_type.width().is_some() && left_type.width() != Some(*size) {
            return None;
        }

        return Some(left_type.clone());
    }

    if matches!(
        (left_type, right_type),
        (Type::BitArray(..), Type::Int(..) | Type::UInt(..))
    ) {
        let Type::BitArray(size, _) = left_type else {
            return None;
        };

        if right_type.width().is_some() && right_type.width() != Some(*size) {
            return None;
        }

        return Some(right_type.clone());
    }
    None
}

// integer promotions are applied only to both operands of
// the shift operators << and >>
pub(crate) fn binop_requires_symmetric_uint_conversion(op: syntax::BinOp) -> bool {
    matches!(op, syntax::BinOp::Shl | syntax::BinOp::Shr)
}

pub(crate) fn is_complex_binop_supported(op: syntax::BinOp) -> bool {
    matches!(
        op,
        syntax::BinOp::Add
            | syntax::BinOp::Sub
            | syntax::BinOp::Mul
            | syntax::BinOp::Div
            | syntax::BinOp::Exp
    )
}

/// Returns true if the binary op is supported for the `lhs` and `rhs` types.
/// Any conversions have been made explicit by inserting casts during lowering.
pub(crate) fn binary_op_is_supported_for_types(op: BinOp, lhs_ty: &Type, rhs_ty: &Type) -> bool {
    use Type::*;

    match op {
        // Bit Shifts: `rhs_ty` must always be `uint`.
        BinOp::Shl | BinOp::Shr => {
            matches!(lhs_ty, UInt(..) | Angle(..) | Bit(..) | BitArray(..))
                && matches!(rhs_ty, UInt(..))
        }

        // Bitwise.
        BinOp::AndB | BinOp::OrB | BinOp::XorB => {
            base_types_equal(lhs_ty, rhs_ty)
                && matches!(lhs_ty, UInt(..) | Angle(..) | Bit(..) | BitArray(..))
        }

        // Logical.
        BinOp::AndL | BinOp::OrL => matches!(lhs_ty, Bool(..)) && matches!(rhs_ty, Bool(..)),

        // Comparison.
        BinOp::Eq | BinOp::Neq | BinOp::Gt | BinOp::Gte | BinOp::Lt | BinOp::Lte => {
            base_types_equal(lhs_ty, rhs_ty)
                && matches!(
                    lhs_ty,
                    Int(..) | UInt(..) | Angle(..) | Bit(..) | BitArray(..)
                )
        }

        // Arithmetic
        BinOp::Add | BinOp::Sub => {
            let duration_case = both_types_are_duration_subtypes(lhs_ty, rhs_ty);

            let base_case = base_types_equal(lhs_ty, rhs_ty)
                && matches!(
                    lhs_ty,
                    Int(..) | UInt(..) | Float(..) | Angle(..) | Complex(..)
                );
            duration_case || base_case
        }
        BinOp::Mul => {
            let duration_case = (type_is_duration_subtype(lhs_ty)
                && matches!(&rhs_ty, Type::Float(_, _) | Type::Int(..) | Type::UInt(..),))
                || (type_is_duration_subtype(rhs_ty)
                    && matches!(&lhs_ty, Type::Float(_, _) | Type::Int(..) | Type::UInt(..)));

            let uint_angle_case = (matches!(lhs_ty, Angle(..)) && matches!(rhs_ty, UInt(..)))
                || (matches!(lhs_ty, UInt(..)) && matches!(rhs_ty, Angle(..)));

            let base_case = base_types_equal(lhs_ty, rhs_ty)
                && matches!(lhs_ty, Int(..) | UInt(..) | Float(..));

            duration_case || uint_angle_case || base_case
        }
        BinOp::Div => {
            let duration_base_case = both_types_are_duration_subtypes(lhs_ty, rhs_ty);
            let duration_asymmetric_case = type_is_duration_subtype(lhs_ty)
                && matches!(&rhs_ty, Type::Float(_, _) | Type::Int(..) | Type::UInt(..));
            let duration_case = duration_base_case || duration_asymmetric_case;

            let uint_angle_case = matches!(lhs_ty, Angle(..)) && matches!(rhs_ty, UInt(..));

            let base_case = base_types_equal(lhs_ty, rhs_ty)
                && matches!(lhs_ty, Int(..) | UInt(..) | Float(..) | Angle(..));

            duration_case || uint_angle_case || base_case
        }
        BinOp::Mod => base_types_equal(lhs_ty, rhs_ty) && matches!(lhs_ty, Int(..) | UInt(..)),
        BinOp::Exp => {
            base_types_equal(lhs_ty, rhs_ty) && matches!(lhs_ty, Int(..) | UInt(..) | Float(..))
        }
    }
}

pub(crate) fn both_types_are_duration_subtypes(left_type: &Type, right_type: &Type) -> bool {
    type_is_duration_subtype(left_type) && type_is_duration_subtype(right_type)
}

pub(crate) fn either_type_is_duration_subtype(left_type: &Type, right_type: &Type) -> bool {
    type_is_duration_subtype(left_type) || type_is_duration_subtype(right_type)
}

pub(crate) fn type_is_duration_subtype(ty: &Type) -> bool {
    matches!(ty, Type::Duration(_) | Type::Stretch(_))
}
