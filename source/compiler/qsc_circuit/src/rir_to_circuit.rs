// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;
pub(crate) mod tracer;

use std::{
    fmt::{Display, Write},
    vec,
};

use crate::{
    Circuit, ComponentColumn, Config, Error, GenerationMethod, Ket, Measurement, Operation,
    Register, Unitary, group_qubits, operation_list_to_grid,
    rir_to_circuit::tracer::{
        BlockBuilder, FixedQubitRegisterMap, GateInputs, QubitRegister, RegisterMap,
        ResultRegister, Tracer,
    },
};
use log::{debug, warn};
use qsc_data_structures::{index_map::IndexMap, line_column::Encoding};
use qsc_frontend::{compile::PackageStore, location::Location};
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

#[derive(Clone, Debug)]
struct Branch {
    condition: Variable,
    true_block: BlockId,
    false_block: BlockId,
    metadata: Option<GroupMetadata>,
    cond_expr_instruction_metadata: Option<InstructionMetadata>,
}

#[derive(Clone, Debug)]
pub(crate) struct Op {
    kind: OperationKind,
    label: String,
    target_qubits: Vec<QubitRegister>,
    control_qubits: Vec<QubitRegister>,
    target_results: Vec<ResultRegister>,
    control_results: Vec<ResultRegister>,
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

#[derive(Clone, Debug, PartialEq)]
struct InstructionStack(Vec<DbgLocationId>); // Can be empty

impl InstructionStack {
    fn scope_stack(&self, dbg_locations: &[DbgLocation]) -> Option<ScopeStack> {
        self.0.split_last().map(|(top, prefix)| ScopeStack {
            caller: InstructionStack(prefix.to_vec()),
            scope: dbg_locations[*top].scope,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ScopeStack {
    caller: InstructionStack,
    scope: DbgScopeId,
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
        scope_stack: Option<ScopeStack>,
        instruction_stack: Option<InstructionMetadata>,
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
                qubit: q.0,
                result: None,
            })
            .chain(
                value
                    .target_results
                    .into_iter()
                    .map(|ResultRegister(q, r)| Register {
                        qubit: q,
                        result: Some(r),
                    }),
            )
            .collect();
        let controls = value
            .control_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q.0,
                result: None,
            })
            .chain(
                value
                    .control_results
                    .into_iter()
                    .map(|ResultRegister(q, r)| Register {
                        qubit: q,
                        result: Some(r),
                    }),
            )
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
                scope_stack: _,
                metadata: _,
                instruction_stack: _,
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
    let dbg_info = DbgInfo {
        dbg_locations: &program.dbg_locations,
        dbg_metadata_scopes: &program.dbg_metadata_scopes,
    };
    assert!(config.generation_method == GenerationMethod::Static);
    let mut program_map = ProgramMap::new();
    let mut register_map = FixedQubitRegisterMap::new(program.num_qubits);
    let callables = &program.callables;

    let mut i = 0;
    let mut done = false;
    while !done {
        for (id, block) in program.blocks.iter() {
            let block_operations = process_block_vars(
                &dbg_info,
                &mut program_map,
                &mut register_map,
                callables,
                block,
            )?;
            program_map.blocks.insert(id, block_operations);
        }

        done = expand_branches_vars(&mut register_map, program, &mut program_map)?;
        program_map.blocks.clear();
        i += 1;
        if i > 100 {
            warn!("make_circuit: too many iterations expanding branches, giving up");
            return Err(Error::UnsupportedFeature(
                "too many iterations expanding branches".to_owned(),
            ));
        }
    }

    let mut ops_remaining = config.max_operations;

    // Do it all again, with all variables properly resolved
    for (id, block) in program.blocks.iter() {
        let block_operations = operations_in_block(
            &mut program_map,
            &register_map,
            &dbg_info,
            callables,
            block,
            ops_remaining,
        )?;

        ops_remaining = ops_remaining.saturating_sub(block_operations.operations.len());

        program_map.blocks.insert(id, block_operations);
    }

    expand_branches(&mut program_map, &register_map, program)?;

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

    let operations = extend_with_successors(&program_map, entry_block);

    let mut operations = if config.group_scopes {
        group_operations(
            &program.dbg_locations,
            &program.dbg_metadata_scopes,
            operations,
        )
    } else {
        operations
    };

    let qubits = register_map.into_qubits();
    fill_in_dbg_metadata(&dbg_info, &mut operations, package_store, position_encoding)?;
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
fn expand_branches_vars(
    register_map: &mut FixedQubitRegisterMap,
    program: &Program,
    state: &mut ProgramMap,
) -> Result<bool, Error> {
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
            let expanded_branch = expand_branch_vars(state, register_map, block_id, &branch)?;

            if let Some(expanded_branch) = expanded_branch {
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
    register_map: &mut FixedQubitRegisterMap,
    curent_block_id: BlockId,
    branch: &Branch,
) -> Result<Option<ExpandedBranchBlock>, Error> {
    let cond_expr = state.expr_for_variable(branch.condition.variable_id)?;
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
        state,
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
        .map(|r| register_map.result_register(*r))
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

    Ok(Some(ExpandedBranchBlock {
        _condition: branch.condition,
        grouped_operation: Op {
            kind: OperationKind::Group {
                children: children.into_iter().collect(),
                scope_stack: None,
                metadata: branch.metadata.clone(),
                instruction_stack: branch.cond_expr_instruction_metadata.clone(),
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

fn make_group_op(
    label: &str,
    operations: &[Op],
    targets: &[QubitRegister],
    control_results: &[ResultRegister],
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
        kind: OperationKind::Group {
            children,
            scope_stack: None,
            metadata: None,
            instruction_stack: None,
        },
        label: label.into(),
        args: vec![],
        target_qubits: targets.to_vec(),
        control_qubits: vec![],
        target_results: vec![],
        control_results: control_results.to_vec(),
        is_adjoint: false,
    }
}

fn process_block_vars(
    dbg_info: &DbgInfo,
    state: &mut ProgramMap,
    register_map: &mut FixedQubitRegisterMap,
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
            state,
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
    register_map: &FixedQubitRegisterMap,
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
    dbg_info: &DbgInfo,
    operations: &mut [Op],
    package_store: &PackageStore,
    position_encoding: Encoding,
) -> Result<(), Error> {
    for op in operations {
        if let OperationKind::Group { children, .. } = &mut op.kind {
            fill_in_dbg_metadata(dbg_info, children, package_store, position_encoding)?;
        }

        let location = match &op.kind {
            OperationKind::Unitary { metadata }
            | OperationKind::Measurement { metadata }
            | OperationKind::Ket { metadata } => metadata
                .as_ref()
                .and_then(|metadata| instruction_logical_stack(dbg_info, metadata))
                .and_then(|s| s.0.last().copied())
                .map(|l| dbg_info.dbg_locations[l].location.clone()),
            OperationKind::Group {
                children: _,
                scope_stack: _,
                metadata,
                instruction_stack,
            } => instruction_stack
                .as_ref()
                .and_then(|metadata| instruction_logical_stack(dbg_info, metadata))
                .and_then(|s| s.0.last().copied())
                .map(|l| dbg_info.dbg_locations[l].location.clone())
                .or(metadata.as_ref().map(|md| md.location.clone())),
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
    register_map: &FixedQubitRegisterMap,
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

    let ConditionalBlock {
        operations: true_operations,
        targets: true_targets,
    } = branch_block.true_block;

    let control_results = results
        .iter()
        .map(|r| register_map.result_register(*r))
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

    Ok(ExpandedBranchBlock {
        _condition: branch.condition,
        grouped_operation: Op {
            kind: OperationKind::Group {
                children: children.into_iter().collect(),
                scope_stack: None,
                metadata: branch.metadata.clone(),
                instruction_stack: branch.cond_expr_instruction_metadata.clone(),
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
    register_map: &FixedQubitRegisterMap,
    dbg_info: &DbgInfo,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    block: &BlockWithMetadata,
    ops_remaining: usize,
) -> Result<CircuitBlock, Error> {
    // TODO: use get_block_successors from utils
    let mut terminator = None;
    let mut phis = vec![];
    let mut done = false;

    let mut builder = BlockBuilder::new(ops_remaining);
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

fn group_operations(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    new_operations: Vec<Op>,
) -> Vec<Op> {
    let dbg_info = DbgInfo {
        dbg_locations,
        dbg_metadata_scopes,
    };
    let mut operations = vec![];
    for op in new_operations {
        let instruction_metadata = match &op.kind {
            OperationKind::Unitary { metadata }
            | OperationKind::Measurement { metadata }
            | OperationKind::Ket { metadata }
            | OperationKind::Group {
                instruction_stack: metadata,
                ..
            } => metadata.as_ref(),
        };
        let instruction_stack = instruction_metadata.and_then(|instruction_metadata| {
            instruction_logical_stack(&dbg_info, instruction_metadata)
        });

        add_op(&mut operations, op, &dbg_info, instruction_stack.as_ref());
    }
    operations
}

fn add_op(
    block_operations: &mut Vec<Op>,
    op: Op,
    dbg_info: &DbgInfo,
    instruction_stack: Option<&InstructionStack>,
) {
    match instruction_stack {
        Some(instruction_stack) => {
            let qubits: FxHashSet<QubitRegister> = op
                .control_qubits
                .iter()
                .chain(&op.target_qubits)
                .copied()
                .collect();

            let target_qubits = qubits.into_iter().collect();
            let results: FxHashSet<ResultRegister> = op
                .control_results
                .iter()
                .chain(&op.target_results)
                .copied()
                .collect();
            let target_results = results.into_iter().collect();

            add_scoped_op(
                block_operations,
                None,
                dbg_info,
                op,
                instruction_stack,
                target_qubits,
                target_results,
            );
        }
        None => block_operations.push(op),
    }
}

fn instruction_logical_stack(
    dbg_info: &DbgInfo,
    metadata: &InstructionMetadata,
) -> Option<InstructionStack> {
    if let Some(dbg_location_idx) = metadata.dbg_location {
        let mut location_stack = vec![];
        let mut current_location_idx = Some(dbg_location_idx);

        while let Some(location_idx) = current_location_idx {
            location_stack.push(location_idx);
            let location = dbg_info
                .dbg_locations
                .get(location_idx)
                .expect("dbg location should exist");
            current_location_idx = location.inlined_at;
        }

        // filter out scopes in std and core
        location_stack.retain(|location| {
            let scope = &dbg_info.dbg_metadata_scopes[dbg_info.dbg_locations[*location].scope];
            match scope {
                DbgMetadataScope::SubProgram { name: _, location } => {
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

fn scope_label(dbg_info: &DbgInfo, scope_stack: &ScopeStack) -> String {
    scope_name(dbg_info.dbg_metadata_scopes, scope_stack.scope)
}

fn loc_name(dbg_info: &DbgInfo, location: DbgLocationId) -> (String, u32) {
    let dbg_location = &dbg_info.dbg_locations[location];
    let scope_id: DbgScopeId = dbg_location.scope;
    let scope_name = scope_name(dbg_info.dbg_metadata_scopes, scope_id);
    let offset = dbg_location.location.span.lo;

    (scope_name, offset)
}

fn scope_name(dbg_metadata_scopes: &[DbgMetadataScope], scope_id: usize) -> String {
    let scope = &dbg_metadata_scopes[scope_id];

    match scope {
        DbgMetadataScope::SubProgram { name, location: _ } => name.to_string(),
    }
}

#[allow(dead_code)]
fn fmt_scope_stack(dbg_info: &DbgInfo, stack: &ScopeStack) -> String {
    let mut names: Vec<String> = stack
        .caller
        .0
        .iter()
        .map(|loc| fmt_loc(dbg_info, *loc))
        .collect();
    names.push(scope_name(dbg_info.dbg_metadata_scopes, stack.scope));
    names.join("->")
}

#[allow(dead_code)]
fn fmt_ops(
    dbg_locations: &[DbgLocation],
    dbg_metadata_scopes: &[DbgMetadataScope],
    ops: &[Op],
) -> String {
    let dbg_info = DbgInfo {
        dbg_locations,
        dbg_metadata_scopes,
    };
    let items: Vec<String> = ops
        .iter()
        .map(|op| {
            let name = &op.label;
            let stack_and_children = match &op.kind {
                OperationKind::Group {
                    children,
                    scope_stack: stack,
                    metadata: _metadata,
                    instruction_stack: _,
                } => {
                    format!(
                        "{}children={}",
                        match stack {
                            Some(stack) => format!("stack={}, ", fmt_scope_stack(&dbg_info, stack)),
                            None => String::new(),
                        },
                        fmt_ops_with_trailing_comma(&dbg_info, children)
                    )
                }
                _ => String::new(),
            };
            if stack_and_children.is_empty() {
                format!(
                    "({name}, q={:?})",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>()
                )
            } else {
                format!(
                    "({name}, q={:?}, {})",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>(),
                    stack_and_children
                )
            }
        })
        .collect();
    let mut s = String::new();
    let _ = writeln!(s, "[");
    for item in items {
        let _ = writeln!(s, "  {item}");
    }
    let _ = writeln!(s, "]");

    s
}

#[allow(dead_code)]
fn fmt_ops_with_trailing_comma(dbg_info: &DbgInfo, ops: &[Op]) -> String {
    let items: Vec<String> = ops
        .iter()
        .map(|op| {
            let name = &op.label;
            let stack_and_children = match &op.kind {
                OperationKind::Group {
                    children,
                    scope_stack: stack,
                    metadata: _metadata,
                    instruction_stack: _,
                } => {
                    format!(
                        "{}children={}",
                        match stack {
                            Some(stack) => format!("stack={}, ", fmt_scope_stack(dbg_info, stack)),
                            None => String::new(),
                        },
                        fmt_ops_with_trailing_comma(dbg_info, children)
                    )
                }
                _ => String::new(),
            };
            if stack_and_children.is_empty() {
                format!(
                    "({name}, q={:?})",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>()
                )
            } else {
                format!(
                    "({name}, q={:?}), {}",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>(),
                    stack_and_children
                )
            }
        })
        .collect();
    format!(
        "[{}]",
        if items.is_empty() {
            String::new()
        } else {
            format!("{}, ", items.join(", "))
        }
    )
}

fn fmt_loc(dbg_info: &DbgInfo, location: usize) -> String {
    let (name, offset) = loc_name(dbg_info, location);
    format!("{name}@{offset}")
}

fn make_scope_metadata(dbg_info: &DbgInfo, scope_stack: &ScopeStack) -> GroupMetadata {
    let scope_location = &dbg_info.dbg_metadata_scopes[scope_stack.scope];
    let scope_location = match scope_location {
        DbgMetadataScope::SubProgram { location: span, .. } => span,
    };

    GroupMetadata {
        location: scope_location.clone(),
    }
}

struct DbgInfo<'a> {
    dbg_locations: &'a [DbgLocation],
    dbg_metadata_scopes: &'a [DbgMetadataScope],
}

fn add_scoped_op(
    current_scope_container: &mut Vec<Op>,
    current_scope: Option<ScopeStack>,
    dbg_info: &DbgInfo,
    op: Op,
    instruction_stack: &InstructionStack,
    target_qubits: Vec<QubitRegister>,
    target_results: Vec<ResultRegister>,
) {
    let full_instruction_stack = concat_stacks(
        dbg_info.dbg_locations,
        current_scope.as_ref(),
        instruction_stack,
    );
    let scope_stack = instruction_stack.scope_stack(dbg_info.dbg_locations);

    if let Some(scope_stack) = scope_stack
        && Some(&scope_stack) != current_scope.as_ref()
    {
        // there is a scope
        if let Some(last_op) = current_scope_container.last_mut() {
            if let OperationKind::Group {
                children: last_scope_children,
                scope_stack: Some(last_scope_stack),
                metadata: _,
                instruction_stack: _,
            } = &mut last_op.kind
            {
                if let Some(rest) =
                    strip_stack_prefix(dbg_info, &full_instruction_stack, last_scope_stack)
                {
                    last_op.target_qubits.extend(target_qubits.clone());
                    last_op.target_qubits.sort_unstable();
                    last_op.target_qubits.dedup();

                    last_op.target_results.extend(target_results.clone());
                    last_op.target_results.sort_unstable();
                    last_op.target_results.dedup();

                    // Recursively add to the children
                    add_scoped_op(
                        last_scope_children,
                        Some(last_scope_stack.clone()),
                        dbg_info,
                        op,
                        &rest,
                        target_qubits,
                        target_results,
                    );

                    return;
                }
            }
        }

        // we need to create a parent for the scope
        let scope_metadata = make_scope_metadata(dbg_info, &scope_stack);
        let label = scope_label(dbg_info, &scope_stack);
        let full_scope_stack = full_instruction_stack
            .scope_stack(dbg_info.dbg_locations)
            .expect("we got here because we had a scope, so what the hell is this");

        if current_scope != Some(full_scope_stack.clone()) {
            let scope_group = Op {
                kind: OperationKind::Group {
                    children: vec![op],
                    scope_stack: Some(full_scope_stack),
                    metadata: Some(scope_metadata),
                    instruction_stack: None,
                },
                label,
                target_qubits,
                control_qubits: vec![],
                target_results,
                control_results: vec![],
                is_adjoint: false,
                args: vec![],
            };

            // create container for the prefix, and add to it
            add_scoped_op(
                current_scope_container,
                current_scope,
                dbg_info,
                scope_group.clone(),
                &scope_stack.caller,
                scope_group.target_qubits.clone(),
                scope_group.target_results.clone(),
            );
            return;
        }
    }
    // no scope, top level, just push to current operations
    current_scope_container.push(op);
}

fn concat_stacks(
    dbg_locations: &[DbgLocation],
    scope: Option<&ScopeStack>,
    tail: &InstructionStack,
) -> InstructionStack {
    match scope {
        Some(prefix) => {
            if let Some(first) = tail.0.first() {
                assert_eq!(
                    dbg_locations[*first].scope, prefix.scope,
                    "concatenating stacks that don't seem to match"
                );
            }
            InstructionStack([prefix.caller.0.clone(), tail.0.clone()].concat())
        }
        None => tail.clone(),
    }
}

fn strip_stack_prefix(
    dbg_info: &DbgInfo,
    full: &InstructionStack,
    prefix: &ScopeStack,
) -> Option<InstructionStack> {
    if full.0.len() > prefix.caller.0.len() {
        if let Some(rest) = full.0.strip_prefix(prefix.caller.0.as_slice()) {
            let next_location = rest[0];
            let next_scope = dbg_info.dbg_locations[next_location].scope;
            if next_scope == prefix.scope {
                return Some(InstructionStack(rest.to_vec()));
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
enum Terminator {
    Unconditional(BlockId),
    Conditional(Branch),
}

fn get_operations_for_instruction_vars_only(
    dbg_info: &DbgInfo,
    state: &mut ProgramMap,
    register_map: &mut FixedQubitRegisterMap,
    callables: &IndexMap<qsc_partial_eval::CallableId, Callable>,
    phis: &mut Vec<(Variable, Vec<(Expr, BlockId)>)>,
    done: &mut bool,
    instruction: &InstructionWithMetadata,
) -> Result<Option<Terminator>, Error> {
    let mut terminator = None;
    match &instruction.instruction {
        Instruction::Call(callable_id, operands, var) => {
            process_callable_variables(
                state,
                register_map,
                callables.get(*callable_id).expect("callable should exist"),
                operands,
                *var,
            )?;
        }
        Instruction::Fcmp(condition_code, operand, operand1, variable) => {
            extend_block_with_fcmp_instruction(
                state,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instruction::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                state,
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
            extend_block_with_phi_instruction(state, phis, pres, *variable)?;
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
    builder: &'a mut BlockBuilder,
    register_map: &'a FixedQubitRegisterMap,
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
                state,
                *condition_code,
                operand,
                operand1,
                *variable,
            )?;
        }
        Instruction::Icmp(condition_code, operand, operand1, variable) => {
            extend_block_with_icmp_instruction(
                state,
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
            extend_block_with_phi_instruction(state, phis, pres, *variable)?;
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
    dbg_info: &DbgInfo,
    terminator: &mut Option<Terminator>,
    instruction: &InstructionWithMetadata,
    variable: Variable,
    block_id_1: BlockId,
    block_id_2: BlockId,
) -> Result<(), Error> {
    let instruction_metadata = instruction.metadata.clone();
    let metadata = instruction_metadata.as_ref().map(|md| GroupMetadata {
        location: md
            .dbg_location
            .map(|l| dbg_info.dbg_locations[l].location.clone())
            .unwrap_or_default(),
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
    targets: Vec<QubitRegister>,
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
            seen.insert((q.0, None));
        }
        for ResultRegister(q, r) in op.target_results.iter().chain(&op.control_results) {
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
    let target_qubits = seen.into_iter().map(|(q, _)| QubitRegister(q)).collect();
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
    fn new() -> Self {
        Self {
            variables: IndexMap::new(),
            blocks: IndexMap::new(),
        }
    }

    fn expr_for_variable(&self, variable_id: VariableId) -> Result<&Expr, Error> {
        let expr = self.variables.get(variable_id);
        Ok(expr.unwrap_or_else(|| {
            panic!("variable {variable_id:?} is not linked to a result or expression")
        }))
    }

    fn store_expr_in_variable(&mut self, var: Variable, expr: Expr) -> Result<(), Error> {
        let variable_id = var.variable_id;
        if let Some(old_value) = self.variables.get(variable_id) {
            if old_value.is_unresolved() {
                // allow overwriting unresolved variables
                debug!("note: variable {variable_id:?} was unresolved, now storing {expr:?}");
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

fn process_callable_variables(
    state: &mut ProgramMap,
    register_map: &mut FixedQubitRegisterMap,
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
                            state.store_expr_in_variable(var, Expr::Bool(BoolExpr::Result(*r)))?;
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
                        .map(|o| expr_from_operand(state, o))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .flat_map(|e| e.flat_exprs())
                        .collect(),
                ));

                state.store_expr_in_variable(var, result_expr)?;
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
        name: gate,
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
            gate,
            is_adjoint,
            GateInputs {
                target_qubits,
                control_qubits,
                control_results,
            },
            args,
            metadata.cloned(),
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
            builder_ctx
                .builder
                .reset(builder_ctx.register_map, qubit, metadata.cloned());
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
            builder_ctx
                .builder
                .mresetz(builder_ctx.register_map, qubit, result, metadata.cloned());
        }
        "__quantum__qis__m__body" => {
            builder_ctx
                .builder
                .m(builder_ctx.register_map, qubit, result, metadata.cloned());
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

fn gather_measurement_operands_inner(operands: &[Operand]) -> Result<(u32, u32), Error> {
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
        Operand::Literal(Literal::Result(r)) => Some(r),
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

    Ok((*qubit, *result))
}

enum OperandType {
    Control,
    Target,
    Arg,
}

type Operands = (Vec<u32>, Vec<u32>, Vec<u32>, Vec<String>);

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
                    qubit_operands_array.push(*q);
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
