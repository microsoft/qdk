// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod control_flow;
pub mod instruction_types;
#[cfg(test)]
mod tests;

use core::panic;
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::fir::PackageId;
use qsc_partial_eval::{
    ConditionCode, FcmpConditionCode, Instruction, Literal, Operand,
    rir::{BlockId, Program, Ty, Variable},
};
use qsc_rir::debug::{DbgInfo, DbgLocationId, DbgScope, DbgScopeId};
use std::{fmt::Display, vec};
use std::{iter::Peekable, mem::take};

use crate::{
    Circuit, Error, TracerConfig,
    builder::{
        CallableId, GateInputs, LogicalStack, LogicalStackEntry, LogicalStackEntryLocation, LoopId,
        OperationListBuilder, OperationReceiver, PackageOffset, Scope, ScopeStack, SourceLookup,
        WireMap, WireMapBuilder, finish_circuit,
    },
    rir_to_circuit::control_flow::{StructuredControlFlow, reconstruct_control_flow},
};
use instruction_types::{
    BinOpKind, BlockIdx, FcmpCondition, IcmpCondition, Instr, Lit, Opr, Var, VarTy, VariableIdx,
};

/// A trait abstracting access to a quantum program's structure,
/// enabling circuit generation from different program representations
/// (e.g. RIR, QIR).
pub trait QuantumProgram {
    fn entry_block_id(&self) -> BlockIdx;
    fn num_qubits(&self) -> usize;
    fn get_block_instructions(&self, id: BlockIdx) -> Vec<Instr>;
    fn block_ids(&self) -> Vec<BlockIdx>;
    fn dbg_info(&self) -> &DbgInfo;
}

impl QuantumProgram for Program {
    fn entry_block_id(&self) -> BlockIdx {
        let block_id = self
            .callables
            .get(self.entry)
            .expect("entry callable should exist")
            .body
            .expect("entry callable should have a body");
        block_id.into()
    }

    fn num_qubits(&self) -> usize {
        self.num_qubits
            .try_into()
            .expect("number of qubits should fit into usize")
    }

    fn get_block_instructions(&self, id: BlockIdx) -> Vec<Instr> {
        let block = self
            .blocks
            .get(BlockId::from(id))
            .expect("block should exist");
        block
            .0
            .iter()
            .map(|instr| convert_instruction(instr, &self.callables))
            .collect()
    }

    fn block_ids(&self) -> Vec<BlockIdx> {
        self.blocks.iter().map(|(id, _)| id.into()).collect()
    }

    fn dbg_info(&self) -> &DbgInfo {
        &self.dbg_info
    }
}

// --- RIR to instruction_types converters ---

fn convert_binop(kind: BinOpKind, a: &Operand, b: &Operand, v: Variable) -> Instr {
    Instr::BinOp(
        kind,
        convert_operand(a),
        convert_operand(b),
        convert_variable(v),
    )
}

fn convert_instruction(
    instr: &Instruction,
    callables: &IndexMap<qsc_partial_eval::CallableId, qsc_partial_eval::rir::Callable>,
) -> Instr {
    match instr {
        Instruction::Call(callable_id, operands, var, metadata) => {
            let name = callables
                .get(*callable_id)
                .expect("callable should exist")
                .name
                .clone();
            Instr::Call {
                callable_name: name,
                args: operands.iter().map(convert_operand).collect(),
                output: var.map(convert_variable),
                dbg_location: metadata.as_ref().map(|m| m.dbg_location.into()),
            }
        }
        Instruction::Jump(target, ..) => Instr::Jump((*target).into()),
        Instruction::Branch(condition, t, f, metadata) => Instr::Branch {
            condition: convert_variable(*condition),
            true_block: (*t).into(),
            false_block: (*f).into(),
            dbg_location: metadata.as_ref().map(|m| m.dbg_location.into()),
        },
        Instruction::Return => Instr::Return,
        Instruction::Icmp(cc, a, b, v) => Instr::Icmp(
            convert_icmp(*cc),
            convert_operand(a),
            convert_operand(b),
            convert_variable(*v),
        ),
        Instruction::Fcmp(cc, a, b, v) => Instr::Fcmp(
            convert_fcmp(*cc),
            convert_operand(a),
            convert_operand(b),
            convert_variable(*v),
        ),
        Instruction::Phi(pres, v) => Instr::Phi(
            pres.iter()
                .map(|(o, bid)| (convert_operand(o), (*bid).into()))
                .collect(),
            convert_variable(*v),
        ),
        Instruction::Add(a, b, v) => convert_binop(BinOpKind::Add, a, b, *v),
        Instruction::Sub(a, b, v) => convert_binop(BinOpKind::Sub, a, b, *v),
        Instruction::Mul(a, b, v) => convert_binop(BinOpKind::Mul, a, b, *v),
        Instruction::Sdiv(a, b, v) => convert_binop(BinOpKind::Sdiv, a, b, *v),
        Instruction::Srem(a, b, v) => convert_binop(BinOpKind::Srem, a, b, *v),
        Instruction::Shl(a, b, v) => convert_binop(BinOpKind::Shl, a, b, *v),
        Instruction::Ashr(a, b, v) => convert_binop(BinOpKind::Ashr, a, b, *v),
        Instruction::Fadd(a, b, v) => convert_binop(BinOpKind::Fadd, a, b, *v),
        Instruction::Fsub(a, b, v) => convert_binop(BinOpKind::Fsub, a, b, *v),
        Instruction::Fmul(a, b, v) => convert_binop(BinOpKind::Fmul, a, b, *v),
        Instruction::Fdiv(a, b, v) => convert_binop(BinOpKind::Fdiv, a, b, *v),
        Instruction::LogicalAnd(a, b, v) => convert_binop(BinOpKind::LogicalAnd, a, b, *v),
        Instruction::LogicalOr(a, b, v) => convert_binop(BinOpKind::LogicalOr, a, b, *v),
        Instruction::BitwiseAnd(a, b, v) => convert_binop(BinOpKind::BitwiseAnd, a, b, *v),
        Instruction::BitwiseOr(a, b, v) => convert_binop(BinOpKind::BitwiseOr, a, b, *v),
        Instruction::BitwiseXor(a, b, v) => convert_binop(BinOpKind::BitwiseXor, a, b, *v),
        Instruction::LogicalNot(a, v) => {
            Instr::LogicalNot(convert_operand(a), convert_variable(*v))
        }
        Instruction::Convert(a, v) => Instr::Convert(convert_operand(a), convert_variable(*v)),
        Instruction::Store(..) | Instruction::BitwiseNot(..) => {
            // These are unsupported in circuit generation and will be caught later.
            // Convert to a LogicalNot as a placeholder that will trigger an error.
            panic!("unsupported instruction in circuit generation: {instr:?}")
        }
    }
}

fn convert_operand(op: &Operand) -> Opr {
    match op {
        Operand::Literal(lit) => Opr::Literal(convert_literal(lit)),
        Operand::Variable(var) => Opr::Variable(convert_variable(*var)),
    }
}

fn convert_literal(lit: &Literal) -> Lit {
    match lit {
        Literal::Qubit(q) => Lit::Qubit(*q),
        Literal::Result(r) => Lit::Result(*r),
        Literal::Bool(b) => Lit::Bool(*b),
        Literal::Integer(i) => Lit::Integer(*i),
        Literal::Double(d) => Lit::Double(*d),
        Literal::Pointer => Lit::Pointer,
        Literal::Tag(idx, len) => Lit::Tag(*idx, *len),
    }
}

fn convert_variable(var: Variable) -> Var {
    Var {
        id: var.variable_id.into(),
        ty: convert_ty(var.ty),
    }
}

fn convert_ty(ty: Ty) -> VarTy {
    match ty {
        Ty::Qubit => VarTy::Qubit,
        Ty::Result => VarTy::Result,
        Ty::Boolean => VarTy::Boolean,
        Ty::Integer => VarTy::Integer,
        Ty::Double => VarTy::Double,
        Ty::Pointer => VarTy::Pointer,
    }
}

fn convert_icmp(cc: ConditionCode) -> IcmpCondition {
    match cc {
        ConditionCode::Eq => IcmpCondition::Eq,
        ConditionCode::Ne => IcmpCondition::Ne,
        ConditionCode::Slt => IcmpCondition::Slt,
        ConditionCode::Sle => IcmpCondition::Sle,
        ConditionCode::Sgt => IcmpCondition::Sgt,
        ConditionCode::Sge => IcmpCondition::Sge,
    }
}

fn convert_fcmp(cc: FcmpConditionCode) -> FcmpCondition {
    match cc {
        FcmpConditionCode::False => FcmpCondition::False,
        FcmpConditionCode::OrderedAndEqual => FcmpCondition::OrderedAndEqual,
        FcmpConditionCode::OrderedAndGreaterThan => FcmpCondition::OrderedAndGreaterThan,
        FcmpConditionCode::OrderedAndGreaterThanOrEqual => {
            FcmpCondition::OrderedAndGreaterThanOrEqual
        }
        FcmpConditionCode::OrderedAndLessThan => FcmpCondition::OrderedAndLessThan,
        FcmpConditionCode::OrderedAndLessThanOrEqual => FcmpCondition::OrderedAndLessThanOrEqual,
        FcmpConditionCode::OrderedAndNotEqual => FcmpCondition::OrderedAndNotEqual,
        FcmpConditionCode::Ordered => FcmpCondition::Ordered,
        FcmpConditionCode::UnorderedOrEqual => FcmpCondition::UnorderedOrEqual,
        FcmpConditionCode::UnorderedOrGreaterThan => FcmpCondition::UnorderedOrGreaterThan,
        FcmpConditionCode::UnorderedOrGreaterThanOrEqual => {
            FcmpCondition::UnorderedOrGreaterThanOrEqual
        }
        FcmpConditionCode::UnorderedOrLessThan => FcmpCondition::UnorderedOrLessThan,
        FcmpConditionCode::UnorderedOrLessThanOrEqual => FcmpCondition::UnorderedOrLessThanOrEqual,
        FcmpConditionCode::UnorderedOrNotEqual => FcmpCondition::UnorderedOrNotEqual,
        FcmpConditionCode::Unordered => FcmpCondition::Unordered,
        FcmpConditionCode::True => FcmpCondition::True,
    }
}

/// Classifies a callable by its well-known name.
/// The callable kind is purely a function of the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallableKind {
    Measurement,
    Reset,
    Readout,
    OutputRecording,
    /// Any gate (regular intrinsic, noise intrinsic, or custom).
    Gate,
}

fn callable_kind(name: &str) -> CallableKind {
    match name {
        "__quantum__qis__m__body" | "__quantum__qis__mresetz__body" => CallableKind::Measurement,
        "__quantum__qis__reset__body" => CallableKind::Reset,
        "__quantum__rt__read_result" => CallableKind::Readout,
        n if n.ends_with("_record_output") => CallableKind::OutputRecording,
        _ => CallableKind::Gate,
    }
}

pub fn rir_to_circuit(
    program: &impl QuantumProgram,
    config: TracerConfig,
    user_package_ids: &[PackageId],
    source_lookup: &impl SourceLookup,
) -> std::result::Result<Circuit, Error> {
    let entry_block_id = program.entry_block_id();
    let num_qubits = program.num_qubits();

    // Initialize the wire map with the known number of qubits.
    let mut wire_map_builder = WireMapBuilder::default();
    for id in 0..num_qubits {
        wire_map_builder.map_qubit(id, None);
    }

    // Initialize the operation list builder with the configuration.
    let mut builder = OperationListBuilder::new(
        config.max_operations,
        user_package_ids.to_vec(),
        config.group_by_scope,
        config.source_locations,
    );

    // First, get a structured control flow so we can traverse the program in proper execution order,
    // following any branches.
    let structured_control_flow = reconstruct_control_flow(program, entry_block_id);

    // Then we traverse the structured control flow, pushing operations to the builder as we go.
    build_operation_list(
        &mut VariableTracker::default(),
        program,
        &mut wire_map_builder,
        &mut builder,
        &structured_control_flow,
        &[],
        &ScopeStack::top(),
    )?;

    // All operations from the program collected, finalize the circuit.
    let qubits = wire_map_builder.into_wire_map().to_qubits(source_lookup);
    let operations = builder.into_operations();
    let circuit = finish_circuit(source_lookup, operations, qubits, config.group_by_scope);

    Ok(circuit)
}

/// Recursively traverses the structured control flow, pushing operations and measurement results
/// to the builder as it goes.
fn build_operation_list(
    variable_tracker: &mut VariableTracker,
    program: &impl QuantumProgram,
    wire_map_builder: &mut WireMapBuilder,
    op_list_builder: &mut impl OperationReceiver,
    scf: &StructuredControlFlow,
    control_results: &[usize],
    current_stack: &ScopeStack,
) -> Result<(), Error> {
    match scf {
        StructuredControlFlow::Seq(items) => {
            for item in items {
                build_operation_list(
                    variable_tracker,
                    program,
                    wire_map_builder,
                    op_list_builder,
                    item,
                    control_results,
                    current_stack,
                )?;
            }
        }
        StructuredControlFlow::BasicBlock(id) => {
            let instructions = program.get_block_instructions(*id);

            assert!(
                !variable_tracker.blocks_to_control_results.contains_key(*id),
                "block should only be processed once"
            );
            variable_tracker
                .blocks_to_control_results
                .insert(*id, control_results.to_vec());

            push_operations_in_block(
                op_list_builder,
                variable_tracker,
                wire_map_builder,
                program,
                &instructions,
                current_stack,
            )?;
        }
        StructuredControlFlow::If {
            cond,
            then_br,
            else_br,
            branch_dbg_location,
        } => {
            let dbg_lookup = DbgLookup {
                dbg_info: program.dbg_info(),
            };

            let expr = expr_for_variable(&variable_tracker.variables, cond.id)?;

            let mut control_results = control_results.to_vec();
            for r in expr.linked_results() {
                if !control_results.contains(&r) {
                    control_results.push(r);
                }
            }

            let cond_expr_true = format!("if: {expr}");
            let cond_expr_false = format!("if: {}", expr.negate());

            let branch_instruction_stack = branch_dbg_location
                .map(|loc| dbg_lookup.instruction_logical_stack(DbgLocationId::from(loc)))
                .unwrap_or_default();

            let full_stack =
                combine_instr_stack_with_current_stack(current_stack, &branch_instruction_stack);

            let new_stack_true = extend_with_branch_scope(
                &full_stack,
                cond_expr_true,
                true,
                control_results.clone(),
            );

            let new_stack_false = extend_with_branch_scope(
                &full_stack,
                cond_expr_false,
                false,
                control_results.clone(),
            );

            build_operation_list(
                variable_tracker,
                program,
                wire_map_builder,
                op_list_builder,
                then_br,
                &control_results,
                &new_stack_true,
            )?;

            build_operation_list(
                variable_tracker,
                program,
                wire_map_builder,
                op_list_builder,
                else_br,
                &control_results,
                &new_stack_false,
            )?;
        }
        StructuredControlFlow::Return => {}
    }
    Ok(())
}

fn push_operations_in_block(
    builder: &mut impl OperationReceiver,
    state: &mut VariableTracker,
    wire_map_builder: &mut WireMapBuilder,
    program: &impl QuantumProgram,
    instructions: &[Instr],
    current_stack: &ScopeStack,
) -> Result<(), Error> {
    let dbg_lookup = DbgLookup {
        dbg_info: program.dbg_info(),
    };

    for instruction in instructions {
        // First, we update the variable tracker according to this instruction,
        // so that when we later trace the instruction, we have the correct relationships
        // between variables and measurement results.
        process_variables(state, wire_map_builder, instruction)?;

        // Then we push operations to the builder.
        if let Instr::Call {
            callable_name,
            args,
            dbg_location,
            ..
        } = instruction
        {
            let call_instruction_stack = dbg_location
                .map(|loc| dbg_lookup.instruction_logical_stack(DbgLocationId::from(loc)))
                .unwrap_or_default();

            let full_stack =
                combine_instr_stack_with_current_stack(current_stack, &call_instruction_stack);

            trace_call(
                &state.variables,
                &mut BuilderWithRegisterMap {
                    builder,
                    wire_map: wire_map_builder.current(),
                },
                callable_name,
                args,
                full_stack,
            )?;
        }
    }

    Ok(())
}

pub(crate) struct DbgLookup<'a> {
    dbg_info: &'a DbgInfo,
}

impl DbgLookup<'_> {
    /// Returns oldest->newest
    fn instruction_logical_stack(&self, dbg_location_idx: DbgLocationId) -> LogicalStack {
        let mut location_stack = vec![];
        let mut current_location_idx = Some(dbg_location_idx);

        while let Some(location_idx) = current_location_idx {
            let scope_id = self.lexical_scope(location_idx);
            let package_offset = self.source_location(location_idx);
            match &self.dbg_info.get_scope(scope_id) {
                DbgScope::SubProgram { name, location } => {
                    let scope = Scope::Callable(CallableId::Source(
                        PackageOffset {
                            package_id: location.package_id.into(),
                            offset: location.offset,
                        },
                        name.clone(),
                    ));
                    location_stack.push(LogicalStackEntry::new_call_site(package_offset, scope));
                }
                DbgScope::LexicalBlockFile {
                    discriminator,
                    location: scope_location,
                } => {
                    let loop_scope_id = LoopId::Source(PackageOffset {
                        package_id: scope_location.package_id.into(),
                        offset: scope_location.offset,
                    });
                    location_stack.push(LogicalStackEntry::new_call_site(
                        package_offset,
                        Scope::LoopIteration(loop_scope_id, *discriminator),
                    ));
                    location_stack.push(LogicalStackEntry::new(
                        LogicalStackEntryLocation::LoopIteration(loop_scope_id, *discriminator),
                        Scope::Loop(loop_scope_id),
                    ));
                }
            }
            let location = self.dbg_info.get_location(location_idx);
            current_location_idx = location.inlined_at;
        }
        location_stack.reverse();
        LogicalStack(location_stack)
    }

    fn lexical_scope(&self, location: DbgLocationId) -> DbgScopeId {
        self.dbg_info.get_location(location).scope
    }

    fn source_location(&self, location: DbgLocationId) -> PackageOffset {
        let dbg_location = self.dbg_info.get_location(location);
        PackageOffset {
            package_id: dbg_location.location.package_id.into(),
            offset: dbg_location.location.offset,
        }
    }
}

fn process_variables(
    state: &mut VariableTracker,
    wire_map_builder: &mut WireMapBuilder,
    instruction: &Instr,
) -> Result<(), Error> {
    match instruction {
        Instr::Call {
            callable_name,
            args,
            output,
            ..
        } => {
            process_call_variables(
                &mut state.variables,
                wire_map_builder,
                callable_name,
                args,
                *output,
            )?;
        }
        Instr::Fcmp(condition_code, operand, operand1, variable) => {
            process_fcmp_variables(
                &mut state.variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instr::Icmp(condition_code, operand, operand1, variable) => {
            process_icmp_variables(
                &mut state.variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instr::Phi(pres, variable) => {
            process_phi_variables(
                &state.blocks_to_control_results,
                &mut state.variables,
                pres,
                *variable,
            )?;
        }
        Instr::BinOp(_, operand, operand1, variable) => {
            process_binop_variables(&mut state.variables, operand, operand1, *variable)?;
        }
        Instr::LogicalNot(operand, variable) => {
            process_logical_not_variables(&mut state.variables, operand, *variable)?;
        }
        Instr::Convert(operand, variable) => {
            let expr = expr_from_operand(&state.variables, operand)?;
            store_expr_in_variable(&mut state.variables, *variable, expr)?;
        }
        Instr::Return | Instr::Branch { .. } | Instr::Jump(..) => {
            // do nothing for terminators
        }
    }

    Ok(())
}

struct BuilderWithRegisterMap<'a, T: OperationReceiver> {
    builder: &'a mut T,
    wire_map: &'a WireMap,
}

/// Combines the current stack, which DOES include classically controlled scopes, with the stack obtained
/// from instruction metadata, which does NOT include classically controlled scopes.
///
/// Produces a new stack for the instruction that includes the classically controlled scopes
/// as well as any frames from the instruction metadata stack that are not already included in the current stack.
///
/// e.g.
/// current stack: [call A -> branch on r1 -> call B]
/// instruction metadata stack: [call A -> call B -> call C]
/// resulting stack: [call A -> branch on r1 -> call B -> call C]
fn combine_instr_stack_with_current_stack(
    current_stack: &ScopeStack,
    instruction_stack: &LogicalStack,
) -> LogicalStack {
    if current_stack.is_top() {
        // no current stack, just use the instruction metadata stack directly
        return instruction_stack.clone();
    }

    // If non-empty, current stack always ends with a classical scope
    assert!(
        matches!(
            current_stack.current_lexical_scope(),
            Scope::ClassicallyControlled { .. }
        ),
        "current scope must be a branch scope"
    );

    let mut current_iter = current_stack.caller().0.iter().peekable();
    let mut instruction_stack_iter = instruction_stack.0.iter();
    let mut instruction_stack_entry_next = instruction_stack_iter.next();

    // Skip over any frames that are already in the current stack
    while let Some(instruction_stack_entry) = instruction_stack_entry_next
        && let Some(current_entry) = next_non_classical_control_entry(&mut current_iter)
    {
        assert!(
            current_entry == *instruction_stack_entry,
            "instruction stack should match current stack"
        );

        instruction_stack_entry_next = instruction_stack_iter.next();
    }

    let next_location = if let Some(instr_stack_entry) = instruction_stack_entry_next {
        instr_stack_entry.location
    } else {
        LogicalStackEntryLocation::Unknown
    };

    let mut new_stack = current_stack.extend(next_location).0;

    for entry in instruction_stack_iter {
        new_stack.push(entry.clone());
    }

    LogicalStack(new_stack)
}

fn next_non_classical_control_entry<'a>(
    stack_iter: &mut Peekable<impl Iterator<Item = &'a LogicalStackEntry>>,
) -> Option<LogicalStackEntry> {
    if let Some(entry) = stack_iter.next() {
        let mut entry = entry.clone();
        while let Some(peek) = stack_iter.peek() {
            if let LogicalStackEntry {
                location,
                scope: Scope::ClassicallyControlled { .. },
            } = peek
            {
                stack_iter.next();
                entry.location = *location;
            } else {
                break;
            }
        }

        match stack_iter.peek() {
            None => {
                // we are at the end
                return None;
            }
            Some(_) => {
                // there are more entries, so we can return this one
                return Some(entry);
            }
        }
    }
    None
}

fn extend_with_branch_scope(
    stack: &LogicalStack,
    label: String,
    branch: bool,
    control_result_ids: Vec<usize>,
) -> ScopeStack {
    let mut base = stack.clone();
    if let Some(last_mut) = base.0.last_mut() {
        match &mut last_mut.location {
            LogicalStackEntryLocation::Source(package_offset) => {
                last_mut.location =
                    LogicalStackEntryLocation::Branch(Some(*package_offset), branch);
            }
            LogicalStackEntryLocation::Unknown => {}
            _ => {
                panic!(
                    "last entry in branch instruction stack must be a source location or unknown"
                );
            }
        }
    }

    ScopeStack::new(
        base,
        Scope::ClassicallyControlled {
            label,
            control_result_ids,
        },
    )
}

fn process_binop_variables(
    variables: &mut IndexMap<usize, Expr>,
    operand: &Opr,
    operand1: &Opr,
    variable: Var,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(variables, operand)?;
    let expr_right = expr_from_operand(variables, operand1)?;
    let expr = Expr::Rich(RichExpr::function_of([expr_left, expr_right]));
    store_expr_in_variable(variables, variable, expr)?;
    Ok(())
}

fn process_logical_not_variables(
    variables: &mut IndexMap<usize, Expr>,
    operand: &Opr,
    variable: Var,
) -> Result<(), Error> {
    let expr = expr_from_operand(variables, operand)?;
    let expr_negated = expr.negate();
    store_expr_in_variable(variables, variable, expr_negated)?;
    Ok(())
}

fn process_phi_variables(
    blocks_to_control_results: &IndexMap<usize, Vec<usize>>,
    variables: &mut IndexMap<usize, Expr>,
    pres: &Vec<(Opr, BlockIdx)>,
    variable: Var,
) -> Result<(), Error> {
    let mut exprs = vec![];
    let mut this_phis = vec![];
    for (operand, block_id) in pres {
        let expr = expr_from_operand(variables, operand)?;
        this_phis.push((expr.clone(), *block_id));

        let control_results = blocks_to_control_results
            .get(*block_id)
            .cloned()
            .unwrap_or_default();

        for result_id in control_results {
            exprs.push(Expr::Bool(BoolExpr::Result(result_id)));
        }

        exprs.push(expr);
    }

    let expr = Expr::Rich(RichExpr::function_of(exprs));
    store_expr_in_variable(variables, variable, expr)?;

    Ok(())
}

fn process_icmp_variables(
    variables: &mut IndexMap<usize, Expr>,
    condition_code: IcmpCondition,
    operand: &Opr,
    operand1: &Opr,
    variable: Var,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(variables, operand)?;
    let expr_right = expr_from_operand(variables, operand1)?;
    let expr = match condition_code {
        IcmpCondition::Eq => eq_expr(expr_left, expr_right)?,
        IcmpCondition::Ne => eq_expr(expr_left, expr_right)?.negate(),
        IcmpCondition::Slt => Expr::Bool(BoolExpr::BinOp(
            expr_left.into(),
            expr_right.into(),
            ComparisonOp::Lt,
        )),
        IcmpCondition::Sgt => Expr::Bool(BoolExpr::BinOp(
            expr_left.into(),
            expr_right.into(),
            ComparisonOp::Gt,
        )),
        IcmpCondition::Sle => Expr::Bool(BoolExpr::BinOp(
            expr_left.into(),
            expr_right.into(),
            ComparisonOp::Le,
        )),
        IcmpCondition::Sge => Expr::Bool(BoolExpr::BinOp(
            expr_left.into(),
            expr_right.into(),
            ComparisonOp::Ge,
        )),
    };
    store_expr_in_variable(variables, variable, expr)?;
    Ok(())
}

fn process_fcmp_variables(
    variables: &mut IndexMap<usize, Expr>,
    condition_code: FcmpCondition,
    operand: &Opr,
    operand1: &Opr,
    variable: Var,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(variables, operand)?;
    let expr_right = expr_from_operand(variables, operand1)?;
    let expr = match condition_code {
        FcmpCondition::False => BoolExpr::LiteralBool(false),
        FcmpCondition::True => BoolExpr::LiteralBool(true),
        FcmpCondition::OrderedAndEqual | FcmpCondition::UnorderedOrEqual => {
            BoolExpr::BinOp(expr_left.into(), expr_right.into(), ComparisonOp::Eq)
        }
        FcmpCondition::OrderedAndNotEqual | FcmpCondition::UnorderedOrNotEqual => {
            BoolExpr::BinOp(expr_left.into(), expr_right.into(), ComparisonOp::Ne)
        }
        FcmpCondition::OrderedAndLessThan | FcmpCondition::UnorderedOrLessThan => {
            BoolExpr::BinOp(expr_left.into(), expr_right.into(), ComparisonOp::Lt)
        }
        FcmpCondition::OrderedAndGreaterThan | FcmpCondition::UnorderedOrGreaterThan => {
            BoolExpr::BinOp(expr_left.into(), expr_right.into(), ComparisonOp::Gt)
        }
        FcmpCondition::OrderedAndLessThanOrEqual | FcmpCondition::UnorderedOrLessThanOrEqual => {
            BoolExpr::BinOp(expr_left.into(), expr_right.into(), ComparisonOp::Le)
        }
        FcmpCondition::OrderedAndGreaterThanOrEqual
        | FcmpCondition::UnorderedOrGreaterThanOrEqual => {
            BoolExpr::BinOp(expr_left.into(), expr_right.into(), ComparisonOp::Ge)
        }
        FcmpCondition::Ordered | FcmpCondition::Unordered => {
            // These don't map to a simple binary comparison; treat as opaque.
            return store_expr_in_variable(
                variables,
                variable,
                Expr::Rich(RichExpr::function_of([expr_left, expr_right])),
            );
        }
    };
    store_expr_in_variable(variables, variable, Expr::Bool(expr))?;
    Ok(())
}

fn eq_expr(expr_left: Expr, expr_right: Expr) -> Result<Expr, Error> {
    Ok(match (expr_left, expr_right) {
        (Expr::Bool(BoolExpr::LiteralBool(b1)), Expr::Bool(BoolExpr::LiteralBool(b2))) => {
            Expr::Bool(BoolExpr::LiteralBool(b1 == b2))
        }
        (Expr::Bool(BoolExpr::Result(r)), Expr::Bool(BoolExpr::LiteralBool(b)))
        | (Expr::Bool(BoolExpr::LiteralBool(b)), Expr::Bool(BoolExpr::Result(r))) => {
            if b {
                Expr::Bool(BoolExpr::Result(r))
            } else {
                Expr::Bool(BoolExpr::NotResult(r))
            }
        }
        (Expr::Bool(BoolExpr::Result(left)), Expr::Bool(BoolExpr::Result(right))) => {
            Expr::Bool(BoolExpr::TwoResultCondition {
                results: (left, right),
                filter: (true, false, false, true), // 00 and 11
            })
        }
        (left, right) => Expr::Bool(BoolExpr::BinOp(left.into(), right.into(), ComparisonOp::Eq)),
    })
}

fn expr_from_operand(variables: &IndexMap<usize, Expr>, operand: &Opr) -> Result<Expr, Error> {
    match operand {
        Opr::Literal(literal) => match literal {
            Lit::Result(r) => Ok(Expr::Bool(BoolExpr::Result(*r as usize))),
            Lit::Bool(b) => Ok(Expr::Bool(BoolExpr::LiteralBool(*b))),
            Lit::Integer(i) => Ok(Expr::Rich(RichExpr::Literal(i.to_string()))),
            Lit::Double(d) => Ok(Expr::Rich(RichExpr::Literal(d.to_string()))),
            _ => Err(Error::UnsupportedFeature(format!(
                "unsupported literal operand: {literal:?}"
            ))),
        },
        Opr::Variable(variable) => expr_for_variable(variables, variable.id).cloned(),
    }
}

#[derive(Default)]
/// Keeps track of the relationships between variables and measurement results,
/// so that when we later trace instructions that use those variables,
/// we can determine which control results they depend on and thus which operations
/// should be classically controlled by those results.
struct VariableTracker {
    variables: IndexMap<usize, Expr>,
    blocks_to_control_results: IndexMap<usize, Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Rich(RichExpr),
    Bool(BoolExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparisonOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

impl ComparisonOp {
    fn negate(self) -> Self {
        match self {
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::Lt => Self::Ge,
            Self::Gt => Self::Le,
            Self::Le => Self::Gt,
            Self::Ge => Self::Lt,
        }
    }
}

impl Display for ComparisonOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eq => write!(f, "=="),
            Self::Ne => write!(f, "!="),
            Self::Lt => write!(f, "<"),
            Self::Gt => write!(f, ">"),
            Self::Le => write!(f, "<="),
            Self::Ge => write!(f, ">="),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum BoolExpr {
    Result(usize),
    NotResult(usize),
    TwoResultCondition {
        results: (usize, usize),
        // 00, 01, 10, 11
        filter: (bool, bool, bool, bool),
    },
    LiteralBool(bool),
    BinOp(Box<Expr>, Box<Expr>, ComparisonOp),
}

/// These could be of type boolean, we just don't necessary know
/// when they get complex. We could keep track, though it's probably
/// not necessary at this point.
#[derive(Debug, Clone, PartialEq)]
enum RichExpr {
    Literal(String),
    FunctionOf(Vec<usize>), // catch-all for complex expressions
}

impl RichExpr {
    /// Creates a `FunctionOf` from expressions, collecting their linked
    /// measurement result IDs into a sorted, deduplicated list.
    fn function_of(exprs: impl IntoIterator<Item = Expr>) -> Self {
        let mut results: Vec<usize> = exprs.into_iter().flat_map(|e| e.linked_results()).collect();
        results.sort_unstable();
        results.dedup();
        RichExpr::FunctionOf(results)
    }
}

impl Expr {
    fn negate(&self) -> Expr {
        match self {
            Expr::Bool(BoolExpr::Result(r)) => Expr::Bool(BoolExpr::NotResult(*r)),
            Expr::Bool(BoolExpr::NotResult(r)) => Expr::Bool(BoolExpr::Result(*r)),
            Expr::Bool(BoolExpr::LiteralBool(b)) => Expr::Bool(BoolExpr::LiteralBool(!b)),
            Expr::Bool(BoolExpr::TwoResultCondition { results, filter }) => {
                let (f00, f01, f10, f11) = filter;
                Expr::Bool(BoolExpr::TwoResultCondition {
                    results: *results,
                    filter: (!f00, !f01, !f10, !f11),
                })
            }
            Expr::Bool(BoolExpr::BinOp(left, right, op)) => {
                Expr::Bool(BoolExpr::BinOp(left.clone(), right.clone(), op.negate()))
            }
            expr => Expr::Rich(RichExpr::function_of([expr.clone()])),
        }
    }

    fn flat_exprs(&self) -> Vec<Expr> {
        match self {
            Expr::Rich(rich_expr) => match rich_expr {
                RichExpr::Literal(_) | RichExpr::FunctionOf(_) => vec![self.clone()],
            },
            Expr::Bool(condition_expr) => match condition_expr {
                BoolExpr::Result(_) | BoolExpr::NotResult(_) | BoolExpr::LiteralBool(_) => {
                    vec![self.clone()]
                }
                BoolExpr::TwoResultCondition { .. } => vec![self.clone()],
                BoolExpr::BinOp(condition_expr, condition_expr1, _) => condition_expr
                    .flat_exprs()
                    .into_iter()
                    .chain(condition_expr1.flat_exprs())
                    .collect(),
            },
        }
    }

    fn linked_results(&self) -> Vec<usize> {
        match self {
            Expr::Rich(rich_expr) => match rich_expr {
                RichExpr::Literal(_) => vec![],
                RichExpr::FunctionOf(results) => results.clone(),
            },
            Expr::Bool(condition_expr) => match condition_expr {
                BoolExpr::Result(result_id) | BoolExpr::NotResult(result_id) => {
                    vec![*result_id]
                }
                BoolExpr::TwoResultCondition { results, .. } => {
                    vec![results.0, results.1]
                }
                BoolExpr::LiteralBool(_) => vec![],
                BoolExpr::BinOp(condition_expr, condition_expr1, _) => condition_expr
                    .linked_results()
                    .into_iter()
                    .chain(condition_expr1.linked_results())
                    .collect(),
            },
        }
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Rich(complicated_expr) => match complicated_expr {
                RichExpr::Literal(literal_str) => write!(f, "{literal_str}"),
                RichExpr::FunctionOf(results) => {
                    let results = results.iter().map(|r| format!("c_{r}")).collect::<Vec<_>>();
                    write!(f, "f({})", results.join(", "))
                }
            },
            Expr::Bool(condition_expr) => write!(f, "{condition_expr}"),
        }
    }
}

impl Display for BoolExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoolExpr::Result(r) => write!(f, "c_{r} = |1〉"),
            BoolExpr::NotResult(r) => write!(f, "c_{r} = |0〉"),
            BoolExpr::LiteralBool(true) => write!(f, "true"),
            BoolExpr::LiteralBool(false) => write!(f, "false"),
            BoolExpr::TwoResultCondition {
                results: (result_1, result_2),
                filter,
            } => {
                let (f00, f01, f10, f11) = filter;
                let var_name = format!("c_{result_1}c_{result_2}");
                let mut conditions = vec![];
                if *f00 {
                    conditions.push(format!("{var_name} = |00〉"));
                }
                if *f01 {
                    conditions.push(format!("{var_name} = |01〉"));
                }
                if *f10 {
                    conditions.push(format!("{var_name} = |10〉"));
                }
                if *f11 {
                    conditions.push(format!("{var_name} = |11〉"));
                }
                write!(f, "{}", conditions.join(" or "))
            }
            BoolExpr::BinOp(condition_expr, condition_expr1, op) => {
                write!(f, "({condition_expr}) {op} ({condition_expr1})")
            }
        }
    }
}

fn expr_for_variable(
    variables: &IndexMap<usize, Expr>,
    variable_id: VariableIdx,
) -> Result<&Expr, Error> {
    let expr = variables.get(variable_id);
    Ok(expr.unwrap_or_else(|| {
        panic!("variable {variable_id} is not linked to a result or expression")
    }))
}

fn store_expr_in_variable(
    variables: &mut IndexMap<usize, Expr>,
    var: Var,
    expr: Expr,
) -> Result<(), Error> {
    let variable_id = var.id;
    if let Some(old_value) = variables.get(variable_id)
        && old_value != &expr
    {
        panic!("variable {variable_id} already stored {old_value:?}, cannot store {expr:?}");
    }
    if let Expr::Bool(condition_expr) = &expr {
        if let VarTy::Boolean = var.ty {
        } else {
            return Err(Error::UnsupportedFeature(format!(
                "variable {variable_id} has type {var_ty:?} but is being assigned a condition expression: {condition_expr:?}",
                var_ty = var.ty,
            )));
        }
    }

    variables.insert(variable_id, expr);
    Ok(())
}

fn process_call_variables(
    variables: &mut IndexMap<usize, Expr>,
    wire_map_builder: &mut WireMapBuilder,
    callable_name: &str,
    operands: &Vec<Opr>,
    var: Option<Var>,
) -> Result<(), Error> {
    match callable_kind(callable_name) {
        CallableKind::Measurement => {
            let Operands::<'_> {
                name,
                control_qubits,
                target_results,
                ..
            } = callable_spec(variables, callable_name, operands)?
                .expect("measurement should have a signature");

            if control_qubits.len() != 1 {
                return Err(Error::UnsupportedFeature(format!(
                    "a measurement must have exactly one control qubit, found {} in {}",
                    control_qubits.len(),
                    name
                )));
            }
            if target_results.len() != 1 {
                return Err(Error::UnsupportedFeature(format!(
                    "a measurement must have exactly one target result, found {} in {}",
                    target_results.len(),
                    name
                )));
            }
            let qubit = control_qubits[0];
            let result = target_results[0];
            wire_map_builder.link_result_to_qubit(qubit, result);
        }
        CallableKind::Readout => match callable_name {
            "__quantum__rt__read_result" => {
                for operand in operands {
                    match operand {
                        Opr::Literal(Lit::Result(r)) => {
                            let var =
                                var.expect("read_result must have a variable to store the result");
                            store_expr_in_variable(
                                variables,
                                var,
                                Expr::Bool(BoolExpr::Result(*r as usize)),
                            )?;
                        }
                        operand => {
                            return Err(Error::UnsupportedFeature(format!(
                                "operand for result readout is not a result: {operand:?}"
                            )));
                        }
                    }
                }
            }
            name => {
                return Err(Error::UnsupportedFeature(format!(
                    "unknown readout intrinsic: {name}"
                )));
            }
        },
        CallableKind::Gate => {
            if let Some(var) = var {
                let exprs: Vec<_> = operands
                    .iter()
                    .map(|o| expr_from_operand(variables, o))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .flat_map(|e| e.flat_exprs())
                    .collect();
                let result_expr = Expr::Rich(RichExpr::function_of(exprs));

                store_expr_in_variable(variables, var, result_expr)?;
            }
        }
        CallableKind::Reset | CallableKind::OutputRecording => {}
    }

    Ok(())
}

fn trace_call(
    variables: &IndexMap<usize, Expr>,
    builder_ctx: &mut BuilderWithRegisterMap<impl OperationReceiver>,
    callable_name: &str,
    operands: &[Opr],
    mut stack: LogicalStack,
) -> Result<(), Error> {
    let kind = callable_kind(callable_name);

    // Get the signature information for known callables. For custom intrinsics, derive
    // them from the actual operands.
    let operands = callable_spec(variables, callable_name, operands)?;

    if let Some(mut operands) = operands {
        let control_results = take(&mut operands.control_results);
        if !control_results.is_empty() {
            // We're going to create a conditional scope and insert it at the end of the stack.
            let location = if let Some(last) = stack.0.last() {
                last.location
            } else {
                LogicalStackEntryLocation::Unknown
            };
            let frame = LogicalStackEntry::new(
                location,
                Scope::ClassicallyControlled {
                    label: format!(
                        "using: {}",
                        control_results
                            .iter()
                            .map(|r| format!("c_{r}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    control_result_ids: control_results,
                },
            );
            stack.0.push(frame);
        }

        match kind {
            CallableKind::Measurement => {
                trace_measurement(builder_ctx, operands.name, operands, stack)?;
            }
            CallableKind::Reset => trace_reset(builder_ctx, operands, stack)?,
            CallableKind::Gate => trace_gate(
                builder_ctx,
                operands.name,
                operands.is_adjoint,
                operands,
                stack,
            )?,
            kind @ (CallableKind::Readout | CallableKind::OutputRecording) => {
                panic!("callable type {kind:?} should not have been classified as a gate");
            }
        }
    } else {
        assert!(
            matches!(kind, CallableKind::Readout | CallableKind::OutputRecording)
                || callable_name == "__quantum__rt__initialize"
        );
    }

    Ok(())
}

fn trace_gate(
    builder_ctx: &mut BuilderWithRegisterMap<impl OperationReceiver>,
    name: &str,
    is_adjoint: bool,
    operands: Operands,
    stack: LogicalStack,
) -> Result<(), Error> {
    let Operands {
        target_qubits,
        control_qubits,
        control_results,
        args,
        ..
    } = operands;
    if target_qubits.is_empty() && control_qubits.is_empty() && control_results.is_empty() {
        // Skip operations without targets or controls.
        // Alternative might be to include these anyway, across all the qubits,
        // or annotated in the circuit in some way.
    } else {
        builder_ctx.builder.gate(
            builder_ctx.wire_map,
            name,
            is_adjoint,
            &GateInputs {
                targets: &target_qubits,
                controls: &control_qubits,
            },
            args,
            stack,
        );
    }
    Ok(())
}

fn trace_reset(
    builder_ctx: &mut BuilderWithRegisterMap<impl OperationReceiver>,
    operands: Operands,
    stack: LogicalStack,
) -> Result<(), Error> {
    let Operands {
        target_qubits,
        control_results,
        ..
    } = operands;
    if !control_results.is_empty() {
        return Err(Error::UnsupportedFeature(
            "reset with dyanmic input".to_owned(),
        ));
    }

    // Should have validated this assumption in match_operands already
    assert_eq!(
        target_qubits.len(),
        1,
        "reset should have exactly one target qubit"
    );

    let qubit = target_qubits[0];
    builder_ctx
        .builder
        .reset(builder_ctx.wire_map, qubit, stack);

    Ok(())
}

fn trace_measurement(
    builder_ctx: &mut BuilderWithRegisterMap<impl OperationReceiver>,
    name: &str,
    operands: Operands,
    stack: LogicalStack,
) -> Result<(), Error> {
    let Operands {
        control_qubits,
        target_results,
        control_results,
        ..
    } = operands;

    // Should have validated these assumptions in match_operands already
    assert!(
        target_results.len() == 1,
        "measurement should have exactly one target result"
    );
    assert_eq!(
        control_qubits.len(),
        1,
        "measurement should have exactly one control qubit"
    );

    if !control_results.is_empty() {
        return Err(Error::UnsupportedFeature(
            "measurement with dynamic input".to_owned(),
        ));
    }

    builder_ctx.builder.measurement(
        builder_ctx.wire_map,
        name,
        control_qubits[0],
        target_results[0],
        stack,
    );

    Ok(())
}

struct GateSpec<'a> {
    name: &'a str,
    operand_types: Vec<OperandType>,
    is_adjoint: bool,
}

impl<'a> GateSpec<'a> {
    fn single_qubit_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::TargetQubit],
            is_adjoint: false,
        }
    }

    fn single_qubit_gate_adjoint(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::TargetQubit],
            is_adjoint: true,
        }
    }

    fn rotation_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::Arg, OperandType::TargetQubit],
            is_adjoint: false,
        }
    }

    fn controlled_gate(name: &'a str, num_controls: usize) -> Self {
        let mut operand_types = vec![];
        for _ in 0..num_controls {
            operand_types.push(OperandType::ControlQubit);
        }
        operand_types.push(OperandType::TargetQubit);
        Self {
            name,
            operand_types,
            is_adjoint: false,
        }
    }

    fn two_qubit_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::TargetQubit, OperandType::TargetQubit],
            is_adjoint: false,
        }
    }

    fn two_qubit_rotation_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![
                OperandType::Arg,
                OperandType::TargetQubit,
                OperandType::TargetQubit,
            ],
            is_adjoint: false,
        }
    }

    fn measurement_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::ControlQubit, OperandType::TargetResult],
            is_adjoint: false,
        }
    }
}

#[allow(clippy::too_many_lines)]
fn callable_spec<'a>(
    variables: &IndexMap<usize, Expr>,
    callable_name: &'a str,
    operands: &[Opr],
) -> Result<Option<Operands<'a>>, Error> {
    let kind = callable_kind(callable_name);
    if let CallableKind::OutputRecording | CallableKind::Readout = kind {
        // These are not shown as gates in the circuit
        return Ok(None);
    }

    if callable_name == "__quantum__rt__initialize" {
        // This is not shown as a gate in the circuit
        return Ok(None);
    }

    let gate_spec = known_gate_spec(callable_name);

    let gate_spec = if let Some(gate_spec) = gate_spec {
        gate_spec
    } else {
        let mut operand_types = vec![];
        for o in operands {
            match o {
                Opr::Literal(Lit::Integer(_) | Lit::Double(_))
                | Opr::Variable(Var {
                    ty: VarTy::Boolean | VarTy::Integer | VarTy::Double,
                    ..
                }) => {
                    operand_types.push(OperandType::Arg);
                }
                Opr::Literal(Lit::Qubit(_))
                | Opr::Variable(Var {
                    ty: VarTy::Qubit, ..
                }) => {
                    operand_types.push(OperandType::TargetQubit);
                }
                Opr::Literal(Lit::Result(_))
                | Opr::Variable(Var {
                    ty: VarTy::Result, ..
                }) => {
                    operand_types.push(OperandType::TargetResult);
                }
                o => {
                    return Err(Error::UnsupportedFeature(format!(
                        "unsupported operand for custom gate {callable_name}: {o:?}",
                    )));
                }
            }
        }

        GateSpec {
            name: callable_name,
            operand_types,
            is_adjoint: false,
        }
    };

    let mut target_qubits = vec![];
    let mut control_qubits = vec![];
    let mut target_results = vec![];
    let mut control_results = vec![];
    let mut args = vec![];
    if gate_spec.operand_types.len() != operands.len() {
        return Err(Error::UnsupportedFeature(
            "unexpected number of operands for known operation".to_owned(),
        ));
    }
    for (operand, operand_type) in operands.iter().zip(gate_spec.operand_types) {
        match operand {
            Opr::Literal(literal) => match literal {
                Lit::Qubit(q) => {
                    let qubit_operands_array = match operand_type {
                        OperandType::ControlQubit => &mut control_qubits,
                        OperandType::TargetQubit => &mut target_qubits,
                        OperandType::Arg => {
                            return Err(Error::UnsupportedFeature(
                                "qubit operand cannot be an argument".to_owned(),
                            ));
                        }
                        OperandType::TargetResult => {
                            return Err(Error::UnsupportedFeature(
                                "expected result, found qubit".to_owned(),
                            ));
                        }
                    };
                    qubit_operands_array.push(*q as usize);
                }
                Lit::Result(r) => match operand_type {
                    OperandType::TargetResult => {
                        target_results.push(*r as usize);
                    }
                    _ => {
                        return Err(Error::UnsupportedFeature(
                            "unexpected result argument to known callable".to_owned(),
                        ));
                    }
                },
                Lit::Integer(i) => match operand_type {
                    OperandType::Arg => {
                        args.push(i.to_string());
                    }
                    _ => {
                        return Err(Error::UnsupportedFeature(
                            "integer operand where qubit was expected".to_owned(),
                        ));
                    }
                },
                Lit::Double(d) => match operand_type {
                    OperandType::Arg => {
                        args.push(format!("{d:.4}"));
                    }
                    _ => {
                        return Err(Error::UnsupportedFeature(
                            "double operand where qubit was expected".to_owned(),
                        ));
                    }
                },
                l => {
                    return Err(Error::UnsupportedFeature(format!(
                        "unsupported literal operand for unitary operation: {l:?}"
                    )));
                }
            },
            o @ Opr::Variable(var) => {
                if let OperandType::Arg = operand_type {
                    let expr = expr_for_variable(variables, var.id)?.clone();
                    // Add classical controls if this expr is dependent on a result
                    let results = expr.linked_results();
                    for r in results {
                        if !control_results.contains(&r) {
                            control_results.push(r);
                        }
                    }
                    args.push(expr.to_string());
                } else {
                    return Err(Error::UnsupportedFeature(format!(
                        "variable operand cannot be a target or control of a unitary operation: {o:?}"
                    )));
                }
            }
        }
    }

    Ok(Some(Operands {
        name: gate_spec.name,
        is_adjoint: gate_spec.is_adjoint,
        target_qubits,
        control_qubits,
        target_results,
        control_results,
        args,
    }))
}

fn known_gate_spec(callable_name: &str) -> Option<GateSpec<'static>> {
    let name = match callable_name {
        // single-qubit gates
        "__quantum__qis__x__body" => GateSpec::single_qubit_gate("X"),
        "__quantum__qis__y__body" => GateSpec::single_qubit_gate("Y"),
        "__quantum__qis__z__body" => GateSpec::single_qubit_gate("Z"),
        "__quantum__qis__s__body" => GateSpec::single_qubit_gate("S"),
        "__quantum__qis__s__adj" => GateSpec::single_qubit_gate_adjoint("S"),
        "__quantum__qis__sx__body" => GateSpec::single_qubit_gate("SX"),
        "__quantum__qis__t__body" => GateSpec::single_qubit_gate("T"),
        "__quantum__qis__t__adj" => GateSpec::single_qubit_gate_adjoint("T"),
        "__quantum__qis__h__body" => GateSpec::single_qubit_gate("H"),
        "__quantum__qis__rx__body" => GateSpec::rotation_gate("Rx"),
        "__quantum__qis__ry__body" => GateSpec::rotation_gate("Ry"),
        "__quantum__qis__rz__body" => GateSpec::rotation_gate("Rz"),
        // reset gate
        "__quantum__qis__reset__body" => GateSpec::single_qubit_gate("Reset"),
        // multi-qubit gates
        "__quantum__qis__cx__body" => GateSpec::controlled_gate("X", 1),
        "__quantum__qis__cy__body" => GateSpec::controlled_gate("Y", 1),
        "__quantum__qis__cz__body" => GateSpec::controlled_gate("Z", 1),
        "__quantum__qis__ccx__body" => GateSpec::controlled_gate("X", 2),
        "__quantum__qis__rxx__body" => GateSpec::two_qubit_rotation_gate("Rxx"),
        "__quantum__qis__ryy__body" => GateSpec::two_qubit_rotation_gate("Ryy"),
        "__quantum__qis__rzz__body" => GateSpec::two_qubit_rotation_gate("Rzz"),
        "__quantum__qis__swap__body" => GateSpec::two_qubit_gate("SWAP"),
        // measurement gates
        "__quantum__qis__mresetz__body" => GateSpec::measurement_gate("MResetZ"),
        "__quantum__qis__m__body" => GateSpec::measurement_gate("M"),
        _ => return None,
    };
    Some(name)
}

enum OperandType {
    ControlQubit,
    TargetQubit,
    TargetResult,
    Arg,
}

#[derive(Default)]
struct Operands<'a> {
    name: &'a str,
    is_adjoint: bool,
    target_qubits: Vec<usize>,
    control_qubits: Vec<usize>,
    target_results: Vec<usize>,
    control_results: Vec<usize>,
    args: Vec<String>,
}
