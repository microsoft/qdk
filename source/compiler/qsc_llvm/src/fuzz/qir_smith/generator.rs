// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use arbitrary::Unstructured;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    model::{
        BasicBlock, BinOpKind, CastKind, Constant, FloatPredicate, Function, GlobalVariable,
        Instruction, IntPredicate, Linkage, Module, Operand, Param, Type,
    },
    qir,
};

use super::{
    config::{BASE_V1_BLOCK_COUNT, EffectiveConfig, QirProfilePreset},
    metadata::{build_qdk_attribute_groups, build_qdk_metadata},
};

const MAX_SHELL_COUNT: usize = 4;
const MAX_OPTIONAL_GLOBALS: usize = 2;
pub(super) const GENERATED_TARGET_DATALAYOUT: &str = "e-p:64:64";
pub(super) const GENERATED_TARGET_TRIPLE: &str = "arm64-apple-macosx15.0.0";
const MODELED_INTEGER_INITIALIZERS: [i64; 4] = [-1, 0, 1, 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShellPreset {
    include_qdk_shell: bool,
    default_include_declarations: bool,
    default_include_globals: bool,
}

impl ShellPreset {
    pub(super) fn from_profile(profile: QirProfilePreset) -> Self {
        match profile {
            QirProfilePreset::BaseV1
            | QirProfilePreset::AdaptiveV1
            | QirProfilePreset::AdaptiveV2 => Self {
                include_qdk_shell: true,
                default_include_declarations: true,
                default_include_globals: true,
            },
            QirProfilePreset::BareRoundtrip => Self {
                include_qdk_shell: false,
                default_include_declarations: false,
                default_include_globals: false,
            },
        }
    }

    fn entry_point_attr_refs(self) -> Vec<u32> {
        if self.include_qdk_shell {
            vec![qir::ENTRY_POINT_ATTR_GROUP_ID]
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct ShellCounts {
    pub(super) required_num_qubits: usize,
    pub(super) required_num_results: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellDeclaration {
    Hadamard,
    ControlledX,
    Measure,
    ArrayRecordOutput,
    ResultRecordOutput,
    ResultArrayRecordOutput,
    TupleRecordOutput,
    BoolRecordOutput,
    IntRecordOutput,
    DoubleRecordOutput,
    QubitAllocate,
    QubitRelease,
    Initialize,
    ReadResult,
}

const SHELL_DECLARATIONS: [ShellDeclaration; 14] = [
    ShellDeclaration::Hadamard,
    ShellDeclaration::ControlledX,
    ShellDeclaration::Measure,
    ShellDeclaration::ArrayRecordOutput,
    ShellDeclaration::ResultRecordOutput,
    ShellDeclaration::ResultArrayRecordOutput,
    ShellDeclaration::TupleRecordOutput,
    ShellDeclaration::BoolRecordOutput,
    ShellDeclaration::IntRecordOutput,
    ShellDeclaration::DoubleRecordOutput,
    ShellDeclaration::QubitAllocate,
    ShellDeclaration::QubitRelease,
    ShellDeclaration::Initialize,
    ShellDeclaration::ReadResult,
];

#[allow(clippy::struct_field_names)]
#[derive(Debug, Default)]
pub(super) struct StableNameAllocator {
    global_index: usize,
    block_index: usize,
    local_index: usize,
}

impl StableNameAllocator {
    pub(super) fn next_global_name(&mut self) -> String {
        let name = self.global_index.to_string();
        self.global_index += 1;
        name
    }

    pub(super) fn next_block_name(&mut self) -> String {
        let name = format!("block_{}", self.block_index);
        self.block_index += 1;
        name
    }

    pub(super) fn next_local_name(&mut self) -> String {
        let name = format!("var_{}", self.local_index);
        self.local_index += 1;
        name
    }
}

#[derive(Debug, Clone)]
struct CfgBlockPlan {
    name: String,
    predecessors: Vec<usize>,
    terminator: BlockTerminator,
}

#[derive(Debug, Clone)]
enum BlockTerminator {
    Ret,
    Jump {
        dest: String,
    },
    Branch {
        true_dest: String,
        false_dest: String,
    },
    Switch {
        ty: Type,
        default_dest: String,
        cases: Vec<(i64, String)>,
    },
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
struct MemorySlot {
    ty: Type,
    ptr_ty: Type,
    ptr: Operand,
}

#[derive(Debug, Clone, Default)]
pub(super) struct TypedValuePool {
    values: FxHashMap<Type, Vec<Operand>>,
    memory_slots: Vec<MemorySlot>,
}

impl TypedValuePool {
    fn add(&mut self, ty: Type, operand: Operand) {
        let entry = self.values.entry(ty).or_default();
        if !entry.contains(&operand) {
            entry.push(operand);
        }
    }

    fn has_values(&self, ty: &Type) -> bool {
        self.values.get(ty).is_some_and(|values| !values.is_empty())
    }

    fn has_local(&self, ty: &Type) -> bool {
        self.values.get(ty).is_some_and(|values| {
            values.iter().any(|operand| {
                matches!(operand, Operand::LocalRef(_) | Operand::TypedLocalRef(_, _))
            })
        })
    }

    fn has_memory_slots(&self) -> bool {
        !self.memory_slots.is_empty()
    }

    fn add_memory_slot(&mut self, ty: Type, ptr_ty: Type, ptr: Operand) {
        self.add(ptr_ty.clone(), ptr.clone());

        let slot = MemorySlot { ty, ptr_ty, ptr };
        if !self.memory_slots.contains(&slot) {
            self.memory_slots.push(slot);
        }
    }

    fn choose_memory_slot(&self, bytes: &mut Unstructured<'_>) -> Option<MemorySlot> {
        choose_index(bytes, self.memory_slots.len()).map(|index| self.memory_slots[index].clone())
    }

    fn choose(
        &self,
        ty: &Type,
        bytes: &mut Unstructured<'_>,
        prefer_locals: bool,
    ) -> Option<Operand> {
        let values = self.values.get(ty)?;
        if values.is_empty() {
            return None;
        }

        if prefer_locals {
            let locals: Vec<_> = values
                .iter()
                .filter(|operand| {
                    matches!(operand, Operand::LocalRef(_) | Operand::TypedLocalRef(_, _))
                })
                .cloned()
                .collect();
            if let Some(index) = choose_index(bytes, locals.len()) {
                return Some(locals[index].clone());
            }
        }

        choose_index(bytes, values.len()).map(|index| values[index].clone())
    }

    fn choose_ptr_operand(
        &self,
        ty: &Type,
        bytes: &mut Unstructured<'_>,
        prefer_globals: bool,
    ) -> Option<Operand> {
        let values = self.values.get(ty)?;
        let filtered: Vec<_> = values
            .iter()
            .filter(|operand| {
                let is_global = is_global_operand(operand);
                if prefer_globals {
                    is_global
                } else {
                    !is_global
                }
            })
            .cloned()
            .collect();

        if let Some(index) = choose_index(bytes, filtered.len()) {
            return Some(filtered[index].clone());
        }

        self.choose(ty, bytes, false)
    }

    fn intersection(pools: &[&Self]) -> Self {
        let Some((first, rest)) = pools.split_first() else {
            return Self::default();
        };

        let mut intersection = (*first).clone();
        for pool in rest {
            intersection.retain_common(pool);
        }
        intersection
    }

    fn retain_common(&mut self, other: &Self) {
        self.values.retain(|ty, operands| {
            operands.retain(|operand| other.contains(ty, operand));
            !operands.is_empty()
        });
        self.memory_slots
            .retain(|slot| other.memory_slots.contains(slot));
    }

    fn contains(&self, ty: &Type, operand: &Operand) -> bool {
        self.values
            .get(ty)
            .is_some_and(|operands| operands.contains(operand))
    }
}

#[derive(Debug, Clone)]
struct CallTarget {
    name: String,
    return_ty: Option<Type>,
    params: Vec<Type>,
}

impl From<&Function> for CallTarget {
    fn from(function: &Function) -> Self {
        Self {
            name: function.name.clone(),
            return_ty: (function.return_type != Type::Void).then(|| function.return_type.clone()),
            params: function
                .params
                .iter()
                .map(|param| param.ty.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BodyInstructionKind {
    Call,
    I64BinOp,
    FloatBinOp,
    ICmp,
    FCmp,
    Zext,
    SIToFP,
    FPToSI,
    Alloca,
    Load,
    Store,
    Select,
    GetElementPtr,
}

const BODY_INSTRUCTION_KINDS: [BodyInstructionKind; 8] = [
    BodyInstructionKind::Call,
    BodyInstructionKind::I64BinOp,
    BodyInstructionKind::FloatBinOp,
    BodyInstructionKind::ICmp,
    BodyInstructionKind::FCmp,
    BodyInstructionKind::Zext,
    BodyInstructionKind::SIToFP,
    BodyInstructionKind::FPToSI,
];

const BASE_BODY_INSTRUCTION_KINDS: [BodyInstructionKind; 1] = [BodyInstructionKind::Call];

const MEMORY_BODY_INSTRUCTION_KINDS: [BodyInstructionKind; 5] = [
    BodyInstructionKind::Load,
    BodyInstructionKind::Store,
    BodyInstructionKind::Select,
    BodyInstructionKind::GetElementPtr,
    BodyInstructionKind::Alloca,
];

const I64_BINOPS: [BinOpKind; 6] = [
    BinOpKind::Add,
    BinOpKind::Sub,
    BinOpKind::Mul,
    BinOpKind::And,
    BinOpKind::Or,
    BinOpKind::Xor,
];

const FLOAT_BINOPS: [BinOpKind; 3] = [BinOpKind::Fadd, BinOpKind::Fsub, BinOpKind::Fmul];

const FLOAT_SCALAR_TYPES: [Type; 3] = [Type::Half, Type::Float, Type::Double];

const INT_PREDICATES: [IntPredicate; 6] = [
    IntPredicate::Eq,
    IntPredicate::Ne,
    IntPredicate::Slt,
    IntPredicate::Sle,
    IntPredicate::Sgt,
    IntPredicate::Sge,
];

const FLOAT_PREDICATES: [FloatPredicate; 6] = [
    FloatPredicate::Oeq,
    FloatPredicate::One,
    FloatPredicate::Olt,
    FloatPredicate::Ole,
    FloatPredicate::Ogt,
    FloatPredicate::Oge,
];

#[derive(Debug)]
pub(super) struct QirGenState {
    pub(super) module: Module,
    preset: ShellPreset,
    profile: QirProfilePreset,
    shell_counts: ShellCounts,
    typed_pointers: bool,
    names: StableNameAllocator,
    declaration_registry: FxHashSet<String>,
    global_registry: FxHashMap<String, String>,
}

impl QirGenState {
    pub(super) fn new(
        preset: ShellPreset,
        shell_counts: ShellCounts,
        profile: QirProfilePreset,
        bytes: &mut Unstructured<'_>,
    ) -> Self {
        let qir_profile = profile.to_qir_profile();
        let typed_pointers = matches!(
            profile,
            QirProfilePreset::BaseV1 | QirProfilePreset::AdaptiveV1
        );
        let (attribute_groups, named_metadata, metadata_nodes, struct_types) =
            if let Some(qir_profile) = qir_profile {
                let (named_metadata, metadata_nodes) = build_qdk_metadata(qir_profile, bytes);
                (
                    build_qdk_attribute_groups(qir_profile, shell_counts),
                    named_metadata,
                    metadata_nodes,
                    qir_profile.struct_types(),
                )
            } else {
                (Vec::new(), Vec::new(), Vec::new(), Vec::new())
            };

        Self {
            module: Module {
                source_filename: None,
                target_datalayout: None,
                target_triple: None,
                struct_types,
                globals: Vec::new(),
                functions: Vec::new(),
                attribute_groups,
                named_metadata,
                metadata_nodes,
            },
            preset,
            profile,
            shell_counts,
            typed_pointers,
            names: StableNameAllocator::default(),
            declaration_registry: FxHashSet::default(),
            global_registry: FxHashMap::default(),
        }
    }

    fn build(mut self, effective: &EffectiveConfig, bytes: &mut Unstructured<'_>) -> Module {
        self.add_optional_declarations(bytes);
        self.add_optional_globals(effective, bytes);
        let entry_point = self.build_entry_point(effective, bytes);
        self.module.functions.insert(0, entry_point);
        self.module
    }

    fn populate_target_headers(&mut self, effective: &EffectiveConfig) {
        if !should_emit_target_headers(effective) {
            return;
        }

        self.module.target_datalayout = Some(GENERATED_TARGET_DATALAYOUT.to_string());
        self.module.target_triple = Some(GENERATED_TARGET_TRIPLE.to_string());
    }

    fn build_entry_point(
        &mut self,
        effective: &EffectiveConfig,
        bytes: &mut Unstructured<'_>,
    ) -> Function {
        let include_initialize = self.preset.include_qdk_shell;
        if include_initialize {
            self.register_declaration(ShellDeclaration::Initialize);
        }

        if self.preset.include_qdk_shell {
            self.register_declaration(ShellDeclaration::TupleRecordOutput);
        }

        if matches!(
            effective.profile,
            QirProfilePreset::AdaptiveV1 | QirProfilePreset::AdaptiveV2
        ) {
            self.register_declaration(ShellDeclaration::ReadResult);
        }

        if matches!(effective.profile, QirProfilePreset::AdaptiveV2) {
            self.register_declaration(ShellDeclaration::ResultArrayRecordOutput);
        }

        let cfg = self.plan_entry_cfg(effective, bytes);
        let call_targets = self.collect_call_targets();
        let mut basic_blocks = Vec::with_capacity(cfg.len());
        let mut exit_pools = Vec::with_capacity(cfg.len());

        for (block_index, plan) in cfg.iter().enumerate() {
            let predecessor_pools: Vec<_> = plan
                .predecessors
                .iter()
                .map(|&index| &exit_pools[index])
                .collect();
            let predecessor_names: Vec<_> = plan
                .predecessors
                .iter()
                .map(|&index| cfg[index].name.clone())
                .collect();

            let mut pool = if block_index == 0 {
                self.build_base_value_pool()
            } else {
                TypedValuePool::intersection(&predecessor_pools)
            };

            let require_nontrivial_body = block_index == 0
                || (self.profile == QirProfilePreset::BaseV1 && block_index + 1 < cfg.len());

            let mut instructions = self.build_block_instructions(
                effective,
                plan,
                &predecessor_names,
                &predecessor_pools,
                &call_targets,
                &mut pool,
                effective.max_instrs_per_block,
                bytes,
                require_nontrivial_body,
            );

            if block_index == 0 && include_initialize {
                let null_arg = Operand::NullPtr;
                let init_call = Instruction::Call {
                    return_ty: None,
                    callee: qir::rt::INITIALIZE.to_string(),
                    args: vec![(
                        if self.typed_pointers {
                            Type::TypedPtr(Box::new(Type::Integer(8)))
                        } else {
                            Type::Ptr
                        },
                        null_arg,
                    )],
                    result: None,
                    attr_refs: Vec::new(),
                };
                instructions.insert(0, init_call);
            }

            exit_pools.push(pool);
            basic_blocks.push(BasicBlock {
                name: plan.name.clone(),
                instructions,
            });
        }

        Function {
            name: qir::ENTRYPOINT_NAME.to_string(),
            return_type: Type::Integer(64),
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: self.preset.entry_point_attr_refs(),
            basic_blocks,
        }
    }

    fn collect_call_targets(&self) -> Vec<CallTarget> {
        self.module
            .functions
            .iter()
            .filter(|function| function.is_declaration)
            .map(CallTarget::from)
            .collect()
    }

    fn pointer_result_type(&self, pointee_ty: &Type) -> Type {
        if self.typed_pointers {
            Type::TypedPtr(Box::new(pointee_ty.clone()))
        } else {
            Type::Ptr
        }
    }

    pub(super) fn build_base_value_pool(&self) -> TypedValuePool {
        let mut pool = TypedValuePool::default();

        for value in [0, 1] {
            pool.add(Type::Integer(1), Operand::IntConst(Type::Integer(1), value));
        }
        for value in [-1, 0, 1, 2, 3] {
            pool.add(
                Type::Integer(64),
                Operand::IntConst(Type::Integer(64), value),
            );
        }
        for ty in FLOAT_SCALAR_TYPES {
            for value in [0.0, 1.0, -1.0, 2.5] {
                pool.add(ty.clone(), Operand::float_const(ty.clone(), value));
            }
        }

        if self.typed_pointers {
            let qubit_ty = Type::NamedPtr("Qubit".to_string());
            let result_ty = Type::NamedPtr("Result".to_string());
            let i8_ptr_ty = Type::TypedPtr(Box::new(Type::Integer(8)));

            for value in 0..=2 {
                pool.add(qubit_ty.clone(), Operand::int_to_named_ptr(value, "Qubit"));
                pool.add(
                    result_ty.clone(),
                    Operand::int_to_named_ptr(value, "Result"),
                );
            }
            for global in self
                .module
                .globals
                .iter()
                .filter(|global| is_string_label_global(global))
            {
                let array_ty = global.ty.clone();
                pool.add(
                    i8_ptr_ty.clone(),
                    Operand::GetElementPtr {
                        ty: array_ty.clone(),
                        ptr: global.name.clone(),
                        ptr_ty: array_ty,
                        indices: vec![
                            Operand::IntConst(Type::Integer(64), 0),
                            Operand::IntConst(Type::Integer(64), 0),
                        ],
                    },
                );
            }
        } else {
            pool.add(Type::Ptr, Operand::NullPtr);
            for value in 0..=2 {
                pool.add(Type::Ptr, Operand::IntToPtr(value, Type::Ptr));
            }
            for global in self
                .module
                .globals
                .iter()
                .filter(|global| is_string_label_global(global))
            {
                pool.add(Type::Ptr, Operand::GlobalRef(global.name.clone()));
            }
        }

        pool
    }

    fn plan_entry_cfg(
        &mut self,
        effective: &EffectiveConfig,
        bytes: &mut Unstructured<'_>,
    ) -> Vec<CfgBlockPlan> {
        let block_count =
            if self.profile == QirProfilePreset::BaseV1 && !has_expanded_generation(effective) {
                BASE_V1_BLOCK_COUNT
            } else if effective.max_blocks_per_func <= 1 {
                1
            } else {
                bytes
                    .int_in_range(2..=effective.max_blocks_per_func)
                    .unwrap_or(2)
            };

        let mut names = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            names.push(self.names.next_block_name());
        }

        let mut predecessors = vec![Vec::new(); block_count];
        let mut terminators = Vec::with_capacity(block_count);
        for block_index in 0..block_count {
            if block_index + 1 >= block_count {
                if effective.allow_switch && take_flag(bytes, false) {
                    terminators.push(BlockTerminator::Unreachable);
                } else {
                    terminators.push(BlockTerminator::Ret);
                }
                continue;
            }

            let next = block_index + 1;
            let can_branch = block_index + 2 < block_count
                && (self.profile != QirProfilePreset::BaseV1 || has_expanded_generation(effective));
            let can_switch = effective.allow_switch && block_index + 2 < block_count;
            let default_branch = block_index == 0 && can_branch;

            if can_switch && take_flag(bytes, false) {
                predecessors[next].push(block_index);
                predecessors[next + 1].push(block_index);

                let mut cases = vec![(0, names[next + 1].clone())];
                if block_index + 3 < block_count && take_flag(bytes, false) {
                    let extra_dest = block_count - 1;
                    if extra_dest > next + 1 {
                        predecessors[extra_dest].push(block_index);
                        cases.push((1, names[extra_dest].clone()));
                    }
                }

                terminators.push(BlockTerminator::Switch {
                    ty: Type::Integer(64),
                    default_dest: names[next].clone(),
                    cases,
                });
            } else if can_branch && take_flag(bytes, default_branch) {
                let false_dest = bytes
                    .int_in_range((next + 1)..=block_count - 1)
                    .unwrap_or(block_count - 1);
                predecessors[next].push(block_index);
                predecessors[false_dest].push(block_index);
                terminators.push(BlockTerminator::Branch {
                    true_dest: names[next].clone(),
                    false_dest: names[false_dest].clone(),
                });
            } else {
                predecessors[next].push(block_index);
                terminators.push(BlockTerminator::Jump {
                    dest: names[next].clone(),
                });
            }
        }

        names
            .into_iter()
            .zip(predecessors)
            .zip(terminators)
            .map(|((name, predecessors), terminator)| CfgBlockPlan {
                name,
                predecessors,
                terminator,
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn build_block_instructions(
        &mut self,
        effective: &EffectiveConfig,
        plan: &CfgBlockPlan,
        predecessor_names: &[String],
        predecessor_pools: &[&TypedValuePool],
        call_targets: &[CallTarget],
        pool: &mut TypedValuePool,
        max_instrs_per_block: usize,
        bytes: &mut Unstructured<'_>,
        require_nontrivial_body: bool,
    ) -> Vec<Instruction> {
        let body_budget = max_instrs_per_block.saturating_sub(1);
        let min_body_instructions = usize::from(require_nontrivial_body && body_budget > 0);
        let body_instruction_count =
            take_body_instruction_count(bytes, body_budget, min_body_instructions);
        let mut instructions = Vec::with_capacity(body_instruction_count + 3);

        if effective.allow_phi
            && body_budget > instructions.len()
            && let Some(instruction) =
                self.build_phi_instruction(predecessor_names, predecessor_pools, pool, bytes)
        {
            instructions.push(instruction);
        }

        if effective.allow_memory_ops
            && !pool.has_memory_slots()
            && body_budget > instructions.len()
            && let Some(instruction) = self.build_alloca_instruction(pool)
        {
            instructions.push(instruction);
        }

        for _ in 0..body_instruction_count {
            if instructions.len() >= body_budget {
                break;
            }

            if let Some(instruction) =
                self.build_body_instruction(effective, call_targets, pool, bytes)
            {
                instructions.push(instruction);
            }
        }

        if require_nontrivial_body && body_budget > 0 && instructions.is_empty() {
            let fallback = if effective.allow_memory_ops {
                self.build_alloca_instruction(pool)
                    .or_else(|| self.build_i64_binop_instruction(pool, bytes))
            } else if self.profile == QirProfilePreset::BaseV1
                && !has_expanded_generation(effective)
            {
                self.build_call_instruction(call_targets, pool, bytes)
            } else {
                self.build_i64_binop_instruction(pool, bytes)
                    .or_else(|| self.build_call_instruction(call_targets, pool, bytes))
            };
            if let Some(instruction) = fallback {
                instructions.push(instruction);
            }
        }

        if matches!(plan.terminator, BlockTerminator::Branch { .. })
            && !pool.has_local(&Type::Integer(1))
            && instructions.len() < body_budget
            && let Some(instruction) = self.build_compare_instruction(pool, bytes)
        {
            instructions.push(instruction);
        }

        instructions.push(Self::build_terminator(plan, pool, bytes));
        instructions
    }

    fn build_body_instruction(
        &mut self,
        effective: &EffectiveConfig,
        call_targets: &[CallTarget],
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let mut kinds: Vec<_> = match self.profile {
            QirProfilePreset::BaseV1 if !has_expanded_generation(effective) => {
                BASE_BODY_INSTRUCTION_KINDS.to_vec()
            }
            _ => BODY_INSTRUCTION_KINDS.to_vec(),
        };

        if effective.allow_memory_ops {
            kinds.splice(0..0, MEMORY_BODY_INSTRUCTION_KINDS);
        }

        let start = choose_index(bytes, kinds.len()).unwrap_or(0);
        for offset in 0..kinds.len() {
            let kind = kinds[(start + offset) % kinds.len()];
            let instruction = match kind {
                BodyInstructionKind::Call => self.build_call_instruction(call_targets, pool, bytes),
                BodyInstructionKind::I64BinOp => self.build_i64_binop_instruction(pool, bytes),
                BodyInstructionKind::FloatBinOp => self.build_float_binop_instruction(pool, bytes),
                BodyInstructionKind::ICmp => self.build_icmp_instruction(pool, bytes),
                BodyInstructionKind::FCmp => self.build_fcmp_instruction(pool, bytes),
                BodyInstructionKind::Zext => self.build_zext_instruction(pool, bytes),
                BodyInstructionKind::SIToFP => self.build_sitofp_instruction(pool, bytes),
                BodyInstructionKind::FPToSI => self.build_fptosi_instruction(pool, bytes),
                BodyInstructionKind::Alloca => self.build_alloca_instruction(pool),
                BodyInstructionKind::Load => self.build_load_instruction(pool, bytes),
                BodyInstructionKind::Store => Self::build_store_instruction(pool, bytes),
                BodyInstructionKind::Select => self.build_select_instruction(pool, bytes),
                BodyInstructionKind::GetElementPtr => self.build_gep_instruction(pool, bytes),
            };
            if instruction.is_some() {
                return instruction;
            }
        }

        None
    }

    fn build_call_instruction(
        &mut self,
        call_targets: &[CallTarget],
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let candidates: Vec<_> = call_targets
            .iter()
            .filter(|target| {
                self.profile != QirProfilePreset::BaseV1 || is_base_v1_safe_call_target(target)
            })
            .filter(|target| target.params.iter().all(|ty| pool.has_values(ty)))
            .collect();
        let target = candidates.get(choose_index(bytes, candidates.len())?)?;

        let mut args = Vec::with_capacity(target.params.len());
        for (param_index, ty) in target.params.iter().enumerate() {
            let operand = if is_pointer_type(ty) {
                pool.choose_ptr_operand(
                    ty,
                    bytes,
                    prefers_global_label_arg(&target.name, param_index),
                )?
            } else {
                pool.choose(ty, bytes, false)?
            };
            args.push((ty.clone(), operand));
        }

        let (return_ty, result) = if let Some(ty) = &target.return_ty {
            let result_name = self.names.next_local_name();
            pool.add(ty.clone(), Operand::LocalRef(result_name.clone()));
            (Some(ty.clone()), Some(result_name))
        } else {
            (None, None)
        };

        Some(Instruction::Call {
            return_ty,
            callee: target.name.clone(),
            args,
            result,
            attr_refs: Vec::new(),
        })
    }

    fn build_i64_binop_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let op = choose_from_slice(bytes, &I64_BINOPS)?;
        let lhs = pool.choose(&Type::Integer(64), bytes, true)?;
        let rhs = pool.choose(&Type::Integer(64), bytes, false)?;
        let result = self.names.next_local_name();
        pool.add(Type::Integer(64), Operand::LocalRef(result.clone()));

        Some(Instruction::BinOp {
            op,
            ty: Type::Integer(64),
            lhs,
            rhs,
            result,
        })
    }

    pub(super) fn build_float_binop_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let op = choose_from_slice(bytes, &FLOAT_BINOPS)?;
        let ty = choose_available_floating_type(pool, bytes)?;
        let lhs = pool.choose(&ty, bytes, true)?;
        let rhs = pool.choose(&ty, bytes, false)?;
        let result = self.names.next_local_name();
        pool.add(ty.clone(), Operand::LocalRef(result.clone()));

        Some(Instruction::BinOp {
            op,
            ty,
            lhs,
            rhs,
            result,
        })
    }

    fn build_icmp_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let pred = choose_from_slice(bytes, &INT_PREDICATES)?;
        let lhs = pool.choose(&Type::Integer(64), bytes, true)?;
        let rhs = pool.choose(&Type::Integer(64), bytes, false)?;
        let result = self.names.next_local_name();
        pool.add(Type::Integer(1), Operand::LocalRef(result.clone()));

        Some(Instruction::ICmp {
            pred,
            ty: Type::Integer(64),
            lhs,
            rhs,
            result,
        })
    }

    pub(super) fn build_fcmp_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let pred = choose_from_slice(bytes, &FLOAT_PREDICATES)?;
        let ty = choose_available_floating_type(pool, bytes)?;
        let lhs = pool.choose(&ty, bytes, true)?;
        let rhs = pool.choose(&ty, bytes, false)?;
        let result = self.names.next_local_name();
        pool.add(Type::Integer(1), Operand::LocalRef(result.clone()));

        Some(Instruction::FCmp {
            pred,
            ty,
            lhs,
            rhs,
            result,
        })
    }

    fn build_compare_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        if take_flag(bytes, true) {
            self.build_icmp_instruction(pool, bytes)
                .or_else(|| self.build_fcmp_instruction(pool, bytes))
        } else {
            self.build_fcmp_instruction(pool, bytes)
                .or_else(|| self.build_icmp_instruction(pool, bytes))
        }
    }

    fn build_zext_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let value = pool.choose(&Type::Integer(1), bytes, true)?;
        let result = self.names.next_local_name();
        pool.add(Type::Integer(64), Operand::LocalRef(result.clone()));

        Some(Instruction::Cast {
            op: CastKind::Zext,
            from_ty: Type::Integer(1),
            to_ty: Type::Integer(64),
            value,
            result,
        })
    }

    pub(super) fn build_sitofp_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let to_ty = choose_available_floating_type(pool, bytes)?;
        let value = pool.choose(&Type::Integer(64), bytes, true)?;
        let result = self.names.next_local_name();
        pool.add(to_ty.clone(), Operand::LocalRef(result.clone()));

        Some(Instruction::Cast {
            op: CastKind::Sitofp,
            from_ty: Type::Integer(64),
            to_ty,
            value,
            result,
        })
    }

    pub(super) fn build_fptosi_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let from_ty = choose_available_floating_type(pool, bytes)?;
        let value = pool.choose(&from_ty, bytes, true)?;
        let result = self.names.next_local_name();
        pool.add(Type::Integer(64), Operand::LocalRef(result.clone()));

        Some(Instruction::Cast {
            op: CastKind::Fptosi,
            from_ty,
            to_ty: Type::Integer(64),
            value,
            result,
        })
    }

    fn build_phi_instruction(
        &mut self,
        predecessor_names: &[String],
        predecessor_pools: &[&TypedValuePool],
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        if predecessor_pools.len() < 2
            || predecessor_names.len() != predecessor_pools.len()
            || !predecessor_pools
                .iter()
                .all(|pred_pool| pred_pool.has_values(&Type::Integer(64)))
        {
            return None;
        }

        let mut incoming = Vec::with_capacity(predecessor_pools.len());
        for (pred_name, pred_pool) in predecessor_names.iter().zip(predecessor_pools.iter()) {
            incoming.push((
                pred_pool.choose(&Type::Integer(64), bytes, true)?,
                pred_name.clone(),
            ));
        }

        let result = self.names.next_local_name();
        pool.add(Type::Integer(64), Operand::LocalRef(result.clone()));

        Some(Instruction::Phi {
            ty: Type::Integer(64),
            incoming,
            result,
        })
    }

    fn build_alloca_instruction(&mut self, pool: &mut TypedValuePool) -> Option<Instruction> {
        let ty = Type::Integer(64);
        let result = self.names.next_local_name();
        let ptr_ty = self.pointer_result_type(&ty);
        let ptr = Operand::LocalRef(result.clone());
        pool.add_memory_slot(ty.clone(), ptr_ty, ptr);

        Some(Instruction::Alloca { ty, result })
    }

    fn build_load_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let slot = pool.choose_memory_slot(bytes)?;
        let result = self.names.next_local_name();
        pool.add(slot.ty.clone(), Operand::LocalRef(result.clone()));

        Some(Instruction::Load {
            ty: slot.ty,
            ptr_ty: slot.ptr_ty,
            ptr: slot.ptr,
            result,
        })
    }

    fn build_store_instruction(
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let slot = pool.choose_memory_slot(bytes)?;
        let value = pool.choose(&slot.ty, bytes, true)?;

        Some(Instruction::Store {
            ty: slot.ty,
            value,
            ptr_ty: slot.ptr_ty,
            ptr: slot.ptr,
        })
    }

    fn build_select_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let result = self.names.next_local_name();
        let cond = pool.choose(&Type::Integer(1), bytes, true)?;
        let true_val = pool.choose(&Type::Integer(64), bytes, true)?;
        let false_val = pool.choose(&Type::Integer(64), bytes, false)?;
        pool.add(Type::Integer(64), Operand::LocalRef(result.clone()));

        Some(Instruction::Select {
            cond,
            true_val,
            false_val,
            ty: Type::Integer(64),
            result,
        })
    }

    fn build_gep_instruction(
        &mut self,
        pool: &mut TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Option<Instruction> {
        let slot = pool.choose_memory_slot(bytes)?;
        let result = self.names.next_local_name();
        let ptr = Operand::LocalRef(result.clone());
        let indices = vec![Operand::IntConst(Type::Integer(64), 0)];
        pool.add_memory_slot(slot.ty.clone(), slot.ptr_ty.clone(), ptr);

        Some(Instruction::GetElementPtr {
            inbounds: true,
            pointee_ty: slot.ty,
            ptr_ty: slot.ptr_ty,
            ptr: slot.ptr,
            indices,
            result,
        })
    }

    fn build_terminator(
        plan: &CfgBlockPlan,
        pool: &TypedValuePool,
        bytes: &mut Unstructured<'_>,
    ) -> Instruction {
        match &plan.terminator {
            BlockTerminator::Ret => Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
            BlockTerminator::Jump { dest } => Instruction::Jump { dest: dest.clone() },
            BlockTerminator::Branch {
                true_dest,
                false_dest,
            } => Instruction::Br {
                cond_ty: Type::Integer(1),
                cond: pool
                    .choose(&Type::Integer(1), bytes, true)
                    .unwrap_or(Operand::IntConst(Type::Integer(1), 1)),
                true_dest: true_dest.clone(),
                false_dest: false_dest.clone(),
            },
            BlockTerminator::Switch {
                ty,
                default_dest,
                cases,
            } => Instruction::Switch {
                ty: ty.clone(),
                value: pool
                    .choose(ty, bytes, true)
                    .unwrap_or_else(|| Operand::IntConst(ty.clone(), 0)),
                default_dest: default_dest.clone(),
                cases: cases.clone(),
            },
            BlockTerminator::Unreachable => Instruction::Unreachable,
        }
    }

    fn add_optional_declarations(&mut self, bytes: &mut Unstructured<'_>) {
        for declaration in SHELL_DECLARATIONS {
            match declaration {
                ShellDeclaration::DoubleRecordOutput => {
                    if matches!(
                        self.profile,
                        QirProfilePreset::AdaptiveV1 | QirProfilePreset::AdaptiveV2
                    ) {
                        continue;
                    }
                }
                ShellDeclaration::BoolRecordOutput
                | ShellDeclaration::IntRecordOutput
                | ShellDeclaration::QubitAllocate
                | ShellDeclaration::QubitRelease => {
                    if self.profile == QirProfilePreset::BaseV1 {
                        continue;
                    }
                }
                _ => {}
            }
            if take_flag(bytes, self.preset.default_include_declarations) {
                self.register_declaration(declaration);
            }
        }
    }

    fn register_declaration(&mut self, declaration: ShellDeclaration) {
        let name = declaration_name(declaration);
        if self.declaration_registry.insert(name.to_string()) {
            self.module
                .functions
                .push(build_declaration(declaration, self.typed_pointers));
        }
    }

    fn add_optional_globals(&mut self, effective: &EffectiveConfig, bytes: &mut Unstructured<'_>) {
        if !take_flag(bytes, self.preset.default_include_globals) {
            return;
        }

        if self.preset.include_qdk_shell {
            self.add_qdk_label_globals();
        } else {
            let count = take_optional_global_count(bytes);
            for index in 0..count {
                self.intern_string_global(build_bare_global_string(bytes, index));
            }
        }

        if supports_modeled_global_generation(effective.profile) {
            self.add_modeled_opaque_globals(bytes);
        }
    }

    fn add_qdk_label_globals(&mut self) {
        self.intern_string_global("0_a".to_string());

        let result_labels = self
            .shell_counts
            .required_num_results
            .min(MAX_OPTIONAL_GLOBALS);
        for result_index in 0..result_labels {
            self.intern_string_global(format!("{}_a{}r", result_index + 1, result_index));
        }
    }

    fn add_modeled_opaque_globals(&mut self, bytes: &mut Unstructured<'_>) {
        if self.typed_pointers {
            return;
        }

        self.push_fresh_global(Type::Integer(64), Linkage::External, false, None);
        self.push_fresh_global(Type::Ptr, Linkage::Internal, false, Some(Constant::Null));

        let value = choose_from_slice(bytes, &MODELED_INTEGER_INITIALIZERS).unwrap_or(0);
        self.push_fresh_global(
            Type::Integer(64),
            Linkage::Internal,
            true,
            Some(Constant::Int(value)),
        );
    }

    fn push_fresh_global(
        &mut self,
        ty: Type,
        linkage: Linkage,
        is_constant: bool,
        initializer: Option<Constant>,
    ) -> String {
        let name = self.names.next_global_name();
        self.module.globals.push(GlobalVariable {
            name: name.clone(),
            ty,
            linkage,
            is_constant,
            initializer,
        });

        name
    }

    pub(super) fn intern_string_global(&mut self, value: String) -> String {
        if let Some(name) = self.global_registry.get(&value) {
            return name.clone();
        }

        let array_len = u64::try_from(value.len() + 1).expect("CString length should fit in u64");
        let name = self.push_fresh_global(
            Type::Array(array_len, Box::new(Type::Integer(8))),
            Linkage::Internal,
            true,
            Some(Constant::CString(value.clone())),
        );
        self.global_registry.insert(value, name.clone());

        name
    }
}

pub(super) fn build_module_shell(
    effective: &EffectiveConfig,
    bytes: &mut Unstructured<'_>,
) -> Module {
    let preset = ShellPreset::from_profile(effective.profile);
    let shell_counts = take_shell_counts(preset, bytes);
    let mut state = QirGenState::new(preset, shell_counts, effective.profile, bytes);
    state.populate_target_headers(effective);
    state.build(effective, bytes)
}

fn take_body_instruction_count(
    bytes: &mut Unstructured<'_>,
    max_body_instructions: usize,
    min_body_instructions: usize,
) -> usize {
    if max_body_instructions == 0 {
        return 0;
    }

    bytes
        .int_in_range(min_body_instructions..=max_body_instructions)
        .unwrap_or(min_body_instructions)
}

fn choose_index(bytes: &mut Unstructured<'_>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }

    Some(bytes.int_in_range(0..=len - 1).unwrap_or(0))
}

fn choose_from_slice<T: Clone>(bytes: &mut Unstructured<'_>, values: &[T]) -> Option<T> {
    choose_index(bytes, values.len()).map(|index| values[index].clone())
}

fn choose_available_floating_type(
    pool: &TypedValuePool,
    bytes: &mut Unstructured<'_>,
) -> Option<Type> {
    let available: Vec<_> = FLOAT_SCALAR_TYPES
        .into_iter()
        .filter(|ty| pool.has_values(ty))
        .collect();
    choose_from_slice(bytes, &available)
}

fn is_pointer_type(ty: &Type) -> bool {
    matches!(ty, Type::Ptr | Type::NamedPtr(_) | Type::TypedPtr(_))
}

fn is_global_operand(op: &Operand) -> bool {
    matches!(op, Operand::GlobalRef(_) | Operand::GetElementPtr { .. })
}

fn is_string_label_global(global: &GlobalVariable) -> bool {
    global.is_constant
        && matches!(
            (&global.ty, &global.initializer),
            (Type::Array(_, element), Some(Constant::CString(_)))
                if element.as_ref() == &Type::Integer(8)
        )
}

fn should_emit_target_headers(effective: &EffectiveConfig) -> bool {
    matches!(
        (
            effective.profile,
            effective.output_mode,
            effective.roundtrip
        ),
        (
            QirProfilePreset::BareRoundtrip,
            super::OutputMode::RoundTripChecked,
            Some(super::RoundTripKind::BitcodeOnly)
        )
    )
}

fn supports_modeled_global_generation(profile: QirProfilePreset) -> bool {
    matches!(
        profile,
        QirProfilePreset::AdaptiveV2 | QirProfilePreset::BareRoundtrip
    )
}

fn has_expanded_generation(effective: &EffectiveConfig) -> bool {
    effective.allow_phi || effective.allow_switch || effective.allow_memory_ops
}

fn prefers_global_label_arg(callee: &str, param_index: usize) -> bool {
    qir::output_label_arg_index(callee) == Some(param_index)
}

fn is_base_v1_safe_call_target(target: &CallTarget) -> bool {
    matches!(
        target.name.as_str(),
        qir::qis::H | qir::qis::CX | qir::qis::M
    )
}

fn take_shell_counts(preset: ShellPreset, bytes: &mut Unstructured<'_>) -> ShellCounts {
    if !preset.include_qdk_shell {
        return ShellCounts::default();
    }

    ShellCounts {
        required_num_qubits: take_small_count(bytes, 0),
        required_num_results: take_small_count(bytes, 0),
    }
}

fn take_small_count(bytes: &mut Unstructured<'_>, default: usize) -> usize {
    bytes.int_in_range(0..=MAX_SHELL_COUNT).unwrap_or(default)
}

fn take_optional_global_count(bytes: &mut Unstructured<'_>) -> usize {
    bytes.int_in_range(0..=MAX_OPTIONAL_GLOBALS).unwrap_or(0)
}

fn take_flag(bytes: &mut Unstructured<'_>, default: bool) -> bool {
    bytes.arbitrary::<bool>().unwrap_or(default)
}

fn build_bare_global_string(bytes: &mut Unstructured<'_>, index: usize) -> String {
    let suffix_len = bytes.int_in_range(3..=6_usize).unwrap_or(4);
    let mut value = format!("g{index}_");

    for _ in 0..suffix_len {
        let symbol = bytes.int_in_range(0..=35_u8).unwrap_or(0);
        let ch = if symbol < 26 {
            char::from(b'a' + symbol)
        } else {
            char::from(b'0' + (symbol - 26))
        };
        value.push(ch);
    }

    value
}

fn declaration_name(declaration: ShellDeclaration) -> &'static str {
    match declaration {
        ShellDeclaration::Hadamard => qir::qis::H,
        ShellDeclaration::ControlledX => qir::qis::CX,
        ShellDeclaration::Measure => qir::qis::M,
        ShellDeclaration::ArrayRecordOutput => qir::rt::ARRAY_RECORD_OUTPUT,
        ShellDeclaration::ResultRecordOutput => qir::rt::RESULT_RECORD_OUTPUT,
        ShellDeclaration::ResultArrayRecordOutput => qir::rt::RESULT_ARRAY_RECORD_OUTPUT,
        ShellDeclaration::TupleRecordOutput => qir::rt::TUPLE_RECORD_OUTPUT,
        ShellDeclaration::BoolRecordOutput => qir::rt::BOOL_RECORD_OUTPUT,
        ShellDeclaration::IntRecordOutput => qir::rt::INT_RECORD_OUTPUT,
        ShellDeclaration::DoubleRecordOutput => qir::rt::DOUBLE_RECORD_OUTPUT,
        ShellDeclaration::QubitAllocate => qir::rt::QUBIT_ALLOCATE,
        ShellDeclaration::QubitRelease => qir::rt::QUBIT_RELEASE,
        ShellDeclaration::Initialize => qir::rt::INITIALIZE,
        ShellDeclaration::ReadResult => qir::rt::READ_RESULT,
    }
}

#[allow(clippy::too_many_lines)]
fn build_declaration(declaration: ShellDeclaration, typed_pointers: bool) -> Function {
    let qubit_ptr = if typed_pointers {
        Type::NamedPtr("Qubit".to_string())
    } else {
        Type::Ptr
    };
    let result_ptr = if typed_pointers {
        Type::NamedPtr("Result".to_string())
    } else {
        Type::Ptr
    };
    let i8_ptr = if typed_pointers {
        Type::TypedPtr(Box::new(Type::Integer(8)))
    } else {
        Type::Ptr
    };

    match declaration {
        ShellDeclaration::Hadamard => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![Param {
                ty: qubit_ptr,
                name: None,
            }],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::ControlledX => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: qubit_ptr.clone(),
                    name: None,
                },
                Param {
                    ty: qubit_ptr,
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::Measure => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: qubit_ptr,
                    name: None,
                },
                Param {
                    ty: result_ptr,
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: vec![qir::IRREVERSIBLE_ATTR_GROUP_ID],
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::ResultRecordOutput => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: result_ptr,
                    name: None,
                },
                Param {
                    ty: i8_ptr,
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::ResultArrayRecordOutput => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: Type::Integer(64),
                    name: None,
                },
                Param {
                    ty: i8_ptr.clone(),
                    name: None,
                },
                Param {
                    ty: i8_ptr.clone(),
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::ArrayRecordOutput
        | ShellDeclaration::TupleRecordOutput
        | ShellDeclaration::IntRecordOutput => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: Type::Integer(64),
                    name: None,
                },
                Param {
                    ty: i8_ptr.clone(),
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::BoolRecordOutput => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: Type::Integer(1),
                    name: None,
                },
                Param {
                    ty: i8_ptr.clone(),
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::DoubleRecordOutput => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: Type::Double,
                    name: None,
                },
                Param {
                    ty: i8_ptr.clone(),
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::QubitAllocate => Function {
            name: declaration_name(declaration).to_string(),
            return_type: if typed_pointers {
                Type::NamedPtr("Qubit".to_string())
            } else {
                Type::Ptr
            },
            params: vec![Param {
                ty: i8_ptr.clone(),
                name: None,
            }],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::QubitRelease => Function {
            name: declaration_name(declaration).to_string(),
            return_type: Type::Void,
            params: vec![Param {
                ty: if typed_pointers {
                    Type::NamedPtr("Qubit".to_string())
                } else {
                    Type::Ptr
                },
                name: None,
            }],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        },
        ShellDeclaration::Initialize => {
            let ptr_param = if typed_pointers {
                Type::TypedPtr(Box::new(Type::Integer(8)))
            } else {
                Type::Ptr
            };
            Function {
                name: declaration_name(declaration).to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: ptr_param,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            }
        }
        ShellDeclaration::ReadResult => {
            let param_ty = if typed_pointers {
                Type::NamedPtr("Result".to_string())
            } else {
                Type::Ptr
            };
            Function {
                name: declaration_name(declaration).to_string(),
                return_type: Type::Integer(1),
                params: vec![Param {
                    ty: param_ty,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            }
        }
    }
}
