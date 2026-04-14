// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! QIR profile validation module.
//!
//! Validates a [`Module`] against the QIR profile it claims to conform to,
//! returning structured [`QirProfileError`] diagnostics for all rule
//! violations found. Supports Base Profile v1 and Adaptive Profile variants.

#[cfg(test)]
mod tests;

use crate::model::Type;
use crate::model::{
    BasicBlock, BinOpKind, CastKind, Constant, Function, Instruction, Module, Operand,
};
use crate::qir::{
    self, DYNAMIC_QUBIT_MGMT_KEY, DYNAMIC_RESULT_MGMT_KEY, IRREVERSIBLE_ATTR,
    OUTPUT_LABELING_SCHEMA_ATTR, QIR_MAJOR_VERSION_KEY, QIR_MINOR_VERSION_KEY, QIR_PROFILES_ATTR,
    QirProfile, REQUIRED_NUM_QUBITS_ATTR, REQUIRED_NUM_RESULTS_ATTR, inspect,
};
use miette::Diagnostic;
use rustc_hash::FxHashSet;
use thiserror::Error;

/// Detected profile and capabilities from module introspection.
#[derive(Debug)]
pub struct DetectedProfile {
    pub profile: QirProfile,
    pub capabilities: Capabilities,
}

/// Capability flags extracted from module flags metadata.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Capabilities {
    pub int_computations: Vec<String>,
    pub float_computations: Vec<String>,
    pub ir_functions: bool,
    pub backwards_branching: u8,
    pub multiple_target_branching: bool,
    pub multiple_return_points: bool,
    pub dynamic_qubit_management: bool,
    pub dynamic_result_management: bool,
    pub arrays: bool,
}

/// QIR profile validation error with Miette diagnostic support.
#[derive(Clone, Debug, Diagnostic, Error, PartialEq, Eq)]
pub enum QirProfileError {
    #[error("missing opaque `{type_name}` struct type definition")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MissingOpaqueType"),
        help(
            "QIR profiles using typed pointers require opaque Qubit and Result struct type definitions"
        )
    )]
    MissingOpaqueType { type_name: String },

    #[error("expected exactly 1 entry point, found {count}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.EntryPointCount"),
        help("a QIR module must define exactly one function with the `entry_point` attribute")
    )]
    EntryPointCount { count: usize },

    #[error("missing required module flag `{flag_name}`")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MissingModuleFlag"),
        help(
            "all QIR profiles require qir_major_version, qir_minor_version, dynamic_qubit_management, and dynamic_result_management flags"
        )
    )]
    MissingModuleFlag { flag_name: String },

    #[error("llvm.module.flags references missing metadata node !{node_ref}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.DanglingModuleFlagReference"),
        help("remove or repair the dangling llvm.module.flags reference")
    )]
    DanglingModuleFlagReference { node_ref: u32 },

    #[error("malformed llvm.module.flags node !{node_ref}: {reason}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MalformedModuleFlagNode"),
        help("repair the malformed llvm.module.flags metadata node structure")
    )]
    MalformedModuleFlagNode { node_ref: u32, reason: String },

    #[error("malformed module flag `{flag_name}`: {reason}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MalformedModuleFlag"),
        help("repair the module flag payload so validation can interpret it")
    )]
    MalformedModuleFlag { flag_name: String, reason: String },

    #[error("entry point missing required attribute `{attr_name}`")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MissingEntryPointAttr"),
        help(
            "entry points require: entry_point, qir_profiles, output_labeling_schema, and required_num_qubits/results unless the matching dynamic management flag is enabled"
        )
    )]
    MissingEntryPointAttr { attr_name: String },

    #[error("qir_profiles attribute value `{found}` does not match detected profile `{expected}`")]
    #[diagnostic(code("Qsc.Llvm.QirValidator.ProfileMismatch"), severity(Warning))]
    ProfileMismatch { expected: String, found: String },

    #[error(
        "unsupported qir_profiles value `{profile_name}` with qir_major_version `{major_version}`"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.UnsupportedProfileMetadata"),
        help(
            "use a supported profile/major-version pair such as base_profile+1 or adaptive_profile+1/2"
        )
    )]
    UnsupportedProfileMetadata {
        profile_name: String,
        major_version: i64,
    },

    #[error("base profile requires `{flag_name}` to be false")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.BaseDynamicMgmtEnabled"),
        help("the base profile does not support dynamic qubit or result management")
    )]
    BaseDynamicMgmtEnabled { flag_name: String },

    #[error("entry point must have no parameters, found {param_count}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.EntryPointParams"),
        help(
            "QIR entry points take no parameters; qubit and result allocation is expressed through inttoptr casts"
        )
    )]
    EntryPointParams { param_count: usize },

    #[error("entry point must return i64, found {found_type}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.EntryPointReturnType"),
        help("the return type represents an exit code: 0 = success")
    )]
    EntryPointReturnType { found_type: String },

    #[error(
        "base profile requires exactly 4 basic blocks (entry, body, measurements, output), found {block_count}"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.BaseBlockCount"),
        help(
            "base profile programs follow a fixed 4-block structure connected by unconditional branches"
        )
    )]
    BaseBlockCount { block_count: usize },

    #[error("instruction `{instruction}` is not allowed in {profile} profile{context}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.InstructionNotAllowed"),
        help("{suggestion}")
    )]
    InstructionNotAllowed {
        instruction: String,
        profile: String,
        context: String,
        suggestion: String,
    },

    #[error(
        "conditional branch is not allowed in base profile in function `{function}` block {block}"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.BaseConditionalBranch"),
        help("base profile only allows unconditional branching between its 4 fixed blocks")
    )]
    BaseConditionalBranch { function: String, block: usize },

    #[error("{instruction} requires `{capability}` capability flag in {location}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MissingCapability"),
        help(
            "set the `{capability}` module flag to enable this instruction in the adaptive profile"
        )
    )]
    MissingCapability {
        instruction: String,
        capability: String,
        location: String,
    },

    #[error("missing required declaration for `{function_name}`")]
    #[diagnostic(code("Qsc.Llvm.QirValidator.MissingDeclaration"), help("{help_text}"))]
    MissingDeclaration {
        function_name: String,
        help_text: String,
    },

    #[error(
        "incorrect signature for `{function_name}`: expected {expected_sig}, found {found_sig}"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.WrongSignature"),
        help("runtime function signatures must match the QIR spec")
    )]
    WrongSignature {
        function_name: String,
        expected_sig: String,
        found_sig: String,
    },

    #[error(
        "`__quantum__rt__initialize` has incorrect signature: expected void(ptr), found {found_sig}"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.InitializeWrongSignature"),
        help("initialize must accept a single ptr argument and return void")
    )]
    InitializeWrongSignature { found_sig: String },

    #[error("QIS function `{function_name}` must return void in base profile, found {found_type}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.QisNonVoidReturn"),
        help(
            "base profile requires all QIS functions to return void; measurement results are communicated via writeonly result pointers"
        )
    )]
    QisNonVoidReturn {
        function_name: String,
        found_type: String,
    },

    #[error("measurement function `{function_name}` must have `irreversible` attribute")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MissingIrreversible"),
        help("measurement functions must be marked irreversible per the QIR spec")
    )]
    MissingIrreversible { function_name: String },

    #[error("measurement function `{function_name}` result parameter must be `writeonly`")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.MissingWriteonly"),
        help(
            "measurement result pointers must be writeonly to ensure results are only consumed by output recording functions"
        )
    )]
    MissingWriteonly { function_name: String },

    #[error(
        "base profile requires linear control flow: block {block_idx} does not jump to block {expected_next}"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.NonLinearFlow"),
        help(
            "base profile blocks must form a linear sequence: entry -> body -> measurements -> output"
        )
    )]
    NonLinearFlow {
        block_idx: usize,
        expected_next: usize,
    },

    #[error("control flow graph contains a cycle without `backwards_branching` capability")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.UnauthorizedCycle"),
        help("set the `backwards_branching` module flag to enable loops in the adaptive profile")
    )]
    UnauthorizedCycle { function: String },

    #[error(
        "multiple return points without `multiple_return_points` capability (found {ret_count} ret instructions)"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.UnauthorizedMultipleReturns"),
        help("set the `multiple_return_points` module flag to enable multiple ret statements")
    )]
    UnauthorizedMultipleReturns { function: String, ret_count: usize },

    #[error("{feature} instructions are used but `{flag_name}` capability is not declared")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.CapabilityNotDeclared"),
        help("add the `{flag_name}` module flag to declare this capability")
    )]
    CapabilityNotDeclared { feature: String, flag_name: String },

    #[error("float width `{width_name}` is used but not declared in `float_computations`")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.FloatWidthNotDeclared"),
        help(
            "add `{width_name}` to the `float_computations` module flag or remove the float-typed IR surface"
        )
    )]
    FloatWidthNotDeclared { width_name: String },

    #[error(
        "`float_computations` is declared but the module contains no floating-point operations"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.FloatCapabilityWithoutOperation"),
        help("remove the `float_computations` module flag or add a floating-point operation")
    )]
    FloatCapabilityWithoutOperation,

    #[error("array instruction `{instruction}` requires `arrays` capability in {location}")]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.ArraysNotEnabled"),
        help(
            "set the `arrays` module flag to enable array types and operations in the adaptive profile"
        )
    )]
    ArraysNotEnabled {
        instruction: String,
        location: String,
    },

    #[error(
        "output-recording call `{function_name}` requires a string label operand in {location}, found {found_operand}"
    )]
    #[diagnostic(
        code("Qsc.Llvm.QirValidator.InvalidOutputLabelOperand"),
        help("use a global constant string or a getelementptr derived from one for output labels")
    )]
    InvalidOutputLabelOperand {
        function_name: String,
        location: String,
        found_operand: String,
    },
}

/// Result of profile validation.
#[derive(Debug)]
pub struct QirProfileValidation {
    pub detected: DetectedProfile,
    pub errors: Vec<QirProfileError>,
}

/// Validate a [`Module`] against the QIR profile it claims to conform to.
///
/// Returns the detected profile + capabilities and all errors found.
#[must_use]
pub fn validate_qir_profile(module: &Module) -> QirProfileValidation {
    let (detected, malformed_flags, mut errors) = detect_profile(module);
    errors.extend(validate_module_structure(
        module,
        &detected,
        &malformed_flags,
    ));
    errors.extend(validate_entry_point(module, &detected));
    errors.extend(validate_instructions(module, &detected, &malformed_flags));
    errors.extend(validate_declarations(module, &detected));
    errors.extend(validate_output_recording_calls(module));
    errors.extend(validate_cfg(module, &detected, &malformed_flags));
    errors.extend(validate_consistency(module, &detected, &malformed_flags));
    QirProfileValidation { detected, errors }
}

fn detect_profile(module: &Module) -> (DetectedProfile, FxHashSet<String>, Vec<QirProfileError>) {
    let mut errors = convert_module_flag_issues(&inspect::inspect_module_flag_metadata(module));
    let mut malformed_flags = FxHashSet::default();

    let major_flag = inspect::inspect_module_flag_int(module, QIR_MAJOR_VERSION_KEY);
    malformed_flags.extend(collect_malformed_flag_names(&major_flag.issues));
    errors.extend(convert_module_flag_issues(&major_flag.issues));

    let minor_flag = inspect::inspect_module_flag_int(module, QIR_MINOR_VERSION_KEY);
    malformed_flags.extend(collect_malformed_flag_names(&minor_flag.issues));
    errors.extend(convert_module_flag_issues(&minor_flag.issues));

    let (profile_name, major, minor) = detect_profile_metadata(module);

    let profile = match (profile_name.as_deref(), major, minor) {
        (Some("base_profile"), Some(1), _) => QirProfile::BaseV1,
        (Some("adaptive_profile"), Some(1), _) => QirProfile::AdaptiveV1,
        (Some("adaptive_profile"), Some(2), _) => QirProfile::AdaptiveV2,
        (Some(profile_name), Some(major_version), _) => {
            errors.push(QirProfileError::UnsupportedProfileMetadata {
                profile_name: profile_name.to_string(),
                major_version,
            });
            QirProfile::BaseV1
        }
        _ => QirProfile::BaseV1,
    };

    let (capabilities, capability_malformed_flags, capability_errors) =
        extract_capabilities(module);
    malformed_flags.extend(capability_malformed_flags);
    errors.extend(capability_errors);

    (
        DetectedProfile {
            profile,
            capabilities,
        },
        malformed_flags,
        errors,
    )
}

fn detect_profile_metadata(module: &Module) -> (Option<String>, Option<i64>, Option<i64>) {
    // Try to read qir_profiles from entry-point attributes.
    let profile_name = inspect::find_entry_point(module)
        .and_then(|idx| inspect::get_function_attribute(module, idx, QIR_PROFILES_ATTR))
        .map(String::from);

    let major = inspect::inspect_module_flag_int(module, QIR_MAJOR_VERSION_KEY).value;
    let minor = inspect::inspect_module_flag_int(module, QIR_MINOR_VERSION_KEY).value;

    (profile_name, major, minor)
}

fn extract_capabilities(
    module: &Module,
) -> (Capabilities, FxHashSet<String>, Vec<QirProfileError>) {
    let mut capabilities = Capabilities::default();
    let mut malformed_flags = FxHashSet::default();
    let mut errors = Vec::new();

    let int_computations = inspect::inspect_module_flag_string_list(module, "int_computations");
    malformed_flags.extend(collect_malformed_flag_names(&int_computations.issues));
    errors.extend(convert_module_flag_issues(&int_computations.issues));
    capabilities.int_computations = int_computations.value.unwrap_or_default();

    let float_computations = inspect::inspect_module_flag_string_list(module, "float_computations");
    malformed_flags.extend(collect_malformed_flag_names(&float_computations.issues));
    errors.extend(convert_module_flag_issues(&float_computations.issues));
    capabilities.float_computations = float_computations.value.unwrap_or_default();

    let ir_functions = inspect::inspect_module_flag_bool(module, "ir_functions");
    malformed_flags.extend(collect_malformed_flag_names(&ir_functions.issues));
    errors.extend(convert_module_flag_issues(&ir_functions.issues));
    capabilities.ir_functions = ir_functions.value.unwrap_or(false);

    let backwards_branching = inspect::inspect_module_flag_int(module, "backwards_branching");
    malformed_flags.extend(collect_malformed_flag_names(&backwards_branching.issues));
    errors.extend(convert_module_flag_issues(&backwards_branching.issues));
    if let Some(value) = backwards_branching.value {
        if let Ok(value) = u8::try_from(value) {
            capabilities.backwards_branching = value;
        } else {
            malformed_flags.insert("backwards_branching".to_string());
            errors.push(QirProfileError::MalformedModuleFlag {
                flag_name: "backwards_branching".into(),
                reason: format!("expected an integer between 0 and 255, found `{value}`"),
            });
        }
    }

    let multiple_target_branching =
        inspect::inspect_module_flag_bool(module, "multiple_target_branching");
    malformed_flags.extend(collect_malformed_flag_names(
        &multiple_target_branching.issues,
    ));
    errors.extend(convert_module_flag_issues(
        &multiple_target_branching.issues,
    ));
    capabilities.multiple_target_branching = multiple_target_branching.value.unwrap_or(false);

    let multiple_return_points =
        inspect::inspect_module_flag_bool(module, "multiple_return_points");
    malformed_flags.extend(collect_malformed_flag_names(&multiple_return_points.issues));
    errors.extend(convert_module_flag_issues(&multiple_return_points.issues));
    capabilities.multiple_return_points = multiple_return_points.value.unwrap_or(false);

    let dynamic_qubit_management =
        inspect::inspect_module_flag_bool(module, DYNAMIC_QUBIT_MGMT_KEY);
    malformed_flags.extend(collect_malformed_flag_names(
        &dynamic_qubit_management.issues,
    ));
    errors.extend(convert_module_flag_issues(&dynamic_qubit_management.issues));
    capabilities.dynamic_qubit_management = dynamic_qubit_management.value.unwrap_or(false);

    let dynamic_result_management =
        inspect::inspect_module_flag_bool(module, DYNAMIC_RESULT_MGMT_KEY);
    malformed_flags.extend(collect_malformed_flag_names(
        &dynamic_result_management.issues,
    ));
    errors.extend(convert_module_flag_issues(
        &dynamic_result_management.issues,
    ));
    capabilities.dynamic_result_management = dynamic_result_management.value.unwrap_or(false);

    let arrays = inspect::inspect_module_flag_bool(module, "arrays");
    malformed_flags.extend(collect_malformed_flag_names(&arrays.issues));
    errors.extend(convert_module_flag_issues(&arrays.issues));
    capabilities.arrays = arrays.value.unwrap_or(false);

    (capabilities, malformed_flags, errors)
}

fn convert_module_flag_issues(issues: &[inspect::ModuleFlagIssue]) -> Vec<QirProfileError> {
    issues
        .iter()
        .map(|issue| match issue {
            inspect::ModuleFlagIssue::DanglingReference { node_ref } => {
                QirProfileError::DanglingModuleFlagReference {
                    node_ref: *node_ref,
                }
            }
            inspect::ModuleFlagIssue::MalformedNode { node_ref, reason } => {
                QirProfileError::MalformedModuleFlagNode {
                    node_ref: *node_ref,
                    reason: (*reason).to_string(),
                }
            }
            inspect::ModuleFlagIssue::InvalidBehavior {
                flag_name,
                node_id,
                found,
            } => QirProfileError::MalformedModuleFlag {
                flag_name: flag_name.clone(),
                reason: format!("node !{node_id} has non-integer merge behavior payload `{found}`"),
            },
            inspect::ModuleFlagIssue::InvalidValue {
                flag_name,
                node_id,
                expected,
                found,
            } => QirProfileError::MalformedModuleFlag {
                flag_name: flag_name.clone(),
                reason: format!("node !{node_id} expected {expected} payload, found `{found}`"),
            },
            inspect::ModuleFlagIssue::InvalidStringListItem {
                flag_name,
                node_id,
                index,
                found,
            } => QirProfileError::MalformedModuleFlag {
                flag_name: flag_name.clone(),
                reason: format!(
                    "node !{node_id} has non-string string-list item at index {index}: `{found}`"
                ),
            },
        })
        .collect()
}

fn collect_malformed_flag_names(issues: &[inspect::ModuleFlagIssue]) -> FxHashSet<String> {
    issues
        .iter()
        .filter_map(|issue| issue.flag_name().map(str::to_string))
        .collect()
}

fn validate_module_structure(
    module: &Module,
    detected: &DetectedProfile,
    malformed_flags: &FxHashSet<String>,
) -> Vec<QirProfileError> {
    let mut v = Vec::new();

    // MS-01: Typed-pointer profiles need Qubit/Result struct types.
    if detected.profile.uses_typed_pointers() {
        let has_qubit = module.struct_types.iter().any(|s| s.name == "Qubit");
        let has_result = module.struct_types.iter().any(|s| s.name == "Result");
        if !has_qubit {
            v.push(QirProfileError::MissingOpaqueType {
                type_name: "Qubit".into(),
            });
        }
        if !has_result {
            v.push(QirProfileError::MissingOpaqueType {
                type_name: "Result".into(),
            });
        }
    }

    // MS-03: Exactly one entry point.
    let entry_count = inspect::count_entry_points(module);
    if entry_count != 1 {
        v.push(QirProfileError::EntryPointCount { count: entry_count });
    }

    // MF-01..04: Required module flags.
    check_required_flag(&mut v, module, QIR_MAJOR_VERSION_KEY);
    check_required_flag(&mut v, module, QIR_MINOR_VERSION_KEY);
    check_required_flag(&mut v, module, DYNAMIC_QUBIT_MGMT_KEY);
    check_required_flag(&mut v, module, DYNAMIC_RESULT_MGMT_KEY);

    // MF-05: qir_major_version behavior must be Error (1).
    let qir_major_behavior = inspect::inspect_module_flag_behavior(module, QIR_MAJOR_VERSION_KEY);
    v.extend(convert_module_flag_issues(&qir_major_behavior.issues));
    if let Some(behavior) = qir_major_behavior.value
        && behavior != qir::FLAG_BEHAVIOR_ERROR
    {
        v.push(QirProfileError::MissingModuleFlag {
            flag_name: format!(
                "{QIR_MAJOR_VERSION_KEY} behavior must be Error (1), found {behavior}"
            ),
        });
    }

    // MF-06: qir_minor_version behavior must be Max (7).
    let qir_minor_behavior = inspect::inspect_module_flag_behavior(module, QIR_MINOR_VERSION_KEY);
    v.extend(convert_module_flag_issues(&qir_minor_behavior.issues));
    if let Some(behavior) = qir_minor_behavior.value
        && behavior != qir::FLAG_BEHAVIOR_MAX
    {
        v.push(QirProfileError::MissingModuleFlag {
            flag_name: format!(
                "{QIR_MINOR_VERSION_KEY} behavior must be Max (7), found {behavior}"
            ),
        });
    }

    // AT-01..05: Entry point attribute checks.
    if let Some(ep_idx) = inspect::find_entry_point(module) {
        let func = &module.functions[ep_idx];
        check_entry_point_attrs(&mut v, module, ep_idx, func, detected, malformed_flags);
    }

    // DT-04/DT-05: Base profile must have dynamic_*_management = false.
    if detected.profile == QirProfile::BaseV1 {
        if detected.capabilities.dynamic_qubit_management {
            v.push(QirProfileError::BaseDynamicMgmtEnabled {
                flag_name: "dynamic_qubit_management".into(),
            });
        }
        if detected.capabilities.dynamic_result_management {
            v.push(QirProfileError::BaseDynamicMgmtEnabled {
                flag_name: "dynamic_result_management".into(),
            });
        }
    }

    v
}

fn check_required_flag(v: &mut Vec<QirProfileError>, module: &Module, key: &str) {
    if inspect::get_module_flag(module, key).is_none() {
        v.push(QirProfileError::MissingModuleFlag {
            flag_name: key.into(),
        });
    }
}

fn check_entry_point_attrs(
    v: &mut Vec<QirProfileError>,
    module: &Module,
    ep_idx: usize,
    _func: &Function,
    detected: &DetectedProfile,
    malformed_flags: &FxHashSet<String>,
) {
    // AT-01: entry_point attribute (implicitly satisfied if find_entry_point found it).

    // AT-02: qir_profiles matches.
    let expected_name = detected.profile.profile_name();
    if let Some(actual) = inspect::get_function_attribute(module, ep_idx, QIR_PROFILES_ATTR) {
        if actual != expected_name {
            v.push(QirProfileError::ProfileMismatch {
                expected: expected_name.into(),
                found: actual.into(),
            });
        }
    } else {
        v.push(QirProfileError::MissingEntryPointAttr {
            attr_name: "qir_profiles".into(),
        });
    }

    let require_qubit_count = !detected.capabilities.dynamic_qubit_management
        && !malformed_flags.contains(DYNAMIC_QUBIT_MGMT_KEY);
    let require_result_count = !detected.capabilities.dynamic_result_management
        && !malformed_flags.contains(DYNAMIC_RESULT_MGMT_KEY);

    // AT-03 + AT-06: required_num_qubits (must exist and parse as u64 unless dynamic).
    match inspect::get_function_attribute(module, ep_idx, REQUIRED_NUM_QUBITS_ATTR) {
        None if require_qubit_count => {
            v.push(QirProfileError::MissingEntryPointAttr {
                attr_name: "required_num_qubits".into(),
            });
        }
        Some(val) if val.parse::<u64>().is_err() => {
            v.push(QirProfileError::MissingEntryPointAttr {
                attr_name: "required_num_qubits (must be a non-negative integer)".into(),
            });
        }
        _ => {}
    }

    // AT-04 + AT-07: required_num_results (must exist and parse as u64 unless dynamic).
    match inspect::get_function_attribute(module, ep_idx, REQUIRED_NUM_RESULTS_ATTR) {
        None if require_result_count => {
            v.push(QirProfileError::MissingEntryPointAttr {
                attr_name: "required_num_results".into(),
            });
        }
        Some(val) if val.parse::<u64>().is_err() => {
            v.push(QirProfileError::MissingEntryPointAttr {
                attr_name: "required_num_results (must be a non-negative integer)".into(),
            });
        }
        _ => {}
    }

    // AT-05: output_labeling_schema.
    if !inspect::has_function_attribute(module, ep_idx, OUTPUT_LABELING_SCHEMA_ATTR) {
        v.push(QirProfileError::MissingEntryPointAttr {
            attr_name: "output_labeling_schema".into(),
        });
    }
}

fn validate_entry_point(module: &Module, detected: &DetectedProfile) -> Vec<QirProfileError> {
    let mut v = Vec::new();
    let Some(ep_idx) = inspect::find_entry_point(module) else {
        return v; // Already reported as MS-03.
    };
    let func = &module.functions[ep_idx];

    // EP-01: No parameters.
    if !func.params.is_empty() {
        v.push(QirProfileError::EntryPointParams {
            param_count: func.params.len(),
        });
    }

    // EP-02: Return type = i64.
    if func.return_type != Type::Integer(64) {
        v.push(QirProfileError::EntryPointReturnType {
            found_type: func.return_type.to_string(),
        });
    }

    // EP-03: Base profile requires exactly 4 blocks.
    if detected.profile == QirProfile::BaseV1 && func.basic_blocks.len() != 4 {
        v.push(QirProfileError::BaseBlockCount {
            block_count: func.basic_blocks.len(),
        });
    }

    // EP-08 / CI-04: Base profile — all branches must be unconditional.
    if detected.profile == QirProfile::BaseV1 {
        for (bi, bb) in func.basic_blocks.iter().enumerate() {
            for instr in &bb.instructions {
                if matches!(instr, Instruction::Br { .. }) {
                    v.push(QirProfileError::BaseConditionalBranch {
                        function: func.name.clone(),
                        block: bi,
                    });
                }
            }
        }
    }

    v
}

fn validate_instructions(
    module: &Module,
    detected: &DetectedProfile,
    malformed_flags: &FxHashSet<String>,
) -> Vec<QirProfileError> {
    let mut v = Vec::new();
    let Some(ep_idx) = inspect::find_entry_point(module) else {
        return v;
    };
    let func = &module.functions[ep_idx];

    for (bi, bb) in func.basic_blocks.iter().enumerate() {
        for (ii, instr) in bb.instructions.iter().enumerate() {
            let context = format!(" in function '{}' block {bi} instruction {ii}", func.name);
            check_instruction_allowed(instr, detected, malformed_flags, &context, &mut v);
        }
    }

    v
}

#[allow(clippy::too_many_lines)]
fn check_instruction_allowed(
    instr: &Instruction,
    detected: &DetectedProfile,
    malformed_flags: &FxHashSet<String>,
    context: &str,
    v: &mut Vec<QirProfileError>,
) {
    let profile_name = detected.profile.profile_name();
    let int_flag_malformed = malformed_flags.contains("int_computations");
    let float_flag_malformed = malformed_flags.contains("float_computations");
    let backwards_branching_malformed = malformed_flags.contains("backwards_branching");
    let multiple_target_branching_malformed = malformed_flags.contains("multiple_target_branching");
    let arrays_flag_malformed = malformed_flags.contains("arrays");
    match instr {
        // Always allowed in all profiles.
        Instruction::Call { .. }
        | Instruction::Ret(_)
        | Instruction::Jump { .. }
        | Instruction::GetElementPtr { .. }
        | Instruction::Unreachable => {}

        // Cast — only IntToPtr allowed in base; expanded in adaptive.
        Instruction::Cast { op, .. } => match op {
            CastKind::IntToPtr => {} // Allowed in all profiles.
            CastKind::Zext | CastKind::Sext | CastKind::Trunc
                if !matches!(detected.profile, QirProfile::BaseV1)
                    && !int_flag_malformed
                    && !detected.capabilities.int_computations.is_empty() => {}
            CastKind::FpExt | CastKind::FpTrunc
                if !matches!(detected.profile, QirProfile::BaseV1)
                    && !float_flag_malformed
                    && !detected.capabilities.float_computations.is_empty() => {}
            CastKind::Sitofp | CastKind::Fptosi
                if !matches!(detected.profile, QirProfile::BaseV1)
                    && !int_flag_malformed
                    && !float_flag_malformed
                    && !detected.capabilities.int_computations.is_empty()
                    && !detected.capabilities.float_computations.is_empty() => {}
            CastKind::Zext | CastKind::Sext | CastKind::Trunc
                if !matches!(detected.profile, QirProfile::BaseV1) && int_flag_malformed => {}
            CastKind::FpExt | CastKind::FpTrunc
                if !matches!(detected.profile, QirProfile::BaseV1) && float_flag_malformed => {}
            CastKind::Sitofp | CastKind::Fptosi
                if !matches!(detected.profile, QirProfile::BaseV1)
                    && (int_flag_malformed || float_flag_malformed) => {}
            _ => {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: format!("cast {op:?}"),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "only inttoptr casts are allowed in base profile; adaptive profiles require appropriate capability flags".into(),
                });
            }
        },

        // Conditional branch — not allowed in base.
        Instruction::Br { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                // Already reported under CI-04 in Pass 2; avoid double-report.
            }
            // For adaptive — conditional branch is always allowed.
        }

        // BinOp — not allowed in base; adaptive depends on capabilities.
        Instruction::BinOp { op, .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: format!("binop {op:?}"),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "binary operations are not allowed in base profile".into(),
                });
            } else if is_int_binop(op)
                && !int_flag_malformed
                && detected.capabilities.int_computations.is_empty()
            {
                v.push(QirProfileError::MissingCapability {
                    instruction: format!("integer binop {op:?}"),
                    capability: "int_computations".into(),
                    location: context.into(),
                });
            } else if is_float_binop(op)
                && !float_flag_malformed
                && detected.capabilities.float_computations.is_empty()
            {
                v.push(QirProfileError::MissingCapability {
                    instruction: format!("float binop {op:?}"),
                    capability: "float_computations".into(),
                    location: context.into(),
                });
            }
        }

        // ICmp — not allowed in base; needs int cap in adaptive.
        Instruction::ICmp { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: "icmp".into(),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "integer comparison is not allowed in base profile".into(),
                });
            } else if !int_flag_malformed && detected.capabilities.int_computations.is_empty() {
                v.push(QirProfileError::MissingCapability {
                    instruction: "icmp".into(),
                    capability: "int_computations".into(),
                    location: context.into(),
                });
            }
        }

        // FCmp — not allowed in base; needs float cap in adaptive.
        Instruction::FCmp { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: "fcmp".into(),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "float comparison is not allowed in base profile".into(),
                });
            } else if !float_flag_malformed && detected.capabilities.float_computations.is_empty() {
                v.push(QirProfileError::MissingCapability {
                    instruction: "fcmp".into(),
                    capability: "float_computations".into(),
                    location: context.into(),
                });
            }
        }

        // Phi — only adaptive with backwards_branching or int_computations.
        Instruction::Phi { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: "phi".into(),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "phi nodes are not allowed in base profile".into(),
                });
            } else if detected.capabilities.backwards_branching == 0
                && detected.capabilities.int_computations.is_empty()
                && !backwards_branching_malformed
                && !int_flag_malformed
            {
                v.push(QirProfileError::MissingCapability {
                    instruction: "phi".into(),
                    capability: "backwards_branching or int_computations".into(),
                    location: context.into(),
                });
            }
        }

        // Select — adaptive with int_computations.
        Instruction::Select { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: "select".into(),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "select is not allowed in base profile".into(),
                });
            } else if !int_flag_malformed && detected.capabilities.int_computations.is_empty() {
                v.push(QirProfileError::MissingCapability {
                    instruction: "select".into(),
                    capability: "int_computations".into(),
                    location: context.into(),
                });
            }
        }

        // Switch — adaptive with multiple_target_branching.
        Instruction::Switch { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: "switch".into(),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: "switch is not allowed in base profile".into(),
                });
            } else if !multiple_target_branching_malformed
                && !detected.capabilities.multiple_target_branching
            {
                v.push(QirProfileError::MissingCapability {
                    instruction: "switch".into(),
                    capability: "multiple_target_branching".into(),
                    location: context.into(),
                });
            }
        }

        // Alloca, Load, Store — not in base; may appear in ir_functions context.
        Instruction::Alloca { .. } | Instruction::Load { .. } | Instruction::Store { .. } => {
            if detected.profile == QirProfile::BaseV1 {
                v.push(QirProfileError::InstructionNotAllowed {
                    instruction: instruction_name(instr).into(),
                    profile: profile_name.into(),
                    context: context.into(),
                    suggestion: format!(
                        "{} is not allowed in base profile",
                        instruction_name(instr)
                    ),
                });
            } else if !arrays_flag_malformed && !detected.capabilities.arrays {
                // AR-02: Alloca/Load/Store require arrays capability in adaptive profiles.
                v.push(QirProfileError::ArraysNotEnabled {
                    instruction: instruction_name(instr).into(),
                    location: context.into(),
                });
            }
        }
    }
}

fn is_int_binop(op: &BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Add
            | BinOpKind::Sub
            | BinOpKind::Mul
            | BinOpKind::Sdiv
            | BinOpKind::Udiv
            | BinOpKind::Srem
            | BinOpKind::Urem
            | BinOpKind::Shl
            | BinOpKind::Ashr
            | BinOpKind::Lshr
            | BinOpKind::And
            | BinOpKind::Or
            | BinOpKind::Xor
    )
}

fn is_float_binop(op: &BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Fadd | BinOpKind::Fsub | BinOpKind::Fmul | BinOpKind::Fdiv
    )
}

fn instruction_name(instr: &Instruction) -> &'static str {
    match instr {
        Instruction::Ret(_) => "ret",
        Instruction::Br { .. } => "br",
        Instruction::Jump { .. } => "jump",
        Instruction::BinOp { .. } => "binop",
        Instruction::ICmp { .. } => "icmp",
        Instruction::FCmp { .. } => "fcmp",
        Instruction::Cast { .. } => "cast",
        Instruction::Call { .. } => "call",
        Instruction::Phi { .. } => "phi",
        Instruction::Alloca { .. } => "alloca",
        Instruction::Load { .. } => "load",
        Instruction::Store { .. } => "store",
        Instruction::Select { .. } => "select",
        Instruction::Switch { .. } => "switch",
        Instruction::GetElementPtr { .. } => "getelementptr",
        Instruction::Unreachable => "unreachable",
    }
}

#[allow(clippy::too_many_lines)]
fn validate_declarations(module: &Module, detected: &DetectedProfile) -> Vec<QirProfileError> {
    let mut v = Vec::new();

    // RT-01: __quantum__rt__initialize must be declared.
    let has_initialize = module
        .functions
        .iter()
        .any(|f| f.is_declaration && f.name == qir::rt::INITIALIZE);
    if !has_initialize {
        v.push(QirProfileError::MissingDeclaration {
            function_name: qir::rt::INITIALIZE.into(),
            help_text: "the runtime initialize function must be declared for all QIR profiles"
                .into(),
        });
    }

    // RT-02: If __quantum__rt__initialize is declared, validate signature void(ptr).
    if let Some(init_func) = module
        .functions
        .iter()
        .find(|f| f.is_declaration && f.name == qir::rt::INITIALIZE)
    {
        let ok = init_func.return_type == Type::Void && init_func.params.len() == 1;
        if !ok {
            v.push(QirProfileError::InitializeWrongSignature {
                found_sig: format_function_sig(init_func),
            });
        }
    }

    // AP-MC-02: Adaptive profiles require __quantum__rt__read_result.
    if detected.profile != QirProfile::BaseV1 {
        let has_read_result = module.functions.iter().any(|f| {
            f.is_declaration
                && f.name == qir::rt::READ_RESULT
                && f.return_type == Type::Integer(1)
                && f.params.len() == 1
        });
        if !has_read_result {
            v.push(QirProfileError::MissingDeclaration {
                function_name: qir::rt::READ_RESULT.into(),
                help_text:
                    "adaptive profile requires __quantum__rt__read_result(ptr) → i1 declaration"
                        .into(),
            });
        }
    }

    // QIS-01: QIS functions should return void (base) or void/data type (adaptive).
    // QIS-02: Measurement QIS functions must have irreversible attribute.
    for (func_idx, func) in module.functions.iter().enumerate() {
        if !func.is_declaration {
            continue;
        }

        if func.name.starts_with("__quantum__qis__") {
            // Base profile: all QIS must return void.
            if detected.profile == QirProfile::BaseV1 && func.return_type != Type::Void {
                v.push(QirProfileError::QisNonVoidReturn {
                    function_name: func.name.clone(),
                    found_type: func.return_type.to_string(),
                });
            }

            // QIS-02: Measurement functions must have irreversible attribute.
            if func.name.to_lowercase().contains("measure") {
                if !inspect::has_function_attribute(module, func_idx, IRREVERSIBLE_ATTR) {
                    v.push(QirProfileError::MissingIrreversible {
                        function_name: func.name.clone(),
                    });
                }
            }
        }

        // RT-03: Check output recording function signatures.
        if func.name == qir::rt::TUPLE_RECORD_OUTPUT || func.name == qir::rt::ARRAY_RECORD_OUTPUT {
            // Expected: void(i64, ptr)
            let ok = func.return_type == Type::Void
                && func.params.len() == 2
                && func.params[0].ty == Type::Integer(64);
            if !ok {
                v.push(QirProfileError::WrongSignature {
                    function_name: func.name.clone(),
                    expected_sig: "void(i64, ptr)".into(),
                    found_sig: format_function_sig(func),
                });
            }
        }

        if func.name == qir::rt::RESULT_RECORD_OUTPUT {
            // Expected: void(ptr, ptr)
            let ok = func.return_type == Type::Void && func.params.len() == 2;
            if !ok {
                v.push(QirProfileError::WrongSignature {
                    function_name: func.name.clone(),
                    expected_sig: "void(ptr, ptr)".into(),
                    found_sig: format_function_sig(func),
                });
            }
        }

        if func.name == qir::rt::RESULT_ARRAY_RECORD_OUTPUT {
            // Expected: void(i64, ptr, ptr)
            let ok = func.return_type == Type::Void
                && func.params.len() == 3
                && func.params[0].ty == Type::Integer(64);
            if !ok {
                v.push(QirProfileError::WrongSignature {
                    function_name: func.name.clone(),
                    expected_sig: "void(i64, ptr, ptr)".into(),
                    found_sig: format_function_sig(func),
                });
            }
        }

        // RT-04: __quantum__rt__bool_record_output signature void(i1, ptr).
        if func.name == qir::rt::BOOL_RECORD_OUTPUT {
            let ok = func.return_type == Type::Void
                && func.params.len() == 2
                && func.params[0].ty == Type::Integer(1);
            if !ok {
                v.push(QirProfileError::WrongSignature {
                    function_name: func.name.clone(),
                    expected_sig: "void(i1, ptr)".into(),
                    found_sig: format_function_sig(func),
                });
            }
        }

        // RT-05: __quantum__rt__int_record_output signature void(i64, ptr).
        if func.name == qir::rt::INT_RECORD_OUTPUT {
            let ok = func.return_type == Type::Void
                && func.params.len() == 2
                && func.params[0].ty == Type::Integer(64);
            if !ok {
                v.push(QirProfileError::WrongSignature {
                    function_name: func.name.clone(),
                    expected_sig: "void(i64, ptr)".into(),
                    found_sig: format_function_sig(func),
                });
            }
        }

        // RT-06: __quantum__rt__double_record_output
        if func.name == qir::rt::DOUBLE_RECORD_OUTPUT {
            let ok = func.return_type == Type::Void
                && func.params.len() == 2
                && func.params[0].ty == Type::Double;
            if !ok {
                v.push(QirProfileError::WrongSignature {
                    function_name: func.name.clone(),
                    expected_sig: "void(double, ptr)".into(),
                    found_sig: format_function_sig(func),
                });
            }
        }
    }

    // RT-07: dynamic_qubit_management requires qubit_allocate.
    if detected.capabilities.dynamic_qubit_management {
        check_required_declaration(
            &mut v,
            module,
            qir::rt::QUBIT_ALLOCATE,
            "dynamic_qubit_management requires __quantum__rt__qubit_allocate declaration",
        );
    }

    // RT-08: dynamic_qubit_management requires qubit_release.
    if detected.capabilities.dynamic_qubit_management {
        check_required_declaration(
            &mut v,
            module,
            qir::rt::QUBIT_RELEASE,
            "dynamic_qubit_management requires __quantum__rt__qubit_release declaration",
        );
    }

    // RT-09: dynamic_result_management requires result_allocate.
    if detected.capabilities.dynamic_result_management {
        check_required_declaration(
            &mut v,
            module,
            qir::rt::RESULT_ALLOCATE,
            "dynamic_result_management requires __quantum__rt__result_allocate declaration",
        );
    }

    // RT-10: dynamic_result_management requires result_release.
    if detected.capabilities.dynamic_result_management {
        check_required_declaration(
            &mut v,
            module,
            qir::rt::RESULT_RELEASE,
            "dynamic_result_management requires __quantum__rt__result_release declaration",
        );
    }

    // AR-03: arrays capability requires array RT functions.
    if detected.capabilities.arrays {
        if detected.capabilities.dynamic_qubit_management {
            check_required_declaration(
                &mut v,
                module,
                qir::rt::QUBIT_ARRAY_ALLOCATE,
                "arrays + dynamic_qubit_management requires __quantum__rt__qubit_array_allocate declaration",
            );
            check_required_declaration(
                &mut v,
                module,
                qir::rt::QUBIT_ARRAY_RELEASE,
                "arrays + dynamic_qubit_management requires __quantum__rt__qubit_array_release declaration",
            );
        }
        if detected.capabilities.dynamic_result_management {
            check_required_declaration(
                &mut v,
                module,
                qir::rt::RESULT_ARRAY_ALLOCATE,
                "arrays + dynamic_result_management requires __quantum__rt__result_array_allocate declaration",
            );
            check_required_declaration(
                &mut v,
                module,
                qir::rt::RESULT_ARRAY_RELEASE,
                "arrays + dynamic_result_management requires __quantum__rt__result_array_release declaration",
            );
        }
    }

    if module_uses_function(module, qir::rt::RESULT_ARRAY_RECORD_OUTPUT) {
        check_required_declaration(
            &mut v,
            module,
            qir::rt::RESULT_ARRAY_RECORD_OUTPUT,
            "calls to __quantum__rt__result_array_record_output require a matching declaration",
        );
    }

    v
}

fn check_required_declaration(
    v: &mut Vec<QirProfileError>,
    module: &Module,
    name: &str,
    help: &str,
) {
    let found = module
        .functions
        .iter()
        .any(|f| f.is_declaration && f.name == name);
    if !found {
        v.push(QirProfileError::MissingDeclaration {
            function_name: name.into(),
            help_text: help.into(),
        });
    }
}

fn module_uses_function(module: &Module, name: &str) -> bool {
    module
        .functions
        .iter()
        .filter(|func| !func.is_declaration)
        .flat_map(|func| &func.basic_blocks)
        .flat_map(|block| &block.instructions)
        .any(|instr| matches!(instr, Instruction::Call { callee, .. } if callee == name))
}

fn format_function_sig(func: &Function) -> String {
    let params: Vec<String> = func.params.iter().map(|p| p.ty.to_string()).collect();
    format!("{}({})", func.return_type, params.join(", "))
}

fn validate_output_recording_calls(module: &Module) -> Vec<QirProfileError> {
    let mut v = Vec::new();

    for func in module.functions.iter().filter(|func| !func.is_declaration) {
        for (block_idx, block) in func.basic_blocks.iter().enumerate() {
            for (instr_idx, instr) in block.instructions.iter().enumerate() {
                let Instruction::Call { callee, args, .. } = instr else {
                    continue;
                };

                let Some(label_arg_index) = qir::output_label_arg_index(callee) else {
                    continue;
                };

                let Some((_, label_operand)) = args.get(label_arg_index) else {
                    continue;
                };

                if !is_string_label_operand(module, label_operand) {
                    v.push(QirProfileError::InvalidOutputLabelOperand {
                        function_name: callee.clone(),
                        location: format!(
                            "function '{}' block {block_idx} instruction {instr_idx}",
                            func.name
                        ),
                        found_operand: describe_operand(label_operand),
                    });
                }
            }
        }
    }

    v
}
fn is_string_label_operand(module: &Module, operand: &Operand) -> bool {
    match operand {
        Operand::GlobalRef(name) => is_string_label_global(module, name),
        Operand::GetElementPtr { ptr, .. } => is_string_label_global(module, ptr),
        _ => false,
    }
}

fn is_string_label_global(module: &Module, name: &str) -> bool {
    module.globals.iter().any(|global| {
        global.name == name
            && global.is_constant
            && matches!(global.initializer, Some(Constant::CString(_)))
    })
}

fn describe_operand(operand: &Operand) -> String {
    match operand {
        Operand::LocalRef(name) => format!("local %{name}"),
        Operand::TypedLocalRef(name, ty) => format!("local %{name} ({ty})"),
        Operand::IntConst(_, value) => format!("integer constant {value}"),
        Operand::FloatConst(ty, value) => format!("{ty} constant {value}"),
        Operand::NullPtr => "null pointer".into(),
        Operand::IntToPtr(value, _) => format!("inttoptr({value})"),
        Operand::GetElementPtr { ptr, .. } => format!("getelementptr from @{ptr}"),
        Operand::GlobalRef(name) => format!("global @{name}"),
    }
}

fn validate_cfg(
    module: &Module,
    detected: &DetectedProfile,
    malformed_flags: &FxHashSet<String>,
) -> Vec<QirProfileError> {
    let mut v = Vec::new();
    let Some(ep_idx) = inspect::find_entry_point(module) else {
        return v;
    };
    let func = &module.functions[ep_idx];

    if func.basic_blocks.is_empty() {
        return v;
    }

    if detected.profile == QirProfile::BaseV1 {
        // CF-01: Linear flow — each block jumps to the next sequentially.
        validate_base_linear_flow(func, &mut v);
    } else {
        // CF-03: Cycle detection when backwards_branching = 0.
        if detected.capabilities.backwards_branching == 0
            && !malformed_flags.contains("backwards_branching")
        {
            detect_cycles(func, &mut v);
        }
        // CF-04: Multiple ret only if multiple_return_points.
        if !detected.capabilities.multiple_return_points
            && !malformed_flags.contains("multiple_return_points")
        {
            let ret_count = count_ret_instructions(func);
            if ret_count > 1 {
                v.push(QirProfileError::UnauthorizedMultipleReturns {
                    function: func.name.clone(),
                    ret_count,
                });
            }
        }
    }

    v
}

fn validate_base_linear_flow(func: &Function, v: &mut Vec<QirProfileError>) {
    // Each block (except last) should end with an unconditional jump to the next block.
    for (bi, bb) in func.basic_blocks.iter().enumerate() {
        if bi == func.basic_blocks.len() - 1 {
            continue; // Last block should end with ret, not checked here.
        }
        let term = bb.instructions.last();
        let next_idx = bi + 1;
        let next_name = &func.basic_blocks[next_idx].name;
        match term {
            Some(Instruction::Jump { dest }) if dest == next_name => {} // OK
            _ => {
                v.push(QirProfileError::NonLinearFlow {
                    block_idx: bi,
                    expected_next: next_idx,
                });
            }
        }
    }
}

fn detect_cycles(func: &Function, v: &mut Vec<QirProfileError>) {
    let n = func.basic_blocks.len();
    if n == 0 {
        return;
    }

    // Build block-name → index map.
    let block_index: FxHashSet<_> = func
        .basic_blocks
        .iter()
        .enumerate()
        .map(|(i, bb)| (bb.name.as_str(), i))
        .collect::<Vec<_>>()
        .into_iter()
        .collect();

    // Build adjacency list.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (bi, bb) in func.basic_blocks.iter().enumerate() {
        for dest_name in block_successors(bb) {
            // Look up block index by name.
            if let Some(&(_, idx)) = block_index.iter().find(|&&(name, _)| name == dest_name) {
                adj[bi].push(idx);
            }
        }
    }

    // DFS back-edge detection.
    let mut color = vec![0u8; n]; // 0=white, 1=gray, 2=black
    let mut has_cycle = false;
    dfs_cycle_check(0, &adj, &mut color, &mut has_cycle);

    if has_cycle {
        v.push(QirProfileError::UnauthorizedCycle {
            function: func.name.clone(),
        });
    }
}

fn dfs_cycle_check(node: usize, adj: &[Vec<usize>], color: &mut [u8], has_cycle: &mut bool) {
    color[node] = 1; // Gray
    for &next in &adj[node] {
        if color[next] == 1 {
            *has_cycle = true;
            return;
        }
        if color[next] == 0 {
            dfs_cycle_check(next, adj, color, has_cycle);
            if *has_cycle {
                return;
            }
        }
    }
    color[node] = 2; // Black
}

fn block_successors(bb: &BasicBlock) -> Vec<&str> {
    let Some(term) = bb.instructions.last() else {
        return Vec::new();
    };
    match term {
        Instruction::Jump { dest } => vec![dest.as_str()],
        Instruction::Br {
            true_dest,
            false_dest,
            ..
        } => vec![true_dest.as_str(), false_dest.as_str()],
        Instruction::Switch {
            default_dest,
            cases,
            ..
        } => {
            let mut dests = vec![default_dest.as_str()];
            for (_, dest) in cases {
                dests.push(dest.as_str());
            }
            dests
        }
        _ => Vec::new(),
    }
}

fn count_ret_instructions(func: &Function) -> usize {
    func.basic_blocks
        .iter()
        .flat_map(|bb| &bb.instructions)
        .filter(|i| matches!(i, Instruction::Ret(_)))
        .count()
}

fn validate_consistency(
    module: &Module,
    detected: &DetectedProfile,
    malformed_flags: &FxHashSet<String>,
) -> Vec<QirProfileError> {
    let mut v = Vec::new();

    // Only applies to adaptive profiles.
    if detected.profile == QirProfile::BaseV1 {
        return v;
    }

    let used = scan_used_capabilities(module, detected);
    let float_analysis = inspect::analyze_float_surface(module);
    let used_float_widths = float_analysis.surface_width_names();
    let float_flag_present = inspect::get_module_flag(module, "float_computations").is_some()
        && !malformed_flags.contains("float_computations");
    let declared_float_widths: FxHashSet<_> = detected
        .capabilities
        .float_computations
        .iter()
        .map(String::as_str)
        .collect();

    // CR-01: int instructions used → int_computations must be declared.
    if used.has_int_instructions
        && detected.capabilities.int_computations.is_empty()
        && !malformed_flags.contains("int_computations")
    {
        v.push(QirProfileError::CapabilityNotDeclared {
            feature: "integer computation".into(),
            flag_name: "int_computations".into(),
        });
    }

    // CR-02: any float-typed IR surface requires float_computations to be declared.
    if !used_float_widths.is_empty()
        && !float_flag_present
        && !malformed_flags.contains("float_computations")
    {
        v.push(QirProfileError::CapabilityNotDeclared {
            feature: "floating-point type usage".into(),
            flag_name: "float_computations".into(),
        });
    }

    // CR-02a: float_computations may only be declared when the module has a float op.
    if float_flag_present && !float_analysis.has_float_op {
        v.push(QirProfileError::FloatCapabilityWithoutOperation);
    }

    // CR-02b: every used float width must be declared in float_computations.
    if float_flag_present {
        for width_name in used_float_widths {
            if !declared_float_widths.contains(width_name) {
                v.push(QirProfileError::FloatWidthNotDeclared {
                    width_name: width_name.to_string(),
                });
            }
        }
    }

    // CR-03: Non-entry-point function definitions → ir_functions must be true.
    if used.has_ir_functions
        && !detected.capabilities.ir_functions
        && !malformed_flags.contains("ir_functions")
    {
        v.push(QirProfileError::CapabilityNotDeclared {
            feature: "non-entry-point function definition".into(),
            flag_name: "ir_functions".into(),
        });
    }

    // CR-05: switch used → multiple_target_branching must be true.
    if used.has_switch
        && !detected.capabilities.multiple_target_branching
        && !malformed_flags.contains("multiple_target_branching")
    {
        v.push(QirProfileError::CapabilityNotDeclared {
            feature: "switch".into(),
            flag_name: "multiple_target_branching".into(),
        });
    }

    // CR-06: Multiple ret → multiple_return_points must be true.
    if used.ret_count > 1
        && !detected.capabilities.multiple_return_points
        && !malformed_flags.contains("multiple_return_points")
    {
        v.push(QirProfileError::CapabilityNotDeclared {
            feature: "multiple return point".into(),
            flag_name: "multiple_return_points".into(),
        });
    }

    // CR-07: phi used → backwards_branching > 0.
    if used.has_phi
        && detected.capabilities.backwards_branching == 0
        && !malformed_flags.contains("backwards_branching")
    {
        v.push(QirProfileError::CapabilityNotDeclared {
            feature: "phi".into(),
            flag_name: "backwards_branching".into(),
        });
    }

    v
}

#[allow(clippy::struct_excessive_bools)]
struct UsedCapabilities {
    has_int_instructions: bool,
    has_switch: bool,
    has_phi: bool,
    has_ir_functions: bool,
    ret_count: usize,
}

fn scan_used_capabilities(module: &Module, _detected: &DetectedProfile) -> UsedCapabilities {
    let mut used = UsedCapabilities {
        has_int_instructions: false,
        has_switch: false,
        has_phi: false,
        has_ir_functions: false,
        ret_count: 0,
    };

    let ep_idx = inspect::find_entry_point(module);

    // CR-03: Check for non-entry-point defined functions.
    for (i, func) in module.functions.iter().enumerate() {
        if !func.is_declaration && Some(i) != ep_idx {
            used.has_ir_functions = true;
        }
    }

    // Scan instructions in all defined functions.
    for func in &module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &func.basic_blocks {
            for instr in &bb.instructions {
                match instr {
                    Instruction::BinOp { op, .. } => {
                        if is_int_binop(op) {
                            used.has_int_instructions = true;
                        }
                    }
                    Instruction::ICmp { .. } | Instruction::Select { .. } => {
                        used.has_int_instructions = true;
                    }
                    Instruction::Cast { op, .. } => match op {
                        CastKind::Zext | CastKind::Sext | CastKind::Trunc => {
                            used.has_int_instructions = true;
                        }
                        CastKind::Sitofp | CastKind::Fptosi => {
                            used.has_int_instructions = true;
                        }
                        _ => {}
                    },
                    Instruction::Phi { .. } => {
                        used.has_phi = true;
                    }
                    Instruction::Switch { .. } => {
                        used.has_switch = true;
                    }
                    Instruction::Ret(_) => {
                        used.ret_count += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    used
}
