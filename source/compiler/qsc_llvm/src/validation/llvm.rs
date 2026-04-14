// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::model::Type;
use crate::model::{BinOpKind, CastKind, Function, Instruction, MetadataValue, Module, Operand};
use miette::Diagnostic;
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error, PartialEq, Eq)]
pub enum LlvmIrError {
    // Structure (3 variants)
    #[error("function `{function}` is not a declaration but has no basic blocks")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.MissingBasicBlocks"))]
    MissingBasicBlocks { function: String },

    #[error("declaration `{function}` should not have basic blocks")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.DeclarationHasBlocks"))]
    DeclarationHasBlocks { function: String },

    #[error("basic block `{block}` in function `{function}` is empty")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.EmptyBasicBlock"))]
    EmptyBasicBlock { function: String, block: String },

    // Terminators (2 variants)
    #[error("basic block `{block}` in function `{function}` does not end with a terminator")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.MissingTerminator"))]
    MissingTerminator { function: String, block: String },

    #[error(
        "terminator at index {instr_idx} in block `{block}` of function `{function}` is not the last instruction"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.MidBlockTerminator"))]
    MidBlockTerminator {
        function: String,
        block: String,
        instr_idx: usize,
    },

    // Type consistency (7 variants)
    #[error("{instruction}: type mismatch — expected `{expected}`, found `{found}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.TypeMismatch"))]
    TypeMismatch {
        instruction: String,
        expected: String,
        found: String,
        location: String,
    },

    #[error(
        "branch condition in block `{block}` of function `{function}` is `{found_type}`, expected `i1`"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.BrCondNotI1"))]
    BrCondNotI1 {
        function: String,
        block: String,
        found_type: String,
    },

    #[error(
        "select condition in block `{block}` of function `{function}` is `{found_type}`, expected `i1`"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.SelectCondNotI1"))]
    SelectCondNotI1 {
        function: String,
        block: String,
        found_type: String,
    },

    #[error(
        "return type mismatch in function `{function}` — expected `{expected}`, found `{found}`"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.RetTypeMismatch"))]
    RetTypeMismatch {
        function: String,
        expected: String,
        found: String,
    },

    #[error("{instruction}: integer operation on non-integer type `{ty}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.IntOpOnNonInt"))]
    IntOpOnNonInt {
        instruction: String,
        ty: String,
        location: String,
    },

    #[error("{instruction}: floating-point operation on non-float type `{ty}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.FloatOpOnNonFloat"))]
    FloatOpOnNonFloat {
        instruction: String,
        ty: String,
        location: String,
    },

    #[error("{instruction}: expected pointer type, found `{found}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PtrExpected"))]
    PtrExpected {
        instruction: String,
        found: String,
        location: String,
    },

    // Switch (2 variants)
    #[error(
        "switch in block `{block}` of function `{function}` declares non-integer type `{found_type}`"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.SwitchTypeNotInteger"))]
    SwitchTypeNotInteger {
        function: String,
        block: String,
        found_type: String,
    },

    #[error(
        "switch in block `{block}` of function `{function}` has duplicate case value `{case_value}`"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.SwitchDuplicateCaseValue"))]
    SwitchDuplicateCaseValue {
        function: String,
        block: String,
        case_value: i64,
    },

    // References (4 variants)
    #[error("undefined local reference `%{name}` in block `{block}` of function `{function}`")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.UndefinedLocalRef"))]
    UndefinedLocalRef {
        name: String,
        function: String,
        block: String,
    },

    #[error("undefined global reference `@{name}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.UndefinedGlobalRef"))]
    UndefinedGlobalRef { name: String, location: String },

    #[error("undefined callee `@{name}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.UndefinedCallee"))]
    UndefinedCallee { name: String, location: String },

    #[error(
        "branch target `{target}` does not exist in function `{function}` (from block `{block}`)"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.InvalidBranchTarget"))]
    InvalidBranchTarget {
        target: String,
        function: String,
        block: String,
    },

    // SSA (1 variant)
    #[error("duplicate definition of `%{name}` in function `{function}`")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.DuplicateDefinition"))]
    DuplicateDefinition { name: String, function: String },

    // Cast (1 variant)
    #[error("invalid cast `{cast_kind}` from `{from_ty}` to `{to_ty}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.InvalidCast"))]
    InvalidCast {
        cast_kind: String,
        from_ty: String,
        to_ty: String,
        location: String,
    },

    // Call (2 variants)
    #[error("call to `{callee}`: expected {expected} arguments, found {found} in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.ArgCountMismatch"))]
    ArgCountMismatch {
        callee: String,
        expected: usize,
        found: usize,
        location: String,
    },

    #[error(
        "call to `{callee}`: argument {param_idx} type mismatch — expected `{expected}`, found `{found}` in {location}"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.ArgTypeMismatch"))]
    ArgTypeMismatch {
        callee: String,
        param_idx: usize,
        expected: String,
        found: String,
        location: String,
    },

    // PHI (6 variants)
    #[error(
        "PHI `%{result}` in block `{block}` of function `{function}` is not at the start of the block"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PhiNotAtBlockStart"))]
    PhiNotAtBlockStart {
        function: String,
        block: String,
        result: String,
    },

    #[error("PHI `%{result}` in block `{block}` of function `{function}` has void type")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PhiVoidType"))]
    PhiVoidType {
        function: String,
        block: String,
        result: String,
    },

    #[error("PHI `%{result}` in entry block of function `{function}`")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PhiInEntryBlock"))]
    PhiInEntryBlock { function: String, result: String },

    #[error(
        "PHI `%{result}` in block `{block}` of function `{function}`: expected {expected} incoming entries, found {found}"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PhiPredCountMismatch"))]
    PhiPredCountMismatch {
        function: String,
        block: String,
        result: String,
        expected: usize,
        found: usize,
    },

    #[error(
        "PHI `%{result}` in block `{block}` of function `{function}`: incoming block `{incoming_block}` is not a predecessor"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PhiIncomingNotPredecessor"))]
    PhiIncomingNotPredecessor {
        function: String,
        block: String,
        result: String,
        incoming_block: String,
    },

    #[error(
        "PHI `%{result}` in block `{block}` of function `{function}`: duplicate incoming block `{dup_block}` with different values"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.PhiDuplicateBlockDiffValue"))]
    PhiDuplicateBlockDiffValue {
        function: String,
        block: String,
        result: String,
        dup_block: String,
    },

    // Alloca (1 variant)
    #[error(
        "alloca `%{result}` in block `{block}` of function `{function}` uses unsized type `{ty}`"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.AllocaUnsizedType"))]
    AllocaUnsizedType {
        function: String,
        block: String,
        result: String,
        ty: String,
    },

    // GEP (2 variants)
    #[error("GEP in block `{block}` of function `{function}` has no indices")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.GepNoIndices"))]
    GepNoIndices { function: String, block: String },

    #[error("{instruction}: unsized pointee type `{pointee_ty}` in {location}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.UnsizedPointeeType"))]
    UnsizedPointeeType {
        instruction: String,
        pointee_ty: String,
        location: String,
    },

    // Typed pointer consistency (1 variant)
    #[error(
        "{instruction}: typed pointer inner type `{ptr_inner_ty}` does not match expected type `{expected_ty}` in {location}"
    )]
    #[diagnostic(code("Qsc.Llvm.IrValidator.TypedPtrMismatch"))]
    TypedPtrMismatch {
        instruction: String,
        ptr_inner_ty: String,
        expected_ty: String,
        location: String,
    },

    // Dominance (1 variant)
    #[error(
        "use of `%{name}` in block `{use_block}` of function `{function}` is not dominated by its definition in block `{def_block}`"
    )]
    #[diagnostic(
        code("Qsc.Llvm.IrValidator.UseNotDominatedByDef"),
        help("ensure the definition of `%{name}` dominates all its uses")
    )]
    UseNotDominatedByDef {
        name: String,
        def_block: String,
        use_block: String,
        function: String,
    },

    // Attribute group integrity (2 variants)
    #[error("duplicate attribute group ID #{id}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.DuplicateAttributeGroupId"))]
    DuplicateAttributeGroupId { id: u32 },

    #[error("function `{function}` references undefined attribute group #{ref_id}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.InvalidAttributeGroupRef"))]
    InvalidAttributeGroupRef { function: String, ref_id: u32 },

    // Metadata integrity (3 variants)
    #[error("duplicate metadata node ID !{id}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.DuplicateMetadataNodeId"))]
    DuplicateMetadataNodeId { id: u32 },

    #[error("undefined metadata node reference !{ref_id} in {context}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.InvalidMetadataNodeRef"))]
    InvalidMetadataNodeRef { context: String, ref_id: u32 },

    #[error("metadata reference cycle detected at node !{node_id}")]
    #[diagnostic(code("Qsc.Llvm.IrValidator.MetadataRefCycle"))]
    MetadataRefCycle { node_id: u32 },
}

type CfgMap<'a> = FxHashMap<&'a str, Vec<&'a str>>;

/// Validates general LLVM IR structural correctness.
/// Returns a list of all errors found. An empty list means the module is well-formed.
#[must_use]
pub fn validate_ir(module: &Module) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();

    errors.extend(validate_attribute_groups(module));
    errors.extend(validate_metadata(module));

    for func in &module.functions {
        errors.extend(validate_function_structure(func));
    }

    for func in module.functions.iter().filter(|f| !f.is_declaration) {
        errors.extend(validate_terminators(func));

        let (ssa_env, ssa_errors) = build_ssa_env(func);
        errors.extend(ssa_errors);
        errors.extend(validate_intra_block_ordering(func));

        let (successors, predecessors) = build_cfg(func);

        errors.extend(validate_references(func, &ssa_env, module));
        errors.extend(validate_types(func, &ssa_env, module));
        errors.extend(validate_casts(func, &ssa_env));
        errors.extend(validate_switches(func, &ssa_env));
        errors.extend(validate_phis(func, &predecessors, &ssa_env));
        errors.extend(validate_allocas(func, module));
        errors.extend(validate_gep(func, &ssa_env));

        if !func.basic_blocks.is_empty() {
            let rpo = reverse_postorder(&func.basic_blocks[0].name, &successors);
            let idom = compute_dominators(&func.basic_blocks[0].name, &rpo, &predecessors);
            errors.extend(validate_dominance(func, &ssa_env, &idom));
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn is_terminator(instr: &Instruction) -> bool {
    matches!(
        instr,
        Instruction::Ret(_)
            | Instruction::Br { .. }
            | Instruction::Jump { .. }
            | Instruction::Switch { .. }
            | Instruction::Unreachable
    )
}

fn instruction_result(instr: &Instruction) -> Option<(String, Type)> {
    match instr {
        Instruction::BinOp { result, ty, .. }
        | Instruction::Phi { result, ty, .. }
        | Instruction::Load { result, ty, .. }
        | Instruction::Select { result, ty, .. } => Some((result.clone(), ty.clone())),
        Instruction::ICmp { result, .. } | Instruction::FCmp { result, .. } => {
            Some((result.clone(), Type::Integer(1)))
        }
        Instruction::Cast { result, to_ty, .. } => Some((result.clone(), to_ty.clone())),
        Instruction::Call {
            result: Some(r),
            return_ty: Some(ty),
            ..
        } => Some((r.clone(), ty.clone())),
        Instruction::Alloca { result, .. } | Instruction::GetElementPtr { result, .. } => {
            Some((result.clone(), Type::Ptr))
        }
        _ => None,
    }
}

fn instruction_operands(instr: &Instruction) -> Vec<&Operand> {
    match instr {
        Instruction::Ret(Some(op)) => vec![op],
        Instruction::Ret(None)
        | Instruction::Unreachable
        | Instruction::Jump { .. }
        | Instruction::Alloca { .. } => vec![],
        Instruction::Br { cond, .. } => vec![cond],
        Instruction::BinOp { lhs, rhs, .. }
        | Instruction::ICmp { lhs, rhs, .. }
        | Instruction::FCmp { lhs, rhs, .. } => vec![lhs, rhs],
        Instruction::Cast { value, .. } | Instruction::Switch { value, .. } => vec![value],
        Instruction::Call { args, .. } => args.iter().map(|(_, op)| op).collect(),
        Instruction::Phi { incoming, .. } => incoming.iter().map(|(op, _)| op).collect(),
        Instruction::Load { ptr, .. } => vec![ptr],
        Instruction::Store { value, ptr, .. } => vec![value, ptr],
        Instruction::Select {
            cond,
            true_val,
            false_val,
            ..
        } => vec![cond, true_val, false_val],
        Instruction::GetElementPtr { ptr, indices, .. } => {
            let mut ops = vec![ptr];
            ops.extend(indices.iter());
            ops
        }
    }
}

fn is_ptr_type(ty: &Type) -> bool {
    matches!(ty, Type::Ptr | Type::NamedPtr(_) | Type::TypedPtr(_))
}

fn bit_width(ty: &Type) -> Option<u32> {
    match ty {
        Type::Integer(n) => Some(*n),
        Type::Half => Some(16),
        Type::Float => Some(32),
        Type::Double => Some(64),
        _ => None,
    }
}

fn types_equivalent(a: &Type, b: &Type) -> bool {
    a == b
}

fn is_sized_alloca_type(ty: &Type, module: &Module) -> bool {
    match ty {
        Type::Void | Type::Label | Type::Function(..) => false,
        Type::Named(name) => !module
            .struct_types
            .iter()
            .any(|struct_ty| struct_ty.name == *name && struct_ty.is_opaque),
        _ => true,
    }
}

fn local_ref_name(operand: &Operand) -> Option<&str> {
    match operand {
        Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => Some(name.as_str()),
        _ => None,
    }
}

fn operand_types_compatible(actual: &Type, expected: &Type) -> bool {
    types_equivalent(actual, expected)
        || (is_ptr_type(actual)
            && is_ptr_type(expected)
            && (matches!(actual, Type::Ptr) || matches!(expected, Type::Ptr)))
}

// ---------------------------------------------------------------------------
// SSA environment builder
// ---------------------------------------------------------------------------

fn build_ssa_env(func: &Function) -> (FxHashMap<String, Type>, Vec<LlvmIrError>) {
    let mut env: FxHashMap<String, Type> = FxHashMap::default();
    let mut errors = Vec::new();

    for (i, param) in func.params.iter().enumerate() {
        let name = param.name.clone().unwrap_or_else(|| i.to_string());
        env.insert(name, param.ty.clone());
    }

    for bb in &func.basic_blocks {
        for instr in &bb.instructions {
            if let Some((name, ty)) = instruction_result(instr) {
                if env.contains_key(&name) {
                    errors.push(LlvmIrError::DuplicateDefinition {
                        name: name.clone(),
                        function: func.name.clone(),
                    });
                }
                env.insert(name, ty);
            }
        }
    }

    (env, errors)
}

// ---------------------------------------------------------------------------
// CFG builder
// ---------------------------------------------------------------------------

fn build_cfg<'a>(func: &'a Function) -> (CfgMap<'a>, CfgMap<'a>) {
    let mut successors: CfgMap<'a> = FxHashMap::default();
    let mut predecessors: CfgMap<'a> = FxHashMap::default();

    for bb in &func.basic_blocks {
        successors.entry(bb.name.as_str()).or_default();
        predecessors.entry(bb.name.as_str()).or_default();
    }

    for bb in &func.basic_blocks {
        let targets: Vec<&str> = match bb.instructions.last() {
            Some(Instruction::Br {
                true_dest,
                false_dest,
                ..
            }) => vec![true_dest.as_str(), false_dest.as_str()],
            Some(Instruction::Jump { dest }) => vec![dest.as_str()],
            Some(Instruction::Switch {
                default_dest,
                cases,
                ..
            }) => {
                let mut t = vec![default_dest.as_str()];
                t.extend(cases.iter().map(|(_, d)| d.as_str()));
                t
            }
            _ => vec![],
        };
        for target in &targets {
            successors.entry(bb.name.as_str()).or_default().push(target);
            predecessors
                .entry(target)
                .or_default()
                .push(bb.name.as_str());
        }
    }

    (successors, predecessors)
}

// ---------------------------------------------------------------------------
// Operand type resolution
// ---------------------------------------------------------------------------

fn resolve_operand_type(operand: &Operand, locals: &FxHashMap<String, Type>) -> Option<Type> {
    match operand {
        Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => locals.get(name).cloned(),
        Operand::IntConst(ty, _) | Operand::IntToPtr(_, ty) | Operand::FloatConst(ty, _) => {
            Some(ty.clone())
        }
        Operand::NullPtr | Operand::GetElementPtr { .. } | Operand::GlobalRef(_) => Some(Type::Ptr),
    }
}

// ---------------------------------------------------------------------------
// Validation passes
// ---------------------------------------------------------------------------

fn validate_function_structure(func: &Function) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    if func.is_declaration {
        if !func.basic_blocks.is_empty() {
            errors.push(LlvmIrError::DeclarationHasBlocks {
                function: func.name.clone(),
            });
        }
    } else if func.basic_blocks.is_empty() {
        errors.push(LlvmIrError::MissingBasicBlocks {
            function: func.name.clone(),
        });
    }
    errors
}

fn validate_terminators(func: &Function) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    for bb in &func.basic_blocks {
        if bb.instructions.is_empty() {
            errors.push(LlvmIrError::EmptyBasicBlock {
                function: func.name.clone(),
                block: bb.name.clone(),
            });
            continue;
        }
        if !is_terminator(bb.instructions.last().expect("non-empty")) {
            errors.push(LlvmIrError::MissingTerminator {
                function: func.name.clone(),
                block: bb.name.clone(),
            });
        }
        for (idx, instr) in bb.instructions.iter().enumerate() {
            if idx < bb.instructions.len() - 1 && is_terminator(instr) {
                errors.push(LlvmIrError::MidBlockTerminator {
                    function: func.name.clone(),
                    block: bb.name.clone(),
                    instr_idx: idx,
                });
            }
        }
    }
    errors
}

fn validate_intra_block_ordering(func: &Function) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    for bb in &func.basic_blocks {
        let block_defs: FxHashSet<String> = bb
            .instructions
            .iter()
            .filter_map(|i| instruction_result(i).map(|(name, _)| name))
            .collect();

        let mut defined_so_far: FxHashSet<String> = FxHashSet::default();
        for instr in &bb.instructions {
            if matches!(instr, Instruction::Phi { .. }) {
                if let Some((name, _)) = instruction_result(instr) {
                    defined_so_far.insert(name);
                }
                continue;
            }

            for op in instruction_operands(instr) {
                if let Some(name) = local_ref_name(op)
                    && block_defs.contains(name)
                    && !defined_so_far.contains(name)
                {
                    errors.push(LlvmIrError::UndefinedLocalRef {
                        name: name.to_string(),
                        function: func.name.clone(),
                        block: bb.name.clone(),
                    });
                }
            }
            if let Some((name, _)) = instruction_result(instr) {
                defined_so_far.insert(name);
            }
        }
    }
    errors
}

fn validate_references(
    func: &Function,
    ssa_env: &FxHashMap<String, Type>,
    module: &Module,
) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    let block_names: FxHashSet<&str> = func
        .basic_blocks
        .iter()
        .map(|bb| bb.name.as_str())
        .collect();
    let func_names: FxHashSet<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    let global_names: FxHashSet<&str> = module.globals.iter().map(|g| g.name.as_str()).collect();

    for bb in &func.basic_blocks {
        let location = format!("function `{}`, block `{}`", func.name, bb.name);

        for instr in &bb.instructions {
            for operand in instruction_operands(instr) {
                match operand {
                    Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => {
                        if !ssa_env.contains_key(name) {
                            errors.push(LlvmIrError::UndefinedLocalRef {
                                name: name.clone(),
                                function: func.name.clone(),
                                block: bb.name.clone(),
                            });
                        }
                    }
                    Operand::GlobalRef(name) => {
                        if !global_names.contains(name.as_str())
                            && !func_names.contains(name.as_str())
                        {
                            errors.push(LlvmIrError::UndefinedGlobalRef {
                                name: name.clone(),
                                location: location.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }

            match instr {
                Instruction::Br {
                    true_dest,
                    false_dest,
                    ..
                } => {
                    for target in [true_dest, false_dest] {
                        if !block_names.contains(target.as_str()) {
                            errors.push(LlvmIrError::InvalidBranchTarget {
                                target: target.clone(),
                                function: func.name.clone(),
                                block: bb.name.clone(),
                            });
                        }
                    }
                }
                Instruction::Jump { dest } => {
                    if !block_names.contains(dest.as_str()) {
                        errors.push(LlvmIrError::InvalidBranchTarget {
                            target: dest.clone(),
                            function: func.name.clone(),
                            block: bb.name.clone(),
                        });
                    }
                }
                Instruction::Switch {
                    default_dest,
                    cases,
                    ..
                } => {
                    if !block_names.contains(default_dest.as_str()) {
                        errors.push(LlvmIrError::InvalidBranchTarget {
                            target: default_dest.clone(),
                            function: func.name.clone(),
                            block: bb.name.clone(),
                        });
                    }
                    for (_, dest) in cases {
                        if !block_names.contains(dest.as_str()) {
                            errors.push(LlvmIrError::InvalidBranchTarget {
                                target: dest.clone(),
                                function: func.name.clone(),
                                block: bb.name.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }

            if let Instruction::Call { callee, .. } = instr
                && !func_names.contains(callee.as_str())
            {
                errors.push(LlvmIrError::UndefinedCallee {
                    name: callee.clone(),
                    location: location.clone(),
                });
            }
        }
    }
    errors
}

fn is_int_binop(op: &BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Add
            | BinOpKind::Sub
            | BinOpKind::Mul
            | BinOpKind::Sdiv
            | BinOpKind::Srem
            | BinOpKind::Shl
            | BinOpKind::Ashr
            | BinOpKind::And
            | BinOpKind::Or
            | BinOpKind::Xor
            | BinOpKind::Udiv
            | BinOpKind::Urem
            | BinOpKind::Lshr
    )
}

#[allow(clippy::too_many_lines)]
fn validate_types(
    func: &Function,
    ssa_env: &FxHashMap<String, Type>,
    module: &Module,
) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();

    for bb in &func.basic_blocks {
        let location = format!("function `{}`, block `{}`", func.name, bb.name);

        for instr in &bb.instructions {
            match instr {
                Instruction::BinOp {
                    op, ty, lhs, rhs, ..
                } => {
                    let instr_name = format!("{op:?}");
                    if is_int_binop(op) {
                        if !matches!(ty, Type::Integer(_)) {
                            errors.push(LlvmIrError::IntOpOnNonInt {
                                instruction: instr_name.clone(),
                                ty: ty.to_string(),
                                location: location.clone(),
                            });
                        }
                    } else if !ty.is_floating_point() {
                        errors.push(LlvmIrError::FloatOpOnNonFloat {
                            instruction: instr_name.clone(),
                            ty: ty.to_string(),
                            location: location.clone(),
                        });
                    }
                    for (side, operand) in [("lhs", lhs), ("rhs", rhs)] {
                        if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                            && !types_equivalent(&resolved, ty)
                        {
                            errors.push(LlvmIrError::TypeMismatch {
                                instruction: instr_name.clone(),
                                expected: ty.to_string(),
                                found: resolved.to_string(),
                                location: format!("{location}, {side}"),
                            });
                        }
                    }
                }

                Instruction::ICmp { ty, lhs, rhs, .. } => {
                    if !matches!(ty, Type::Integer(_)) && !is_ptr_type(ty) {
                        errors.push(LlvmIrError::TypeMismatch {
                            instruction: "ICmp".to_string(),
                            expected: "integer or pointer type".to_string(),
                            found: ty.to_string(),
                            location: location.clone(),
                        });
                    }
                    for (side, operand) in [("lhs", lhs), ("rhs", rhs)] {
                        if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                            && !types_equivalent(&resolved, ty)
                        {
                            errors.push(LlvmIrError::TypeMismatch {
                                instruction: "ICmp".to_string(),
                                expected: ty.to_string(),
                                found: resolved.to_string(),
                                location: format!("{location}, {side}"),
                            });
                        }
                    }
                }

                Instruction::FCmp { ty, lhs, rhs, .. } => {
                    if !ty.is_floating_point() {
                        errors.push(LlvmIrError::FloatOpOnNonFloat {
                            instruction: "FCmp".to_string(),
                            ty: ty.to_string(),
                            location: location.clone(),
                        });
                    }
                    for (side, operand) in [("lhs", lhs), ("rhs", rhs)] {
                        if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                            && !types_equivalent(&resolved, ty)
                        {
                            errors.push(LlvmIrError::TypeMismatch {
                                instruction: "FCmp".to_string(),
                                expected: ty.to_string(),
                                found: resolved.to_string(),
                                location: format!("{location}, {side}"),
                            });
                        }
                    }
                }

                Instruction::Br { cond_ty, cond, .. } => {
                    if *cond_ty != Type::Integer(1) {
                        errors.push(LlvmIrError::BrCondNotI1 {
                            function: func.name.clone(),
                            block: bb.name.clone(),
                            found_type: cond_ty.to_string(),
                        });
                    }
                    if let Some(resolved) = resolve_operand_type(cond, ssa_env)
                        && resolved != Type::Integer(1)
                    {
                        errors.push(LlvmIrError::BrCondNotI1 {
                            function: func.name.clone(),
                            block: bb.name.clone(),
                            found_type: resolved.to_string(),
                        });
                    }
                }

                Instruction::Select {
                    cond,
                    true_val,
                    false_val,
                    ty,
                    ..
                } => {
                    if let Some(cond_ty) = resolve_operand_type(cond, ssa_env)
                        && cond_ty != Type::Integer(1)
                    {
                        errors.push(LlvmIrError::SelectCondNotI1 {
                            function: func.name.clone(),
                            block: bb.name.clone(),
                            found_type: cond_ty.to_string(),
                        });
                    }
                    for (side, operand) in [("true_val", true_val), ("false_val", false_val)] {
                        if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                            && !types_equivalent(&resolved, ty)
                        {
                            errors.push(LlvmIrError::TypeMismatch {
                                instruction: "Select".to_string(),
                                expected: ty.to_string(),
                                found: resolved.to_string(),
                                location: format!("{location}, {side}"),
                            });
                        }
                    }
                }

                Instruction::Ret(Some(operand)) => {
                    if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                        && !types_equivalent(&resolved, &func.return_type)
                    {
                        errors.push(LlvmIrError::RetTypeMismatch {
                            function: func.name.clone(),
                            expected: func.return_type.to_string(),
                            found: resolved.to_string(),
                        });
                    }
                }
                Instruction::Ret(None) => {
                    if func.return_type != Type::Void {
                        errors.push(LlvmIrError::RetTypeMismatch {
                            function: func.name.clone(),
                            expected: func.return_type.to_string(),
                            found: "void".to_string(),
                        });
                    }
                }

                Instruction::Store {
                    ty,
                    value,
                    ptr_ty,
                    ptr,
                } => {
                    if !is_ptr_type(ptr_ty) {
                        errors.push(LlvmIrError::PtrExpected {
                            instruction: "Store".to_string(),
                            found: ptr_ty.to_string(),
                            location: location.clone(),
                        });
                    }
                    if let Some(resolved) = resolve_operand_type(value, ssa_env)
                        && !types_equivalent(&resolved, ty)
                    {
                        errors.push(LlvmIrError::TypeMismatch {
                            instruction: "Store".to_string(),
                            expected: ty.to_string(),
                            found: resolved.to_string(),
                            location: location.clone(),
                        });
                    }
                    if is_ptr_type(ptr_ty)
                        && let Some(resolved_ptr_ty) = resolve_operand_type(ptr, ssa_env)
                        && !operand_types_compatible(&resolved_ptr_ty, ptr_ty)
                    {
                        errors.push(LlvmIrError::TypeMismatch {
                            instruction: "Store".to_string(),
                            expected: ptr_ty.to_string(),
                            found: resolved_ptr_ty.to_string(),
                            location: format!("{location}, ptr"),
                        });
                    }
                    if let Type::TypedPtr(inner) = ptr_ty
                        && !types_equivalent(inner, ty)
                    {
                        errors.push(LlvmIrError::TypedPtrMismatch {
                            instruction: "Store".to_string(),
                            ptr_inner_ty: inner.to_string(),
                            expected_ty: ty.to_string(),
                            location: location.clone(),
                        });
                    }
                }

                Instruction::Load {
                    ty, ptr_ty, ptr, ..
                } => {
                    if !is_ptr_type(ptr_ty) {
                        errors.push(LlvmIrError::PtrExpected {
                            instruction: "Load".to_string(),
                            found: ptr_ty.to_string(),
                            location: location.clone(),
                        });
                    }
                    if is_ptr_type(ptr_ty)
                        && let Some(resolved_ptr_ty) = resolve_operand_type(ptr, ssa_env)
                        && !operand_types_compatible(&resolved_ptr_ty, ptr_ty)
                    {
                        errors.push(LlvmIrError::TypeMismatch {
                            instruction: "Load".to_string(),
                            expected: ptr_ty.to_string(),
                            found: resolved_ptr_ty.to_string(),
                            location: format!("{location}, ptr"),
                        });
                    }
                    if let Type::TypedPtr(inner) = ptr_ty
                        && !types_equivalent(inner, ty)
                    {
                        errors.push(LlvmIrError::TypedPtrMismatch {
                            instruction: "Load".to_string(),
                            ptr_inner_ty: inner.to_string(),
                            expected_ty: ty.to_string(),
                            location: location.clone(),
                        });
                    }
                }

                Instruction::Call {
                    callee,
                    args,
                    return_ty,
                    ..
                } => {
                    if let Some(target_func) = module.functions.iter().find(|f| f.name == *callee) {
                        match (return_ty, &target_func.return_type) {
                            (None, Type::Void) => {}
                            (Some(found), expected) if types_equivalent(found, expected) => {}
                            (Some(found), expected) => {
                                errors.push(LlvmIrError::TypeMismatch {
                                    instruction: format!("Call @{callee}"),
                                    expected: expected.to_string(),
                                    found: found.to_string(),
                                    location: format!("{location}, return type"),
                                });
                            }
                            (None, expected) => {
                                errors.push(LlvmIrError::TypeMismatch {
                                    instruction: format!("Call @{callee}"),
                                    expected: expected.to_string(),
                                    found: "void".to_string(),
                                    location: format!("{location}, return type"),
                                });
                            }
                        }

                        if args.len() == target_func.params.len() {
                            for (i, ((arg_ty, operand), param)) in
                                args.iter().zip(&target_func.params).enumerate()
                            {
                                if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                                    && !operand_types_compatible(&resolved, arg_ty)
                                {
                                    errors.push(LlvmIrError::TypeMismatch {
                                        instruction: format!("Call @{callee}"),
                                        expected: arg_ty.to_string(),
                                        found: resolved.to_string(),
                                        location: format!("{location}, arg {i}"),
                                    });
                                }
                                if !types_equivalent(arg_ty, &param.ty) {
                                    errors.push(LlvmIrError::ArgTypeMismatch {
                                        callee: callee.clone(),
                                        param_idx: i,
                                        expected: param.ty.to_string(),
                                        found: arg_ty.to_string(),
                                        location: location.clone(),
                                    });
                                }
                            }
                        } else {
                            errors.push(LlvmIrError::ArgCountMismatch {
                                callee: callee.clone(),
                                expected: target_func.params.len(),
                                found: args.len(),
                                location: location.clone(),
                            });
                        }
                    }
                }

                _ => {}
            }
        }
    }
    errors
}

fn is_valid_cast(kind: &CastKind, from: &Type, to: &Type) -> bool {
    match kind {
        CastKind::Zext | CastKind::Sext => matches!(
            (from, to),
            (Type::Integer(fw), Type::Integer(tw)) if fw < tw
        ),
        CastKind::Trunc => matches!(
            (from, to),
            (Type::Integer(fw), Type::Integer(tw)) if fw > tw
        ),
        CastKind::Sitofp => matches!(from, Type::Integer(_)) && to.is_floating_point(),
        CastKind::Fptosi => from.is_floating_point() && matches!(to, Type::Integer(_)),
        CastKind::FpExt => matches!(
            (from.floating_point_bit_width(), to.floating_point_bit_width()),
            (Some(from_width), Some(to_width)) if from_width < to_width
        ),
        CastKind::FpTrunc => matches!(
            (from.floating_point_bit_width(), to.floating_point_bit_width()),
            (Some(from_width), Some(to_width)) if from_width > to_width
        ),
        CastKind::IntToPtr => matches!(from, Type::Integer(_)) && is_ptr_type(to),
        CastKind::PtrToInt => is_ptr_type(from) && matches!(to, Type::Integer(_)),
        CastKind::Bitcast => match (is_ptr_type(from), is_ptr_type(to)) {
            (true, true) => true,
            (false, false) => bit_width(from) == bit_width(to) && bit_width(from).is_some(),
            _ => false,
        },
    }
}

fn validate_casts(func: &Function, ssa_env: &FxHashMap<String, Type>) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    for bb in &func.basic_blocks {
        let location = format!("function `{}`, block `{}`", func.name, bb.name);
        for instr in &bb.instructions {
            if let Instruction::Cast {
                op,
                from_ty,
                to_ty,
                value,
                ..
            } = instr
            {
                if let Some(resolved) = resolve_operand_type(value, ssa_env)
                    && !types_equivalent(&resolved, from_ty)
                {
                    errors.push(LlvmIrError::TypeMismatch {
                        instruction: "Cast".to_string(),
                        expected: from_ty.to_string(),
                        found: resolved.to_string(),
                        location: location.clone(),
                    });
                }
                if !is_valid_cast(op, from_ty, to_ty) {
                    errors.push(LlvmIrError::InvalidCast {
                        cast_kind: format!("{op:?}"),
                        from_ty: from_ty.to_string(),
                        to_ty: to_ty.to_string(),
                        location: location.clone(),
                    });
                }
            }
        }
    }
    errors
}

fn validate_switches(func: &Function, ssa_env: &FxHashMap<String, Type>) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();

    for bb in &func.basic_blocks {
        let location = format!("function `{}`, block `{}`", func.name, bb.name);

        for instr in &bb.instructions {
            if let Instruction::Switch {
                ty, value, cases, ..
            } = instr
            {
                if !matches!(ty, Type::Integer(_)) {
                    errors.push(LlvmIrError::SwitchTypeNotInteger {
                        function: func.name.clone(),
                        block: bb.name.clone(),
                        found_type: ty.to_string(),
                    });
                }

                if let Some(resolved) = resolve_operand_type(value, ssa_env)
                    && !types_equivalent(&resolved, ty)
                {
                    errors.push(LlvmIrError::TypeMismatch {
                        instruction: "Switch".to_string(),
                        expected: ty.to_string(),
                        found: resolved.to_string(),
                        location: location.clone(),
                    });
                }

                let mut seen_case_values = FxHashSet::default();
                for (case_value, _) in cases {
                    if !seen_case_values.insert(*case_value) {
                        errors.push(LlvmIrError::SwitchDuplicateCaseValue {
                            function: func.name.clone(),
                            block: bb.name.clone(),
                            case_value: *case_value,
                        });
                    }
                }
            }
        }
    }

    errors
}

fn validate_phis(
    func: &Function,
    predecessors: &CfgMap<'_>,
    ssa_env: &FxHashMap<String, Type>,
) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();

    for (bb_idx, bb) in func.basic_blocks.iter().enumerate() {
        let mut seen_non_phi = false;

        for instr in &bb.instructions {
            if let Instruction::Phi {
                ty,
                incoming,
                result,
            } = instr
            {
                // Rule 7: No PHI in entry block
                if bb_idx == 0 {
                    errors.push(LlvmIrError::PhiInEntryBlock {
                        function: func.name.clone(),
                        result: result.clone(),
                    });
                }

                // Rule 1: PHIs must be grouped at start of block
                if seen_non_phi {
                    errors.push(LlvmIrError::PhiNotAtBlockStart {
                        function: func.name.clone(),
                        block: bb.name.clone(),
                        result: result.clone(),
                    });
                }

                // Rule 2: PHI type must not be Void
                if *ty == Type::Void {
                    errors.push(LlvmIrError::PhiVoidType {
                        function: func.name.clone(),
                        block: bb.name.clone(),
                        result: result.clone(),
                    });
                }

                // Rule 3: Incoming value types must match PHI type
                for (operand, _label) in incoming {
                    if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                        && !types_equivalent(&resolved, ty)
                    {
                        errors.push(LlvmIrError::TypeMismatch {
                            instruction: format!("Phi %{result}"),
                            expected: ty.to_string(),
                            found: resolved.to_string(),
                            location: format!("function `{}`, block `{}`", func.name, bb.name),
                        });
                    }
                }

                // Rule 4: Incoming count == predecessor count
                let preds = predecessors.get(bb.name.as_str());
                let pred_count = preds.map_or(0, Vec::len);
                if incoming.len() != pred_count {
                    errors.push(LlvmIrError::PhiPredCountMismatch {
                        function: func.name.clone(),
                        block: bb.name.clone(),
                        result: result.clone(),
                        expected: pred_count,
                        found: incoming.len(),
                    });
                }

                // Rule 5: Incoming labels must match predecessors as an exact multiset.
                let mut pred_counts: FxHashMap<&str, usize> = FxHashMap::default();
                if let Some(preds) = preds {
                    for &pred in preds {
                        *pred_counts.entry(pred).or_default() += 1;
                    }
                }
                for (_operand, label) in incoming {
                    match pred_counts.get_mut(label.as_str()) {
                        Some(remaining) if *remaining > 0 => *remaining -= 1,
                        _ => {
                            errors.push(LlvmIrError::PhiIncomingNotPredecessor {
                                function: func.name.clone(),
                                block: bb.name.clone(),
                                result: result.clone(),
                                incoming_block: label.clone(),
                            });
                        }
                    }
                }

                // Rule 6: Duplicate incoming blocks must carry identical values
                let mut seen_labels: FxHashMap<&str, &Operand> = FxHashMap::default();
                for (operand, label) in incoming {
                    if let Some(prev_op) = seen_labels.get(label.as_str()) {
                        if *prev_op != operand {
                            errors.push(LlvmIrError::PhiDuplicateBlockDiffValue {
                                function: func.name.clone(),
                                block: bb.name.clone(),
                                result: result.clone(),
                                dup_block: label.clone(),
                            });
                        }
                    } else {
                        seen_labels.insert(label.as_str(), operand);
                    }
                }
            } else {
                seen_non_phi = true;
            }
        }
    }
    errors
}

fn validate_allocas(func: &Function, module: &Module) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();

    for bb in &func.basic_blocks {
        for instr in &bb.instructions {
            if let Instruction::Alloca { ty, result } = instr
                && !is_sized_alloca_type(ty, module)
            {
                errors.push(LlvmIrError::AllocaUnsizedType {
                    function: func.name.clone(),
                    block: bb.name.clone(),
                    result: result.clone(),
                    ty: ty.to_string(),
                });
            }
        }
    }

    errors
}

fn validate_gep(func: &Function, ssa_env: &FxHashMap<String, Type>) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    for bb in &func.basic_blocks {
        let location = format!("function `{}`, block `{}`", func.name, bb.name);
        for instr in &bb.instructions {
            if let Instruction::GetElementPtr {
                pointee_ty,
                ptr_ty,
                indices,
                ..
            } = instr
            {
                if !is_ptr_type(ptr_ty) {
                    errors.push(LlvmIrError::PtrExpected {
                        instruction: "GetElementPtr".to_string(),
                        found: ptr_ty.to_string(),
                        location: location.clone(),
                    });
                }

                if matches!(pointee_ty, Type::Void | Type::Function(..)) {
                    errors.push(LlvmIrError::UnsizedPointeeType {
                        instruction: "GetElementPtr".to_string(),
                        pointee_ty: pointee_ty.to_string(),
                        location: location.clone(),
                    });
                }

                if indices.is_empty() {
                    errors.push(LlvmIrError::GepNoIndices {
                        function: func.name.clone(),
                        block: bb.name.clone(),
                    });
                }

                for (index, operand) in indices.iter().enumerate() {
                    if let Some(resolved) = resolve_operand_type(operand, ssa_env)
                        && !matches!(resolved, Type::Integer(_))
                    {
                        errors.push(LlvmIrError::TypeMismatch {
                            instruction: "GetElementPtr".to_string(),
                            expected: "integer type".to_string(),
                            found: resolved.to_string(),
                            location: format!("{location}, index {index}"),
                        });
                    }
                }

                if let Type::TypedPtr(inner) = ptr_ty
                    && !types_equivalent(inner, pointee_ty)
                {
                    errors.push(LlvmIrError::TypedPtrMismatch {
                        instruction: "GetElementPtr".to_string(),
                        ptr_inner_ty: inner.to_string(),
                        expected_ty: pointee_ty.to_string(),
                        location: location.clone(),
                    });
                }
            }
        }
    }
    errors
}

// ---------------------------------------------------------------------------
// Dominance analysis
// ---------------------------------------------------------------------------

fn reverse_postorder<'a>(
    entry: &'a str,
    successors: &FxHashMap<&'a str, Vec<&'a str>>,
) -> Vec<&'a str> {
    fn dfs<'a>(
        block: &'a str,
        successors: &FxHashMap<&'a str, Vec<&'a str>>,
        visited: &mut FxHashSet<&'a str>,
        postorder: &mut Vec<&'a str>,
    ) {
        if !visited.insert(block) {
            return;
        }
        if let Some(succs) = successors.get(block) {
            for &s in succs {
                dfs(s, successors, visited, postorder);
            }
        }
        postorder.push(block);
    }

    let mut visited = FxHashSet::default();
    let mut postorder = Vec::new();
    dfs(entry, successors, &mut visited, &mut postorder);
    postorder.reverse();
    postorder
}

fn compute_dominators<'a>(
    entry: &'a str,
    rpo: &[&'a str],
    predecessors: &FxHashMap<&'a str, Vec<&'a str>>,
) -> FxHashMap<&'a str, &'a str> {
    let rpo_number: FxHashMap<&str, usize> = rpo.iter().enumerate().map(|(i, &b)| (b, i)).collect();
    let mut idom: FxHashMap<&str, &str> = FxHashMap::default();

    let mut changed = true;
    while changed {
        changed = false;
        for &block in rpo {
            if block == entry {
                continue;
            }
            let Some(preds) = predecessors.get(block) else {
                continue;
            };
            let mut new_idom = None;
            for &pred in preds {
                if pred == entry || idom.contains_key(pred) {
                    new_idom = Some(pred);
                    break;
                }
            }
            let Some(mut new_idom_val) = new_idom else {
                continue;
            };
            for &pred in preds {
                if pred == new_idom_val {
                    continue;
                }
                if pred == entry || idom.contains_key(pred) {
                    new_idom_val = intersect(pred, new_idom_val, &idom, &rpo_number, entry);
                }
            }
            if idom.get(block) != Some(&new_idom_val) {
                idom.insert(block, new_idom_val);
                changed = true;
            }
        }
    }
    idom
}

fn intersect<'a>(
    mut b1: &'a str,
    mut b2: &'a str,
    idom: &FxHashMap<&'a str, &'a str>,
    rpo_number: &FxHashMap<&str, usize>,
    entry: &'a str,
) -> &'a str {
    while b1 != b2 {
        while rpo_number.get(b1).copied().unwrap_or(0) > rpo_number.get(b2).copied().unwrap_or(0) {
            b1 = if b1 == entry {
                entry
            } else {
                idom.get(b1).copied().unwrap_or(entry)
            };
        }
        while rpo_number.get(b2).copied().unwrap_or(0) > rpo_number.get(b1).copied().unwrap_or(0) {
            b2 = if b2 == entry {
                entry
            } else {
                idom.get(b2).copied().unwrap_or(entry)
            };
        }
    }
    b1
}

fn dominates(def: &str, use_block: &str, idom: &FxHashMap<&str, &str>, entry: &str) -> bool {
    if def == use_block {
        return true;
    }
    let mut current = use_block;
    while current != entry {
        current = match idom.get(current) {
            Some(&dom) => dom,
            None => return false,
        };
        if current == def {
            return true;
        }
    }
    def == entry
}

fn validate_dominance(
    func: &Function,
    ssa_env: &FxHashMap<String, Type>,
    idom: &FxHashMap<&str, &str>,
) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    let entry = func.basic_blocks[0].name.as_str();

    // Build def_block map: SSA name → block where it's defined
    let mut def_block: FxHashMap<String, String> = FxHashMap::default();
    for (i, param) in func.params.iter().enumerate() {
        let name = param.name.clone().unwrap_or_else(|| i.to_string());
        def_block.insert(name, entry.to_string());
    }
    for bb in &func.basic_blocks {
        for instr in &bb.instructions {
            if let Some((name, _)) = instruction_result(instr) {
                def_block.insert(name, bb.name.clone());
            }
        }
    }

    for bb in &func.basic_blocks {
        for instr in &bb.instructions {
            // PHI incoming values: check that def dominates the incoming block
            if let Instruction::Phi { incoming, .. } = instr {
                for (operand, label) in incoming {
                    let (Operand::LocalRef(name) | Operand::TypedLocalRef(name, _)) = operand
                    else {
                        continue;
                    };
                    if !ssa_env.contains_key(name) {
                        continue;
                    }
                    if let Some(db) = def_block.get(name)
                        && db != label
                        && !dominates(db, label, idom, entry)
                    {
                        errors.push(LlvmIrError::UseNotDominatedByDef {
                            name: name.clone(),
                            def_block: db.clone(),
                            use_block: label.clone(),
                            function: func.name.clone(),
                        });
                    }
                }
                continue;
            }

            // Regular instructions: check that def dominates the current block
            for operand in instruction_operands(instr) {
                let (Operand::LocalRef(name) | Operand::TypedLocalRef(name, _)) = operand else {
                    continue;
                };
                if !ssa_env.contains_key(name) {
                    continue;
                }
                if let Some(db) = def_block.get(name)
                    && db.as_str() != bb.name.as_str()
                    && !dominates(db, &bb.name, idom, entry)
                {
                    errors.push(LlvmIrError::UseNotDominatedByDef {
                        name: name.clone(),
                        def_block: db.clone(),
                        use_block: bb.name.clone(),
                        function: func.name.clone(),
                    });
                }
            }
        }
    }
    errors
}

// ---------------------------------------------------------------------------
// Attribute group validation
// ---------------------------------------------------------------------------

fn validate_attribute_groups(module: &Module) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    let mut seen_ids = FxHashSet::default();
    let mut valid_ids = FxHashSet::default();

    for group in &module.attribute_groups {
        if !seen_ids.insert(group.id) {
            errors.push(LlvmIrError::DuplicateAttributeGroupId { id: group.id });
        }
        valid_ids.insert(group.id);
    }

    for func in &module.functions {
        for &ref_id in &func.attribute_group_refs {
            if !valid_ids.contains(&ref_id) {
                errors.push(LlvmIrError::InvalidAttributeGroupRef {
                    function: func.name.clone(),
                    ref_id,
                });
            }
        }

        for bb in &func.basic_blocks {
            for instr in &bb.instructions {
                if let Instruction::Call { attr_refs, .. } = instr {
                    for &ref_id in attr_refs {
                        if !valid_ids.contains(&ref_id) {
                            errors.push(LlvmIrError::InvalidAttributeGroupRef {
                                function: func.name.clone(),
                                ref_id,
                            });
                        }
                    }
                }
            }
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Metadata validation
// ---------------------------------------------------------------------------

fn extract_node_refs(values: &[MetadataValue]) -> Vec<u32> {
    let mut refs = Vec::new();
    for v in values {
        match v {
            MetadataValue::NodeRef(id) => refs.push(*id),
            MetadataValue::SubList(sub) => refs.extend(extract_node_refs(sub)),
            _ => {}
        }
    }
    refs
}

fn detect_metadata_cycles(
    node_id: u32,
    nodes: &FxHashMap<u32, &[MetadataValue]>,
    visited: &mut FxHashSet<u32>,
    in_stack: &mut FxHashSet<u32>,
    errors: &mut Vec<LlvmIrError>,
) {
    if in_stack.contains(&node_id) {
        errors.push(LlvmIrError::MetadataRefCycle { node_id });
        return;
    }
    if visited.contains(&node_id) {
        return;
    }
    visited.insert(node_id);
    in_stack.insert(node_id);
    if let Some(values) = nodes.get(&node_id) {
        for ref_id in extract_node_refs(values) {
            detect_metadata_cycles(ref_id, nodes, visited, in_stack, errors);
        }
    }
    in_stack.remove(&node_id);
}

fn validate_metadata(module: &Module) -> Vec<LlvmIrError> {
    let mut errors = Vec::new();
    let mut seen_ids = FxHashSet::default();
    let mut valid_ids = FxHashSet::default();
    let mut node_map: FxHashMap<u32, &[MetadataValue]> = FxHashMap::default();

    // ID uniqueness
    for node in &module.metadata_nodes {
        if !seen_ids.insert(node.id) {
            errors.push(LlvmIrError::DuplicateMetadataNodeId { id: node.id });
        }
        valid_ids.insert(node.id);
        node_map.insert(node.id, &node.values);
    }

    // Reference validity: named metadata
    for nm in &module.named_metadata {
        for &ref_id in &nm.node_refs {
            if !valid_ids.contains(&ref_id) {
                errors.push(LlvmIrError::InvalidMetadataNodeRef {
                    context: format!("named metadata `!{}`", nm.name),
                    ref_id,
                });
            }
        }
    }

    // Reference validity: node-to-node references
    for node in &module.metadata_nodes {
        for ref_id in extract_node_refs(&node.values) {
            if !valid_ids.contains(&ref_id) {
                errors.push(LlvmIrError::InvalidMetadataNodeRef {
                    context: format!("metadata node !{}", node.id),
                    ref_id,
                });
            }
        }
    }

    // Cycle detection
    let mut visited = FxHashSet::default();
    let mut in_stack = FxHashSet::default();
    for node in &module.metadata_nodes {
        detect_metadata_cycles(node.id, &node_map, &mut visited, &mut in_stack, &mut errors);
    }

    errors
}
