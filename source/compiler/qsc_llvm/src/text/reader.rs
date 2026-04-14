// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::model::Type;
use crate::model::{
    Attribute, AttributeGroup, BasicBlock, BinOpKind, CastKind, Constant, FloatPredicate, Function,
    GlobalVariable, Instruction, IntPredicate, Linkage, MetadataNode, MetadataValue, Module,
    NamedMetadata, Operand, Param, StructType,
};
use crate::{ReadDiagnostic, ReadDiagnosticKind, ReadPolicy};
use winnow::combinator::opt;
use winnow::error::{ContextError, ErrMode, StrContext};
use winnow::prelude::*;
use winnow::token::{any, literal, one_of, take_while};

type Input<'a> = &'a str;
type PResult<T> = winnow::ModalResult<T, ContextError>;

fn ws_no_newline(input: &mut Input<'_>) -> PResult<()> {
    take_while(0.., |c: char| c == ' ' || c == '\t' || c == '\r')
        .void()
        .parse_next(input)
}

fn ws(input: &mut Input<'_>) -> PResult<()> {
    take_while(0.., |c: char| c.is_ascii_whitespace())
        .void()
        .parse_next(input)
}

fn line_comment(input: &mut Input<'_>) -> PResult<()> {
    (';', take_while(0.., |c: char| c != '\n'), opt('\n'))
        .void()
        .parse_next(input)
}

fn ws_and_comments(input: &mut Input<'_>) -> PResult<()> {
    loop {
        ws(input)?;
        if input.starts_with(';') {
            line_comment(input)?;
        } else {
            break;
        }
    }
    Ok(())
}

fn identifier_chars(input: &mut Input<'_>) -> PResult<String> {
    take_while(1.., |c: char| {
        c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
    })
    .map(String::from)
    .context(StrContext::Label("identifier"))
    .parse_next(input)
}

/// Parse unsigned decimal integer
fn parse_u64(input: &mut Input<'_>) -> PResult<u64> {
    take_while(1.., |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u64>())
        .context(StrContext::Label("unsigned integer"))
        .parse_next(input)
}

fn parse_u32(input: &mut Input<'_>) -> PResult<u32> {
    take_while(1.., |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u32>())
        .context(StrContext::Label("u32"))
        .parse_next(input)
}

/// Parse a possibly-negative decimal integer
fn parse_integer(input: &mut Input<'_>) -> PResult<i64> {
    let neg = opt('-').parse_next(input)?;
    let digits: &str = take_while(1.., |c: char| c.is_ascii_digit())
        .context(StrContext::Label("integer digits"))
        .parse_next(input)?;
    let val: i64 = digits
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    Ok(if neg.is_some() { -val } else { val })
}

/// Parse a possibly-negative float (digits with optional dot)
fn parse_float_literal(input: &mut Input<'_>) -> PResult<f64> {
    // LLVM hex float: 0xHHHHHHHHHHHHHHHH (16 hex digits = IEEE 754 double)
    if input.starts_with("0x") {
        literal("0x").parse_next(input)?;
        let hex_str: &str = take_while(1.., |c: char| c.is_ascii_hexdigit())
            .context(StrContext::Label("hex float digits"))
            .parse_next(input)?;
        let bits = u64::from_str_radix(hex_str, 16)
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        return Ok(f64::from_bits(bits));
    }

    let neg = opt('-').parse_next(input)?;
    let int_part: &str = take_while(1.., |c: char| c.is_ascii_digit())
        .context(StrContext::Label("float digits"))
        .parse_next(input)?;
    let frac = opt(('.', take_while(0.., |c: char| c.is_ascii_digit()))).parse_next(input)?;
    let exp = opt((
        'e',
        opt(one_of(['+', '-'])),
        take_while(1.., |c: char| c.is_ascii_digit()),
    ))
    .parse_next(input)?;
    let mut s = String::new();
    if neg.is_some() {
        s.push('-');
    }
    s.push_str(int_part);
    if let Some((_, frac_digits)) = frac {
        s.push('.');
        s.push_str(frac_digits);
    }
    if let Some((_, sign, exp_digits)) = exp {
        s.push('e');
        if let Some(sign_ch) = sign {
            s.push(sign_ch);
        }
        s.push_str(exp_digits);
    }
    let val: f64 = s
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    Ok(val)
}

/// Parse a quoted string: "..."
fn parse_quoted_string(input: &mut Input<'_>) -> PResult<String> {
    '"'.parse_next(input)?;
    let mut s = String::new();
    loop {
        let ch = any
            .context(StrContext::Label("char in quoted string"))
            .parse_next(input)?;
        match ch {
            '"' => return Ok(s),
            '\\' => {
                let next = any.parse_next(input)?;
                match next {
                    '\\' => s.push('\\'),
                    '"' => s.push('"'),
                    'n' => s.push('\n'),
                    c if c.is_ascii_hexdigit() => {
                        let mut hex = String::new();
                        hex.push(c);
                        // Peek for a second hex digit
                        if input.chars().next().is_some_and(|h| h.is_ascii_hexdigit()) {
                            let h2 = any.parse_next(input)?;
                            hex.push(h2);
                        }
                        let byte = u8::from_str_radix(&hex, 16)
                            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
                        s.push(byte as char);
                    }
                    other => {
                        s.push('\\');
                        s.push(other);
                    }
                }
            }
            other => s.push(other),
        }
    }
}

/// Parse a C-string: c"...\00"
fn parse_c_string(input: &mut Input<'_>) -> PResult<String> {
    'c'.parse_next(input)?;
    '"'.parse_next(input)?;
    let mut s = String::new();
    loop {
        let ch = any.parse_next(input)?;
        match ch {
            '"' => return Ok(s),
            '\\' => {
                let h1 = any.parse_next(input)?;
                let h2 = any.parse_next(input)?;
                let mut hex = String::new();
                hex.push(h1);
                hex.push(h2);
                let byte = u8::from_str_radix(&hex, 16)
                    .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
                if byte == 0 {
                    continue; // null terminator - skip
                }
                s.push(byte as char);
            }
            other => s.push(other),
        }
    }
}

fn parse_type(input: &mut Input<'_>) -> PResult<Type> {
    let ty = parse_base_type(input)?;
    // Check for pointer suffix `*`
    if input.starts_with('*') {
        '*'.parse_next(input)?;
        match ty {
            Type::Named(name) => Ok(Type::NamedPtr(name)),
            other => Ok(Type::TypedPtr(Box::new(other))),
        }
    } else {
        Ok(ty)
    }
}

fn parse_base_type(input: &mut Input<'_>) -> PResult<Type> {
    if input.starts_with("void") {
        literal("void").parse_next(input)?;
        return Ok(Type::Void);
    }
    if input.starts_with("half") {
        literal("half").parse_next(input)?;
        return Ok(Type::Half);
    }
    if input.starts_with("float") {
        literal("float").parse_next(input)?;
        return Ok(Type::Float);
    }
    if input.starts_with("double") {
        literal("double").parse_next(input)?;
        return Ok(Type::Double);
    }
    if input.starts_with("ptr") {
        literal("ptr").parse_next(input)?;
        return Ok(Type::Ptr);
    }
    if input.starts_with('i') {
        'i'.parse_next(input)?;
        let n = parse_u32(input)?;
        return Ok(Type::Integer(n));
    }
    if input.starts_with('[') {
        '['.parse_next(input)?;
        ws_no_newline(input)?;
        let count = parse_u64(input)?;
        ws_no_newline(input)?;
        literal("x").parse_next(input)?;
        ws_no_newline(input)?;
        let elem = parse_type(input)?;
        ws_no_newline(input)?;
        ']'.parse_next(input)?;
        return Ok(Type::Array(count, Box::new(elem)));
    }
    if input.starts_with('%') {
        '%'.parse_next(input)?;
        let name = identifier_chars(input)?;
        return Ok(Type::Named(name));
    }
    Err(ErrMode::Backtrack(ContextError::new()))
}

/// Parse a typed operand like `i64 42`, `ptr null`, `ptr @name`, etc.
fn parse_typed_operand(input: &mut Input<'_>) -> PResult<Operand> {
    // LocalRef shorthand: %name (no type prefix when used in `ret %name`)
    if input.starts_with('%') {
        '%'.parse_next(input)?;
        let name = identifier_chars(input)?;
        return Ok(Operand::LocalRef(name));
    }

    let ty = parse_type(input)?;
    ws_no_newline(input)?;

    parse_operand_value_with_type(&ty, input)
}

fn parse_operand_value_with_type(ty: &Type, input: &mut Input<'_>) -> PResult<Operand> {
    match ty {
        Type::Ptr => {
            if input.starts_with("null") {
                literal("null").parse_next(input)?;
                Ok(Operand::NullPtr)
            } else if input.starts_with("inttoptr") {
                parse_inttoptr_expr(input)
            } else if input.starts_with("getelementptr") {
                parse_gep_expr(input)
            } else if input.starts_with('@') {
                '@'.parse_next(input)?;
                let name = identifier_chars(input)?;
                Ok(Operand::GlobalRef(name))
            } else if input.starts_with('%') {
                '%'.parse_next(input)?;
                let name = identifier_chars(input)?;
                Ok(Operand::TypedLocalRef(name, ty.clone()))
            } else {
                Err(ErrMode::Backtrack(ContextError::new()))
            }
        }
        Type::NamedPtr(_) => {
            if input.starts_with("inttoptr") {
                parse_inttoptr_expr_with_type(ty, input)
            } else if input.starts_with('%') {
                '%'.parse_next(input)?;
                let name = identifier_chars(input)?;
                Ok(Operand::TypedLocalRef(name, ty.clone()))
            } else {
                Err(ErrMode::Backtrack(ContextError::new()))
            }
        }
        Type::TypedPtr(_) => {
            if input.starts_with("null") {
                literal("null").parse_next(input)?;
                Ok(Operand::NullPtr)
            } else if input.starts_with("getelementptr") {
                parse_gep_expr(input)
            } else if input.starts_with('%') {
                '%'.parse_next(input)?;
                let name = identifier_chars(input)?;
                Ok(Operand::TypedLocalRef(name, ty.clone()))
            } else {
                Err(ErrMode::Backtrack(ContextError::new()))
            }
        }
        Type::Integer(_) => {
            if input.starts_with('%') {
                '%'.parse_next(input)?;
                let name = identifier_chars(input)?;
                Ok(Operand::TypedLocalRef(name, ty.clone()))
            } else {
                parse_int_or_bool_value(ty, input)
            }
        }
        Type::Half | Type::Float | Type::Double => {
            if input.starts_with('%') {
                '%'.parse_next(input)?;
                let name = identifier_chars(input)?;
                Ok(Operand::TypedLocalRef(name, ty.clone()))
            } else {
                let f = parse_float_literal(input)?;
                Ok(Operand::float_const(ty.clone(), f))
            }
        }
        _ => Err(ErrMode::Backtrack(ContextError::new())),
    }
}

/// Parse an untyped operand given the expected type context
fn parse_untyped_operand(ty: &Type, input: &mut Input<'_>) -> PResult<Operand> {
    if input.starts_with('%') {
        '%'.parse_next(input)?;
        let name = identifier_chars(input)?;
        return Ok(Operand::TypedLocalRef(name, ty.clone()));
    }
    if input.starts_with('@') {
        '@'.parse_next(input)?;
        let name = identifier_chars(input)?;
        return Ok(Operand::GlobalRef(name));
    }
    if input.starts_with("null") {
        literal("null").parse_next(input)?;
        return Ok(Operand::NullPtr);
    }
    if input.starts_with("inttoptr") {
        if matches!(ty, Type::Ptr | Type::NamedPtr(_) | Type::TypedPtr(_)) {
            return parse_inttoptr_expr_with_type(ty, input);
        }
        return parse_inttoptr_expr(input);
    }
    if input.starts_with("getelementptr") {
        return parse_gep_expr(input);
    }
    if input.starts_with("true") {
        literal("true").parse_next(input)?;
        return Ok(Operand::IntConst(ty.clone(), 1));
    }
    if input.starts_with("false") {
        literal("false").parse_next(input)?;
        return Ok(Operand::IntConst(ty.clone(), 0));
    }

    if ty.is_floating_point() {
        let f = parse_float_literal(input)?;
        Ok(Operand::float_const(ty.clone(), f))
    } else {
        let val = parse_integer(input)?;
        Ok(Operand::IntConst(ty.clone(), val))
    }
}

fn parse_int_or_bool_value(ty: &Type, input: &mut Input<'_>) -> PResult<Operand> {
    if input.starts_with("true") {
        literal("true").parse_next(input)?;
        return Ok(Operand::IntConst(ty.clone(), 1));
    }
    if input.starts_with("false") {
        literal("false").parse_next(input)?;
        return Ok(Operand::IntConst(ty.clone(), 0));
    }
    let val = parse_integer(input)?;
    Ok(Operand::IntConst(ty.clone(), val))
}

fn parse_inttoptr_expr(input: &mut Input<'_>) -> PResult<Operand> {
    literal("inttoptr").parse_next(input)?;
    ws_no_newline(input)?;
    '('.parse_next(input)?;
    ws_no_newline(input)?;
    literal("i64").parse_next(input)?;
    ws_no_newline(input)?;
    let val = parse_integer(input)?;
    ws_no_newline(input)?;
    literal("to").parse_next(input)?;
    ws_no_newline(input)?;
    let target_ty = parse_type(input)?;
    ws_no_newline(input)?;
    ')'.parse_next(input)?;
    Ok(Operand::IntToPtr(val, target_ty))
}

fn parse_inttoptr_expr_with_type(ty: &Type, input: &mut Input<'_>) -> PResult<Operand> {
    literal("inttoptr").parse_next(input)?;
    ws_no_newline(input)?;
    '('.parse_next(input)?;
    ws_no_newline(input)?;
    literal("i64").parse_next(input)?;
    ws_no_newline(input)?;
    let val = parse_integer(input)?;
    ws_no_newline(input)?;
    literal("to").parse_next(input)?;
    ws_no_newline(input)?;
    let target_ty = parse_type(input)?;
    if target_ty != *ty {
        return Err(ErrMode::Cut(ContextError::new()));
    }
    ws_no_newline(input)?;
    ')'.parse_next(input)?;
    Ok(Operand::IntToPtr(val, ty.clone()))
}

fn parse_gep_expr(input: &mut Input<'_>) -> PResult<Operand> {
    literal("getelementptr").parse_next(input)?;
    ws_no_newline(input)?;
    literal("inbounds").parse_next(input)?;
    ws_no_newline(input)?;
    '('.parse_next(input)?;
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let ptr_ty = parse_type(input)?;
    ws_no_newline(input)?;
    '@'.parse_next(input)?;
    let ptr = identifier_chars(input)?;

    let mut indices = Vec::new();
    ws_no_newline(input)?;
    while input.starts_with(',') {
        ','.parse_next(input)?;
        ws_no_newline(input)?;
        let idx = parse_typed_operand(input)?;
        indices.push(idx);
        ws_no_newline(input)?;
    }
    ')'.parse_next(input)?;

    Ok(Operand::GetElementPtr {
        ty,
        ptr,
        ptr_ty,
        indices,
    })
}

/// Parse GEP instruction body (keyword already consumed by dispatch):
/// `[inbounds] <pointee_ty>, <ptr_ty> <ptr>, <idx_ty> <idx>, ...`
fn parse_gep_instruction_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let inbounds = if input.starts_with("inbounds") {
        literal("inbounds").parse_next(input)?;
        ws_no_newline(input)?;
        true
    } else {
        false
    };
    let pointee_ty = parse_type(input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let ptr_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let ptr = parse_untyped_operand(&ptr_ty, input)?;

    let mut indices = Vec::new();
    ws_no_newline(input)?;
    while input.starts_with(',') {
        ','.parse_next(input)?;
        ws_no_newline(input)?;
        let idx = parse_typed_operand(input)?;
        indices.push(idx);
        ws_no_newline(input)?;
    }

    Ok(Instruction::GetElementPtr {
        inbounds,
        pointee_ty,
        ptr_ty,
        ptr,
        indices,
        result: result.to_string(),
    })
}

fn parse_ret(input: &mut Input<'_>) -> PResult<Instruction> {
    literal("ret").parse_next(input)?;
    ws_no_newline(input)?;
    if input.starts_with("void") {
        literal("void").parse_next(input)?;
        Ok(Instruction::Ret(None))
    } else {
        let operand = parse_typed_operand(input)?;
        Ok(Instruction::Ret(Some(operand)))
    }
}

fn parse_br(input: &mut Input<'_>) -> PResult<Instruction> {
    literal("br").parse_next(input)?;
    ws_no_newline(input)?;

    if input.starts_with("label") {
        // Unconditional: br label %dest
        literal("label").parse_next(input)?;
        ws_no_newline(input)?;
        '%'.parse_next(input)?;
        let dest = identifier_chars(input)?;
        return Ok(Instruction::Jump { dest });
    }

    // Conditional: br i1 %cond, label %true, label %false
    let cond_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let cond = parse_untyped_operand(&cond_ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    literal("label").parse_next(input)?;
    ws_no_newline(input)?;
    '%'.parse_next(input)?;
    let true_dest = identifier_chars(input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    literal("label").parse_next(input)?;
    ws_no_newline(input)?;
    '%'.parse_next(input)?;
    let false_dest = identifier_chars(input)?;

    Ok(Instruction::Br {
        cond_ty,
        cond,
        true_dest,
        false_dest,
    })
}

fn parse_binop(op: BinOpKind, result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    let lhs = parse_untyped_operand(&ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let rhs = parse_untyped_operand(&ty, input)?;

    Ok(Instruction::BinOp {
        op,
        ty,
        lhs,
        rhs,
        result: result.to_string(),
    })
}

fn parse_int_predicate(input: &mut Input<'_>) -> PResult<IntPredicate> {
    let kw: &str = take_while(1.., |c: char| c.is_ascii_alphabetic()).parse_next(input)?;
    match kw {
        "eq" => Ok(IntPredicate::Eq),
        "ne" => Ok(IntPredicate::Ne),
        "sgt" => Ok(IntPredicate::Sgt),
        "sge" => Ok(IntPredicate::Sge),
        "slt" => Ok(IntPredicate::Slt),
        "sle" => Ok(IntPredicate::Sle),
        "ult" => Ok(IntPredicate::Ult),
        "ule" => Ok(IntPredicate::Ule),
        "ugt" => Ok(IntPredicate::Ugt),
        "uge" => Ok(IntPredicate::Uge),
        _ => Err(ErrMode::Backtrack(ContextError::new())),
    }
}

fn parse_float_predicate(input: &mut Input<'_>) -> PResult<FloatPredicate> {
    let kw: &str = take_while(1.., |c: char| c.is_ascii_alphabetic()).parse_next(input)?;
    match kw {
        "oeq" => Ok(FloatPredicate::Oeq),
        "ogt" => Ok(FloatPredicate::Ogt),
        "oge" => Ok(FloatPredicate::Oge),
        "olt" => Ok(FloatPredicate::Olt),
        "ole" => Ok(FloatPredicate::Ole),
        "one" => Ok(FloatPredicate::One),
        "ord" => Ok(FloatPredicate::Ord),
        "uno" => Ok(FloatPredicate::Uno),
        "ueq" => Ok(FloatPredicate::Ueq),
        "ugt" => Ok(FloatPredicate::Ugt),
        "uge" => Ok(FloatPredicate::Uge),
        "ult" => Ok(FloatPredicate::Ult),
        "ule" => Ok(FloatPredicate::Ule),
        "une" => Ok(FloatPredicate::Une),
        _ => Err(ErrMode::Backtrack(ContextError::new())),
    }
}

fn parse_cast(kind: CastKind, result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let from_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let value = parse_untyped_operand(&from_ty, input)?;
    ws_no_newline(input)?;
    literal("to").parse_next(input)?;
    ws_no_newline(input)?;
    let to_ty = parse_type(input)?;

    Ok(Instruction::Cast {
        op: kind,
        from_ty,
        to_ty,
        value,
        result: result.to_string(),
    })
}

fn parse_switch(input: &mut Input<'_>) -> PResult<Instruction> {
    literal("switch").parse_next(input)?;
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    let value = parse_untyped_operand(&ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    literal("label").parse_next(input)?;
    ws_no_newline(input)?;
    '%'.parse_next(input)?;
    let default_dest = identifier_chars(input)?;
    ws_no_newline(input)?;
    '['.parse_next(input)?;

    let mut cases = Vec::new();
    loop {
        ws_and_comments(input)?;
        if input.starts_with(']') {
            ']'.parse_next(input)?;
            break;
        }
        let _case_ty = parse_type(input)?;
        ws_no_newline(input)?;
        let case_val = parse_integer(input)?;
        ws_no_newline(input)?;
        ','.parse_next(input)?;
        ws_no_newline(input)?;
        literal("label").parse_next(input)?;
        ws_no_newline(input)?;
        '%'.parse_next(input)?;
        let dest = identifier_chars(input)?;
        cases.push((case_val, dest));
    }

    Ok(Instruction::Switch {
        ty,
        value,
        default_dest,
        cases,
    })
}

fn parse_call(result: Option<&str>, input: &mut Input<'_>) -> PResult<Instruction> {
    literal("call").parse_next(input)?;
    ws_no_newline(input)?;
    let ret_type = parse_type(input)?;
    ws_no_newline(input)?;
    '@'.parse_next(input)?;
    let callee = identifier_chars(input)?;
    '('.parse_next(input)?;

    let mut args = Vec::new();
    ws_no_newline(input)?;
    if !input.starts_with(')') {
        loop {
            ws_no_newline(input)?;
            let ty = parse_type(input)?;
            ws_no_newline(input)?;
            let op = parse_untyped_operand(&ty, input)?;
            args.push((ty, op));
            ws_no_newline(input)?;
            if input.starts_with(',') {
                ','.parse_next(input)?;
            } else {
                break;
            }
        }
    }
    ')'.parse_next(input)?;

    let mut attr_refs = Vec::new();
    ws_no_newline(input)?;
    while input.starts_with('#') {
        '#'.parse_next(input)?;
        let id = parse_u32(input)?;
        attr_refs.push(id);
        ws_no_newline(input)?;
    }

    let (return_ty, result_name) = if ret_type == Type::Void {
        (None, None)
    } else {
        (Some(ret_type), result.map(String::from))
    };

    Ok(Instruction::Call {
        return_ty,
        callee,
        args,
        result: result_name,
        attr_refs,
    })
}

fn parse_store(input: &mut Input<'_>) -> PResult<Instruction> {
    literal("store").parse_next(input)?;
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    let value = parse_untyped_operand(&ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let ptr_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let ptr = parse_untyped_operand(&ptr_ty, input)?;

    Ok(Instruction::Store {
        ty,
        value,
        ptr_ty,
        ptr,
    })
}

fn parse_icmp_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let pred = parse_int_predicate(input)?;
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    let lhs = parse_untyped_operand(&ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let rhs = parse_untyped_operand(&ty, input)?;

    Ok(Instruction::ICmp {
        pred,
        ty,
        lhs,
        rhs,
        result: result.to_string(),
    })
}

fn parse_fcmp_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let pred = parse_float_predicate(input)?;
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    let lhs = parse_untyped_operand(&ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let rhs = parse_untyped_operand(&ty, input)?;

    Ok(Instruction::FCmp {
        pred,
        ty,
        lhs,
        rhs,
        result: result.to_string(),
    })
}

fn parse_select_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let _cond_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let cond = parse_untyped_operand(&Type::Integer(1), input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    let true_val = parse_untyped_operand(&ty, input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let _false_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let false_val = parse_untyped_operand(&ty, input)?;

    Ok(Instruction::Select {
        cond,
        true_val,
        false_val,
        ty,
        result: result.to_string(),
    })
}

fn parse_call_body(result: Option<&str>, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let ret_type = parse_type(input)?;
    ws_no_newline(input)?;
    '@'.parse_next(input)?;
    let callee = identifier_chars(input)?;
    '('.parse_next(input)?;

    let mut args = Vec::new();
    ws_no_newline(input)?;
    if !input.starts_with(')') {
        loop {
            ws_no_newline(input)?;
            let ty = parse_type(input)?;
            ws_no_newline(input)?;
            let op = parse_untyped_operand(&ty, input)?;
            args.push((ty, op));
            ws_no_newline(input)?;
            if input.starts_with(',') {
                ','.parse_next(input)?;
            } else {
                break;
            }
        }
    }
    ')'.parse_next(input)?;

    let mut attr_refs = Vec::new();
    ws_no_newline(input)?;
    while input.starts_with('#') {
        '#'.parse_next(input)?;
        let id = parse_u32(input)?;
        attr_refs.push(id);
        ws_no_newline(input)?;
    }

    let (return_ty, result_name) = if ret_type == Type::Void {
        (None, None)
    } else {
        (Some(ret_type), result.map(String::from))
    };

    Ok(Instruction::Call {
        return_ty,
        callee,
        args,
        result: result_name,
        attr_refs,
    })
}

fn parse_phi_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;

    let mut incoming = Vec::new();
    loop {
        ws_no_newline(input)?;
        if !input.starts_with('[') {
            break;
        }
        '['.parse_next(input)?;
        ws_no_newline(input)?;
        let val = parse_untyped_operand(&ty, input)?;
        ws_no_newline(input)?;
        ','.parse_next(input)?;
        ws_no_newline(input)?;
        '%'.parse_next(input)?;
        let block = identifier_chars(input)?;
        ws_no_newline(input)?;
        ']'.parse_next(input)?;
        incoming.push((val, block));
        ws_no_newline(input)?;
        if input.starts_with(',') {
            ','.parse_next(input)?;
        }
    }

    Ok(Instruction::Phi {
        ty,
        incoming,
        result: result.to_string(),
    })
}

fn parse_alloca_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let ty = parse_type(input)?;

    Ok(Instruction::Alloca {
        ty,
        result: result.to_string(),
    })
}

fn parse_load_body(result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;
    ','.parse_next(input)?;
    ws_no_newline(input)?;
    let ptr_ty = parse_type(input)?;
    ws_no_newline(input)?;
    let ptr = parse_untyped_operand(&ptr_ty, input)?;

    Ok(Instruction::Load {
        ty,
        ptr_ty,
        ptr,
        result: result.to_string(),
    })
}

/// Unified assignment RHS dispatcher — keyword already consumed
fn dispatch_assignment_rhs(kw: &str, result: &str, input: &mut Input<'_>) -> PResult<Instruction> {
    match kw {
        "add" => parse_binop(BinOpKind::Add, result, input),
        "sub" => parse_binop(BinOpKind::Sub, result, input),
        "mul" => parse_binop(BinOpKind::Mul, result, input),
        "sdiv" => parse_binop(BinOpKind::Sdiv, result, input),
        "srem" => parse_binop(BinOpKind::Srem, result, input),
        "shl" => parse_binop(BinOpKind::Shl, result, input),
        "ashr" => parse_binop(BinOpKind::Ashr, result, input),
        "and" => parse_binop(BinOpKind::And, result, input),
        "or" => parse_binop(BinOpKind::Or, result, input),
        "xor" => parse_binop(BinOpKind::Xor, result, input),
        "fadd" => parse_binop(BinOpKind::Fadd, result, input),
        "fsub" => parse_binop(BinOpKind::Fsub, result, input),
        "fmul" => parse_binop(BinOpKind::Fmul, result, input),
        "fdiv" => parse_binop(BinOpKind::Fdiv, result, input),
        "udiv" => parse_binop(BinOpKind::Udiv, result, input),
        "urem" => parse_binop(BinOpKind::Urem, result, input),
        "lshr" => parse_binop(BinOpKind::Lshr, result, input),
        "icmp" => parse_icmp_body(result, input),
        "fcmp" => parse_fcmp_body(result, input),
        "sitofp" => parse_cast(CastKind::Sitofp, result, input),
        "fptosi" => parse_cast(CastKind::Fptosi, result, input),
        "zext" => parse_cast(CastKind::Zext, result, input),
        "sext" => parse_cast(CastKind::Sext, result, input),
        "trunc" => parse_cast(CastKind::Trunc, result, input),
        "fpext" => parse_cast(CastKind::FpExt, result, input),
        "fptrunc" => parse_cast(CastKind::FpTrunc, result, input),
        "inttoptr" => parse_cast(CastKind::IntToPtr, result, input),
        "ptrtoint" => parse_cast(CastKind::PtrToInt, result, input),
        "bitcast" => parse_cast(CastKind::Bitcast, result, input),
        "select" => parse_select_body(result, input),
        "call" => parse_call_body(Some(result), input),
        "phi" => parse_phi_body(result, input),
        "alloca" => parse_alloca_body(result, input),
        "load" => parse_load_body(result, input),
        "getelementptr" => parse_gep_instruction_body(result, input),
        _ => Err(ErrMode::Backtrack(ContextError::new())),
    }
}

fn parse_instruction(input: &mut Input<'_>) -> PResult<Instruction> {
    ws_no_newline(input)?;

    // Check for assignment: %result = ...
    if input.starts_with('%') {
        // Try parsing as assignment
        let checkpoint = *input;
        '%'.parse_next(input)?;
        let result_name = identifier_chars(input)?;
        ws_no_newline(input)?;
        if input.starts_with('=') {
            '='.parse_next(input)?;
            ws_no_newline(input)?;
            // Read keyword
            let kw: String = take_while(1.., |c: char| c.is_ascii_alphanumeric())
                .map(String::from)
                .parse_next(input)?;
            return dispatch_assignment_rhs(&kw, &result_name, input);
        }
        // Not an assignment, restore
        *input = checkpoint;
    }

    // Non-assignment instructions
    if input.starts_with("ret") {
        return parse_ret(input);
    }
    if input.starts_with("br") {
        return parse_br(input);
    }
    if input.starts_with("call") {
        return parse_call(None, input);
    }
    if input.starts_with("store") {
        return parse_store(input);
    }
    if input.starts_with("switch") {
        return parse_switch(input);
    }
    if input.starts_with("unreachable") {
        literal("unreachable").parse_next(input)?;
        return Ok(Instruction::Unreachable);
    }

    Err(ErrMode::Backtrack(ContextError::new()))
}

fn parse_block_label(input: &mut Input<'_>) -> PResult<String> {
    let label: &str = take_while(1.., |c: char| c != ':' && c != '\n' && c != '}')
        .context(StrContext::Label("block label"))
        .parse_next(input)?;
    ':'.parse_next(input)?;
    Ok(label.to_string())
}

fn is_label_start(input: &Input<'_>) -> bool {
    // Labels start at column 0.  Instructions are indented (leading space).
    // A label is: non-whitespace chars followed by ':'
    if let Some(ch) = input.chars().next()
        && (ch.is_ascii_alphanumeric() || ch == '_')
    {
        // Scan ahead for ':'
        for c in input.chars() {
            if c == ':' {
                return true;
            }
            if c == '\n' || c == ' ' || c == '=' {
                return false;
            }
        }
    }
    false
}

fn parse_instructions(input: &mut Input<'_>) -> PResult<Vec<Instruction>> {
    let mut instructions = Vec::new();

    loop {
        ws_and_comments(input)?;

        if input.is_empty() || input.starts_with('}') {
            break;
        }

        // Check if this is a label (next block)
        if is_label_start(input) {
            break;
        }

        instructions.push(parse_instruction(input)?);
    }

    Ok(instructions)
}

fn parse_basic_blocks(input: &mut Input<'_>) -> PResult<Vec<BasicBlock>> {
    let mut blocks = Vec::new();

    loop {
        ws_and_comments(input)?;

        if input.starts_with('}') || input.is_empty() {
            break;
        }

        // If this looks like a label (identifier at column 0 followed by ':'),
        // parse it. Otherwise, the first block has an implicit label.
        let label = if is_label_start(input) {
            parse_block_label(input)?
        } else if blocks.is_empty() {
            // LLVM IR allows the first basic block to omit the label.
            // Use an implicit "0" label (LLVM default for unnamed blocks).
            "0".to_string()
        } else {
            // Non-first block without a label is unexpected — stop.
            break;
        };

        let instructions = parse_instructions(input)?;
        blocks.push(BasicBlock {
            name: label,
            instructions,
        });
    }

    Ok(blocks)
}

fn parse_source_filename(input: &mut Input<'_>) -> PResult<String> {
    literal("source_filename").parse_next(input)?;
    ws_no_newline(input)?;
    '='.parse_next(input)?;
    ws_no_newline(input)?;
    parse_quoted_string(input)
}

fn parse_target_directive(module: &mut Module, input: &mut Input<'_>) -> PResult<()> {
    literal("target").parse_next(input)?;
    ws_no_newline(input)?;
    if input.starts_with("datalayout") {
        literal("datalayout").parse_next(input)?;
        ws_no_newline(input)?;
        '='.parse_next(input)?;
        ws_no_newline(input)?;
        module.target_datalayout = Some(parse_quoted_string(input)?);
    } else if input.starts_with("triple") {
        literal("triple").parse_next(input)?;
        ws_no_newline(input)?;
        '='.parse_next(input)?;
        ws_no_newline(input)?;
        module.target_triple = Some(parse_quoted_string(input)?);
    } else {
        return Err(ErrMode::Backtrack(ContextError::new()));
    }
    Ok(())
}

fn parse_struct_type(input: &mut Input<'_>) -> PResult<StructType> {
    '%'.parse_next(input)?;
    let name = identifier_chars(input)?;
    ws_no_newline(input)?;
    '='.parse_next(input)?;
    ws_no_newline(input)?;
    literal("type").parse_next(input)?;
    ws_no_newline(input)?;
    let is_opaque = if input.starts_with("opaque") {
        literal("opaque").parse_next(input)?;
        true
    } else {
        '{'.parse_next(input)?;
        '}'.parse_next(input)?;
        false
    };
    Ok(StructType { name, is_opaque })
}

fn parse_global(input: &mut Input<'_>) -> PResult<GlobalVariable> {
    '@'.parse_next(input)?;
    let name = identifier_chars(input)?;
    ws_no_newline(input)?;
    '='.parse_next(input)?;
    ws_no_newline(input)?;

    let linkage = if input.starts_with("internal") {
        literal("internal").parse_next(input)?;
        Linkage::Internal
    } else if input.starts_with("external") {
        literal("external").parse_next(input)?;
        Linkage::External
    } else {
        return Err(ErrMode::Backtrack(ContextError::new()));
    };

    ws_no_newline(input)?;

    let is_constant = if input.starts_with("constant") {
        literal("constant").parse_next(input)?;
        true
    } else if input.starts_with("global") {
        literal("global").parse_next(input)?;
        false
    } else {
        return Err(ErrMode::Backtrack(ContextError::new()));
    };

    ws_no_newline(input)?;
    let ty = parse_type(input)?;
    ws_no_newline(input)?;

    let initializer = if input.starts_with('c') && input.get(1..2) == Some("\"") {
        Some(Constant::CString(parse_c_string(input)?))
    } else if input.starts_with("null") {
        literal("null").parse_next(input)?;
        Some(Constant::Null)
    } else if ty.is_floating_point() {
        Some(Constant::float(ty.clone(), parse_float_literal(input)?))
    } else if input
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '-')
    {
        Some(Constant::Int(parse_integer(input)?))
    } else {
        None
    };

    Ok(GlobalVariable {
        name,
        ty,
        linkage,
        is_constant,
        initializer,
    })
}

fn parse_declaration(input: &mut Input<'_>) -> PResult<Function> {
    literal("declare").parse_next(input)?;
    ws_no_newline(input)?;
    let return_type = parse_type(input)?;
    ws_no_newline(input)?;
    '@'.parse_next(input)?;
    let name = identifier_chars(input)?;
    '('.parse_next(input)?;
    let params = parse_param_list(input)?;
    ')'.parse_next(input)?;

    let mut attribute_group_refs = Vec::new();
    ws_no_newline(input)?;
    while input.starts_with('#') {
        '#'.parse_next(input)?;
        let id = parse_u32(input)?;
        attribute_group_refs.push(id);
        ws_no_newline(input)?;
    }

    Ok(Function {
        name,
        return_type,
        params,
        is_declaration: true,
        attribute_group_refs,
        basic_blocks: Vec::new(),
    })
}

fn parse_definition(input: &mut Input<'_>) -> PResult<Function> {
    literal("define").parse_next(input)?;
    ws_no_newline(input)?;
    let return_type = parse_type(input)?;
    ws_no_newline(input)?;
    '@'.parse_next(input)?;
    let name = identifier_chars(input)?;
    '('.parse_next(input)?;
    let params = parse_param_list(input)?;
    ')'.parse_next(input)?;

    let mut attribute_group_refs = Vec::new();
    ws_no_newline(input)?;
    while input.starts_with('#') {
        '#'.parse_next(input)?;
        let id = parse_u32(input)?;
        attribute_group_refs.push(id);
        ws_no_newline(input)?;
    }

    ws_no_newline(input)?;
    '{'.parse_next(input)?;

    let basic_blocks = parse_basic_blocks(input)?;

    ws_and_comments(input)?;
    '}'.parse_next(input)?;

    Ok(Function {
        name,
        return_type,
        params,
        is_declaration: false,
        attribute_group_refs,
        basic_blocks,
    })
}

fn parse_param_list(input: &mut Input<'_>) -> PResult<Vec<Param>> {
    let mut params = Vec::new();
    ws_no_newline(input)?;
    if input.starts_with(')') {
        return Ok(params);
    }
    loop {
        ws_no_newline(input)?;
        let ty = parse_type(input)?;
        ws_no_newline(input)?;
        let name = if input.starts_with('%') {
            '%'.parse_next(input)?;
            Some(identifier_chars(input)?)
        } else {
            None
        };
        params.push(Param { ty, name });
        ws_no_newline(input)?;
        if input.starts_with(',') {
            ','.parse_next(input)?;
        } else {
            break;
        }
    }
    Ok(params)
}

fn parse_attribute_group(input: &mut Input<'_>) -> PResult<AttributeGroup> {
    literal("attributes").parse_next(input)?;
    ws_no_newline(input)?;
    '#'.parse_next(input)?;
    let id = parse_u32(input)?;
    ws_no_newline(input)?;
    '='.parse_next(input)?;
    ws_no_newline(input)?;
    '{'.parse_next(input)?;
    ws_no_newline(input)?;

    let mut attributes = Vec::new();
    while !input.starts_with('}') {
        let key = parse_quoted_string(input)?;
        ws_no_newline(input)?;
        if input.starts_with('=') {
            '='.parse_next(input)?;
            let value = parse_quoted_string(input)?;
            attributes.push(Attribute::KeyValue(key, value));
        } else {
            attributes.push(Attribute::StringAttr(key));
        }
        ws_no_newline(input)?;
    }
    '}'.parse_next(input)?;

    Ok(AttributeGroup { id, attributes })
}

fn parse_named_metadata(input: &mut Input<'_>) -> PResult<NamedMetadata> {
    '!'.parse_next(input)?;
    let name = identifier_chars(input)?;
    ws_no_newline(input)?;
    '='.parse_next(input)?;
    ws_no_newline(input)?;
    '!'.parse_next(input)?;
    '{'.parse_next(input)?;

    let mut node_refs = Vec::new();
    ws_no_newline(input)?;
    if !input.starts_with('}') {
        loop {
            ws_no_newline(input)?;
            '!'.parse_next(input)?;
            let id = parse_u32(input)?;
            node_refs.push(id);
            ws_no_newline(input)?;
            if input.starts_with(',') {
                ','.parse_next(input)?;
            } else {
                break;
            }
        }
    }
    '}'.parse_next(input)?;

    Ok(NamedMetadata { name, node_refs })
}

fn parse_metadata_node(input: &mut Input<'_>) -> PResult<MetadataNode> {
    '!'.parse_next(input)?;
    let id = parse_u32(input)?;
    ws_no_newline(input)?;
    '='.parse_next(input)?;
    ws_no_newline(input)?;
    '!'.parse_next(input)?;
    '{'.parse_next(input)?;

    let values = parse_metadata_values(input)?;

    '}'.parse_next(input)?;

    Ok(MetadataNode { id, values })
}

fn parse_metadata_values(input: &mut Input<'_>) -> PResult<Vec<MetadataValue>> {
    let mut values = Vec::new();
    ws_no_newline(input)?;
    if input.starts_with('}') {
        return Ok(values);
    }
    loop {
        ws_no_newline(input)?;
        let val = parse_metadata_value(input)?;
        values.push(val);
        ws_no_newline(input)?;
        if input.starts_with(',') {
            ','.parse_next(input)?;
        } else {
            break;
        }
    }
    Ok(values)
}

fn parse_metadata_value(input: &mut Input<'_>) -> PResult<MetadataValue> {
    if input.starts_with('!') {
        '!'.parse_next(input)?;
        if input.starts_with('"') {
            // !"string"
            let s = parse_quoted_string(input)?;
            return Ok(MetadataValue::String(s));
        }
        if input.starts_with('{') {
            // !{...} sublist
            '{'.parse_next(input)?;
            let vals = parse_metadata_values(input)?;
            '}'.parse_next(input)?;
            return Ok(MetadataValue::SubList(vals));
        }
        // !N node reference
        let id = parse_u32(input)?;
        return Ok(MetadataValue::NodeRef(id));
    }

    // Typed value: i32 42, i1 true, etc.
    let ty = parse_type(input)?;
    ws_no_newline(input)?;

    if let Type::Integer(1) = &ty {
        if input.starts_with("true") {
            literal("true").parse_next(input)?;
            return Ok(MetadataValue::Int(ty, 1));
        }
        if input.starts_with("false") {
            literal("false").parse_next(input)?;
            return Ok(MetadataValue::Int(ty, 0));
        }
    }

    let val = parse_integer(input)?;
    Ok(MetadataValue::Int(ty, val))
}

fn parse_module_inner(input: &mut Input<'_>) -> PResult<Module> {
    let mut module = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    loop {
        ws_and_comments(input)?;

        if input.is_empty() {
            break;
        }

        if input.starts_with("source_filename") {
            module.source_filename = Some(parse_source_filename(input)?);
        } else if input.starts_with("target") {
            parse_target_directive(&mut module, input)?;
        } else if input.starts_with('%') {
            module.struct_types.push(parse_struct_type(input)?);
        } else if input.starts_with('@') {
            module.globals.push(parse_global(input)?);
        } else if input.starts_with("declare") {
            module.functions.push(parse_declaration(input)?);
        } else if input.starts_with("define") {
            module.functions.push(parse_definition(input)?);
        } else if input.starts_with("attributes") {
            module.attribute_groups.push(parse_attribute_group(input)?);
        } else if input.starts_with('!') {
            // Could be named metadata or numbered metadata node
            if input
                .get(1..2)
                .is_some_and(|c| c.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            {
                module.metadata_nodes.push(parse_metadata_node(input)?);
            } else {
                module.named_metadata.push(parse_named_metadata(input)?);
            }
        } else {
            return Err(ErrMode::Backtrack(ContextError::new()));
        }
    }

    Ok(module)
}

fn format_read_diagnostics(diagnostics: &[ReadDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

fn parse_module_with_policy(input: &str, _policy: ReadPolicy) -> Result<Module, ReadDiagnostic> {
    let mut inp: Input<'_> = input;
    parse_module_inner
        .parse_next(&mut inp)
        .map_err(|error| ReadDiagnostic {
            kind: ReadDiagnosticKind::MalformedInput,
            offset: Some(input.len().saturating_sub(inp.len())),
            context: "text IR",
            message: format!("parse error: {error}"),
        })
}

pub fn parse_module_detailed(
    input: &str,
    policy: ReadPolicy,
) -> Result<Module, Vec<ReadDiagnostic>> {
    parse_module_with_policy(input, policy).map_err(|error| vec![error])
}

/// Parse LLVM text IR into a `Module`.
///
/// This is a drop-in replacement for the hand-written recursive-descent parser
/// in `text_reader.rs`, implemented using winnow combinators.
pub fn parse_module(input: &str) -> Result<Module, String> {
    parse_module_detailed(input, ReadPolicy::QirSubsetStrict)
        .map_err(|diagnostics| format_read_diagnostics(&diagnostics))
}

pub fn parse_module_compatibility(input: &str) -> Result<Module, String> {
    parse_module_detailed(input, ReadPolicy::Compatibility)
        .map_err(|diagnostics| format_read_diagnostics(&diagnostics))
}
