// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use qsc_rir::rir::{self, Operand, Ty, Variable, VariableId};

fn bool_var(id: u32) -> Variable {
    Variable {
        variable_id: VariableId(id),
        ty: Ty::Boolean,
    }
}

fn int_var(id: u32) -> Variable {
    Variable {
        variable_id: VariableId(id),
        ty: Ty::Integer,
    }
}

fn bool_ref(id: u32) -> Operand {
    Operand::Variable(bool_var(id))
}

fn int_ref(id: u32) -> Operand {
    Operand::Variable(int_var(id))
}

/// Logical AND operates on `i1` (boolean) operands.
/// Bitwise AND operates on `i64` (integer) operands.
/// Both map to LLVM `and` — the type is what distinguishes them.
#[test]
fn logical_and_produces_i1_type() {
    let instr = rir::Instruction::LogicalAnd(bool_ref(0), bool_ref(1), bool_var(2));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, .. } => {
            assert_eq!(op, BinOpKind::And);
            assert_eq!(ty, Type::Integer(1));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

#[test]
fn bitwise_and_produces_i64_type() {
    let instr = rir::Instruction::BitwiseAnd(int_ref(0), int_ref(1), int_var(2));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, .. } => {
            assert_eq!(op, BinOpKind::And);
            assert_eq!(ty, Type::Integer(64));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

#[test]
fn logical_or_produces_i1_type() {
    let instr = rir::Instruction::LogicalOr(bool_ref(0), bool_ref(1), bool_var(2));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, .. } => {
            assert_eq!(op, BinOpKind::Or);
            assert_eq!(ty, Type::Integer(1));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

#[test]
fn bitwise_or_produces_i64_type() {
    let instr = rir::Instruction::BitwiseOr(int_ref(0), int_ref(1), int_var(2));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, .. } => {
            assert_eq!(op, BinOpKind::Or);
            assert_eq!(ty, Type::Integer(64));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

/// Logical NOT is `xor i1 %val, true` (flip a boolean).
/// Bitwise NOT is `xor i64 %val, -1` (flip all 64 bits).
#[test]
fn logical_not_produces_xor_i1_with_true() {
    let instr = rir::Instruction::LogicalNot(bool_ref(0), bool_var(1));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, rhs, .. } => {
            assert_eq!(op, BinOpKind::Xor);
            assert_eq!(ty, Type::Integer(1));
            assert_eq!(rhs, super::Operand::IntConst(Type::Integer(1), 1));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

#[test]
fn bitwise_not_produces_xor_i64_with_minus_one() {
    let instr = rir::Instruction::BitwiseNot(int_ref(0), int_var(1));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, rhs, .. } => {
            assert_eq!(op, BinOpKind::Xor);
            assert_eq!(ty, Type::Integer(64));
            assert_eq!(rhs, super::Operand::IntConst(Type::Integer(64), -1));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

#[test]
fn bitwise_xor_produces_i64_type() {
    let instr = rir::Instruction::BitwiseXor(int_ref(0), int_ref(1), int_var(2));
    let result = convert_instruction(&instr, &rir::Program::default(), PointerStyle::Opaque);
    match result {
        Instruction::BinOp { op, ty, .. } => {
            assert_eq!(op, BinOpKind::Xor);
            assert_eq!(ty, Type::Integer(64));
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}
