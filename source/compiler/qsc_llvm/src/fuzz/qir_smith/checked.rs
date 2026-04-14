// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::model::{Instruction, Module, Operand, Type};

use super::{
    compare::{
        assert_bitcode_roundtrip_matches_supported_v1_subset, ensure_text_roundtrip_matches,
    },
    config::{GeneratedArtifact, QirSmithError, RoundTripKind},
    io::{emit_bitcode, emit_text, parse_bitcode_roundtrip, parse_text_roundtrip},
};

pub(super) fn populate_checked_artifact(
    artifact: &mut GeneratedArtifact,
) -> Result<(), QirSmithError> {
    validate_checked_module(
        &artifact.module,
        artifact.effective_config.allow_typed_pointers,
    )?;

    if artifact.effective_config.profile.to_qir_profile().is_some() {
        let validation = crate::validation::validate_qir_profile(&artifact.module);
        if let Some(first_error) = validation.errors.into_iter().next() {
            return Err(QirSmithError::ProfileViolation(first_error));
        }
    }

    match artifact
        .effective_config
        .roundtrip
        .unwrap_or(RoundTripKind::TextAndBitcodeSinglePass)
    {
        RoundTripKind::TextOnly => {
            let text = emit_text(&artifact.module);
            let reparsed_text = parse_text_roundtrip(&text)?;
            ensure_text_roundtrip_matches(&artifact.module, &reparsed_text)?;
            artifact.text = Some(text);
        }
        RoundTripKind::BitcodeOnly => {
            let bitcode = emit_bitcode(&artifact.module)?;
            let reparsed_bitcode = parse_bitcode_roundtrip(&bitcode)?;
            assert_bitcode_roundtrip_matches_supported_v1_subset(
                &artifact.module,
                &reparsed_bitcode,
            )?;
            artifact.bitcode = Some(bitcode);
        }
        RoundTripKind::TextAndBitcodeSinglePass => {
            let text = emit_text(&artifact.module);
            let reparsed_text = parse_text_roundtrip(&text)?;
            ensure_text_roundtrip_matches(&artifact.module, &reparsed_text)?;
            artifact.text = Some(text);
            let bitcode = emit_bitcode(&reparsed_text)?;
            let reparsed_bitcode = parse_bitcode_roundtrip(&bitcode)?;
            assert_bitcode_roundtrip_matches_supported_v1_subset(
                &reparsed_text,
                &reparsed_bitcode,
            )?;
            artifact.bitcode = Some(bitcode);
        }
    }

    Ok(())
}

fn validate_checked_module(
    module: &Module,
    allow_typed_pointers: bool,
) -> Result<(), QirSmithError> {
    let Some(entry_point) = module.functions.first() else {
        return Err(QirSmithError::ModelGeneration(
            "checked mode requires a generated entry point".to_string(),
        ));
    };

    if entry_point.name != crate::qir::ENTRYPOINT_NAME || entry_point.is_declaration {
        return Err(QirSmithError::ModelGeneration(
            "checked mode expects the generated module to start with a defined ENTRYPOINT__main"
                .to_string(),
        ));
    }

    if entry_point.return_type != Type::Integer(64) {
        return Err(QirSmithError::ModelGeneration(
            "checked mode expects ENTRYPOINT__main to return i64".to_string(),
        ));
    }

    if !entry_point.params.is_empty() {
        return Err(QirSmithError::ModelGeneration(
            "checked mode expects ENTRYPOINT__main to take no parameters".to_string(),
        ));
    }

    if module
        .functions
        .iter()
        .filter(|function| !function.is_declaration)
        .count()
        != 1
    {
        return Err(QirSmithError::ModelGeneration(
            "checked mode only supports a single defined entry point in the v1 subset".to_string(),
        ));
    }

    for global in &module.globals {
        validate_checked_type(&global.ty, allow_typed_pointers)?;
    }

    for function in &module.functions {
        validate_checked_type(&function.return_type, allow_typed_pointers)?;
        for param in &function.params {
            validate_checked_type(&param.ty, allow_typed_pointers)?;
        }
        for block in &function.basic_blocks {
            for instruction in &block.instructions {
                validate_checked_instruction(instruction, allow_typed_pointers)?;
            }
        }
    }

    Ok(())
}

fn validate_checked_type(ty: &Type, allow_typed_pointers: bool) -> Result<(), QirSmithError> {
    match ty {
        Type::Void | Type::Integer(_) | Type::Half | Type::Float | Type::Double | Type::Ptr => {
            Ok(())
        }
        Type::Label | Type::Function(_, _) => Err(QirSmithError::ModelGeneration(format!(
            "type {ty} is outside the supported checked subset"
        ))),
        Type::Array(_, element) => validate_checked_type(element, allow_typed_pointers),
        Type::NamedPtr(_) | Type::TypedPtr(_) | Type::Named(_) => {
            if allow_typed_pointers {
                Ok(())
            } else {
                Err(QirSmithError::ModelGeneration(format!(
                    "type {ty} is outside the supported opaque-pointer checked subset"
                )))
            }
        }
    }
}

fn validate_checked_operand(
    operand: &Operand,
    allow_typed_pointers: bool,
) -> Result<(), QirSmithError> {
    match operand {
        Operand::LocalRef(_)
        | Operand::TypedLocalRef(_, _)
        | Operand::FloatConst(_, _)
        | Operand::NullPtr
        | Operand::GlobalRef(_) => Ok(()),
        Operand::IntConst(ty, _) => validate_checked_type(ty, allow_typed_pointers),
        Operand::IntToPtr(_, ty) => {
            validate_checked_type(ty, allow_typed_pointers)?;
            if !allow_typed_pointers && ty != &Type::Ptr {
                return Err(QirSmithError::ModelGeneration(format!(
                    "inttoptr target type {ty} is outside the supported opaque-pointer checked subset"
                )));
            }
            Ok(())
        }
        Operand::GetElementPtr {
            ty,
            ptr_ty,
            indices,
            ..
        } => {
            if allow_typed_pointers {
                validate_checked_type(ty, allow_typed_pointers)?;
                validate_checked_type(ptr_ty, allow_typed_pointers)?;
                for idx_op in indices {
                    validate_checked_operand(idx_op, allow_typed_pointers)?;
                }
                Ok(())
            } else {
                Err(QirSmithError::ModelGeneration(
                    "getelementptr operands are outside the supported opaque-pointer checked subset"
                        .to_string(),
                ))
            }
        }
    }
}

fn validate_checked_instruction(
    instruction: &Instruction,
    allow_typed_pointers: bool,
) -> Result<(), QirSmithError> {
    match instruction {
        Instruction::Ret(value) => {
            if let Some(value) = value {
                validate_checked_operand(value, allow_typed_pointers)?;
            }
            Ok(())
        }
        Instruction::Br { cond_ty, cond, .. } => {
            validate_checked_type(cond_ty, allow_typed_pointers)?;
            validate_checked_operand(cond, allow_typed_pointers)
        }
        Instruction::Jump { .. } => Ok(()),
        Instruction::BinOp { ty, lhs, rhs, .. }
        | Instruction::ICmp { ty, lhs, rhs, .. }
        | Instruction::FCmp { ty, lhs, rhs, .. } => {
            validate_checked_type(ty, allow_typed_pointers)?;
            validate_checked_operand(lhs, allow_typed_pointers)?;
            validate_checked_operand(rhs, allow_typed_pointers)
        }
        Instruction::Cast {
            from_ty,
            to_ty,
            value,
            ..
        } => {
            validate_checked_type(from_ty, allow_typed_pointers)?;
            validate_checked_type(to_ty, allow_typed_pointers)?;
            validate_checked_operand(value, allow_typed_pointers)
        }
        Instruction::Call {
            return_ty,
            args,
            attr_refs,
            ..
        } => {
            if let Some(return_ty) = return_ty {
                validate_checked_type(return_ty, allow_typed_pointers)?;
            }
            for (ty, operand) in args {
                validate_checked_type(ty, allow_typed_pointers)?;
                validate_checked_operand(operand, allow_typed_pointers)?;
            }
            if !attr_refs.is_empty() {
                return Err(QirSmithError::ModelGeneration(
                    "call attribute references are outside the supported v1 checked subset"
                        .to_string(),
                ));
            }
            Ok(())
        }
        Instruction::Phi { .. } => Err(QirSmithError::ModelGeneration(
            "phi instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::Alloca { .. } => Err(QirSmithError::ModelGeneration(
            "alloca instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::Load { .. } => Err(QirSmithError::ModelGeneration(
            "load instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::Store { .. } => Err(QirSmithError::ModelGeneration(
            "store instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::Select { .. } => Err(QirSmithError::ModelGeneration(
            "select instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::Switch { .. } => Err(QirSmithError::ModelGeneration(
            "switch instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::GetElementPtr { .. } => Err(QirSmithError::ModelGeneration(
            "getelementptr instructions are outside the supported v1 checked subset".to_string(),
        )),
        Instruction::Unreachable => Err(QirSmithError::ModelGeneration(
            "unreachable instructions are outside the supported v1 checked subset".to_string(),
        )),
    }
}
