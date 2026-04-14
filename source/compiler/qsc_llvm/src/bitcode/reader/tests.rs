// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::bitcode::bitstream::BitstreamWriter;
use crate::bitcode::writer::write_bitcode;
use crate::model::Param;
use crate::model::test_helpers::*;
use crate::test_utils::{PointerProbe, assemble_text_ir, available_fast_matrix_lanes};
use crate::{ReadDiagnosticKind, ReadPolicy};
use std::cell::RefCell;

fn round_trip_module(module: &Module) -> Module {
    let bc = write_bitcode(module);
    parse_bitcode(&bc).expect("should parse round-tripped bitcode")
}

fn build_module_constants_bitcode(
    type_records: &[(u32, Vec<u64>)],
    module_records: &[(u32, Vec<u64>)],
    constant_records: &[(u32, Vec<u64>)],
) -> Vec<u8> {
    const TOP_ABBREV_WIDTH: u32 = 2;
    const BLOCK_ABBREV_WIDTH: u32 = 4;

    let mut writer = BitstreamWriter::new();
    for magic in [0x42_u64, 0x43, 0xC0, 0xDE] {
        writer.emit_bits(magic, 8);
    }

    writer.enter_subblock(MODULE_BLOCK_ID, BLOCK_ABBREV_WIDTH, TOP_ABBREV_WIDTH);

    if !type_records.is_empty() {
        writer.enter_subblock(TYPE_BLOCK_ID_NEW, BLOCK_ABBREV_WIDTH, BLOCK_ABBREV_WIDTH);
        for (code, values) in type_records {
            writer.emit_record(*code, values, BLOCK_ABBREV_WIDTH);
        }
        writer.exit_block(BLOCK_ABBREV_WIDTH);
    }

    for (code, values) in module_records {
        writer.emit_record(*code, values, BLOCK_ABBREV_WIDTH);
    }

    if !constant_records.is_empty() {
        writer.enter_subblock(CONSTANTS_BLOCK_ID, BLOCK_ABBREV_WIDTH, BLOCK_ABBREV_WIDTH);
        for (code, values) in constant_records {
            writer.emit_record(*code, values, BLOCK_ABBREV_WIDTH);
        }
        writer.exit_block(BLOCK_ABBREV_WIDTH);
    }

    writer.exit_block(BLOCK_ABBREV_WIDTH);
    writer.finish()
}

fn bad_current_type_id_in_constants_block_fixture() -> Vec<u8> {
    build_module_constants_bitcode(
        &[(TYPE_CODE_NUMENTRY, vec![1]), (TYPE_CODE_INTEGER, vec![64])],
        &[],
        &[(CST_CODE_SETTYPE, vec![99]), (CST_CODE_INTEGER, vec![0])],
    )
}

fn bad_inttoptr_source_id_constants_block_fixture() -> Vec<u8> {
    build_module_constants_bitcode(
        &[
            (TYPE_CODE_NUMENTRY, vec![2]),
            (TYPE_CODE_INTEGER, vec![64]),
            (TYPE_CODE_OPAQUE_POINTER, vec![]),
        ],
        &[],
        &[
            (CST_CODE_SETTYPE, vec![1]),
            (CST_CODE_CE_CAST, vec![10, 0, 77]),
        ],
    )
}

fn bad_gep_pointer_type_id_constants_block_fixture() -> Vec<u8> {
    build_module_constants_bitcode(
        &[(TYPE_CODE_NUMENTRY, vec![1]), (TYPE_CODE_INTEGER, vec![64])],
        &[(MODULE_CODE_GLOBALVAR, vec![0, 0, 0, 0, 3])],
        &[(CST_CODE_CE_INBOUNDS_GEP, vec![0, 99, 0])],
    )
}

fn bad_gep_index_constant_id_constants_block_fixture() -> Vec<u8> {
    build_module_constants_bitcode(
        &[
            (TYPE_CODE_NUMENTRY, vec![2]),
            (TYPE_CODE_INTEGER, vec![64]),
            (TYPE_CODE_OPAQUE_POINTER, vec![]),
        ],
        &[(MODULE_CODE_GLOBALVAR, vec![0, 0, 0, 0, 3])],
        &[(CST_CODE_CE_INBOUNDS_GEP, vec![0, 1, 0, 0, 77])],
    )
}

fn assert_strict_rejection_and_report_recovery(
    bitcode: &[u8],
    expected_context: &'static str,
    expected_message_fragment: &str,
) {
    let diagnostics = parse_bitcode_detailed(bitcode, ReadPolicy::QirSubsetStrict)
        .expect_err("strict reader should reject malformed constants-block input");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].kind,
        ReadDiagnosticKind::UnsupportedSemanticConstruct
    );
    assert_eq!(diagnostics[0].context, expected_context);
    assert!(
        diagnostics[0].message.contains(expected_message_fragment),
        "unexpected strict diagnostic: {:?}",
        diagnostics
    );

    let report = parse_bitcode_compatibility_report(bitcode)
        .expect("compatibility report path should recover malformed constants-block input");

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].kind,
        ReadDiagnosticKind::UnsupportedSemanticConstruct
    );
    assert_eq!(report.diagnostics[0].context, expected_context);
    assert!(
        report.diagnostics[0]
            .message
            .contains(expected_message_fragment),
        "unexpected compatibility diagnostic: {:?}",
        report.diagnostics
    );
}

#[test]
fn sign_unrotate_positive() {
    assert_eq!(sign_unrotate(0), 0);
    assert_eq!(sign_unrotate(2), 1);
    assert_eq!(sign_unrotate(4), 2);
    assert_eq!(sign_unrotate(200), 100);
}

#[test]
fn sign_unrotate_negative() {
    assert_eq!(sign_unrotate(1), i64::MIN);
    assert_eq!(sign_unrotate(3), -1);
    assert_eq!(sign_unrotate(5), -2);
    assert_eq!(sign_unrotate(7), -3);
}

#[test]
fn invalid_magic_returns_error() {
    let data = vec![0x00, 0x00, 0x00, 0x00];
    let result = parse_bitcode(&data);
    assert!(result.is_err());
    let err = result.expect_err("should error on invalid magic");
    assert!(err.message.contains("magic"), "error: {err}");
}

#[test]
fn too_short_returns_error() {
    let data = vec![0x42, 0x43];
    let result = parse_bitcode(&data);
    assert!(result.is_err());
}

#[test]
fn parse_empty_module_bitcode() {
    let m = empty_module();
    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("should parse empty module bitcode");
    assert!(parsed.functions.is_empty());
    assert!(parsed.globals.is_empty());
}

#[test]
fn parse_module_with_declaration() {
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
    let parsed = parse_bitcode(&bc).expect("should parse module with declaration");
    assert_eq!(parsed.functions.len(), 1);
    assert!(parsed.functions[0].is_declaration);
    assert_eq!(parsed.functions[0].name, "__quantum__qis__h__body");
    assert_eq!(parsed.functions[0].params.len(), 1);
}

#[test]
fn parse_simple_function_body() {
    let m = Module {
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
                instructions: vec![Instruction::Ret(None)],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("should parse simple function body");
    assert_eq!(parsed.functions.len(), 1);
    assert!(!parsed.functions[0].is_declaration);
    assert_eq!(parsed.functions[0].basic_blocks.len(), 1);
    assert_eq!(parsed.functions[0].basic_blocks[0].instructions.len(), 1);
    assert!(matches!(
        parsed.functions[0].basic_blocks[0].instructions[0],
        Instruction::Ret(None)
    ));
}

#[test]
fn apply_pending_strtab_names_remaps_function_placeholders_and_call_uses() {
    let mut reader = BlockReader::new(&[0x42, 0x43, 0xC0, 0xDE], ReadPolicy::QirSubsetStrict)
        .expect("bitcode magic header should construct a reader");
    reader.functions.push(Function {
        name: "__func_0".to_string(),
        return_type: Type::Void,
        params: Vec::new(),
        is_declaration: false,
        attribute_group_refs: Vec::new(),
        basic_blocks: vec![BasicBlock {
            name: "entry".to_string(),
            instructions: vec![Instruction::Call {
                return_ty: None,
                callee: "__func_0".to_string(),
                args: Vec::new(),
                result: None,
                attr_refs: Vec::new(),
            }],
        }],
    });
    reader
        .global_value_table
        .push(ValueEntry::Function("__func_0".to_string()));
    reader.pending_strtab_names.push(PendingStrtabName {
        value_id: 0,
        offset: 0,
        size: 4,
    });
    reader.string_table = b"test".to_vec();

    reader.apply_pending_strtab_names();

    assert!(reader.pending_strtab_names.is_empty());
    assert_eq!(reader.functions[0].name, "test");
    assert!(matches!(
        reader.global_value_table[0],
        ValueEntry::Function(ref name) if name == "test"
    ));
    assert!(matches!(
        &reader.functions[0].basic_blocks[0].instructions[0],
        Instruction::Call { callee, .. } if callee == "test"
    ));
}

#[test]
fn bitcode_round_trip_preserves_half_float_double_kinds() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "takes_fp".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Half,
                        name: None,
                    },
                    Param {
                        ty: Type::Float,
                        name: None,
                    },
                    Param {
                        ty: Type::Double,
                        name: None,
                    },
                ],
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
                            callee: "takes_fp".to_string(),
                            args: vec![
                                (Type::Half, Operand::float_const(Type::Half, 1.5)),
                                (Type::Float, Operand::float_const(Type::Float, 2.5)),
                                (Type::Double, Operand::float_const(Type::Double, 3.5)),
                            ],
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

    assert_eq!(
        parsed.functions[0]
            .params
            .iter()
            .map(|param| param.ty.clone())
            .collect::<Vec<_>>(),
        vec![Type::Half, Type::Float, Type::Double]
    );
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
                &vec![
                    (Type::Half, Operand::float_const(Type::Half, 1.5)),
                    (Type::Float, Operand::float_const(Type::Float, 2.5)),
                    (Type::Double, Operand::float_const(Type::Double, 3.5)),
                ]
            );
            assert_eq!(result, &None);
            assert!(attr_refs.is_empty());
        }
        other => panic!("expected call instruction, found {other:?}"),
    }
}

#[test]
fn parse_call_record_uses_relative_callee_value_with_explicit_type_flag() {
    let globals = vec![
        ValueEntry::Function("__func_0".to_string()),
        ValueEntry::Function("main".to_string()),
    ];
    let locals = vec![ValueEntry::Param("a".to_string(), Type::Integer(64))];
    let type_table = vec![Type::Function(
        Box::new(Type::Void),
        vec![Type::Integer(64)],
    )];
    let bb_names = FxHashMap::default();
    let diagnostics = RefCell::new(Vec::new());
    let ctx = InstrContext {
        global_value_table: &globals,
        local_values: &locals,
        type_table: &type_table,
        paramattr_lists: &[],
        bb_names: &bb_names,
        diagnostics: &diagnostics,
        current_value_id: 3,
        byte_offset: 12,
        policy: ReadPolicy::QirSubsetStrict,
    };
    let values = [0u64, CALL_EXPLICIT_TYPE_FLAG, 0, 3, 1];
    let mut input: RecordInput<'_> = &values;

    let instruction = parse_call_record(&ctx, &mut input).expect("should parse call record");

    assert!(matches!(
        instruction,
        Instruction::Call { callee, args, .. }
            if callee == "__func_0"
                && matches!(
                    args.as_slice(),
                    [(Type::Integer(64), Operand::TypedLocalRef(name, ty))]
                        if name == "a" && ty == &Type::Integer(64)
                )
    ));
}

#[test]
fn parse_call_record_accepts_legacy_absolute_callee_value_with_explicit_type_flag() {
    let globals = vec![
        ValueEntry::Function("__func_0".to_string()),
        ValueEntry::Function("main".to_string()),
    ];
    let locals = vec![ValueEntry::Param("a".to_string(), Type::Integer(64))];
    let type_table = vec![Type::Function(
        Box::new(Type::Void),
        vec![Type::Integer(64)],
    )];
    let bb_names = FxHashMap::default();
    let diagnostics = RefCell::new(Vec::new());
    let ctx = InstrContext {
        global_value_table: &globals,
        local_values: &locals,
        type_table: &type_table,
        paramattr_lists: &[],
        bb_names: &bb_names,
        diagnostics: &diagnostics,
        current_value_id: 3,
        byte_offset: 12,
        policy: ReadPolicy::QirSubsetStrict,
    };
    let values = [0u64, CALL_EXPLICIT_TYPE_FLAG, 0, 0, 1];
    let mut input: RecordInput<'_> = &values;

    let instruction = parse_call_record(&ctx, &mut input).expect("should parse legacy call record");

    assert!(matches!(
        instruction,
        Instruction::Call { callee, args, .. }
            if callee == "__func_0"
                && matches!(
                    args.as_slice(),
                    [(Type::Integer(64), Operand::TypedLocalRef(name, ty))]
                        if name == "a" && ty == &Type::Integer(64)
                )
    ));
}

#[test]
fn parse_call_record_resolves_attr_refs_from_paramattr_list() {
    let globals = vec![ValueEntry::Function("callee".to_string())];
    let locals = Vec::new();
    let type_table = vec![Type::Function(Box::new(Type::Void), Vec::new())];
    let paramattr_lists = vec![vec![7, 11]];
    let bb_names = FxHashMap::default();
    let diagnostics = RefCell::new(Vec::new());
    let ctx = InstrContext {
        global_value_table: &globals,
        local_values: &locals,
        type_table: &type_table,
        paramattr_lists: &paramattr_lists,
        bb_names: &bb_names,
        diagnostics: &diagnostics,
        current_value_id: 1,
        byte_offset: 12,
        policy: ReadPolicy::QirSubsetStrict,
    };
    let values = [1u64, 0, 0, 1];
    let mut input: RecordInput<'_> = &values;

    let instruction = parse_call_record(&ctx, &mut input).expect("should parse call record");

    assert!(matches!(
        instruction,
        Instruction::Call { callee, attr_refs, .. }
            if callee == "callee" && attr_refs == vec![7, 11]
    ));
}

#[test]
fn strict_call_target_placeholder_is_rejected() {
    let globals = Vec::new();
    let locals = Vec::new();
    let type_table = vec![Type::Function(Box::new(Type::Void), Vec::new())];
    let bb_names = FxHashMap::default();
    let diagnostics = RefCell::new(Vec::new());
    let ctx = InstrContext {
        global_value_table: &globals,
        local_values: &locals,
        type_table: &type_table,
        paramattr_lists: &[],
        bb_names: &bb_names,
        diagnostics: &diagnostics,
        current_value_id: 0,
        byte_offset: 33,
        policy: ReadPolicy::QirSubsetStrict,
    };

    let err = ctx
        .resolve_call_target_name(0)
        .expect_err("strict mode should reject unresolved callees");

    assert_eq!(err.kind, ReadDiagnosticKind::UnsupportedSemanticConstruct);
    assert_eq!(err.context, "call instruction");
}

#[test]
fn strict_phi_forward_reference_uses_placeholder_name() {
    let globals = Vec::new();
    let locals = Vec::new();
    let type_table = vec![Type::Integer(64)];
    let bb_names = FxHashMap::default();
    let diagnostics = RefCell::new(Vec::new());
    let ctx = InstrContext {
        global_value_table: &globals,
        local_values: &locals,
        type_table: &type_table,
        paramattr_lists: &[],
        bb_names: &bb_names,
        diagnostics: &diagnostics,
        current_value_id: 4,
        byte_offset: 41,
        policy: ReadPolicy::QirSubsetStrict,
    };

    let operand = ctx
        .resolve_phi_operand(3, &Type::Integer(64))
        .expect("strict mode should preserve forward PHI references as placeholders");

    assert_eq!(
        operand,
        Operand::TypedLocalRef("val_5".to_string(), Type::Integer(64))
    );
}

#[test]
fn strict_unresolved_phi_placeholder_is_rejected_after_function_parse() {
    let reader = BlockReader::new(&[0x42, 0x43, 0xC0, 0xDE], ReadPolicy::QirSubsetStrict)
        .expect("bitcode magic header should construct a reader");
    let basic_blocks = vec![BasicBlock {
        name: "loop".to_string(),
        instructions: vec![Instruction::Phi {
            ty: Type::Integer(64),
            incoming: vec![(
                Operand::TypedLocalRef("val_99".to_string(), Type::Integer(64)),
                "loop".to_string(),
            )],
            result: "val_4".to_string(),
        }],
    }];
    let local_values = vec![ValueEntry::Local("val_4".to_string(), Type::Integer(64))];

    let err = reader
        .validate_phi_operands_resolved(&basic_blocks, &local_values)
        .expect_err("strict mode should reject unresolved PHI placeholders after function parse");

    assert_eq!(err.kind, ReadDiagnosticKind::UnsupportedSemanticConstruct);
    assert_eq!(err.context, "phi instruction");
}

#[test]
fn strict_unknown_attribute_encoding_is_rejected() {
    let diagnostics = RefCell::new(Vec::new());
    let err =
        BlockReader::parse_attr_encodings(&[7], ReadPolicy::QirSubsetStrict, 19, &diagnostics)
            .expect_err("strict mode should reject unknown attribute encodings");

    assert_eq!(err.kind, ReadDiagnosticKind::UnsupportedSemanticConstruct);
    assert_eq!(err.context, "attribute group");
}

#[test]
fn strict_global_initializer_record_is_rejected() {
    let mut reader = BlockReader::new(&[0x42, 0x43, 0xC0, 0xDE], ReadPolicy::QirSubsetStrict)
        .expect("bitcode magic header should construct a reader");
    reader.type_table.push(Type::Integer(8));

    let err = reader
        .handle_module_record(
            MODULE_CODE_GLOBALVAR,
            &[0, 0, 1, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        )
        .expect_err("strict mode should reject placeholder global initializers");

    assert_eq!(err.kind, ReadDiagnosticKind::UnsupportedSemanticConstruct);
    assert_eq!(err.context, "global variable");
}

#[test]
fn strict_unknown_type_record_is_rejected() {
    let mut reader = BlockReader::new(&[0x42, 0x43, 0xC0, 0xDE], ReadPolicy::QirSubsetStrict)
        .expect("bitcode magic header should construct a reader");

    let err = reader
        .handle_type_record(999, &[])
        .expect_err("strict mode should reject unsupported type records");

    assert_eq!(err.kind, ReadDiagnosticKind::UnsupportedSemanticConstruct);
    assert_eq!(err.context, "type record");
}

#[test]
fn strict_reader_rejects_bad_current_type_id_in_constants_block() {
    let bitcode = bad_current_type_id_in_constants_block_fixture();

    assert_strict_rejection_and_report_recovery(&bitcode, "constant record", "unknown type ID 99");
}

#[test]
fn strict_reader_rejects_bad_inttoptr_source_id() {
    let bitcode = bad_inttoptr_source_id_constants_block_fixture();

    assert_strict_rejection_and_report_recovery(
        &bitcode,
        "constant expression",
        "inttoptr constant source value ID 77",
    );
}

#[test]
fn strict_reader_rejects_bad_gep_pointer_type_id() {
    let bitcode = bad_gep_pointer_type_id_constants_block_fixture();

    assert_strict_rejection_and_report_recovery(
        &bitcode,
        "constant expression",
        "getelementptr constant pointer type references unknown type ID 99",
    );
}

#[test]
fn strict_reader_rejects_bad_gep_index_constant_id() {
    let bitcode = bad_gep_index_constant_id_constants_block_fixture();

    assert_strict_rejection_and_report_recovery(
        &bitcode,
        "constant expression",
        "getelementptr constant index value ID 77",
    );
}

#[test]
fn compatibility_entry_points_require_report_api_for_constants_recovery() {
    let bitcode = bad_current_type_id_in_constants_block_fixture();

    let diagnostics = parse_bitcode_detailed(&bitcode, ReadPolicy::Compatibility)
        .expect_err("compatibility detailed parse should require the report API for recovery");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].kind,
        ReadDiagnosticKind::UnsupportedSemanticConstruct
    );
    assert_eq!(diagnostics[0].context, "constant record");

    let err = parse_bitcode_compatibility(&bitcode)
        .expect_err("legacy compatibility helper should reject recoveries without diagnostics");

    assert_eq!(err.kind, ReadDiagnosticKind::UnsupportedSemanticConstruct);
    assert_eq!(err.context, "constant record");
    assert!(err.message.contains("unknown type ID 99"));
}

#[test]
fn strict_bitcode_import_rejects_non_opaque_struct_body_fixture() {
    let Some(lane) = available_fast_matrix_lanes().into_iter().next() else {
        eprintln!(
            "no external LLVM fast-matrix lane is available, skipping non-opaque struct bitcode fixture"
        );
        return;
    };

    let bitcode = assemble_text_ir(
        lane,
        PointerProbe::OpaqueText,
        // `llvm-as` drops unused type aliases, so force the non-opaque struct
        // body into the type table via a declaration that references `%Pair`.
        "%Pair = type { i64, i64 }\ndeclare void @use(%Pair)\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "llvm@{} should assemble non-opaque struct fixture: {error}",
            lane.version
        )
    });

    let diagnostics = parse_bitcode_detailed(&bitcode, ReadPolicy::QirSubsetStrict)
        .expect_err("strict bitcode import should reject non-opaque struct bodies");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].kind,
        ReadDiagnosticKind::UnsupportedSemanticConstruct
    );
    assert_eq!(diagnostics[0].context, "type record");
    assert!(
        diagnostics[0]
            .message
            .contains("unsupported type record code")
    );
}

// Winnow-specific tests: verify primitive parsers on raw &[u64] slices

#[test]
fn winnow_parse_char_string_from_record() {
    let values: Vec<u64> = "hello".bytes().map(u64::from).collect();
    let mut input: RecordInput<'_> = &values;
    let result = parse_char_string(&mut input).expect("should parse char string");
    assert_eq!(result, "hello");
    assert!(input.is_empty());
}

#[test]
fn winnow_parse_type_integer_record() {
    let values = [64u64];
    let mut input: RecordInput<'_> = &values;
    let result = parse_type_integer(&mut input).expect("should parse integer type");
    assert_eq!(result, Type::Integer(64));
}

#[test]
fn winnow_parse_type_integer_default() {
    let values: [u64; 0] = [];
    let mut input: RecordInput<'_> = &values;
    let result = parse_type_integer(&mut input).expect("should default to 32");
    assert_eq!(result, Type::Integer(32));
}

#[test]
fn winnow_parse_type_pointer_named_record_preserves_named_ptr() {
    let mut reader = BlockReader::new(&[0x42, 0x43, 0xC0, 0xDE], ReadPolicy::QirSubsetStrict)
        .expect("bitcode magic header should construct a reader");
    reader.type_table.push(Type::Named("Qubit".to_string()));

    reader
        .handle_type_record(TYPE_CODE_POINTER, &[0])
        .expect("should parse named pointer type record");

    assert_eq!(reader.type_table[1], Type::NamedPtr("Qubit".to_string()));
}

#[test]
fn winnow_parse_type_label_record_preserves_slot_identity() {
    let mut reader = BlockReader::new(&[0x42, 0x43, 0xC0, 0xDE], ReadPolicy::QirSubsetStrict)
        .expect("bitcode magic header should construct a reader");

    reader
        .handle_type_record(TYPE_CODE_LABEL, &[])
        .expect("should parse label type record");
    reader
        .handle_type_record(TYPE_CODE_INTEGER, &[64])
        .expect("should parse later integer type record");

    assert_eq!(reader.type_table[0], Type::Label);
    assert_eq!(reader.type_table[1], Type::Integer(64));
}

#[test]
fn winnow_parse_global_var_record() {
    let values = [0u64, 0, 1, 1, 3]; // ptr_ty=0, addr=0, const=true, init=true, internal
    let mut input: RecordInput<'_> = &values;
    let record = parse_global_var_record(&mut input).expect("should parse global var");
    assert!(record.is_const);
    assert!(record.legacy_placeholder);
    assert_eq!(record.init_value_id, None);
    assert!(matches!(record.linkage, Linkage::Internal));
    assert_eq!(record.elem_type_id, None); // no trailing element type in short record
}
