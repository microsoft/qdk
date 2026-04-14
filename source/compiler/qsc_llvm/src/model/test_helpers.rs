// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

pub fn empty_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

pub fn single_instruction_module(instr: Instruction) -> Module {
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
                instructions: vec![instr, Instruction::Ret(None)],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

#[allow(clippy::too_many_lines)]
pub fn bell_module_v2() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: vec![
            GlobalVariable {
                name: "0".to_string(),
                ty: Type::Array(4, Box::new(Type::Integer(8))),
                linkage: Linkage::Internal,
                is_constant: true,
                initializer: Some(Constant::CString("0_a".to_string())),
            },
            GlobalVariable {
                name: "1".to_string(),
                ty: Type::Array(6, Box::new(Type::Integer(8))),
                linkage: Linkage::Internal,
                is_constant: true,
                initializer: Some(Constant::CString("1_a0r".to_string())),
            },
            GlobalVariable {
                name: "2".to_string(),
                ty: Type::Array(6, Box::new(Type::Integer(8))),
                linkage: Linkage::Internal,
                is_constant: true,
                initializer: Some(Constant::CString("2_a1r".to_string())),
            },
        ],
        functions: vec![
            Function {
                name: "__quantum__qis__h__body".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Ptr,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "__quantum__qis__cx__body".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                ],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "__quantum__qis__m__body".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                ],
                is_declaration: true,
                attribute_group_refs: vec![1],
                basic_blocks: Vec::new(),
            },
            Function {
                name: "__quantum__rt__array_record_output".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Integer(64),
                        name: None,
                    },
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                ],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "__quantum__rt__result_record_output".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                ],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "ENTRYPOINT__main".to_string(),
                return_type: Type::Integer(64),
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: vec![0],
                basic_blocks: vec![BasicBlock {
                    name: "block_0".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__qis__h__body".to_string(),
                            args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__qis__cx__body".to_string(),
                            args: vec![
                                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
                                (Type::Ptr, Operand::IntToPtr(1, Type::Ptr)),
                            ],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__qis__m__body".to_string(),
                            args: vec![
                                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
                                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
                            ],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__qis__m__body".to_string(),
                            args: vec![
                                (Type::Ptr, Operand::IntToPtr(1, Type::Ptr)),
                                (Type::Ptr, Operand::IntToPtr(1, Type::Ptr)),
                            ],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__rt__array_record_output".to_string(),
                            args: vec![
                                (Type::Integer(64), Operand::IntConst(Type::Integer(64), 2)),
                                (Type::Ptr, Operand::GlobalRef("0".to_string())),
                            ],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__rt__result_record_output".to_string(),
                            args: vec![
                                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
                                (Type::Ptr, Operand::GlobalRef("1".to_string())),
                            ],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__rt__result_record_output".to_string(),
                            args: vec![
                                (Type::Ptr, Operand::IntToPtr(1, Type::Ptr)),
                                (Type::Ptr, Operand::GlobalRef("2".to_string())),
                            ],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
                    ],
                }],
            },
        ],
        attribute_groups: vec![
            AttributeGroup {
                id: 0,
                attributes: vec![
                    Attribute::StringAttr("entry_point".to_string()),
                    Attribute::StringAttr("output_labeling_schema".to_string()),
                    Attribute::KeyValue("qir_profiles".to_string(), "adaptive_profile".to_string()),
                    Attribute::KeyValue("required_num_qubits".to_string(), "2".to_string()),
                    Attribute::KeyValue("required_num_results".to_string(), "2".to_string()),
                ],
            },
            AttributeGroup {
                id: 1,
                attributes: vec![Attribute::StringAttr("irreversible".to_string())],
            },
        ],
        named_metadata: vec![NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![0, 1, 2, 3, 4, 5, 6],
        }],
        metadata_nodes: vec![
            MetadataNode {
                id: 0,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 1),
                    MetadataValue::String("qir_major_version".to_string()),
                    MetadataValue::Int(Type::Integer(32), 2),
                ],
            },
            MetadataNode {
                id: 1,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 7),
                    MetadataValue::String("qir_minor_version".to_string()),
                    MetadataValue::Int(Type::Integer(32), 1),
                ],
            },
            MetadataNode {
                id: 2,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 1),
                    MetadataValue::String("dynamic_qubit_management".to_string()),
                    MetadataValue::Int(Type::Integer(1), 0),
                ],
            },
            MetadataNode {
                id: 3,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 1),
                    MetadataValue::String("dynamic_result_management".to_string()),
                    MetadataValue::Int(Type::Integer(1), 0),
                ],
            },
            MetadataNode {
                id: 4,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 5),
                    MetadataValue::String("int_computations".to_string()),
                    MetadataValue::SubList(vec![MetadataValue::String("i64".to_string())]),
                ],
            },
            MetadataNode {
                id: 5,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 7),
                    MetadataValue::String("backwards_branching".to_string()),
                    MetadataValue::Int(Type::Integer(2), 3),
                ],
            },
            MetadataNode {
                id: 6,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 1),
                    MetadataValue::String("arrays".to_string()),
                    MetadataValue::Int(Type::Integer(1), 1),
                ],
            },
        ],
    }
}
