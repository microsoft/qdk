// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;
pub(crate) mod tracer;

use core::panic;
use std::{fmt::Display, vec};

use crate::{
    Circuit, ComponentColumn, Error, Ket, Measurement, Operation, Register, TracerConfig, Unitary,
    builder::{
        GateInputs, LexicalScope, OperationOrGroupExt, QubitWire, ResultWire, WireMap,
        add_op_with_grouping, finish_circuit, retain_user_frames,
    },
    circuit::{PackageOffset, SourceLocation},
    rir_to_circuit::tracer::FixedQubitRegisterMapBuilder,
};
use log::{debug, warn};
use qsc_data_structures::{
    debug::{DbgInfo, DbgLocationId, DbgMetadataScope, DbgScopeId, InstructionMetadata},
    index_map::IndexMap,
};
use qsc_frontend::compile::PackageStore;
use qsc_hir::hir::PackageId;
use qsc_partial_eval::{
    Callable, CallableType, ConditionCode, FcmpConditionCode, Instruction, Literal, Operand,
    VariableId,
    rir::{BlockId, BlockWithMetadata, InstructionWithMetadata, Program, Ty, Variable},
};
use rustc_hash::FxHashSet;

#[derive(Clone, Debug)]
struct Branch {
    condition: Variable,
    true_block: BlockId,
    false_block: BlockId,
    metadata: Option<PackageOffset>,
    cond_expr_instruction_metadata: Option<InstructionMetadata>,
}

#[derive(Clone, Debug)]
pub(crate) struct Op {
    kind: OperationKind,
    target_qubits: Vec<QubitWire>,
    control_qubits: Vec<QubitWire>,
    target_results: Vec<ResultWire>,
    control_results: Vec<ResultWire>,
    is_adjoint: bool,
    args: Vec<String>,
    location: Option<DbgLocationId>,
}

impl OperationOrGroupExt for Op {
    type Scope = DbgScopeId;
    type SourceLocation = DbgLocationId;
    type DbgStuff<'a> = DbgStuff<'a>;

    fn all_qubits(&self) -> Vec<QubitWire> {
        let qubits: FxHashSet<QubitWire> = self
            .control_qubits
            .iter()
            .chain(&self.target_qubits)
            .copied()
            .collect();
        qubits.into_iter().collect()
    }

    fn all_results(&self) -> Vec<ResultWire> {
        let results: FxHashSet<ResultWire> = self
            .control_results
            .iter()
            .chain(&self.target_results)
            .copied()
            .collect();
        results.into_iter().collect()
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Self>>
    where
        Self: std::marker::Sized,
    {
        if let OperationKind::Group {
            children: last_scope_children,
            ..
        } = &mut self.kind
        {
            Some(last_scope_children)
        } else {
            None
        }
    }

    fn group(
        scope_stack: ScopeStack<Self::SourceLocation, Self::Scope>,
        children: Vec<Self>,
    ) -> Self
    where
        Self: std::marker::Sized,
    {
        let all_qubits = children
            .iter()
            .flat_map(OperationOrGroupExt::all_qubits)
            .collect::<FxHashSet<QubitWire>>()
            .into_iter()
            .collect::<Vec<QubitWire>>();

        let all_results = children
            .iter()
            .flat_map(OperationOrGroupExt::all_results)
            .collect::<FxHashSet<ResultWire>>()
            .into_iter()
            .collect::<Vec<ResultWire>>();
        Op {
            kind: OperationKind::Group {
                children,
                scope_stack,
                scope_span: None, // to be filled in later
            },
            target_qubits: all_qubits,
            control_qubits: vec![],
            target_results: all_results,
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
            location: None,
        }
    }

    fn extend_target_qubits(&mut self, target_qubits: &[QubitWire]) {
        self.target_qubits.extend(target_qubits);
        self.target_qubits.sort_unstable();
        self.target_qubits.dedup();
    }

    fn extend_target_results(&mut self, target_results: &[ResultWire]) {
        self.target_results.extend(target_results);
        self.target_results.sort_unstable();
        self.target_results.dedup();
    }

    fn scope_stack_if_group(&self) -> Option<&ScopeStack<Self::SourceLocation, Self::Scope>> {
        match &self.kind {
            OperationKind::Group { scope_stack, .. } => Some(scope_stack),
            _ => None,
        }
    }

    fn name(
        &self,
        dbg_stuff: &impl DbgStuffExt<SourceLocation = Self::SourceLocation, Scope = Self::Scope>,
    ) -> String {
        match &self.kind {
            OperationKind::Group { scope_stack, .. } => scope_stack.fmt(dbg_stuff),
            OperationKind::ConditionalGroup { label, .. }
            | OperationKind::Unitary { label }
            | OperationKind::Measurement { label }
            | OperationKind::Ket { label } => label.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScopeStack<SourceLocation, Scope> {
    caller: Vec<SourceLocation>,
    scope: Scope,
}

impl<SourceLocation, Scope> ScopeStack<SourceLocation, Scope>
where
    Scope: std::fmt::Debug + std::fmt::Display + Default + PartialEq,
    SourceLocation: PartialEq + Sized,
{
    pub(crate) fn caller(&self) -> &[SourceLocation] {
        &self.caller
    }

    pub(crate) fn current_lexical_scope(&self) -> &Scope {
        assert!(!self.is_top(), "top scope has no lexical scope");
        &self.scope
    }

    pub(crate) fn is_top(&self) -> bool {
        self.caller.is_empty() && self.scope == Scope::default()
    }

    pub(crate) fn top() -> Self {
        ScopeStack {
            caller: Vec::new(),
            scope: Scope::default(),
        }
    }

    pub(crate) fn resolve_scope(
        &self,
        scope_resolver: &impl ScopeResolver<ScopeId = Scope>,
    ) -> LexicalScope {
        if self.is_top() {
            LexicalScope::top()
        } else {
            scope_resolver.resolve_scope(&self.scope)
        }
    }

    #[allow(dead_code)]
    pub fn fmt(
        &self,
        dbg_stuff: &impl DbgStuffExt<Scope = Scope, SourceLocation = SourceLocation>,
    ) -> String {
        if self.is_top() {
            return "<top>".to_string();
        }

        let call_stack = self.caller();

        let mut names: Vec<String> = call_stack
            .iter()
            .map(|location| fmt_location(dbg_stuff, location))
            .collect();
        names.push(self.current_lexical_scope().to_string());
        names.join("->")
    }
}

fn fmt_location<Scope, SourceLocation>(
    dbg_stuff: &impl DbgStuffExt<Scope = Scope, SourceLocation = SourceLocation>,
    location: &SourceLocation,
) -> String
where
    Scope: Display,
{
    let scope_id = &dbg_stuff.lexical_scope(location);
    format!("{scope_id}@{}", dbg_stuff.source_location(location).offset)
}

#[derive(Clone, Debug)]
enum OperationKind {
    Unitary {
        label: String,
    },
    Measurement {
        label: String,
    },
    Ket {
        label: String,
    },
    Group {
        children: Vec<Op>,
        scope_stack: ScopeStack<DbgLocationId, DbgScopeId>,
        scope_span: Option<PackageOffset>,
    },
    ConditionalGroup {
        children: Vec<Op>,
        label: String,
        scope_span: Option<PackageOffset>,
    },
}

impl Op {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn into_operation(
        self,
        user_package_ids: &[PackageId],
        dbg_stuff: &DbgStuff,
    ) -> Operation {
        let args = self.args.into_iter().collect();

        let targets = self
            .target_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q.0,
                result: None,
            })
            .chain(
                self.target_results
                    .into_iter()
                    .map(|ResultWire(q, r)| Register {
                        qubit: q,
                        result: Some(r),
                    }),
            )
            .collect();
        let controls = self
            .control_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q.0,
                result: None,
            })
            .chain(
                self.control_results
                    .into_iter()
                    .map(|ResultWire(q, r)| Register {
                        qubit: q,
                        result: Some(r),
                    }),
            )
            .collect();

        let source = self
            .location
            .and_then(|l| resolve_location(user_package_ids, dbg_stuff, l))
            .or(
                if let OperationKind::Group { scope_span, .. }
                | OperationKind::ConditionalGroup { scope_span, .. } = &self.kind
                {
                    *scope_span
                } else {
                    None
                },
            )
            .or(
                // fall back to scope span for groups
                if let OperationKind::Group { scope_stack, .. } = &self.kind {
                    Some(scope_stack.resolve_scope(dbg_stuff.dbg_info).location())
                } else {
                    None
                },
            );

        let source = source.map(SourceLocation::Unresolved);

        match self.kind {
            OperationKind::Unitary { label } => Operation::Unitary(Unitary {
                gate: label,
                args,
                children: vec![],
                targets,
                controls,
                is_adjoint: self.is_adjoint,
                source,
            }),
            OperationKind::Measurement { label } => Operation::Measurement(Measurement {
                gate: label,
                args,
                children: vec![],
                qubits: controls,
                results: targets,
                source,
            }),
            OperationKind::Ket { label } => Operation::Ket(Ket {
                gate: label,
                args,
                children: vec![],
                targets,
                source,
            }),
            OperationKind::Group {
                children,
                scope_stack,
                ..
            } => Operation::Unitary(Unitary {
                gate: scope_stack.resolve_scope(dbg_stuff.dbg_info).name(),
                args,
                children: vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(|o| o.into_operation(user_package_ids, dbg_stuff))
                        .collect(),
                }],
                targets,
                controls,
                is_adjoint: false,
                source,
            }),
            OperationKind::ConditionalGroup {
                children, label, ..
            } => Operation::Unitary(Unitary {
                gate: label,
                args,
                children: vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(|o| o.into_operation(user_package_ids, dbg_stuff))
                        .collect(),
                }],
                targets,
                controls,
                is_adjoint: false,
                source,
            }),
        }
    }
    fn has_children(&self) -> bool {
        matches!(&self.kind, OperationKind::Group { children, .. } |  OperationKind::ConditionalGroup { children, .. } if !children.is_empty())
    }
}

pub fn make_circuit(
    program: &Program,
    package_store: &PackageStore,
    config: TracerConfig,
    user_package_ids: &[PackageId],
) -> std::result::Result<Circuit, Error> {
    let mut register_map_builder = FixedQubitRegisterMapBuilder::new(
        usize::try_from(program.num_qubits).expect("number of qubits should fit in usize"),
    );
    let callables = &program.callables;

    let mut variables: IndexMap<VariableId, Expr> = IndexMap::default();
    let mut i = 0;
    let mut done = false;
    while !done {
        let mut blocks: IndexMap<BlockId, CircuitBlock> = IndexMap::default();
        for (id, block) in program.blocks.iter() {
            let block_operations = process_block_vars(
                &program.dbg_info,
                &mut variables,
                &mut register_map_builder,
                callables,
                block,
            )?;
            blocks.insert(id, block_operations);
        }

        done = expand_branches_for_vars(
            register_map_builder.register_map(),
            program,
            &mut variables,
            &blocks,
        )?;
        i += 1;
        if i > 100 {
            warn!("make_circuit: too many iterations expanding branches, giving up");
            return Err(Error::UnsupportedFeature(
                "too many iterations expanding branches".to_owned(),
            ));
        }
    }

    let mut program_map = ProgramMap {
        variables,
        blocks: IndexMap::default(),
    };

    let wire_map = register_map_builder.into_register_map();

    let mut ops_remaining = config.max_operations;

    let dbg_stuff = DbgStuff {
        dbg_info: &program.dbg_info,
    };
    // Do it all again, with all variables properly resolved
    for (id, block) in program.blocks.iter() {
        let block_operations = operations_in_block(
            &mut program_map,
            &wire_map,
            &program.dbg_info,
            callables,
            block,
            ops_remaining,
        )?;

        ops_remaining = ops_remaining.saturating_sub(block_operations.operations.len());

        program_map.blocks.insert(id, block_operations);
    }

    expand_branches(&mut program_map, &wire_map, program)?;

    let entry_block = program
        .callables
        .get(program.entry)
        .expect("entry callable should exist")
        .body
        .expect("entry callable should have a body");

    let entry_block = program_map
        .blocks
        .get(entry_block)
        .expect("entry block should have been processed");

    let operations = extend_with_successors(
        user_package_ids,
        &dbg_stuff,
        &program_map,
        entry_block,
        config,
    );

    let circuit = finish_circuit(
        &wire_map,
        operations
            .into_iter()
            .map(|o| o.into_operation(user_package_ids, &dbg_stuff))
            .collect(),
        Some(package_store),
        config.loop_detection,
        config.collapse_qubit_registers,
    );

    Ok(circuit)
}

fn resolve_location(
    user_package_ids: &[PackageId],
    dbg_stuff: &DbgStuff,
    dbg_location: DbgLocationId,
) -> Option<PackageOffset> {
    let location_stack = dbg_stuff.instruction_logical_stack(dbg_location);
    let stack = retain_user_frames(dbg_stuff, user_package_ids, location_stack);
    stack
        .last()
        .map(|l| dbg_stuff.dbg_info.get_location(*l).location)
        .map(|span| PackageOffset {
            package_id: span.package,
            offset: span.span.lo,
        })
}

/// true result means done
fn expand_branches_for_vars(
    register_map: &WireMap,
    program: &Program,
    variables: &mut IndexMap<VariableId, Expr>,
    blocks: &IndexMap<BlockId, CircuitBlock>,
) -> Result<bool, Error> {
    let mut done = true;
    for (block_id, _) in program.blocks.iter() {
        // TODO: we can just iterate over state.blocks here
        let mut circuit_block = blocks.get(block_id).expect("block should exist").clone();

        if let Some(Terminator::Conditional(branch)) = circuit_block
            .terminator
            .take_if(|t| matches!(t, Terminator::Conditional(_)))
        {
            let expanded_branch =
                expand_branch_vars(variables, blocks, register_map, block_id, &branch)?;

            if let Some(expanded_branch) = expanded_branch {
                let condition_expr =
                    expr_for_variable(variables, branch.condition.variable_id)?.clone();
                // Find the successor and see if it has any phi nodes
                for successor in expanded_branch.successors_to_check_for_phis {
                    let successor_block = blocks
                        .get(successor.block_id)
                        .expect("successor block should exist");

                    let phi_vars = get_phi_vars_from_branch(
                        successor_block,
                        &successor.predecessors,
                        &condition_expr,
                    )?;
                    if let Some(phi_vars) = phi_vars {
                        for (var, expr) in phi_vars {
                            store_expr_in_variable(variables, var, expr)?;
                        }
                    } else {
                        done = false;
                    }
                }
            } else {
                done = false;
            }
        }

        // blocks.insert(block_id, circuit_block);
    }
    Ok(done)
}

// TODO: this could be represented by a circuit block, maybe. Consider.
struct ExpandedBranchBlockVarsOnly {
    successors_to_check_for_phis: Vec<Successor>,
}

// None means more work to be done
fn expand_branch_vars(
    variables: &IndexMap<VariableId, Expr>,
    blocks: &IndexMap<BlockId, CircuitBlock>,
    register_map: &WireMap,
    curent_block_id: BlockId,
    branch: &Branch,
) -> Result<Option<ExpandedBranchBlockVarsOnly>, Error> {
    let cond_expr = expr_for_variable(variables, branch.condition.variable_id)?;
    if cond_expr.is_unresolved() {
        return Ok(None);
    }
    let results = cond_expr.linked_results();

    if let Expr::Bool(BoolExpr::LiteralBool(_)) = cond_expr {
        return Err(Error::UnsupportedFeature(
            "constant condition in branch".to_owned(),
        ));
    }

    if results.is_empty() {
        return Ok(None);
    }

    let branch_block = make_simple_branch_block(
        blocks,
        cond_expr,
        curent_block_id,
        branch.true_block,
        branch.false_block,
    )?;
    let ConditionalBlock {
        operations: true_operations,
        targets: true_targets,
    } = branch_block.true_block;

    let control_results = results
        .iter()
        .map(|r| register_map.result_wire(*r))
        .collect::<Vec<_>>();
    let true_container = make_group_op_vars_only(&true_operations, &control_results);

    let false_container = branch_block.false_block.map(
        |ConditionalBlock {
             operations: false_operations,
             targets: false_targets,
         }| {
            (
                make_group_op_vars_only(&false_operations, &control_results),
                false_targets,
            )
        },
    );

    let true_container = if true_container.children.is_empty() {
        None
    } else {
        Some(true_container)
    };
    let false_container = false_container.and_then(|f| {
        if f.0.children.is_empty() {
            None
        } else {
            Some(f)
        }
    });

    let mut children = vec![];
    let mut target_qubits = vec![];

    if let Some(true_container) = true_container {
        children.push(true_container);
        target_qubits.extend(true_targets);
    }

    if let Some((false_container, false_targets)) = false_container {
        children.push(false_container);
        target_qubits.extend(false_targets);
    }

    // dedup targets
    target_qubits.sort_unstable();
    target_qubits.dedup();
    // TODO: target results for container? measurements in branches?

    Ok(Some(ExpandedBranchBlockVarsOnly {
        successors_to_check_for_phis: [
            branch_block.unconditional_successor,
            branch_block.true_successor,
        ]
        .into_iter()
        .chain(branch_block.false_successor)
        .collect(),
    }))
}

struct VarsOnlyGroupOp {
    children: Vec<()>,
}

fn make_group_op_vars_only(operations: &[Op], control_results: &[ResultWire]) -> VarsOnlyGroupOp {
    let children = operations
        .iter()
        .map(|o| {
            let mut o = o.clone();
            o.control_results.extend(control_results.iter().copied());
            o.control_results.sort_unstable();
            o.control_results.dedup();
        })
        .collect();
    VarsOnlyGroupOp { children }
}

fn make_group_op(
    label: &str,
    operations: &[Op],
    targets: &[QubitWire],
    control_results: &[ResultWire],
) -> Op {
    let children = operations
        .iter()
        .map(|o| {
            let mut o = o.clone();
            o.control_results.extend(control_results.iter().copied());
            o.control_results.sort_unstable();
            o.control_results.dedup();
            o
        })
        .collect();
    Op {
        kind: OperationKind::ConditionalGroup {
            children,
            label: label.into(),
            scope_span: None,
        },
        args: vec![],
        target_qubits: targets.to_vec(),
        control_qubits: vec![],
        target_results: vec![],
        control_results: control_results.to_vec(),
        is_adjoint: false,
        location: None,
    }
}

fn process_block_vars(
    dbg_info: &DbgInfo,
    variables: &mut IndexMap<VariableId, Expr>,
    register_map: &mut FixedQubitRegisterMapBuilder,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    block: &BlockWithMetadata,
) -> Result<CircuitBlock, Error> {
    // TODO: use get_block_successors from utils
    let mut terminator = None;
    let mut phis = vec![];
    let mut done = false;

    for instruction in &block.0 {
        if done {
            return Err(Error::UnsupportedFeature(
                "instructions after return or jump in block".to_owned(),
            ));
        }
        let new_terminator = get_operations_for_instruction_vars_only(
            dbg_info,
            variables,
            register_map,
            callables,
            &mut phis,
            &mut done,
            instruction,
        )?;

        if let Some(new_terminator) = new_terminator {
            let old = terminator.replace(new_terminator);
            assert!(
                old.is_none(),
                "did not expect more than one unconditional successor for block, old: {old:?} new: {terminator:?}"
            );
        }
    }

    Ok(CircuitBlock {
        phis,
        operations: vec![],
        terminator, // TODO: make this exhaustive, and detect corrupt blocks
    })
}

/// Iterates over all the basic blocks in the original program. If a block ends with a conditional branch,
/// the corresponding block in the program map is modified to replace the conditional branch with an unconditional branch,
/// and an operation is added to the block that represents the branch logic (i.e., a unitary operation with two children, one for the true branch and one for the false branch).
fn expand_branches(
    state: &mut ProgramMap,
    register_map: &WireMap,
    program: &Program,
) -> Result<(), Error> {
    for (block_id, _) in program.blocks.iter() {
        // TODO: we can just iterate over state.blocks here
        let mut circuit_block = state
            .blocks
            .get(block_id)
            .expect("block should exist")
            .clone();

        if let Some(Terminator::Conditional(branch)) = circuit_block
            .terminator
            .take_if(|t| matches!(t, Terminator::Conditional(_)))
        {
            let expanded_branch = expand_branch(state, register_map, block_id, &branch)?;

            let add = match &expanded_branch.grouped_operation.kind {
                OperationKind::Group { children, .. }
                | OperationKind::ConditionalGroup { children, .. } => !children.is_empty(),
                _ => false,
            };

            if add {
                // don't add operations for empty branches
                circuit_block
                    .operations
                    .push(expanded_branch.grouped_operation);
            }
            circuit_block.terminator = Some(Terminator::Unconditional(
                expanded_branch.unconditional_successor,
            ));
        }

        state.blocks.insert(block_id, circuit_block);
    }
    Ok(())
}

// None means unresolved, more work to do
fn get_phi_vars_from_branch(
    successor_block: &CircuitBlock,
    predecessors: &[BlockId],
    condition: &Expr,
) -> Result<Option<Vec<(Variable, Expr)>>, Error> {
    let mut done = true;
    let mut phi_vars = vec![];
    for phi in &successor_block.phis {
        let (var, pres) = phi;
        let mut options = vec![];
        for (expr, block_id) in pres {
            // TODO: this is not how it works
            if predecessors.contains(block_id) {
                if options.is_empty() {
                    options.push(condition.clone());
                }
                options.push(expr.clone());
            }
        }

        let rich = combine_exprs(options)?;
        if let Some(rich) = rich {
            phi_vars.push((*var, rich));
        } else {
            done = false;
        }
    }
    if done { Ok(Some(phi_vars)) } else { Ok(None) }
}

// None means unresolved, more work to do
fn combine_exprs(options: Vec<Expr>) -> Result<Option<Expr>, Error> {
    if options.iter().any(Expr::is_unresolved) {
        return Ok(None);
    }

    let e = Expr::Rich(RichExpr::FunctionOf(
        options.into_iter().flat_map(|e| e.flat_exprs()).collect(),
    ));
    Ok(Some(e))
}

fn extend_with_successors(
    user_package_ids: &[PackageId],
    dbg_stuff: &DbgStuff,
    state: &ProgramMap,
    entry_block: &CircuitBlock,
    config: TracerConfig,
) -> Vec<Op> {
    let mut operations = vec![];
    let mut block_stack = vec![entry_block.clone()];

    while let Some(block) = block_stack.pop() {
        // At this point we expect only unconditional successors or none
        if let Some(Terminator::Unconditional(successor_block_id)) = &block.terminator {
            let successor_block = state
                .blocks
                .get(*successor_block_id)
                .expect("successor block should exist");

            block_stack.push(successor_block.clone());
        }

        for op in block.operations {
            if config.group_scopes {
                let op_call_stack = op
                    .location
                    .as_ref()
                    .map(|dbg_location| dbg_stuff.instruction_logical_stack(*dbg_location))
                    .unwrap_or_default();

                add_op_with_grouping(
                    user_package_ids,
                    dbg_stuff,
                    &mut operations,
                    op,
                    op_call_stack,
                );
            } else {
                operations.push(op);
            }
        }
    }
    operations
}

// TODO: this could be represented by a circuit block, maybe. Consider.
struct ExpandedBranchBlock {
    grouped_operation: Op, // TODO: Can only be group
    unconditional_successor: BlockId,
}

fn expand_branch(
    state: &mut ProgramMap,
    register_map: &WireMap,
    curent_block_id: BlockId,
    branch: &Branch,
) -> Result<ExpandedBranchBlock, Error> {
    let cond_expr = expr_for_variable(&state.variables, branch.condition.variable_id)?;
    let results = cond_expr.linked_results();

    if let Expr::Bool(BoolExpr::LiteralBool(_)) = cond_expr {
        return Err(Error::UnsupportedFeature(
            "constant condition in branch".to_owned(),
        ));
    }

    if results.is_empty() {
        return Err(Error::UnsupportedFeature(format!(
            "branching on a condition that doesn't involve at least one result: {cond_expr:?}, {}",
            branch.condition
        )));
    }

    let branch_block = make_simple_branch_block(
        &state.blocks,
        cond_expr,
        curent_block_id,
        branch.true_block,
        branch.false_block,
    )?;

    let ConditionalBlock {
        operations: true_operations,
        targets: true_targets,
    } = branch_block.true_block;

    let control_results = results
        .iter()
        .map(|r| register_map.result_wire(*r))
        .collect::<Vec<_>>();
    let true_container = make_group_op("true", &true_operations, &true_targets, &control_results);

    let false_container = branch_block.false_block.map(
        |ConditionalBlock {
             operations: false_operations,
             targets: false_targets,
         }| {
            (
                make_group_op("false", &false_operations, &false_targets, &control_results),
                false_targets,
            )
        },
    );

    let true_container = if true_container.has_children() {
        Some(true_container)
    } else {
        None
    };
    let false_container =
        false_container.and_then(|f| if f.0.has_children() { Some(f) } else { None });

    let mut children = vec![];
    let mut target_qubits = vec![];

    if let Some(true_container) = true_container {
        children.push(true_container);
        target_qubits.extend(true_targets);
    }

    if let Some((false_container, false_targets)) = false_container {
        children.push(false_container);
        target_qubits.extend(false_targets);
    }

    // dedup targets
    target_qubits.sort_unstable();
    target_qubits.dedup();
    // TODO: target results for container? measurements in branches?

    let args = vec![branch_block.cond_expr.to_string().clone()];
    let label = "check ".to_string();

    let location = branch
        .cond_expr_instruction_metadata
        .as_ref()
        .and_then(|md| md.dbg_location);

    Ok(ExpandedBranchBlock {
        grouped_operation: Op {
            kind: OperationKind::ConditionalGroup {
                children: children.into_iter().collect(),
                scope_span: branch.metadata,
                label,
            },
            target_qubits,
            control_qubits: vec![],
            target_results: vec![],
            control_results: control_results.clone(),
            is_adjoint: false,
            args,
            location,
        },
        unconditional_successor: branch_block.unconditional_successor.block_id,
    })
}

#[derive(Clone, Debug)]
struct CircuitBlock {
    phis: Vec<(Variable, Vec<(Expr, BlockId)>)>,
    // This has to be Op since it may contain logical stacks from dbg metadata
    operations: Vec<Op>,
    terminator: Option<Terminator>,
}

fn operations_in_block(
    state: &mut ProgramMap,
    register_map: &WireMap,
    dbg_info: &DbgInfo,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    block: &BlockWithMetadata,
    ops_remaining: usize,
) -> Result<CircuitBlock, Error> {
    // TODO: use get_block_successors from utils
    let mut terminator = None;
    let mut phis = vec![];
    let mut done = false;

    let mut builder = OpListBuilder::new(ops_remaining);
    for instruction in &block.0 {
        if done {
            return Err(Error::UnsupportedFeature(
                "instructions after return or jump in block".to_owned(),
            ));
        }
        let new_terminator = get_operations_for_instruction(
            state,
            BuilderWithRegisterMap {
                builder: &mut builder,
                register_map,
            },
            dbg_info,
            callables,
            &mut phis,
            &mut done,
            instruction,
        )?;

        if let Some(new_terminator) = new_terminator {
            let old = terminator.replace(new_terminator);
            assert!(
                old.is_none(),
                "did not expect more than one unconditional successor for block, old: {old:?} new: {terminator:?}"
            );
        }
    }

    Ok(CircuitBlock {
        phis,
        operations: builder.into_operations(),
        terminator, // TODO: make this exhaustive, and detect corrupt blocks
    })
}

pub(crate) struct DbgStuff<'a> {
    dbg_info: &'a DbgInfo,
}

impl DbgStuff<'_> {
    /// Returns oldest->youngest
    fn instruction_logical_stack(&self, dbg_location_idx: DbgLocationId) -> Vec<DbgLocationId> {
        let mut location_stack = vec![];
        let mut current_location_idx = Some(dbg_location_idx);

        while let Some(location_idx) = current_location_idx {
            location_stack.push(location_idx);
            let location = self.dbg_info.get_location(location_idx);
            current_location_idx = location.inlined_at;
        }
        location_stack.reverse();
        location_stack
    }
}

pub(crate) trait ScopeResolver {
    type ScopeId;
    fn resolve_scope(&self, scope: &Self::ScopeId) -> LexicalScope;
}

impl ScopeResolver for DbgInfo {
    type ScopeId = DbgScopeId;

    fn resolve_scope(&self, scope_id: &Self::ScopeId) -> LexicalScope {
        match &self.get_scope(*scope_id) {
            DbgMetadataScope::SubProgram { name, location } => LexicalScope::Named {
                name: name.clone(),
                location: PackageOffset {
                    package_id: location.package,
                    offset: location.span.lo,
                },
            },
        }
    }
}

pub(crate) trait DbgStuffExt {
    type SourceLocation: PartialEq + Sized + Clone + PartialEq;
    type Scope: std::fmt::Debug + std::fmt::Display + Default + PartialEq;

    fn package_id(&self, location: &Self::SourceLocation) -> PackageId;
    fn lexical_scope(&self, location: &Self::SourceLocation) -> Self::Scope;
    fn source_location(&self, location: &Self::SourceLocation) -> PackageOffset;

    /// full is a call stack
    /// prefix is a scope stack
    /// if prefix isn't a prefix of full, return None
    /// if it is, return the rest of full after removing prefix,
    /// starting from the first location in full that is in the scope of prefix.scope
    fn strip_scope_stack_prefix(
        &self,
        full_call_stack: &[Self::SourceLocation],
        prefix_scope_stack: &ScopeStack<Self::SourceLocation, Self::Scope>,
    ) -> Option<Vec<Self::SourceLocation>> {
        if prefix_scope_stack.is_top() {
            return Some(full_call_stack.to_vec());
        }

        if full_call_stack.len() > prefix_scope_stack.caller().len() {
            if let Some(rest) = full_call_stack.strip_prefix(prefix_scope_stack.caller()) {
                if self.lexical_scope(&rest[0]) == *prefix_scope_stack.current_lexical_scope() {
                    assert!(!rest.is_empty());
                    return Some(rest.to_vec());
                }
            }
        }
        None
    }

    fn scope_stack(
        &self,
        instruction_stack: &[Self::SourceLocation],
    ) -> ScopeStack<Self::SourceLocation, Self::Scope>
    where
        Self::SourceLocation: Clone,
    {
        instruction_stack
            .split_last()
            .map_or(ScopeStack::top(), |(youngest, prefix)| ScopeStack::<
                Self::SourceLocation,
                Self::Scope,
            > {
                caller: prefix.to_vec(),
                scope: self.lexical_scope(youngest),
            })
    }
}

impl DbgStuffExt for DbgStuff<'_> {
    type SourceLocation = DbgLocationId;
    type Scope = DbgScopeId;

    fn lexical_scope(&self, location: &Self::SourceLocation) -> Self::Scope {
        self.dbg_info.get_location(*location).scope
    }

    fn package_id(&self, location: &Self::SourceLocation) -> PackageId {
        // TODO: I think we have problems here when it comes to entry expr
        match &self.dbg_info.get_scope(self.lexical_scope(location)) {
            DbgMetadataScope::SubProgram { name: _, location } => location.package,
        }
    }

    fn source_location(&self, location: &Self::SourceLocation) -> PackageOffset {
        let dbg_location = self.dbg_info.get_location(*location);
        PackageOffset {
            package_id: dbg_location.location.package,
            offset: dbg_location.location.span.lo,
        }
    }
}

pub(crate) fn add_scoped_op<OG: OperationOrGroupExt>(
    dbg_stuff: &OG::DbgStuff<'_>,
    current_container: &mut Vec<OG>,
    current_scope_stack: &ScopeStack<OG::SourceLocation, OG::Scope>,
    op: OG,
    op_call_stack: &[OG::SourceLocation],
) {
    let op_call_stack_rel = dbg_stuff.strip_scope_stack_prefix(
        op_call_stack,
        current_scope_stack,
    ).expect("op_call_stack_rel should be a suffix of op_call_stack_abs after removing current_scope_stack_abs");

    if !op_call_stack_rel.is_empty() {
        if let Some(last_op) = current_container.last_mut() {
            // See if we can add to the last scope inside the current container
            if let Some(last_scope_stack_abs) = last_op.scope_stack_if_group() {
                if dbg_stuff
                    .strip_scope_stack_prefix(op_call_stack, last_scope_stack_abs)
                    .is_some()
                {
                    let last_scope_stack_abs = last_scope_stack_abs.clone();

                    // The last scope matched, add to it
                    last_op.extend_target_qubits(&op.all_qubits());
                    last_op.extend_target_results(&op.all_results());
                    let last_op_children =
                        last_op.children_mut().expect("operation should be a group");

                    // Recursively add to the children
                    add_scoped_op(
                        dbg_stuff,
                        last_op_children,
                        &last_scope_stack_abs,
                        op,
                        op_call_stack,
                    );

                    return;
                }
            }
        }

        let op_scope_stack = dbg_stuff.scope_stack(op_call_stack);
        if *current_scope_stack != op_scope_stack {
            let scope_group = OG::group(op_scope_stack, vec![op]);

            let parent = op_call_stack
                .split_last()
                .expect("should have more than one etc")
                .1
                .to_vec();
            add_scoped_op(
                dbg_stuff,
                current_container,
                current_scope_stack,
                scope_group,
                &parent,
            );

            return;
        }
    }

    current_container.push(op);
}

#[derive(Debug, Clone)]
enum Terminator {
    Unconditional(BlockId),
    Conditional(Branch),
}

fn get_operations_for_instruction_vars_only(
    dbg_info: &DbgInfo,
    variables: &mut IndexMap<VariableId, Expr>,
    register_map: &mut FixedQubitRegisterMapBuilder,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    phis: &mut Vec<(Variable, Vec<(Expr, BlockId)>)>,
    done: &mut bool,
    instruction: &InstructionWithMetadata,
) -> Result<Option<Terminator>, Error> {
    let mut terminator = None;
    match &instruction.instruction {
        Instruction::Call(callable_id, operands, var) => {
            process_callable_variables(
                variables,
                register_map,
                callables.get(*callable_id).expect("callable should exist"),
                operands,
                *var,
            )?;
        }
        Instruction::Fcmp(condition_code, operand, operand1, variable) => {
            extend_block_with_fcmp_instruction(
                variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instruction::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instruction::Return => {
            *done = true;
        }
        Instruction::Branch(variable, block_id_1, block_id_2) => {
            *done = true;
            extend_block_with_branch_instruction_vars_only(
                dbg_info,
                &mut terminator,
                instruction,
                *variable,
                *block_id_1,
                *block_id_2,
            )?;
        }
        Instruction::Jump(block_id) => {
            extend_block_with_jump_instruction(&mut terminator, *block_id)?;
            *done = true;
        }
        Instruction::Phi(pres, variable) => {
            extend_block_with_phi_instruction(variables, phis, pres, *variable)?;
        }
        Instruction::Add(operand, operand1, variable)
        | Instruction::Sub(operand, operand1, variable)
        | Instruction::Mul(operand, operand1, variable)
        | Instruction::Sdiv(operand, operand1, variable)
        | Instruction::Srem(operand, operand1, variable)
        | Instruction::Shl(operand, operand1, variable)
        | Instruction::Ashr(operand, operand1, variable)
        | Instruction::Fadd(operand, operand1, variable)
        | Instruction::Fsub(operand, operand1, variable)
        | Instruction::Fmul(operand, operand1, variable)
        | Instruction::Fdiv(operand, operand1, variable)
        | Instruction::LogicalAnd(operand, operand1, variable)
        | Instruction::LogicalOr(operand, operand1, variable)
        | Instruction::BitwiseAnd(operand, operand1, variable)
        | Instruction::BitwiseOr(operand, operand1, variable)
        | Instruction::BitwiseXor(operand, operand1, variable) => {
            extend_block_with_binop_instruction(variables, operand, operand1, *variable)?;
        }
        instruction @ (Instruction::LogicalNot(..) | Instruction::BitwiseNot(..)) => {
            // Leave the variable unassigned, if it's used in anything that's going to be shown in the circuit, we'll raise an error then
            debug!("ignoring not instruction: {instruction:?}");
        }
        instruction @ Instruction::Store(..) => {
            return Err(Error::UnsupportedFeature(format!(
                "unsupported instruction in block: {instruction:?}"
            )));
        }
    }

    Ok(terminator)
}

struct BuilderWithRegisterMap<'a> {
    builder: &'a mut OpListBuilder,
    register_map: &'a WireMap,
}

fn get_operations_for_instruction(
    state: &mut ProgramMap,
    mut builder_ctx: BuilderWithRegisterMap,
    dbg_info: &DbgInfo,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    phis: &mut Vec<(Variable, Vec<(Expr, BlockId)>)>,
    done: &mut bool,
    instruction: &InstructionWithMetadata,
) -> Result<Option<Terminator>, Error> {
    let mut terminator = None;
    match &instruction.instruction {
        Instruction::Call(callable_id, operands, _) => {
            trace_call(
                state,
                &mut builder_ctx,
                callables.get(*callable_id).expect("callable should exist"),
                operands,
                instruction.metadata.as_ref(),
            )?;
        }
        Instruction::Fcmp(condition_code, operand, operand1, variable) => {
            extend_block_with_fcmp_instruction(
                &mut state.variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instruction::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                &mut state.variables,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instruction::Return => {
            *done = true;
        }
        Instruction::Branch(variable, block_id_1, block_id_2) => {
            *done = true;
            extend_block_with_branch_instruction(
                dbg_info,
                &mut terminator,
                instruction,
                *variable,
                *block_id_1,
                *block_id_2,
            )?;
        }
        Instruction::Jump(block_id) => {
            extend_block_with_jump_instruction(&mut terminator, *block_id)?;
            *done = true;
        }
        Instruction::Phi(pres, variable) => {
            extend_block_with_phi_instruction(&mut state.variables, phis, pres, *variable)?;
        }
        Instruction::Add(operand, operand1, variable)
        | Instruction::Sub(operand, operand1, variable)
        | Instruction::Mul(operand, operand1, variable)
        | Instruction::Sdiv(operand, operand1, variable)
        | Instruction::Srem(operand, operand1, variable)
        | Instruction::Shl(operand, operand1, variable)
        | Instruction::Ashr(operand, operand1, variable)
        | Instruction::Fadd(operand, operand1, variable)
        | Instruction::Fsub(operand, operand1, variable)
        | Instruction::Fmul(operand, operand1, variable)
        | Instruction::Fdiv(operand, operand1, variable)
        | Instruction::LogicalAnd(operand, operand1, variable)
        | Instruction::LogicalOr(operand, operand1, variable)
        | Instruction::BitwiseAnd(operand, operand1, variable)
        | Instruction::BitwiseOr(operand, operand1, variable)
        | Instruction::BitwiseXor(operand, operand1, variable) => {
            extend_block_with_binop_instruction(
                &mut state.variables,
                operand,
                operand1,
                *variable,
            )?;
        }
        instruction @ (Instruction::LogicalNot(..) | Instruction::BitwiseNot(..)) => {
            // Leave the variable unassigned, if it's used in anything that's going to be shown in the circuit, we'll raise an error then
            debug!("ignoring not instruction: {instruction:?}");
        }
        instruction @ Instruction::Store(..) => {
            return Err(Error::UnsupportedFeature(format!(
                "unsupported instruction in block: {instruction:?}"
            )));
        }
    }

    Ok(terminator)
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

fn extend_block_with_phi_instruction(
    variables: &mut IndexMap<VariableId, Expr>,
    phis: &mut Vec<(Variable, Vec<(Expr, BlockId)>)>,
    pres: &Vec<(Operand, BlockId)>,
    variable: Variable,
) -> Result<(), Error> {
    let mut exprs = vec![];
    let mut this_phis = vec![];
    for (var, label) in pres {
        let expr = expr_from_operand(variables, var)?;
        this_phis.push((expr.clone(), *label));
        exprs.push(expr);
    }
    phis.push((variable, this_phis));

    store_variable_placeholder(variables, variable);

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

fn extend_block_with_branch_instruction_vars_only(
    dbg_info: &DbgInfo,
    terminator: &mut Option<Terminator>,
    instruction: &InstructionWithMetadata,
    variable: Variable,
    block_id_1: BlockId,
    block_id_2: BlockId,
) -> Result<(), Error> {
    let instruction_metadata = instruction.metadata.clone();
    let metadata = instruction_metadata
        .as_ref()
        .and_then(|md| md.dbg_location)
        .map(|l| dbg_info.get_location(l).location)
        .map(|span| PackageOffset {
            package_id: span.package,
            offset: span.span.lo,
        });
    let branch = Branch {
        condition: variable,
        true_block: block_id_1,
        false_block: block_id_2,
        metadata,
        cond_expr_instruction_metadata: instruction_metadata,
    };
    let old = terminator.replace(Terminator::Conditional(branch));
    if old.is_some() {
        return Err(Error::UnsupportedFeature(
            "block contains more than one branch".to_owned(),
        ));
    }
    Ok(())
}

fn extend_block_with_branch_instruction(
    dbg_info: &DbgInfo,
    terminator: &mut Option<Terminator>,
    instruction: &InstructionWithMetadata,
    variable: Variable,
    block_id_1: BlockId,
    block_id_2: BlockId,
) -> Result<(), Error> {
    let instruction_metadata = instruction.metadata.clone();
    let metadata = instruction_metadata
        .as_ref()
        .and_then(|md| md.dbg_location)
        .map(|l| dbg_info.get_location(l).location)
        .map(|span| PackageOffset {
            package_id: span.package,
            offset: span.span.lo,
        });
    let branch = Branch {
        condition: variable,
        true_block: block_id_1,
        false_block: block_id_2,
        metadata,
        cond_expr_instruction_metadata: instruction_metadata,
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
    match condition_code {
        ConditionCode::Eq => {
            let expr_left = expr_from_operand(variables, operand)?;
            let expr_right = expr_from_operand(variables, operand1)?;
            let expr = eq_expr(expr_left, expr_right)?;
            store_expr_in_variable(variables, variable, Expr::Bool(expr))
        }
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

fn eq_expr(expr_left: Expr, expr_right: Expr) -> Result<BoolExpr, Error> {
    Ok(match (expr_left, expr_right) {
        (Expr::Bool(BoolExpr::LiteralBool(b1)), Expr::Bool(BoolExpr::LiteralBool(b2))) => {
            BoolExpr::LiteralBool(b1 == b2)
        }
        (Expr::Bool(BoolExpr::Result(r)), Expr::Bool(BoolExpr::LiteralBool(b)))
        | (Expr::Bool(BoolExpr::LiteralBool(b)), Expr::Bool(BoolExpr::Result(r))) => {
            if b {
                BoolExpr::Result(r)
            } else {
                BoolExpr::NotResult(r)
            }
        }
        (Expr::Bool(BoolExpr::Result(left)), Expr::Bool(BoolExpr::Result(right))) => {
            BoolExpr::TwoResultCondition {
                results: (left, right),
                filter: (true, false, false, true), // 00 and 11
            }
        }
        (left, right) => {
            return Err(Error::UnsupportedFeature(format!(
                "unsupported equality expression combination: left={left:?}, right={right:?}"
            )));
        }
    })
}

#[derive(Clone, Debug)]
struct ConditionalBlock {
    operations: Vec<Op>,
    targets: Vec<QubitWire>,
}

#[derive(Clone, Debug)]
struct Successor {
    block_id: BlockId,
    predecessors: Vec<BlockId>,
}

#[derive(Clone, Debug)]
struct BranchBlock {
    cond_expr: Expr,
    true_block: ConditionalBlock,
    false_block: Option<ConditionalBlock>,
    unconditional_successor: Successor,
    true_successor: Successor,
    false_successor: Option<Successor>,
}

/// Can only handle basic branches
fn make_simple_branch_block(
    blocks: &IndexMap<BlockId, CircuitBlock>,
    cond_expr: &Expr,
    current_block_id: BlockId,
    true_block_id: BlockId,
    false_block_id: BlockId,
) -> Result<BranchBlock, Error> {
    let CircuitBlock {
        operations: true_operations,
        terminator: true_terminator,
        ..
    } = blocks.get(true_block_id).expect("block should exist");
    let CircuitBlock {
        operations: false_operations,
        terminator: false_terminator,
        ..
    } = blocks.get(false_block_id).expect("block should exist");

    let true_successor = match true_terminator {
        Some(Terminator::Unconditional(s)) => Some(*s),
        _ => None,
    };
    let false_successor = match false_terminator {
        Some(Terminator::Unconditional(s)) => Some(*s),
        _ => None,
    };

    if true_successor.is_some_and(|c| c == false_block_id) && false_successor.is_none() {
        // simple if
        let true_block = expand_real_branch_block(true_operations)?;

        Ok(BranchBlock {
            cond_expr: cond_expr.clone(),
            true_block,
            false_block: None,
            unconditional_successor: Successor {
                block_id: false_block_id,
                predecessors: vec![true_block_id, current_block_id],
            },
            true_successor: Successor {
                block_id: true_block_id,
                predecessors: vec![current_block_id],
            },
            false_successor: None,
        })
    } else if false_successor.is_some_and(|c| c == true_block_id) && true_successor.is_none() {
        // simple if, but flipped (i.e. just else)
        // TODO: test

        let true_block = expand_real_branch_block(false_operations)?;

        Ok(BranchBlock {
            cond_expr: cond_expr.negate()?, // negating
            true_block,
            false_block: None,
            unconditional_successor: Successor {
                block_id: true_block_id,
                predecessors: vec![false_block_id, current_block_id],
            },
            true_successor: Successor {
                block_id: false_block_id,
                predecessors: vec![current_block_id],
            },
            false_successor: None,
        })
    } else if true_successor
        .and_then(|true_successor| {
            false_successor.map(|false_successor| (true_successor, false_successor))
        })
        .is_some_and(|(true_successor, false_successor)| true_successor == false_successor)
    {
        // both branches go to the same successor, so it's an if/else
        let true_block = expand_real_branch_block(true_operations)?;
        let false_block = expand_real_branch_block(false_operations)?;

        Ok(BranchBlock {
            cond_expr: cond_expr.clone(),
            true_block,
            false_block: Some(false_block),
            unconditional_successor: Successor {
                block_id: true_successor.expect("should exist"),
                predecessors: vec![true_block_id, false_block_id],
            },
            true_successor: Successor {
                block_id: true_block_id,
                predecessors: vec![current_block_id],
            },
            false_successor: Some(Successor {
                block_id: false_block_id,
                predecessors: vec![current_block_id],
            }),
        })
    } else {
        Err(Error::UnsupportedFeature(format!(
            "complex branch: true_block={true_block_id:?} successor={true_successor:?}, false_block={false_block_id:?} successor={false_successor:?}"
        )))
    }
}

fn expand_real_branch_block(operations: &Vec<Op>) -> Result<ConditionalBlock, Error> {
    let mut seen = FxHashSet::default();
    let mut real_ops = vec![];
    for op in operations {
        real_ops.push(op.clone());
        for q in op.target_qubits.iter().chain(&op.control_qubits) {
            seen.insert((q.0, None));
        }
        for ResultWire(q, r) in op.target_results.iter().chain(&op.control_results) {
            seen.insert((*q, Some(r)));
        }
    }
    // TODO: actually test measurements in branches

    // if seen.iter().any(|(_, r)| r.is_some()) {
    //     return Err(Error::UnsupportedFeature(
    //         "measurement operation in a branch block".to_owned(),
    //     ));
    // }

    // TODO: everything is a target. Don't know how else we would do this.
    let target_qubits = seen.into_iter().map(|(q, _)| QubitWire(q)).collect();
    Ok(ConditionalBlock {
        operations: real_ops,
        targets: target_qubits,
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
    /// variable id -> result id
    variables: IndexMap<VariableId, Expr>,
    /// block id -> (operations, successor)
    blocks: IndexMap<BlockId, CircuitBlock>,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Rich(RichExpr),
    Bool(BoolExpr),
    Unresolved(VariableId),
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
    fn negate(&self) -> Result<Expr, Error> {
        let b = match self {
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
        };
        Ok(b)
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
            Expr::Unresolved(_) => vec![self.clone()],
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
            Expr::Unresolved(variable_id) => {
                warn!(
                    "warning: trying to get linked results for unresolved variable {variable_id:?}"
                );
                vec![]
            }
        }
    }

    fn is_unresolved(&self) -> bool {
        match self {
            Expr::Rich(rich_expr) => match rich_expr {
                RichExpr::Literal(_) => false,
                RichExpr::FunctionOf(exprs) => exprs.iter().any(Expr::is_unresolved),
            },
            Expr::Bool(bool_expr) => match bool_expr {
                BoolExpr::TwoResultCondition { .. }
                | BoolExpr::Result(_)
                | BoolExpr::NotResult(_)
                | BoolExpr::LiteralBool(_) => false,
                BoolExpr::BinOp(condition_expr, condition_expr1, _) => {
                    condition_expr.is_unresolved() || condition_expr1.is_unresolved()
                }
            },
            Expr::Unresolved(_) => true,
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
                    write!(f, "function of: ({})", results.join(", "))
                }
            },
            Expr::Bool(condition_expr) => write!(f, "{condition_expr}"),
            Expr::Unresolved(variable_id) => write!(f, "unresolved({variable_id:?})"),
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

fn store_variable_placeholder(variables: &mut IndexMap<VariableId, Expr>, variable: Variable) {
    if variables.get(variable.variable_id).is_none() {
        variables.insert(variable.variable_id, Expr::Unresolved(variable.variable_id));
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
    if let Some(old_value) = variables.get(variable_id) {
        if old_value.is_unresolved() {
            // allow overwriting unresolved variables
            debug!("note: variable {variable_id:?} was unresolved, now storing {expr:?}");
        } else if old_value != &expr {
            panic!("variable {variable_id:?} already stored {old_value:?}, cannot store {expr:?}");
        }
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
            let (qubit, result) = gather_measurement_operands_inner(operands)?;
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
                    "unknown readout callable: {name}"
                )));
            }
        },
        CallableType::Regular => {
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
        CallableType::Reset | CallableType::OutputRecording | CallableType::Initialize => {}
    }

    Ok(())
}

fn trace_call(
    state: &mut ProgramMap,
    builder_ctx: &mut BuilderWithRegisterMap,
    callable: &Callable,
    operands: &[Operand],
    metadata: Option<&InstructionMetadata>,
) -> Result<(), Error> {
    match callable.call_type {
        CallableType::Measurement => trace_measurement(builder_ctx, callable, operands, metadata),
        CallableType::Reset => trace_reset(state, builder_ctx, callable, operands, metadata),
        CallableType::Regular => trace_gate(state, builder_ctx, callable, operands, metadata),
        CallableType::Readout | CallableType::OutputRecording | CallableType::Initialize => Ok(()),
    }
}

fn trace_gate(
    state: &mut ProgramMap,
    builder_ctx: &mut BuilderWithRegisterMap,
    callable: &Callable,
    operands: &[Operand],
    metadata: Option<&InstructionMetadata>,
) -> Result<(), Error> {
    let GateSpec {
        name,
        operand_types,
        is_adjoint,
    } = callable_spec(callable, operands)?;

    let (target_qubits, control_qubits, control_results, args) =
        match_operands(state, &operand_types, operands)?;

    if target_qubits.is_empty() && control_qubits.is_empty() && control_results.is_empty() {
        // Skip operations without targets or controls.
        // Alternative might be to include these anyway, across the entire state,
        // or annotated in the circuit in some way.
    } else {
        builder_ctx.builder.gate(
            builder_ctx.register_map,
            name,
            is_adjoint,
            &GateInputs {
                targets: &target_qubits,
                control_qubits: &control_qubits,
                control_results: &control_results,
            },
            args,
            metadata.and_then(|md| md.dbg_location),
        );
    }
    Ok(())
}

fn trace_reset(
    state: &mut ProgramMap,
    builder_ctx: &mut BuilderWithRegisterMap,
    callable: &Callable,
    operands: &[Operand],
    metadata: Option<&InstructionMetadata>,
) -> Result<(), Error> {
    match callable.name.as_str() {
        "__quantum__qis__reset__body" => {
            let (target_qubits, control_qubits, control_results, _) =
                match_operands(state, &[OperandType::Target], operands)?;
            assert!(
                control_qubits.is_empty() && control_results.is_empty(),
                "reset cannot have controls"
            );
            assert!(
                control_results.is_empty(),
                "reset cannot have control results"
            );
            assert!(
                target_qubits.len() == 1,
                "reset must have exactly one target"
            );

            let qubit = target_qubits[0];
            builder_ctx.builder.reset(
                builder_ctx.register_map,
                qubit,
                metadata.and_then(|md| md.dbg_location),
            );
        }
        name => {
            return Err(Error::UnsupportedFeature(format!(
                "unknown reset callable: {name}"
            )));
        }
    }
    Ok(())
}

fn trace_measurement(
    builder_ctx: &mut BuilderWithRegisterMap,
    callable: &Callable,
    operands: &[Operand],
    metadata: Option<&InstructionMetadata>,
) -> Result<(), Error> {
    let (qubit, result) = gather_measurement_operands_inner(operands)?;

    match callable.name.as_str() {
        "__quantum__qis__mresetz__body" => {
            builder_ctx.builder.mresetz(
                builder_ctx.register_map,
                qubit,
                result,
                metadata.and_then(|md| md.dbg_location),
            );
        }
        "__quantum__qis__m__body" => {
            builder_ctx.builder.m(
                builder_ctx.register_map,
                qubit,
                result,
                metadata.and_then(|md| md.dbg_location),
            );
        }
        name => panic!("unknown measurement callable: {name}"),
    }

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
            operand_types: vec![OperandType::Target],
            is_adjoint: false,
        }
    }

    fn single_qubit_gate_adjoint(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::Target],
            is_adjoint: true,
        }
    }

    fn rotation_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::Arg, OperandType::Target],
            is_adjoint: false,
        }
    }

    fn controlled_gate(name: &'a str, num_controls: usize) -> Self {
        let mut operand_types = vec![];
        for _ in 0..num_controls {
            operand_types.push(OperandType::Control);
        }
        operand_types.push(OperandType::Target);
        Self {
            name,
            operand_types,
            is_adjoint: false,
        }
    }

    fn two_qubit_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::Target, OperandType::Target],
            is_adjoint: false,
        }
    }

    fn two_qubit_rotation_gate(name: &'a str) -> Self {
        Self {
            name,
            operand_types: vec![OperandType::Arg, OperandType::Target, OperandType::Target],
            is_adjoint: false,
        }
    }
}

fn callable_spec<'a>(callable: &'a Callable, operands: &[Operand]) -> Result<GateSpec<'a>, Error> {
    let gate_spec = match callable.name.as_str() {
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
        // multi-qubit gates
        "__quantum__qis__cx__body" => GateSpec::controlled_gate("X", 1),
        "__quantum__qis__cy__body" => GateSpec::controlled_gate("Y", 1),
        "__quantum__qis__cz__body" => GateSpec::controlled_gate("Z", 1),
        "__quantum__qis__ccx__body" => GateSpec::controlled_gate("X", 2),
        "__quantum__qis__rxx__body" => GateSpec::two_qubit_rotation_gate("Rxx"),
        "__quantum__qis__ryy__body" => GateSpec::two_qubit_rotation_gate("Ryy"),
        "__quantum__qis__rzz__body" => GateSpec::two_qubit_rotation_gate("Rzz"),
        "__quantum__qis__swap__body" => GateSpec::two_qubit_gate("SWAP"),
        custom => {
            let mut operand_types = vec![];
            for o in operands {
                match o {
                    Operand::Literal(Literal::Integer(_) | Literal::Double(_)) => {
                        operand_types.push(OperandType::Arg);
                    }
                    Operand::Variable(Variable {
                        ty: Ty::Boolean | Ty::Integer | Ty::Double,
                        ..
                    }) => operand_types.push(OperandType::Arg),
                    Operand::Variable(Variable { ty: Ty::Qubit, .. })
                    | Operand::Literal(Literal::Qubit(_)) => {
                        // assume all qubit operands are targets for custom gates
                        operand_types.push(OperandType::Target);
                    }
                    o => {
                        return Err(Error::UnsupportedFeature(format!(
                            "unsupported operand for custom gate {custom}: {o:?}"
                        )));
                    }
                }
            }

            GateSpec {
                name: custom,
                operand_types,
                is_adjoint: false,
            }
        }
    };
    Ok(gate_spec)
}

fn gather_measurement_operands_inner(operands: &[Operand]) -> Result<(usize, usize), Error> {
    let mut qubits = operands.iter().filter_map(|o| match o {
        Operand::Literal(Literal::Qubit(q)) => Some(q),
        _ => None,
    });
    let qubit = qubits.next();
    let Some(qubit) = qubit else {
        return Err(Error::UnsupportedFeature(
            "measurement must have a qubit operand".to_owned(),
        ));
    };
    if qubits.next().is_some() {
        return Err(Error::UnsupportedFeature(
            "measurement should only have one qubit operand".to_owned(),
        ));
    }

    let mut results = operands.iter().filter_map(|o| match o {
        Operand::Literal(Literal::Result(r)) => {
            Some(usize::try_from(*r).expect("result id should fit in usize"))
        }
        _ => None,
    });
    let result = results.next();
    let Some(result) = result else {
        return Err(Error::UnsupportedFeature(
            "measurement must have a result operand".to_owned(),
        ));
    };
    if results.next().is_some() {
        return Err(Error::UnsupportedFeature(
            "measurement should only have one result operand".to_owned(),
        ));
    }

    if operands.len() != 2 {
        return Err(Error::UnsupportedFeature(
            "measurement should only have a qubit and result operand".to_owned(),
        ));
    }

    Ok((
        usize::try_from(*qubit).expect("qubit id should fit in usize"),
        result,
    ))
}

enum OperandType {
    Control,
    Target,
    Arg,
}

type Operands = (Vec<usize>, Vec<usize>, Vec<usize>, Vec<String>);

fn match_operands(
    state: &mut ProgramMap,
    operand_types: &[OperandType],
    operands: &[Operand],
) -> Result<Operands, Error> {
    let mut target_qubits = vec![];
    let mut control_results = vec![];
    let mut control_qubits = vec![];
    let mut args = vec![];
    if operand_types.len() != operands.len() {
        return Err(Error::UnsupportedFeature(
            "unexpected number of operands for known operation".to_owned(),
        ));
    }
    for (operand, operand_type) in operands.iter().zip(operand_types) {
        match operand {
            Operand::Literal(literal) => match literal {
                Literal::Qubit(q) => {
                    let qubit_operands_array = match operand_type {
                        OperandType::Control => &mut control_qubits,
                        OperandType::Target => &mut target_qubits,
                        OperandType::Arg => {
                            return Err(Error::UnsupportedFeature(
                                "qubit operand cannot be an argument".to_owned(),
                            ));
                        }
                    };
                    qubit_operands_array
                        .push(usize::try_from(*q).expect("qubit id should fit in usize"));
                }
                Literal::Result(_r) => {
                    return Err(Error::UnsupportedFeature(
                        "result operand cannot be a target of a unitary operation".to_owned(),
                    ));
                }
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
                if let &OperandType::Arg = operand_type {
                    let expr = expr_for_variable(&state.variables, var.variable_id)?.clone();
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
    Ok((target_qubits, control_qubits, control_results, args))
}

// TODO: __quantum__rt__read_loss

// TODO: merge with OperationListBuilder
struct OpListBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations: Vec<Op>,
}

impl OpListBuilder {
    pub fn new(max_operations: usize) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
        }
    }

    fn push_op(&mut self, op: Op) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        self.operations.push(op);
    }

    pub fn into_operations(self) -> Vec<Op> {
        self.operations
    }

    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        called_at: Option<DbgLocationId>,
    ) {
        self.push_op(Self::new_unitary(
            wire_map, name, is_adjoint, inputs, args, called_at,
        ));
    }

    fn m(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        called_at: Option<DbgLocationId>,
    ) {
        self.push_op(Self::new_measurement(
            "M", wire_map, qubit, result, called_at,
        ));
    }

    fn mresetz(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        called_at: Option<DbgLocationId>,
    ) {
        self.push_op(Self::new_measurement(
            "MResetZ", wire_map, qubit, result, called_at,
        ));
        self.push_op(Self::new_ket(wire_map, qubit, called_at));
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, called_at: Option<DbgLocationId>) {
        self.push_op(Self::new_ket(wire_map, qubit, called_at));
    }

    fn new_unitary(
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs<'_>,
        args: Vec<String>,
        called_at: Option<DbgLocationId>,
    ) -> Op {
        Op {
            kind: OperationKind::Unitary {
                label: name.to_string(),
            },
            args,
            control_qubits: inputs
                .control_qubits
                .iter()
                .map(|q| wire_map.qubit_wire(*q))
                .collect(),
            control_results: inputs
                .control_results
                .iter()
                .map(|r| wire_map.result_wire(*r))
                .collect(),
            target_qubits: inputs
                .targets
                .iter()
                .map(|q| wire_map.qubit_wire(*q))
                .collect(),
            target_results: vec![],
            is_adjoint,
            location: called_at,
        }
    }

    fn new_measurement(
        label: &str,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        called_at: Option<DbgLocationId>,
    ) -> Op {
        Op {
            kind: OperationKind::Measurement {
                label: label.to_string(),
            },
            args: vec![],
            control_qubits: vec![wire_map.qubit_wire(qubit)],
            is_adjoint: false,
            target_qubits: vec![],
            target_results: vec![wire_map.result_wire(result)],
            control_results: vec![],
            location: called_at,
        }
    }

    fn new_ket(wire_map: &WireMap, qubit: usize, called_at: Option<DbgLocationId>) -> Op {
        Op {
            kind: OperationKind::Ket {
                label: "0".to_string(),
            },
            target_qubits: vec![wire_map.qubit_wire(qubit)],
            control_qubits: vec![],
            control_results: vec![],
            args: vec![],
            target_results: vec![],
            is_adjoint: false,
            location: called_at,
        }
    }
}
