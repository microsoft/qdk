// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::model::{
    Attribute, BinOpKind, CastKind, Constant, Function, Instruction, MetadataValue, Module,
    ModuleFlagNodeIssue, Operand, Type,
};

use super::spec::ENTRY_POINT_ATTR;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FloatSurfaceAnalysis {
    pub(crate) has_float_op: bool,
    surface_widths: BTreeSet<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModuleFlagIssue {
    DanglingReference {
        node_ref: u32,
    },
    MalformedNode {
        node_ref: u32,
        reason: &'static str,
    },
    InvalidBehavior {
        flag_name: String,
        node_id: u32,
        found: String,
    },
    InvalidValue {
        flag_name: String,
        node_id: u32,
        expected: &'static str,
        found: String,
    },
    InvalidStringListItem {
        flag_name: String,
        node_id: u32,
        index: usize,
        found: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModuleFlagAccess<T> {
    pub(crate) value: Option<T>,
    pub(crate) issues: Vec<ModuleFlagIssue>,
}

impl ModuleFlagIssue {
    #[must_use]
    pub(crate) fn flag_name(&self) -> Option<&str> {
        match self {
            Self::DanglingReference { .. } | Self::MalformedNode { .. } => None,
            Self::InvalidBehavior { flag_name, .. }
            | Self::InvalidValue { flag_name, .. }
            | Self::InvalidStringListItem { flag_name, .. } => Some(flag_name.as_str()),
        }
    }
}

fn describe_metadata_value(value: &MetadataValue) -> String {
    match value {
        MetadataValue::Int(ty, _) => format!("integer ({ty})"),
        MetadataValue::String(_) => "string".to_string(),
        MetadataValue::NodeRef(node_id) => format!("node reference !{node_id}"),
        MetadataValue::SubList(_) => "metadata sublist".to_string(),
    }
}

fn map_module_flag_node_issue(issue: &ModuleFlagNodeIssue) -> ModuleFlagIssue {
    match issue {
        ModuleFlagNodeIssue::DanglingReference { node_ref } => ModuleFlagIssue::DanglingReference {
            node_ref: *node_ref,
        },
        ModuleFlagNodeIssue::MalformedEntry { node_ref, reason } => {
            ModuleFlagIssue::MalformedNode {
                node_ref: *node_ref,
                reason,
            }
        }
    }
}

fn find_module_flag_entry<'a>(
    module: &'a Module,
    key: &str,
) -> Option<crate::model::ModuleFlagNode<'a>> {
    module
        .audit_module_flags()
        .entries
        .into_iter()
        .find(|entry| entry.key == key)
}

impl FloatSurfaceAnalysis {
    fn record_type(&mut self, ty: &Type) {
        match ty {
            Type::TypedPtr(inner) | Type::Array(_, inner) => self.record_type(inner),
            Type::Function(return_type, params) => {
                self.record_type(return_type);
                for param in params {
                    self.record_type(param);
                }
            }
            _ => {
                if let Some(width) = ty.floating_point_bit_width() {
                    self.surface_widths.insert(width);
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn surface_width_names(&self) -> Vec<&'static str> {
        self.surface_widths
            .iter()
            .filter_map(|width| match width {
                16 => Some("half"),
                32 => Some("float"),
                64 => Some("double"),
                _ => None,
            })
            .collect()
    }
}

#[must_use]
pub(crate) fn analyze_float_surface(module: &Module) -> FloatSurfaceAnalysis {
    let mut analysis = FloatSurfaceAnalysis::default();

    for global in &module.globals {
        analysis.record_type(&global.ty);
        if let Some(initializer) = &global.initializer {
            analyze_constant(initializer, &mut analysis);
        }
    }

    for function in &module.functions {
        analyze_function(function, &mut analysis);
    }

    analysis
}

fn analyze_function(function: &Function, analysis: &mut FloatSurfaceAnalysis) {
    analysis.record_type(&function.return_type);
    for param in &function.params {
        analysis.record_type(&param.ty);
    }

    for block in &function.basic_blocks {
        for instruction in &block.instructions {
            analyze_instruction(instruction, analysis);
        }
    }
}

#[allow(clippy::too_many_lines)]
fn analyze_instruction(instruction: &Instruction, analysis: &mut FloatSurfaceAnalysis) {
    analysis.has_float_op |= matches!(
        instruction,
        Instruction::BinOp {
            op: BinOpKind::Fadd | BinOpKind::Fsub | BinOpKind::Fmul | BinOpKind::Fdiv,
            ..
        } | Instruction::FCmp { .. }
            | Instruction::Cast {
                op: CastKind::FpExt | CastKind::FpTrunc | CastKind::Sitofp | CastKind::Fptosi,
                ..
            }
    );

    match instruction {
        Instruction::Ret(value) => {
            if let Some(value) = value {
                analyze_operand(value, analysis);
            }
        }
        Instruction::Br { cond_ty, cond, .. } => {
            analysis.record_type(cond_ty);
            analyze_operand(cond, analysis);
        }
        Instruction::Jump { .. } | Instruction::Unreachable => {}
        Instruction::BinOp { ty, lhs, rhs, .. }
        | Instruction::ICmp { ty, lhs, rhs, .. }
        | Instruction::FCmp { ty, lhs, rhs, .. } => {
            analysis.record_type(ty);
            analyze_operand(lhs, analysis);
            analyze_operand(rhs, analysis);
        }
        Instruction::Cast {
            from_ty,
            to_ty,
            value,
            ..
        } => {
            analysis.record_type(from_ty);
            analysis.record_type(to_ty);
            analyze_operand(value, analysis);
        }
        Instruction::Call {
            return_ty, args, ..
        } => {
            if let Some(return_ty) = return_ty {
                analysis.record_type(return_ty);
            }
            for (ty, operand) in args {
                analysis.record_type(ty);
                analyze_operand(operand, analysis);
            }
        }
        Instruction::Phi { ty, incoming, .. } => {
            analysis.record_type(ty);
            for (operand, _) in incoming {
                analyze_operand(operand, analysis);
            }
        }
        Instruction::Alloca { ty, .. } => analysis.record_type(ty),
        Instruction::Load {
            ty, ptr_ty, ptr, ..
        } => {
            analysis.record_type(ty);
            analysis.record_type(ptr_ty);
            analyze_operand(ptr, analysis);
        }
        Instruction::Store {
            ty,
            value,
            ptr_ty,
            ptr,
        } => {
            analysis.record_type(ty);
            analyze_operand(value, analysis);
            analysis.record_type(ptr_ty);
            analyze_operand(ptr, analysis);
        }
        Instruction::Select {
            cond,
            true_val,
            false_val,
            ty,
            ..
        } => {
            analyze_operand(cond, analysis);
            analyze_operand(true_val, analysis);
            analyze_operand(false_val, analysis);
            analysis.record_type(ty);
        }
        Instruction::Switch { ty, value, .. } => {
            analysis.record_type(ty);
            analyze_operand(value, analysis);
        }
        Instruction::GetElementPtr {
            pointee_ty,
            ptr_ty,
            ptr,
            indices,
            ..
        } => {
            analysis.record_type(pointee_ty);
            analysis.record_type(ptr_ty);
            analyze_operand(ptr, analysis);
            for index in indices {
                analyze_operand(index, analysis);
            }
        }
    }
}

fn analyze_constant(constant: &Constant, analysis: &mut FloatSurfaceAnalysis) {
    if let Constant::Float(ty, _) = constant {
        analysis.record_type(ty);
    }
}

fn analyze_operand(operand: &Operand, analysis: &mut FloatSurfaceAnalysis) {
    match operand {
        Operand::LocalRef(_) | Operand::GlobalRef(_) | Operand::NullPtr => {}
        Operand::TypedLocalRef(_, ty)
        | Operand::IntConst(ty, _)
        | Operand::FloatConst(ty, _)
        | Operand::IntToPtr(_, ty) => analysis.record_type(ty),
        Operand::GetElementPtr {
            ty,
            ptr_ty,
            indices,
            ..
        } => {
            analysis.record_type(ty);
            analysis.record_type(ptr_ty);
            for index in indices {
                analyze_operand(index, analysis);
            }
        }
    }
}

fn function_attributes(module: &Module, func_idx: usize) -> impl Iterator<Item = &Attribute> + '_ {
    module.functions[func_idx]
        .attribute_group_refs
        .iter()
        .filter_map(|&group_ref| module.attribute_groups.iter().find(|ag| ag.id == group_ref))
        .flat_map(|group| group.attributes.iter())
}

fn has_function_string_attribute(module: &Module, func_idx: usize, attr_name: &str) -> bool {
    function_attributes(module, func_idx)
        .any(|attr| matches!(attr, Attribute::StringAttr(name) if name == attr_name))
}

/// Extract the integer ID from an `IntToPtr` or `NullPtr` operand.
/// `NullPtr` is treated as ID 0 (`inttoptr(i64 0)` can normalize to `null`).
#[must_use]
pub fn extract_id(operand: &Operand) -> Option<u32> {
    match operand {
        Operand::IntToPtr(val, _) => u32::try_from(*val).ok(),
        Operand::NullPtr => Some(0),
        _ => None,
    }
}

/// Extract an `f64` value from a `FloatConst` operand.
#[must_use]
pub fn extract_float(operand: &Operand) -> Option<f64> {
    match operand {
        Operand::FloatConst(_, val) => Some(*val),
        _ => None,
    }
}

/// Generate a stable string key for an operand.
#[must_use]
pub fn operand_key(operand: &Operand) -> String {
    format!("{operand:?}")
}

/// Find the entry-point function index in a module.
#[must_use]
pub fn find_entry_point(module: &Module) -> Option<usize> {
    module
        .functions
        .iter()
        .enumerate()
        .find_map(|(func_idx, func)| {
            (!func.is_declaration
                && has_function_string_attribute(module, func_idx, ENTRY_POINT_ATTR))
            .then_some(func_idx)
        })
}

/// Count the number of non-declaration entry-point functions in a module.
#[must_use]
pub(crate) fn count_entry_points(module: &Module) -> usize {
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(func_idx, func)| {
            !func.is_declaration
                && has_function_string_attribute(module, *func_idx, ENTRY_POINT_ATTR)
        })
        .count()
}

/// Extract a key-value attribute from the given function's attribute groups.
#[must_use]
pub fn get_function_attribute<'a>(
    module: &'a Module,
    func_idx: usize,
    key: &str,
) -> Option<&'a str> {
    function_attributes(module, func_idx).find_map(|attr| {
        if let Attribute::KeyValue(attr_key, value) = attr
            && attr_key == key
        {
            Some(value.as_str())
        } else {
            None
        }
    })
}

/// Check whether a function has the given attribute in string or key-value form.
#[must_use]
pub(crate) fn has_function_attribute(module: &Module, func_idx: usize, attr_name: &str) -> bool {
    function_attributes(module, func_idx).any(|attr| match attr {
        Attribute::StringAttr(name) => name == attr_name,
        Attribute::KeyValue(key, _) => key == attr_name,
    })
}

/// Look up a module flag value by key.
#[must_use]
pub(crate) fn get_module_flag<'a>(module: &'a Module, key: &str) -> Option<&'a MetadataValue> {
    module.get_flag(key)
}

#[must_use]
pub(crate) fn inspect_module_flag_metadata(module: &Module) -> Vec<ModuleFlagIssue> {
    module
        .audit_module_flags()
        .issues
        .iter()
        .map(map_module_flag_node_issue)
        .collect()
}

#[must_use]
pub(crate) fn inspect_module_flag_int(module: &Module, key: &str) -> ModuleFlagAccess<i64> {
    let mut access = ModuleFlagAccess::default();

    if let Some(entry) = find_module_flag_entry(module, key) {
        match entry.value {
            MetadataValue::Int(_, value) => access.value = Some(*value),
            other => access.issues.push(ModuleFlagIssue::InvalidValue {
                flag_name: key.to_string(),
                node_id: entry.node_id,
                expected: "integer",
                found: describe_metadata_value(other),
            }),
        }
    }

    access
}

/// Look up an integer module flag value by key.
#[cfg(test)]
#[must_use]
pub(crate) fn get_module_flag_int(module: &Module, key: &str) -> Option<i64> {
    inspect_module_flag_int(module, key).value
}

#[must_use]
pub(crate) fn inspect_module_flag_bool(module: &Module, key: &str) -> ModuleFlagAccess<bool> {
    let mut access = ModuleFlagAccess::default();

    if let Some(entry) = find_module_flag_entry(module, key) {
        match entry.value {
            MetadataValue::Int(_, value) => access.value = Some(*value != 0),
            other => access.issues.push(ModuleFlagIssue::InvalidValue {
                flag_name: key.to_string(),
                node_id: entry.node_id,
                expected: "integer boolean",
                found: describe_metadata_value(other),
            }),
        }
    }

    access
}

/// Look up a boolean module flag value by key.
#[cfg(test)]
#[must_use]
pub(crate) fn get_module_flag_bool(module: &Module, key: &str) -> bool {
    inspect_module_flag_bool(module, key).value.unwrap_or(false)
}

#[must_use]
pub(crate) fn inspect_module_flag_string_list(
    module: &Module,
    key: &str,
) -> ModuleFlagAccess<Vec<String>> {
    let mut access = ModuleFlagAccess::default();

    if let Some(entry) = find_module_flag_entry(module, key) {
        match entry.value {
            MetadataValue::SubList(items) => {
                let mut values = Vec::with_capacity(items.len());
                for (index, value) in items.iter().enumerate() {
                    if let MetadataValue::String(text) = value {
                        values.push(text.clone());
                    } else {
                        access.issues.push(ModuleFlagIssue::InvalidStringListItem {
                            flag_name: key.to_string(),
                            node_id: entry.node_id,
                            index,
                            found: describe_metadata_value(value),
                        });
                    }
                }

                if access.issues.is_empty() {
                    access.value = Some(values);
                }
            }
            other => access.issues.push(ModuleFlagIssue::InvalidValue {
                flag_name: key.to_string(),
                node_id: entry.node_id,
                expected: "metadata string list",
                found: describe_metadata_value(other),
            }),
        }
    }

    access
}

/// Look up a string-list module flag value by key.
#[cfg(test)]
#[must_use]
pub(crate) fn get_module_flag_string_list(module: &Module, key: &str) -> Vec<String> {
    inspect_module_flag_string_list(module, key)
        .value
        .unwrap_or_default()
}

#[must_use]
pub(crate) fn inspect_module_flag_behavior(module: &Module, key: &str) -> ModuleFlagAccess<i64> {
    let mut access = ModuleFlagAccess::default();

    if let Some(entry) = find_module_flag_entry(module, key) {
        match entry.behavior {
            MetadataValue::Int(_, behavior) => access.value = Some(*behavior),
            other => access.issues.push(ModuleFlagIssue::InvalidBehavior {
                flag_name: key.to_string(),
                node_id: entry.node_id,
                found: describe_metadata_value(other),
            }),
        }
    }

    access
}

/// Look up the module-flag merge behavior for a given key.
#[cfg(test)]
#[must_use]
pub(crate) fn get_module_flag_behavior(module: &Module, key: &str) -> Option<i64> {
    inspect_module_flag_behavior(module, key).value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Type;
    use crate::model::{
        AttributeGroup, BasicBlock, FloatPredicate, Function, GlobalVariable, Instruction, Linkage,
        MetadataNode, MetadataValue, Module, NamedMetadata, Operand, Param,
    };
    use crate::qir::spec::MODULE_FLAGS_NAME;

    fn module_with_inspection_data() -> Module {
        Module {
            source_filename: None,
            target_datalayout: None,
            target_triple: None,
            struct_types: Vec::new(),
            globals: Vec::new(),
            functions: vec![
                Function {
                    name: "decl_entry".to_string(),
                    return_type: Type::Void,
                    params: Vec::new(),
                    is_declaration: true,
                    attribute_group_refs: vec![0],
                    basic_blocks: Vec::new(),
                },
                Function {
                    name: "ENTRYPOINT__main".to_string(),
                    return_type: Type::Integer(64),
                    params: Vec::new(),
                    is_declaration: false,
                    attribute_group_refs: vec![1],
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
                    attributes: vec![Attribute::StringAttr(ENTRY_POINT_ATTR.to_string())],
                },
                AttributeGroup {
                    id: 1,
                    attributes: vec![
                        Attribute::StringAttr(ENTRY_POINT_ATTR.to_string()),
                        Attribute::StringAttr("output_labeling_schema".to_string()),
                        Attribute::KeyValue(
                            "qir_profiles".to_string(),
                            "adaptive_profile".to_string(),
                        ),
                    ],
                },
            ],
            named_metadata: vec![NamedMetadata {
                name: MODULE_FLAGS_NAME.to_string(),
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
                        MetadataValue::Int(Type::Integer(1), 1),
                    ],
                },
                MetadataNode {
                    id: 4,
                    values: vec![
                        MetadataValue::Int(Type::Integer(32), 5),
                        MetadataValue::String("int_computations".to_string()),
                        MetadataValue::SubList(vec![
                            MetadataValue::String("i64".to_string()),
                            MetadataValue::String("i32".to_string()),
                        ]),
                    ],
                },
            ],
        }
    }

    #[test]
    fn test_extract_id_from_inttoptr() {
        let op = Operand::int_to_named_ptr(7, "Qubit");
        assert_eq!(extract_id(&op), Some(7));
    }

    #[test]
    fn test_extract_id_from_nullptr() {
        assert_eq!(extract_id(&Operand::NullPtr), Some(0));
    }

    #[test]
    fn test_extract_id_from_other() {
        let op = Operand::float_const(Type::Double, 1.0);
        assert_eq!(extract_id(&op), None);
    }

    #[test]
    fn test_extract_float() {
        let op = Operand::float_const(Type::Double, std::f64::consts::E);
        assert_eq!(extract_float(&op), Some(std::f64::consts::E));

        let op_int = Operand::IntConst(Type::Integer(64), 1);
        assert_eq!(extract_float(&op_int), None);
    }

    #[test]
    fn test_operand_key() {
        let op1 = Operand::int_to_named_ptr(0, "Qubit");
        let op2 = Operand::int_to_named_ptr(1, "Qubit");
        let key1 = operand_key(&op1);
        let key2 = operand_key(&op2);
        assert_ne!(key1, key2);

        let key1b = operand_key(&op1);
        assert_eq!(key1, key1b);
    }

    #[test]
    fn test_find_entry_point_ignores_declarations() {
        let module = module_with_inspection_data();
        assert_eq!(find_entry_point(&module), Some(1));
    }

    #[test]
    fn test_count_entry_points_ignores_declarations() {
        let module = module_with_inspection_data();
        assert_eq!(count_entry_points(&module), 1);
    }

    #[test]
    fn test_get_function_attribute_reads_key_value() {
        let module = module_with_inspection_data();
        assert_eq!(
            get_function_attribute(&module, 1, "qir_profiles"),
            Some("adaptive_profile")
        );
    }

    #[test]
    fn test_has_function_attribute_matches_string_and_key_value() {
        let module = module_with_inspection_data();
        assert!(has_function_attribute(&module, 1, ENTRY_POINT_ATTR));
        assert!(has_function_attribute(&module, 1, "output_labeling_schema"));
        assert!(has_function_attribute(&module, 1, "qir_profiles"));
    }

    #[test]
    fn test_get_module_flag_helpers() {
        let module = module_with_inspection_data();

        assert_eq!(get_module_flag_int(&module, "qir_major_version"), Some(2));
        assert!(!get_module_flag_bool(&module, "dynamic_qubit_management"));
        assert!(get_module_flag_bool(&module, "dynamic_result_management"));
        assert_eq!(
            get_module_flag_string_list(&module, "int_computations"),
            vec!["i64".to_string(), "i32".to_string()]
        );
        assert_eq!(
            get_module_flag_behavior(&module, "qir_minor_version"),
            Some(7)
        );
    }

    #[test]
    fn test_get_module_flag_string_list_reads_float_computations() {
        let mut module = module_with_inspection_data();
        module.named_metadata[0].node_refs.push(5);
        module.metadata_nodes.push(MetadataNode {
            id: 5,
            values: vec![
                MetadataValue::Int(Type::Integer(32), 5),
                MetadataValue::String("float_computations".to_string()),
                MetadataValue::SubList(vec![
                    MetadataValue::String("half".to_string()),
                    MetadataValue::String("double".to_string()),
                ]),
            ],
        });

        assert_eq!(
            get_module_flag_string_list(&module, "float_computations"),
            vec!["half".to_string(), "double".to_string()]
        );
    }

    #[test]
    fn test_inspect_module_flag_metadata_reports_dangling_refs_without_hiding_valid_flags() {
        let mut module = module_with_inspection_data();
        module.named_metadata[0].node_refs.insert(0, 999);

        assert_eq!(
            inspect_module_flag_metadata(&module),
            vec![ModuleFlagIssue::DanglingReference { node_ref: 999 }]
        );
        assert_eq!(
            inspect_module_flag_int(&module, "qir_major_version").value,
            Some(2)
        );
    }

    #[test]
    fn test_inspect_module_flag_access_reports_malformed_payloads() {
        let mut module = module_with_inspection_data();
        module.metadata_nodes[3].values[2] = MetadataValue::String("true".to_string());
        module.metadata_nodes[4].values[2] = MetadataValue::SubList(vec![
            MetadataValue::String("i64".to_string()),
            MetadataValue::Int(Type::Integer(32), 1),
        ]);

        let bool_flag = inspect_module_flag_bool(&module, "dynamic_result_management");
        assert_eq!(bool_flag.value, None);
        assert_eq!(
            bool_flag.issues,
            vec![ModuleFlagIssue::InvalidValue {
                flag_name: "dynamic_result_management".to_string(),
                node_id: 3,
                expected: "integer boolean",
                found: "string".to_string(),
            }]
        );

        let string_list_flag = inspect_module_flag_string_list(&module, "int_computations");
        assert_eq!(string_list_flag.value, None);
        assert_eq!(
            string_list_flag.issues,
            vec![ModuleFlagIssue::InvalidStringListItem {
                flag_name: "int_computations".to_string(),
                node_id: 4,
                index: 1,
                found: "integer (i32)".to_string(),
            }]
        );
    }

    #[test]
    fn test_analyze_float_surface_collects_recursive_widths_and_ops() {
        let module = Module {
            source_filename: None,
            target_datalayout: None,
            target_triple: None,
            struct_types: Vec::new(),
            globals: vec![GlobalVariable {
                name: "g".to_string(),
                ty: Type::Array(1, Box::new(Type::Float)),
                linkage: Linkage::Internal,
                is_constant: false,
                initializer: None,
            }],
            functions: vec![
                Function {
                    name: "decl".to_string(),
                    return_type: Type::TypedPtr(Box::new(Type::Double)),
                    params: vec![Param {
                        ty: Type::Function(Box::new(Type::Void), vec![Type::Half]),
                        name: None,
                    }],
                    is_declaration: true,
                    attribute_group_refs: Vec::new(),
                    basic_blocks: Vec::new(),
                },
                Function {
                    name: "entry".to_string(),
                    return_type: Type::Integer(64),
                    params: Vec::new(),
                    is_declaration: false,
                    attribute_group_refs: Vec::new(),
                    basic_blocks: vec![BasicBlock {
                        name: "entry".to_string(),
                        instructions: vec![
                            Instruction::FCmp {
                                pred: FloatPredicate::Olt,
                                ty: Type::Half,
                                lhs: Operand::TypedLocalRef("lhs".to_string(), Type::Half),
                                rhs: Operand::float_const(Type::Half, 0.0),
                                result: "cond".to_string(),
                            },
                            Instruction::Select {
                                cond: Operand::LocalRef("cond".to_string()),
                                true_val: Operand::TypedLocalRef("then".to_string(), Type::Float),
                                false_val: Operand::TypedLocalRef("else".to_string(), Type::Double),
                                ty: Type::Float,
                                result: "value".to_string(),
                            },
                            Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
                        ],
                    }],
                },
            ],
            attribute_groups: Vec::new(),
            named_metadata: Vec::new(),
            metadata_nodes: Vec::new(),
        };

        let analysis = analyze_float_surface(&module);

        assert!(analysis.has_float_op);
        assert_eq!(
            analysis.surface_width_names(),
            vec!["half", "float", "double"]
        );
    }

    #[test]
    fn test_analyze_float_surface_tracks_declaration_only_widths_without_ops() {
        let module = Module {
            source_filename: None,
            target_datalayout: None,
            target_triple: None,
            struct_types: Vec::new(),
            globals: vec![GlobalVariable {
                name: "g".to_string(),
                ty: Type::Float,
                linkage: Linkage::Internal,
                is_constant: true,
                initializer: Some(Constant::Float(Type::Float, 1.0)),
            }],
            functions: vec![Function {
                name: "decl".to_string(),
                return_type: Type::Double,
                params: vec![Param {
                    ty: Type::TypedPtr(Box::new(Type::Half)),
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

        let analysis = analyze_float_surface(&module);

        assert!(!analysis.has_float_op);
        assert_eq!(
            analysis.surface_width_names(),
            vec!["half", "float", "double"]
        );
    }

    #[test]
    fn test_analyze_float_surface_ignores_metadata_payload_types() {
        let module = Module {
            source_filename: None,
            target_datalayout: None,
            target_triple: None,
            struct_types: Vec::new(),
            globals: Vec::new(),
            functions: Vec::new(),
            attribute_groups: Vec::new(),
            named_metadata: Vec::new(),
            metadata_nodes: vec![MetadataNode {
                id: 0,
                values: vec![MetadataValue::Int(Type::Double, 1)],
            }],
        };

        let analysis = analyze_float_surface(&module);

        assert!(!analysis.has_float_op);
        assert!(analysis.surface_width_names().is_empty());
    }
}
