// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::super::bitstream::BitstreamReader;
use super::*;
use crate::model::test_helpers::*;
use crate::model::{Attribute, AttributeGroup, BasicBlock, Param, StructType};
use crate::parse_bitcode;
use crate::qir::QirEmitTarget;

fn round_trip_module(module: &Module) -> Module {
    let bc = write_bitcode(module);
    parse_bitcode(&bc).expect("should parse round-tripped bitcode")
}

fn simple_function_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "callee".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "main".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "callee".to_string(),
                            args: Vec::new(),
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

fn scan_module_version_and_top_level_blocks(bitcode: &[u8]) -> (Option<u64>, Vec<u32>) {
    let mut reader = BitstreamReader::new(bitcode);
    assert_eq!(reader.read_bits(8), 0x42);
    assert_eq!(reader.read_bits(8), 0x43);
    assert_eq!(reader.read_bits(8), 0xC0);
    assert_eq!(reader.read_bits(8), 0xDE);

    let mut module_version = None;
    let mut top_level_blocks = Vec::new();

    while !reader.at_end() {
        let abbrev_id = reader.read_abbrev_id(TOP_LEVEL_ABBREV_WIDTH);
        match abbrev_id {
            0 => reader.align32(),
            1 => {
                let (block_id, new_abbrev_width, block_len_words) = reader.enter_subblock();
                top_level_blocks.push(block_id);

                if block_id != MODULE_BLOCK_ID {
                    reader.skip_block(block_len_words);
                    continue;
                }

                reader.push_block_scope(MODULE_BLOCK_ID);
                loop {
                    let module_abbrev_id = reader.read_abbrev_id(new_abbrev_width);
                    match module_abbrev_id {
                        0 => {
                            reader.align32();
                            break;
                        }
                        1 => {
                            let (_, _, nested_len_words) = reader.enter_subblock();
                            reader.skip_block(nested_len_words);
                        }
                        2 => reader
                            .read_define_abbrev()
                            .expect("module DEFINE_ABBREV should decode"),
                        3 => {
                            let (code, values) = reader.read_unabbrev_record();
                            if code == MODULE_CODE_VERSION {
                                module_version = values.first().copied();
                            }
                        }
                        id => {
                            let (code, values) = reader
                                .read_abbreviated_record(id)
                                .expect("module abbreviated record should decode");
                            if code == MODULE_CODE_VERSION {
                                module_version = values.first().copied();
                            }
                        }
                    }
                }
                reader.pop_block_scope();
            }
            2 => reader
                .read_define_abbrev()
                .expect("top-level DEFINE_ABBREV should decode"),
            3 => {
                let _ = reader.read_unabbrev_record();
            }
            id => {
                let _ = reader
                    .read_abbreviated_record(id)
                    .expect("top-level abbreviated record should decode");
            }
        }
    }

    (module_version, top_level_blocks)
}

#[test]
fn magic_bytes_present() {
    let m = empty_module();
    let bc = write_bitcode(&m);
    assert!(bc.len() >= 4);
    assert_eq!(&bc[0..4], &[0x42, 0x43, 0xC0, 0xDE]);
}

#[test]
fn empty_module_produces_valid_bitcode() {
    let m = empty_module();
    let bc = write_bitcode(&m);
    assert_eq!(&bc[0..4], &[0x42, 0x43, 0xC0, 0xDE]);
    // Must have content beyond just the magic bytes
    assert!(bc.len() > 4);
    // Length must be 4-byte aligned (bitstream alignment)
    assert_eq!(bc.len() % 4, 0);
}

#[test]
fn sign_rotate_matches_llvm_dense_vbr_contract() {
    assert_eq!(sign_rotate(0), 0);
    assert_eq!(sign_rotate(1), 2);
    assert_eq!(sign_rotate(2), 4);
    assert_eq!(sign_rotate(-1), 3);
    assert_eq!(sign_rotate(-2), 5);
    assert_eq!(sign_rotate(i64::MIN), 1);
}

#[test]
fn module_with_declaration_produces_bitcode() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "__quantum__qis__h__body".to_string(),
            return_type: Type::Void,
            params: vec![Param {
                ty: Type::Ptr,
                name: None,
            }],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let bc = write_bitcode(&m);
    assert_eq!(&bc[0..4], &[0x42, 0x43, 0xC0, 0xDE]);
    assert!(bc.len() > 4);
    assert_eq!(bc.len() % 4, 0);
}

#[test]
fn typed_target_keeps_legacy_module_layout_without_top_level_strtab() {
    let bitcode = write_bitcode_for_target(&simple_function_module(), QirEmitTarget::QirV1Typed);
    let (module_version, top_level_blocks) = scan_module_version_and_top_level_blocks(&bitcode);

    assert_eq!(module_version, Some(1));
    assert!(!top_level_blocks.contains(&STRTAB_BLOCK_ID));
}

#[test]
fn opaque_target_emits_module_v2_and_top_level_strtab() {
    let bitcode = write_bitcode_for_target(&simple_function_module(), QirEmitTarget::QirV2Opaque);
    let (module_version, top_level_blocks) = scan_module_version_and_top_level_blocks(&bitcode);

    assert_eq!(module_version, Some(2));
    assert!(top_level_blocks.contains(&STRTAB_BLOCK_ID));
}

#[test]
fn bitcode_round_trip_preserves_named_ptr_argument_shapes() {
    let qubit_ty = Type::NamedPtr("Qubit".to_string());
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: vec![StructType {
            name: "Qubit".to_string(),
            is_opaque: true,
        }],
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "takes_qubit".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: qubit_ty.clone(),
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "caller".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "takes_qubit".to_string(),
                            args: vec![(qubit_ty.clone(), Operand::int_to_named_ptr(7, "Qubit"))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let parsed = round_trip_module(&m);

    assert_eq!(parsed.functions[0].params[0].ty, qubit_ty.clone());
    match &parsed.functions[1].basic_blocks[0].instructions[0] {
        Instruction::Call {
            return_ty,
            args,
            result,
            attr_refs,
            ..
        } => {
            assert_eq!(return_ty, &None);
            assert_eq!(
                args,
                &vec![(qubit_ty, Operand::int_to_named_ptr(7, "Qubit"))]
            );
            assert_eq!(result, &None);
            assert!(attr_refs.is_empty());
        }
        other => panic!("expected call instruction, found {other:?}"),
    }
}

#[test]
fn bitcode_roundtrip_preserves_call_site_attr_refs_and_function_attrs() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "callee".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Integer(64),
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "caller".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: vec![0],
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "callee".to_string(),
                            args: vec![(
                                Type::Integer(64),
                                Operand::IntConst(Type::Integer(64), 7),
                            )],
                            result: None,
                            attr_refs: vec![0],
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: vec![AttributeGroup {
            id: 0,
            attributes: vec![Attribute::StringAttr("alwaysinline".to_string())],
        }],
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let parsed = round_trip_module(&m);

    assert_eq!(parsed.attribute_groups, m.attribute_groups);
    assert_eq!(parsed.functions[1].attribute_group_refs, vec![0]);
    match &parsed.functions[1].basic_blocks[0].instructions[0] {
        Instruction::Call {
            args,
            attr_refs,
            result,
            ..
        } => {
            assert_eq!(
                args,
                &vec![(Type::Integer(64), Operand::IntConst(Type::Integer(64), 7))]
            );
            assert_eq!(attr_refs, &vec![0]);
            assert_eq!(result, &None);
        }
        other => panic!("expected call instruction, found {other:?}"),
    }
}

#[test]
fn try_write_bitcode_reports_unknown_callee() {
    let mut module = simple_function_module();
    let Instruction::Call { callee, .. } = &mut module.functions[1].basic_blocks[0].instructions[0]
    else {
        panic!("expected call instruction in test module");
    };
    *callee = "missing".to_string();

    let err = try_write_bitcode(&module).expect_err("missing callee should fail emission");

    assert_eq!(
        err,
        WriteError::UnknownCallee {
            callee: "missing".to_string(),
        }
    );
}

#[test]
fn try_write_bitcode_reports_missing_branch_target() {
    let module = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".to_string(),
                instructions: vec![Instruction::Jump {
                    dest: "missing".to_string(),
                }],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let err = try_write_bitcode(&module).expect_err("missing branch target should fail emission");

    assert_eq!(
        err,
        WriteError::MissingBasicBlock {
            context: "unconditional branch".to_string(),
            block: "missing".to_string(),
        }
    );
}

#[test]
fn try_write_bitcode_reports_invalid_float_constant_type() {
    let module = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".to_string(),
                instructions: vec![Instruction::Ret(Some(Operand::FloatConst(
                    Type::Integer(64),
                    1.0,
                )))],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let err =
        try_write_bitcode(&module).expect_err("invalid float constant type should fail emission");

    assert_eq!(
        err,
        WriteError::InvalidFloatingConstant {
            ty: Type::Integer(64),
            value: 1.0,
        }
    );
}
