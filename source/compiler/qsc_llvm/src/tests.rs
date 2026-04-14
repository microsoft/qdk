use super::model::Type;
use super::model::test_helpers::*;
use super::model::*;
use super::qir::{QirEmitTarget, QirProfile};
use super::test_utils::{
    LlvmCompatLane, PointerProbe, analyze_bitcode, assemble_text_ir, available_fast_matrix_lanes,
    disassemble_bitcode, verify_bitcode,
};
use super::{
    QirProfilePreset, QirSmithConfig, ReadPolicy, generate_module_from_bytes,
    parse_bitcode_compatibility_report, parse_bitcode_detailed, parse_module,
    write_bitcode_for_target, write_module_to_string,
};

#[test]
fn round_trip_empty_module() {
    let m = empty_module();
    let text1 = write_module_to_string(&m);
    let parsed = parse_module(&text1).expect("failed to parse module");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text1, text2);
}

#[test]
fn round_trip_bell_module_v2() {
    let m = bell_module_v2();
    let text1 = write_module_to_string(&m);
    let parsed = parse_module(&text1).expect("failed to parse module");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text1, text2);
    assert_eq!(m, parsed);
}

#[allow(clippy::too_many_lines)]
#[test]
fn text_to_model_preserves_all_constructs() {
    // Build a comprehensive module with every construct type
    let m = Module {
        source_filename: Some("qir".to_string()),
        target_datalayout: None,
        target_triple: None,
        struct_types: vec![StructType {
            name: "Qubit".to_string(),
            is_opaque: true,
        }],
        globals: vec![GlobalVariable {
            name: "0".to_string(),
            ty: Type::Array(4, Box::new(Type::Integer(8))),
            linkage: Linkage::Internal,
            is_constant: true,
            initializer: Some(Constant::CString("0_r".to_string())),
        }],
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
                name: "ENTRYPOINT__main".to_string(),
                return_type: Type::Integer(64),
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: vec![0],
                basic_blocks: vec![
                    BasicBlock {
                        name: "block_0".to_string(),
                        instructions: vec![
                            Instruction::Call {
                                return_ty: None,
                                callee: "__quantum__qis__h__body".to_string(),
                                args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
                                result: None,
                                attr_refs: Vec::new(),
                            },
                            Instruction::BinOp {
                                op: BinOpKind::Add,
                                ty: Type::Integer(64),
                                lhs: Operand::IntConst(Type::Integer(64), 1),
                                rhs: Operand::IntConst(Type::Integer(64), 2),
                                result: "var_0".to_string(),
                            },
                            Instruction::ICmp {
                                pred: IntPredicate::Slt,
                                ty: Type::Integer(64),
                                lhs: Operand::LocalRef("var_0".to_string()),
                                rhs: Operand::IntConst(Type::Integer(64), 10),
                                result: "var_1".to_string(),
                            },
                            Instruction::Br {
                                cond_ty: Type::Integer(1),
                                cond: Operand::LocalRef("var_1".to_string()),
                                true_dest: "block_1".to_string(),
                                false_dest: "block_2".to_string(),
                            },
                        ],
                    },
                    BasicBlock {
                        name: "block_1".to_string(),
                        instructions: vec![Instruction::Jump {
                            dest: "block_2".to_string(),
                        }],
                    },
                    BasicBlock {
                        name: "block_2".to_string(),
                        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
                            Type::Integer(64),
                            0,
                        )))],
                    },
                ],
            },
        ],
        attribute_groups: vec![AttributeGroup {
            id: 0,
            attributes: vec![
                Attribute::StringAttr("entry_point".to_string()),
                Attribute::KeyValue("qir_profiles".to_string(), "adaptive_profile".to_string()),
            ],
        }],
        named_metadata: vec![NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![0, 1],
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
                    MetadataValue::Int(Type::Integer(32), 5),
                    MetadataValue::String("int_computations".to_string()),
                    MetadataValue::SubList(vec![MetadataValue::String("i64".to_string())]),
                ],
            },
        ],
    };

    let text1 = write_module_to_string(&m);
    let parsed = parse_module(&text1).expect("failed to parse comprehensive module");
    let text2 = write_module_to_string(&parsed);

    // Text round-trip
    assert_eq!(text1, text2);

    // Model equality
    assert_eq!(m.source_filename, parsed.source_filename);
    assert_eq!(m.struct_types, parsed.struct_types);
    assert_eq!(m.globals, parsed.globals);
    assert_eq!(m.functions.len(), parsed.functions.len());
    assert_eq!(m.attribute_groups, parsed.attribute_groups);
    assert_eq!(m.named_metadata, parsed.named_metadata);
    assert_eq!(m.metadata_nodes, parsed.metadata_nodes);
    assert_eq!(m, parsed);
}

// --- Cross-format round-trip tests (Step 6.2) ---

#[test]
fn text_to_bitcode_to_text_round_trip() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    // Build a module, emit text, parse text, write bitcode, read bitcode, emit text again
    let m = bell_module_v2();
    let text1 = write_module_to_string(&m);
    let parsed_text = parse_module(&text1).expect("text parse failed");
    let bc = write_bitcode(&parsed_text);
    let parsed_bc = parse_bitcode(&bc).expect("bitcode parse failed");

    // Compare structural properties that survive bitcode round-trip
    assert_eq!(parsed_bc.functions.len(), m.functions.len());
    for (orig, parsed) in m.functions.iter().zip(parsed_bc.functions.iter()) {
        assert_eq!(orig.name, parsed.name);
        assert_eq!(orig.is_declaration, parsed.is_declaration);
        assert_eq!(orig.params.len(), parsed.params.len());
        assert_eq!(orig.attribute_group_refs, parsed.attribute_group_refs);
    }
    assert_eq!(parsed_bc.attribute_groups, m.attribute_groups);
    assert_eq!(parsed_bc.named_metadata, m.named_metadata);
    assert_eq!(parsed_bc.metadata_nodes, m.metadata_nodes);
}

#[test]
fn bitcode_roundtrip_attribute_groups() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "__quantum__qis__m__body".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Ptr,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: vec![1],
                basic_blocks: Vec::new(),
            },
            Function {
                name: "ENTRYPOINT__main".to_string(),
                return_type: Type::Integer(64),
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: vec![0],
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Ret(Some(Operand::IntConst(
                        Type::Integer(64),
                        0,
                    )))],
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
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let bc1 = write_bitcode(&m);
    let parsed = parse_bitcode(&bc1).expect("parse failed");
    assert_eq!(parsed.attribute_groups, m.attribute_groups);
    assert_eq!(
        parsed.functions[0].attribute_group_refs,
        m.functions[0].attribute_group_refs
    );
    assert_eq!(
        parsed.functions[1].attribute_group_refs,
        m.functions[1].attribute_group_refs
    );
    let bc2 = write_bitcode(&parsed);
    assert_eq!(bc1, bc2);
}

#[test]
fn bitcode_roundtrip_call_site_attr_refs() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

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
                params: Vec::new(),
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
                            callee: "callee".to_string(),
                            args: Vec::new(),
                            result: None,
                            attr_refs: vec![0, 1],
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: vec![
            AttributeGroup {
                id: 0,
                attributes: vec![Attribute::StringAttr("alwaysinline".to_string())],
            },
            AttributeGroup {
                id: 1,
                attributes: vec![Attribute::StringAttr("noreturn".to_string())],
            },
        ],
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let bc1 = write_bitcode(&m);
    let parsed = parse_bitcode(&bc1).expect("parse failed");

    assert_eq!(parsed.attribute_groups, m.attribute_groups);
    assert!(matches!(
        &parsed.functions[1].basic_blocks[0].instructions[0],
        Instruction::Call { attr_refs, .. } if attr_refs == &vec![0, 1]
    ));

    let bc2 = write_bitcode(&parsed);
    assert_eq!(bc1, bc2);
}

#[test]
fn module_flags_roundtrip() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let m = bell_module_v2();
    let bc1 = write_bitcode(&m);
    let parsed = parse_bitcode(&bc1).expect("parse failed");

    assert_eq!(parsed.named_metadata, m.named_metadata);
    assert_eq!(parsed.metadata_nodes, m.metadata_nodes);
    assert!(
        parsed
            .get_flag("qir_major_version")
            .is_some_and(|v| *v == MetadataValue::Int(Type::Integer(32), 2))
    );
    assert!(
        parsed
            .get_flag("backwards_branching")
            .is_some_and(|v| *v == MetadataValue::Int(Type::Integer(2), 3))
    );
    assert!(
        parsed
            .get_flag("int_computations")
            .is_some_and(|v| matches!(v, MetadataValue::SubList(_)))
    );
    assert_eq!(parsed.attribute_groups, m.attribute_groups);
    for (i, (a, b)) in m.functions.iter().zip(parsed.functions.iter()).enumerate() {
        assert_eq!(
            a.attribute_group_refs, b.attribute_group_refs,
            "function {i} attribute_group_refs mismatch"
        );
    }
}

#[test]
fn bitcode_round_trip_empty_module() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let m = empty_module();
    let bc1 = write_bitcode(&m);
    let parsed = parse_bitcode(&bc1).expect("first parse failed");
    let bc2 = write_bitcode(&parsed);
    // Second bitcode should be identical to first
    assert_eq!(bc1, bc2);
}

#[test]
fn bitcode_round_trip_declarations() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "__quantum__qis__h__body".to_string(),
                return_type: Type::Void,
                params: vec![super::model::Param {
                    ty: Type::Ptr,
                    name: None,
                }],
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
                    instructions: vec![Instruction::Ret(None)],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let bc1 = write_bitcode(&m);
    let parsed = parse_bitcode(&bc1).expect("parse failed");
    assert_eq!(parsed.functions.len(), 2);
    assert_eq!(parsed.functions[0].name, "__quantum__qis__h__body");
    assert!(parsed.functions[0].is_declaration);
    assert_eq!(parsed.functions[1].name, "main");
    assert!(!parsed.functions[1].is_declaration);
    assert_eq!(parsed.functions[1].basic_blocks.len(), 1);
}

// --- LLVM tool verification tests (Step 6.3) ---

#[test]
fn bitcode_accepted_by_llvm_dis() {
    use super::bitcode::writer::write_bitcode;
    use std::io::Write;
    use std::process::Command;

    // Check if llvm-dis is available
    let llvm_dis = Command::new("llvm-dis").arg("--version").output();
    if llvm_dis.is_err() || !llvm_dis.expect("llvm-dis check failed").status.success() {
        eprintln!("llvm-dis not available, skipping test");
        return;
    }

    let m = bell_module_v2();
    let bc = write_bitcode(&m);

    let tmp = std::env::temp_dir().join("qsc_test_llvm_dis.bc");
    let mut f = std::fs::File::create(&tmp).expect("failed to create temp file");
    f.write_all(&bc).expect("failed to write bitcode");
    drop(f);

    let output = Command::new("llvm-dis")
        .arg(&tmp)
        .arg("-o")
        .arg("-")
        .output()
        .expect("failed to run llvm-dis");

    std::fs::remove_file(&tmp).ok();

    assert!(
        output.status.success(),
        "llvm-dis failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn text_ir_accepted_by_llvm_as() {
    use std::io::Write;
    use std::process::Command;

    let llvm_as = Command::new("llvm-as").arg("--version").output();
    if llvm_as.is_err() || !llvm_as.expect("llvm-as check failed").status.success() {
        eprintln!("llvm-as not available, skipping test");
        return;
    }

    let m = bell_module_v2();
    let text = write_module_to_string(&m);

    let tmp = std::env::temp_dir().join("qsc_test_llvm_as.ll");
    let mut f = std::fs::File::create(&tmp).expect("failed to create temp file");
    f.write_all(text.as_bytes())
        .expect("failed to write text IR");
    drop(f);

    let output = Command::new("llvm-as")
        .arg(&tmp)
        .arg("-o")
        .arg("/dev/null")
        .output()
        .expect("failed to run llvm-as");

    std::fs::remove_file(&tmp).ok();

    assert!(
        output.status.success(),
        "llvm-as failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bitcode_analyzable_by_llvm_bcanalyzer() {
    use super::bitcode::writer::write_bitcode;
    use std::io::Write;
    use std::process::Command;

    let llvm_bc = Command::new("llvm-bcanalyzer").arg("--version").output();
    if llvm_bc.is_err()
        || !llvm_bc
            .expect("llvm-bcanalyzer check failed")
            .status
            .success()
    {
        eprintln!("llvm-bcanalyzer not available, skipping test");
        return;
    }

    let m = bell_module_v2();
    let bc = write_bitcode(&m);

    let tmp = std::env::temp_dir().join("qsc_test_bcanalyzer.bc");
    let mut f = std::fs::File::create(&tmp).expect("failed to create temp file");
    f.write_all(&bc).expect("failed to write bitcode");
    drop(f);

    let output = Command::new("llvm-bcanalyzer")
        .arg(&tmp)
        .output()
        .expect("failed to run llvm-bcanalyzer");

    std::fs::remove_file(&tmp).ok();

    assert!(
        output.status.success(),
        "llvm-bcanalyzer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// --- Phase 1 round-trip tests for new variants ---

fn round_trip_text(ir: &str) {
    let parsed = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let text = write_module_to_string(&parsed);
    let reparsed = parse_module(&text).unwrap_or_else(|e| panic!("reparse failed: {e}"));
    let text2 = write_module_to_string(&reparsed);
    assert_eq!(text, text2);
}

fn wrap_instr(body: &str) -> String {
    format!(
        "\
declare void @dummy()

define void @test() {{
entry:
{body}
  ret void
}}
"
    )
}

fn wrap_instr_i64(body: &str) -> String {
    format!(
        "\
declare void @dummy()

define i64 @test(i64 %a, i64 %b) {{
entry:
{body}
  ret i64 %r
}}
"
    )
}

#[test]
fn round_trip_select() {
    let ir = "\
declare void @dummy()

define i64 @test(i64 %a, i64 %b) {
entry:
  %cond = icmp slt i64 %a, %b
  %r = select i1 %cond, i64 %a, i64 %b
  ret i64 %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_switch() {
    let ir = "\
declare void @dummy()

define void @test(i32 %val) {
entry:
  switch i32 %val, label %default [
    i32 0, label %case0
    i32 1, label %case1
  ]
case0:
  ret void
case1:
  ret void
default:
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_unreachable() {
    let ir = wrap_instr("  unreachable");
    round_trip_text(&ir);
}

#[test]
fn round_trip_udiv() {
    let ir = wrap_instr_i64("  %r = udiv i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_urem() {
    let ir = wrap_instr_i64("  %r = urem i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_lshr() {
    let ir = wrap_instr_i64("  %r = lshr i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_zext() {
    let ir = "\
declare void @dummy()

define i64 @test(i32 %a) {
entry:
  %r = zext i32 %a to i64
  ret i64 %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_sext() {
    let ir = "\
declare void @dummy()

define i64 @test(i32 %a) {
entry:
  %r = sext i32 %a to i64
  ret i64 %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_trunc() {
    let ir = "\
declare void @dummy()

define i32 @test(i64 %a) {
entry:
  %r = trunc i64 %a to i32
  ret i32 %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fpext() {
    let ir = "\
declare void @use_double(double)

define void @test() {
entry:
  %r = fpext double 1.0 to double
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fptrunc() {
    let ir = "\
declare void @use_double(double)

define void @test() {
entry:
  %r = fptrunc double 1.0 to double
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_inttoptr_cast() {
    let ir = "\
declare void @dummy()

define ptr @test(i64 %a) {
entry:
  %r = inttoptr i64 %a to ptr
  ret ptr %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_ptrtoint() {
    let ir = "\
declare void @dummy()

define i64 @test(ptr %a) {
entry:
  %r = ptrtoint ptr %a to i64
  ret i64 %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_bitcast() {
    let ir = "\
declare void @dummy()

define ptr @test(ptr %a) {
entry:
  %r = bitcast ptr %a to ptr
  ret ptr %r
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_icmp_ult() {
    let ir = wrap_instr_i64("  %c = icmp ult i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_ule() {
    let ir = wrap_instr_i64("  %c = icmp ule i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_ugt() {
    let ir = wrap_instr_i64("  %c = icmp ugt i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_uge() {
    let ir = wrap_instr_i64("  %c = icmp uge i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn module_get_flag() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
        attribute_groups: Vec::new(),
        named_metadata: vec![NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![0, 1],
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
                    MetadataValue::Int(Type::Integer(32), 5),
                    MetadataValue::String("qir_minor_version".to_string()),
                    MetadataValue::Int(Type::Integer(32), 0),
                ],
            },
        ],
    };

    assert_eq!(
        m.get_flag("qir_major_version"),
        Some(&MetadataValue::Int(Type::Integer(32), 2))
    );
    assert_eq!(
        m.get_flag("qir_minor_version"),
        Some(&MetadataValue::Int(Type::Integer(32), 0))
    );
    assert_eq!(m.get_flag("nonexistent"), None);
}

#[test]
fn module_get_flag_skips_dangling_module_flag_refs() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
        attribute_groups: Vec::new(),
        named_metadata: vec![NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![999, 0, 1],
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
                    MetadataValue::Int(Type::Integer(32), 0),
                ],
            },
        ],
    };

    assert_eq!(
        m.get_flag("qir_major_version"),
        Some(&MetadataValue::Int(Type::Integer(32), 2))
    );
    assert_eq!(
        m.get_flag("qir_minor_version"),
        Some(&MetadataValue::Int(Type::Integer(32), 0))
    );
}

#[test]
fn bitcode_self_round_trip_comprehensive() {
    // Verify text→parse→write_bitcode→parse_bitcode→write_text round-trip
    // with a comprehensive module exercising many construct types.
    // Our writer produces UNABBREV_RECORD only; this verifies the new
    // abbreviation infrastructure (scope tracking etc.) does not break
    // existing record reading.
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let m = Module {
        source_filename: None,
        target_datalayout: Some(
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128".to_string(),
        ),
        target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
        struct_types: vec![StructType {
            name: "Qubit".to_string(),
            is_opaque: true,
        }],
        globals: Vec::new(),
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
                name: "main".to_string(),
                return_type: Type::Integer(64),
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "__quantum__qis__h__body".to_string(),
                            args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::IntConst(Type::Integer(64), 1),
                            rhs: Operand::IntConst(Type::Integer(64), 2),
                            result: "var_0".to_string(),
                        },
                        Instruction::Ret(Some(Operand::LocalRef("var_0".to_string()))),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let bc1 = write_bitcode(&m);
    let parsed1 = parse_bitcode(&bc1).expect("first bitcode parse failed");

    // Verify structural properties survive the round-trip
    assert_eq!(parsed1.functions.len(), m.functions.len());
    assert_eq!(parsed1.target_triple, m.target_triple);
    assert_eq!(parsed1.target_datalayout, m.target_datalayout);
    for (orig, parsed) in m.functions.iter().zip(parsed1.functions.iter()) {
        assert_eq!(orig.name, parsed.name);
        assert_eq!(orig.is_declaration, parsed.is_declaration);
        assert_eq!(orig.params.len(), parsed.params.len());
        assert_eq!(orig.basic_blocks.len(), parsed.basic_blocks.len());
    }

    // Verify re-encoding the parsed module produces structurally equivalent output
    let bc2 = write_bitcode(&parsed1);
    let parsed2 = parse_bitcode(&bc2).expect("second bitcode parse failed");
    assert_eq!(parsed1.functions.len(), parsed2.functions.len());
    assert_eq!(parsed1.target_triple, parsed2.target_triple);
    assert_eq!(parsed1.target_datalayout, parsed2.target_datalayout);
    for (f1, f2) in parsed1.functions.iter().zip(parsed2.functions.iter()) {
        assert_eq!(f1.name, f2.name);
        assert_eq!(f1.is_declaration, f2.is_declaration);
        assert_eq!(f1.basic_blocks.len(), f2.basic_blocks.len());
    }
}

#[test]
fn bitcode_llvm_as_round_trip() {
    // If llvm-as is available, produce bitcode with LLVM (which uses abbreviations)
    // and verify our reader can parse it back.
    use super::bitcode::reader::parse_bitcode;
    let lane = LlvmCompatLane::LLVM_21;
    if !lane.is_available() {
        eprintln!("llvm@21 toolchain not available, skipping test");
        return;
    }

    let text = "\
; ModuleID = 'test'\n\
target triple = \"x86_64-unknown-linux-gnu\"\n\
\n\
%Qubit = type opaque\n\
\n\
declare void @__quantum__qis__h__body(ptr)\n\
\n\
define void @main() {\n\
entry:\n\
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))\n\
  ret void\n\
}\n";

    let bc_data = assemble_text_ir(lane, PointerProbe::OpaqueText, text)
        .unwrap_or_else(|error| panic!("llvm@21 llvm-as failed: {error}"));

    // The LLVM-produced bitcode will contain abbreviations.
    // This tests our new DEFINE_ABBREV + abbreviated record support.
    let parsed = parse_bitcode(&bc_data).expect("failed to parse LLVM-produced bitcode");
    assert_eq!(parsed.functions.len(), 2);
    assert!(
        parsed
            .functions
            .iter()
            .any(|f| f.name == "__quantum__qis__h__body")
    );
    assert!(parsed.functions.iter().any(|f| f.name == "main"));

    let main = parsed
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("missing main function");
    assert!(matches!(
        &main.basic_blocks[0].instructions[0],
        Instruction::Call { callee, .. } if callee == "__quantum__qis__h__body"
    ));
}

fn llvm_modern_module_naming_fixture_ir() -> &'static str {
    "\
target triple = \"x86_64-unknown-linux-gnu\"\n\
\n\
define i64 @test(i64 %arg, i64 %other) {\n\
entry:\n\
  %sum = add i64 %arg, %other\n\
  br label %loop\n\
loop:\n\
  %acc = phi i64 [ %sum, %entry ], [ %next, %loop ]\n\
  %next = add i64 %acc, 1\n\
  %cond = icmp slt i64 %next, 10\n\
  br i1 %cond, label %loop, label %exit\n\
exit:\n\
  ret i64 %next\n\
}\n"
}

#[test]
fn bitcode_llvm_modern_module_naming_fixture_preserves_names() {
    let lane = LlvmCompatLane::LLVM_21;
    if !lane.is_available() || !lane.has_tool("llvm-bcanalyzer") {
        eprintln!("llvm@21 with llvm-bcanalyzer not available, skipping test");
        return;
    }

    let bc_data = assemble_text_ir(
        lane,
        PointerProbe::OpaqueText,
        llvm_modern_module_naming_fixture_ir(),
    )
    .unwrap_or_else(|error| panic!("llvm@21 llvm-as failed: {error}"));

    let analysis = analyze_bitcode(lane, &bc_data)
        .unwrap_or_else(|error| panic!("llvm@21 llvm-bcanalyzer failed: {error}"));
    for expected in [
        "<BLOCKINFO_BLOCK/>",
        "<VSTOFFSET",
        "<FNENTRY",
        "<STRTAB_BLOCK",
        "record string = 'arg'",
        "record string = 'entry'",
        "blob data = 'test",
    ] {
        assert!(
            analysis.contains(expected),
            "expected llvm-bcanalyzer dump to contain {expected:?}, got:\n{analysis}"
        );
    }

    let parsed = super::parse_bitcode(&bc_data)
        .expect("failed to parse LLVM 21 modern naming fixture bitcode");
    let errors = super::validate_ir(&parsed);
    assert!(
        errors.is_empty(),
        "parsed fixture should validate: {errors:?}"
    );

    let function = parsed
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    assert_eq!(function.params.len(), 2);
    assert_eq!(function.params[0].name.as_deref(), Some("arg"));
    assert_eq!(function.params[1].name.as_deref(), Some("other"));
    assert_eq!(
        function
            .basic_blocks
            .iter()
            .map(|block| block.name.as_str())
            .collect::<Vec<_>>(),
        vec!["entry", "loop", "exit"]
    );
    assert!(matches!(
        &function.basic_blocks[0].instructions[0],
        Instruction::BinOp { result, .. } if result == "sum"
    ));
    assert!(matches!(
        &function.basic_blocks[1].instructions[0],
        Instruction::Phi { result, incoming, .. }
            if result == "acc"
                && incoming.len() == 2
                && incoming[0].1 == "entry"
                && incoming[1].1 == "loop"
    ));
    assert!(matches!(
        &function.basic_blocks[1].instructions[1],
        Instruction::BinOp { result, .. } if result == "next"
    ));
    assert!(matches!(
        &function.basic_blocks[1].instructions[2],
        Instruction::ICmp { result, .. } if result == "cond"
    ));
    assert!(matches!(
        &function.basic_blocks[1].instructions[3],
        Instruction::Br {
            true_dest,
            false_dest,
            ..
        } if true_dest == "loop" && false_dest == "exit"
    ));
}

fn qir_typed_pointer_smoke_ir() -> &'static str {
    "\
%Qubit = type opaque\n\
\n\
declare void @__quantum__qis__h__body(%Qubit*)\n\
\n\
define void @main() {\n\
entry:\n\
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))\n\
  ret void\n\
}\n"
}

fn qir_opaque_pointer_smoke_ir() -> &'static str {
    "\
declare void @__quantum__qis__h__body(ptr)\n\
\n\
define void @main() {\n\
entry:\n\
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))\n\
  ret void\n\
}\n"
}

fn qir_typed_pointer_smoke_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: QirProfile::AdaptiveV1.struct_types(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "__quantum__qis__h__body".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::NamedPtr("Qubit".to_string()),
                    name: None,
                }],
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
                            callee: "__quantum__qis__h__body".to_string(),
                            args: vec![(
                                Type::NamedPtr("Qubit".to_string()),
                                Operand::int_to_named_ptr(0, "Qubit"),
                            )],
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

fn qir_opaque_pointer_smoke_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
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
                            callee: "__quantum__qis__h__body".to_string(),
                            args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
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

fn qir_typed_pointer_gep_smoke_module() -> Module {
    let array_ty = Type::Array(4, Box::new(Type::Integer(8)));

    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: vec![GlobalVariable {
            name: "0".to_string(),
            ty: array_ty.clone(),
            linkage: Linkage::Internal,
            is_constant: true,
            initializer: Some(Constant::CString("abc".to_string())),
        }],
        functions: vec![
            Function {
                name: "use_ptr".to_string(),
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
                            callee: "use_ptr".to_string(),
                            args: vec![(
                                Type::Ptr,
                                Operand::GetElementPtr {
                                    ty: array_ty.clone(),
                                    ptr: "0".to_string(),
                                    ptr_ty: Type::TypedPtr(Box::new(array_ty.clone())),
                                    indices: vec![
                                        Operand::IntConst(Type::Integer(64), 0),
                                        Operand::IntConst(Type::Integer(64), 0),
                                    ],
                                },
                            )],
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

fn main_gep_ptr_ty(module: &Module) -> &Type {
    match &module.functions[1].basic_blocks[0].instructions[0] {
        Instruction::Call { args, .. } => match &args[0].1 {
            Operand::GetElementPtr { ptr_ty, .. } => ptr_ty,
            other => panic!("expected getelementptr operand, found {other:?}"),
        },
        other => panic!("expected leading call instruction, found {other:?}"),
    }
}

fn adaptive_half_only_float_external_fixture_ir() -> &'static str {
    "declare void @use_half(half)\n\
\n\
define i64 @ENTRYPOINT__main() #0 {\n\
entry:\n\
  %half_sum = fadd half 1.5, 2.25\n\
  call void @use_half(half %half_sum)\n\
  ret i64 0\n\
}\n\
\n\
attributes #0 = { \"entry_point\" \"output_labeling_schema\" \"qir_profiles\"=\"adaptive_profile\" \"required_num_qubits\"=\"0\" \"required_num_results\"=\"0\" }\n\
\n\
!llvm.module.flags = !{!0, !1, !2, !3, !4}\n\
!0 = !{i32 1, !\"qir_major_version\", i32 2}\n\
!1 = !{i32 7, !\"qir_minor_version\", i32 0}\n\
!2 = !{i32 1, !\"dynamic_qubit_management\", i1 false}\n\
!3 = !{i32 1, !\"dynamic_result_management\", i1 false}\n\
!4 = !{i32 5, !\"float_computations\", !{!\"half\"}}\n"
}

fn adaptive_no_float_external_fixture_ir() -> &'static str {
    "define i64 @ENTRYPOINT__main() #0 {\n\
entry:\n\
  ret i64 0\n\
}\n\
\n\
attributes #0 = { \"entry_point\" \"output_labeling_schema\" \"qir_profiles\"=\"adaptive_profile\" \"required_num_qubits\"=\"0\" \"required_num_results\"=\"0\" }\n\
\n\
!llvm.module.flags = !{!0, !1, !2, !3}\n\
!0 = !{i32 1, !\"qir_major_version\", i32 2}\n\
!1 = !{i32 7, !\"qir_minor_version\", i32 0}\n\
!2 = !{i32 1, !\"dynamic_qubit_management\", i1 false}\n\
!3 = !{i32 1, !\"dynamic_result_management\", i1 false}\n"
}

fn assert_opaque_qir_text_fixture_survives_fast_matrix(
    text: &str,
    expected_substrings: &[&str],
    absent_substrings: &[&str],
) {
    let lanes = available_fast_matrix_lanes();
    if lanes.is_empty() {
        eprintln!("external LLVM fast matrix not available, skipping test");
        return;
    }

    for lane in lanes {
        let bitcode =
            assemble_text_ir(lane, PointerProbe::OpaqueText, text).unwrap_or_else(|error| {
                panic!("llvm@{} opaque assemble failed: {error}", lane.version)
            });

        verify_bitcode(lane, PointerProbe::OpaqueText, &bitcode)
            .unwrap_or_else(|error| panic!("llvm@{} opaque verify failed: {error}", lane.version));

        let disassembly = disassemble_bitcode(lane, PointerProbe::OpaqueText, &bitcode)
            .unwrap_or_else(|error| {
                panic!("llvm@{} opaque disassembly failed: {error}", lane.version)
            });

        for expected in expected_substrings {
            assert!(
                disassembly.contains(expected),
                "llvm@{} disassembly should contain {expected:?}, got:\n{disassembly}",
                lane.version
            );
        }

        for absent in absent_substrings {
            assert!(
                !disassembly.contains(absent),
                "llvm@{} disassembly should not contain {absent:?}, got:\n{disassembly}",
                lane.version
            );
        }
    }
}

#[test]
fn qir_explicit_typed_emit_target_roundtrips_named_pointer_module() {
    use super::bitcode::reader::parse_bitcode;

    let module = qir_typed_pointer_smoke_module();
    let bitcode = write_bitcode_for_target(&module, QirEmitTarget::QirV1Typed);
    let round_tripped = parse_bitcode(&bitcode).expect("typed lane parse failed");

    assert_eq!(
        round_tripped.functions[0].params[0].ty,
        Type::NamedPtr("Qubit".into())
    );
    match &round_tripped.functions[1].basic_blocks[0].instructions[0] {
        Instruction::Call { args, .. } => assert_eq!(
            args,
            &vec![(
                Type::NamedPtr("Qubit".into()),
                Operand::int_to_named_ptr(0, "Qubit"),
            )]
        ),
        other => panic!("expected typed-lane call instruction, found {other:?}"),
    }
}

#[test]
fn qir_explicit_opaque_emit_target_roundtrips_opaque_pointer_module() {
    use super::bitcode::reader::parse_bitcode;

    let module = qir_opaque_pointer_smoke_module();
    let bitcode = write_bitcode_for_target(&module, QirEmitTarget::QirV2Opaque);
    let round_tripped = parse_bitcode(&bitcode).expect("opaque lane parse failed");

    assert_eq!(round_tripped.functions[0].params[0].ty, Type::Ptr);
    match &round_tripped.functions[1].basic_blocks[0].instructions[0] {
        Instruction::Call { callee, args, .. } => {
            assert_eq!(callee, "__quantum__qis__h__body");
            assert_eq!(args, &vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))]);
        }
        other => panic!("expected opaque-lane call instruction, found {other:?}"),
    }
}

#[test]
fn typed_pointer_gep_text_roundtrip_preserves_base_pointer_type() {
    let module = qir_typed_pointer_gep_smoke_module();
    let text = write_module_to_string(&module);
    let parsed = parse_module(&text).expect("typed-pointer GEP text should parse");
    let expected = Type::TypedPtr(Box::new(Type::Array(4, Box::new(Type::Integer(8)))));

    assert_eq!(main_gep_ptr_ty(&module), &expected);
    assert_eq!(main_gep_ptr_ty(&parsed), &expected);
}

#[test]
fn qir_emitted_opaque_bitcode_verifies_across_external_opaque_lanes() {
    const REQUIRED_OPAQUE_LANES: [u8; 2] = [16, 21];

    let available_lanes = available_fast_matrix_lanes();
    let missing_lanes: Vec<_> = REQUIRED_OPAQUE_LANES
        .into_iter()
        .filter(|version| !available_lanes.iter().any(|lane| lane.version == *version))
        .collect();
    if !missing_lanes.is_empty() {
        let missing = missing_lanes
            .into_iter()
            .map(|version| format!("llvm@{version}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "required external LLVM opaque lanes not available, skipping emitted opaque verification: {missing}"
        );
        return;
    }

    let module = qir_opaque_pointer_smoke_module();
    let bitcode = write_bitcode_for_target(&module, QirEmitTarget::QirV2Opaque);

    for lane in available_lanes
        .into_iter()
        .filter(|lane| REQUIRED_OPAQUE_LANES.contains(&lane.version))
    {
        verify_bitcode(lane, PointerProbe::OpaqueText, &bitcode).unwrap_or_else(|error| {
            let reproducer = disassemble_bitcode(lane, PointerProbe::OpaqueText, &bitcode)
                .map(|disassembly| format!("disassembly:\n{disassembly}"))
                .or_else(|disassembly_error| {
                    analyze_bitcode(lane, &bitcode).map(|dump| {
                        format!(
                            "disassembly failed: {disassembly_error}\nllvm-bcanalyzer dump:\n{dump}"
                        )
                    })
                })
                .unwrap_or_else(|analyzer_error| {
                    format!(
                        "disassembly and llvm-bcanalyzer both failed while preparing a reproducer: {analyzer_error}"
                    )
                });
            panic!(
                "llvm@{} rejected qsc_llvm-emitted opaque bitcode: {error}\n{reproducer}",
                lane.version
            );
        });
    }
}

#[test]
fn qir_emitted_opaque_bitcode_uses_modern_module_function_naming_records() {
    let lane = LlvmCompatLane::LLVM_21;
    if !lane.is_available() || !lane.has_tool("llvm-bcanalyzer") {
        eprintln!("llvm@21 with llvm-bcanalyzer not available, skipping test");
        return;
    }

    let module = qir_opaque_pointer_smoke_module();
    let bitcode = write_bitcode_for_target(&module, QirEmitTarget::QirV2Opaque);

    let analysis = analyze_bitcode(lane, &bitcode)
        .unwrap_or_else(|error| panic!("llvm@21 llvm-bcanalyzer failed: {error}"));

    for expected in ["<VSTOFFSET", "<FNENTRY", "<STRTAB_BLOCK"] {
        assert!(
            analysis.contains(expected),
            "expected llvm-bcanalyzer dump to contain {expected:?}, got:\n{analysis}"
        );
    }
}

#[test]
fn qir_external_llvm_fast_matrix_helpers_cover_expected_pointer_lanes() {
    let lanes = available_fast_matrix_lanes();
    if lanes.is_empty() {
        eprintln!("external LLVM fast matrix not available, skipping test");
        return;
    }

    for lane in lanes {
        match lane.version {
            14 => {
                let typed_bc =
                    assemble_text_ir(lane, PointerProbe::TypedText, qir_typed_pointer_smoke_ir())
                        .unwrap_or_else(|error| panic!("llvm@14 typed assemble failed: {error}"));
                verify_bitcode(lane, PointerProbe::TypedText, &typed_bc)
                    .unwrap_or_else(|error| panic!("llvm@14 typed verify failed: {error}"));

                let opaque_bc = assemble_text_ir(
                    lane,
                    PointerProbe::OpaqueText,
                    qir_opaque_pointer_smoke_ir(),
                )
                .unwrap_or_else(|error| panic!("llvm@14 opaque assemble failed: {error}"));
                verify_bitcode(lane, PointerProbe::OpaqueText, &opaque_bc)
                    .unwrap_or_else(|error| panic!("llvm@14 opaque verify failed: {error}"));
            }
            15 => {
                let typed_bc =
                    assemble_text_ir(lane, PointerProbe::TypedText, qir_typed_pointer_smoke_ir())
                        .unwrap_or_else(|error| panic!("llvm@15 typed assemble failed: {error}"));
                verify_bitcode(lane, PointerProbe::TypedText, &typed_bc)
                    .unwrap_or_else(|error| panic!("llvm@15 typed verify failed: {error}"));
                let typed_disassembly =
                    disassemble_bitcode(lane, PointerProbe::TypedText, &typed_bc).unwrap_or_else(
                        |error| panic!("llvm@15 typed disassembly failed: {error}"),
                    );
                assert!(
                    typed_disassembly.contains("%Qubit*"),
                    "llvm@15 bridge lane should preserve typed spelling, got:\n{typed_disassembly}"
                );

                let opaque_bc = assemble_text_ir(
                    lane,
                    PointerProbe::OpaqueText,
                    qir_opaque_pointer_smoke_ir(),
                )
                .unwrap_or_else(|error| panic!("llvm@15 opaque assemble failed: {error}"));
                verify_bitcode(lane, PointerProbe::OpaqueText, &opaque_bc)
                    .unwrap_or_else(|error| panic!("llvm@15 opaque verify failed: {error}"));
                let opaque_disassembly =
                    disassemble_bitcode(lane, PointerProbe::OpaqueText, &opaque_bc).unwrap_or_else(
                        |error| panic!("llvm@15 opaque disassembly failed: {error}"),
                    );
                assert!(
                    opaque_disassembly.contains("ptr"),
                    "llvm@15 bridge lane should preserve opaque spelling, got:\n{opaque_disassembly}"
                );
            }
            16 | 21 => {
                let opaque_bc = assemble_text_ir(
                    lane,
                    PointerProbe::OpaqueText,
                    qir_opaque_pointer_smoke_ir(),
                )
                .unwrap_or_else(|error| {
                    panic!("llvm@{} opaque assemble failed: {error}", lane.version)
                });
                verify_bitcode(lane, PointerProbe::OpaqueText, &opaque_bc).unwrap_or_else(
                    |error| panic!("llvm@{} opaque verify failed: {error}", lane.version),
                );
            }
            other => panic!("unexpected fast-matrix lane llvm@{other}"),
        }
    }
}

#[test]
fn qir_external_llvm_fast_matrix_accepts_half_only_float_metadata_artifact() {
    assert_opaque_qir_text_fixture_survives_fast_matrix(
        adaptive_half_only_float_external_fixture_ir(),
        &["fadd half", "!\"float_computations\"", "!{!\"half\"}"],
        &["!{!\"half\","],
    );
}

#[test]
fn qir_external_llvm_fast_matrix_accepts_no_float_artifact() {
    assert_opaque_qir_text_fixture_survives_fast_matrix(
        adaptive_no_float_external_fixture_ir(),
        &["ret i64 0", "!\"qir_major_version\""],
        &["!\"float_computations\""],
    );
}

#[test]
fn external_global_initializer_bitcode_is_preserved_in_strict_mode() {
    let Some(lane) = available_fast_matrix_lanes().into_iter().next() else {
        eprintln!(
            "no external LLVM fast-matrix lane is available, skipping unsupported-input bitcode fixture"
        );
        return;
    };

    let bitcode = assemble_text_ir(
        lane,
        PointerProbe::OpaqueText,
        "@0 = internal constant [4 x i8] c\"0_r\\00\"\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "llvm@{} should assemble external global initializer fixture: {error}",
            lane.version
        )
    });

    let module = parse_bitcode_detailed(&bitcode, ReadPolicy::QirSubsetStrict)
        .expect("strict bitcode import should preserve supported global initializers");

    assert_eq!(module.globals.len(), 1);
    assert_eq!(
        module.globals[0].ty,
        Type::Array(4, Box::new(Type::Integer(8)))
    );
    assert!(module.globals[0].is_constant);
    assert_eq!(
        module.globals[0].initializer,
        Some(Constant::CString("0_r".to_string()))
    );
}

#[test]
fn external_global_initializer_bitcode_has_no_compatibility_diagnostics() {
    let Some(lane) = available_fast_matrix_lanes().into_iter().next() else {
        eprintln!(
            "no external LLVM fast-matrix lane is available, skipping unsupported-input bitcode fixture"
        );
        return;
    };

    let bitcode = assemble_text_ir(
        lane,
        PointerProbe::OpaqueText,
        "@0 = internal constant [4 x i8] c\"0_r\\00\"\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "llvm@{} should assemble compatibility-report fixture: {error}",
            lane.version
        )
    });

    let report = parse_bitcode_compatibility_report(&bitcode)
        .expect("compatibility bitcode import should preserve supported global initializers");

    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    assert_eq!(report.module.globals.len(), 1);
    assert_eq!(
        report.module.globals[0].initializer,
        Some(Constant::CString("0_r".to_string()))
    );
}

#[test]
fn round_trip_gep_inbounds() {
    let ir = "\
@str = internal constant [5 x i8] c\"hello\"\n\
\n\
declare void @use_ptr(ptr)\n\
\n\
define void @test_fn() {\n\
entry:\n\
  %0 = getelementptr inbounds [5 x i8], ptr @str, i64 0, i64 0\n\
  call void @use_ptr(ptr %0)\n\
  ret void\n\
}\n";
    let parsed = parse_module(ir).expect("failed to parse GEP IR");
    let text1 = write_module_to_string(&parsed);
    let reparsed = parse_module(&text1).expect("failed to reparse GEP IR");
    let text2 = write_module_to_string(&reparsed);
    assert_eq!(text1, text2);
    assert_eq!(parsed, reparsed);

    // Verify the instruction is a GetElementPtr
    let func = parsed
        .functions
        .iter()
        .find(|f| f.name == "test_fn")
        .expect("missing test_fn");
    let instrs = &func.basic_blocks[0].instructions;
    assert!(
        matches!(
            &instrs[0],
            Instruction::GetElementPtr { inbounds: true, .. }
        ),
        "expected GEP inbounds instruction"
    );
}

#[test]
fn round_trip_gep_no_inbounds() {
    let ir = "\
@label = internal constant [10 x i8] c\"some_label\"\n\
\n\
declare void @use_ptr(ptr)\n\
\n\
define void @test_fn() {\n\
entry:\n\
  %0 = getelementptr [10 x i8], ptr @label, i64 0, i64 0\n\
  call void @use_ptr(ptr %0)\n\
  ret void\n\
}\n";
    let parsed = parse_module(ir).expect("failed to parse GEP IR");
    let text1 = write_module_to_string(&parsed);
    let reparsed = parse_module(&text1).expect("failed to reparse GEP IR");
    let text2 = write_module_to_string(&reparsed);
    assert_eq!(text1, text2);
    assert_eq!(parsed, reparsed);

    // Verify the instruction is a GetElementPtr without inbounds
    let func = parsed
        .functions
        .iter()
        .find(|f| f.name == "test_fn")
        .expect("missing test_fn");
    let instrs = &func.basic_blocks[0].instructions;
    assert!(
        matches!(
            &instrs[0],
            Instruction::GetElementPtr {
                inbounds: false,
                ..
            }
        ),
        "expected GEP non-inbounds instruction"
    );
}

#[test]
fn parse_adaptive_profile_ir() {
    // Test patterns from the adaptive profile tests that caused failures
    let ir = r#"%Result = type opaque
%Qubit = type opaque

define void @make_bell(%Qubit* %q0, %Qubit* %q1) {
entry:
  call void @__quantum__qis__h__body(%Qubit* %q0)
  call void @__quantum__qis__cx__body(%Qubit* %q0, %Qubit* %q1)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions.len(), 3);
}

#[test]
fn parse_icmp_ult_in_context() {
    let ir = r#"define void @test() {
entry:
  %cond = icmp ult i64 %i, 8
  ret void
}
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions.len(), 1);
}

#[test]
fn parse_block_with_comment() {
    let ir = r#"define void @test() {
block_0:
  br label %loop_cond
loop_cond:                                        ; preds = %loop_body, %block_0
  ret void
}
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions[0].basic_blocks.len(), 2);
}

#[test]
fn parse_declare_i1_return() {
    let ir = r#"declare i1 @__quantum__rt__read_loss(%Result*)
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions.len(), 1);
}

#[test]
fn parse_metadata_i1_bool() {
    let ir = r#"!llvm.module.flags = !{!0}
!0 = !{i32 1, !"dynamic_qubit_management", i1 false}
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.metadata_nodes.len(), 1);
}

#[test]
fn parse_phi_with_named_blocks() {
    let ir = r#"define void @test() {
block_0:
  br label %loop_cond
loop_cond:
  %i = phi i64 [ 0, %block_0 ], [ %i_next, %loop_body ]
  ret void
loop_body:
  %i_next = add i64 %i, 1
  br label %loop_cond
}
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions[0].basic_blocks.len(), 3);
}

#[test]
fn parse_metadata_nested_group() {
    let ir = r#"!llvm.module.flags = !{!0}
!0 = !{i32 5, !"int_computations", !{!"i64"}}
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.metadata_nodes.len(), 1);
}

#[test]
fn parse_two_declares_same_line() {
    // Some IR has declarations on consecutive lines without blank lines
    let ir = r#"declare void @__quantum__rt__bool_record_output(i1, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions.len(), 2);
}

#[test]
fn parse_full_bell_loop_funcs() {
    let ir = r#"%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  br label %loop_cond
loop_cond:                                        ; preds = %loop_body, %block_0
  %i = phi i64 [ 0, %block_0 ], [ %i_next, %loop_body ]
  %cond = icmp ult i64 %i, 8
  br i1 %cond, label %loop_body, label %loop_cond2
loop_body:                                        ; preds = %loop_cond
  %q0 = inttoptr i64 %i to %Qubit*
  %i1 = add i64 %i, 1
  %q1 = inttoptr i64 %i1 to %Qubit*
  call void @make_bell(%Qubit* %q0, %Qubit* %q1)
  %i_next = add i64 %i, 2
  br label %loop_cond
loop_cond2:                                       ; preds = %loop_cond
  %i3 = phi i64 [ 0, %loop_cond ], [ %i_next2, %loop_body2 ]
  %cond2 = icmp ult i64 %i3, 16
  br i1 %cond2, label %loop_body2, label %end
loop_body2:                                       ; preds = %loop_cond2
  %q2 = inttoptr i64 %i3 to %Qubit*
  %r = inttoptr i64 %i3 to %Result*
  call void @__quantum__qis__mresetz__body(%Qubit* %q2, %Result* %r)
  %i_next2 = add i64 %i3, 1
  br label %loop_cond2
end:                                              ; preds = %loop_cond2
  call void @__quantum__rt__array_record_output(i64 8, i8* null)
  ret i64 0
}

define void @make_bell(%Qubit* %q0, %Qubit* %q1) {
entry:
  call void @__quantum__qis__h__body(%Qubit* %q0)
  call void @__quantum__qis__cx__body(%Qubit* %q0, %Qubit* %q1)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare void @__quantum__rt__array_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="16" "required_num_results"="16" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"#;
    let m = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(m.functions.len(), 6);
    assert_eq!(m.functions[0].basic_blocks.len(), 6);
}

// --- Phase 2: Text roundtrip tests for untested BinOp variants ---

#[test]
fn round_trip_sub() {
    let ir = wrap_instr_i64("  %r = sub i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_mul() {
    let ir = wrap_instr_i64("  %r = mul i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_sdiv() {
    let ir = wrap_instr_i64("  %r = sdiv i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_srem() {
    let ir = wrap_instr_i64("  %r = srem i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_shl() {
    let ir = wrap_instr_i64("  %r = shl i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_ashr() {
    let ir = wrap_instr_i64("  %r = ashr i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_and() {
    let ir = wrap_instr_i64("  %r = and i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_or() {
    let ir = wrap_instr_i64("  %r = or i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_xor() {
    let ir = wrap_instr_i64("  %r = xor i64 %a, %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_fadd() {
    let ir = "\
declare void @use_double(double)

define void @test(double %a, double %b) {
entry:
  %r = fadd double %a, %b
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fsub() {
    let ir = "\
declare void @use_double(double)

define void @test(double %a, double %b) {
entry:
  %r = fsub double %a, %b
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fmul() {
    let ir = "\
declare void @use_double(double)

define void @test(double %a, double %b) {
entry:
  %r = fmul double %a, %b
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fdiv() {
    let ir = "\
declare void @use_double(double)

define void @test(double %a, double %b) {
entry:
  %r = fdiv double %a, %b
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

// --- Phase 2: Text roundtrip tests for untested ICmp predicates ---

#[test]
fn round_trip_icmp_eq() {
    let ir = wrap_instr_i64("  %c = icmp eq i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_ne() {
    let ir = wrap_instr_i64("  %c = icmp ne i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_sgt() {
    let ir = wrap_instr_i64("  %c = icmp sgt i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_sge() {
    let ir = wrap_instr_i64("  %c = icmp sge i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

#[test]
fn round_trip_icmp_sle() {
    let ir = wrap_instr_i64("  %c = icmp sle i64 %a, %b\n  %r = select i1 %c, i64 %a, i64 %b");
    round_trip_text(&ir);
}

// --- Phase 2: Text roundtrip tests for FCmp predicates ---

#[test]
fn round_trip_fcmp_oeq() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp oeq double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ogt() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ogt double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_oge() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp oge double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_olt() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp olt double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ole() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ole double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_one() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp one double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ord() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ord double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ueq() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ueq double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ugt() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ugt double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_uge() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp uge double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ult() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ult double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_ule() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp ule double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_une() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp une double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fcmp_uno() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp uno double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    round_trip_text(ir);
}

// --- Phase 2: Text roundtrip tests for missing instruction types ---

#[test]
fn round_trip_phi() {
    let ir = "\
define i64 @test() {
block_0:
  br label %loop
loop:
  %i = phi i64 [ 0, %block_0 ], [ %next, %loop ]
  %next = add i64 %i, 1
  %cond = icmp slt i64 %next, 10
  br i1 %cond, label %loop, label %exit
exit:
  ret i64 %i
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_alloca_load_store() {
    let ir = "\
define i64 @test() {
entry:
  %ptr = alloca i64
  store i64 42, ptr %ptr
  %val = load i64, ptr %ptr
  ret i64 %val
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_sitofp() {
    let ir = "\
declare void @use_double(double)

define void @test(i64 %a) {
entry:
  %r = sitofp i64 %a to double
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_fptosi() {
    let ir = "\
define i64 @test(double %a) {
entry:
  %r = fptosi double %a to i64
  ret i64 %r
}
";
    round_trip_text(ir);
}

// --- Phase 2: Text roundtrip tests for missing operand types ---

#[test]
fn round_trip_float_const() {
    let ir = "\
declare void @use_double(double)

define void @test() {
entry:
  %r = fadd double 1.0, 2.0
  call void @use_double(double %r)
  ret void
}
";
    round_trip_text(ir);
}

#[test]
fn round_trip_null_ptr() {
    let ir = "\
declare void @use_ptr(ptr)

define void @test() {
entry:
  call void @use_ptr(ptr null)
  ret void
}
";
    round_trip_text(ir);
}

// --- Phase 3: Bitcode roundtrip tests for instruction types ---

/// Helper: parse text IR, write to bitcode, parse bitcode back, return both modules.
fn bitcode_roundtrip(ir: &str) -> (Module, Module) {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let module = parse_module(ir).unwrap_or_else(|e| panic!("text parse failed: {e}"));
    let bc = write_bitcode(&module);
    let rt = parse_bitcode(&bc).unwrap_or_else(|e| panic!("bitcode parse failed: {e}"));
    (module, rt)
}

/// Helper: assert structural equivalence of functions (non-lossy fields only).
fn assert_bitcode_roundtrip_structure(orig: &Module, rt: &Module) {
    assert_eq!(orig.functions.len(), rt.functions.len());
    for (fo, fr) in orig.functions.iter().zip(rt.functions.iter()) {
        assert_eq!(fo.name, fr.name);
        assert_eq!(fo.is_declaration, fr.is_declaration);
        assert_eq!(fo.params.len(), fr.params.len());
        assert_eq!(fo.basic_blocks.len(), fr.basic_blocks.len());
        for (bo, br) in fo.basic_blocks.iter().zip(fr.basic_blocks.iter()) {
            assert_eq!(
                bo.instructions.len(),
                br.instructions.len(),
                "instruction count mismatch in block '{}' of function '{}'",
                bo.name,
                fo.name,
            );
        }
    }
}

fn qir_smith_v1_config(profile: QirProfilePreset) -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: 4,
        max_instrs_per_block: 6,
        ..QirSmithConfig::for_profile(profile)
    }
}

fn assert_qir_smith_v1_type(ty: &Type) {
    match ty {
        Type::NamedPtr(_) | Type::TypedPtr(_) => {
            panic!("typed-pointer types are outside the v1 qir_smith test boundary")
        }
        Type::Array(_, element) => assert_qir_smith_v1_type(element),
        Type::Function(result, params) => {
            assert_qir_smith_v1_type(result);
            for param in params {
                assert_qir_smith_v1_type(param);
            }
        }
        Type::Void
        | Type::Integer(_)
        | Type::Half
        | Type::Float
        | Type::Double
        | Type::Label
        | Type::Ptr
        | Type::Named(_) => {}
    }
}

fn assert_qir_smith_v1_operand(operand: &Operand) {
    match operand {
        Operand::IntConst(ty, _) | Operand::IntToPtr(_, ty) => assert_qir_smith_v1_type(ty),
        Operand::GetElementPtr {
            ty,
            ptr_ty,
            indices,
            ..
        } => {
            assert_qir_smith_v1_type(ty);
            assert_qir_smith_v1_type(ptr_ty);
            for index in indices {
                assert_qir_smith_v1_operand(index);
            }
        }
        Operand::LocalRef(_)
        | Operand::TypedLocalRef(_, _)
        | Operand::FloatConst(_, _)
        | Operand::NullPtr
        | Operand::GlobalRef(_) => {}
    }
}

#[allow(clippy::too_many_lines)]
fn assert_qir_smith_v1_module(module: &Module) {
    for global in &module.globals {
        assert_qir_smith_v1_type(&global.ty);
    }

    for function in &module.functions {
        assert_qir_smith_v1_type(&function.return_type);
        for param in &function.params {
            assert_qir_smith_v1_type(&param.ty);
        }

        for block in &function.basic_blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Ret(Some(operand)) => assert_qir_smith_v1_operand(operand),
                    Instruction::Ret(None)
                    | Instruction::Jump { .. }
                    | Instruction::Unreachable => {}
                    Instruction::Br { cond_ty, cond, .. } => {
                        assert_qir_smith_v1_type(cond_ty);
                        assert_qir_smith_v1_operand(cond);
                    }
                    Instruction::BinOp { ty, lhs, rhs, .. }
                    | Instruction::ICmp { ty, lhs, rhs, .. }
                    | Instruction::FCmp { ty, lhs, rhs, .. } => {
                        assert_qir_smith_v1_type(ty);
                        assert_qir_smith_v1_operand(lhs);
                        assert_qir_smith_v1_operand(rhs);
                    }
                    Instruction::Cast {
                        from_ty,
                        to_ty,
                        value,
                        ..
                    } => {
                        assert_qir_smith_v1_type(from_ty);
                        assert_qir_smith_v1_type(to_ty);
                        assert_qir_smith_v1_operand(value);
                    }
                    Instruction::Call {
                        return_ty, args, ..
                    } => {
                        if let Some(return_ty) = return_ty {
                            assert_qir_smith_v1_type(return_ty);
                        }
                        for (arg_ty, operand) in args {
                            assert_qir_smith_v1_type(arg_ty);
                            assert_qir_smith_v1_operand(operand);
                        }
                    }
                    Instruction::Phi { .. } => {
                        panic!("phi instructions are outside the v1 qir_smith test boundary")
                    }
                    Instruction::Alloca { ty, .. } => assert_qir_smith_v1_type(ty),
                    Instruction::Load {
                        ty, ptr_ty, ptr, ..
                    } => {
                        assert_qir_smith_v1_type(ty);
                        assert_qir_smith_v1_type(ptr_ty);
                        assert_qir_smith_v1_operand(ptr);
                    }
                    Instruction::Store {
                        ty,
                        value,
                        ptr_ty,
                        ptr,
                    } => {
                        assert_qir_smith_v1_type(ty);
                        assert_qir_smith_v1_operand(value);
                        assert_qir_smith_v1_type(ptr_ty);
                        assert_qir_smith_v1_operand(ptr);
                    }
                    Instruction::Select {
                        cond,
                        true_val,
                        false_val,
                        ty,
                        ..
                    } => {
                        assert_qir_smith_v1_operand(cond);
                        assert_qir_smith_v1_operand(true_val);
                        assert_qir_smith_v1_operand(false_val);
                        assert_qir_smith_v1_type(ty);
                    }
                    Instruction::Switch { .. } => {
                        panic!("switch instructions are outside the v1 qir_smith test boundary")
                    }
                    Instruction::GetElementPtr {
                        pointee_ty,
                        ptr_ty,
                        ptr,
                        indices,
                        ..
                    } => {
                        assert_qir_smith_v1_type(pointee_ty);
                        assert_qir_smith_v1_type(ptr_ty);
                        assert_qir_smith_v1_operand(ptr);
                        for index in indices {
                            assert_qir_smith_v1_operand(index);
                        }
                    }
                }
            }
        }
    }
}

fn has_opaque_ptr_in_type(ty: &Type) -> bool {
    match ty {
        Type::Ptr => true,
        Type::Array(_, element) => has_opaque_ptr_in_type(element),
        Type::Function(result, params) => {
            has_opaque_ptr_in_type(result) || params.iter().any(has_opaque_ptr_in_type)
        }
        _ => false,
    }
}

fn has_opaque_ptr_in_operand(op: &Operand) -> bool {
    match op {
        Operand::IntConst(ty, _) | Operand::IntToPtr(_, ty) => has_opaque_ptr_in_type(ty),
        Operand::GetElementPtr {
            ty,
            ptr_ty,
            indices,
            ..
        } => {
            has_opaque_ptr_in_type(ty)
                || has_opaque_ptr_in_type(ptr_ty)
                || indices.iter().any(has_opaque_ptr_in_operand)
        }
        _ => false,
    }
}

fn instr_has_opaque_ptr(instr: &Instruction) -> bool {
    match instr {
        Instruction::Call {
            return_ty, args, ..
        } => {
            if let Some(rt) = return_ty
                && has_opaque_ptr_in_type(rt)
            {
                return true;
            }
            args.iter()
                .any(|(ty, op)| has_opaque_ptr_in_type(ty) || has_opaque_ptr_in_operand(op))
        }
        Instruction::Br { cond_ty, cond, .. } => {
            has_opaque_ptr_in_type(cond_ty) || has_opaque_ptr_in_operand(cond)
        }
        Instruction::BinOp { ty, lhs, rhs, .. }
        | Instruction::ICmp { ty, lhs, rhs, .. }
        | Instruction::FCmp { ty, lhs, rhs, .. } => {
            has_opaque_ptr_in_type(ty)
                || has_opaque_ptr_in_operand(lhs)
                || has_opaque_ptr_in_operand(rhs)
        }
        Instruction::Cast {
            from_ty,
            to_ty,
            value,
            ..
        } => {
            has_opaque_ptr_in_type(from_ty)
                || has_opaque_ptr_in_type(to_ty)
                || has_opaque_ptr_in_operand(value)
        }
        Instruction::Ret(Some(op)) => has_opaque_ptr_in_operand(op),
        Instruction::Alloca { ty, .. } => has_opaque_ptr_in_type(ty),
        Instruction::Load {
            ty, ptr_ty, ptr, ..
        } => {
            has_opaque_ptr_in_type(ty)
                || has_opaque_ptr_in_type(ptr_ty)
                || has_opaque_ptr_in_operand(ptr)
        }
        Instruction::Store {
            ty,
            value,
            ptr_ty,
            ptr,
        } => {
            has_opaque_ptr_in_type(ty)
                || has_opaque_ptr_in_operand(value)
                || has_opaque_ptr_in_type(ptr_ty)
                || has_opaque_ptr_in_operand(ptr)
        }
        Instruction::Select {
            cond,
            true_val,
            false_val,
            ty,
            ..
        } => {
            has_opaque_ptr_in_operand(cond)
                || has_opaque_ptr_in_operand(true_val)
                || has_opaque_ptr_in_operand(false_val)
                || has_opaque_ptr_in_type(ty)
        }
        Instruction::GetElementPtr {
            pointee_ty,
            ptr_ty,
            ptr,
            indices,
            ..
        } => {
            has_opaque_ptr_in_type(pointee_ty)
                || has_opaque_ptr_in_type(ptr_ty)
                || has_opaque_ptr_in_operand(ptr)
                || indices.iter().any(has_opaque_ptr_in_operand)
        }
        _ => false,
    }
}

/// Run profile validation on a generated module and verify it completes
/// without panicking. Does not assert that violations are empty because
/// ``BareRoundtrip`` intentionally lacks QIR metadata (MS-*/MF-* violations).
fn assert_qir_smith_profile_validation_does_not_panic(module: &Module) {
    // Validate — must not panic regardless of violation count.
    let _result = super::validate_qir_profile(module);
}

/// Run profile validation on a generated module and assert that all
/// errors are empty. Use this for QIR-profile-conformant modules
fn assert_qir_smith_profile_valid(module: &Module) {
    let result = super::validate_qir_profile(module);
    assert!(
        result.errors.is_empty(),
        "Generated module has profile errors: {:#?}",
        result.errors
    );
}

fn assert_qir_smith_typed_pointer_module(module: &Module) {
    assert!(
        module.struct_types.iter().any(|s| s.name == "Qubit"),
        "v1 module must define %Qubit struct type"
    );
    assert!(
        module.struct_types.iter().any(|s| s.name == "Result"),
        "v1 module must define %Result struct type"
    );

    for global in &module.globals {
        assert!(
            !has_opaque_ptr_in_type(&global.ty),
            "opaque Ptr in global {}: {:?}",
            global.name,
            global.ty
        );
    }

    for function in &module.functions {
        assert!(
            !has_opaque_ptr_in_type(&function.return_type),
            "opaque Ptr in return type of {}: {:?}",
            function.name,
            function.return_type
        );
        for (pi, param) in function.params.iter().enumerate() {
            assert!(
                !has_opaque_ptr_in_type(&param.ty),
                "opaque Ptr in param {pi} of {}: {:?}",
                function.name,
                param.ty
            );
        }

        for (bi, block) in function.basic_blocks.iter().enumerate() {
            for (ii, instruction) in block.instructions.iter().enumerate() {
                assert!(
                    !instr_has_opaque_ptr(instruction),
                    "opaque Ptr in function {} block {bi} instruction {ii}: {instruction:?}",
                    function.name,
                );
            }
        }
    }
}

fn assert_generated_qir_smith_roundtrips(config: &QirSmithConfig, seed: &[u8]) {
    let generated = generate_module_from_bytes(config, seed)
        .unwrap_or_else(|err| panic!("qir_smith generation failed: {err}"));
    assert_qir_smith_v1_module(&generated);

    if matches!(config.profile, QirProfilePreset::BareRoundtrip) {
        // BareRoundtrip intentionally lacks QIR metadata.
        assert_qir_smith_profile_validation_does_not_panic(&generated);
    } else {
        assert_qir_smith_profile_valid(&generated);
    }

    let ir = write_module_to_string(&generated);
    round_trip_text(&ir);

    let orig = parse_module(&ir).unwrap_or_else(|e| panic!("text parse failed: {e}"));
    let bc = write_bitcode_for_target(&orig, QirEmitTarget::QirV2Opaque);
    let report = parse_bitcode_compatibility_report(&bc).unwrap_or_else(|diagnostics| {
        panic!("bitcode compatibility parse failed: {diagnostics:?}")
    });
    if orig
        .globals
        .iter()
        .any(|global| global.initializer.is_some())
    {
        assert!(
            report.diagnostics.is_empty(),
            "unexpected compatibility diagnostics for qir_smith globals with initializers: {:?}",
            report.diagnostics
        );
    }
    let rt = report.module;
    assert_qir_smith_v1_module(&orig);
    assert_qir_smith_v1_module(&rt);
    assert_bitcode_roundtrip_structure(&orig, &rt);
}

#[test]
fn qir_smith_adaptive_v2_roundtrips_with_fixed_seed() {
    let config = qir_smith_v1_config(QirProfilePreset::AdaptiveV2);
    assert_generated_qir_smith_roundtrips(&config, b"qir-smith-opaque-adaptive-v1");
}

#[test]
fn qir_smith_bare_roundtrip_roundtrips_with_fixed_seed() {
    let config = qir_smith_v1_config(QirProfilePreset::BareRoundtrip);
    assert_generated_qir_smith_roundtrips(&config, b"qir-smith-bare-roundtrip-v1");
}

#[test]
fn qir_smith_base_v1_text_roundtrips_with_fixed_seed() {
    let config = qir_smith_v1_config(QirProfilePreset::BaseV1);
    let generated = generate_module_from_bytes(&config, b"base-v1-seed-000")
        .unwrap_or_else(|err| panic!("qir_smith BaseV1 generation failed: {err}"));
    assert_qir_smith_typed_pointer_module(&generated);
    assert_qir_smith_profile_valid(&generated);

    let ir = write_module_to_string(&generated);
    let parsed = parse_module(&ir).unwrap_or_else(|e| panic!("BaseV1 text parse failed: {e}"));
    assert_qir_smith_typed_pointer_module(&parsed);

    round_trip_text(&ir);
}

#[test]
fn qir_smith_adaptive_v1_text_roundtrips_with_fixed_seed() {
    let config = qir_smith_v1_config(QirProfilePreset::AdaptiveV1);
    let generated = generate_module_from_bytes(&config, b"qir-smith-adaptive-v1")
        .unwrap_or_else(|err| panic!("qir_smith AdaptiveV1 generation failed: {err}"));
    assert_qir_smith_typed_pointer_module(&generated);
    assert_qir_smith_profile_valid(&generated);

    let ir = write_module_to_string(&generated);
    let parsed = parse_module(&ir).unwrap_or_else(|e| panic!("AdaptiveV1 text parse failed: {e}"));
    assert_qir_smith_typed_pointer_module(&parsed);

    round_trip_text(&ir);

    let bc = write_bitcode_for_target(&parsed, QirEmitTarget::QirV1Typed);
    let report = parse_bitcode_compatibility_report(&bc).unwrap_or_else(|diagnostics| {
        panic!("bitcode compatibility parse failed: {diagnostics:?}")
    });
    assert!(
        report.diagnostics.is_empty(),
        "unexpected compatibility diagnostics for AdaptiveV1 globals with initializers: {:?}",
        report.diagnostics
    );
    let rt = report.module;
    // This compatibility-report roundtrip remains structural-only; the direct
    // text parse above covers typed-pointer fidelity.
    assert_bitcode_roundtrip_structure(&parsed, &rt);
}

#[test]
fn qir_v2_qubit_and_result_operands_remain_opaque_ptrs() {
    let ir = r#"%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_r\00"

define i64 @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)
declare void @__quantum__qis__h__body(ptr)
declare void @__quantum__qis__m__body(ptr, ptr) #1
declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3}
!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"#;

    let module = parse_module(ir).unwrap_or_else(|e| panic!("parse failed: {e}"));

    assert_eq!(
        module.struct_types,
        vec![
            StructType {
                name: "Result".to_string(),
                is_opaque: true,
            },
            StructType {
                name: "Qubit".to_string(),
                is_opaque: true,
            },
        ]
    );

    for name in [
        "__quantum__rt__initialize",
        "__quantum__qis__h__body",
        "__quantum__qis__m__body",
        "__quantum__rt__result_record_output",
    ] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing function {name}"));
        for (index, param) in function.params.iter().enumerate() {
            assert_eq!(
                param.ty,
                Type::Ptr,
                "{name} param {index} should remain ptr, got {:?}",
                param.ty
            );
        }
    }

    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "ENTRYPOINT__main")
        .expect("missing entry point");
    for instruction in &entry.basic_blocks[0].instructions {
        if let Instruction::Call { callee, args, .. } = instruction {
            for (index, (arg_ty, operand)) in args.iter().enumerate() {
                if let Operand::IntToPtr(_, cast_ty) = operand {
                    assert_eq!(
                        *arg_ty,
                        Type::Ptr,
                        "{callee} arg {index} should use ptr, got {arg_ty:?}"
                    );
                    assert_eq!(
                        *cast_ty,
                        Type::Ptr,
                        "{callee} arg {index} inttoptr should target ptr, got {cast_ty:?}"
                    );
                }
            }
        }
    }

    let round_tripped = parse_module(&write_module_to_string(&module))
        .unwrap_or_else(|e| panic!("roundtrip parse failed: {e}"));
    assert_eq!(module, round_tripped);
}

#[allow(clippy::too_many_lines)]
#[test]
fn adaptive_float_computations_roundtrip_preserves_half_float_and_double() {
    let module = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "use_half".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Half,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "use_float".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Float,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "use_double".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Double,
                    name: None,
                }],
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
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Fadd,
                            ty: Type::Half,
                            lhs: Operand::float_const(Type::Half, 1.5),
                            rhs: Operand::float_const(Type::Half, 2.25),
                            result: "half_sum".to_string(),
                        },
                        Instruction::Cast {
                            op: CastKind::FpExt,
                            from_ty: Type::Half,
                            to_ty: Type::Float,
                            value: Operand::LocalRef("half_sum".to_string()),
                            result: "as_float".to_string(),
                        },
                        Instruction::BinOp {
                            op: BinOpKind::Fadd,
                            ty: Type::Float,
                            lhs: Operand::LocalRef("as_float".to_string()),
                            rhs: Operand::float_const(Type::Float, 0.5),
                            result: "float_sum".to_string(),
                        },
                        Instruction::Cast {
                            op: CastKind::FpExt,
                            from_ty: Type::Float,
                            to_ty: Type::Double,
                            value: Operand::LocalRef("float_sum".to_string()),
                            result: "as_double".to_string(),
                        },
                        Instruction::Cast {
                            op: CastKind::FpTrunc,
                            from_ty: Type::Double,
                            to_ty: Type::Half,
                            value: Operand::LocalRef("as_double".to_string()),
                            result: "back_to_half".to_string(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "use_half".to_string(),
                            args: vec![(Type::Half, Operand::LocalRef("back_to_half".to_string()))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "use_float".to_string(),
                            args: vec![(Type::Float, Operand::LocalRef("float_sum".to_string()))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "use_double".to_string(),
                            args: vec![(Type::Double, Operand::LocalRef("as_double".to_string()))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
                    ],
                }],
            },
        ],
        attribute_groups: vec![AttributeGroup {
            id: 0,
            attributes: vec![
                Attribute::StringAttr("entry_point".to_string()),
                Attribute::StringAttr("output_labeling_schema".to_string()),
                Attribute::KeyValue("qir_profiles".to_string(), "adaptive_profile".to_string()),
                Attribute::KeyValue("required_num_qubits".to_string(), "0".to_string()),
                Attribute::KeyValue("required_num_results".to_string(), "0".to_string()),
            ],
        }],
        named_metadata: vec![NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![0, 1, 2, 3, 4],
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
                    MetadataValue::Int(Type::Integer(32), 0),
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
                    MetadataValue::String("float_computations".to_string()),
                    MetadataValue::SubList(vec![
                        MetadataValue::String("half".to_string()),
                        MetadataValue::String("float".to_string()),
                        MetadataValue::String("double".to_string()),
                    ]),
                ],
            },
        ],
    };

    let text = write_module_to_string(&module);
    let parsed = parse_module(&text).unwrap_or_else(|e| panic!("parse failed: {e}"));

    assert_eq!(
        parsed.get_flag("float_computations"),
        Some(&MetadataValue::SubList(vec![
            MetadataValue::String("half".to_string()),
            MetadataValue::String("float".to_string()),
            MetadataValue::String("double".to_string()),
        ]))
    );

    for (name, expected_ty) in [
        ("use_half", Type::Half),
        ("use_float", Type::Float),
        ("use_double", Type::Double),
    ] {
        let function = parsed
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing function {name}"));
        assert_eq!(
            function.params,
            vec![Param {
                ty: expected_ty,
                name: None,
            }]
        );
    }

    let entry = parsed
        .functions
        .iter()
        .find(|function| function.name == "ENTRYPOINT__main")
        .expect("missing entry point");
    let instructions = &entry.basic_blocks[0].instructions;
    assert!(matches!(
        &instructions[0],
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Half,
            lhs: Operand::FloatConst(Type::Half, _),
            rhs: Operand::FloatConst(Type::Half, _),
            result,
        } if result == "half_sum"
    ));
    assert!(matches!(
        &instructions[1],
        Instruction::Cast {
            op: CastKind::FpExt,
            from_ty: Type::Half,
            to_ty: Type::Float,
            value: Operand::TypedLocalRef(name, Type::Half),
            result,
        } if name == "half_sum" && result == "as_float"
    ));
    assert!(matches!(
        &instructions[2],
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Float,
            lhs: Operand::TypedLocalRef(name, Type::Float),
            rhs: Operand::FloatConst(Type::Float, _),
            result,
        } if name == "as_float" && result == "float_sum"
    ));
    assert!(matches!(
        &instructions[3],
        Instruction::Cast {
            op: CastKind::FpExt,
            from_ty: Type::Float,
            to_ty: Type::Double,
            value: Operand::TypedLocalRef(name, Type::Float),
            result,
        } if name == "float_sum" && result == "as_double"
    ));
    assert!(matches!(
        &instructions[4],
        Instruction::Cast {
            op: CastKind::FpTrunc,
            from_ty: Type::Double,
            to_ty: Type::Half,
            value: Operand::TypedLocalRef(name, Type::Double),
            result,
        } if name == "as_double" && result == "back_to_half"
    ));

    let (from_text, from_bc) = bitcode_roundtrip(&text);
    let from_text_entry = from_text
        .functions
        .iter()
        .find(|function| function.name == "ENTRYPOINT__main")
        .expect("missing text entry point");
    let from_bc_entry = from_bc
        .functions
        .iter()
        .find(|function| function.name == "ENTRYPOINT__main")
        .expect("missing bitcode entry point");

    assert_eq!(
        from_text.get_flag("float_computations"),
        from_bc.get_flag("float_computations")
    );
    assert_eq!(from_text.functions.len(), from_bc.functions.len());
    assert_eq!(
        from_bc.functions[0]
            .params
            .iter()
            .map(|param| param.ty.clone())
            .collect::<Vec<_>>(),
        vec![Type::Half]
    );
    assert_eq!(
        from_bc.functions[1]
            .params
            .iter()
            .map(|param| param.ty.clone())
            .collect::<Vec<_>>(),
        vec![Type::Float]
    );
    assert_eq!(
        from_bc.functions[2]
            .params
            .iter()
            .map(|param| param.ty.clone())
            .collect::<Vec<_>>(),
        vec![Type::Double]
    );

    let from_bc_instructions = &from_bc_entry.basic_blocks[0].instructions;
    assert_eq!(
        from_text_entry.basic_blocks[0].instructions.len(),
        from_bc_instructions.len()
    );
    assert!(matches!(
        &from_bc_instructions[0],
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Half,
            lhs: Operand::FloatConst(Type::Half, _),
            rhs: Operand::FloatConst(Type::Half, _),
            ..
        }
    ));
    assert!(matches!(
        &from_bc_instructions[1],
        Instruction::Cast {
            op: CastKind::FpExt,
            from_ty: Type::Half,
            to_ty: Type::Float,
            value: Operand::TypedLocalRef(_, Type::Half),
            ..
        }
    ));
    assert!(matches!(
        &from_bc_instructions[2],
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Float,
            lhs: Operand::TypedLocalRef(_, Type::Float),
            rhs: Operand::FloatConst(Type::Float, _),
            ..
        }
    ));
    assert!(matches!(
        &from_bc_instructions[3],
        Instruction::Cast {
            op: CastKind::FpExt,
            from_ty: Type::Float,
            to_ty: Type::Double,
            value: Operand::TypedLocalRef(_, Type::Float),
            ..
        }
    ));
    assert!(matches!(
        &from_bc_instructions[4],
        Instruction::Cast {
            op: CastKind::FpTrunc,
            from_ty: Type::Double,
            to_ty: Type::Half,
            value: Operand::TypedLocalRef(_, Type::Double),
            ..
        }
    ));
    assert!(matches!(
        &from_bc_instructions[5],
        Instruction::Call {
            return_ty: None,
            args,
            ..
        } if args.len() == 1
            && args[0].0 == Type::Half
            && matches!(&args[0].1, Operand::TypedLocalRef(_, Type::Half))
    ));
    assert!(matches!(
        &from_bc_instructions[6],
        Instruction::Call {
            return_ty: None,
            args,
            ..
        } if args.len() == 1
            && args[0].0 == Type::Float
            && matches!(&args[0].1, Operand::TypedLocalRef(_, Type::Float))
    ));
    assert!(matches!(
        &from_bc_instructions[7],
        Instruction::Call {
            return_ty: None,
            args,
            ..
        } if args.len() == 1
            && args[0].0 == Type::Double
            && matches!(&args[0].1, Operand::TypedLocalRef(_, Type::Double))
    ));
}

#[test]
fn bitcode_roundtrip_preserves_nested_metadata_sublists_and_node_refs() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let complex_flag = MetadataValue::SubList(vec![
        MetadataValue::NodeRef(0),
        MetadataValue::SubList(vec![
            MetadataValue::String("half".to_string()),
            MetadataValue::NodeRef(0),
        ]),
        MetadataValue::String("leaf".to_string()),
    ]);

    let module = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
        attribute_groups: Vec::new(),
        named_metadata: vec![NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![0, 1],
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
                    MetadataValue::Int(Type::Integer(32), 5),
                    MetadataValue::String("complex_flag".to_string()),
                    complex_flag.clone(),
                ],
            },
        ],
    };

    let bitcode = write_bitcode(&module);
    let parsed = parse_bitcode(&bitcode).expect("nested metadata roundtrip should parse");

    assert_eq!(parsed.metadata_nodes, module.metadata_nodes);
    assert_eq!(parsed.get_flag("complex_flag"), Some(&complex_flag));
}

#[test]
fn bitcode_roundtrip_binop_sub() {
    let ir = "\
define i64 @test(i64 %a, i64 %b) {
entry:
  %r = sub i64 %a, %b
  ret i64 %r
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::BinOp {
            op: BinOpKind::Sub,
            ..
        }
    ));
}

#[test]
fn bitcode_roundtrip_binop_fadd() {
    let ir = "\
declare void @use_double(double)

define void @test(double %a, double %b) {
entry:
  %r = fadd double %a, %b
  call void @use_double(double %r)
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_fn = rt
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    assert_eq!(rt_fn.params[0].name.as_deref(), Some("a"));
    assert_eq!(rt_fn.params[1].name.as_deref(), Some("b"));

    let rt_instrs = &rt_fn.basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty,
            lhs,
            rhs,
            result,
        } if ty == &Type::Double
            && result == "r"
            && matches!(lhs, Operand::TypedLocalRef(name, lhs_ty) if name == "a" && lhs_ty == &Type::Double)
            && matches!(rhs, Operand::TypedLocalRef(name, rhs_ty) if name == "b" && rhs_ty == &Type::Double)
    ));
    assert!(matches!(
        &rt_instrs[1],
        Instruction::Call {
            return_ty: None,
            args,
            result: None,
            ..
        } if matches!(
            args.as_slice(),
            [(Type::Double, Operand::TypedLocalRef(name, ty))]
                if name == "r" && ty == &Type::Double
        )
    ));
}

#[test]
fn bitcode_roundtrip_icmp_slt() {
    let ir = "\
define i64 @test(i64 %a, i64 %b) {
entry:
  %c = icmp slt i64 %a, %b
  %r = select i1 %c, i64 %a, i64 %b
  ret i64 %r
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::ICmp {
            pred: IntPredicate::Slt,
            ..
        }
    ));
}

#[test]
fn bitcode_roundtrip_fcmp_oeq() {
    let ir = "\
declare void @use_i1(i1)

define void @test(double %a, double %b) {
entry:
  %c = fcmp oeq double %a, %b
  call void @use_i1(i1 %c)
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[1].basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::FCmp {
            pred: FloatPredicate::Oeq,
            ..
        }
    ));
}

#[test]
fn bitcode_roundtrip_call() {
    let ir = "\
declare void @callee(i64)

define void @test(i64 %a) {
entry:
  call void @callee(i64 %a)
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let test_fn = rt
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    let rt_instrs = &test_fn.basic_blocks[0].instructions;
    assert!(
        matches!(&rt_instrs[0], Instruction::Call { callee, .. } if callee == "callee"),
        "expected Call instruction targeting @callee, got {:?}",
        rt_instrs[0]
    );
}

#[test]
fn bitcode_roundtrip_global_ref_operand() {
    let ir = "\
@message = internal constant [5 x i8] c\"hello\"\n\
\n\
declare void @use_ptr(ptr)\n\
\n\
define void @test() {\n\
entry:\n\
  call void @use_ptr(ptr @message)\n\
  ret void\n\
}\n";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let test_fn = rt
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[0],
        Instruction::Call { args, .. }
            if matches!(args.first(), Some((Type::Ptr, Operand::GlobalRef(name))) if name == "message")
    ));
}

#[test]
fn bitcode_roundtrip_operand_gep_base_name() {
    let ir = "\
@str = internal constant [5 x i8] c\"hello\"\n\
\n\
declare void @use_ptr(ptr)\n\
\n\
define void @test() {\n\
entry:\n\
  call void @use_ptr(ptr getelementptr inbounds ([5 x i8], ptr @str, i64 0, i64 0))\n\
  ret void\n\
}\n";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let test_fn = rt
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[0],
        Instruction::Call { args, .. }
            if matches!(
                args.first(),
                Some((
                    Type::Ptr,
                    Operand::GetElementPtr { ptr, .. },
                )) if ptr == "str"
            )
    ));
}

#[test]
fn bitcode_roundtrip_ret_value() {
    let ir = "\
define i64 @test() {
entry:
  ret i64 42
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(matches!(&rt_instrs[0], Instruction::Ret(Some(_))));
}

#[test]
fn bitcode_roundtrip_ret_void() {
    let ir = "\
define void @test() {
entry:
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(matches!(&rt_instrs[0], Instruction::Ret(None)));
}

#[test]
fn bitcode_roundtrip_br_conditional() {
    let ir = "\
define void @test(i64 %a, i64 %b) {
entry:
  %cond = icmp slt i64 %a, %b
  br i1 %cond, label %then, label %else
then:
  ret void
else:
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_fn = &rt.functions[0];
    assert_eq!(rt_fn.params[0].name.as_deref(), Some("a"));
    assert_eq!(rt_fn.params[1].name.as_deref(), Some("b"));
    assert_eq!(
        rt_fn
            .basic_blocks
            .iter()
            .map(|bb| bb.name.as_str())
            .collect::<Vec<_>>(),
        vec!["entry", "then", "else"]
    );

    let rt_instrs = &rt_fn.basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::ICmp { lhs, rhs, result, .. }
            if result == "cond"
                && matches!(lhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "a")
                && matches!(rhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "b")
    ));
    assert!(matches!(
        &rt_instrs[1],
        Instruction::Br {
            cond,
            true_dest,
            false_dest,
            ..
        } if true_dest == "then"
            && false_dest == "else"
            && matches!(cond, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "cond")
    ));
}

#[test]
fn bitcode_roundtrip_jump() {
    let ir = "\
define void @test() {
entry:
  br label %exit
exit:
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_fn = &rt.functions[0];
    assert_eq!(
        rt_fn
            .basic_blocks
            .iter()
            .map(|bb| bb.name.as_str())
            .collect::<Vec<_>>(),
        vec!["entry", "exit"]
    );

    let rt_instrs = &rt_fn.basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::Jump { dest } if dest == "exit"
    ));
}

#[test]
fn bitcode_roundtrip_phi() {
    let ir = "\
define i64 @test() {
block_0:
  br label %loop
loop:
  %i = phi i64 [ 0, %block_0 ], [ %next, %loop ]
  %next = add i64 %i, 1
  %cond = icmp slt i64 %next, 10
  br i1 %cond, label %loop, label %exit
exit:
  ret i64 %i
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_fn = &rt.functions[0];
    assert_eq!(
        rt_fn
            .basic_blocks
            .iter()
            .map(|bb| bb.name.as_str())
            .collect::<Vec<_>>(),
        vec!["block_0", "loop", "exit"]
    );

    let rt_instrs = &rt_fn.basic_blocks[1].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::Phi {
            ty,
            incoming,
            result,
        } if ty == &Type::Integer(64)
            && result == "i"
            && incoming.len() == 2
            && matches!(&incoming[0], (Operand::IntConst(phi_ty, 0), from) if phi_ty == &Type::Integer(64) && from == "block_0")
            && matches!(&incoming[1], (Operand::LocalRef(name) | Operand::TypedLocalRef(name, _), from) if name == "next" && from == "loop")
    ));
    assert!(matches!(
        &rt_instrs[1],
        Instruction::BinOp { lhs, rhs, result, .. }
            if result == "next"
                && matches!(lhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "i")
                && matches!(rhs, Operand::IntConst(ty, 1) if ty == &Type::Integer(64))
    ));
    assert!(matches!(
        &rt_instrs[2],
        Instruction::ICmp { lhs, rhs, result, .. }
            if result == "cond"
                && matches!(lhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "next")
                && matches!(rhs, Operand::IntConst(ty, 10) if ty == &Type::Integer(64))
    ));
    assert!(matches!(
        &rt_instrs[3],
        Instruction::Br {
            cond,
            true_dest,
            false_dest,
            ..
        } if true_dest == "loop"
            && false_dest == "exit"
            && matches!(cond, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "cond")
    ));
    assert!(matches!(
        &rt_fn.basic_blocks[2].instructions[0],
        Instruction::Ret(Some(Operand::LocalRef(name) | Operand::TypedLocalRef(name, _))) if name == "i"
    ));
}

#[test]
fn bitcode_roundtrip_alloca() {
    let ir = "\
define void @test() {
entry:
  %ptr = alloca i64
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(
        matches!(&rt_instrs[0], Instruction::Alloca { .. }),
        "expected Alloca instruction, got {:?}",
        rt_instrs[0]
    );
}

#[test]
fn bitcode_roundtrip_load() {
    let ir = "\
define i64 @test() {
entry:
  %ptr = alloca i64
  %val = load i64, ptr %ptr
  ret i64 %val
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::Alloca { ty, result } if ty == &Type::Integer(64) && result == "ptr"
    ));
    assert!(matches!(
        &rt_instrs[1],
        Instruction::Load {
            ty,
            ptr_ty,
            ptr,
            result,
        } if ty == &Type::Integer(64)
            && ptr_ty == &Type::Ptr
            && result == "val"
            && matches!(ptr, Operand::TypedLocalRef(name, local_ty) if name == "ptr" && local_ty == &Type::Ptr)
    ));
    assert!(matches!(
        &rt_instrs[2],
        Instruction::Ret(Some(Operand::TypedLocalRef(name, ty)))
            if name == "val" && ty == &Type::Integer(64)
    ));
}

#[test]
fn bitcode_roundtrip_store() {
    let ir = "\
define void @test() {
entry:
  %ptr = alloca i64
  store i64 42, ptr %ptr
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(
        matches!(&rt_instrs[1], Instruction::Store { .. }),
        "expected Store instruction, got {:?}",
        rt_instrs[1]
    );
}

#[test]
fn bitcode_roundtrip_select() {
    let ir = "\
define i64 @test(i64 %a, i64 %b) {
entry:
  %cond = icmp slt i64 %a, %b
  %r = select i1 %cond, i64 %a, i64 %b
  ret i64 %r
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(
        matches!(&rt_instrs[1], Instruction::Select { .. }),
        "expected Select instruction, got {:?}",
        rt_instrs[1]
    );
}

#[test]
fn bitcode_roundtrip_switch() {
    let ir = "\
define void @test(i32 %val) {
entry:
  switch i32 %val, label %default [
    i32 0, label %case0
    i32 1, label %case1
  ]
case0:
  ret void
case1:
  ret void
default:
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_fn = &rt.functions[0];
    assert_eq!(rt_fn.params[0].name.as_deref(), Some("val"));
    assert_eq!(
        rt_fn
            .basic_blocks
            .iter()
            .map(|bb| bb.name.as_str())
            .collect::<Vec<_>>(),
        vec!["entry", "case0", "case1", "default"]
    );

    let rt_instrs = &rt_fn.basic_blocks[0].instructions;
    assert!(matches!(
        &rt_instrs[0],
        Instruction::Switch {
            ty,
            value,
            default_dest,
            cases,
        } if ty == &Type::Integer(32)
            && default_dest == "default"
            && cases == &vec![(0, "case0".to_string()), (1, "case1".to_string())]
            && matches!(value, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "val")
    ));
}

#[test]
fn bitcode_roundtrip_gep() {
    let ir = "\
@str = internal constant [5 x i8] c\"hello\"

declare void @use_ptr(ptr)

define void @test() {
entry:
  %0 = getelementptr inbounds [5 x i8], ptr @str, i64 0, i64 0
  call void @use_ptr(ptr %0)
  ret void
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    // The test function is after the declare
    let test_fn = rt
        .functions
        .iter()
        .find(|f| f.name == "test")
        .expect("missing test function");
    let rt_instrs = &test_fn.basic_blocks[0].instructions;
    assert!(
        matches!(
            &rt_instrs[0],
            Instruction::GetElementPtr { inbounds: true, .. }
        ),
        "expected GetElementPtr instruction, got {:?}",
        rt_instrs[0]
    );
}

#[test]
fn bitcode_roundtrip_unreachable() {
    let ir = "\
define void @test() {
entry:
  unreachable
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(
        matches!(&rt_instrs[0], Instruction::Unreachable),
        "expected Unreachable instruction, got {:?}",
        rt_instrs[0]
    );
}

#[test]
fn bitcode_roundtrip_cast_zext() {
    let ir = "\
define i64 @test(i32 %a) {
entry:
  %r = zext i32 %a to i64
  ret i64 %r
}
";
    let (orig, rt) = bitcode_roundtrip(ir);
    assert_bitcode_roundtrip_structure(&orig, &rt);
    let rt_instrs = &rt.functions[0].basic_blocks[0].instructions;
    assert!(
        matches!(
            &rt_instrs[0],
            Instruction::Cast {
                op: CastKind::Zext,
                ..
            }
        ),
        "expected Cast/Zext instruction, got {:?}",
        rt_instrs[0]
    );
}

// --- Phase 3: Cross-format roundtrip tests ---

#[test]
fn bitcode_roundtrip_global_type_preserved() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let ir = "\
@str = internal constant [5 x i8] c\"hello\"

declare void @use_ptr(ptr)

define void @test() {
entry:
  %0 = getelementptr inbounds [5 x i8], ptr @str, i64 0, i64 0
  call void @use_ptr(ptr %0)
  ret void
}
";
    let module = parse_module(ir).expect("text parse");
    assert_eq!(
        module.globals[0].ty,
        Type::Array(5, Box::new(Type::Integer(8)))
    );

    let bc = write_bitcode(&module);
    let rt = parse_bitcode(&bc).expect("bitcode parse");
    assert_eq!(
        rt.globals[0].ty,
        Type::Array(5, Box::new(Type::Integer(8))),
        "global type should be preserved through bitcode roundtrip"
    );
    assert!(rt.globals[0].is_constant);
}

#[test]
fn cross_format_text_to_bitcode_to_text_call() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let ir = "\
declare void @callee(i64)

define void @test(i64 %a) {
entry:
  call void @callee(i64 %a)
  ret void
}
";
    let m1 = parse_module(ir).expect("text parse");
    let bc = write_bitcode(&m1);
    let m2 = parse_bitcode(&bc).expect("bitcode parse");
    let text2 = write_module_to_string(&m2);
    let m3 = parse_module(&text2).expect("re-text parse");

    // Structural comparison
    assert_eq!(m2.functions.len(), m3.functions.len());
    for (f2, f3) in m2.functions.iter().zip(m3.functions.iter()) {
        assert_eq!(f2.name, f3.name);
        assert_eq!(f2.is_declaration, f3.is_declaration);
        assert_eq!(f2.basic_blocks.len(), f3.basic_blocks.len());
    }

    let bitcode_test_fn = m2
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing bitcode test function");
    assert!(matches!(
        &bitcode_test_fn.basic_blocks[0].instructions[0],
        Instruction::Call { callee, .. } if callee == "callee"
    ));

    let reparsed_test_fn = m3
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing reparsed test function");
    assert!(matches!(
        &reparsed_test_fn.basic_blocks[0].instructions[0],
        Instruction::Call { callee, .. } if callee == "callee"
    ));
}

#[test]
fn cross_format_text_to_bitcode_to_text_branch() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let ir = "\
define void @test(i64 %a, i64 %b) {
entry:
  %cond = icmp slt i64 %a, %b
  br i1 %cond, label %then, label %else
then:
  ret void
else:
  ret void
}
";
    let m1 = parse_module(ir).expect("text parse");
    let bc = write_bitcode(&m1);
    let m2 = parse_bitcode(&bc).expect("bitcode parse");
    let text2 = write_module_to_string(&m2);
    let m3 = parse_module(&text2).expect("re-text parse");

    assert_eq!(m2.functions.len(), m3.functions.len());
    let assert_branch_function = |function: &Function| {
        assert_eq!(function.params[0].name.as_deref(), Some("a"));
        assert_eq!(function.params[1].name.as_deref(), Some("b"));
        assert_eq!(
            function
                .basic_blocks
                .iter()
                .map(|bb| bb.name.as_str())
                .collect::<Vec<_>>(),
            vec!["entry", "then", "else"]
        );
        assert!(matches!(
            &function.basic_blocks[0].instructions[0],
            Instruction::ICmp { lhs, rhs, result, .. }
                if result == "cond"
                    && matches!(lhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "a")
                    && matches!(rhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "b")
        ));
        assert!(matches!(
            &function.basic_blocks[0].instructions[1],
            Instruction::Br {
                cond,
                true_dest,
                false_dest,
                ..
            } if true_dest == "then"
                && false_dest == "else"
                && matches!(cond, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "cond")
        ));
    };

    let bitcode_test_fn = m2
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing bitcode test function");
    assert_branch_function(bitcode_test_fn);

    let reparsed_test_fn = m3
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing reparsed test function");
    assert_branch_function(reparsed_test_fn);
}

#[test]
fn cross_format_text_to_bitcode_to_text_binop() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let ir = "\
define i64 @test(i64 %a, i64 %b) {
entry:
  %r = add i64 %a, %b
  ret i64 %r
}
";
    let m1 = parse_module(ir).expect("text parse");
    let bc = write_bitcode(&m1);
    let m2 = parse_bitcode(&bc).expect("bitcode parse");
    let text2 = write_module_to_string(&m2);
    let m3 = parse_module(&text2).expect("re-text parse");

    assert_eq!(m2.functions.len(), m3.functions.len());
    for (f2, f3) in m2.functions.iter().zip(m3.functions.iter()) {
        assert_eq!(f2.name, f3.name);
        assert_eq!(f2.basic_blocks.len(), f3.basic_blocks.len());
        for (b2, b3) in f2.basic_blocks.iter().zip(f3.basic_blocks.iter()) {
            assert_eq!(b2.instructions.len(), b3.instructions.len());
        }
    }
}

#[test]
fn cross_format_bitcode_to_text_to_bitcode() {
    use super::bitcode::reader::parse_bitcode;
    use super::bitcode::writer::write_bitcode;

    let ir = "\
define i64 @test(i64 %a, i64 %b) {
entry:
  %sum = add i64 %a, %b
  %cond = icmp slt i64 %sum, 100
  br i1 %cond, label %then, label %else
then:
  ret i64 %sum
else:
  ret i64 0
}
";
    // text -> module -> bitcode -> module -> text -> module -> bitcode
    let m1 = parse_module(ir).expect("text parse");
    let bc1 = write_bitcode(&m1);
    let m2 = parse_bitcode(&bc1).expect("bitcode parse 1");
    let text2 = write_module_to_string(&m2);
    let m3 = parse_module(&text2).expect("re-text parse");
    let bc2 = write_bitcode(&m3);
    let m4 = parse_bitcode(&bc2).expect("bitcode parse 2");

    // m2 and m4 should be structurally equivalent (both went through bitcode)
    assert_eq!(m2.functions.len(), m4.functions.len());
    for (f2, f4) in m2.functions.iter().zip(m4.functions.iter()) {
        assert_eq!(f2.name, f4.name);
        assert_eq!(f2.is_declaration, f4.is_declaration);
        assert_eq!(f2.basic_blocks.len(), f4.basic_blocks.len());
        for (b2, b4) in f2.basic_blocks.iter().zip(f4.basic_blocks.iter()) {
            assert_eq!(b2.instructions.len(), b4.instructions.len());
        }
    }
}
