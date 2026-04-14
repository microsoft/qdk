// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rustc_hash::FxHashSet;

use crate::model::{Constant, Instruction, Module, Operand};

use super::config::QirSmithError;

pub(super) fn ensure_text_roundtrip_matches(
    original: &Module,
    reparsed: &Module,
) -> Result<(), QirSmithError> {
    if original == reparsed {
        Ok(())
    } else {
        Err(QirSmithError::TextRoundTrip(
            "text roundtrip changed module structure for the supported v1 subset".to_string(),
        ))
    }
}

fn is_synthetic_param_name(name: &str, param_index: usize) -> bool {
    name.strip_prefix("param_")
        .and_then(|suffix| suffix.parse::<usize>().ok())
        .is_some_and(|index| index == param_index)
}

fn param_names_semantically_equal(
    expected: Option<&str>,
    actual: Option<&str>,
    param_index: usize,
) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (None, Some(name)) => is_synthetic_param_name(name, param_index),
        (Some(expected_name), Some(actual_name)) => expected_name == actual_name,
        (Some(_), None) => false,
    }
}

fn local_ref_name(operand: &Operand) -> Option<&str> {
    match operand {
        Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => Some(name),
        _ => None,
    }
}

fn constants_semantically_equal(expected: &Constant, actual: &Constant) -> bool {
    match (expected, actual) {
        (Constant::CString(expected_text), Constant::CString(actual_text)) => {
            expected_text == actual_text
        }
        (Constant::Int(expected_value), Constant::Int(actual_value)) => {
            expected_value == actual_value
        }
        (
            Constant::Float(expected_ty, expected_value),
            Constant::Float(actual_ty, actual_value),
        ) => expected_ty == actual_ty && expected_value.to_bits() == actual_value.to_bits(),
        (Constant::Null, Constant::Null) => true,
        _ => false,
    }
}

fn optional_constants_semantically_equal(
    expected: Option<&Constant>,
    actual: Option<&Constant>,
) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected_constant), Some(actual_constant)) => {
            constants_semantically_equal(expected_constant, actual_constant)
        }
        _ => false,
    }
}

fn operands_semantically_equal(expected: &Operand, actual: &Operand) -> bool {
    if let (Some(expected_name), Some(actual_name)) =
        (local_ref_name(expected), local_ref_name(actual))
    {
        return expected_name == actual_name;
    }

    match (expected, actual) {
        (
            Operand::IntConst(expected_ty, expected_value),
            Operand::IntConst(actual_ty, actual_value),
        ) => expected_ty == actual_ty && expected_value == actual_value,
        (
            Operand::FloatConst(expected_ty, expected_value),
            Operand::FloatConst(actual_ty, actual_value),
        ) => expected_ty == actual_ty && expected_value.to_bits() == actual_value.to_bits(),
        (Operand::NullPtr, Operand::NullPtr) => true,
        (
            Operand::IntToPtr(expected_value, expected_ty),
            Operand::IntToPtr(actual_value, actual_ty),
        ) => expected_value == actual_value && expected_ty == actual_ty,
        (
            Operand::GetElementPtr {
                ty: expected_ty,
                ptr: expected_ptr,
                ptr_ty: expected_ptr_ty,
                indices: expected_indices,
            },
            Operand::GetElementPtr {
                ty: actual_ty,
                ptr: actual_ptr,
                ptr_ty: actual_ptr_ty,
                indices: actual_indices,
            },
        ) => {
            expected_ty == actual_ty
                && expected_ptr == actual_ptr
                && expected_ptr_ty == actual_ptr_ty
                && expected_indices.len() == actual_indices.len()
                && expected_indices.iter().zip(actual_indices.iter()).all(
                    |(expected_index, actual_index)| {
                        operands_semantically_equal(expected_index, actual_index)
                    },
                )
        }
        (Operand::GlobalRef(expected_name), Operand::GlobalRef(actual_name)) => {
            expected_name == actual_name
        }
        _ => false,
    }
}

fn optional_operands_semantically_equal(
    expected: Option<&Operand>,
    actual: Option<&Operand>,
) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected_operand), Some(actual_operand)) => {
            operands_semantically_equal(expected_operand, actual_operand)
        }
        _ => false,
    }
}

#[allow(clippy::too_many_lines)]
fn instructions_semantically_equal(expected: &Instruction, actual: &Instruction) -> bool {
    match (expected, actual) {
        (Instruction::Ret(expected_value), Instruction::Ret(actual_value)) => {
            optional_operands_semantically_equal(expected_value.as_ref(), actual_value.as_ref())
        }
        (
            Instruction::Br {
                cond_ty: expected_cond_ty,
                cond: expected_cond,
                true_dest: expected_true_dest,
                false_dest: expected_false_dest,
            },
            Instruction::Br {
                cond_ty: actual_cond_ty,
                cond: actual_cond,
                true_dest: actual_true_dest,
                false_dest: actual_false_dest,
            },
        ) => {
            expected_cond_ty == actual_cond_ty
                && expected_true_dest == actual_true_dest
                && expected_false_dest == actual_false_dest
                && operands_semantically_equal(expected_cond, actual_cond)
        }
        (
            Instruction::Jump {
                dest: expected_dest,
            },
            Instruction::Jump { dest: actual_dest },
        ) => expected_dest == actual_dest,
        (
            Instruction::BinOp {
                op: expected_op,
                ty: expected_ty,
                lhs: expected_lhs,
                rhs: expected_rhs,
                result: expected_result,
            },
            Instruction::BinOp {
                op: actual_op,
                ty: actual_ty,
                lhs: actual_lhs,
                rhs: actual_rhs,
                result: actual_result,
            },
        ) => {
            expected_op == actual_op
                && expected_ty == actual_ty
                && expected_result == actual_result
                && operands_semantically_equal(expected_lhs, actual_lhs)
                && operands_semantically_equal(expected_rhs, actual_rhs)
        }
        (
            Instruction::ICmp {
                pred: expected_pred,
                ty: expected_ty,
                lhs: expected_lhs,
                rhs: expected_rhs,
                result: expected_result,
            },
            Instruction::ICmp {
                pred: actual_pred,
                ty: actual_ty,
                lhs: actual_lhs,
                rhs: actual_rhs,
                result: actual_result,
            },
        ) => {
            expected_pred == actual_pred
                && expected_ty == actual_ty
                && expected_result == actual_result
                && operands_semantically_equal(expected_lhs, actual_lhs)
                && operands_semantically_equal(expected_rhs, actual_rhs)
        }
        (
            Instruction::FCmp {
                pred: expected_pred,
                ty: expected_ty,
                lhs: expected_lhs,
                rhs: expected_rhs,
                result: expected_result,
            },
            Instruction::FCmp {
                pred: actual_pred,
                ty: actual_ty,
                lhs: actual_lhs,
                rhs: actual_rhs,
                result: actual_result,
            },
        ) => {
            expected_pred == actual_pred
                && expected_ty == actual_ty
                && expected_result == actual_result
                && operands_semantically_equal(expected_lhs, actual_lhs)
                && operands_semantically_equal(expected_rhs, actual_rhs)
        }
        (
            Instruction::Cast {
                op: expected_op,
                from_ty: expected_from_ty,
                to_ty: expected_to_ty,
                value: expected_value,
                result: expected_result,
            },
            Instruction::Cast {
                op: actual_op,
                from_ty: actual_from_ty,
                to_ty: actual_to_ty,
                value: actual_value,
                result: actual_result,
            },
        ) => {
            expected_op == actual_op
                && expected_from_ty == actual_from_ty
                && expected_to_ty == actual_to_ty
                && expected_result == actual_result
                && operands_semantically_equal(expected_value, actual_value)
        }
        (
            Instruction::Call {
                return_ty: expected_return_ty,
                callee: expected_callee,
                args: expected_args,
                result: expected_result,
                attr_refs: expected_attr_refs,
            },
            Instruction::Call {
                return_ty: actual_return_ty,
                callee: actual_callee,
                args: actual_args,
                result: actual_result,
                attr_refs: actual_attr_refs,
            },
        ) => {
            expected_return_ty == actual_return_ty
                && expected_callee == actual_callee
                && expected_result == actual_result
                && expected_attr_refs == actual_attr_refs
                && expected_args.len() == actual_args.len()
                && expected_args.iter().zip(actual_args.iter()).all(
                    |((expected_ty, expected_operand), (actual_ty, actual_operand))| {
                        expected_ty == actual_ty
                            && operands_semantically_equal(expected_operand, actual_operand)
                    },
                )
        }
        _ => false,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn assert_bitcode_roundtrip_matches_supported_v1_subset(
    original: &Module,
    reparsed: &Module,
) -> Result<(), QirSmithError> {
    if original.source_filename != reparsed.source_filename {
        return Err(QirSmithError::BitcodeRoundTrip(
            "source_filename changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    if original.target_datalayout != reparsed.target_datalayout {
        return Err(QirSmithError::BitcodeRoundTrip(
            "target_datalayout changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    if original.target_triple != reparsed.target_triple {
        return Err(QirSmithError::BitcodeRoundTrip(
            "target_triple changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    if original.struct_types != reparsed.struct_types {
        return Err(QirSmithError::BitcodeRoundTrip(
            "struct types changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    if original.globals.len() != reparsed.globals.len() {
        return Err(QirSmithError::BitcodeRoundTrip(format!(
            "global count changed across the supported v1 bitcode roundtrip: expected {}, found {}",
            original.globals.len(),
            reparsed.globals.len()
        )));
    }

    for (global_index, (expected, actual)) in original
        .globals
        .iter()
        .zip(reparsed.globals.iter())
        .enumerate()
    {
        if expected.name != actual.name {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "global {global_index} name changed across the supported v1 bitcode roundtrip"
            )));
        }
        if expected.linkage != actual.linkage {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "global {global_index} linkage changed across the supported v1 bitcode roundtrip"
            )));
        }
        if expected.ty != actual.ty {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "global {global_index} type changed across the supported v1 bitcode roundtrip"
            )));
        }
        if expected.is_constant != actual.is_constant {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "global {global_index} mutability changed across the supported v1 bitcode roundtrip"
            )));
        }
        if !optional_constants_semantically_equal(
            expected.initializer.as_ref(),
            actual.initializer.as_ref(),
        ) {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "global {global_index} initializer changed across the supported v1 bitcode roundtrip"
            )));
        }
    }

    if original.functions.len() != reparsed.functions.len() {
        return Err(QirSmithError::BitcodeRoundTrip(format!(
            "function count changed across the supported v1 bitcode roundtrip: expected {}, found {}",
            original.functions.len(),
            reparsed.functions.len()
        )));
    }

    for (function_index, (expected, actual)) in original
        .functions
        .iter()
        .zip(reparsed.functions.iter())
        .enumerate()
    {
        if expected.name != actual.name {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "function {function_index} name changed across the supported v1 bitcode roundtrip"
            )));
        }
        if expected.is_declaration != actual.is_declaration {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "function {function_index} declaration shape changed across the supported v1 bitcode roundtrip"
            )));
        }
        if expected.return_type != actual.return_type {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "function {function_index} return type changed across the supported v1 bitcode roundtrip"
            )));
        }
        if expected.attribute_group_refs != actual.attribute_group_refs {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "function {function_index} attribute_group_refs changed across the supported v1 bitcode roundtrip"
            )));
        }

        if expected.params.len() != actual.params.len() {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "function {function_index} parameter count changed across the supported v1 bitcode roundtrip"
            )));
        }

        for (param_index, (expected_param, actual_param)) in
            expected.params.iter().zip(actual.params.iter()).enumerate()
        {
            if expected_param.ty != actual_param.ty {
                return Err(QirSmithError::BitcodeRoundTrip(format!(
                    "function {function_index} parameter {param_index} type changed across the supported v1 bitcode roundtrip"
                )));
            }
            if !param_names_semantically_equal(
                expected_param.name.as_deref(),
                actual_param.name.as_deref(),
                param_index,
            ) {
                return Err(QirSmithError::BitcodeRoundTrip(format!(
                    "function {function_index} parameter {param_index} name changed across the supported v1 bitcode roundtrip"
                )));
            }
        }

        if expected.basic_blocks.len() != actual.basic_blocks.len() {
            return Err(QirSmithError::BitcodeRoundTrip(format!(
                "function {function_index} basic block count changed across the supported v1 bitcode roundtrip"
            )));
        }

        for (block_index, (expected_block, actual_block)) in expected
            .basic_blocks
            .iter()
            .zip(actual.basic_blocks.iter())
            .enumerate()
        {
            if expected_block.name != actual_block.name {
                return Err(QirSmithError::BitcodeRoundTrip(format!(
                    "function {function_index} block {block_index} name changed across the supported v1 bitcode roundtrip"
                )));
            }

            if expected_block.instructions.len() != actual_block.instructions.len() {
                return Err(QirSmithError::BitcodeRoundTrip(format!(
                    "function {function_index} block {block_index} instruction count changed across the supported v1 bitcode roundtrip"
                )));
            }

            for (instruction_index, (expected_instruction, actual_instruction)) in expected_block
                .instructions
                .iter()
                .zip(actual_block.instructions.iter())
                .enumerate()
            {
                if !instructions_semantically_equal(expected_instruction, actual_instruction) {
                    return Err(QirSmithError::BitcodeRoundTrip(format!(
                        "function {function_index} block {block_index} instruction {instruction_index} changed across the supported v1 bitcode roundtrip"
                    )));
                }
            }
        }
    }

    let referenced_ids: FxHashSet<u32> = original
        .functions
        .iter()
        .flat_map(|function| function.attribute_group_refs.iter().copied())
        .collect();
    let original_referenced: Vec<_> = original
        .attribute_groups
        .iter()
        .filter(|group| referenced_ids.contains(&group.id))
        .collect();
    let reparsed_referenced: Vec<_> = reparsed
        .attribute_groups
        .iter()
        .filter(|group| referenced_ids.contains(&group.id))
        .collect();
    if original_referenced != reparsed_referenced {
        return Err(QirSmithError::BitcodeRoundTrip(
            "attribute_groups changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    if original.named_metadata != reparsed.named_metadata {
        return Err(QirSmithError::BitcodeRoundTrip(
            "named_metadata changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    if original.metadata_nodes != reparsed.metadata_nodes {
        return Err(QirSmithError::BitcodeRoundTrip(
            "metadata_nodes changed across the supported v1 bitcode roundtrip".to_string(),
        ));
    }

    Ok(())
}
