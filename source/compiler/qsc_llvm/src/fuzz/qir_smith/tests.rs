// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{PointerProbe, assemble_text_ir, available_fast_matrix_lanes};

fn deterministic_seed_bytes() -> Vec<u8> {
    (0_u8..=127).collect()
}

fn opaque_adaptive_config() -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: 4,
        max_instrs_per_block: 6,
        ..QirSmithConfig::default()
    }
}

fn bare_roundtrip_config() -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: 4,
        max_instrs_per_block: 6,
        ..QirSmithConfig::for_profile(QirProfilePreset::BareRoundtrip)
    }
}

fn checked_effective_config(config: &QirSmithConfig, roundtrip: RoundTripKind) -> EffectiveConfig {
    QirSmithConfig {
        output_mode: OutputMode::RoundTripChecked,
        roundtrip: Some(roundtrip),
        ..config.clone()
    }
    .sanitize()
}

fn adaptive_shell_state() -> QirGenState {
    let seed_bytes = deterministic_seed_bytes();
    let mut unstructured = Unstructured::new(&seed_bytes);
    QirGenState::new(
        ShellPreset::from_profile(QirProfilePreset::AdaptiveV1),
        ShellCounts::default(),
        QirProfilePreset::AdaptiveV1,
        &mut unstructured,
    )
}

fn generated_entry_point(module: &Module) -> &Function {
    module
        .functions
        .first()
        .expect("generated module should include an entry point")
}

fn metadata_string_list(module: &Module, key: &str) -> Option<Vec<String>> {
    match module.get_flag(key) {
        Some(MetadataValue::SubList(items)) => Some(
            items
                .iter()
                .filter_map(|value| match value {
                    MetadataValue::String(text) => Some(text.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn adaptive_v1_module_with_float_metadata_shell(
    globals: Vec<GlobalVariable>,
    declarations: Vec<Function>,
    instructions: Vec<Instruction>,
) -> Module {
    let seed_bytes = [0_u8; 1];
    let mut unstructured = Unstructured::new(&seed_bytes);
    let (named_metadata, metadata_nodes) =
        build_qdk_metadata(qir::QirProfile::AdaptiveV1, &mut unstructured);

    let mut functions = declarations;
    functions.push(Function {
        name: qir::ENTRYPOINT_NAME.to_string(),
        return_type: Type::Integer(64),
        params: Vec::new(),
        is_declaration: false,
        attribute_group_refs: Vec::new(),
        basic_blocks: vec![BasicBlock {
            name: "entry".to_string(),
            instructions,
        }],
    });

    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals,
        functions,
        attribute_groups: Vec::new(),
        named_metadata,
        metadata_nodes,
    }
}

fn double_record_output_declaration() -> Function {
    Function {
        name: qir::rt::DOUBLE_RECORD_OUTPUT.to_string(),
        return_type: Type::Void,
        params: vec![
            Param {
                ty: Type::Double,
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
    }
}

#[test]
fn sanitize_clamps_counts_and_preserves_opt_in_expansion_flags_outside_checked_mode() {
    let effective = QirSmithConfig {
        output_mode: OutputMode::Text,
        roundtrip: Some(RoundTripKind::BitcodeOnly),
        max_funcs: 0,
        max_blocks_per_func: 0,
        max_instrs_per_block: 0,
        allow_phi: true,
        allow_switch: true,
        allow_memory_ops: true,
        allow_typed_pointers: true,
        ..QirSmithConfig::default()
    }
    .sanitize();

    assert_eq!(effective.profile, QirProfilePreset::AdaptiveV2);
    assert_eq!(effective.output_mode, OutputMode::Text);
    assert_eq!(effective.roundtrip, None);
    assert_eq!(effective.max_funcs, DEFAULT_MAX_FUNCS);
    assert_eq!(effective.max_blocks_per_func, DEFAULT_MAX_BLOCKS_PER_FUNC);
    assert_eq!(effective.max_instrs_per_block, DEFAULT_MAX_INSTRS_PER_BLOCK);
    assert!(effective.allow_phi);
    assert!(effective.allow_switch);
    assert!(effective.allow_memory_ops);
    assert!(!effective.allow_typed_pointers);
    assert!(!effective.bare_roundtrip_mode);
}

#[test]
fn sanitize_clears_opt_in_expansion_flags_in_checked_mode() {
    let effective = QirSmithConfig {
        output_mode: OutputMode::RoundTripChecked,
        allow_phi: true,
        allow_switch: true,
        allow_memory_ops: true,
        ..bare_roundtrip_config()
    }
    .sanitize();

    assert_eq!(
        effective.roundtrip,
        Some(RoundTripKind::TextAndBitcodeSinglePass)
    );
    assert!(!effective.allow_phi);
    assert!(!effective.allow_switch);
    assert!(!effective.allow_memory_ops);
}

#[test]
fn sanitize_promotes_bare_roundtrip_profile_and_defaults_checked_roundtrip() {
    let effective = QirSmithConfig {
        profile: QirProfilePreset::AdaptiveV2,
        output_mode: OutputMode::RoundTripChecked,
        roundtrip: None,
        max_blocks_per_func: 2,
        max_instrs_per_block: 3,
        bare_roundtrip_mode: true,
        ..QirSmithConfig::default()
    }
    .sanitize();

    assert_eq!(effective.profile, QirProfilePreset::BareRoundtrip);
    assert_eq!(effective.output_mode, OutputMode::RoundTripChecked);
    assert_eq!(
        effective.roundtrip,
        Some(RoundTripKind::TextAndBitcodeSinglePass)
    );
    assert_eq!(effective.max_blocks_per_func, 2);
    assert_eq!(effective.max_instrs_per_block, 3);
    assert!(effective.bare_roundtrip_mode);
}

#[test]
fn stable_name_allocator_uses_monotonic_indices() {
    let mut allocator = StableNameAllocator::default();

    assert_eq!(allocator.next_global_name(), "0");
    assert_eq!(allocator.next_global_name(), "1");
    assert_eq!(allocator.next_block_name(), "block_0");
    assert_eq!(allocator.next_block_name(), "block_1");
    assert_eq!(allocator.next_local_name(), "var_0");
    assert_eq!(allocator.next_local_name(), "var_1");
}

#[test]
fn intern_string_global_reuses_existing_names() {
    let seed_bytes = deterministic_seed_bytes();
    let mut unstructured = Unstructured::new(&seed_bytes);
    let mut state = QirGenState::new(
        ShellPreset::from_profile(QirProfilePreset::BareRoundtrip),
        ShellCounts::default(),
        QirProfilePreset::BareRoundtrip,
        &mut unstructured,
    );

    let first = state.intern_string_global("alpha".to_string());
    let second = state.intern_string_global("alpha".to_string());
    let third = state.intern_string_global("beta".to_string());

    assert_eq!(first, second);
    assert_ne!(first, third);
    assert_eq!(state.module.globals.len(), 2);
    assert_eq!(state.module.globals[0].name, "0");
    assert_eq!(state.module.globals[1].name, "1");
    assert_eq!(
        state.module.globals[0].initializer,
        Some(Constant::CString("alpha".to_string()))
    );
    assert_eq!(
        state.module.globals[1].initializer,
        Some(Constant::CString("beta".to_string()))
    );
}

#[test]
fn generate_from_bytes_is_deterministic_for_same_seed_and_config() {
    let seed_bytes = deterministic_seed_bytes();
    let config = opaque_adaptive_config();

    let first = generate_from_bytes(&config, &seed_bytes)
        .expect("generation should succeed for deterministic seed bytes");
    let second = generate_from_bytes(&config, &seed_bytes)
        .expect("generation should succeed for deterministic seed bytes");

    assert_eq!(first, second);
}

#[test]
fn opaque_adaptive_modules_stay_within_safe_v1_shape() {
    let seed_bytes = deterministic_seed_bytes();
    let module = generate_module_from_bytes(&opaque_adaptive_config(), &seed_bytes)
        .expect("generation should produce a module");
    let entry_point = generated_entry_point(&module);
    let float_analysis = crate::qir::inspect::analyze_float_surface(&module);

    assert_eq!(entry_point.name, qir::ENTRYPOINT_NAME);
    assert!(!entry_point.is_declaration);
    assert_eq!(entry_point.return_type, Type::Integer(64));
    assert!(entry_point.params.is_empty());
    assert_eq!(
        entry_point.attribute_group_refs,
        vec![qir::ENTRY_POINT_ATTR_GROUP_ID]
    );
    assert_eq!(module.attribute_groups.len(), 2);
    assert_eq!(module.named_metadata.len(), 1);
    assert_eq!(
        module.metadata_nodes.len(),
        7 + usize::from(float_analysis.has_float_op)
    );
    assert_eq!(
        metadata_string_list(&module, qir::FLOAT_COMPUTATIONS_KEY),
        float_analysis.has_float_op.then(|| {
            float_analysis
                .surface_width_names()
                .into_iter()
                .map(str::to_string)
                .collect()
        })
    );
    assert!(
        module
            .get_flag(qir::QIR_MAJOR_VERSION_KEY)
            .is_some_and(|value| *value == MetadataValue::Int(Type::Integer(32), 2))
    );
    assert!(
        module
            .get_flag(qir::QIR_MINOR_VERSION_KEY)
            .is_some_and(|value| *value == MetadataValue::Int(Type::Integer(32), 0))
    );

    let block_names: Vec<_> = entry_point
        .basic_blocks
        .iter()
        .map(|block| block.name.clone())
        .collect();
    let expected_block_names: Vec<_> = (0..block_names.len())
        .map(|index| format!("block_{index}"))
        .collect();
    assert_eq!(block_names, expected_block_names);

    let global_names: Vec<_> = module
        .globals
        .iter()
        .map(|global| global.name.clone())
        .collect();
    let expected_global_names: Vec<_> = (0..module.globals.len())
        .map(|index| index.to_string())
        .collect();
    assert_eq!(global_names, expected_global_names);

    for block in &entry_point.basic_blocks {
        assert!(
            !block.instructions.is_empty(),
            "generated blocks should always end with a terminator"
        );
        assert!(matches!(
            block
                .instructions
                .last()
                .expect("blocks should contain instructions"),
            Instruction::Ret(_) | Instruction::Jump { .. } | Instruction::Br { .. }
        ));

        for instruction in &block.instructions {
            assert!(matches!(
                instruction,
                Instruction::Ret(_)
                    | Instruction::Jump { .. }
                    | Instruction::Br { .. }
                    | Instruction::Call { .. }
                    | Instruction::BinOp { .. }
                    | Instruction::ICmp { .. }
                    | Instruction::FCmp { .. }
                    | Instruction::Cast { .. }
            ));
        }
    }
}

#[test]
fn bare_roundtrip_profile_omits_qdk_shell_metadata() {
    let seed_bytes = deterministic_seed_bytes();
    let module = generate_module_from_bytes(&bare_roundtrip_config(), &seed_bytes)
        .expect("generation should produce a bare roundtrip module");
    let entry_point = generated_entry_point(&module);

    assert_eq!(entry_point.name, qir::ENTRYPOINT_NAME);
    assert!(entry_point.attribute_group_refs.is_empty());
    assert!(module.attribute_groups.is_empty());
    assert!(module.named_metadata.is_empty());
    assert!(module.metadata_nodes.is_empty());
}

#[test]
fn checked_mode_reuses_model_generation_core() {
    let seed_bytes = deterministic_seed_bytes();
    let config = bare_roundtrip_config();
    let checked_config = QirSmithConfig {
        roundtrip: Some(RoundTripKind::TextAndBitcodeSinglePass),
        ..config.clone()
    };

    let model = generate_module_from_bytes(&config, &seed_bytes)
        .expect("model generation should succeed for deterministic seed bytes");
    let checked = generate_checked_from_bytes(&checked_config, &seed_bytes)
        .expect("checked generation should succeed for deterministic seed bytes");

    assert_eq!(checked.module, model);
    assert_eq!(
        checked.effective_config,
        checked_effective_config(&config, RoundTripKind::TextAndBitcodeSinglePass)
    );
    assert!(checked.text.is_some());
    assert!(checked.bitcode.is_some());
}

#[test]
fn checked_text_only_emits_text_without_bitcode() {
    let checked = generate_checked_from_bytes(
        &QirSmithConfig {
            roundtrip: Some(RoundTripKind::TextOnly),
            ..bare_roundtrip_config()
        },
        &deterministic_seed_bytes(),
    )
    .expect("checked text-only generation should succeed");

    assert!(checked.text.is_some());
    assert!(checked.bitcode.is_none());
}

#[test]
fn checked_bitcode_only_emits_bitcode_without_text() {
    let checked = generate_checked_from_bytes(
        &QirSmithConfig {
            roundtrip: Some(RoundTripKind::BitcodeOnly),
            ..bare_roundtrip_config()
        },
        &deterministic_seed_bytes(),
    )
    .expect("checked bitcode-only generation should succeed");

    assert!(checked.text.is_none());
    assert!(checked.bitcode.is_some());
}

fn synthesize_placeholder_param_names(functions: &mut [Function]) {
    for function in functions {
        for (param_index, param) in function.params.iter_mut().enumerate() {
            if param.name.is_none() {
                param.name = Some(format!("param_{param_index}"));
            }
        }
    }
}

fn annotate_local_ref_operand(operand: &mut Operand, ty: &Type) {
    match operand {
        Operand::LocalRef(name) => {
            *operand = Operand::TypedLocalRef(name.clone(), ty.clone());
        }
        Operand::TypedLocalRef(_, actual_ty) => {
            *actual_ty = ty.clone();
        }
        Operand::GetElementPtr { indices, .. } => {
            for index in indices {
                annotate_local_ref_operand(index, &Type::Integer(64));
            }
        }
        Operand::IntConst(_, _)
        | Operand::FloatConst(_, _)
        | Operand::NullPtr
        | Operand::IntToPtr(_, _)
        | Operand::GlobalRef(_) => {}
    }
}

fn annotate_checked_subset_local_refs(functions: &mut [Function]) {
    for function in functions {
        let return_ty = function.return_type.clone();
        for block in &mut function.basic_blocks {
            for instruction in &mut block.instructions {
                match instruction {
                    Instruction::Ret(Some(value)) => {
                        annotate_local_ref_operand(value, &return_ty);
                    }
                    Instruction::Br { cond_ty, cond, .. } => {
                        annotate_local_ref_operand(cond, cond_ty);
                    }
                    Instruction::BinOp { ty, lhs, rhs, .. }
                    | Instruction::ICmp { ty, lhs, rhs, .. }
                    | Instruction::FCmp { ty, lhs, rhs, .. } => {
                        annotate_local_ref_operand(lhs, ty);
                        annotate_local_ref_operand(rhs, ty);
                    }
                    Instruction::Cast { from_ty, value, .. } => {
                        annotate_local_ref_operand(value, from_ty);
                    }
                    Instruction::Call { args, .. } => {
                        for (arg_ty, operand) in args {
                            annotate_local_ref_operand(operand, arg_ty);
                        }
                    }
                    Instruction::Ret(None)
                    | Instruction::Jump { .. }
                    | Instruction::Phi { .. }
                    | Instruction::Alloca { .. }
                    | Instruction::Load { .. }
                    | Instruction::Store { .. }
                    | Instruction::Select { .. }
                    | Instruction::Switch { .. }
                    | Instruction::GetElementPtr { .. }
                    | Instruction::Unreachable => {}
                }
            }
        }
    }
}

fn checked_bitcode_semantic_fixture() -> Module {
    Module {
        source_filename: Some("checked_bitcode_fixture".to_string()),
        target_datalayout: Some("e-p:64:64".to_string()),
        target_triple: Some("arm64-apple-macosx15.0.0".to_string()),
        struct_types: Vec::new(),
        globals: vec![GlobalVariable {
            name: "message".to_string(),
            ty: Type::Array(6, Box::new(Type::Integer(8))),
            linkage: Linkage::Internal,
            is_constant: true,
            initializer: Some(Constant::CString("hello".to_string())),
        }],
        functions: vec![
            Function {
                name: "callee".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Ptr,
                        name: None,
                    },
                    Param {
                        ty: Type::Integer(64),
                        name: Some("count".to_string()),
                    },
                ],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "test".to_string(),
                return_type: Type::Integer(64),
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![
                    BasicBlock {
                        name: "entry".to_string(),
                        instructions: vec![
                            Instruction::Call {
                                return_ty: None,
                                callee: "callee".to_string(),
                                args: vec![
                                    (Type::Ptr, Operand::GlobalRef("message".to_string())),
                                    (Type::Integer(64), Operand::IntConst(Type::Integer(64), 3)),
                                ],
                                result: None,
                                attr_refs: Vec::new(),
                            },
                            Instruction::BinOp {
                                op: BinOpKind::Add,
                                ty: Type::Integer(64),
                                lhs: Operand::IntConst(Type::Integer(64), 1),
                                rhs: Operand::IntConst(Type::Integer(64), 2),
                                result: "sum".to_string(),
                            },
                            Instruction::ICmp {
                                pred: IntPredicate::Eq,
                                ty: Type::Integer(64),
                                lhs: Operand::LocalRef("sum".to_string()),
                                rhs: Operand::IntConst(Type::Integer(64), 3),
                                result: "cond".to_string(),
                            },
                            Instruction::Br {
                                cond_ty: Type::Integer(1),
                                cond: Operand::LocalRef("cond".to_string()),
                                true_dest: "then".to_string(),
                                false_dest: "exit".to_string(),
                            },
                        ],
                    },
                    BasicBlock {
                        name: "then".to_string(),
                        instructions: vec![Instruction::Ret(Some(Operand::LocalRef(
                            "sum".to_string(),
                        )))],
                    },
                    BasicBlock {
                        name: "exit".to_string(),
                        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
                            Type::Integer(64),
                            0,
                        )))],
                    },
                ],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

#[test]
fn checked_bitcode_equivalence_verifies_attrs_and_metadata() {
    let seed_bytes = deterministic_seed_bytes();
    let original = generate_module_from_bytes(&opaque_adaptive_config(), &seed_bytes)
        .expect("generation should produce a module");
    let mut reparsed = original.clone();
    synthesize_placeholder_param_names(&mut reparsed.functions);

    assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed).expect(
        "checked bitcode comparison should preserve attrs and metadata while allowing placeholder parameter names",
    );
}

#[test]
fn checked_bitcode_equivalence_allows_typed_local_refs_in_instruction_payloads() {
    let original = checked_bitcode_semantic_fixture();
    let mut reparsed = original.clone();
    synthesize_placeholder_param_names(&mut reparsed.functions);
    annotate_checked_subset_local_refs(&mut reparsed.functions);

    assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed).expect(
        "checked bitcode comparison should allow typed local refs and placeholder parameter names when the instruction context fixes the type",
    );
}

#[test]
fn checked_bitcode_rejects_missing_attribute_groups() {
    let seed_bytes = deterministic_seed_bytes();
    let original = generate_module_from_bytes(&opaque_adaptive_config(), &seed_bytes)
        .expect("generation should produce a module");
    let mut reparsed = original.clone();
    reparsed.attribute_groups.clear();

    let err = assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed)
        .expect_err("should reject missing attribute groups");
    assert!(matches!(
        err,
        QirSmithError::BitcodeRoundTrip(message) if message.contains("attribute_groups")
    ));
}

#[test]
fn checked_bitcode_rejects_missing_metadata() {
    let seed_bytes = deterministic_seed_bytes();
    let original = generate_module_from_bytes(&opaque_adaptive_config(), &seed_bytes)
        .expect("generation should produce a module");
    let mut reparsed = original.clone();
    reparsed.named_metadata.clear();
    reparsed.metadata_nodes.clear();

    let err = assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed)
        .expect_err("should reject missing metadata");
    assert!(matches!(
        err,
        QirSmithError::BitcodeRoundTrip(message) if message.contains("named_metadata")
    ));
}

#[test]
fn checked_bitcode_rejects_global_initializer_mismatch() {
    let original = checked_bitcode_semantic_fixture();
    let mut reparsed = original.clone();
    reparsed.globals[0].initializer = Some(Constant::CString("hullo".to_string()));

    let err = assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed)
        .expect_err("should reject changed global initializers");
    assert!(matches!(
        err,
        QirSmithError::BitcodeRoundTrip(message) if message.contains("initializer")
    ));
}

#[test]
fn checked_bitcode_rejects_instruction_payload_mismatch() {
    let original = checked_bitcode_semantic_fixture();
    let mut reparsed = original.clone();
    let Instruction::Call { callee, .. } =
        &mut reparsed.functions[1].basic_blocks[0].instructions[0]
    else {
        panic!("fixture should start with a call instruction");
    };
    *callee = "other_callee".to_string();

    let err = assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed)
        .expect_err("should reject changed instruction payloads");
    assert!(matches!(
        err,
        QirSmithError::BitcodeRoundTrip(message)
            if message.contains("instruction 0 changed")
    ));
}

#[test]
fn checked_mode_reports_unsupported_v1_models_as_model_generation_errors() {
    let seed_bytes = deterministic_seed_bytes();
    let base_config = bare_roundtrip_config();
    let mut module = generate_module_from_bytes(&base_config, &seed_bytes)
        .expect("generation should produce a module");
    let incoming_block = module.functions[0].basic_blocks[0].name.clone();
    module.functions[0].basic_blocks[0].instructions.insert(
        0,
        Instruction::Phi {
            ty: Type::Integer(1),
            incoming: vec![(Operand::IntConst(Type::Integer(1), 1), incoming_block)],
            result: "var_phi".to_string(),
        },
    );

    let mut artifact = GeneratedArtifact {
        effective_config: checked_effective_config(&base_config, RoundTripKind::TextOnly),
        module,
        text: None,
        bitcode: None,
    };

    let err = populate_checked_artifact(&mut artifact)
        .expect_err("checked mode should reject unsupported v1 model shapes");
    assert!(matches!(
        err,
        QirSmithError::ModelGeneration(message) if message.contains("phi")
    ));
}

#[test]
fn checked_text_mismatches_use_text_error_category() {
    let seed_bytes = deterministic_seed_bytes();
    let original = generate_module_from_bytes(&bare_roundtrip_config(), &seed_bytes)
        .expect("generation should produce a module");
    let mut reparsed = original.clone();
    reparsed.functions[0].name.push_str("_changed");

    let err = ensure_text_roundtrip_matches(&original, &reparsed)
        .expect_err("mismatched text roundtrip structure should fail");
    assert!(matches!(
        err,
        QirSmithError::TextRoundTrip(message)
            if message.contains("changed module structure")
    ));
}

#[test]
fn checked_bitcode_mismatches_use_bitcode_error_category() {
    let seed_bytes = deterministic_seed_bytes();
    let original = generate_module_from_bytes(&bare_roundtrip_config(), &seed_bytes)
        .expect("generation should produce a module");
    let mut reparsed = original.clone();
    reparsed.functions.pop();

    let err = assert_bitcode_roundtrip_matches_supported_v1_subset(&original, &reparsed)
        .expect_err("mismatched bitcode roundtrip structure should fail");
    assert!(matches!(
        err,
        QirSmithError::BitcodeRoundTrip(message) if message.contains("function count")
    ));
}

// --- BaseV1 / AdaptiveV1 module generation tests ---

fn base_v1_config() -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: BASE_V1_BLOCK_COUNT,
        max_instrs_per_block: 6,
        ..QirSmithConfig::for_profile(QirProfilePreset::BaseV1)
    }
}

fn adaptive_v1_config() -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: 4,
        max_instrs_per_block: 6,
        ..QirSmithConfig::for_profile(QirProfilePreset::AdaptiveV1)
    }
}

fn control_flow_expansion_config() -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: 5,
        max_instrs_per_block: 10,
        allow_phi: true,
        allow_switch: true,
        ..bare_roundtrip_config()
    }
}

fn memory_expansion_config() -> QirSmithConfig {
    QirSmithConfig {
        max_blocks_per_func: 4,
        max_instrs_per_block: 10,
        allow_memory_ops: true,
        ..bare_roundtrip_config()
    }
}

fn expansion_seed_bank() -> Vec<Vec<u8>> {
    let mut seeds: Vec<_> = (0_u8..=15).map(|byte| vec![byte; 128]).collect();
    seeds.push(deterministic_seed_bytes());
    seeds
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
struct InstructionCoverage {
    phi: bool,
    switch: bool,
    unreachable: bool,
    alloca: bool,
    load: bool,
    store: bool,
    select: bool,
    instruction_gep: bool,
}

impl InstructionCoverage {
    fn observe_module(&mut self, module: &Module) {
        for function in &module.functions {
            for block in &function.basic_blocks {
                for instruction in &block.instructions {
                    match instruction {
                        Instruction::Phi { .. } => self.phi = true,
                        Instruction::Alloca { .. } => self.alloca = true,
                        Instruction::Load { .. } => self.load = true,
                        Instruction::Store { .. } => self.store = true,
                        Instruction::Select { .. } => self.select = true,
                        Instruction::Switch { .. } => self.switch = true,
                        Instruction::Unreachable => self.unreachable = true,
                        Instruction::GetElementPtr { .. } => self.instruction_gep = true,
                        Instruction::Ret(_)
                        | Instruction::Br { .. }
                        | Instruction::Jump { .. }
                        | Instruction::BinOp { .. }
                        | Instruction::ICmp { .. }
                        | Instruction::FCmp { .. }
                        | Instruction::Cast { .. }
                        | Instruction::Call { .. } => {}
                    }
                }
            }
        }
    }
}

fn generated_text_roundtrip_module(config: &QirSmithConfig, seed: &[u8]) -> Module {
    let seed_summary = checked_smoke_seed_summary(seed);
    let text = generate_text_from_bytes(config, seed)
        .unwrap_or_else(|err| panic!("text generation failed for {seed_summary}: {err}"));

    parse_text_roundtrip(&text)
        .unwrap_or_else(|err| panic!("text roundtrip failed for {seed_summary}: {err}"))
}

fn missing_control_flow_families(coverage: &InstructionCoverage) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !coverage.phi {
        missing.push("phi");
    }
    if !coverage.switch {
        missing.push("switch");
    }
    if !coverage.unreachable {
        missing.push("unreachable");
    }
    missing
}

fn missing_memory_families(coverage: &InstructionCoverage) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !coverage.alloca {
        missing.push("alloca");
    }
    if !coverage.load {
        missing.push("load");
    }
    if !coverage.store {
        missing.push("store");
    }
    if !coverage.select {
        missing.push("select");
    }
    if !coverage.instruction_gep {
        missing.push("instruction getelementptr");
    }
    missing
}

fn typed_checked_smoke_seeds() -> [&'static [u8]; 3] {
    [&[0_u8; 64][..], &[1_u8; 64][..], &[42_u8; 128][..]]
}

fn checked_smoke_seed_summary(seed: &[u8]) -> String {
    format!(
        "len={} first_byte={}",
        seed.len(),
        seed.first().copied().unwrap_or_default()
    )
}

fn assert_checked_generation_smoke_case(
    config: &QirSmithConfig,
    seeds: &[&[u8]],
    invariant: impl Fn(&GeneratedArtifact),
) {
    let checked_config = QirSmithConfig {
        output_mode: OutputMode::RoundTripChecked,
        roundtrip: None,
        ..config.clone()
    };
    let expected_effective = checked_config.sanitize();
    let profile = checked_config.profile;
    let expects_bitcode = matches!(
        expected_effective.roundtrip,
        Some(RoundTripKind::BitcodeOnly | RoundTripKind::TextAndBitcodeSinglePass)
    );

    assert!(
        expected_effective.roundtrip.is_some(),
        "{profile:?} checked smoke should sanitize to a roundtrip mode"
    );

    for seed in seeds {
        let seed_summary = checked_smoke_seed_summary(seed);
        let artifact = generate_checked_from_bytes(&checked_config, seed).unwrap_or_else(|err| {
            panic!("{profile:?} checked smoke failed for {seed_summary}: {err}")
        });

        assert_eq!(
            artifact.effective_config, expected_effective,
            "{profile:?} checked smoke should sanitize to the expected config for {seed_summary}"
        );
        assert!(
            artifact.text.is_some(),
            "{profile:?} checked smoke should always emit text for {seed_summary}"
        );
        assert_eq!(
            artifact.bitcode.is_some(),
            expects_bitcode,
            "{profile:?} checked smoke bitcode presence should match sanitize defaults for {seed_summary}"
        );

        invariant(&artifact);
    }
}

fn assert_v1_typed_checked_smoke_shell(artifact: &GeneratedArtifact) {
    assert!(artifact.effective_config.allow_typed_pointers);
    assert!(
        artifact
            .module
            .struct_types
            .iter()
            .any(|ty| ty.name == "Qubit"),
        "typed checked smokes should retain the %Qubit shell type"
    );
    assert!(
        artifact
            .module
            .struct_types
            .iter()
            .any(|ty| ty.name == "Result"),
        "typed checked smokes should retain the %Result shell type"
    );
}

fn assert_base_v1_checked_smoke_invariant(artifact: &GeneratedArtifact) {
    assert_v1_typed_checked_smoke_shell(artifact);
    assert_eq!(artifact.module.metadata_nodes.len(), 4);
    assert!(
        artifact
            .module
            .get_flag(qir::INT_COMPUTATIONS_KEY)
            .is_none()
    );
    assert!(metadata_string_list(&artifact.module, qir::FLOAT_COMPUTATIONS_KEY).is_none());
}

fn assert_adaptive_v1_checked_smoke_invariant(artifact: &GeneratedArtifact) {
    assert_v1_typed_checked_smoke_shell(artifact);
    assert!(
        artifact
            .module
            .get_flag(qir::INT_COMPUTATIONS_KEY)
            .is_some()
    );
}

fn assert_adaptive_v2_checked_smoke_invariant(artifact: &GeneratedArtifact) {
    let analysis = crate::qir::inspect::analyze_float_surface(&artifact.module);

    assert!(!analysis.has_float_op);
    assert!(metadata_string_list(&artifact.module, qir::FLOAT_COMPUTATIONS_KEY).is_none());

    let metadata_ids: Vec<_> = artifact
        .module
        .metadata_nodes
        .iter()
        .map(|node| node.id)
        .collect();
    let expected_ids: Vec<_> =
        (0..u32::try_from(metadata_ids.len()).expect("metadata count should fit in u32")).collect();
    assert_eq!(metadata_ids, expected_ids);
}

fn assert_bare_roundtrip_checked_smoke_invariant(artifact: &GeneratedArtifact) {
    assert!(artifact.module.attribute_groups.is_empty());
    assert!(artifact.module.named_metadata.is_empty());
    assert!(artifact.module.metadata_nodes.is_empty());
}

fn checked_bitcode_only_artifact(config: &QirSmithConfig, seed: &[u8]) -> GeneratedArtifact {
    let checked_config = QirSmithConfig {
        output_mode: OutputMode::RoundTripChecked,
        roundtrip: Some(RoundTripKind::BitcodeOnly),
        ..config.clone()
    };
    let profile = checked_config.profile;
    let seed_summary = checked_smoke_seed_summary(seed);

    let artifact = generate_checked_from_bytes(&checked_config, seed).unwrap_or_else(|err| {
        panic!("{profile:?} checked bitcode-only generation failed for {seed_summary}: {err}")
    });

    assert_eq!(
        artifact.effective_config,
        checked_effective_config(config, RoundTripKind::BitcodeOnly),
        "{profile:?} checked bitcode-only generation should sanitize to the expected config for {seed_summary}"
    );
    assert!(artifact.text.is_none());
    assert!(artifact.bitcode.is_some());

    artifact
}

fn assert_generated_target_headers(module: &Module) {
    assert_eq!(
        module.target_datalayout.as_deref(),
        Some(GENERATED_TARGET_DATALAYOUT)
    );
    assert_eq!(
        module.target_triple.as_deref(),
        Some(GENERATED_TARGET_TRIPLE)
    );
}

fn has_generated_modeled_globals(module: &Module) -> bool {
    module.globals.iter().any(|global| {
        matches!(
            global,
            GlobalVariable {
                ty: Type::Integer(64),
                linkage: Linkage::External,
                is_constant: false,
                initializer: None,
                ..
            }
        )
    })
}

fn assert_generated_modeled_globals(module: &Module) {
    assert!(module.globals.iter().any(|global| {
        matches!(
            global,
            GlobalVariable {
                ty: Type::Integer(64),
                linkage: Linkage::External,
                is_constant: false,
                initializer: None,
                ..
            }
        )
    }));
    assert!(module.globals.iter().any(|global| {
        matches!(
            global,
            GlobalVariable {
                ty: Type::Ptr,
                linkage: Linkage::Internal,
                is_constant: false,
                initializer: Some(Constant::Null),
                ..
            }
        )
    }));
    assert!(module.globals.iter().any(|global| {
        matches!(
            global,
            GlobalVariable {
                ty: Type::Integer(64),
                linkage: Linkage::Internal,
                is_constant: true,
                initializer: Some(Constant::Int(_)),
                ..
            }
        )
    }));
}

fn find_checked_bitcode_only_artifact(
    config: &QirSmithConfig,
    seeds: &[&[u8]],
    predicate: impl Fn(&GeneratedArtifact) -> bool,
    description: &str,
) -> GeneratedArtifact {
    for seed in seeds {
        let artifact = checked_bitcode_only_artifact(config, seed);
        if predicate(&artifact) {
            return artifact;
        }
    }

    panic!(
        "no checked bitcode-only artifact matched {description} across {} fixed seeds",
        seeds.len()
    );
}

#[test]
fn base_v1_generates_correct_shell() {
    let seed_bytes = deterministic_seed_bytes();
    let text = generate_text_from_bytes(&base_v1_config(), &seed_bytes)
        .expect("BaseV1 text generation should succeed");

    assert!(text.contains("%Qubit = type opaque"));
    assert!(text.contains("%Result = type opaque"));
    assert!(text.contains("base_profile"));
    assert!(text.contains("qir_major_version\", i32 1"));
    assert!(text.contains("qir_minor_version\", i32 0"));
    assert!(!text.contains("int_computations"));
    assert!(!text.contains("backwards_branching"));
}

#[test]
fn base_v1_module_has_struct_types_and_v1_metadata() {
    let seed_bytes = deterministic_seed_bytes();
    let module = generate_module_from_bytes(&base_v1_config(), &seed_bytes)
        .expect("BaseV1 module generation should succeed");
    let entry_point = generated_entry_point(&module);

    assert_eq!(module.struct_types.len(), 2);
    assert_eq!(module.named_metadata.len(), 1);
    assert_eq!(module.metadata_nodes.len(), 4);
    assert_eq!(entry_point.name, qir::ENTRYPOINT_NAME);
    assert!(
        module
            .get_flag(qir::QIR_MAJOR_VERSION_KEY)
            .is_some_and(|value| *value == MetadataValue::Int(Type::Integer(32), 1))
    );
    assert!(
        module
            .get_flag(qir::QIR_MINOR_VERSION_KEY)
            .is_some_and(|value| *value == MetadataValue::Int(Type::Integer(32), 0))
    );
    assert_eq!(entry_point.basic_blocks.len(), BASE_V1_BLOCK_COUNT);
}

#[test]
fn adaptive_v1_generates_correct_shell() {
    let seed_bytes = deterministic_seed_bytes();
    let text = generate_text_from_bytes(&adaptive_v1_config(), &seed_bytes)
        .expect("AdaptiveV1 text generation should succeed");

    assert!(text.contains("%Qubit = type opaque"));
    assert!(text.contains("%Result = type opaque"));
    assert!(text.contains("adaptive_profile"));
    assert!(text.contains("qir_major_version\", i32 1"));
    assert!(text.contains("qir_minor_version\", i32 0"));
    assert!(!text.contains("backwards_branching"));
}

#[test]
fn adaptive_v1_module_has_struct_types_and_v1_metadata() {
    let seed_bytes = deterministic_seed_bytes();
    let module = generate_module_from_bytes(&adaptive_v1_config(), &seed_bytes)
        .expect("AdaptiveV1 module generation should succeed");
    let entry_point = generated_entry_point(&module);

    assert_eq!(module.struct_types.len(), 2);
    assert_eq!(module.named_metadata.len(), 1);
    assert!(
        module.metadata_nodes.len() >= 4 && module.metadata_nodes.len() <= 6,
        "AdaptiveV1 should have 4 base nodes plus up to 2 optional capability nodes, got {}",
        module.metadata_nodes.len()
    );
    assert_eq!(entry_point.name, qir::ENTRYPOINT_NAME);
    assert!(
        module
            .get_flag(qir::QIR_MAJOR_VERSION_KEY)
            .is_some_and(|value| *value == MetadataValue::Int(Type::Integer(32), 1))
    );
    assert!(
        module
            .get_flag(qir::QIR_MINOR_VERSION_KEY)
            .is_some_and(|value| *value == MetadataValue::Int(Type::Integer(32), 0))
    );
}

// --- BaseV1 / AdaptiveV1 text roundtrip tests ---

#[test]
fn base_v1_text_roundtrip_checked() {
    let seeds = typed_checked_smoke_seeds();
    assert_checked_generation_smoke_case(
        &base_v1_config(),
        &seeds,
        assert_base_v1_checked_smoke_invariant,
    );
}

#[test]
fn adaptive_v1_text_roundtrip_checked() {
    let seeds = typed_checked_smoke_seeds();
    assert_checked_generation_smoke_case(
        &adaptive_v1_config(),
        &seeds,
        assert_adaptive_v1_checked_smoke_invariant,
    );
}

#[test]
fn bare_roundtrip_checked_omits_qdk_shell_metadata() {
    let seed_bytes = deterministic_seed_bytes();
    let seeds = [seed_bytes.as_slice()];

    assert_checked_generation_smoke_case(
        &bare_roundtrip_config(),
        &seeds,
        assert_bare_roundtrip_checked_smoke_invariant,
    );
}

#[test]
fn bare_roundtrip_checked_bitcode_only_emits_target_headers() {
    let seed_bytes = deterministic_seed_bytes();
    let artifact = checked_bitcode_only_artifact(&bare_roundtrip_config(), &seed_bytes);

    assert_generated_target_headers(&artifact.module);
}

#[test]
fn bare_roundtrip_checked_bitcode_only_preserves_broader_modeled_globals() {
    let deterministic_seed = deterministic_seed_bytes();
    let seeds = [
        deterministic_seed.as_slice(),
        &[0_u8; 64][..],
        &[1_u8; 64][..],
        &[42_u8; 128][..],
    ];
    let artifact = find_checked_bitcode_only_artifact(
        &bare_roundtrip_config(),
        &seeds,
        |artifact| has_generated_modeled_globals(&artifact.module),
        "broader modeled globals",
    );

    assert_generated_modeled_globals(&artifact.module);
}

#[test]
fn bare_roundtrip_checked_bitcode_only_preserves_generated_headers() {
    let seed_bytes = deterministic_seed_bytes();
    let artifact = checked_bitcode_only_artifact(&bare_roundtrip_config(), &seed_bytes);
    let bitcode = artifact
        .bitcode
        .as_ref()
        .expect("checked bitcode-only generation should emit bitcode");
    let reparsed = parse_bitcode_roundtrip(bitcode)
        .expect("checked roundtrip should preserve generated target headers");

    assert_generated_target_headers(&reparsed);
    assert_bitcode_roundtrip_matches_supported_v1_subset(&artifact.module, &reparsed)
        .expect("checked compat roundtrip should preserve generated target header fidelity");
}

#[test]
fn bare_roundtrip_checked_bitcode_only_also_succeeds_through_strict_parse() {
    let deterministic_seed = deterministic_seed_bytes();
    let seeds = [
        deterministic_seed.as_slice(),
        &[0_u8; 64][..],
        &[1_u8; 64][..],
        &[42_u8; 128][..],
    ];
    let artifact = find_checked_bitcode_only_artifact(
        &bare_roundtrip_config(),
        &seeds,
        |artifact| has_generated_modeled_globals(&artifact.module),
        "strict-parse parity coverage for broader modeled globals",
    );
    let bitcode = artifact
        .bitcode
        .as_ref()
        .expect("checked bitcode-only generation should emit bitcode");
    let strict = crate::bitcode::reader::parse_bitcode(bitcode)
        .expect("zero-diagnostic checked bitcode should also succeed through strict parse_bitcode");

    assert_generated_target_headers(&strict);
    assert_generated_modeled_globals(&strict);
    assert_bitcode_roundtrip_matches_supported_v1_subset(&artifact.module, &strict).expect(
        "strict bitcode parse should preserve generated header and global fidelity for the supported subset",
    );
}

// --- Sanitize tests for v1 profiles ---

#[test]
fn sanitize_preserves_base_v1_profile() {
    let effective = QirSmithConfig {
        output_mode: OutputMode::Text,
        ..QirSmithConfig::for_profile(QirProfilePreset::BaseV1)
    }
    .sanitize();

    assert_eq!(effective.profile, QirProfilePreset::BaseV1);
    assert!(effective.allow_typed_pointers);
    assert!(!effective.bare_roundtrip_mode);
    assert_eq!(effective.max_blocks_per_func, BASE_V1_BLOCK_COUNT);
}

#[test]
fn sanitize_preserves_adaptive_v1_profile() {
    let effective = QirSmithConfig {
        output_mode: OutputMode::Text,
        ..QirSmithConfig::for_profile(QirProfilePreset::AdaptiveV1)
    }
    .sanitize();

    assert_eq!(effective.profile, QirProfilePreset::AdaptiveV1);
    assert!(effective.allow_typed_pointers);
    assert!(!effective.bare_roundtrip_mode);
}

#[test]
fn sanitize_v1_profiles_default_to_text_only_roundtrip() {
    let base_v1_effective = QirSmithConfig {
        output_mode: OutputMode::RoundTripChecked,
        roundtrip: None,
        ..QirSmithConfig::for_profile(QirProfilePreset::BaseV1)
    }
    .sanitize();

    assert_eq!(base_v1_effective.roundtrip, Some(RoundTripKind::TextOnly));

    let adaptive_v1_effective = QirSmithConfig {
        output_mode: OutputMode::RoundTripChecked,
        roundtrip: None,
        ..QirSmithConfig::for_profile(QirProfilePreset::AdaptiveV1)
    }
    .sanitize();

    assert_eq!(
        adaptive_v1_effective.roundtrip,
        Some(RoundTripKind::TextOnly)
    );
}

#[test]
fn initialize_declaration_appears_when_flag_enabled() {
    // Use all-1s seed: take_flag(bytes, false) returns true for all flags
    let seed_bytes: Vec<u8> = vec![1; 128];
    let text = generate_text_from_bytes(&base_v1_config(), &seed_bytes)
        .expect("BaseV1 text generation should succeed");

    assert!(
        text.contains("@__quantum__rt__initialize"),
        "Initialize declaration should appear when flag is set"
    );
}

#[test]
fn initialize_call_in_entry_block_when_flag_enabled() {
    let seed_bytes: Vec<u8> = vec![1; 128];
    let text = generate_text_from_bytes(&base_v1_config(), &seed_bytes)
        .expect("BaseV1 text generation should succeed");

    if text.contains("@__quantum__rt__initialize") {
        assert!(
            text.contains("call void @__quantum__rt__initialize("),
            "Initialize call should appear when declaration is present"
        );
    }
}

#[test]
fn initialize_always_present_for_qdk_shell() {
    // Initialize is always included for QDK shell presets.
    let seed_bytes: Vec<u8> = vec![0; 128];
    let text = generate_text_from_bytes(&base_v1_config(), &seed_bytes)
        .expect("BaseV1 text generation should succeed");

    assert!(
        text.contains("call void @__quantum__rt__initialize("),
        "Initialize call should always appear for QDK shell presets"
    );
}

#[test]
fn adaptive_v1_emits_capability_metadata_when_flags_enabled() {
    // Use all-1s seed: all take_flag calls return true
    let seed_bytes: Vec<u8> = vec![1; 128];
    let module = generate_module_from_bytes(&adaptive_v1_config(), &seed_bytes)
        .expect("AdaptiveV1 module generation should succeed");

    let has_int = module.get_flag(qir::INT_COMPUTATIONS_KEY).is_some();
    let has_float = module.get_flag(qir::FLOAT_COMPUTATIONS_KEY).is_some();

    assert!(
        has_int || has_float,
        "AdaptiveV1 with all-1s seed should emit at least one capability metadata node"
    );
}

#[test]
fn adaptive_v1_zero_seed_smoke_omits_float_metadata_without_float_operations() {
    let seed_bytes = vec![0_u8; 16];
    let module = generate_module_from_bytes(&adaptive_v1_config(), &seed_bytes)
        .expect("AdaptiveV1 module generation should succeed");
    let analysis = crate::qir::inspect::analyze_float_surface(&module);

    assert!(
        module.get_flag(qir::INT_COMPUTATIONS_KEY).is_some(),
        "AdaptiveV1 should always emit int_computations"
    );
    assert!(
        !analysis.has_float_op,
        "zero seed should continue to exercise a no-float generator path"
    );
    assert!(
        analysis.surface_width_names().is_empty(),
        "no-float AdaptiveV1 modules should not retain float-typed IR surface"
    );
    assert!(
        metadata_string_list(&module, qir::FLOAT_COMPUTATIONS_KEY).is_none(),
        "float_computations should be omitted when no floating-point operation exists"
    );
}

#[test]
fn adaptive_v1_checked_seed_smokes_do_not_leave_float_surface_without_float_ops() {
    for seed in [&[0_u8; 64][..], &[1_u8; 64], &[42_u8; 128]] {
        let module = generate_module_from_bytes(&adaptive_v1_config(), seed)
            .expect("AdaptiveV1 module generation should succeed");
        let analysis = crate::qir::inspect::analyze_float_surface(&module);

        assert!(
            analysis.has_float_op || analysis.surface_width_names().is_empty(),
            "AdaptiveV1 should not leave float-typed IR surface without a float op for seed length {} and first byte {}",
            seed.len(),
            seed.first().copied().unwrap_or_default()
        );
    }
}

#[test]
fn adaptive_v2_checked_empty_seed_keeps_metadata_ids_dense() {
    let seeds: [&[u8]; 1] = [&[]];

    assert_checked_generation_smoke_case(
        &QirSmithConfig::for_profile(QirProfilePreset::AdaptiveV2),
        &seeds,
        assert_adaptive_v2_checked_smoke_invariant,
    );
}

#[test]
fn opt_in_text_roundtrip_emits_control_flow_instruction_slice() {
    let config = control_flow_expansion_config();
    let mut coverage = InstructionCoverage::default();

    for seed in expansion_seed_bank() {
        let module = generated_text_roundtrip_module(&config, &seed);
        coverage.observe_module(&module);

        if missing_control_flow_families(&coverage).is_empty() {
            break;
        }
    }

    let missing = missing_control_flow_families(&coverage);
    assert!(
        missing.is_empty(),
        "fixed qir_smith expansion seeds should emit phi, switch, and unreachable after text roundtrip; missing {}",
        missing.join(", ")
    );
}

#[test]
fn opt_in_text_roundtrip_emits_memory_instruction_slice() {
    let config = memory_expansion_config();
    let mut coverage = InstructionCoverage::default();

    for seed in expansion_seed_bank() {
        let module = generated_text_roundtrip_module(&config, &seed);
        coverage.observe_module(&module);

        if missing_memory_families(&coverage).is_empty() {
            break;
        }
    }

    let missing = missing_memory_families(&coverage);
    assert!(
        missing.is_empty(),
        "fixed qir_smith expansion seeds should emit alloca, load, store, select, and instruction getelementptr after text roundtrip; missing {}",
        missing.join(", ")
    );
}

#[test]
fn adaptive_float_builders_cover_half_float_and_double() {
    let mut state = adaptive_shell_state();

    for (selector, expected_ty) in [
        (0_u8, Type::Half),
        (1_u8, Type::Float),
        (2_u8, Type::Double),
    ] {
        let mut pool = state.build_base_value_pool();
        let selector_bytes = [selector; 16];
        let mut bytes = Unstructured::new(&selector_bytes);

        let binop = state
            .build_float_binop_instruction(&mut pool, &mut bytes)
            .expect("float binop builder should succeed for supported widths");
        assert!(matches!(binop, Instruction::BinOp { ty, .. } if ty == expected_ty.clone()));

        let mut pool = state.build_base_value_pool();
        let selector_bytes = [selector; 16];
        let mut bytes = Unstructured::new(&selector_bytes);
        let fcmp = state
            .build_fcmp_instruction(&mut pool, &mut bytes)
            .expect("fcmp builder should succeed for supported widths");
        assert!(matches!(fcmp, Instruction::FCmp { ty, .. } if ty == expected_ty.clone()));

        let mut pool = state.build_base_value_pool();
        let selector_bytes = [selector; 16];
        let mut bytes = Unstructured::new(&selector_bytes);
        let sitofp = state
            .build_sitofp_instruction(&mut pool, &mut bytes)
            .expect("sitofp builder should succeed for supported widths");
        assert!(
            matches!(sitofp, Instruction::Cast { op: CastKind::Sitofp, to_ty, .. } if to_ty == expected_ty.clone())
        );

        let mut pool = state.build_base_value_pool();
        let selector_bytes = [selector; 16];
        let mut bytes = Unstructured::new(&selector_bytes);
        let fptosi = state
            .build_fptosi_instruction(&mut pool, &mut bytes)
            .expect("fptosi builder should succeed for supported widths");
        assert!(
            matches!(fptosi, Instruction::Cast { op: CastKind::Fptosi, from_ty, .. } if from_ty == expected_ty)
        );
    }
}

#[test]
fn finalize_float_computations_rewrites_supported_metadata_to_exact_surface_subset() {
    let mut module = adaptive_v1_module_with_float_metadata_shell(
        vec![GlobalVariable {
            name: "g".to_string(),
            ty: Type::Float,
            linkage: Linkage::Internal,
            is_constant: false,
            initializer: None,
        }],
        Vec::new(),
        vec![
            Instruction::BinOp {
                op: BinOpKind::Fadd,
                ty: Type::Half,
                lhs: Operand::float_const(Type::Half, 1.0),
                rhs: Operand::float_const(Type::Half, 2.0),
                result: "sum".to_string(),
            },
            Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
        ],
    );

    assert_eq!(
        metadata_string_list(&module, qir::FLOAT_COMPUTATIONS_KEY),
        Some(vec![
            "half".to_string(),
            "float".to_string(),
            "double".to_string(),
        ])
    );

    finalize_float_computations(&mut module);

    assert_eq!(
        metadata_string_list(&module, qir::FLOAT_COMPUTATIONS_KEY),
        Some(vec!["half".to_string(), "float".to_string()])
    );
}

#[test]
fn finalize_float_computations_removes_flag_without_float_operations() {
    let mut module = adaptive_v1_module_with_float_metadata_shell(
        Vec::new(),
        vec![double_record_output_declaration()],
        vec![
            Instruction::Call {
                return_ty: None,
                callee: qir::rt::DOUBLE_RECORD_OUTPUT.to_string(),
                args: vec![
                    (Type::Double, Operand::float_const(Type::Double, 1.0)),
                    (Type::Ptr, Operand::NullPtr),
                ],
                result: None,
                attr_refs: Vec::new(),
            },
            Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
        ],
    );
    let analysis = crate::qir::inspect::analyze_float_surface(&module);

    assert!(!analysis.has_float_op);
    assert_eq!(analysis.surface_width_names(), vec!["double"]);
    assert!(metadata_string_list(&module, qir::FLOAT_COMPUTATIONS_KEY).is_some());

    finalize_float_computations(&mut module);

    assert!(metadata_string_list(&module, qir::FLOAT_COMPUTATIONS_KEY).is_none());
}

#[test]
fn base_v1_never_emits_capability_metadata() {
    for seed in [&[0u8; 128][..], &[1; 128], &[42; 128]] {
        let module = generate_module_from_bytes(&base_v1_config(), seed)
            .expect("BaseV1 module generation should succeed");

        assert!(
            module.get_flag(qir::INT_COMPUTATIONS_KEY).is_none(),
            "BaseV1 should never emit int_computations"
        );
        assert!(
            module.get_flag(qir::FLOAT_COMPUTATIONS_KEY).is_none(),
            "BaseV1 should never emit float_computations"
        );
    }
}

#[test]
fn parse_bitcode_roundtrip_preserves_supported_global_initializers() {
    let Some(lane) = available_fast_matrix_lanes().into_iter().next() else {
        eprintln!(
            "no external LLVM fast-matrix lane is available, skipping qir_smith global-initializer regression"
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
            "llvm@{} should assemble qir_smith global-initializer fixture: {error}",
            lane.version
        )
    });

    let module = parse_bitcode_roundtrip(&bitcode)
        .expect("checked roundtrip should preserve supported global initializers");

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
