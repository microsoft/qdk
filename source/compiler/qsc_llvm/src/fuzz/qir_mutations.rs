// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::model::{BasicBlock, Type};
use crate::model::{Function, Instruction, Module, Operand, Param, StructType};

#[must_use]
pub fn mutate_adaptive_v1_typed_pointer_seed(seed: &Module, selector: u8) -> Module {
    let mut mutated = seed.clone();

    match selector % 3 {
        0 => {
            let Some(function) = first_defined_function(&mut mutated) else {
                return mutated;
            };
            mutate_load_pointer_operand_mismatch(function);
        }
        1 => {
            let Some(function) = first_defined_function(&mut mutated) else {
                return mutated;
            };
            mutate_store_pointer_operand_mismatch(function);
        }
        _ => mutate_named_pointer_call_arg_mismatch(&mut mutated),
    }

    mutated
}

fn first_defined_function(module: &mut Module) -> Option<&mut Function> {
    if let Some(index) = module
        .functions
        .iter()
        .position(|function| !function.is_declaration)
    {
        module.functions.get_mut(index)
    } else {
        module.functions.first_mut()
    }
}

fn mutate_load_pointer_operand_mismatch(function: &mut Function) {
    let result_name = next_available_local_name(function, "__qir_mut_load");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::Load {
                ty: Type::Integer(64),
                ptr_ty: typed_ptr(Type::Integer(64)),
                ptr: Operand::IntToPtr(0, typed_ptr(Type::Integer(8))),
                result: result_name,
            },
        );
    }
}

fn mutate_store_pointer_operand_mismatch(function: &mut Function) {
    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::Store {
                ty: Type::Integer(64),
                value: Operand::IntConst(Type::Integer(64), 0),
                ptr_ty: typed_ptr(Type::Integer(64)),
                ptr: Operand::IntToPtr(0, typed_ptr(Type::Integer(8))),
            },
        );
    }
}

fn mutate_named_pointer_call_arg_mismatch(module: &mut Module) {
    ensure_opaque_struct(module, "Qubit");

    let callee = next_available_function_name(module, "__qir_mut_named_qubit_callee");
    let qubit_ptr = Type::NamedPtr("Qubit".to_string());

    push_declaration(
        module,
        callee.clone(),
        Type::Void,
        vec![Param {
            ty: qubit_ptr.clone(),
            name: Some("qubit".to_string()),
        }],
    );

    let Some(function) = first_defined_function(module) else {
        return;
    };

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::Call {
                return_ty: None,
                callee,
                args: vec![(
                    qubit_ptr,
                    Operand::IntToPtr(0, Type::Named("Qubit".to_string())),
                )],
                result: None,
                attr_refs: Vec::new(),
            },
        );
    }
}

fn typed_ptr(inner: Type) -> Type {
    Type::TypedPtr(Box::new(inner))
}

fn ensure_opaque_struct(module: &mut Module, name: &str) {
    if module
        .struct_types
        .iter()
        .any(|struct_ty| struct_ty.name == name)
    {
        return;
    }

    module.struct_types.push(StructType {
        name: name.to_string(),
        is_opaque: true,
    });
}

fn push_declaration(module: &mut Module, name: String, return_type: Type, params: Vec<Param>) {
    module.functions.push(Function {
        name,
        return_type,
        params,
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
}

fn insert_before_terminator(block: &mut BasicBlock, instruction: Instruction) {
    let insert_index = block.instructions.len().saturating_sub(1);
    block.instructions.insert(insert_index, instruction);
}

fn next_available_function_name(module: &Module, prefix: &str) -> String {
    next_available_name(prefix, |candidate| {
        module
            .functions
            .iter()
            .any(|function| function.name == candidate)
    })
}

fn next_available_local_name(function: &Function, prefix: &str) -> String {
    next_available_name(prefix, |candidate| {
        function
            .params
            .iter()
            .any(|param| param.name.as_deref() == Some(candidate))
            || function
                .basic_blocks
                .iter()
                .flat_map(|block| block.instructions.iter())
                .filter_map(instruction_result_name)
                .any(|name| name == candidate)
    })
}

fn next_available_name(prefix: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(prefix) {
        return prefix.to_string();
    }

    for index in 0.. {
        let candidate = format!("{prefix}_{index}");
        if !exists(&candidate) {
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should always find a unique name")
}

fn instruction_result_name(instruction: &Instruction) -> Option<&str> {
    match instruction {
        Instruction::BinOp { result, .. }
        | Instruction::ICmp { result, .. }
        | Instruction::FCmp { result, .. }
        | Instruction::Cast { result, .. }
        | Instruction::Call {
            result: Some(result),
            ..
        }
        | Instruction::Phi { result, .. }
        | Instruction::Alloca { result, .. }
        | Instruction::Load { result, .. }
        | Instruction::Select { result, .. }
        | Instruction::GetElementPtr { result, .. } => Some(result.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BasicBlock;
    use crate::{LlvmIrError, validate_ir};

    fn adaptive_v1_seed_module() -> Module {
        Module {
            source_filename: None,
            target_datalayout: None,
            target_triple: None,
            struct_types: vec![StructType {
                name: "Qubit".to_string(),
                is_opaque: true,
            }],
            globals: Vec::new(),
            functions: vec![Function {
                name: "caller".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                }],
            }],
            attribute_groups: Vec::new(),
            named_metadata: Vec::new(),
            metadata_nodes: Vec::new(),
        }
    }

    fn assert_selector_triggers_type_mismatch(selector: u8, expected_instruction: &str) {
        let seed = adaptive_v1_seed_module();
        assert!(validate_ir(&seed).is_empty(), "seed module should be valid");

        let mutated = mutate_adaptive_v1_typed_pointer_seed(&seed, selector);
        let errors = validate_ir(&mutated);

        assert!(
            errors.iter().any(|error| matches!(
                error,
                LlvmIrError::TypeMismatch { instruction, .. }
                    if instruction == expected_instruction
            )),
            "expected {expected_instruction} type mismatch validation error, got {errors:?}"
        );
    }

    #[test]
    fn load_pointer_operand_mutation_triggers_validator_error() {
        assert_selector_triggers_type_mismatch(0, "Load");
    }

    #[test]
    fn store_pointer_operand_mutation_triggers_validator_error() {
        assert_selector_triggers_type_mismatch(1, "Store");
    }

    #[test]
    fn named_pointer_call_argument_mutation_triggers_validator_error() {
        let seed = adaptive_v1_seed_module();
        assert!(validate_ir(&seed).is_empty(), "seed module should be valid");

        let mutated = mutate_adaptive_v1_typed_pointer_seed(&seed, 2);
        let errors = validate_ir(&mutated);

        assert!(
            errors.iter().any(|error| matches!(
                error,
                LlvmIrError::TypeMismatch {
                    instruction,
                    expected,
                    found,
                    ..
                } if instruction.starts_with("Call @__qir_mut_named_qubit_callee")
                    && expected == "%Qubit*"
                    && found == "%Qubit"
            )),
            "expected named-pointer call mismatch validation error, got {errors:?}"
        );
    }
}
