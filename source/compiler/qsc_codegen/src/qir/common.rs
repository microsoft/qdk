// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc_data_structures::{attrs::Attributes, target::TargetCapabilityFlags};
use qsc_rir::{
    rir::{self, ConditionCode, FcmpConditionCode},
    utils::get_all_block_successors,
};

use qsc_llvm::model::Type;
use qsc_llvm::model::{
    Attribute, AttributeGroup, BasicBlock, BinOpKind, CastKind, Constant, FloatPredicate, Function,
    GlobalVariable, Instruction, IntPredicate, Linkage, MetadataNode, MetadataValue, Module,
    NamedMetadata, Operand, Param,
};
use qsc_llvm::qir::{self, QirProfile};

/// Whether to use typed pointers (`%Qubit*`, `i8*`) or opaque pointers (`ptr`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointerStyle {
    Typed,  // v1: `%Qubit*`, `%Result*`, `i8*`
    Opaque, // v2: `ptr`
}

/// Build a complete `Module` from a RIR program for any QIR profile.
pub fn build_qir_module(program: &rir::Program, profile: QirProfile) -> Module {
    let style = if profile.uses_typed_pointers() {
        PointerStyle::Typed
    } else {
        PointerStyle::Opaque
    };

    let globals = build_globals(program);
    let functions = build_functions(program, style);
    let attribute_groups = build_attribute_groups(program, profile.profile_name());
    let (named_metadata, metadata_nodes) = build_metadata(program, profile);

    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: profile.struct_types(),
        globals,
        functions,
        attribute_groups,
        named_metadata,
        metadata_nodes,
    }
}

pub fn rir_ty(ty: rir::Ty, style: PointerStyle) -> Type {
    match ty {
        rir::Ty::Boolean => Type::Integer(1),
        rir::Ty::Double => Type::Double,
        rir::Ty::Integer => Type::Integer(64),
        rir::Ty::Pointer => match style {
            PointerStyle::Typed => Type::TypedPtr(Box::new(Type::Integer(8))),
            PointerStyle::Opaque => Type::Ptr,
        },
        rir::Ty::Qubit => match style {
            PointerStyle::Typed => Type::NamedPtr(qir::QUBIT_TYPE_NAME.into()),
            PointerStyle::Opaque => Type::Ptr,
        },
        rir::Ty::Result => match style {
            PointerStyle::Typed => Type::NamedPtr(qir::RESULT_TYPE_NAME.into()),
            PointerStyle::Opaque => Type::Ptr,
        },
    }
}

pub fn rir_output_ty(ty: Option<rir::Ty>, style: PointerStyle) -> Option<Type> {
    ty.map(|t| rir_ty(t, style))
}

fn var_name(id: rir::VariableId) -> String {
    format!("var_{}", id.0)
}

fn block_name(id: rir::BlockId) -> String {
    format!("block_{}", id.0)
}

/// Convert an RIR operand to an untyped `llvm_ir::Operand` (for use within binary ops, etc.).
fn operand_untyped(op: &rir::Operand, style: PointerStyle) -> Operand {
    match op {
        rir::Operand::Literal(lit) => literal_untyped(lit, style),
        rir::Operand::Variable(var) => Operand::LocalRef(var_name(var.variable_id)),
    }
}

fn literal_untyped(lit: &rir::Literal, style: PointerStyle) -> Operand {
    match lit {
        rir::Literal::Bool(b) => Operand::IntConst(Type::Integer(1), i64::from(*b)),
        rir::Literal::Double(d) => Operand::float_const(Type::Double, *d),
        rir::Literal::Integer(i) => Operand::IntConst(Type::Integer(64), *i),
        rir::Literal::Pointer => Operand::NullPtr,
        rir::Literal::Qubit(q) => {
            let q_i64 = i64::from(*q);
            match style {
                PointerStyle::Typed => {
                    Operand::IntToPtr(q_i64, Type::NamedPtr(qir::QUBIT_TYPE_NAME.into()))
                }
                PointerStyle::Opaque => Operand::IntToPtr(q_i64, Type::Ptr),
            }
        }
        rir::Literal::Result(r) => {
            let r_i64 = i64::from(*r);
            match style {
                PointerStyle::Typed => {
                    Operand::IntToPtr(r_i64, Type::NamedPtr(qir::RESULT_TYPE_NAME.into()))
                }
                PointerStyle::Opaque => Operand::IntToPtr(r_i64, Type::Ptr),
            }
        }
        rir::Literal::Tag(idx, len) => {
            let idx_i64 = i64::try_from(*idx).expect("tag index should fit in i64");
            let array_len = u64::try_from(*len).expect("tag length should fit in u64") + 1; // +1 for null terminator
            match style {
                PointerStyle::Typed => {
                    let arr_ty = Type::Array(array_len, Box::new(Type::Integer(8)));
                    Operand::GetElementPtr {
                        ty: arr_ty.clone(),
                        ptr: idx_i64.to_string(),
                        ptr_ty: Type::TypedPtr(Box::new(arr_ty)),
                        indices: vec![
                            Operand::IntConst(Type::Integer(64), 0),
                            Operand::IntConst(Type::Integer(64), 0),
                        ],
                    }
                }
                PointerStyle::Opaque => Operand::GlobalRef(idx_i64.to_string()),
            }
        }
    }
}

/// Convert an RIR operand to a (Type, Operand) pair for use as a call argument.
fn call_arg(op: &rir::Operand, style: PointerStyle) -> (Type, Operand) {
    match op {
        rir::Operand::Literal(lit) => call_arg_literal(lit, style),
        rir::Operand::Variable(var) => {
            let ty = rir_ty(var.ty, style);
            (ty, Operand::LocalRef(var_name(var.variable_id)))
        }
    }
}

fn call_arg_literal(lit: &rir::Literal, style: PointerStyle) -> (Type, Operand) {
    match lit {
        rir::Literal::Bool(b) => (
            Type::Integer(1),
            Operand::IntConst(Type::Integer(1), i64::from(*b)),
        ),
        rir::Literal::Double(d) => (Type::Double, Operand::float_const(Type::Double, *d)),
        rir::Literal::Integer(i) => (Type::Integer(64), Operand::IntConst(Type::Integer(64), *i)),
        rir::Literal::Pointer => match style {
            PointerStyle::Typed => (Type::TypedPtr(Box::new(Type::Integer(8))), Operand::NullPtr),
            PointerStyle::Opaque => (Type::Ptr, Operand::NullPtr),
        },
        rir::Literal::Qubit(q) => {
            let q_i64 = i64::from(*q);
            match style {
                PointerStyle::Typed => {
                    let ty = Type::NamedPtr(qir::QUBIT_TYPE_NAME.into());
                    (ty.clone(), Operand::IntToPtr(q_i64, ty))
                }
                PointerStyle::Opaque => (Type::Ptr, Operand::IntToPtr(q_i64, Type::Ptr)),
            }
        }
        rir::Literal::Result(r) => {
            let r_i64 = i64::from(*r);
            match style {
                PointerStyle::Typed => {
                    let ty = Type::NamedPtr(qir::RESULT_TYPE_NAME.into());
                    (ty.clone(), Operand::IntToPtr(r_i64, ty))
                }
                PointerStyle::Opaque => (Type::Ptr, Operand::IntToPtr(r_i64, Type::Ptr)),
            }
        }
        rir::Literal::Tag(idx, len) => {
            let idx_i64 = i64::try_from(*idx).expect("tag index should fit in i64");
            let array_len = u64::try_from(*len).expect("tag length should fit in u64") + 1;
            match style {
                PointerStyle::Typed => {
                    let arr_ty = Type::Array(array_len, Box::new(Type::Integer(8)));
                    (
                        Type::TypedPtr(Box::new(Type::Integer(8))),
                        Operand::GetElementPtr {
                            ty: arr_ty.clone(),
                            ptr: idx_i64.to_string(),
                            ptr_ty: Type::TypedPtr(Box::new(arr_ty)),
                            indices: vec![
                                Operand::IntConst(Type::Integer(64), 0),
                                Operand::IntConst(Type::Integer(64), 0),
                            ],
                        },
                    )
                }
                PointerStyle::Opaque => (Type::Ptr, Operand::GlobalRef(idx_i64.to_string())),
            }
        }
    }
}

fn operand_ir_ty(op: &rir::Operand, style: PointerStyle) -> Type {
    match op {
        rir::Operand::Literal(lit) => match lit {
            rir::Literal::Bool(_) => Type::Integer(1),
            rir::Literal::Double(_) => Type::Double,
            rir::Literal::Integer(_) => Type::Integer(64),
            rir::Literal::Pointer
            | rir::Literal::Qubit(_)
            | rir::Literal::Result(_)
            | rir::Literal::Tag(..) => match style {
                PointerStyle::Typed => match lit {
                    rir::Literal::Qubit(_) => Type::NamedPtr(qir::QUBIT_TYPE_NAME.into()),
                    rir::Literal::Result(_) => Type::NamedPtr(qir::RESULT_TYPE_NAME.into()),
                    _ => Type::TypedPtr(Box::new(Type::Integer(8))),
                },
                PointerStyle::Opaque => Type::Ptr,
            },
        },
        rir::Operand::Variable(var) => rir_ty(var.ty, style),
    }
}

fn convert_binop(
    op: BinOpKind,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    var: rir::Variable,
    style: PointerStyle,
) -> Instruction {
    Instruction::BinOp {
        op,
        ty: rir_ty(var.ty, style),
        lhs: operand_untyped(lhs, style),
        rhs: operand_untyped(rhs, style),
        result: var_name(var.variable_id),
    }
}

#[allow(clippy::too_many_lines)]
pub fn convert_instruction(
    instr: &rir::Instruction,
    program: &rir::Program,
    style: PointerStyle,
) -> Instruction {
    match instr {
        rir::Instruction::Add(lhs, rhs, var) => {
            convert_binop(BinOpKind::Add, lhs, rhs, *var, style)
        }
        rir::Instruction::Ashr(lhs, rhs, var) => {
            convert_binop(BinOpKind::Ashr, lhs, rhs, *var, style)
        }
        rir::Instruction::BitwiseAnd(lhs, rhs, var)
        | rir::Instruction::LogicalAnd(lhs, rhs, var) => {
            convert_binop(BinOpKind::And, lhs, rhs, *var, style)
        }
        rir::Instruction::BitwiseNot(value, var) => Instruction::BinOp {
            op: BinOpKind::Xor,
            ty: rir_ty(var.ty, style),
            lhs: operand_untyped(value, style),
            rhs: Operand::IntConst(Type::Integer(64), -1),
            result: var_name(var.variable_id),
        },
        rir::Instruction::BitwiseOr(lhs, rhs, var) | rir::Instruction::LogicalOr(lhs, rhs, var) => {
            convert_binop(BinOpKind::Or, lhs, rhs, *var, style)
        }
        rir::Instruction::BitwiseXor(lhs, rhs, var) => {
            convert_binop(BinOpKind::Xor, lhs, rhs, *var, style)
        }
        rir::Instruction::Branch(cond, true_id, false_id, _) => Instruction::Br {
            cond_ty: rir_ty(cond.ty, style),
            cond: Operand::LocalRef(var_name(cond.variable_id)),
            true_dest: block_name(*true_id),
            false_dest: block_name(*false_id),
        },
        rir::Instruction::Call(call_id, args, output, _) => {
            let callable = program.get_callable(*call_id);
            Instruction::Call {
                return_ty: rir_output_ty(callable.output_type, style),
                callee: callable.name.clone(),
                args: args.iter().map(|a| call_arg(a, style)).collect(),
                result: output.map(|v| var_name(v.variable_id)),
                attr_refs: vec![],
            }
        }
        rir::Instruction::Convert(operand, var) => {
            let from_ty = operand_ir_ty(operand, style);
            let to_ty = rir_ty(var.ty, style);
            let cast_op = match (&from_ty, &to_ty) {
                (Type::Integer(64), Type::Double) => CastKind::Sitofp,
                (Type::Double, Type::Integer(64)) => CastKind::Fptosi,
                _ => panic!("unsupported conversion from {from_ty} to {to_ty}"),
            };
            Instruction::Cast {
                op: cast_op,
                from_ty,
                to_ty,
                value: operand_untyped(operand, style),
                result: var_name(var.variable_id),
            }
        }
        rir::Instruction::Fadd(lhs, rhs, var) => {
            convert_binop(BinOpKind::Fadd, lhs, rhs, *var, style)
        }
        rir::Instruction::Fdiv(lhs, rhs, var) => {
            convert_binop(BinOpKind::Fdiv, lhs, rhs, *var, style)
        }
        rir::Instruction::Fmul(lhs, rhs, var) => {
            convert_binop(BinOpKind::Fmul, lhs, rhs, *var, style)
        }
        rir::Instruction::Fsub(lhs, rhs, var) => {
            convert_binop(BinOpKind::Fsub, lhs, rhs, *var, style)
        }
        rir::Instruction::Fcmp(op, lhs, rhs, var) => Instruction::FCmp {
            pred: convert_fcmp(*op),
            ty: operand_ir_ty(lhs, style),
            lhs: operand_untyped(lhs, style),
            rhs: operand_untyped(rhs, style),
            result: var_name(var.variable_id),
        },
        rir::Instruction::Icmp(op, lhs, rhs, var) => Instruction::ICmp {
            pred: convert_icmp(*op),
            ty: operand_ir_ty(lhs, style),
            lhs: operand_untyped(lhs, style),
            rhs: operand_untyped(rhs, style),
            result: var_name(var.variable_id),
        },
        rir::Instruction::Jump(block_id) => Instruction::Jump {
            dest: block_name(*block_id),
        },
        rir::Instruction::LogicalNot(value, var) => Instruction::BinOp {
            op: BinOpKind::Xor,
            ty: Type::Integer(1),
            lhs: operand_untyped(value, style),
            rhs: Operand::IntConst(Type::Integer(1), 1),
            result: var_name(var.variable_id),
        },
        rir::Instruction::Mul(lhs, rhs, var) => {
            convert_binop(BinOpKind::Mul, lhs, rhs, *var, style)
        }
        rir::Instruction::Phi(args, var) => Instruction::Phi {
            ty: rir_ty(var.ty, style),
            incoming: args
                .iter()
                .map(|(op, bid)| (operand_untyped(op, style), block_name(*bid)))
                .collect(),
            result: var_name(var.variable_id),
        },
        rir::Instruction::Return => Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
        rir::Instruction::Sdiv(lhs, rhs, var) => {
            convert_binop(BinOpKind::Sdiv, lhs, rhs, *var, style)
        }
        rir::Instruction::Shl(lhs, rhs, var) => {
            convert_binop(BinOpKind::Shl, lhs, rhs, *var, style)
        }
        rir::Instruction::Srem(lhs, rhs, var) => {
            convert_binop(BinOpKind::Srem, lhs, rhs, *var, style)
        }
        rir::Instruction::Store(operand, variable) => Instruction::Store {
            ty: operand_ir_ty(operand, style),
            value: operand_untyped(operand, style),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef(var_name(variable.variable_id)),
        },
        rir::Instruction::Sub(lhs, rhs, var) => {
            convert_binop(BinOpKind::Sub, lhs, rhs, *var, style)
        }
        rir::Instruction::Alloca(var) => Instruction::Alloca {
            ty: rir_ty(var.ty, style),
            result: var_name(var.variable_id),
        },
        rir::Instruction::Load(var_from, var_to) => Instruction::Load {
            ty: rir_ty(var_to.ty, style),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef(var_name(var_from.variable_id)),
            result: var_name(var_to.variable_id),
        },
    }
}

fn convert_icmp(op: ConditionCode) -> IntPredicate {
    match op {
        ConditionCode::Eq => IntPredicate::Eq,
        ConditionCode::Ne => IntPredicate::Ne,
        ConditionCode::Sgt => IntPredicate::Sgt,
        ConditionCode::Sge => IntPredicate::Sge,
        ConditionCode::Slt => IntPredicate::Slt,
        ConditionCode::Sle => IntPredicate::Sle,
    }
}

fn convert_fcmp(op: FcmpConditionCode) -> FloatPredicate {
    match op {
        FcmpConditionCode::False | FcmpConditionCode::True => {
            panic!("unsupported fcmp predicate: {op}")
        }
        FcmpConditionCode::OrderedAndEqual => FloatPredicate::Oeq,
        FcmpConditionCode::OrderedAndGreaterThan => FloatPredicate::Ogt,
        FcmpConditionCode::OrderedAndGreaterThanOrEqual => FloatPredicate::Oge,
        FcmpConditionCode::OrderedAndLessThan => FloatPredicate::Olt,
        FcmpConditionCode::OrderedAndLessThanOrEqual => FloatPredicate::Ole,
        FcmpConditionCode::OrderedAndNotEqual => FloatPredicate::One,
        FcmpConditionCode::Ordered => FloatPredicate::Ord,
        FcmpConditionCode::UnorderedOrEqual => FloatPredicate::Ueq,
        FcmpConditionCode::UnorderedOrGreaterThan => FloatPredicate::Ugt,
        FcmpConditionCode::UnorderedOrGreaterThanOrEqual => FloatPredicate::Uge,
        FcmpConditionCode::UnorderedOrLessThan => FloatPredicate::Ult,
        FcmpConditionCode::UnorderedOrLessThanOrEqual => FloatPredicate::Ule,
        FcmpConditionCode::UnorderedOrNotEqual => FloatPredicate::Une,
        FcmpConditionCode::Unordered => FloatPredicate::Uno,
    }
}

pub fn build_globals(program: &rir::Program) -> Vec<GlobalVariable> {
    program
        .tags
        .iter()
        .enumerate()
        .map(|(idx, tag)| {
            let array_len = u64::try_from(tag.len() + 1).expect("tag length should fit in u64");
            GlobalVariable {
                name: idx.to_string(),
                ty: Type::Array(array_len, Box::new(Type::Integer(8))),
                linkage: Linkage::Internal,
                is_constant: true,
                initializer: Some(Constant::CString(tag.clone())),
            }
        })
        .collect()
}

pub fn build_functions(program: &rir::Program, style: PointerStyle) -> Vec<Function> {
    let mut declarations = Vec::new();
    let mut definitions = Vec::new();

    for (_, callable) in program.callables.iter() {
        if callable.body.is_some() {
            definitions.push(build_definition(callable, program, style));
        } else {
            declarations.push(build_declaration(callable, style));
        }
    }

    // Definitions first, then declarations (matching the original QIR generator ordering)
    definitions.extend(declarations);
    definitions
}

fn build_declaration(callable: &rir::Callable, style: PointerStyle) -> Function {
    let attr_refs = match callable.call_type {
        rir::CallableType::Measurement | rir::CallableType::Reset => vec![1],
        rir::CallableType::NoiseIntrinsic => vec![2],
        _ => vec![],
    };

    Function {
        name: callable.name.clone(),
        return_type: rir_output_ty(callable.output_type, style).unwrap_or(Type::Void),
        params: callable
            .input_type
            .iter()
            .map(|t| Param {
                ty: rir_ty(*t, style),
                name: None,
            })
            .collect(),
        is_declaration: true,
        attribute_group_refs: attr_refs,
        basic_blocks: vec![],
    }
}

fn build_definition(
    callable: &rir::Callable,
    program: &rir::Program,
    style: PointerStyle,
) -> Function {
    let entry_id = callable.body.expect("definition should have a body");

    let mut all_blocks = vec![entry_id];
    all_blocks.extend(get_all_block_successors(entry_id, program));

    let basic_blocks = all_blocks
        .iter()
        .map(|&bid| {
            let block = program.get_block(bid);
            BasicBlock {
                name: block_name(bid),
                instructions: block
                    .0
                    .iter()
                    .map(|i| convert_instruction(i, program, style))
                    .collect(),
            }
        })
        .collect();

    Function {
        name: qir::ENTRYPOINT_NAME.into(),
        return_type: Type::Integer(64),
        params: vec![],
        is_declaration: false,
        attribute_group_refs: vec![0],
        basic_blocks,
    }
}

pub fn build_attribute_groups(program: &rir::Program, profile: &str) -> Vec<AttributeGroup> {
    let mut groups = vec![
        AttributeGroup {
            id: 0,
            attributes: vec![
                Attribute::StringAttr(qir::ENTRY_POINT_ATTR.into()),
                Attribute::StringAttr(qir::OUTPUT_LABELING_SCHEMA_ATTR.into()),
                Attribute::KeyValue(qir::QIR_PROFILES_ATTR.into(), profile.into()),
                Attribute::KeyValue(
                    qir::REQUIRED_NUM_QUBITS_ATTR.into(),
                    program.num_qubits.to_string(),
                ),
                Attribute::KeyValue(
                    qir::REQUIRED_NUM_RESULTS_ATTR.into(),
                    program.num_results.to_string(),
                ),
            ],
        },
        AttributeGroup {
            id: 1,
            attributes: vec![Attribute::StringAttr(qir::IRREVERSIBLE_ATTR.into())],
        },
    ];

    if program.attrs.contains(Attributes::QdkNoise) {
        groups.push(AttributeGroup {
            id: 2,
            attributes: vec![Attribute::StringAttr(qir::QDK_NOISE_ATTR.into())],
        });
    }

    groups
}

/// Build module flags metadata for any QIR profile.
///
/// The metadata encodes:
/// - `qir_major_version` / `qir_minor_version` (from `profile`)
/// - `dynamic_qubit_management` / `dynamic_result_management` (always false for now)
/// - Optional capability flags (int computations, float computations, backwards branching, arrays)
///   based on `profile` and `program.config.capabilities`.
#[allow(clippy::too_many_lines)]
pub fn build_metadata(
    program: &rir::Program,
    profile: QirProfile,
) -> (Vec<NamedMetadata>, Vec<MetadataNode>) {
    let mut nodes = vec![
        MetadataNode {
            id: 0,
            values: vec![
                MetadataValue::Int(Type::Integer(32), 1),
                MetadataValue::String(qir::QIR_MAJOR_VERSION_KEY.into()),
                MetadataValue::Int(Type::Integer(32), profile.major_version()),
            ],
        },
        MetadataNode {
            id: 1,
            values: vec![
                MetadataValue::Int(Type::Integer(32), 7),
                MetadataValue::String(qir::QIR_MINOR_VERSION_KEY.into()),
                MetadataValue::Int(Type::Integer(32), profile.minor_version()),
            ],
        },
        MetadataNode {
            id: 2,
            values: vec![
                MetadataValue::Int(Type::Integer(32), 1),
                MetadataValue::String(qir::DYNAMIC_QUBIT_MGMT_KEY.into()),
                MetadataValue::Int(Type::Integer(1), 0),
            ],
        },
        MetadataNode {
            id: 3,
            values: vec![
                MetadataValue::Int(Type::Integer(32), 1),
                MetadataValue::String(qir::DYNAMIC_RESULT_MGMT_KEY.into()),
                MetadataValue::Int(Type::Integer(1), 0),
            ],
        },
    ];

    let mut next_id: u32 = 4;

    // For v1 profiles, capabilities come from the program config.
    // For v2/v2.1 profiles, all capabilities are always emitted.
    match profile {
        QirProfile::BaseV1 => {
            // Base profile: no capability metadata beyond version + dynamic mgmt.
        }
        QirProfile::AdaptiveV1 => {
            // Adaptive v1: emit capabilities from the program config.
            for cap in program.config.capabilities.iter() {
                match cap {
                    TargetCapabilityFlags::IntegerComputations => {
                        nodes.push(MetadataNode {
                            id: next_id,
                            values: vec![
                                MetadataValue::Int(Type::Integer(32), 5),
                                MetadataValue::String(qir::INT_COMPUTATIONS_KEY.into()),
                                MetadataValue::SubList(vec![MetadataValue::String("i64".into())]),
                            ],
                        });
                        next_id += 1;
                    }
                    TargetCapabilityFlags::FloatingPointComputations => {
                        nodes.push(MetadataNode {
                            id: next_id,
                            values: vec![
                                MetadataValue::Int(Type::Integer(32), 5),
                                MetadataValue::String(qir::FLOAT_COMPUTATIONS_KEY.into()),
                                MetadataValue::SubList(vec![MetadataValue::String(
                                    "double".into(),
                                )]),
                            ],
                        });
                        next_id += 1;
                    }
                    _ => {}
                }
            }
        }
        QirProfile::AdaptiveV2 => {
            // Adaptive v2/v2.1: always emit all capability metadata.
            nodes.push(MetadataNode {
                id: next_id,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 5),
                    MetadataValue::String(qir::INT_COMPUTATIONS_KEY.into()),
                    MetadataValue::SubList(vec![MetadataValue::String("i64".into())]),
                ],
            });
            next_id += 1;
            nodes.push(MetadataNode {
                id: next_id,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 5),
                    MetadataValue::String(qir::FLOAT_COMPUTATIONS_KEY.into()),
                    MetadataValue::SubList(vec![MetadataValue::String("double".into())]),
                ],
            });
            next_id += 1;
            nodes.push(MetadataNode {
                id: next_id,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 7),
                    MetadataValue::String(qir::BACKWARDS_BRANCHING_KEY.into()),
                    MetadataValue::Int(Type::Integer(2), 3),
                ],
            });
            next_id += 1;
            nodes.push(MetadataNode {
                id: next_id,
                values: vec![
                    MetadataValue::Int(Type::Integer(32), 1),
                    MetadataValue::String(qir::ARRAYS_KEY.into()),
                    MetadataValue::Int(Type::Integer(1), 1),
                ],
            });
            next_id += 1;
        }
    }

    let node_refs: Vec<u32> = (0..next_id).collect();
    let named = vec![NamedMetadata {
        name: qir::MODULE_FLAGS_NAME.into(),
        node_refs,
    }];

    (named, nodes)
}
