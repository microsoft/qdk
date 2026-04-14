// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::model::{Attribute, AttributeGroup, BasicBlock, Function};
use crate::text::writer::write_module_to_string;

fn simple_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".to_string(),
                instructions: vec![
                    Instruction::Call {
                        return_ty: None,
                        callee: "__quantum__qis__h__body".to_string(),
                        args: vec![(Type::Ptr, Operand::NullPtr)],
                        result: None,
                        attr_refs: Vec::new(),
                    },
                    Instruction::Ret(None),
                ],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

fn two_function_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "func_a".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![
                    BasicBlock {
                        name: "entry".to_string(),
                        instructions: vec![Instruction::Jump {
                            dest: "exit".to_string(),
                        }],
                    },
                    BasicBlock {
                        name: "exit".to_string(),
                        instructions: vec![Instruction::Ret(None)],
                    },
                ],
            },
            Function {
                name: "func_b".to_string(),
                return_type: Type::Integer(32),
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Ret(Some(Operand::IntConst(
                        Type::Integer(32),
                        42,
                    )))],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

#[test]
fn insert_before_adds_instruction() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("test_fn")
        .expect("function should exist");
    builder
        .position_at_block("entry")
        .expect("block should exist");

    // Position before the ret instruction (index 1)
    builder.position_before(1);
    builder.call(
        "__quantum__qis__cx__body",
        vec![(Type::Ptr, Operand::NullPtr), (Type::Ptr, Operand::NullPtr)],
        None,
        None,
    );

    let block = &module.functions[0].basic_blocks[0];
    assert_eq!(block.instructions.len(), 3);
    // Original call at 0, new call at 1, ret at 2
    assert!(matches!(block.instructions[1], Instruction::Call { .. }));
    assert!(matches!(block.instructions[2], Instruction::Ret(None)));
}

#[test]
fn insert_at_end_appends() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("test_fn")
        .expect("function should exist");
    builder
        .position_at_block("entry")
        .expect("block should exist");

    builder.insert_at_end(Instruction::Unreachable);

    let block = &module.functions[0].basic_blocks[0];
    assert_eq!(block.instructions.len(), 3);
    assert!(matches!(block.instructions[2], Instruction::Unreachable));
}

#[test]
fn erase_at_cursor_removes_instruction() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("test_fn")
        .expect("function should exist");
    builder
        .position_at_block("entry")
        .expect("block should exist");

    // Erase the first instruction (h gate call)
    builder.position_before(0);
    let erased = builder.erase_at_cursor();
    assert!(matches!(erased, Instruction::Call { .. }));

    let block = &module.functions[0].basic_blocks[0];
    assert_eq!(block.instructions.len(), 1);
    assert!(matches!(block.instructions[0], Instruction::Ret(None)));
}

#[test]
fn position_at_different_functions() {
    let mut module = two_function_module();
    let mut builder = ModuleBuilder::new(&mut module);

    builder
        .position_at_function("func_b")
        .expect("func_b should exist");
    builder
        .position_at_block("entry")
        .expect("block should exist");
    assert_eq!(builder.current_block_len(), 1);

    builder
        .position_at_function("func_a")
        .expect("func_a should exist");
    builder
        .position_at_block("exit")
        .expect("exit block should exist");
    assert_eq!(builder.current_block_len(), 1);
}

#[test]
fn position_at_nonexistent_function_errors() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    let result = builder.position_at_function("no_such_fn");
    assert!(result.is_err());
}

#[test]
fn position_at_nonexistent_block_errors() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("test_fn")
        .expect("function should exist");
    let result = builder.position_at_block("no_such_block");
    assert!(result.is_err());
}

#[test]
fn instr_inserts_arbitrary_instruction() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("test_fn")
        .expect("function should exist");
    builder
        .position_at_block("entry")
        .expect("block should exist");
    builder.position_before(0);

    builder.instr(Instruction::Alloca {
        ty: Type::Integer(64),
        result: "%x".to_string(),
    });

    let block = &module.functions[0].basic_blocks[0];
    assert_eq!(block.instructions.len(), 3);
    assert!(matches!(block.instructions[0], Instruction::Alloca { .. }));
}

#[test]
fn multiple_sequential_mutations() {
    let mut module = simple_module();
    {
        let mut builder = ModuleBuilder::new(&mut module);
        builder
            .position_at_function("test_fn")
            .expect("function should exist");
        builder
            .position_at_block("entry")
            .expect("block should exist");

        // Insert two calls before the ret (index 1)
        builder.position_before(1);
        builder.call(
            "__quantum__qis__x__body",
            vec![(Type::Ptr, Operand::NullPtr)],
            None,
            None,
        );
        // Cursor is now at 2, insert another
        builder.call(
            "__quantum__qis__z__body",
            vec![(Type::Ptr, Operand::NullPtr)],
            None,
            None,
        );

        // h, x, z, ret
        assert_eq!(builder.current_block_len(), 4);

        // Erase the original h call at index 0
        builder.position_before(0);
        let erased = builder.erase_at_cursor();
        assert!(
            matches!(erased, Instruction::Call { callee, .. } if callee == "__quantum__qis__h__body")
        );

        // x, z, ret
        assert_eq!(builder.current_block_len(), 3);
    }

    // Verify final state after builder is dropped
    assert_eq!(module.functions[0].basic_blocks[0].instructions.len(), 3);
}

#[test]
fn position_at_end_sets_cursor_past_last() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("test_fn")
        .expect("function should exist");
    builder
        .position_at_block("entry")
        .expect("block should exist");

    builder.position_at_end();
    assert_eq!(builder.current_block_len(), 2);
    // Inserting at end via insert_before at position == len is effectively append
    builder.instr(Instruction::Unreachable);
    assert_eq!(builder.current_block_len(), 3);
}

#[test]
fn modified_module_serializes_to_valid_ir() {
    let mut module = simple_module();
    {
        let mut builder = ModuleBuilder::new(&mut module);
        builder
            .position_at_function("test_fn")
            .expect("function should exist");
        builder
            .position_at_block("entry")
            .expect("block should exist");

        // Insert a cx call before ret
        builder.position_before(1);
        builder.call(
            "__quantum__qis__cx__body",
            vec![(Type::Ptr, Operand::NullPtr), (Type::Ptr, Operand::NullPtr)],
            None,
            None,
        );
    }

    let text = write_module_to_string(&module);
    assert!(text.contains("call void @__quantum__qis__h__body(ptr null)"));
    assert!(text.contains("call void @__quantum__qis__cx__body(ptr null, ptr null)"));
    assert!(text.contains("ret void"));
}

#[test]
fn round_trip_after_mutation() {
    use crate::text::reader::parse_module;

    let mut module = simple_module();
    {
        let mut builder = ModuleBuilder::new(&mut module);
        builder
            .position_at_function("test_fn")
            .expect("function should exist");
        builder
            .position_at_block("entry")
            .expect("block should exist");
        builder.position_before(1);
        builder.call(
            "__quantum__qis__z__body",
            vec![(Type::Ptr, Operand::NullPtr)],
            None,
            None,
        );
    }

    let text1 = write_module_to_string(&module);
    let parsed = parse_module(&text1).expect("should parse modified module");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text1, text2);
}

#[test]
fn ensure_declaration_adds_new() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    let added = builder.ensure_declaration("new_decl", Type::Void, vec![Type::Ptr]);
    assert!(added);
    assert_eq!(module.functions.len(), 2);
    assert_eq!(module.functions[1].name, "new_decl");
    assert!(module.functions[1].is_declaration);
}

#[test]
fn ensure_declaration_skips_existing() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    let added = builder.ensure_declaration("test_fn", Type::Void, vec![]);
    assert!(!added);
    assert_eq!(module.functions.len(), 1);
}

#[test]
fn retain_functions_removes_matching() {
    let mut module = two_function_module();
    let mut builder = ModuleBuilder::new(&mut module);
    builder
        .position_at_function("func_b")
        .expect("func_b should exist");
    builder.retain_functions(|f| f.name == "func_a");
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "func_a");
}

#[test]
fn entry_point_index_found() {
    let mut module = simple_module();
    module.functions[0].attribute_group_refs = vec![0];
    module.attribute_groups.push(AttributeGroup {
        id: 0,
        attributes: vec![Attribute::StringAttr("entry_point".to_string())],
    });
    let builder = ModuleBuilder::new(&mut module);
    assert_eq!(builder.entry_point_index(), Some(0));
}

#[test]
fn entry_point_index_not_found() {
    let mut module = simple_module();
    let builder = ModuleBuilder::new(&mut module);
    assert_eq!(builder.entry_point_index(), None);
}

#[test]
fn entry_point_index_skips_declarations() {
    let mut module = simple_module();
    module.functions.insert(
        0,
        Function {
            name: "decl_entry".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: true,
            attribute_group_refs: vec![0],
            basic_blocks: Vec::new(),
        },
    );
    module.functions[1].attribute_group_refs = vec![0];
    module.attribute_groups.push(AttributeGroup {
        id: 0,
        attributes: vec![Attribute::StringAttr("entry_point".to_string())],
    });

    let builder = ModuleBuilder::new(&mut module);
    assert_eq!(builder.entry_point_index(), Some(1));
}

#[test]
fn module_access() {
    let mut module = simple_module();
    let mut builder = ModuleBuilder::new(&mut module);
    assert_eq!(builder.module().functions.len(), 1);
    builder.module_mut().functions.clear();
    assert_eq!(builder.module().functions.len(), 0);
}
