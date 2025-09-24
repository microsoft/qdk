// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::{
    fmt::{Display, Write},
    mem::take,
};

use crate::{
    Circuit, ComponentColumn, Config, Error, GenerationMethod, Ket, Measurement, Operation, Qubit,
    Register, Unitary, group_qubits, operation_list_to_grid,
};
use log::{debug, warn};
use qsc_data_structures::{index_map::IndexMap, line_column::Encoding};
use qsc_frontend::{compile::PackageStore, location::Location, resolve::Scope};
use qsc_hir::hir::PackageId;
use qsc_partial_eval::{
    Callable, CallableType, ConditionCode, FcmpConditionCode, Instruction, Literal, Operand,
    VariableId,
    rir::{
        BlockId, BlockWithMetadata, DbgLocation, DbgMetadataScope, InstructionMetadata,
        InstructionWithMetadata, MetadataPackageSpan, Program, Ty, Variable,
    },
};
use rustc_hash::FxHashSet;

type ResultId = (usize, usize);

#[derive(Clone, Debug)]
struct Branch {
    condition: Variable,
    true_block: BlockId,
    false_block: BlockId,
    metadata: Option<GroupMetadata>,
}

#[derive(Clone, Debug)]
struct Op {
    kind: OperationKind,
    label: String,
    target_qubits: Vec<usize>,
    control_qubits: Vec<usize>,
    target_results: Vec<ResultId>,
    control_results: Vec<ResultId>,
    is_adjoint: bool,
    args: Vec<String>,
}

#[derive(Clone, Debug)]
struct GroupMetadata {
    pub location: MetadataPackageSpan,
}

impl Op {
    fn has_children(&self) -> bool {
        matches!(&self.kind, OperationKind::Group { children, .. } if !children.is_empty())
    }
}

type DbgLocationId = usize;
type DbgScopeId = usize;

#[derive(Clone, Debug)]
struct ScopeStack {
    caller: InstructionStack,
    scope: DbgScopeId,
}

#[derive(Clone, Debug)]
struct InstructionStack(Vec<DbgLocationId>);

impl InstructionStack {
    fn scope_stack(&self, dbg_locations: &[DbgLocation]) -> ScopeStack {
        let (top, prefix) = self
            .0
            .split_last()
            .expect("InstructionStack should not be empty");
        let scope = dbg_locations[*top].scope;
        ScopeStack {
            caller: InstructionStack(prefix.to_vec()),
            scope,
        }
    }
}

#[derive(Clone, Debug)]
enum OperationKind {
    Unitary {
        metadata: Option<InstructionMetadata>,
    },
    Measurement {
        metadata: Option<InstructionMetadata>,
    },
    Ket {
        metadata: Option<InstructionMetadata>,
    },
    Group {
        children: Vec<Op>,
        stack: Option<ScopeStack>,
        metadata: Option<GroupMetadata>,
    },
}

impl From<Op> for Operation {
    fn from(value: Op) -> Self {
        let args = value.args.into_iter().collect();

        let targets = value
            .target_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q,
                result: None,
            })
            .chain(value.target_results.into_iter().map(|(q, r)| Register {
                qubit: q,
                result: Some(r),
            }))
            .collect();
        let controls = value
            .control_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q,
                result: None,
            })
            .chain(value.control_results.into_iter().map(|(q, r)| Register {
                qubit: q,
                result: Some(r),
            }))
            .collect();

        match value.kind {
            OperationKind::Unitary { metadata: _ } => Operation::Unitary(Unitary {
                gate: value.label,
                args,
                children: vec![],
                targets,
                controls,
                is_adjoint: value.is_adjoint,
            }),
            OperationKind::Measurement { metadata: _ } => Operation::Measurement(Measurement {
                gate: value.label,
                args,
                children: vec![],
                qubits: controls,
                results: targets,
            }),
            OperationKind::Ket { metadata: _ } => Operation::Ket(Ket {
                gate: value.label,
                args,
                children: vec![],
                targets,
            }),
            OperationKind::Group {
                children,
                stack: _,
                metadata: _,
            } => Operation::Unitary(Unitary {
                gate: value.label,
                args,
                children: vec![ComponentColumn {
                    components: children.into_iter().map(Into::into).collect(),
                }],
                targets,
                controls,
                is_adjoint: false,
            }),
        }
    }
}

pub fn make_circuit(
    program: &Program,
    package_store: &PackageStore,
    position_encoding: Encoding,
    config: Config,
) -> std::result::Result<Circuit, Error> {
    assert!(config.generation_method == GenerationMethod::Static);
    eprintln!("make_circuit: program={program}");
    let mut program_map = ProgramMap::new(program.num_qubits);
    let callables = &program.callables;

    let mut i = 0;
    let mut done = false;
    while !done {
        for (id, block) in program.blocks.iter() {
            let block_operations = process_block_vars(&mut program_map, callables, block)?;
            program_map.blocks.insert(id, block_operations);
        }

        done = expand_branches_vars(program, &mut program_map)?;
        program_map.blocks.clear();
        i += 1;
        if i > 100 {
            warn!("make_circuit: too many iterations expanding branches, giving up");
            return Err(Error::UnsupportedFeature(
                "too many iterations expanding branches".to_owned(),
            ));
        }
    }

    // Do it all again, with all variables properly resolved
    for (id, block) in program.blocks.iter() {
        let block_operations = operations_in_block(
            &mut program_map,
            &program.dbg_locations,
            &program.dbg_metadata_scopes,
            callables,
            block,
            config.group_scopes,
        )?;
        program_map.blocks.insert(id, block_operations);
    }

    debug!("expanding branches, #2");
    expand_branches(program, &mut program_map)?;

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

    let mut operations = extend_with_successors(&program_map, entry_block);

    let qubits = program_map.into_qubits();

    fill_in_dbg_metadata(&mut operations, package_store, position_encoding)?;
    let operations = operations.into_iter().map(Into::into).collect();

    let (operations, qubits) = if config.collapse_qubit_registers && qubits.len() > 2 {
        // TODO: dummy values for now
        group_qubits(operations, qubits, &[0, 1])
    } else {
        (operations, qubits)
    };

    let component_grid = operation_list_to_grid(operations, &qubits, config.loop_detection);

    let circuit = Circuit {
        qubits,
        component_grid,
    };
    Ok(circuit)
}

/// true result means done
fn expand_branches_vars(program: &Program, state: &mut ProgramMap) -> Result<bool, Error> {
    let mut done = true;
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
            let expanded_branch = expand_branch_vars(state, block_id, &branch)?;

            if let Some(expanded_branch) = expanded_branch {
                let add = match &expanded_branch.grouped_operation.kind {
                    OperationKind::Group {
                        children,
                        stack: _,
                        metadata: _,
                    } => !children.is_empty(),
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

                let condition_expr = state
                    .expr_for_variable(branch.condition.variable_id)?
                    .clone();
                // Find the successor and see if it has any phi nodes
                for successor in expanded_branch.successors_to_check_for_phis {
                    let successor_block = state
                        .blocks
                        .get(successor.block_id)
                        .expect("successor block should exist");

                    let phi_vars = get_phi_vars_from_branch(
                        successor_block,
                        &successor.predecessors,
                        &condition_expr,
                    )?;
                    if let Some(phi_vars) = phi_vars {
                        for (var, expr) in phi_vars {
                            state.store_expr_in_variable(var, expr)?;
                        }
                    } else {
                        done = false;
                    }
                }
            } else {
                done = false;
            }
        }

        state.blocks.insert(block_id, circuit_block);
    }
    Ok(done)
}

// None means more work to be done
fn expand_branch_vars(
    state: &mut ProgramMap,
    curent_block_id: BlockId,
    branch: &Branch,
) -> Result<Option<ExpandedBranchBlock>, Error> {
    let cond_expr = state.expr_for_variable(branch.condition.variable_id)?;
    if cond_expr.is_unresolved() {
        debug!(
            "expand_branch_vars: unresolved condition expr for branch: {:?}",
            cond_expr
        );
        return Ok(None);
    }
    let results = cond_expr.linked_results();

    if let Expr::Bool(BoolExpr::LiteralBool(_)) = cond_expr {
        return Err(Error::UnsupportedFeature(
            "constant condition in branch".to_owned(),
        ));
    }

    if results.is_empty() {
        debug!(
            "expand_branch_vars: condition expr has no results for branch: {:?}",
            cond_expr
        );
        return Ok(None);
    }

    let branch_block = make_simple_branch_block(
        state,
        cond_expr,
        curent_block_id,
        branch.true_block,
        branch.false_block,
    )?;
    debug!("expand_branch: made simple branch block: {branch_block:?}");
    let ConditionalBlock {
        operations: true_operations,
        targets: true_targets,
    } = branch_block.true_block;

    let control_results = results
        .iter()
        .map(|r| state.result_register(*r))
        .map(|r| {
            (
                r.qubit,
                r.result.expect("result register must have result idx"),
            )
        })
        .collect::<Vec<_>>();
    let true_container = Op {
        kind: OperationKind::Group {
            children: true_operations.clone(),
            stack: None,
            metadata: None,
        },
        label: "true".into(),
        args: vec![],
        target_qubits: true_targets.clone(),
        control_qubits: vec![],
        target_results: vec![],
        control_results: control_results.clone(),
        is_adjoint: false,
    };

    let false_container = branch_block.false_block.map(
        |ConditionalBlock {
             operations: false_operations,
             targets: false_targets,
         }| {
            (
                Op {
                    kind: OperationKind::Group {
                        children: false_operations.clone(),
                        stack: None,
                        metadata: None,
                    },
                    label: "false".into(),
                    target_qubits: false_targets.clone(),
                    control_qubits: vec![],
                    target_results: vec![],
                    control_results: control_results.clone(),
                    args: vec![],
                    is_adjoint: false,
                },
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

    Ok(Some(ExpandedBranchBlock {
        _condition: branch.condition,
        grouped_operation: Op {
            kind: OperationKind::Group {
                children: children.into_iter().collect(),
                stack: None,
                metadata: branch.metadata.clone(),
            },
            label,
            target_qubits,
            control_qubits: vec![],
            target_results: vec![],
            control_results: control_results.clone(),
            is_adjoint: false,
            args,
        },
        unconditional_successor: branch_block.unconditional_successor.block_id,
        successors_to_check_for_phis: [
            branch_block.unconditional_successor,
            branch_block.true_successor,
        ]
        .into_iter()
        .chain(branch_block.false_successor)
        .collect(),
    }))
}

fn process_block_vars(
    state: &mut ProgramMap,
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
        let BlockUpdate {
            terminator: new_terminator,
            ..
        } = get_operations_for_instruction(state, callables, &mut phis, &mut done, instruction)?;

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
fn expand_branches(program: &Program, state: &mut ProgramMap) -> Result<(), Error> {
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
            let expanded_branch = expand_branch(state, block_id, &branch)?;

            let add = match &expanded_branch.grouped_operation.kind {
                OperationKind::Group { children, .. } => !children.is_empty(),
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
        debug!("attempting to resolve phi var {var:?} with pres {pres:?}");
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
            debug!("get_phi_vars_from_branch: unresolved phi var {var:?} in successor block");
            done = false;
        }
    }
    if done { Ok(Some(phi_vars)) } else { Ok(None) }
}

// None means unresolved, more work to do
fn combine_exprs(options: Vec<Expr>) -> Result<Option<Expr>, Error> {
    if options.iter().any(Expr::is_unresolved) {
        debug!("combine_exprs: unresolved expr in options: {options:?}");
        return Ok(None);
    }

    let e = Expr::Rich(RichExpr::FunctionOf(
        options.into_iter().flat_map(|e| e.flat_exprs()).collect(),
    ));
    Ok(Some(e))
}

fn extend_with_successors(state: &ProgramMap, entry_block: &CircuitBlock) -> Vec<Op> {
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
            operations.push(op.clone());
        }
    }
    operations
}

fn fill_in_dbg_metadata(
    operations: &mut [Op],
    package_store: &PackageStore,
    position_encoding: Encoding,
) -> Result<(), Error> {
    for op in operations {
        if let OperationKind::Group { children, .. } = &mut op.kind {
            fill_in_dbg_metadata(children, package_store, position_encoding)?;
        }

        let location = match &op.kind {
            OperationKind::Unitary { metadata }
            | OperationKind::Measurement { metadata }
            | OperationKind::Ket { metadata } => metadata.as_ref().map(|md| &md.location),
            OperationKind::Group {
                children: _,
                stack: _,
                metadata,
            } => metadata.as_ref().map(|md| &md.location),
        };

        if let Some(MetadataPackageSpan {
            package: package_id,
            span,
        }) = &location
        {
            let location = Location::from(
                *span,
                usize::try_from(*package_id)
                    .expect("package id should fit into usize")
                    .into(),
                package_store,
                position_encoding,
            );
            let mut json = String::new();
            writeln!(&mut json, "metadata={{").expect("writing to string should work");
            writeln!(&mut json, r#""source": {:?},"#, location.source)
                .expect("writing to string should work");
            write!(
                        &mut json,
                        r#""span": {{"start": {{"line": {}, "character": {}}}, "end": {{"line": {}, "character": {}}}}}"#,
                        location.range.start.line,
                        location.range.start.column,
                        location.range.end.line,
                        location.range.end.column
                    )
                    .expect("writing to string should work");
            write!(&mut json, "}}").expect("writing to string should work");
            op.args.push(json);
        }
    }
    Ok(())
}

// TODO: this could be represented by a circuit block, maybe. Consider.
struct ExpandedBranchBlock {
    _condition: Variable,
    grouped_operation: Op,
    successors_to_check_for_phis: Vec<Successor>,
    unconditional_successor: BlockId,
}

fn expand_branch(
    state: &mut ProgramMap,
    curent_block_id: BlockId,
    branch: &Branch,
) -> Result<ExpandedBranchBlock, Error> {
    let cond_expr = state.expr_for_variable(branch.condition.variable_id)?;
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
        state,
        cond_expr,
        curent_block_id,
        branch.true_block,
        branch.false_block,
    )?;

    debug!("expand_branch: made simple branch block: {branch_block:?}");
    let ConditionalBlock {
        operations: true_operations,
        targets: true_targets,
    } = branch_block.true_block;

    let control_results = results
        .iter()
        .map(|r| state.result_register(*r))
        .map(|r| {
            (
                r.qubit,
                r.result.expect("result register must have result idx"),
            )
        })
        .collect::<Vec<_>>();
    let true_container = Op {
        kind: OperationKind::Group {
            children: true_operations.clone(),
            stack: None,
            metadata: None,
        },
        label: "true".into(),
        args: vec![],
        target_qubits: true_targets.clone(),
        control_qubits: vec![],
        target_results: vec![],
        control_results: control_results.clone(),
        is_adjoint: false,
    };

    let false_container = branch_block.false_block.map(
        |ConditionalBlock {
             operations: false_operations,
             targets: false_targets,
         }| {
            (
                Op {
                    kind: OperationKind::Group {
                        children: false_operations.clone(),
                        stack: None,
                        metadata: None,
                    },
                    label: "false".into(),
                    target_qubits: false_targets.clone(),
                    control_qubits: vec![],
                    target_results: vec![],
                    control_results: control_results.clone(),
                    args: vec![],
                    is_adjoint: false,
                },
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

    Ok(ExpandedBranchBlock {
        _condition: branch.condition,
        grouped_operation: Op {
            kind: OperationKind::Group {
                children: children.into_iter().collect(),
                stack: None,
                metadata: branch.metadata.clone(),
            },
            label,
            target_qubits,
            control_qubits: vec![],
            target_results: vec![],
            control_results: control_results.clone(),
            is_adjoint: false,
            args,
        },
        unconditional_successor: branch_block.unconditional_successor.block_id,
        successors_to_check_for_phis: [
            branch_block.unconditional_successor,
            branch_block.true_successor,
        ]
        .into_iter()
        .chain(branch_block.false_successor)
        .collect(),
    })
}

#[derive(Clone, Debug)]
struct CircuitBlock {
    phis: Vec<(Variable, Vec<(Expr, BlockId)>)>,
    operations: Vec<Op>,
    terminator: Option<Terminator>,
}

fn operations_in_block(
    state: &mut ProgramMap,
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    block: &BlockWithMetadata,
    group_scopes: bool,
) -> Result<CircuitBlock, Error> {
    // TODO: use get_block_successors from utils
    let mut terminator = None;
    let mut phis = vec![];
    let mut operations = vec![];
    let mut done = false;

    let mut current_scope = vec![];
    let mut last_stack = None;
    // let mut last_discriminator = None;
    for instruction in &block.0 {
        if done {
            return Err(Error::UnsupportedFeature(
                "instructions after return or jump in block".to_owned(),
            ));
        }
        let BlockUpdate {
            operations: new_operations,
            terminator: new_terminator,
        } = get_operations_for_instruction(state, callables, &mut phis, &mut done, instruction)?;

        if let Some(new_terminator) = new_terminator {
            let old = terminator.replace(new_terminator);
            assert!(
                old.is_none(),
                "did not expect more than one unconditional successor for block, old: {old:?} new: {terminator:?}"
            );
        }

        extend_operations(
            &mut operations,
            &mut current_scope,
            &mut last_stack,
            dbg_locations,
            dbg_metadata_scopes,
            new_operations,
            group_scopes,
        );
    }

    // flush any remaining scope
    flush_batch(
        &mut operations,
        current_scope,
        dbg_locations,
        dbg_metadata_scopes,
        last_stack.as_ref(),
        group_scopes,
    );

    Ok(CircuitBlock {
        phis,
        operations,
        terminator, // TODO: make this exhaustive, and detect corrupt blocks
    })
}

fn extend_operations(
    operations: &mut Vec<Op>,
    current_scope: &mut Vec<Op>,
    last_scope: &mut Option<ScopeStack>,
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    new_operations: Vec<Op>,
    group_scopes: bool,
) {
    for op in new_operations {
        let instruction_metadata = match &op.kind {
            OperationKind::Unitary { metadata }
            | OperationKind::Measurement { metadata }
            | OperationKind::Ket { metadata } => metadata.as_ref(),
            OperationKind::Group { .. } => None,
        };
        if let Some(metadata) = instruction_metadata {
            let scope_stack =
                instruction_logical_stack(dbg_locations, dbg_metadata_scopes, metadata)
                    .map(|s| s.scope_stack(dbg_locations));

            if let Some(scope_stack) = scope_stack {
                // let mut add_to_current = last_stack
                //     .as_ref()
                //     .is_some_and(|last_stack| are_stacks_siblings(last_stack, &stack));
                let mut add_to_current = false;

                if !matches!(
                    &op,
                    Op {
                        kind: OperationKind::Unitary { .. },
                        ..
                    }
                ) {
                    // only group unitaries
                    add_to_current = false;
                }

                if add_to_current {
                    // Add to current group
                    current_scope.push(op);
                } else {
                    // Start new group
                    flush_batch(
                        operations,
                        take(current_scope),
                        dbg_locations,
                        dbg_metadata_scopes,
                        last_scope.as_ref(),
                        group_scopes,
                    );
                    current_scope.push(op);
                }

                *last_scope = Some(scope_stack);

                continue;
            }
        }
        // no scope grouping, flush current scope if any, then add this one right away

        // flush group
        flush_batch(
            operations,
            take(current_scope),
            dbg_locations,
            dbg_metadata_scopes,
            last_scope.as_ref(),
            group_scopes,
        );

        // reset last scope
        *last_scope = None;

        // add this operation
        operations.push(op);
    }
}

fn are_stacks_siblings(left: &[DbgLocationId], right: &[DbgLocationId]) -> bool {
    if left.len() == right.len() {
        let last_stack = left.split_last();
        let stack = right.split_last();
        if let (Some((_last_top, last_rest)), Some((_top, rest))) = (last_stack, stack) {
            // the tail of the stack should match exactly
            return last_rest == rest;
        }
    }
    false
}

fn flush_batch(
    block_operations: &mut Vec<Op>,
    mut current_batch: Vec<Op>,
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    scope_stack: Option<&ScopeStack>,
    group_scopes: bool,
) {
    eprintln!(
        "flush_batch: current_batch={:?} ops={:?}",
        current_batch
            .iter()
            .map(|op| op.label.clone())
            .collect::<Vec<_>>(),
        block_operations
            .iter()
            .map(|op| op.label.clone())
            .collect::<Vec<_>>()
    );

    if !current_batch.is_empty() {
        let scope_stack = scope_stack.expect("instruction stack should exist");

        if group_scopes {
            let qubits: FxHashSet<usize> = current_batch
                .iter()
                .flat_map(|op| op.control_qubits.iter().chain(&op.target_qubits).copied())
                .collect();
            // TODO: use these results somehow
            let _results: FxHashSet<(usize, usize)> = current_batch
                .iter()
                .flat_map(|op| op.control_results.iter().chain(&op.target_results).copied())
                .collect();

            let children = current_batch.clone();
            let target_qubits = qubits.into_iter().collect();

            flush_scoped_ops(
                block_operations,
                dbg_locations,
                dbg_metadata_scopes,
                children,
                InstructionStack(vec![]),
                scope_stack.clone(),
                target_qubits,
            );
        } else {
            // Add individually
            for op in current_batch.drain(..) {
                block_operations.push(op);
            }
        }
    } else {
        eprintln!("flush_batch: current_batch is empty, nothing to do");
    }
}

fn instruction_logical_stack(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    metadata: &InstructionMetadata,
) -> Option<InstructionStack> {
    if let Some(dbg_location_idx) = metadata.dbg_location {
        let mut location_stack = vec![];
        let mut current_location_idx = Some(dbg_location_idx);

        while let Some(location_idx) = current_location_idx {
            location_stack.push(location_idx);
            let location = dbg_locations
                .get(location_idx)
                .expect("dbg location should exist");
            current_location_idx = location.inlined_at;
        }

        // filter out scopes in std and core
        location_stack.retain(|location| {
            let scope = &dbg_metadata_scopes[dbg_locations[*location].scope];
            match scope {
                DbgMetadataScope::SubProgram {
                    name: _,
                    span: location,
                } => {
                    let package_id =
                        usize::try_from(location.package).expect("package id should fit in usize");
                    package_id != usize::from(PackageId::CORE)
                        && package_id != usize::from(PackageId::CORE.successor())
                }
            }
        });

        location_stack.reverse();

        return Some(InstructionStack(location_stack));
    }
    None
}

fn scope_name(scope: DbgScopeId, dbg_metadata_scopes: &[DbgMetadataScope]) -> String {
    let scope_location = &dbg_metadata_scopes[scope];
    match scope_location {
        DbgMetadataScope::SubProgram { name, span: _ } => name.to_string(),
    }
}

fn loc_name(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    location: DbgLocationId,
) -> (String, u32) {
    let dbg_location = &dbg_locations[location];
    let scope = &dbg_metadata_scopes[dbg_location.scope];
    let offset = dbg_location.span.span.lo;
    match scope {
        DbgMetadataScope::SubProgram { name, span: _ } => (name.to_string(), offset),
    }
}

fn fmt_stack(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    stack: &InstructionStack,
) -> String {
    let names: Vec<String> = stack
        .0
        .iter()
        .map(|loc| fmt_loc(dbg_locations, dbg_metadata_scopes, *loc))
        .collect();
    names.join("->")
}

fn fmt_scope_stack(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    stack: &ScopeStack,
) -> String {
    let mut prefix = fmt_stack(dbg_locations, dbg_metadata_scopes, &stack.caller);
    if !prefix.is_empty() {
        prefix.push_str("->");
    }
    prefix.push_str(&scope_name(stack.scope, dbg_metadata_scopes));
    prefix
}

fn fmt_ops(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    ops: &[Op],
) -> String {
    let names: Vec<String> = ops
        .iter()
        .map(|op| match &op.kind {
            OperationKind::Group {
                children,
                stack,
                metadata,
            } => {
                format!(
                    "{} {}children=({})",
                    op.label,
                    match stack {
                        Some(stack) => format!(
                            "stack={}, ",
                            fmt_scope_stack(dbg_locations, dbg_metadata_scopes, stack)
                        ),
                        None => "".to_string(),
                    },
                    fmt_ops(dbg_locations, dbg_metadata_scopes, children)
                )
            }
            _ => op.label.clone(),
        })
        .collect();
    names.join(", ")
}

fn fmt_loc(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    location: usize,
) -> String {
    let (name, offset) = loc_name(dbg_locations, dbg_metadata_scopes, location);
    format!("{name}@{offset}")
}

fn make_scope_metadata(
    _dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    scope_location: DbgScopeId,
) -> GroupMetadata {
    let scope_location = &dbg_metadata_scopes[scope_location];
    let scope_location = match scope_location {
        DbgMetadataScope::SubProgram { span, .. } => span,
    };

    GroupMetadata {
        location: scope_location.clone(),
    }
}

fn flush_scoped_ops(
    current_container: &mut Vec<Op>,
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    ops: Vec<Op>,
    current_caller: InstructionStack,
    scope_stack: ScopeStack,
    target_qubits: Vec<usize>,
) {
    let scope_metadata = make_scope_metadata(dbg_locations, dbg_metadata_scopes, scope_stack.scope);
    let label = scope_name(scope_stack.scope, dbg_metadata_scopes);
    eprintln!(
        "flush_group: current_caller={} scope_stack={}",
        fmt_stack(dbg_locations, dbg_metadata_scopes, &current_caller),
        fmt_scope_stack(dbg_locations, dbg_metadata_scopes, &scope_stack)
    );

    if !scope_stack.caller.0.is_empty() {
        if let Some(last_op) = current_container.last_mut() {
            if let OperationKind::Group {
                children: last_op_children,
                stack: Some(last_op_stack),
                metadata: last_op_metadata,
            } = &mut last_op.kind
            {
                let full_scope_stack = concat_stacks(&current_caller, &scope_stack);
                eprintln!(
                    "  flush_group: considering last_op with stack {} for full_stack {}",
                    fmt_scope_stack(dbg_locations, dbg_metadata_scopes, last_op_stack),
                    fmt_scope_stack(dbg_locations, dbg_metadata_scopes, &full_scope_stack)
                );
                if let Some(rest) = strip_stack_prefix(
                    dbg_locations,
                    dbg_metadata_scopes,
                    &full_scope_stack,
                    last_op_stack,
                ) {
                    eprintln!(
                        "  flush_group: found existing matching group with stack {}",
                        fmt_scope_stack(dbg_locations, dbg_metadata_scopes, last_op_stack)
                    );
                    last_op.target_qubits.extend(target_qubits.clone());
                    last_op.target_qubits.sort_unstable();
                    last_op.target_qubits.dedup();

                    // add to this group

                    flush_scoped_ops(
                        last_op_children,
                        dbg_locations,
                        dbg_metadata_scopes,
                        ops,
                        last_op_stack.caller.clone(),
                        rest,
                        target_qubits,
                    );

                    return;
                }
            }
        }

        let parent = scope_stack.clone().caller;
        if !parent.0.is_empty() {
            let full_scope_stack = concat_stacks(&current_caller, &scope_stack);
            eprintln!(
                "  flush_group: creating new parent group with full_stack = {}",
                fmt_scope_stack(dbg_locations, dbg_metadata_scopes, &full_scope_stack)
            );
            let group = Op {
                kind: OperationKind::Group {
                    children: ops,
                    stack: Some(full_scope_stack),
                    metadata: Some(scope_metadata),
                },
                label,
                target_qubits,
                control_qubits: vec![],
                target_results: vec![], // results.into_iter().collect(), TODO: include results too somehow
                control_results: vec![],
                is_adjoint: false,
                args: vec![],
            };
            // create container for the prefix, and add to it
            flush_scoped_ops(
                current_container,
                dbg_locations,
                dbg_metadata_scopes,
                vec![group.clone()],
                current_caller.clone(),
                parent.scope_stack(dbg_locations),
                group.target_qubits.clone(),
            );
            return;
        }
    } else {
        let full_scope_stack = concat_stacks(&current_caller, &scope_stack);

        eprintln!(
            "  flush_group: PUSHING FINAL OP for full stack {} ops={}\n",
            fmt_scope_stack(dbg_locations, dbg_metadata_scopes, &full_scope_stack,),
            fmt_ops(dbg_locations, dbg_metadata_scopes, &ops)
        );

        let group = Op {
            kind: OperationKind::Group {
                children: ops,
                stack: Some(full_scope_stack),
                metadata: Some(scope_metadata),
            },
            label,
            target_qubits,
            control_qubits: vec![],
            target_results: vec![], // results.into_iter().collect(), TODO: include results too somehow
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        };
        current_container.push(group);
    }
}

fn concat_stacks(prefix: &InstructionStack, scope_stack: &ScopeStack) -> ScopeStack {
    ScopeStack {
        caller: InstructionStack(
            [prefix.0.clone(), scope_stack.caller.0.clone()]
                .concat()
                .clone(),
        ),
        scope: scope_stack.scope,
    }
}

fn strip_stack_prefix(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    tip: &ScopeStack,
    prefix: &ScopeStack,
) -> Option<ScopeStack> {
    if let Some(candidate_caller_rest) = tip.caller.0.strip_prefix(prefix.caller.0.as_slice()) {
        if candidate_caller_rest.is_empty() {
            // exact match
            // if tip.scope == prefix.scope {
            //     return Some()
            // }
            return None;
        }
        // eprintln!(
        //     "  strip_stack_prefix: stripped prefix, left with {} ",
        //     fmt_stack(
        //         dbg_locations,
        //         dbg_metadata_scopes,
        //         &InstructionStack(candidate_caller_rest.to_vec())
        //     )
        // );
        let candidate_rest_base_scope = dbg_locations[candidate_caller_rest[0]].scope;
        if candidate_rest_base_scope == prefix.scope {
            // match
            return Some(ScopeStack {
                caller: InstructionStack(candidate_caller_rest.to_vec()),
                scope: tip.scope,
            });
        }
    }
    None
}

// fn add_to_existing_matching_group(
//     stack_to_match: &[DbgLocationId],
//     candidate_op: &mut Op,
//     op_to_add: Op,
// ) -> bool {
//     if let OperationKind::Group { children, .. } = &mut candidate_op.kind {
//         // consider only the last child
//         if let Some(last_child) = children.last_mut() {
//             if add_to_existing_matching_group(stack_to_match, last_child, op_to_add.clone()) {
//                 return true;
//             }
//         }
//     }

//     add_to_existing_matching_group_base(stack_to_match, candidate_op, op_to_add)
// }

// fn add_to_existing_matching_group_base(
//     stack_to_match: &[DbgLocationId],
//     candidate_op: &mut Op,
//     op_to_add: Op,
// ) -> bool {
//     if let OperationKind::Group {
//         children: candidate_op_children,
//         stack: Some(candidate_stack),
//         metadata: _,
//     } = &mut candidate_op.kind
//     {
//         // check if candidate_stack is a prefix of stack_to_match

//         if stack_to_match.starts_with(candidate_stack) {
//             eprintln!(
//                 "add_to_existing_matching_group_base: found matching group with stack {:?}, adding op {:?}",
//                 candidate_stack, op_to_add
//             );
//             // add to this group
//             candidate_op
//                 .target_qubits
//                 .extend(op_to_add.target_qubits.clone());
//             candidate_op.target_qubits.sort_unstable();
//             candidate_op.target_qubits.dedup();
//             candidate_op_children.push(op_to_add);
//             return true;
//         }
//         eprintln!(
//             "add_to_existing_matching_group_base: no match for stack {:?} in candidate op {:?}",
//             stack_to_match, candidate_op
//         );
//     }
//     false
// }

struct BlockUpdate {
    operations: Vec<Op>,
    terminator: Option<Terminator>,
}

#[derive(Debug, Clone)]
enum Terminator {
    Unconditional(BlockId),
    Conditional(Branch),
}

fn get_operations_for_instruction(
    state: &mut ProgramMap,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    phis: &mut Vec<(Variable, Vec<(Expr, BlockId)>)>,
    done: &mut bool,
    instruction: &InstructionWithMetadata,
) -> Result<BlockUpdate, Error> {
    let mut terminator = None;
    let operations = match &instruction.instruction {
        Instruction::Call(callable_id, operands, var) => extend_block_with_call_instruction(
            state,
            callables,
            instruction,
            *callable_id,
            operands,
            *var,
        )?,
        Instruction::Fcmp(condition_code, operand, operand1, variable) => {
            extend_block_with_fcmp_instruction(
                state,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
            vec![]
        }
        Instruction::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                state,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
            vec![]
        }
        Instruction::Return => {
            *done = true;
            vec![]
        }
        Instruction::Branch(variable, block_id_1, block_id_2) => {
            *done = true;
            extend_block_with_branch_instruction(
                &mut terminator,
                instruction,
                *variable,
                *block_id_1,
                *block_id_2,
            )?;
            vec![]
        }
        Instruction::Jump(block_id) => {
            extend_block_with_jump_instruction(&mut terminator, *block_id)?;
            *done = true;
            vec![]
        }
        Instruction::Phi(pres, variable) => {
            extend_block_with_phi_instruction(state, phis, pres, *variable)?;
            vec![]
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
            extend_block_with_binop_instruction(state, operand, operand1, *variable)?;
            vec![]
        }
        instruction @ (Instruction::LogicalNot(..) | Instruction::BitwiseNot(..)) => {
            // Leave the variable unassigned, if it's used in anything that's going to be shown in the circuit, we'll raise an error then
            debug!("ignoring not instruction: {instruction:?}");
            vec![]
        }
        instruction @ Instruction::Store(..) => {
            return Err(Error::UnsupportedFeature(format!(
                "unsupported instruction in block: {instruction:?}"
            )));
        }
    };
    Ok(BlockUpdate {
        operations,
        terminator,
    })
}

fn extend_block_with_binop_instruction(
    state: &mut ProgramMap,
    operand: &Operand,
    operand1: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(state, operand)?;
    let expr_right = expr_from_operand(state, operand1)?;
    let expr = Expr::Rich(RichExpr::FunctionOf(
        [expr_left, expr_right]
            .into_iter()
            .flat_map(|e| e.flat_exprs())
            .collect(),
    ));
    state.store_expr_in_variable(variable, expr)?;
    Ok(())
}

fn extend_block_with_phi_instruction(
    state: &mut ProgramMap,
    phis: &mut Vec<(Variable, Vec<(Expr, BlockId)>)>,
    pres: &Vec<(Operand, BlockId)>,
    variable: Variable,
) -> Result<(), Error> {
    let mut exprs = vec![];
    let mut this_phis = vec![];
    for (var, label) in pres {
        let expr = expr_from_operand(state, var)?;
        this_phis.push((expr.clone(), *label));
        exprs.push(expr);
    }
    phis.push((variable, this_phis));

    state.store_variable_placeholder(variable);

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
    terminator: &mut Option<Terminator>,
    instruction: &InstructionWithMetadata,
    variable: Variable,
    block_id_1: BlockId,
    block_id_2: BlockId,
) -> Result<(), Error> {
    let branch = Branch {
        condition: variable,
        true_block: block_id_1,
        false_block: block_id_2,
        metadata: instruction.metadata.as_ref().map(|md| GroupMetadata {
            location: md.location.clone(),
        }),
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
    state: &mut ProgramMap,
    condition_code: ConditionCode,
    operand: &Operand,
    operand1: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    match condition_code {
        ConditionCode::Eq => {
            let expr_left = expr_from_operand(state, operand)?;
            let expr_right = expr_from_operand(state, operand1)?;
            let expr = eq_expr(expr_left, expr_right)?;
            state.store_expr_in_variable(variable, Expr::Bool(expr))
        }
        condition_code => Err(Error::UnsupportedFeature(format!(
            "unsupported condition code in icmp: {condition_code:?}"
        ))),
    }
}

fn extend_block_with_fcmp_instruction(
    state: &mut ProgramMap,
    condition_code: FcmpConditionCode,
    operand: &Operand,
    operand1: &Operand,
    variable: Variable,
) -> Result<(), Error> {
    let expr_left = expr_from_operand(state, operand)?;
    let expr_right = expr_from_operand(state, operand1)?;
    let expr = match condition_code {
        FcmpConditionCode::False => BoolExpr::LiteralBool(false),
        FcmpConditionCode::True => BoolExpr::LiteralBool(true),
        cmp => BoolExpr::BinOp(expr_left.into(), expr_right.into(), cmp.to_string()),
    };
    state.store_expr_in_variable(variable, Expr::Bool(expr))?;
    Ok(())
}

fn extend_block_with_call_instruction(
    state: &mut ProgramMap,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    instruction: &InstructionWithMetadata,
    callable_id: qsc_partial_eval::CallableId,
    operands: &Vec<Operand>,
    var: Option<Variable>,
) -> Result<Vec<Op>, Error> {
    map_callable_to_operations(
        state,
        callables.get(callable_id).expect("callable should exist"),
        operands,
        var,
        instruction.metadata.as_ref(),
    )
    .map(|ops| ops.into_iter().collect())
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
    targets: Vec<usize>,
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
    state: &ProgramMap,
    cond_expr: &Expr,
    current_block_id: BlockId,
    true_block_id: BlockId,
    false_block_id: BlockId,
) -> Result<BranchBlock, Error> {
    let CircuitBlock {
        operations: true_operations,
        terminator: true_terminator,
        ..
    } = state.blocks.get(true_block_id).expect("block should exist");
    let CircuitBlock {
        operations: false_operations,
        terminator: false_terminator,
        ..
    } = state
        .blocks
        .get(false_block_id)
        .expect("block should exist");

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
            seen.insert((*q, None));
        }
        for r in op.target_results.iter().chain(&op.control_results) {
            seen.insert((r.0, Some(r.1)));
        }
    }
    // TODO: actually test measurements in branches

    // if seen.iter().any(|(_, r)| r.is_some()) {
    //     return Err(Error::UnsupportedFeature(
    //         "measurement operation in a branch block".to_owned(),
    //     ));
    // }

    // TODO: everything is a target. Don't know how else we would do this.
    let target_qubits = seen.into_iter().map(|(q, _)| q).collect();
    Ok(ConditionalBlock {
        operations: real_ops,
        targets: target_qubits,
    })
}

fn expr_from_operand(state: &ProgramMap, operand: &Operand) -> Result<Expr, Error> {
    match operand {
        Operand::Literal(literal) => match literal {
            Literal::Result(r) => Ok(Expr::Bool(BoolExpr::Result(*r))),
            Literal::Bool(b) => Ok(Expr::Bool(BoolExpr::LiteralBool(*b))),
            Literal::Integer(i) => Ok(Expr::Rich(RichExpr::Literal(i.to_string()))),
            Literal::Double(d) => Ok(Expr::Rich(RichExpr::Literal(d.to_string()))),
            _ => Err(Error::UnsupportedFeature(format!(
                "unsupported literal operand: {literal:?}"
            ))),
        },
        Operand::Variable(variable) => state.expr_for_variable(variable.variable_id).cloned(),
    }
}

struct ProgramMap {
    /// qubit decl, result idx -> result id
    qubits: Vec<(Qubit, Vec<u32>)>,
    /// result id -> qubit id
    results: IndexMap<usize, u32>,
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
    Result(u32),
    NotResult(u32),
    TwoResultCondition {
        results: (u32, u32),
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

    fn linked_results(&self) -> Vec<u32> {
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

impl ProgramMap {
    fn into_qubits(self) -> Vec<Qubit> {
        self.qubits
            .into_iter()
            .map(|(q, results)| Qubit {
                id: q.id,
                num_results: results.len(),
            })
            .collect()
    }

    fn new(num_qubits: u32) -> Self {
        Self {
            qubits: (0..num_qubits)
                .map(|id| {
                    (
                        Qubit {
                            id: usize::try_from(id).expect("qubit id should fit in usize"),
                            num_results: 0,
                        },
                        vec![],
                    )
                })
                .collect::<Vec<_>>(),
            variables: IndexMap::new(),
            blocks: IndexMap::new(),
            results: IndexMap::new(),
        }
    }

    fn result_register(&mut self, result_id: u32) -> Register {
        let qubit_id = self
            .results
            .get(usize::try_from(result_id).expect("result id should fit into usize"))
            .copied()
            .expect("result should be linked to a qubit");

        let qubit_result_idx = self.link_result_to_qubit(qubit_id, result_id);

        Register {
            qubit: usize::try_from(qubit_id).expect("qubit id should fit in usize"),
            result: Some(qubit_result_idx),
        }
    }

    fn expr_for_variable(&self, variable_id: VariableId) -> Result<&Expr, Error> {
        let expr = self.variables.get(variable_id);
        eprintln!("debug: expr for variable {variable_id:?} is {expr:?}");
        Ok(expr.unwrap_or_else(|| {
            panic!("variable {variable_id:?} is not linked to a result or expression")
        }))
    }

    fn link_result_to_qubit(&mut self, qubit_id: u32, result_id: u32) -> usize {
        self.results.insert(
            result_id
                .try_into()
                .expect("result id should fit into usize"),
            qubit_id,
        );
        let result_ids_for_qubit =
            &mut self.qubits[usize::try_from(qubit_id).expect("qubit id should fit in usize")].1;
        let qubit_result_idx = result_ids_for_qubit
            .iter_mut()
            .enumerate()
            .find(|(_, qubit_r)| **qubit_r == result_id)
            .map(|(a, _)| a);

        qubit_result_idx.unwrap_or_else(|| {
            result_ids_for_qubit.push(result_id);
            result_ids_for_qubit.len() - 1
        })
    }

    fn store_expr_in_variable(&mut self, var: Variable, expr: Expr) -> Result<(), Error> {
        let variable_id = var.variable_id;
        if let Some(old_value) = self.variables.get(variable_id) {
            if old_value.is_unresolved() {
                // allow overwriting unresolved variables
                eprintln!("note: variable {variable_id:?} was unresolved, now storing {expr:?}");
            } else if old_value != &expr {
                panic!(
                    "variable {variable_id:?} already stored {old_value:?}, cannot store {expr:?}"
                );
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

        self.variables.insert(variable_id, expr);
        Ok(())
    }

    fn store_variable_placeholder(&mut self, variable: Variable) {
        if self.variables.get(variable.variable_id).is_none() {
            self.variables
                .insert(variable.variable_id, Expr::Unresolved(variable.variable_id));
        }
    }
}

fn map_callable_to_operations(
    state: &mut ProgramMap,
    callable: &Callable,
    operands: &Vec<Operand>,
    var: Option<Variable>,
    metadata: Option<&InstructionMetadata>,
) -> Result<Vec<Op>, Error> {
    Ok(match callable.call_type {
        CallableType::Measurement => {
            map_measurement_call_to_operations(state, callable, operands, metadata)?
        }
        CallableType::Reset => map_reset_call_into_operations(state, callable, operands, metadata)?,
        CallableType::Readout => match callable.name.as_str() {
            "__quantum__qis__read_result__body" => {
                for operand in operands {
                    match operand {
                        Operand::Literal(Literal::Result(r)) => {
                            let var =
                                var.expect("read_result must have a variable to store the result");
                            state.store_expr_in_variable(var, Expr::Bool(BoolExpr::Result(*r)))?;
                        }
                        operand => {
                            return Err(Error::UnsupportedFeature(format!(
                                "operand for result readout is not a result: {operand:?}"
                            )));
                        }
                    }
                }
                vec![]
            }
            name => {
                return Err(Error::UnsupportedFeature(format!(
                    "unknown readout callable: {name}"
                )));
            }
        },
        CallableType::OutputRecording => {
            vec![]
        }
        CallableType::Regular => {
            let (gate, operand_types) = callable_spec(callable, operands)?;
            if let Some(var) = var {
                let result_expr = Expr::Rich(RichExpr::FunctionOf(
                    operands
                        .iter()
                        .map(|o| expr_from_operand(state, o))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .flat_map(|e| e.flat_exprs())
                        .collect(),
                ));

                state.store_expr_in_variable(var, result_expr)?;
            }

            let (targets, controls, args) = gather_operands(state, &operand_types, operands)?;

            if targets.is_empty() && controls.is_empty() {
                // Skip operations without targets or controls.
                // Alternative might be to include these anyway, across the entire state,
                // or annotated in the circuit in some way.
                vec![]
            } else {
                vec![Op {
                    kind: OperationKind::Unitary {
                        metadata: metadata.cloned(),
                    },
                    label: gate.to_string(),
                    target_qubits: targets
                        .iter()
                        .filter_map(|r| {
                            if r.result.is_some() {
                                None
                            } else {
                                Some(r.qubit)
                            }
                        })
                        .collect(),
                    control_qubits: controls
                        .iter()
                        .filter_map(|r| {
                            if r.result.is_some() {
                                None
                            } else {
                                Some(r.qubit)
                            }
                        })
                        .collect(),
                    target_results: targets
                        .iter()
                        .filter_map(|reg| reg.result.map(|r| (reg.qubit, r)))
                        .collect(),
                    control_results: controls
                        .iter()
                        .filter_map(|reg| reg.result.map(|r| (reg.qubit, r)))
                        .collect(),
                    is_adjoint: false,
                    args,
                }]
            }
        }
    })
}

fn map_reset_call_into_operations(
    state: &mut ProgramMap,
    callable: &Callable,
    operands: &[Operand],
    metadata: Option<&InstructionMetadata>,
) -> Result<Vec<Op>, Error> {
    Ok(match callable.name.as_str() {
        "__quantum__qis__reset__body" => {
            let operand_types = vec![OperandType::Target];
            let (targets, _, _) = gather_operands(state, &operand_types, operands)?;

            vec![Op {
                kind: OperationKind::Ket {
                    metadata: metadata.cloned(),
                },
                label: "0".to_string(),
                target_qubits: targets
                    .iter()
                    .filter_map(|r| {
                        if r.result.is_some() {
                            None
                        } else {
                            Some(r.qubit)
                        }
                    })
                    .collect(),
                control_qubits: vec![],
                target_results: targets
                    .iter()
                    .filter_map(|reg| reg.result.map(|r| (reg.qubit, r)))
                    .collect(),
                control_results: vec![],
                is_adjoint: false,
                args: vec![],
            }]
        }
        name => {
            return Err(Error::UnsupportedFeature(format!(
                "unknown reset callable: {name}"
            )));
        }
    })
}

fn map_measurement_call_to_operations(
    state: &mut ProgramMap,
    callable: &Callable,
    operands: &Vec<Operand>,
    metadata: Option<&InstructionMetadata>,
) -> Result<Vec<Op>, Error> {
    let gate = match callable.name.as_str() {
        "__quantum__qis__m__body" => "M",
        "__quantum__qis__mresetz__body" => "MResetZ",
        name => name,
    };
    let (this_qubits, this_results) = gather_measurement_operands(state, operands)?;
    Ok(if gate == "MResetZ" {
        vec![
            Op {
                kind: OperationKind::Measurement {
                    metadata: metadata.cloned(),
                },
                label: gate.to_string(),
                target_qubits: vec![],
                control_qubits: this_qubits
                    .iter()
                    .filter_map(|r| {
                        if r.result.is_some() {
                            None
                        } else {
                            Some(r.qubit)
                        }
                    })
                    .collect(),
                target_results: this_results
                    .iter()
                    .map(|r| {
                        (
                            r.qubit,
                            r.result.expect("result register must have result idx"),
                        )
                    })
                    .collect(),
                control_results: vec![],
                is_adjoint: false,
                args: vec![],
            },
            Op {
                kind: OperationKind::Ket {
                    metadata: metadata.cloned(),
                },
                label: "0".to_string(),
                target_qubits: this_qubits
                    .iter()
                    .filter_map(|r| {
                        if r.result.is_some() {
                            None
                        } else {
                            Some(r.qubit)
                        }
                    })
                    .collect(),
                control_qubits: vec![],
                target_results: vec![],
                control_results: vec![],
                is_adjoint: false,
                args: vec![],
            },
        ]
    } else {
        vec![Op {
            kind: OperationKind::Measurement {
                metadata: metadata.cloned(),
            },
            label: gate.to_string(),
            target_qubits: vec![],
            control_qubits: this_qubits
                .iter()
                .filter_map(|r| {
                    if r.result.is_some() {
                        None
                    } else {
                        Some(r.qubit)
                    }
                })
                .collect(),
            target_results: this_results
                .iter()
                .map(|r| {
                    (
                        r.qubit,
                        r.result.expect("result register must have result idx"),
                    )
                })
                .collect(),
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        }]
    })
}

fn callable_spec<'a>(
    callable: &'a Callable,
    operands: &[Operand],
) -> Result<(&'a str, Vec<OperandType>), Error> {
    Ok(match callable.name.as_str() {
        // single-qubit gates
        "__quantum__qis__x__body" => ("X", vec![OperandType::Target]),
        "__quantum__qis__y__body" => ("Y", vec![OperandType::Target]),
        "__quantum__qis__z__body" => ("Z", vec![OperandType::Target]),
        "__quantum__qis__s__body" => ("S", vec![OperandType::Target]),
        "__quantum__qis__s__adj" => ("S'", vec![OperandType::Target]),
        "__quantum__qis__h__body" => ("H", vec![OperandType::Target]),
        "__quantum__qis__rx__body" => ("Rx", vec![OperandType::Arg, OperandType::Target]),
        "__quantum__qis__ry__body" => ("Ry", vec![OperandType::Arg, OperandType::Target]),
        "__quantum__qis__rz__body" => ("Rz", vec![OperandType::Arg, OperandType::Target]),
        // multi-qubit gates
        "__quantum__qis__cx__body" => ("X", vec![OperandType::Control, OperandType::Target]),
        "__quantum__qis__cy__body" => ("Y", vec![OperandType::Control, OperandType::Target]),
        "__quantum__qis__cz__body" => ("Z", vec![OperandType::Control, OperandType::Target]),
        "__quantum__qis__ccx__body" => (
            "X",
            vec![
                OperandType::Control,
                OperandType::Control,
                OperandType::Target,
            ],
        ),
        "__quantum__qis__rxx__body" => (
            "Rxx",
            vec![OperandType::Arg, OperandType::Target, OperandType::Target],
        ),
        "__quantum__qis__ryy__body" => (
            "Ryy",
            vec![OperandType::Arg, OperandType::Target, OperandType::Target],
        ),
        "__quantum__qis__rzz__body" => (
            "Rzz",
            vec![OperandType::Arg, OperandType::Target, OperandType::Target],
        ),
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

            (custom, operand_types)
        }
    })
}

fn gather_measurement_operands(
    state: &mut ProgramMap,
    operands: &Vec<Operand>,
) -> Result<(Vec<Register>, Vec<Register>), Error> {
    let mut qubit_registers = vec![];
    let mut result_registers = vec![];
    let mut qubit_id = None;
    for operand in operands {
        match operand {
            Operand::Literal(Literal::Qubit(q)) => {
                let old = qubit_id.replace(q);
                if old.is_some() {
                    return Err(Error::UnsupportedFeature(format!(
                        "measurement should only have one qubit operand, found {old:?} and {q}"
                    )));
                }
                qubit_registers.push(Register {
                    qubit: usize::try_from(*q).expect("qubit id should fit in usize"),
                    result: None,
                });
            }
            Operand::Literal(Literal::Result(r)) => {
                let q = *qubit_id.expect("measurement should have a qubit operand");
                state.link_result_to_qubit(q, *r);
                let result_register = state.result_register(*r);
                result_registers.push(result_register);
            }
            o => {
                return Err(Error::UnsupportedFeature(format!(
                    "unsupported operand for measurement: {o:?}"
                )));
            }
        }
    }
    Ok((qubit_registers, result_registers))
}

enum OperandType {
    Control,
    Target,
    Arg,
}

type TargetsControlsArgs = (Vec<Register>, Vec<Register>, Vec<String>);

fn gather_operands(
    state: &mut ProgramMap,
    operand_types: &[OperandType],
    operands: &[Operand],
) -> Result<TargetsControlsArgs, Error> {
    let mut targets = vec![];
    let mut controls = vec![];
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
                    let operands_array = match operand_type {
                        OperandType::Control => &mut controls,
                        OperandType::Target => &mut targets,
                        OperandType::Arg => {
                            return Err(Error::UnsupportedFeature(
                                "qubit operand cannot be an argument".to_owned(),
                            ));
                        }
                    };
                    operands_array.push(Register {
                        qubit: usize::try_from(*q).expect("qubit id should fit in usize"),
                        result: None,
                    });
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
                    let expr = state.expr_for_variable(var.variable_id)?.clone();
                    // Add classical controls if this expr is dependent on a result
                    let results = expr
                        .linked_results()
                        .into_iter()
                        .map(|r| state.result_register(r))
                        .collect::<Vec<_>>();
                    for r in results {
                        if !controls.contains(&r) {
                            controls.push(r);
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
    Ok((targets, controls, args))
}
