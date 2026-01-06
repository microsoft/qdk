// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reference: <https://openqasm.com/versions/3.0/language/types.html#built-in-constant-expression-functions>
//!
//! The following are compile-time functions that take const inputs and have a const output.
//! The normal implicit casting rules apply to the inputs of these functions.

use crate::{
    semantic::{
        Lowerer, SemanticErrorKind,
        ast::{Expr, ExprKind, LiteralKind},
        const_eval::ConstEvalError,
        types::{Type, can_cast_literal, can_cast_literal_with_value_knowledge},
    },
    stdlib::complex::Complex,
};
use core::f64;
use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use std::fmt::Write;
use std::sync::LazyLock;

// ---------------------------------------------------
// Dispatch mechanism for polymorphic function calls.
// ---------------------------------------------------

/// The output of calling a polymorphic function is a pair
/// where the first element is the result of the computation
/// and the second element is the type of the monomorphic
/// function that was selected, if any.
type PolymorphicFunctionOutput = Option<Expr>;

// A function table mapping function signatures to functions.
// This is a vector and not a hash-map because the order of
// iteration matters. Overloads should be tried in the order
// they appear.
type FnTable = Vec<(
    Type,
    Box<dyn Fn(&[Expr], Span) -> Result<LiteralKind, ConstEvalError> + Sync + Send>,
)>;

type FnTableRef<'a> = &'a [(
    Type,
    Box<dyn Fn(&[Expr], Span) -> Result<LiteralKind, ConstEvalError> + Sync + Send>,
)];

fn dispatch(
    name: &str,
    name_span: Span,
    call_span: Span,
    inputs: &[Expr],
    fn_table: FnTableRef,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    // All the builtin functions take const expressions as inputs
    // and return a const expression as output. If any of the
    // inputs is not const, we return an error.
    if inputs.iter().filter_map(|e| check_const(e, ctx)).count() != inputs.len() {
        return None;
    }

    // Reference: <https://openqasm.com/versions/3.0/language/types.html#built-in-constant-expression-functions>
    //
    // For each built-in function, the chosen overload is the first one to appear in Table 2 in
    // the link above where all given operands can be implicitly cast to the valid input types.
    // The output type is not considered when choosing an overload. It is an error if there is
    // no valid overload for a given sequence of operands.
    for (signature, function) in fn_table {
        match try_implicit_cast_inputs(inputs, signature, ctx) {
            Ok(new_inputs) => {
                let output = function(&new_inputs, call_span);
                match output {
                    Ok(output) => {
                        return Some(Expr::builtin_funcall(
                            name,
                            call_span,
                            name_span,
                            signature.clone(),
                            inputs,
                            output,
                        ));
                    }
                    Err(err) => {
                        ctx.push_const_eval_error(err);
                        return None;
                    }
                };
            }
            Err(Some(err)) => {
                // This error is special. It means that the cast between the types is allowed,
                // but the literal value itself doesn't fit in the target type.
                // So, we push this error to let the user know.
                //
                // Note that in the other cases (when the cast is not allowed) we don't push
                // the error since the goal is to keep trying with all the overloads until
                // we find one that works.
                if matches!(err, SemanticErrorKind::InvalidCastValueRange(..)) {
                    ctx.push_semantic_error(err);
                    return None;
                }
            }
            Err(None) => {
                // This case means that the cast isn't allowed. So, we do nothing and
                // keep trying the other signatures in the function table.
            }
        }
    }

    ctx.push_const_eval_error(no_valid_overload_error(name, call_span, inputs, fn_table));

    None
}

fn check_const(expr: &Expr, ctx: &mut Lowerer) -> Option<()> {
    if expr.ty.is_const() && expr.const_value.is_some() {
        Some(())
    } else {
        ctx.push_const_eval_error(ConstEvalError::ExprMustBeConst(expr.span));
        None
    }
}

/// A helper macro for unwrapping the literal value of a const expression.
macro_rules! unwrap_lit {
    // This pattern is used for unary expressions.
    ($const_expr:expr, $pat:pat) => {
        #[allow(irrefutable_let_patterns)]
        let $pat = $const_expr.get_const_value().expect("expr is const") else {
            unreachable!("if we hit this, there is a bug in our dispatch mechanism")
        };
    };
}

/// Tries to implicitly cast all inputs to the signature of the overload being
/// considered in dispatch. Returns Some(_) if all the inputs can be successfully
/// cast to match the signature of the overload, and None otherwise.
fn try_implicit_cast_inputs(
    inputs: &[Expr],
    signature: &Type,
    ctx: &mut Lowerer,
) -> Result<Vec<Expr>, Option<SemanticErrorKind>> {
    let mut new_inputs = Vec::with_capacity(inputs.len());
    let Type::Function(input_types, _) = signature else {
        unreachable!("if we hit this we are initializing the function table incorrectly");
    };

    if inputs.len() != input_types.len() {
        return Err(None);
    }

    for (input, ty) in inputs.iter().zip(input_types.iter()) {
        unwrap_lit!(input, value);
        if can_cast_literal(ty, &input.ty) || can_cast_literal_with_value_knowledge(ty, &value) {
            let mut value_expr = input.clone();
            // `coerce_literal_expr_to_type` expects a value expression.
            // So, we build an adhoc expression where `Expr::Kind` is a
            // `LiteralKind` to satisfy this method.
            value_expr.kind = Box::new(ExprKind::Lit(
                input.get_const_value().expect("input should be const"),
            ));
            match Lowerer::try_coerce_literal_expr_to_type(ty, &value_expr, &value) {
                Ok(coerced_input) => new_inputs.push(coerced_input.with_const_value(ctx)),
                Err(err) => return Err(err),
            }
        } else {
            return Err(None);
        }
    }

    Ok(new_inputs)
}

/// The Display method for [`Type`] doesn't include the name of the function.
/// So, we have this custom formatter to print better errors in this module.
fn format_function_signature(name: &str, signature: &Type) -> String {
    let Type::Function(params_ty, return_ty) = signature else {
        panic!();
    };

    let params_ty_str = params_ty
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    format!("def {name}({params_ty_str}) -> {return_ty}")
}

/// Builds an error message explaining to the user that there is no
/// valid overload matching the inputs they provided, and showing them
/// the available overloads for the function they tried to call.
fn no_valid_overload_error(
    name: &str,
    call_span: Span,
    inputs: &[Expr],
    fn_table: FnTableRef,
) -> ConstEvalError {
    let mut error_msg = String::new();
    write!(
        error_msg,
        "There is no valid overload of `{name}` for inputs: "
    )
    .expect("write should succeed");

    let inputs_str = inputs
        .iter()
        .map(|expr| expr.ty.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    write!(error_msg, "({inputs_str})").expect("write should succeed");
    write!(error_msg, "\nOverloads available are:").expect("write should succeed");

    for (signature, _) in fn_table {
        write!(
            error_msg,
            "\n    {}",
            format_function_signature(name, signature)
        )
        .expect("write should succeed");
    }

    ConstEvalError::NoValidOverloadForBuiltinFunction(error_msg, call_span)
}

/// This is a special case of [`no_valid_overload_error`] used for [`rotl`] and [`rotr`]
/// when the user doesn't provides a sized type as the first argument. We can't construct
/// the function table in this case, therefore, we don't get to the point in the dispatch
/// mechanism where `no_valid_overload_error` is pushed into the ctx, so we need this special
/// error for that case.
fn no_valid_overload_rot_error(name: &str, call_span: Span, inputs: &[Expr]) -> ConstEvalError {
    let mut error_msg = String::new();
    write!(
        error_msg,
        "There is no valid overload of `{name}` for inputs: "
    )
    .expect("write should succeed");

    let inputs_str = inputs
        .iter()
        .map(|expr| expr.ty.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    write!(error_msg, "({inputs_str})").expect("write should succeed");
    write!(error_msg, "\nOverloads available are:").expect("write should succeed");
    write!(error_msg, "\n    fn {name}(bit[n], int) -> bit[n]").expect("write should succeed");
    write!(error_msg, "\n    fn {name}(uint[n], int) -> uint[n]").expect("write should succeed");

    ConstEvalError::NoValidOverloadForBuiltinFunction(error_msg, call_span)
}

/// This is a special case of [`no_valid_overload_error`] used for [`popcount`]
/// when the user doesn't provide a sized type as the first argument. We can't construct
/// the function table in this case, therefore, we don't get to the point in the dispatch
/// mechanism where `no_valid_overload_error` is pushed into the ctx, so we need this special
/// error for that case.
fn no_valid_overload_popcount_error(call_span: Span, inputs: &[Expr]) -> ConstEvalError {
    let mut error_msg = String::new();
    write!(
        error_msg,
        "There is no valid overload of `popcount` for inputs: "
    )
    .expect("write should succeed");

    let inputs_str = inputs
        .iter()
        .map(|expr| expr.ty.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    write!(error_msg, "({inputs_str})").expect("write should succeed");
    write!(error_msg, "\nOverloads available are:").expect("write should succeed");
    write!(error_msg, "\n    fn popcount(bit[n]) -> uint").expect("write should succeed");

    ConstEvalError::NoValidOverloadForBuiltinFunction(error_msg, call_span)
}

// ---------------------------------------
// Helper macro to reduce boilerplate
// when initializing the function tables.
// ---------------------------------------

pub fn fun(inputs: &[Type], output: Type) -> Type {
    Type::Function(inputs.into(), output.into())
}

macro_rules! ty {
    (int) => {
        Type::Int(None, true)
    };
    (uint [$n:expr]) => {
        Type::UInt(Some($n), true)
    };
    (uint) => {
        Type::UInt(None, true)
    };
    (float) => {
        Type::Float(None, true)
    };
    (angle) => {
        Type::Angle(None, true)
    };
    (complex) => {
        Type::Complex(None, true)
    };
    (bit [$n:expr]) => {
        Type::BitArray($n, true)
    };
}

/// This macro is used to create symbol tables for popcount, rotl, and rotr which
/// cannot be static since they depend on the width of the inputs and outputs.
macro_rules! fn_table {
    ($( ($($arg:tt $([$arg_witdh:expr])?),+) -> $output:tt $([$output_width:expr])? : $fun:expr),+) => {
        vec![
            $(
                (fun(
                    &[$(ty!($arg $([$arg_witdh])?)),+],
                    ty!($output $([$output_width])?)
                ), Box::new($fun))
            ),+
        ]
    };
}

/// This macro is used to create static symbol tables for most of the builtin functions,
/// except for popcount, rotl, and rotr which cannot be static since they depend on the
/// width of the inputs and outputs.
macro_rules! static_fn_table {
    ($( ($($arg:tt),+) -> $output:tt : $fun:expr),+) => {
        static FN_TABLE: LazyLock<FnTable> =
        LazyLock::new(|| vec![
                $(
                    (fun(
                        &[$(ty!($arg)),+],
                        ty!($output)
                    ), Box::new($fun))
                ),+
            ]
        );
    };
}

// --------------------------------------------
// Helpers for functions that manipulate bits,
// i.e.: `popcount`, `rotl`, and `rotr`.
// --------------------------------------------

/// Returns the width of a type.
/// [`Type::width`] doesn't return the width of `BitArary`,
/// since that is treated as an array size in the rest of
/// the compiler. Therefore, we need this helper function
/// for this module.
fn get_ty_width(ty: &Type) -> Option<u32> {
    match ty {
        Type::Angle(w, _)
        | Type::Complex(w, _)
        | Type::Float(w, _)
        | Type::Int(w, _)
        | Type::UInt(w, _) => *w,
        Type::BitArray(w, _) => Some(*w),
        _ => None,
    }
}

fn count_ones_bigint(mut value: BigInt) -> u32 {
    let mut ones = 0;
    while value != BigInt::ZERO {
        if value.bit(0) {
            ones += 1;
        }
        value >>= 1;
    }
    ones
}

#[inline]
fn rot1_left(value: &mut i64, width: u32) {
    let mask: i64 = 1 << (width - 1);
    let last_bit = *value & mask;

    // Unset the last bit.
    *value ^= last_bit;

    // Shift the value one bit to the left.
    *value <<= 1;

    // Set the first bit to the value of `last_bit`.
    *value &= last_bit >> (width - 1);
}

#[inline]
fn rot1_right(value: &mut i64, width: u32) {
    let first_bit = *value & 1;

    // Shift the value one bit to the right.
    *value >>= 1;

    // Set the last bit to the value of `first_bit`.
    *value &= first_bit << (width - 1);
}

#[inline]
fn rot1_left_bigint(value: &mut BigInt, width: u32) {
    let last_bit_pos: u64 = (width - 1).into();
    let last_bit = value.bit(last_bit_pos);

    // Unset the last bit.
    value.set_bit(last_bit_pos, false);

    // Shift the value one bit to the left.
    *value <<= 1;

    // Set the first bit to the value of `last_bit`.
    value.set_bit(0, last_bit);
}

#[inline]
fn rot1_right_bigint(value: &mut BigInt, width: u32) {
    let first_bit = value.bit(0);

    // Shift the value one bit to the right.
    *value >>= 1;

    // Set the last bit to the value of `first_bit`.
    let last_bit_pos: u64 = (width - 1).into();
    value.set_bit(last_bit_pos, first_bit);
}

// ----------------------------------
// Builtin functions implementation.
// ----------------------------------

// ----------------------------------
// arccos

pub(crate) fn arccos(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : arccos_float
    }

    dispatch("arccos", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn arccos_float(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));

    if (-1.0..=1.0).contains(&a) {
        Ok(LiteralKind::Float(a.acos()))
    } else {
        Err(ConstEvalError::DomainError(
            "arccos input should be in the range [-1.0, 1.0]".to_string(),
            span,
        ))
    }
}

// ----------------------------------
// arcsin

pub(crate) fn arcsin(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : arcsin_float
    }
    dispatch("arcsin", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn arcsin_float(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));

    if (-1.0..=1.0).contains(&a) {
        Ok(LiteralKind::Float(a.asin()))
    } else {
        Err(ConstEvalError::DomainError(
            "arcsin input should be in the range [-1.0, 1.0]".to_string(),
            span,
        ))
    }
}

// ----------------------------------
// arctan

pub(crate) fn arctan(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : arctan_float
    }
    dispatch("arctan", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn arctan_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.atan()))
}

// ----------------------------------
// ceiling

pub(crate) fn ceiling(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : ceiling_float
    }
    dispatch("ceiling", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn ceiling_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.ceil()))
}

// ----------------------------------
// cos

pub(crate) fn cos(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : cos_float,
        (angle) -> float : cos_angle
    }
    dispatch("cos", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn cos_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.cos()))
}

fn cos_angle(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Angle(a));
    if let Ok(a) = TryInto::<f64>::try_into(a) {
        Ok(LiteralKind::Float(a.cos()))
    } else {
        Err(ConstEvalError::ValueOverflow(
            format!("{}", inputs[0].ty),
            "float".into(),
            span,
        ))
    }
}

// ----------------------------------
// exp

pub(crate) fn exp(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : exp_float,
        (complex) -> complex : exp_complex
    }
    dispatch("exp", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn exp_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.exp()))
}

fn exp_complex(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Complex(a));
    let base = Complex::real(f64::consts::E);
    Ok(base.pow(a).into())
}

// ----------------------------------
// floor

pub(crate) fn floor(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : floor_float
    }
    dispatch("floor", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn floor_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.floor()))
}

// ----------------------------------
// log

pub(crate) fn log(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : log_float
    }
    dispatch("log", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn log_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.ln()))
}

// ----------------------------------
// mod

pub(crate) fn mod_(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (int, int) -> int : mod_int,
        (float, float) -> float : mod_float
    }
    dispatch("mod", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn mod_int(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Int(a));
    unwrap_lit!(inputs[1], LiteralKind::Int(b));

    if b == 0 {
        Err(ConstEvalError::DivisionByZero(span))
    } else {
        Ok(LiteralKind::Int(a.rem_euclid(b)))
    }
}

fn mod_float(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    unwrap_lit!(inputs[1], LiteralKind::Float(b));

    if b == 0. {
        Err(ConstEvalError::DivisionByZero(span))
    } else {
        Ok(LiteralKind::Float(a.rem_euclid(b)))
    }
}

// ----------------------------------
// popcount

pub(crate) fn popcount(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    let Some(n) = inputs.first().and_then(|expr| get_ty_width(&expr.ty)) else {
        ctx.push_const_eval_error(no_valid_overload_popcount_error(call_span, inputs));
        return None;
    };

    let fn_table: FnTable = fn_table! {
        (bit[n]) -> uint : popcount_bitarray
    };

    dispatch("popcount", name_span, call_span, inputs, &fn_table, ctx)
}

fn popcount_bitarray(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Bitstring(a, _));
    Ok(LiteralKind::Int(count_ones_bigint(a).into()))
}

// ----------------------------------
// pow

pub(crate) fn pow(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (int, uint) -> float : pow_int_uint,
        (float, float) -> float : pow_float,
        (complex, complex) -> complex : pow_complex
    }
    dispatch("pow", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn pow_int_uint(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Int(a));
    unwrap_lit!(inputs[1], LiteralKind::Int(b));

    if let Ok(b) = TryInto::<u32>::try_into(b) {
        Ok(LiteralKind::Int(a.pow(b)))
    } else {
        Err(ConstEvalError::ValueOverflow(
            format!("{}", inputs[1].ty),
            "uint[32]".into(),
            span,
        ))
    }
}

fn pow_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    unwrap_lit!(inputs[1], LiteralKind::Float(b));
    Ok(LiteralKind::Float(a.powf(b)))
}

fn pow_complex(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Complex(a));
    unwrap_lit!(inputs[1], LiteralKind::Complex(b));
    Ok(a.pow(b).into())
}

// ----------------------------------
// rotl

pub(crate) fn rotl(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    let Some(n) = inputs.first().and_then(|expr| get_ty_width(&expr.ty)) else {
        ctx.push_const_eval_error(no_valid_overload_rot_error("rotl", call_span, inputs));
        return None;
    };

    let fn_table: FnTable = fn_table! {
        (bit[n], int) -> bit[n] : rotl_bitarray,
        (uint[n], int) -> uint[n] : rotl_uint
    };

    dispatch("rotl", name_span, call_span, inputs, &fn_table, ctx)
}

fn rotl_bitarray(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Bitstring(mut a, width));
    unwrap_lit!(inputs[1], LiteralKind::Int(distance));

    let rot1 = if distance >= 0 {
        rot1_left_bigint
    } else {
        rot1_right_bigint
    };

    for _ in 0..distance.abs() {
        rot1(&mut a, width);
    }

    Ok(LiteralKind::Bitstring(a, width))
}

fn rotl_uint(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    let width = get_ty_width(&inputs[0].ty)
        .expect("width should be Some, because we dispatched the function correctly");
    unwrap_lit!(inputs[0], LiteralKind::Int(mut a));
    unwrap_lit!(inputs[1], LiteralKind::Int(distance));

    let rot1 = if distance >= 0 { rot1_left } else { rot1_right };

    for _ in 0..distance.abs() {
        rot1(&mut a, width);
    }

    Ok(LiteralKind::Int(a))
}

// ----------------------------------
// rotr

pub(crate) fn rotr(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    let Some(n) = inputs.first().and_then(|expr| get_ty_width(&expr.ty)) else {
        ctx.push_const_eval_error(no_valid_overload_rot_error("rotr", call_span, inputs));
        return None;
    };

    let fn_table: FnTable = fn_table! {
        (bit[n], int) -> bit[n] : rotr_bitarray,
        (uint[n], int) -> uint[n] : rotr_uint
    };

    dispatch("rotr", name_span, call_span, inputs, &fn_table, ctx)
}

fn rotr_bitarray(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Bitstring(mut a, width));
    unwrap_lit!(inputs[1], LiteralKind::Int(distance));

    let rot1 = if distance >= 0 {
        rot1_right_bigint
    } else {
        rot1_left_bigint
    };

    for _ in 0..distance.abs() {
        rot1(&mut a, width);
    }

    Ok(LiteralKind::Bitstring(a, width))
}

fn rotr_uint(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    let width = get_ty_width(&inputs[0].ty)
        .expect("width should be Some, because we dispatched the function correctly");
    unwrap_lit!(inputs[0], LiteralKind::Int(mut a));
    unwrap_lit!(inputs[1], LiteralKind::Int(distance));

    let rot1 = if distance >= 0 { rot1_right } else { rot1_left };

    for _ in 0..distance.abs() {
        rot1(&mut a, width);
    }

    Ok(LiteralKind::Int(a))
}

// ----------------------------------
// sin

pub(crate) fn sin(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : sin_float,
        (angle) -> float : sin_angle
    }
    dispatch("sin", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn sin_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.sin()))
}

fn sin_angle(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Angle(a));
    if let Ok(a) = TryInto::<f64>::try_into(a) {
        Ok(LiteralKind::Float(a.sin()))
    } else {
        Err(ConstEvalError::ValueOverflow(
            format!("{}", inputs[0].ty),
            "float".into(),
            span,
        ))
    }
}

// ----------------------------------
// sqrt

pub(crate) fn sqrt(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : sqrt_float,
        (complex) -> complex : sqrt_complex
    }
    dispatch("sqrt", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn sqrt_float(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    if a >= 0. {
        Ok(LiteralKind::Float(a.sqrt()))
    } else {
        Err(ConstEvalError::DomainError(
            "cannot compute square root of negative floats".to_string(),
            span,
        ))
    }
}

fn sqrt_complex(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Complex(a));
    Ok(a.pow(Complex::real(0.5)).into())
}

// ----------------------------------
// tan

pub(crate) fn tan(
    inputs: &[Expr],
    name_span: Span,
    call_span: Span,
    ctx: &mut Lowerer,
) -> PolymorphicFunctionOutput {
    static_fn_table! {
        (float) -> float : tan_float,
        (angle) -> float : tan_angle
    }
    dispatch("tan", name_span, call_span, inputs, &FN_TABLE, ctx)
}

fn tan_float(inputs: &[Expr], _: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Float(a));
    Ok(LiteralKind::Float(a.tan()))
}

fn tan_angle(inputs: &[Expr], span: Span) -> Result<LiteralKind, ConstEvalError> {
    unwrap_lit!(inputs[0], LiteralKind::Angle(a));
    if let Ok(a) = TryInto::<f64>::try_into(a) {
        Ok(LiteralKind::Float(a.tan()))
    } else {
        Err(ConstEvalError::ValueOverflow(
            format!("{}", inputs[0].ty),
            "float".into(),
            span,
        ))
    }
}
