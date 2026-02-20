// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use core::panic;
use log::debug;
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::fir::PackageId;
use qsc_partial_eval::{
    Callable, CallableType, ConditionCode, FcmpConditionCode, InstructionKind, Literal, Operand,
    VariableId,
    rir::{Block, BlockId, Instruction, Program, Ty, Variable},
};
use qsc_rir::debug::{
    DbgInfo, DbgLocationId, DbgMetadataScope, DbgScopeId, InstructionDbgMetadata,
};
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::{collections::VecDeque, iter::Peekable, mem::take};
use std::{fmt::Display, vec};

use crate::{
    Circuit, Error, TracerConfig,
    builder::{
        CallableId, GateInputs, LogicalStack, LogicalStackEntry, LogicalStackEntryLocation, LoopId,
        OperationListBuilder, OperationReceiver, PackageOffset, Scope, ScopeStack, SourceLookup,
        WireMap, WireMapBuilder, finish_circuit,
    },
};

#[derive(Clone, Debug)]
struct Branch {
    condition: Variable,
    true_block: BlockId,
    false_block: BlockId,
    instruction_metadata: Option<Box<InstructionDbgMetadata>>,
}

#[derive(Debug, Clone)]
enum Terminator {
    Unconditional(BlockId),
    Conditional(Branch),
    Return,
}

#[must_use]
fn terminator(block: &Block) -> Terminator {
    // Assume that the block is well-formed and that terminators only appear as the last instruction.
    match &block
        .0
        .last()
        .expect("block should have at least one instruction")
    {
        Instruction {
            kind: InstructionKind::Branch(condition, target1, target2),
            metadata,
        } => Terminator::Conditional(Branch {
            condition: *condition,
            true_block: *target1,
            false_block: *target2,
            instruction_metadata: metadata.clone(),
        }),
        Instruction {
            kind: InstructionKind::Jump(target),
            ..
        } => Terminator::Unconditional(*target),
        Instruction {
            kind: InstructionKind::Return,
            ..
        } => Terminator::Return,
        _ => panic!("unexpected terminator kind"),
    }
}

/// A structured AST you can pretty-print back to if/else.
#[derive(Debug, Clone)]
enum StructuredControlFlow {
    Seq(Vec<StructuredControlFlow>),
    /// A single basic block's "payload" (you can expand to instructions later).
    BasicBlock(BlockId),
    If {
        cond: Variable,
        then_br: Box<StructuredControlFlow>,
        else_br: Box<StructuredControlFlow>,
        branch_instruction_metadata: Option<Box<InstructionDbgMetadata>>,
    },
    Return,
}

/// ---- Graph helpers ----
/// A block either:
/// - jumps to one next block,
/// - splits into two paths (if/else),
/// - or finishes (return).
fn next_blocks(block: &Block) -> Vec<BlockId> {
    match terminator(block) {
        Terminator::Unconditional(t) => vec![t],
        Terminator::Conditional(br) => vec![br.true_block, br.false_block],
        Terminator::Return => vec![],
    }
}

/// Find the one final "finish" block (Return).
fn find_return_block(blocks: &IndexMap<BlockId, Block>) -> BlockId {
    let mut returns = blocks
        .iter()
        .filter_map(|(id, b)| matches!(terminator(b), Terminator::Return).then_some(id))
        .collect::<Vec<_>>();

    assert_eq!(returns.len(), 1, "expected exactly 1 Return block");
    returns.pop().expect("just checked non-empty")
}

/// Produce an order where every block appears before anything it can jump to.
/// (This works because you said there are no cycles.)
fn execution_order(blocks: &IndexMap<BlockId, Block>) -> Vec<BlockId> {
    // Count how many incoming edges each block has.
    let mut incoming_count: FxHashMap<BlockId, usize> = FxHashMap::default();
    for id in blocks.iter().map(|(k, _)| k) {
        incoming_count.insert(id, 0);
    }

    for (id, b) in blocks.iter() {
        for nxt in next_blocks(b) {
            *incoming_count.get_mut(&nxt).expect("missing successor") += 1;
        }
        let _ = id;
    }

    // Start with blocks that have no incoming edges.
    let mut ready: VecDeque<BlockId> = incoming_count
        .iter()
        .filter_map(|(id, n)| (*n == 0).then_some(*id))
        .collect();

    // Optional: keep deterministic ordering.
    {
        let mut v: Vec<_> = ready.drain(..).collect();
        v.sort();
        ready.extend(v);
    }

    let mut ordered = Vec::with_capacity(blocks.iter().count());

    while let Some(bid) = ready.pop_front() {
        ordered.push(bid);

        let b = blocks.get(bid).expect("missing block");
        for nxt in next_blocks(b) {
            let n = incoming_count.get_mut(&nxt).expect("missing successor");
            *n -= 1;
            if *n == 0 {
                ready.push_back(nxt);
            }
        }

        // Optional: keep deterministic ordering.
        if ready.len() > 1 {
            let mut v: Vec<_> = ready.drain(..).collect();
            v.sort();
            ready.extend(v);
        }
    }

    assert_eq!(
        ordered.len(),
        blocks.iter().count(),
        "graph has a cycle or inconsistent edges"
    );

    ordered
}

/// ---- "Must reach" sets (used to find merge points) ----
///
/// For each block b, compute the set of blocks that are guaranteed to happen
/// after b on the way to the final return.
///
/// This is the key trick for turning a split (if/else) into a clean structured
/// region with a well-defined merge point.
///
/// Rules:
/// - The return block must reach itself.
/// - If b unconditionally jumps to n, then b must reach everything n must reach.
/// - If b conditionally jumps to t/f, then b must reach only what BOTH branches
///   must reach (intersection).
fn compute_must_reach_sets(
    blocks: &IndexMap<BlockId, Block>,
    return_block: BlockId,
    ordered: &[BlockId],
) -> FxHashMap<BlockId, FxHashSet<BlockId>> {
    // Walk backwards so successors are already computed.
    let mut must_reach: FxHashMap<BlockId, FxHashSet<BlockId>> = FxHashMap::default();

    for &b in ordered.iter().rev() {
        if b == return_block {
            let mut s = FxHashSet::default();
            s.insert(return_block);
            must_reach.insert(b, s);
            continue;
        }

        let succs = next_blocks(blocks.get(b).expect("block should exist"));
        assert!(
            !succs.is_empty(),
            "non-return block must have a next step under your assumptions"
        );

        // Start with the first successor's must_reach set...
        let mut guaranteed = must_reach
            .get(&succs[0])
            .expect("in a DAG, successors appear later in reverse order walk")
            .clone();

        // ...and if there are multiple successors, keep only what's in ALL of them.
        for s in succs.iter().skip(1) {
            let ss = must_reach
                .get(s)
                .expect("in a DAG, successors appear later in reverse order walk");
            guaranteed.retain(|x| ss.contains(x));
        }

        // A block trivially "must reaches" itself (we include it to simplify joins).
        guaranteed.insert(b);
        must_reach.insert(b, guaranteed);
    }

    must_reach
}

/// Pick the earliest merge point for two paths a and b:
/// - find blocks that both paths are guaranteed to reach
/// - choose the one that happens earliest in the overall forward order
fn earliest_merge_point(
    must_reach: &FxHashMap<BlockId, FxHashSet<BlockId>>,
    order_index: &FxHashMap<BlockId, usize>,
    a: BlockId,
    b: BlockId,
) -> BlockId {
    let sa = must_reach.get(&a).expect("must reach set should exist");
    let sb = must_reach.get(&b).expect("must reach set should exist");

    let mut shared: Vec<BlockId> = sa.intersection(sb).copied().collect();
    assert!(
        !shared.is_empty(),
        "paths should reconverge under your assumptions"
    );

    shared.sort_by_key(|id| order_index[id]);
    shared[0]
}

/// Collect blocks reachable from `start` without stepping through `stop`.
/// Useful if you want to validate that a branch arm is a clean, contained region.
fn reachable_until(
    blocks: &IndexMap<BlockId, Block>,
    start: BlockId,
    stop: BlockId,
) -> FxHashSet<BlockId> {
    let mut seen = FxHashSet::default();
    let mut stack = vec![start];

    while let Some(n) = stack.pop() {
        if n == stop || seen.contains(&n) {
            continue;
        }
        seen.insert(n);

        for nxt in next_blocks(blocks.get(n).expect("block should exist")) {
            if nxt != stop {
                stack.push(nxt);
            }
        }
    }

    seen
}

/// ---- Build structured if/else ----
///
/// `build_structured(entry, stop_at)` produces a structured AST by:
/// - walking forward normally for straight-line jumps
/// - when it hits a split (conditional), it:
///     1) finds the merge point
///     2) recursively builds the "then" path until the merge
///     3) recursively builds the "else" path until the merge
///     4) continues after the merge
///
/// `stop_at` means "stop before entering this block" (don't include it).
fn build_structured(
    blocks: &IndexMap<BlockId, Block>,
    must_reach: &FxHashMap<BlockId, FxHashSet<BlockId>>,
    order_index: &FxHashMap<BlockId, usize>,
    entry: BlockId,
    stop_at: Option<BlockId>,
) -> StructuredControlFlow {
    let mut statements: Vec<StructuredControlFlow> = Vec::new();
    let mut cur = entry;

    // Safety belt: if something is malformed, don't spin.
    let mut visited_here: FxHashSet<BlockId> = FxHashSet::default();

    loop {
        if let Some(stop) = stop_at
            && cur == stop
        {
            break;
        }
        if !visited_here.insert(cur) {
            // In a clean DAG region we shouldn't re-visit blocks.
            break;
        }

        let blk = blocks.get(cur).expect("block should exist");

        // "Do this block's work"
        statements.push(StructuredControlFlow::BasicBlock(cur));

        match terminator(blk) {
            Terminator::Return => {
                statements.push(StructuredControlFlow::Return);
                break;
            }

            Terminator::Unconditional(next) => {
                cur = next;
            }

            Terminator::Conditional(br) => {
                let merge =
                    earliest_merge_point(must_reach, order_index, br.true_block, br.false_block);

                // Optional: region sanity checks / debugging
                let _then_region = reachable_until(blocks, br.true_block, merge);
                let _else_region = reachable_until(blocks, br.false_block, merge);

                let then_ast =
                    build_structured(blocks, must_reach, order_index, br.true_block, Some(merge));
                let else_ast =
                    build_structured(blocks, must_reach, order_index, br.false_block, Some(merge));

                statements.push(StructuredControlFlow::If {
                    cond: br.condition,
                    then_br: Box::new(then_ast),
                    else_br: Box::new(else_ast),
                    branch_instruction_metadata: br.instruction_metadata.clone(),
                });

                // After both paths, continue from the merge point.
                cur = merge;
            }
        }
    }

    if statements.len() == 1 {
        statements.pop().expect("just checked non-empty")
    } else {
        StructuredControlFlow::Seq(statements)
    }
}

/// RIR blocks -> Structured Control Flow
/// TODO: other naming suggestions: decompile, build postdominator tree, etc
fn reconstruct_control_flow(
    blocks: &IndexMap<BlockId, Block>,
    entry: BlockId,
) -> StructuredControlFlow {
    let return_block = find_return_block(blocks);
    let ordered = execution_order(blocks);

    let topo_index: FxHashMap<BlockId, usize> =
        ordered.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let must_reach = compute_must_reach_sets(blocks, return_block, &ordered);

    build_structured(blocks, &must_reach, &topo_index, entry, None)
}

pub fn make_circuit(
    program_rir: &Program,
    config: TracerConfig,
    user_package_ids: &[PackageId],
    source_lookup: &impl SourceLookup,
) -> std::result::Result<Circuit, Error> {
    let entry_block_id = program_rir
        .callables
        .get(program_rir.entry)
        .expect("entry callable should exist")
        .body
        .expect("entry callable should have a body");
    let structured_control_flow = reconstruct_control_flow(&program_rir.blocks, entry_block_id);

    let num_qubits = program_rir
        .num_qubits
        .try_into()
        .expect("number of qubits should fit into usize");
    let mut wire_map_builder = FixedQubitRegisterMapBuilder::new(num_qubits);

    let mut builder = OperationListBuilder::new(
        100,
        user_package_ids.to_vec(),
        config.group_by_scope,
        config.source_locations,
    );

    let mut program_map = ProgramMap {
        variables: IndexMap::default(),
        blocks_to_control_results: IndexMap::default(),
    };

    build_operation_list(
        &mut program_map,
        program_rir,
        &mut wire_map_builder,
        &mut builder,
        &structured_control_flow,
        &[],
        &ScopeStack::top(),
    )?;

    let qubits = wire_map_builder.into_wire_map().to_qubits(source_lookup);
    let operations = builder.into_operations();
    let circuit = finish_circuit(source_lookup, operations, qubits, config.group_by_scope);

    Ok(circuit)
}

fn build_operation_list(
    program_map: &mut ProgramMap,
    program_rir: &Program,
    wire_map_builder: &mut FixedQubitRegisterMapBuilder,
    op_list_builder: &mut impl OperationReceiver,
    ast: &StructuredControlFlow,
    control_results: &[usize],
    current_stack: &ScopeStack,
) -> Result<(), Error> {
    match ast {
        StructuredControlFlow::Seq(items) => {
            for item in items {
                build_operation_list(
                    program_map,
                    program_rir,
                    wire_map_builder,
                    op_list_builder,
                    item,
                    control_results,
                    current_stack,
                )?;
            }
        }
        StructuredControlFlow::BasicBlock(id) => {
            let block = program_rir.blocks.get(*id).expect("block should exist");

            assert!(
                !program_map.blocks_to_control_results.contains_key(*id),
                "block should only be processed once"
            );
            program_map
                .blocks_to_control_results
                .insert(*id, control_results.to_vec());

            push_operations_in_block(
                op_list_builder,
                program_map,
                wire_map_builder,
                &program_rir.dbg_info,
                &program_rir.callables,
                block,
                current_stack,
            )?;
        }
        StructuredControlFlow::If {
            cond,
            then_br,
            else_br,
            branch_instruction_metadata,
        } => {
            let dbg_stuff = DbgStuff {
                dbg_info: &program_rir.dbg_info,
            };

            let expr = expr_for_variable(&program_map.variables, cond.variable_id)?;

            let mut control_results = control_results.to_vec();
            for r in expr.linked_results() {
                if !control_results.contains(&r) {
                    control_results.push(r);
                }
            }

            let cond_expr_true = format!("if: {expr}");
            let cond_expr_false = format!("if: {}", expr.negate());

            let caller =
                dbg_stuff.map_instruction_logical_stack(branch_instruction_metadata.as_deref());

            let new_stack_true = combine_branch_instr_stack_with_current_stack(
                current_stack,
                &caller,
                cond_expr_true,
                true,
                control_results.clone(),
            );
            let new_stack_false = combine_branch_instr_stack_with_current_stack(
                current_stack,
                &caller,
                cond_expr_false,
                false,
                control_results.clone(),
            );

            build_operation_list(
                program_map,
                program_rir,
                wire_map_builder,
                op_list_builder,
                then_br,
                &control_results,
                &new_stack_true,
            )?;

            build_operation_list(
                program_map,
                program_rir,
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

#[allow(clippy::too_many_arguments)]
fn push_operations_in_block(
    builder: &mut impl OperationReceiver,
    state: &mut ProgramMap,
    wire_map_builder: &mut FixedQubitRegisterMapBuilder,
    dbg_info: &DbgInfo,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    block: &Block,
    current_stack: &ScopeStack,
) -> Result<(), Error> {
    let mut terminator = None;
    let dbg_stuff = DbgStuff { dbg_info };

    for instruction in &block.0 {
        get_operations_for_instruction_vars_only(
            &mut state.variables,
            wire_map_builder,
            callables,
            &state.blocks_to_control_results,
            instruction,
        )?;

        let new_terminator = push_operations_for_instruction(
            state,
            BuilderWithRegisterMap {
                builder,
                wire_map: wire_map_builder.wire_map(),
            },
            &dbg_stuff,
            callables,
            instruction,
            current_stack,
        )?;

        if let Some(new_terminator) = new_terminator {
            let old = terminator.replace(new_terminator);
            assert!(
                old.is_none(),
                "did not expect more than one unconditional successor for block, old: {old:?} new: {terminator:?}"
            );
        }
    }

    Ok(())
}

pub(crate) struct DbgStuff<'a> {
    dbg_info: &'a DbgInfo,
}

impl DbgStuff<'_> {
    fn map_instruction_logical_stack(
        &self,
        metadata: Option<&InstructionDbgMetadata>,
    ) -> LogicalStack {
        metadata
            .map(|md| md.dbg_location)
            .map(|dbg_location| self.instruction_logical_stack(dbg_location))
            .unwrap_or_default()
    }

    /// Returns oldest->newest
    fn instruction_logical_stack(&self, dbg_location_idx: DbgLocationId) -> LogicalStack {
        let mut location_stack = vec![];
        let mut current_location_idx = Some(dbg_location_idx);

        while let Some(location_idx) = current_location_idx {
            let scope_id = self.lexical_scope(location_idx);
            let package_offset = self.source_location(location_idx);
            match &self.dbg_info.get_scope(scope_id) {
                DbgMetadataScope::SubProgram { name, location } => {
                    let scope = Scope::Callable(CallableId::Source(
                        PackageOffset {
                            package_id: location.package_id.into(),
                            offset: location.offset,
                        },
                        name.clone(),
                    ));
                    location_stack.push(LogicalStackEntry::new_call_site(package_offset, scope));
                }
                DbgMetadataScope::LexicalBlockFile {
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
}

impl DbgStuff<'_> {
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

fn get_operations_for_instruction_vars_only(
    variables: &mut IndexMap<VariableId, Expr>,
    register_map: &mut FixedQubitRegisterMapBuilder,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    blocks_to_control_results: &IndexMap<BlockId, Vec<usize>>,
    instruction: &Instruction,
) -> Result<(), Error> {
    match &instruction.kind {
        InstructionKind::Call(callable_id, operands, var) => {
            process_callable_variables(
                variables,
                register_map,
                callables.get(*callable_id).expect("callable should exist"),
                operands,
                *var,
            )?;
        }
        InstructionKind::Fcmp(condition_code, operand, operand1, variable) => {
            extend_block_with_fcmp_instruction(
                variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        InstructionKind::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        InstructionKind::Phi(pres, variable) => {
            extend_block_with_phi_instruction(
                blocks_to_control_results,
                variables,
                pres,
                *variable,
            )?;
        }
        InstructionKind::Return | InstructionKind::Branch(..) | InstructionKind::Jump(..) => {
            // do nothing for terminators
        }
        InstructionKind::Add(operand, operand1, variable)
        | InstructionKind::Sub(operand, operand1, variable)
        | InstructionKind::Mul(operand, operand1, variable)
        | InstructionKind::Sdiv(operand, operand1, variable)
        | InstructionKind::Srem(operand, operand1, variable)
        | InstructionKind::Shl(operand, operand1, variable)
        | InstructionKind::Ashr(operand, operand1, variable)
        | InstructionKind::Fadd(operand, operand1, variable)
        | InstructionKind::Fsub(operand, operand1, variable)
        | InstructionKind::Fmul(operand, operand1, variable)
        | InstructionKind::Fdiv(operand, operand1, variable)
        | InstructionKind::LogicalAnd(operand, operand1, variable)
        | InstructionKind::LogicalOr(operand, operand1, variable)
        | InstructionKind::BitwiseAnd(operand, operand1, variable)
        | InstructionKind::BitwiseOr(operand, operand1, variable)
        | InstructionKind::BitwiseXor(operand, operand1, variable) => {
            extend_block_with_binop_instruction(variables, operand, operand1, *variable)?;
        }
        instruction @ (InstructionKind::LogicalNot(..) | InstructionKind::BitwiseNot(..)) => {
            // TODO: I'm guessing we need to handle these?
            // Leave the variable unassigned, if it's used in anything that's going to be shown in the circuit, we'll raise an error then
            debug!("ignoring not instruction: {instruction:?}");
        }
        instruction @ InstructionKind::Store(..) => {
            // TODO: who generates these?
            return Err(Error::UnsupportedFeature(format!(
                "unsupported instruction in block: {instruction:?}"
            )));
        }
    }

    Ok(())
}

struct BuilderWithRegisterMap<'a, T: OperationReceiver> {
    builder: &'a mut T,
    wire_map: &'a WireMap,
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn push_operations_for_instruction(
    state: &mut ProgramMap,
    mut builder_ctx: BuilderWithRegisterMap<impl OperationReceiver>,
    dbg_stuff: &DbgStuff,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    instruction: &Instruction,
    current_stack: &ScopeStack,
) -> Result<Option<Terminator>, Error> {
    let mut terminator = None;
    match &instruction.kind {
        InstructionKind::Call(callable_id, operands, _) => {
            let stack = combine_stacks(dbg_stuff, current_stack, instruction.metadata.as_deref());
            trace_call(
                &state.variables,
                &mut builder_ctx,
                callables.get(*callable_id).expect("callable should exist"),
                operands,
                stack,
            )?;
        }
        InstructionKind::Fcmp(condition_code, operand, operand1, variable) => {
            extend_block_with_fcmp_instruction(
                &mut state.variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        InstructionKind::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                &mut state.variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        InstructionKind::Return => {
            // do nothing for terminators
        }
        InstructionKind::Branch(variable, block_id_1, block_id_2) => {
            // this only touches the terminator
            extend_block_with_branch_instruction(
                dbg_stuff.dbg_info,
                &mut terminator,
                instruction,
                *variable,
                *block_id_1,
                *block_id_2,
            )?;
        }
        InstructionKind::Jump(block_id) => {
            // this only touches the terminator
            extend_block_with_jump_instruction(&mut terminator, *block_id)?;
        }
        InstructionKind::Phi(pres, variable) => {
            extend_block_with_phi_instruction(
                &state.blocks_to_control_results,
                &mut state.variables,
                pres,
                *variable,
            )?;
        }
        InstructionKind::Add(operand, operand1, variable)
        | InstructionKind::Sub(operand, operand1, variable)
        | InstructionKind::Mul(operand, operand1, variable)
        | InstructionKind::Sdiv(operand, operand1, variable)
        | InstructionKind::Srem(operand, operand1, variable)
        | InstructionKind::Shl(operand, operand1, variable)
        | InstructionKind::Ashr(operand, operand1, variable)
        | InstructionKind::Fadd(operand, operand1, variable)
        | InstructionKind::Fsub(operand, operand1, variable)
        | InstructionKind::Fmul(operand, operand1, variable)
        | InstructionKind::Fdiv(operand, operand1, variable)
        | InstructionKind::LogicalAnd(operand, operand1, variable)
        | InstructionKind::LogicalOr(operand, operand1, variable)
        | InstructionKind::BitwiseAnd(operand, operand1, variable)
        | InstructionKind::BitwiseOr(operand, operand1, variable)
        | InstructionKind::BitwiseXor(operand, operand1, variable) => {
            extend_block_with_binop_instruction(
                &mut state.variables,
                operand,
                operand1,
                *variable,
            )?;
        }
        InstructionKind::LogicalNot(operand, variable) => {
            extend_block_with_logical_not_instruction(&mut state.variables, operand, *variable)?;
        }
        instruction @ (InstructionKind::Store(..) | InstructionKind::BitwiseNot(..)) => {
            // TODO: what generates this? what should we do?
            return Err(Error::UnsupportedFeature(format!(
                "unsupported instruction in block: {instruction:?}"
            )));
        }
    }

    Ok(terminator)
}

fn combine_stacks(
    dbg_stuff: &DbgStuff<'_>,
    current_incl_branches: &ScopeStack,
    instruction_metadata: Option<&InstructionDbgMetadata>,
) -> LogicalStack {
    let stack = instruction_metadata
        .map(|md| md.dbg_location)
        .map(|dbg_location| dbg_stuff.instruction_logical_stack(dbg_location))
        .unwrap_or_default();

    if current_incl_branches.is_top() {
        // no current stack, just use the instruction metadata stack
        return stack;
    }

    let mut current_iter = current_incl_branches.caller().0.iter().peekable();
    let mut instr_stack_iter = stack.0.iter();
    let mut instr_stack_entry_next = instr_stack_iter.next();

    while let Some(instr_stack_entry) = instr_stack_entry_next
        && let Some(current_entry) = next_real(
            &mut current_iter,
            current_incl_branches.current_lexical_scope(),
        )
    {
        assert!(
            !(current_entry != *instr_stack_entry),
            "instruction stack should match current stack, current_entry: {current_entry:?}, instr_stack_entry: {instr_stack_entry:?}"
        );

        instr_stack_entry_next = instr_stack_iter.next();
    }

    // current stack should have been fully consumed
    // TODO: not sure why this failed - bring it bacK?
    // assert!(
    //     next_real(
    //         &mut current_iter,
    //         current_incl_branches.current_lexical_scope()
    //     )
    //     .is_none(),
    //     "current stack should be fully consumed"
    // );

    // collect the current stack + the rest of instr_stack_iter (could be empty)
    let mut new_stack: Vec<LogicalStackEntry> = vec![];

    new_stack.extend(current_incl_branches.caller().0.iter().cloned());

    new_stack.push(LogicalStackEntry::new(
        LogicalStackEntryLocation::Unknown,
        current_incl_branches.current_lexical_scope().clone(),
    ));

    if let Some(instr_stack_entry) = instr_stack_entry_next {
        // new_stack.push(instr_stack_entry.clone());
        new_stack.last_mut().expect("we just pushed it").location = instr_stack_entry.location;
    } else {
        // panic!("I don't think this can happen?");
        new_stack.last_mut().expect("we just pushed it").location =
            LogicalStackEntryLocation::Unknown;
    }

    for entry in instr_stack_iter {
        new_stack.push(entry.clone());
    }

    LogicalStack(new_stack)
}

fn combine_branch_instr_stack_with_current_stack(
    current_incl_branches: &ScopeStack,
    stack_from_br_instr_md: &LogicalStack,
    scope_label: String,
    branch: bool,
    control_results: Vec<usize>,
) -> ScopeStack {
    let mut new_stack = if current_incl_branches.is_top() {
        // we just take the stack from the branch instruction,
        stack_from_br_instr_md.0.clone()
    } else {
        // if there's anything in the current stack, we must be in a branch.

        assert!(
            matches!(
                current_incl_branches.current_lexical_scope(),
                Scope::ClassicallyControlled { .. }
            ),
            "current scope must be a branch scope"
        );

        let mut current_iter = current_incl_branches.caller().0.iter().peekable();
        let mut br_stack_iter = stack_from_br_instr_md.0.iter();
        let mut branch_instr_stack_entry_next = br_stack_iter.next();

        while let Some(branch_instr_stack_entry) = branch_instr_stack_entry_next
            && let Some(current_entry) = next_real(
                &mut current_iter,
                current_incl_branches.current_lexical_scope(),
            )
        {
            assert!(
                !(current_entry != *branch_instr_stack_entry),
                "branch instruction stack should match current stack"
            );

            branch_instr_stack_entry_next = br_stack_iter.next();
        }

        // current stack should have been fully consumed
        assert!(
            next_real(
                &mut current_iter,
                current_incl_branches.current_lexical_scope()
            )
            .is_none(),
            "current stack should be fully consumed"
        );

        // collect the current stack + the rest of br_stack_iter (could be empty)
        let mut new_stack: Vec<LogicalStackEntry> = vec![];

        new_stack.extend(current_incl_branches.caller().0.iter().cloned());

        new_stack.push(LogicalStackEntry::new(
            LogicalStackEntryLocation::Unknown,
            current_incl_branches.current_lexical_scope().clone(),
        ));

        if let Some(branch_instr_stack_entry) = branch_instr_stack_entry_next {
            // new_stack.push(instr_stack_entry.clone());
            new_stack.last_mut().expect("we just pushed it").location =
                branch_instr_stack_entry.location;
        } else {
            new_stack.last_mut().expect("we just pushed it").location =
                LogicalStackEntryLocation::Unknown;
        }

        for entry in br_stack_iter {
            new_stack.push(entry.clone());
        }
        new_stack
    };

    if let Some(last_mut) = new_stack.last_mut() {
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
        LogicalStack(new_stack),
        Scope::ClassicallyControlled {
            label: scope_label,
            control_result_ids: control_results,
        },
    )
}

fn next_real<'a>(
    stack_iter: &mut Peekable<impl Iterator<Item = &'a LogicalStackEntry>>,
    final_scope: &Scope,
) -> Option<LogicalStackEntry> {
    if let Some(entry) = stack_iter.next() {
        let mut entry = entry.clone();
        while let Some(peek) = stack_iter.peek() {
            if let LogicalStackEntry {
                location,
                scope: Scope::ClassicallyControlled { .. },
            } = peek
            {
                // that means the current entry's location is the branch expression.
                // we don't want that. we will adopt *its* location. and keep our scope.

                // consume the conditional scope entry
                stack_iter.next();

                entry.location = *location;
            } else {
                // next entry is not a ConditionalBranch sentinel; stop adjusting
                break;
            }
        }

        match stack_iter.peek() {
            None => {
                if matches!(final_scope, Scope::ClassicallyControlled { .. }) {
                    // we are at the end, and the final scope is a branch scope,
                    // so we don't want to return this entry
                    return None;
                }
            }
            Some(_) => {
                // there are more entries, so we can return this one
                return Some(entry);
            }
        }
    }
    None
}

fn extend_block_with_binop_instruction(
    variables: &mut IndexMap<VariableId, Expr>,
    operand: &Operand,
    operand1: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(variables, operand)?;
    let expr_right = expr_from_operand(variables, operand1)?;
    let expr = Expr::Rich(RichExpr::FunctionOf(
        [expr_left, expr_right]
            .into_iter()
            .flat_map(|e| e.flat_exprs())
            .collect(),
    ));
    store_expr_in_variable(variables, variable, expr)?;
    Ok(())
}

fn extend_block_with_logical_not_instruction(
    variables: &mut IndexMap<VariableId, Expr>,
    operand: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    let expr = expr_from_operand(variables, operand)?;
    let expr_negated = expr.negate();
    store_expr_in_variable(variables, variable, expr_negated)?;
    Ok(())
}

fn extend_block_with_phi_instruction(
    blocks_to_control_results: &IndexMap<BlockId, Vec<usize>>,
    variables: &mut IndexMap<VariableId, Expr>,
    pres: &Vec<(Operand, BlockId)>,
    variable: Variable,
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

    let expr = Expr::Rich(RichExpr::FunctionOf(exprs));
    store_expr_in_variable(variables, variable, expr)?;

    Ok(())
}

fn extend_block_with_jump_instruction(
    terminator: &mut Option<Terminator>,
    block_id: BlockId,
) -> Result<(), Error> {
    let old = terminator.replace(Terminator::Unconditional(block_id));
    let r = if old.is_some() {
        Err(Error::UnsupportedFeature(
            "block contains more than one terminator".to_owned(),
        ))
    } else {
        Ok(())
    };
    r?;
    Ok(())
}

fn extend_block_with_branch_instruction(
    _dbg_info: &DbgInfo,
    terminator: &mut Option<Terminator>,
    instruction: &Instruction,
    variable: Variable,
    block_id_1: BlockId,
    block_id_2: BlockId,
) -> Result<(), Error> {
    let branch = Branch {
        condition: variable,
        true_block: block_id_1,
        false_block: block_id_2,
        instruction_metadata: instruction.metadata.clone(), // cond_expr_instruction_metadata: instruction_metadata,
    };
    let old = terminator.replace(Terminator::Conditional(branch));
    if old.is_some() {
        return Err(Error::UnsupportedFeature(
            "block contains more than one branch".to_owned(),
        ));
    }
    Ok(())
}

fn extend_block_with_icmp_instruction(
    variables: &mut IndexMap<VariableId, Expr>,
    condition_code: ConditionCode,
    operand: &Operand,
    operand1: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(variables, operand)?;
    let expr_right = expr_from_operand(variables, operand1)?;
    let expr = eq_expr(expr_left, expr_right)?;
    match condition_code {
        ConditionCode::Eq => store_expr_in_variable(variables, variable, expr),
        ConditionCode::Ne => store_expr_in_variable(variables, variable, expr.negate()), // TODO: add a test that exercises the NE condition code
        condition_code => Err(Error::UnsupportedFeature(format!(
            "unsupported condition code in icmp: {condition_code:?}"
        ))),
    }
}

fn extend_block_with_fcmp_instruction(
    variables: &mut IndexMap<VariableId, Expr>,
    condition_code: FcmpConditionCode,
    operand: &Operand,
    operand1: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(variables, operand)?;
    let expr_right = expr_from_operand(variables, operand1)?;
    let expr = match condition_code {
        FcmpConditionCode::False => BoolExpr::LiteralBool(false),
        FcmpConditionCode::True => BoolExpr::LiteralBool(true),
        cmp => BoolExpr::BinOp(expr_left.into(), expr_right.into(), cmp.to_string()),
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
        (left, right) => Expr::Rich(RichExpr::FunctionOf(vec![left, right])),
    })
}

fn expr_from_operand(
    variables: &IndexMap<VariableId, Expr>,
    operand: &Operand,
) -> Result<Expr, Error> {
    match operand {
        Operand::Literal(literal) => match literal {
            Literal::Result(r) => Ok(Expr::Bool(BoolExpr::Result(
                usize::try_from(*r).expect("result id should fit in usize"),
            ))),
            Literal::Bool(b) => Ok(Expr::Bool(BoolExpr::LiteralBool(*b))),
            Literal::Integer(i) => Ok(Expr::Rich(RichExpr::Literal(i.to_string()))),
            Literal::Double(d) => Ok(Expr::Rich(RichExpr::Literal(d.to_string()))),
            _ => Err(Error::UnsupportedFeature(format!(
                "unsupported literal operand: {literal:?}"
            ))),
        },
        Operand::Variable(variable) => expr_for_variable(variables, variable.variable_id).cloned(),
    }
}

struct ProgramMap {
    variables: IndexMap<VariableId, Expr>,
    blocks_to_control_results: IndexMap<BlockId, Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Rich(RichExpr),
    Bool(BoolExpr),
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
    BinOp(Box<Expr>, Box<Expr>, String),
}

/// These could be of type boolean, we just don't necessary know
/// when they get complex. We could keep track, though it's probably
/// not necessary at this point.
#[derive(Debug, Clone, PartialEq)]
enum RichExpr {
    Literal(String),
    FunctionOf(Vec<Expr>), // catch-all for complex expressions
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
            expr => Expr::Rich(RichExpr::FunctionOf(expr.flat_exprs())),
        }
    }

    fn flat_exprs(&self) -> Vec<Expr> {
        match self {
            Expr::Rich(rich_expr) => match rich_expr {
                RichExpr::Literal(_) => vec![self.clone()],
                RichExpr::FunctionOf(exprs) => exprs.iter().flat_map(Expr::flat_exprs).collect(),
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
                RichExpr::FunctionOf(exprs) => {
                    exprs.iter().flat_map(Expr::linked_results).collect()
                }
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
                RichExpr::FunctionOf(exprs) => {
                    let mut results = exprs
                        .iter()
                        .flat_map(Expr::linked_results)
                        .map(|r| format!("c_{r}"))
                        .collect::<Vec<_>>();

                    results.sort();
                    results.dedup();
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
    variables: &IndexMap<VariableId, Expr>,
    variable_id: VariableId,
) -> Result<&Expr, Error> {
    let expr = variables.get(variable_id);
    Ok(expr.unwrap_or_else(|| {
        panic!("variable {variable_id:?} is not linked to a result or expression")
    }))
}

fn store_expr_in_variable(
    variables: &mut IndexMap<VariableId, Expr>,
    var: Variable,
    expr: Expr,
) -> Result<(), Error> {
    let variable_id = var.variable_id;
    if let Some(old_value) = variables.get(variable_id)
        && old_value != &expr
    {
        // TODO: do we ever get here now where we store the same variable twice?
        panic!("variable {variable_id:?} already stored {old_value:?}, cannot store {expr:?}");
    }
    if let Expr::Bool(condition_expr) = &expr {
        if let Ty::Boolean = var.ty {
        } else {
            return Err(Error::UnsupportedFeature(format!(
                "variable {variable_id:?} has type {var_ty:?} but is being assigned a condition expression: {condition_expr:?}",
                var_ty = var.ty,
            )));
        }
    }

    variables.insert(variable_id, expr);
    Ok(())
}

fn process_callable_variables(
    variables: &mut IndexMap<VariableId, Expr>,
    register_map: &mut FixedQubitRegisterMapBuilder,
    callable: &Callable,
    operands: &Vec<Operand>,
    var: Option<Variable>,
) -> Result<(), Error> {
    match callable.call_type {
        CallableType::Measurement => {
            let Operands::<'_> {
                name,
                control_qubits,
                target_results,
                ..
            } = callable_spec(variables, callable, operands)?
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
            register_map.link_result_to_qubit(qubit, result);
        }
        CallableType::Readout => match callable.name.as_str() {
            "__quantum__rt__read_result" => {
                for operand in operands {
                    match operand {
                        Operand::Literal(Literal::Result(r)) => {
                            let var =
                                var.expect("read_result must have a variable to store the result");
                            store_expr_in_variable(
                                variables,
                                var,
                                Expr::Bool(BoolExpr::Result(
                                    usize::try_from(*r).expect("result id should fit in usize"),
                                )),
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
        CallableType::Regular | CallableType::NoiseIntrinsic => {
            if let Some(var) = var {
                let result_expr = Expr::Rich(RichExpr::FunctionOf(
                    operands
                        .iter()
                        .map(|o| expr_from_operand(variables, o))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .flat_map(|e| e.flat_exprs())
                        .collect(),
                ));

                store_expr_in_variable(variables, var, result_expr)?;
            }
        }
        CallableType::Reset | CallableType::OutputRecording => {}
    }

    Ok(())
}

fn trace_call(
    variables: &IndexMap<VariableId, Expr>,
    builder_ctx: &mut BuilderWithRegisterMap<impl OperationReceiver>,
    callable: &Callable,
    operands: &[Operand],
    mut stack: LogicalStack,
) -> Result<(), Error> {
    // Get the signature information for known callables. For custom intrinsics, derive
    // them from the actual operands.
    let operands = callable_spec(variables, callable, operands)?;

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

        match callable.call_type {
            CallableType::Measurement => {
                trace_measurement(builder_ctx, operands.name, operands, stack)?;
            }
            CallableType::Reset => trace_reset(builder_ctx, operands, stack)?,
            CallableType::Regular | CallableType::NoiseIntrinsic => trace_gate(
                builder_ctx,
                operands.name,
                operands.is_adjoint,
                operands,
                stack,
            )?,
            callable_type @ (CallableType::Readout | CallableType::OutputRecording) => {
                panic!("callable type {callable_type} should not have been classified as a gate");
            }
        }
    } else {
        assert!(
            matches!(
                callable.call_type,
                CallableType::Readout | CallableType::OutputRecording
            ) || callable.name == "__quantum__rt__initialize"
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
    variables: &IndexMap<VariableId, Expr>,
    callable: &'a Callable,
    operands: &[Operand],
) -> Result<Option<Operands<'a>>, Error> {
    if let CallableType::OutputRecording | CallableType::Readout = callable.call_type {
        // These are not shown as gates in the circuit
        return Ok(None);
    }

    if &callable.name == "__quantum__rt__initialize" {
        // This is not shown as a gate in the circuit
        return Ok(None);
    }

    let gate_spec = known_gate_spec(&callable.name);

    let gate_spec = if let Some(gate_spec) = gate_spec {
        gate_spec
    } else {
        let mut operand_types = vec![];
        for o in operands {
            match o {
                Operand::Literal(Literal::Integer(_) | Literal::Double(_))
                | Operand::Variable(Variable {
                    ty: Ty::Boolean | Ty::Integer | Ty::Double,
                    ..
                }) => {
                    operand_types.push(OperandType::Arg);
                }
                Operand::Literal(Literal::Qubit(_))
                | Operand::Variable(Variable { ty: Ty::Qubit, .. }) => {
                    operand_types.push(OperandType::TargetQubit);
                }
                Operand::Literal(Literal::Result(_))
                | Operand::Variable(Variable { ty: Ty::Result, .. }) => {
                    operand_types.push(OperandType::TargetResult);
                }
                o => {
                    return Err(Error::UnsupportedFeature(format!(
                        "unsupported operand for custom gate {}: {o:?}",
                        &callable.name
                    )));
                }
            }
        }

        GateSpec {
            name: &callable.name,
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
            Operand::Literal(literal) => match literal {
                Literal::Qubit(q) => {
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
                    qubit_operands_array
                        .push(usize::try_from(*q).expect("qubit id should fit in usize"));
                }
                Literal::Result(r) => match operand_type {
                    OperandType::TargetResult => {
                        target_results
                            .push(usize::try_from(*r).expect("result id should fit in usize"));
                    }
                    _ => {
                        return Err(Error::UnsupportedFeature(
                            "unexpected result argument to known callable".to_owned(),
                        ));
                    }
                },
                Literal::Integer(i) => match operand_type {
                    OperandType::Arg => {
                        args.push(i.to_string());
                    }
                    _ => {
                        return Err(Error::UnsupportedFeature(
                            "integer operand where qubit was expected".to_owned(),
                        ));
                    }
                },
                Literal::Double(d) => match operand_type {
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
            o @ Operand::Variable(var) => {
                if let OperandType::Arg = operand_type {
                    let expr = expr_for_variable(variables, var.variable_id)?.clone();
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

pub(crate) struct FixedQubitRegisterMapBuilder {
    remapper: WireMapBuilder,
}
impl FixedQubitRegisterMapBuilder {
    pub(crate) fn new(num_qubits: usize) -> Self {
        let mut remapper = WireMapBuilder::default();

        for id in 0..num_qubits {
            remapper.map_qubit(id, None); // TODO: source location
        }
        Self { remapper }
    }

    pub(crate) fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        self.remapper.link_result_to_qubit(q, r);
    }

    pub(crate) fn wire_map(&self) -> &WireMap {
        self.remapper.current()
    }

    pub(crate) fn into_wire_map(self) -> WireMap {
        self.remapper.into_wire_map()
    }
}
