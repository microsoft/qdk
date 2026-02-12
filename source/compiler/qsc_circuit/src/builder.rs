// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
pub(crate) mod tests;

use crate::{
    circuit::{
        Circuit, ComponentColumn, Ket, Measurement, Metadata, Operation, Qubit, Register,
        SourceLocation, Unitary, operation_list_to_grid,
    },
    operations::QubitParam,
};
use qsc_data_structures::{
    functors::FunctorApp,
    index_map::IndexMap,
    line_column::{Encoding, Position},
};
use qsc_eval::{
    backend::Tracer,
    debug::Frame,
    val::{self, Value},
};
use qsc_fir::fir::{
    self, ExprId, ExprKind, PackageId, PackageLookup, PackageStoreLookup, StoreItemId,
};
use qsc_frontend::compile::{self};
use qsc_lowerer::map_fir_package_to_hir;
use rustc_hash::{FxHashMap, FxHashSet};
#[cfg(test)]
use std::fmt::Display;
use std::{
    fmt::{Debug, Write},
    hash::Hash,
    mem::{replace, take},
    rc::Rc,
};

/// Circuit builder that implements the `Tracer` trait to build a circuit
/// while tracing execution.
pub struct CircuitTracer {
    config: TracerConfig,
    wire_map_builder: WireMapBuilder,
    circuit_builder: OperationListBuilder,
    next_result_id: usize,
    user_package_ids: Vec<PackageId>,
    superposition_qubits: FxHashSet<QubitWire>,
    classical_one_qubits: FxHashSet<QubitWire>,
}

impl Tracer for CircuitTracer {
    fn qubit_allocate(&mut self, stack: &[Frame], q: usize) {
        let declared_at = self.user_code_call_location(stack);
        self.wire_map_builder.map_qubit(q, declared_at);
    }

    fn qubit_release(&mut self, _stack: &[Frame], q: usize) {
        self.wire_map_builder.unmap_qubit(q);
    }

    fn qubit_swap_id(&mut self, _stack: &[Frame], q0: usize, q1: usize) {
        self.wire_map_builder.swap(q0, q1);
    }

    fn gate(
        &mut self,
        stack: &[Frame],
        name: &str,
        is_adjoint: bool,
        targets: &[usize],
        controls: &[usize],
        theta: Option<f64>,
    ) {
        let called_at = LogicalStack::from_evaluator_trace(stack);
        let display_args: Vec<String> = theta.map(|p| format!("{p:.4}")).into_iter().collect();
        let controls = if self.config.prune_classical_qubits {
            // Any controls that are known to be classically one can be removed, so this
            // will return the updated controls list.
            &self.update_qubit_status(name, targets, controls)
        } else {
            controls
        };
        self.circuit_builder.gate(
            self.wire_map_builder.current(),
            name,
            is_adjoint,
            &GateInputs { targets, controls },
            display_args,
            called_at,
        );
    }

    fn measure(&mut self, stack: &[Frame], name: &str, q: usize, val: &val::Result) {
        let called_at = LogicalStack::from_evaluator_trace(stack);
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => {
                let id = self.next_result_id;
                self.next_result_id += 1;
                id
            }
        };
        self.wire_map_builder.link_result_to_qubit(q, r);
        if name == "MResetZ" {
            self.classical_one_qubits
                .remove(&self.wire_map_builder.wire_map.qubit_wire(q));
            self.circuit_builder.measurement(
                self.wire_map_builder.current(),
                "MResetZ",
                q,
                r,
                called_at.clone(),
            );
        } else {
            self.circuit_builder
                .measurement(self.wire_map_builder.current(), "M", q, r, called_at);
        }
    }

    fn reset(&mut self, stack: &[Frame], q: usize) {
        let called_at = LogicalStack::from_evaluator_trace(stack);
        self.classical_one_qubits
            .remove(&self.wire_map_builder.wire_map.qubit_wire(q));
        self.circuit_builder
            .reset(self.wire_map_builder.current(), q, called_at);
    }

    fn custom_intrinsic(&mut self, stack: &[Frame], name: &str, arg: Value) {
        // The qubit arguments are treated as the targets for custom gates.
        // Any remaining arguments will be kept in the display_args field
        // to be shown as part of the gate label when the circuit is rendered.
        let (qubit_args, classical_args) = self.split_qubit_args(arg);

        if qubit_args.is_empty() {
            // don't add a gate with no qubit targets
            return;
        }

        self.circuit_builder.gate(
            self.wire_map_builder.current(),
            name,
            false, // is_adjoint
            &GateInputs {
                targets: &qubit_args,
                controls: &[],
            },
            if classical_args.is_empty() {
                vec![]
            } else {
                vec![classical_args]
            },
            LogicalStack::from_evaluator_trace(stack),
        );
    }

    fn is_stack_tracing_enabled(&self) -> bool {
        self.config.source_locations || self.config.group_by_scope
    }
}

impl CircuitTracer {
    #[must_use]
    pub fn new(config: TracerConfig, user_package_ids: &[PackageId]) -> Self {
        CircuitTracer {
            config,
            wire_map_builder: WireMapBuilder::new(vec![]),
            circuit_builder: OperationListBuilder::new(
                config.max_operations,
                user_package_ids.to_vec(),
                config.group_by_scope,
                config.source_locations,
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
            superposition_qubits: FxHashSet::default(),
            classical_one_qubits: FxHashSet::default(),
        }
    }

    #[must_use]
    pub fn with_qubit_input_params(
        config: TracerConfig,
        user_package_ids: &[PackageId],
        operation_qubit_params: Option<(PackageId, Vec<QubitParam>)>,
    ) -> Self {
        // Pre-initialize the qubit declaration locations for the operation's
        // input parameters. These will get allocated during execution, but
        // the declaration locations inferred from the callstacks will be meaningless
        // since those will be in the generated entry expression.
        let params = if config.source_locations {
            operation_qubit_params
                .map(|(package_id, info)| {
                    let mut decls = vec![];
                    for param in &info {
                        for _ in 0..param.num_qubits() {
                            decls.push(PackageOffset {
                                package_id,
                                offset: param.source_offset,
                            });
                        }
                    }
                    decls
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        CircuitTracer {
            config,
            wire_map_builder: WireMapBuilder::new(params),
            circuit_builder: OperationListBuilder::new(
                config.max_operations,
                user_package_ids.to_vec(),
                config.group_by_scope,
                config.source_locations,
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
            superposition_qubits: FxHashSet::default(),
            classical_one_qubits: FxHashSet::default(),
        }
    }

    #[must_use]
    pub fn snapshot(&self, source_lookup: &impl SourceLookup) -> Circuit {
        self.finish_circuit(self.circuit_builder.operations(), source_lookup)
    }

    #[must_use]
    pub fn finish(mut self, source_lookup: &impl SourceLookup) -> Circuit {
        let ops = replace(
            &mut self.circuit_builder,
            OperationListBuilder::new(
                self.config.max_operations,
                self.user_package_ids.clone(),
                self.config.group_by_scope,
                self.config.source_locations,
            ),
        )
        .into_operations();

        self.finish_circuit(&ops, source_lookup)
    }

    fn finish_circuit(
        &self,
        operations: &[OperationOrGroup],
        source_lookup: &impl SourceLookup,
    ) -> Circuit {
        let mut operations = operations.to_vec();
        let mut qubits = self.wire_map_builder.wire_map.to_qubits(source_lookup);

        if self.config.prune_classical_qubits {
            // Remove qubits that are always classical.
            qubits.retain(|q| self.superposition_qubits.contains(&q.id.into()));

            // Remove operations that don't use any non-classical qubits.
            operations.retain_mut(|op| self.should_keep_operation_mut(op));
        }

        finish_circuit(
            source_lookup,
            operations,
            qubits,
            self.config.group_by_scope,
        )
    }

    fn should_keep_operation_mut(&self, op: &mut OperationOrGroup) -> bool {
        if matches!(op.kind, OperationOrGroupKind::Single) {
            // This is a normal gate operation, so only keep it if all the qubits are non-classical.
            op.all_qubits()
                .iter()
                .all(|q| self.superposition_qubits.contains(q))
        } else {
            // This is a grouped operation, so process the children recursively.
            let mut used_qubits = FxHashSet::default();
            op.children_mut()
                .expect("operation should be a group with children")
                .retain_mut(|child_op| {
                    // Prune out child ops that don't use any non-classical qubits.
                    // This has the side effect of updating each child op's target qubits.
                    if self.should_keep_operation_mut(child_op) {
                        for q in child_op.all_qubits() {
                            used_qubits.insert(q);
                        }
                        true
                    } else {
                        false
                    }
                });
            // Update the targets of this grouped operation to only include qubits actually used by child operations.
            op.op
                .targets_mut()
                .retain(|q| used_qubits.contains(&q.qubit.into()));
            // Only keep this grouped operation if any of its targets were kept.
            !op.op.targets_mut().is_empty()
        }
    }

    /// Splits the qubit arguments from classical arguments so that the qubits
    /// can be treated as the targets for custom gates.
    /// The classical arguments get formatted into a comma-separated list.
    fn split_qubit_args(&mut self, arg: Value) -> (Vec<usize>, String) {
        let arg = if let Value::Tuple(vals, _) = arg {
            vals
        } else {
            // Single arguments are not passed as tuples, wrap in an array
            Rc::new([arg])
        };
        let mut qubits = vec![];
        let mut classical_args = String::new();
        self.push_vals(&arg, &mut qubits, &mut classical_args);
        (qubits, classical_args)
    }

    /// Pushes all qubit values into `qubits`, and formats all classical values into `classical_args`.
    fn push_val(&self, arg: &Value, qubits: &mut Vec<usize>, classical_args: &mut String) {
        match arg {
            Value::Array(vals) => {
                self.push_list::<'[', ']'>(vals, qubits, classical_args);
            }
            Value::Tuple(vals, _) => {
                self.push_list::<'(', ')'>(vals, qubits, classical_args);
            }
            Value::Qubit(q) => {
                qubits.push(q.deref().0);
            }
            v => {
                let _ = write!(classical_args, "{v}");
            }
        }
        qubits.sort_unstable();
        qubits.dedup();
    }

    /// Pushes all qubit values into `qubits`, and formats all
    /// classical values into `classical_args` as a list.
    fn push_list<const OPEN: char, const CLOSE: char>(
        &self,
        vals: &[Value],
        qubits: &mut Vec<usize>,
        classical_args: &mut String,
    ) {
        classical_args.push(OPEN);
        let start = classical_args.len();
        self.push_vals(vals, qubits, classical_args);
        if classical_args.len() > start {
            classical_args.push(CLOSE);
        } else {
            classical_args.pop();
        }
    }

    /// Pushes all qubit values into `qubits`, and formats all
    /// classical values into `classical_args` as comma-separated values.
    fn push_vals(&self, vals: &[Value], qubits: &mut Vec<usize>, classical_args: &mut String) {
        let mut any = false;
        for v in vals {
            let start = classical_args.len();
            self.push_val(v, qubits, classical_args);
            if classical_args.len() > start {
                any = true;
                classical_args.push_str(", ");
            }
        }
        if any {
            // remove trailing comma
            classical_args.pop();
            classical_args.pop();
        }
    }

    fn user_code_call_location(&self, stack: &[Frame]) -> Option<PackageOffset> {
        if self.config.source_locations {
            let logical_stack = LogicalStack::from_evaluator_trace(stack);
            retain_user_frames(&self.user_package_ids, logical_stack)
                .0
                .last()
                .map(|l| {
                    let LogicalStackEntryLocation::Source(location) = *l.location() else {
                        panic!("last frame in stack trace should be a call to an intrinsic")
                    };
                    location
                })
        } else {
            None
        }
    }

    fn mark_qubit_in_superposition(&mut self, wire: QubitWire) {
        assert!(
            self.config.prune_classical_qubits,
            "should only be called when pruning is enabled"
        );
        self.superposition_qubits.insert(wire);
        self.classical_one_qubits.remove(&wire);
    }

    fn flip_classical_qubit(&mut self, wire: QubitWire) {
        assert!(
            self.config.prune_classical_qubits,
            "should only be called when pruning is enabled"
        );
        if self.classical_one_qubits.contains(&wire) {
            self.classical_one_qubits.remove(&wire);
        } else {
            self.classical_one_qubits.insert(wire);
        }
    }

    fn update_qubit_status(
        &mut self,
        name: &str,
        targets: &[usize],
        controls: &[usize],
    ) -> Vec<usize> {
        match name {
            "H" | "Rx" | "Ry" | "SX" | "Rxx" | "Ryy" => {
                // These gates create superpositions, so mark the qubits as non-trimmable
                for &q in targets {
                    let mapped_q = self.wire_map_builder.wire_map.qubit_wire(q);
                    self.mark_qubit_in_superposition(mapped_q);
                }
            }
            "X" | "Y" => {
                let mapped_target = self.wire_map_builder.wire_map.qubit_wire(targets[0]);
                let controls: Vec<usize> = controls
                    .iter()
                    .filter(|c| !self.classical_one_qubits.contains(&(**c).into()))
                    .copied()
                    .collect();
                if !self.superposition_qubits.contains(&mapped_target) {
                    // The target is not yet marked as non-trimmable, so check the controls.
                    let superposition_controls_count = controls
                        .iter()
                        .filter(|c| self.superposition_qubits.contains(&(**c).into()))
                        .count();

                    if controls.is_empty() {
                        // If all controls are classical 1 or there are no controls, the target is flipped
                        self.flip_classical_qubit(mapped_target);
                    } else if superposition_controls_count == controls.len() {
                        // If all controls are in superposition, the target is also in superposition
                        self.mark_qubit_in_superposition(mapped_target);
                    }
                }
                return controls;
            }
            "Z" => {
                // Only clean up the classical 1 qubits from the controls list. No need to update the target,
                // since Z does not introduce superpositions.
                return controls
                    .iter()
                    .filter(|c| !self.classical_one_qubits.contains(&(**c).into()))
                    .copied()
                    .collect();
            }
            "SWAP" => {
                // If either qubit is non-trimmable, both become non-trimmable
                let q0_mapped = self.wire_map_builder.wire_map.qubit_wire(targets[0]);
                let q1_mapped = self.wire_map_builder.wire_map.qubit_wire(targets[1]);
                if self.superposition_qubits.contains(&q0_mapped)
                    || self.superposition_qubits.contains(&q1_mapped)
                {
                    self.mark_qubit_in_superposition(q0_mapped);
                    self.mark_qubit_in_superposition(q1_mapped);
                } else {
                    match (
                        self.classical_one_qubits.contains(&q0_mapped),
                        self.classical_one_qubits.contains(&q1_mapped),
                    ) {
                        (true, false) | (false, true) => {
                            self.flip_classical_qubit(q0_mapped);
                            self.flip_classical_qubit(q1_mapped);
                        }
                        _ => {
                            // Nothing to do if both are classical 0 or both are in superposition
                        }
                    }
                }
            }
            "S" | "T" | "Rz" | "Rzz" => {
                // These gates don't create superpositions on their own, so do nothing
            }
            _ => {
                // For any other gate, conservatively mark all target qubits as non-trimmable
                for &q in targets.iter().chain(controls.iter()) {
                    let mapped_q = self.wire_map_builder.wire_map.qubit_wire(q);
                    self.mark_qubit_in_superposition(mapped_q);
                }
            }
        }
        // Return the normal controls list if no changes were made.
        controls.to_vec()
    }
}

pub(crate) fn finish_circuit(
    source_lookup: &impl SourceLookup,
    mut operations: Vec<OperationOrGroup>,
    qubits: Vec<Qubit>,
    collapse_trivial_groups: bool,
) -> Circuit {
    if collapse_trivial_groups {
        collapse_unnecessary_scopes(&mut operations, source_lookup);
    }
    let mut loop_id_cache = Default::default();
    let operations = operations
        .into_iter()
        .map(|o| o.into_operation(source_lookup, &mut loop_id_cache))
        .collect();

    let component_grid = operation_list_to_grid(operations, &qubits);
    Circuit {
        qubits,
        component_grid,
    }
}

/// Removes any scopes that are unnecessary and replaces them with their children operations.
/// An unnecessary loop scope is one that either has a single child iteration,
/// or has multiple iterations that each operate on distinct sets of qubits (i.e. a "vertical" loop).
/// An unnecessary lambda scope is one where the lambda has a single child operation.
fn collapse_unnecessary_scopes(
    operations: &mut Vec<OperationOrGroup>,
    source_lookup: &impl SourceLookup,
) {
    let mut ops = vec![];
    for mut op in operations.drain(..) {
        match &mut op.kind {
            OperationOrGroupKind::Single => {}
            OperationOrGroupKind::Group { children, .. } => {
                collapse_unnecessary_scopes(children, source_lookup);
            }
        }

        if let Some(children) = collapse_if_unnecessary(&mut op, source_lookup) {
            ops.extend(children);
        } else {
            ops.push(op);
        }
    }
    *operations = ops;
}

/// If the given operation or group is an outer scope that can be collapsed,
/// returns its children operations or groups.
fn collapse_if_unnecessary(
    op: &mut OperationOrGroup,
    source_lookup: &impl SourceLookup,
) -> Option<Vec<OperationOrGroup>> {
    if let OperationOrGroupKind::Group {
        scope_stack,
        children,
    } = &mut op.kind
    {
        if let Scope::Loop(..) = scope_stack.current_lexical_scope() {
            if children.len() == 1 {
                // remove the loop scope
                let mut only_child = children.remove(0);
                let OperationOrGroupKind::Group { children, .. } = &mut only_child.kind else {
                    panic!("only child of an outer loop scope should be a group");
                };
                return Some(take(children));
            }

            // now, if each c applies to a distinct set of qubits, this loop is entirely vertical and can be collapsed as well
            let mut distinct_sets_of_qubits = FxHashSet::default();
            for child_op in children.iter() {
                let qs = child_op.all_qubits();
                if !distinct_sets_of_qubits.insert(qs) {
                    // There's overlap, so we won't collapse
                    return None;
                }
            }
            let mut all_children = vec![];
            for mut child_op in children.drain(..) {
                let OperationOrGroupKind::Group { children, .. } = &mut child_op.kind else {
                    panic!("only child of an outer loop scope should be a group");
                };
                all_children.extend(take(children));
            }
            return Some(all_children);
        } else if let Scope::Callable(..) = scope_stack.current_lexical_scope()
            && children.len() == 1
            && source_lookup
                .resolve_scope(scope_stack.current_lexical_scope(), &mut Default::default())
                .name
                .as_ref()
                == "<lambda>"
        {
            // remove the lambda scope
            return Some(take(children));
        }
    }
    None
}

// loop_expr_id_cache: FxHashMap<PackageOffset, (PackageId, ExprId)>,
pub(crate) type LoopIdCache = FxHashMap<PackageOffset, (PackageId, ExprId)>;

/// Resolves structs that use compilation-specific IDs (`PackageId`s, `ExprId`s etc.)
/// to user legible names and source file locations.
pub trait SourceLookup {
    fn resolve_package_offset(&self, package_offset: &PackageOffset) -> SourceLocation;
    fn resolve_scope(&self, scope: &Scope, loop_id_cache: &mut LoopIdCache) -> LexicalScope;
    fn resolve_logical_stack_entry_location(
        &self,
        location: LogicalStackEntryLocation,
        loop_id_cache: &mut LoopIdCache,
    ) -> Option<PackageOffset>;
}

impl SourceLookup for (&compile::PackageStore, &fir::PackageStore) {
    fn resolve_package_offset(&self, package_offset: &PackageOffset) -> SourceLocation {
        let package = self
            .0
            .get(map_fir_package_to_hir(package_offset.package_id))
            .expect("package id must exist in store");

        let source = package
            .sources
            .find_by_offset(package_offset.offset)
            .expect("source should exist for offset");

        let pos = Position::from_utf8_byte_offset(
            Encoding::Utf8,
            &source.contents,
            package_offset.offset - source.offset,
        );

        SourceLocation {
            file: source.name.to_string(),
            line: pos.line,
            column: pos.column,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_scope(&self, scope_id: &Scope, loop_id_cache: &mut LoopIdCache) -> LexicalScope {
        match scope_id {
            Scope::Callable(CallableId::Id(store_item_id, functor_app)) => {
                let item = self.1.get_item(*store_item_id);

                let fir::ItemKind::Callable(callable_decl) = &item.kind else {
                    panic!("only callables should be in the stack")
                };

                let scope_offset = match &callable_decl.implementation {
                    fir::CallableImpl::Intrinsic => callable_decl.span.lo,
                    fir::CallableImpl::Spec(spec_impl) => {
                        if functor_app.adjoint && functor_app.controlled > 0 {
                            spec_impl.ctl_adj.as_ref().unwrap_or(&spec_impl.body)
                        } else if functor_app.adjoint {
                            spec_impl.adj.as_ref().unwrap_or(&spec_impl.body)
                        } else if functor_app.controlled > 0 {
                            spec_impl.ctl.as_ref().unwrap_or(&spec_impl.body)
                        } else {
                            &spec_impl.body
                        }
                        .span
                        .lo
                    }
                    fir::CallableImpl::SimulatableIntrinsic(spec_decl) => spec_decl.span.lo,
                };

                let scope_name = callable_decl.name.name.clone();

                LexicalScope {
                    location: Some(PackageOffset {
                        package_id: store_item_id.package,
                        offset: scope_offset,
                    }),
                    name: scope_name,
                    is_adjoint: functor_app.adjoint,
                    is_conditional: false,
                }
            }
            Scope::Callable(CallableId::Source(package_offset, name)) => {
                // trim the trailing dagger symbol and set `is_adjoint` accordingly
                let (name, is_adjoint) = if let Some(pos) = name.rfind('\'') {
                    if pos == name.len() - 1 {
                        (name[..pos].to_string().into(), true)
                    } else {
                        (name.clone(), false)
                    }
                } else {
                    (name.clone(), false)
                };
                LexicalScope {
                    location: Some(*package_offset),
                    name,
                    is_adjoint,
                    is_conditional: false,
                }
            }
            Scope::Loop(LoopId::Id(package_id, expr_id)) => {
                let package_store = self.1;
                let package = package_store.get(*package_id);
                let loop_expr = package.get_expr(*expr_id);
                let ExprKind::While(cond_expr_id, _) = &loop_expr.kind else {
                    panic!("only while loops should be in loop containers");
                };
                let cond_expr = package.get_expr(*cond_expr_id);
                let expr_contents = self
                    .0
                    .get(map_fir_package_to_hir(*package_id))
                    .and_then(|p| p.sources.find_by_offset(cond_expr.span.lo))
                    .map(|s| {
                        s.contents[(cond_expr.span.lo - s.offset) as usize
                            ..(cond_expr.span.hi - s.offset) as usize]
                            .to_string()
                    });

                LexicalScope {
                    name: format!("loop: {}", expr_contents.unwrap_or_default()).into(),
                    location: Some(PackageOffset {
                        package_id: *package_id,
                        offset: loop_expr.span.lo,
                    }),
                    is_adjoint: false,
                    is_conditional: false,
                }
            }

            Scope::Loop(LoopId::Source(package_offset)) => {
                let found_loop_expr = if let Some(cached) = loop_id_cache.get(package_offset) {
                    Some(*cached)
                } else {
                    let val = self.1.get(package_offset.package_id).exprs.iter().find_map(
                        |(expr_id, expr)| {
                            if expr.span.lo == package_offset.offset
                                && matches!(expr.kind, ExprKind::While(_, _))
                            {
                                Some((package_offset.package_id, expr_id))
                            } else {
                                None
                            }
                        },
                    );
                    if let Some(val) = val {
                        // cache the result
                        loop_id_cache.insert(*package_offset, val);
                    }
                    val
                };

                let expr_contents = if let Some((package_id, expr_id)) = found_loop_expr {
                    let package_store = self.1;
                    let package = package_store.get(package_id);
                    let loop_expr = package.get_expr(expr_id);
                    let ExprKind::While(cond_expr_id, _) = &loop_expr.kind else {
                        panic!("only while loops should be in loop containers");
                    };
                    let cond_expr = package.get_expr(*cond_expr_id);
                    self.0
                        .get(map_fir_package_to_hir(package_id))
                        .and_then(|p| p.sources.find_by_offset(cond_expr.span.lo))
                        .map(|s| {
                            s.contents[(cond_expr.span.lo - s.offset) as usize
                                ..(cond_expr.span.hi - s.offset) as usize]
                                .to_string()
                        })
                } else {
                    None
                };

                LexicalScope {
                    name: format!("loop: {}", expr_contents.unwrap_or_default()).into(),
                    location: Some(*package_offset),
                    is_adjoint: false,
                    is_conditional: false,
                }
            }
            Scope::LoopIteration(LoopId::Id(package_id, expr_id), i) => {
                let package_store = self.1;
                let package = package_store.get(*package_id);
                let loop_expr = package.get_expr(*expr_id);
                let ExprKind::While(_cond, body) = &loop_expr.kind else {
                    panic!(
                        "only while loops should be used for loop body locations, {expr_id} is: {:?}",
                        loop_expr.kind
                    );
                };
                let block = package.get_block(*body);

                LexicalScope {
                    name: format!("({i})").into(),
                    location: Some(PackageOffset {
                        package_id: *package_id,
                        offset: block.span.lo,
                    }),
                    is_adjoint: false,
                    is_conditional: false,
                }
            }
            Scope::LoopIteration(LoopId::Source(package_offset), i) => LexicalScope {
                name: format!("({i})").into(),
                location: Some(*package_offset),
                is_adjoint: false,
                is_conditional: false,
            },
            Scope::Top => LexicalScope {
                name: "top".into(),
                location: None,
                is_adjoint: false,
                is_conditional: false,
            },
            Scope::ClassicallyControlled {
                label,
                control_result_ids: _,
            } => LexicalScope {
                location: None,
                name: label.clone().into(),
                is_adjoint: false,
                is_conditional: true,
            },
        }
    }

    fn resolve_logical_stack_entry_location(
        &self,
        location: LogicalStackEntryLocation,
        loop_id_cache: &mut LoopIdCache,
    ) -> Option<PackageOffset> {
        match location {
            LogicalStackEntryLocation::Unknown => None,
            LogicalStackEntryLocation::Branch(package_offset, _) => package_offset,
            LogicalStackEntryLocation::Source(package_offset)
            | LogicalStackEntryLocation::Loop(LoopId::Source(package_offset)) => {
                Some(package_offset)
            }
            LogicalStackEntryLocation::Loop(LoopId::Id(package_id, loop_expr_id)) => {
                let fir_package_store = self.1;
                let package = fir_package_store.get(package_id);
                let expr = package.get_expr(loop_expr_id);

                Some(PackageOffset {
                    package_id,
                    offset: expr.span.lo,
                })
            }
            LogicalStackEntryLocation::LoopIteration(LoopId::Id(package_id, expr_id), _) => {
                let fir_package_store = self.1;
                let package = fir_package_store.get(package_id);
                let expr = package.get_expr(expr_id);
                let ExprKind::While(_cond, body) = &expr.kind else {
                    panic!("only while loops should be used for loop body locations");
                };

                let block = package.get_block(*body);

                Some(PackageOffset {
                    package_id,
                    offset: block.span.lo,
                })
            }
            LogicalStackEntryLocation::LoopIteration(LoopId::Source(package_offset), _) => {
                let found_loop_expr = if let Some(cached) = loop_id_cache.get(&package_offset) {
                    Some(*cached)
                } else {
                    let val = self.1.get(package_offset.package_id).exprs.iter().find_map(
                        |(expr_id, expr)| {
                            if expr.span.lo == package_offset.offset
                                && matches!(expr.kind, ExprKind::While(_, _))
                            {
                                Some((package_offset.package_id, expr_id))
                            } else {
                                None
                            }
                        },
                    );
                    if let Some(val) = val {
                        // cache the result
                        loop_id_cache.insert(package_offset, val);
                    }
                    val
                };

                if let Some((package_id, expr_id)) = found_loop_expr {
                    let fir_package_store = self.1;
                    let package = fir_package_store.get(package_id);
                    let expr = package.get_expr(expr_id);
                    let ExprKind::While(_cond, body) = &expr.kind else {
                        panic!("only while loops should be used for loop body locations");
                    };

                    let block = package.get_block(*body);

                    Some(PackageOffset {
                        package_id,
                        offset: block.span.lo,
                    })
                } else {
                    // Fall back to loop expr location
                    Some(package_offset)
                }
            }
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Copy)]
pub struct TracerConfig {
    /// Maximum number of operations the builder will add to the circuit
    pub max_operations: usize,
    /// Capture the source code locations of operations and qubit declarations
    /// in the circuit diagram
    pub source_locations: bool,
    /// Group operations according to call graph in the circuit diagram
    pub group_by_scope: bool,
    /// Prune purely classical or unused qubits
    pub prune_classical_qubits: bool,
}

impl TracerConfig {
    /// Set to the current UI limit + 1 so that it still triggers
    /// the "this circuit has too many gates" warning in the UI.
    /// (see npm\qsharp\ux\circuit.tsx)
    ///
    /// A more refined way to do this might be to communicate the
    /// "limit exceeded" state up to the UI somehow.
    pub const DEFAULT_MAX_OPERATIONS: usize = 10001;
}

/// Maps qubit IDs to their corresponding wire IDs and tracks measurement results
/// along with their source locations.
#[derive(Default)]
pub(crate) struct WireMap {
    /// Maps qubit IDs to their assigned wire IDs.
    qubits: IndexMap<usize, QubitWire>,
    /// Maps wire IDs to their declaration locations and measurement result IDs.
    qubit_wires: IndexMap<QubitWire, (Vec<PackageOffset>, Vec<usize>)>,
}

impl WireMap {
    pub fn qubit_wire(&self, qubit_id: usize) -> QubitWire {
        self.qubits
            .get(qubit_id)
            .unwrap_or_else(|| panic!("qubit {qubit_id} should already be mapped"))
            .to_owned()
    }

    pub fn result_wire(&self, result_id: usize) -> ResultWire {
        self.qubit_wires
            .iter()
            .find_map(|(QubitWire(qubit_wire), (_, results))| {
                let r_idx = results.iter().position(|&r| r == result_id);
                r_idx.map(|r_idx| ResultWire(qubit_wire, r_idx))
            })
            .expect("result should already be mapped")
    }

    pub(crate) fn to_qubits(&self, source_lookup: &impl SourceLookup) -> Vec<Qubit> {
        let mut qubits = vec![];
        for (QubitWire(wire_id), (declarations, results)) in self.qubit_wires.iter() {
            qubits.push(Qubit {
                id: wire_id,
                num_results: results.len(),
                declarations: declarations
                    .iter()
                    .map(|offset| source_lookup.resolve_package_offset(offset))
                    .collect(),
            });
        }

        qubits
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ResultWire(pub(crate) usize, pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct QubitWire(pub(crate) usize);

impl From<usize> for QubitWire {
    fn from(value: usize) -> Self {
        QubitWire(value)
    }
}

impl From<QubitWire> for usize {
    fn from(value: QubitWire) -> Self {
        value.0
    }
}

/// Manages the mapping between qubits and wires during circuit construction.
/// Tracks qubit allocations, measurement results, and their source locations.
/// Also acts as a result ID allocator when the result IDs aren't passed in
/// by the tracer.
///
/// This implementation is similar to the partial evaluation resource manager,
/// which is used in RIR/QIR generation, in its Qubit ID and Result ID management.
/// (see `source/compiler/qsc_partial_eval/src/management.rs`)
pub(crate) struct WireMapBuilder {
    next_qubit_wire_id: QubitWire,
    wire_map: WireMap,
}

impl Default for WireMapBuilder {
    fn default() -> Self {
        Self {
            next_qubit_wire_id: QubitWire(0),
            wire_map: WireMap::default(),
        }
    }
}

impl WireMapBuilder {
    fn new(qubit_input_decls: Vec<PackageOffset>) -> Self {
        let mut new = Self {
            next_qubit_wire_id: QubitWire(0),
            wire_map: WireMap::default(),
        };

        let mut i = new.next_qubit_wire_id;
        for decl in qubit_input_decls {
            new.wire_map.qubit_wires.insert(i, (vec![decl], vec![]));
            i.0 += 1;
        }

        new
    }

    pub fn current(&self) -> &WireMap {
        &self.wire_map
    }

    pub fn map_qubit(&mut self, qubit: usize, declared_at: Option<PackageOffset>) {
        let mapped = self.next_qubit_wire_id;
        self.next_qubit_wire_id.0 += 1;
        self.wire_map.qubits.insert(qubit, mapped);

        if let Some(q) = self.wire_map.qubit_wires.get_mut(mapped) {
            if let Some(location) = declared_at {
                q.0.push(location);
            }
        } else {
            let l = declared_at.map(|l| vec![l]).unwrap_or_default();
            self.wire_map.qubit_wires.insert(mapped, (l, vec![]));
        }
    }

    pub(crate) fn into_wire_map(self) -> WireMap {
        self.wire_map
    }

    fn unmap_qubit(&mut self, q: usize) {
        // Simple behavior assuming qubits are always released in reverse order of allocation
        self.next_qubit_wire_id.0 -= 1;
        self.wire_map.qubits.remove(q);
    }

    pub fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        let mapped_q = self.wire_map.qubit_wire(q);
        let Some((_, measurements)) = self.wire_map.qubit_wires.get_mut(mapped_q) else {
            panic!("qubit should already be mapped");
        };
        if !measurements.contains(&r) {
            measurements.push(r);
        }
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        let q0_mapped = self.wire_map.qubit_wire(q0);
        let q1_mapped = self.wire_map.qubit_wire(q1);
        self.wire_map.qubits.insert(q0, q1_mapped);
        self.wire_map.qubits.insert(q1, q0_mapped);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OperationOrGroup {
    kind: OperationOrGroupKind,
    location: Option<LogicalStackEntryLocation>,
    op: Operation,
}

#[derive(Clone, Debug)]
enum OperationOrGroupKind {
    Single,
    Group {
        scope_stack: ScopeStack,
        children: Vec<OperationOrGroup>,
    },
    // ConditionalGroup {
    //     label: String,
    //     children: Vec<OperationOrGroup>,
    //     scope_location: Option<PackageOffset>,
    // },
}

impl OperationOrGroup {
    pub(crate) fn new_single(op: Operation) -> Self {
        Self {
            kind: OperationOrGroupKind::Single,
            op,
            location: None,
        }
    }

    pub(crate) fn new_unitary(
        name: &str,
        is_adjoint: bool,
        targets: &[QubitWire],
        controls: &[QubitWire],
        args: Vec<String>,
    ) -> Self {
        Self::new_single(Operation::Unitary(Unitary {
            gate: name.to_string(),
            args,
            children: vec![],
            targets: targets
                .iter()
                .map(|q| Register {
                    qubit: q.0,
                    result: None,
                })
                .collect(),
            controls: controls
                .iter()
                .map(|q| Register {
                    qubit: q.0,
                    result: None,
                })
                .collect(),
            is_adjoint,
            is_conditional: false,
            metadata: None,
        }))
    }

    pub(crate) fn new_measurement(label: &str, qubit: QubitWire, result: ResultWire) -> Self {
        Self::new_single(Operation::Measurement(Measurement {
            gate: label.to_string(),
            args: vec![],
            children: vec![],
            qubits: vec![Register {
                qubit: qubit.0,
                result: None,
            }],
            results: vec![Register {
                qubit: result.0,
                result: Some(result.1),
            }],
            metadata: None,
        }))
    }

    pub(crate) fn new_reset(qubit: QubitWire) -> Self {
        Self::new_single(Operation::Ket(Ket {
            gate: "0".to_string(),
            args: vec![],
            children: vec![],
            targets: vec![Register {
                qubit: qubit.0,
                result: None,
            }],
            metadata: None,
        }))
    }

    pub(crate) fn all_qubits(&self) -> Vec<QubitWire> {
        let qubits: FxHashSet<QubitWire> = match &self.op {
            Operation::Measurement(measurement) => measurement.qubits.clone(),
            Operation::Unitary(unitary) => unitary
                .targets
                .iter()
                .chain(unitary.controls.iter())
                .filter(|r| r.result.is_none())
                .cloned()
                .collect(),
            Operation::Ket(ket) => ket.targets.clone(),
        }
        .into_iter()
        .map(|r| QubitWire(r.qubit))
        .collect();
        qubits.into_iter().collect()
    }

    pub(crate) fn target_results(&self) -> Vec<ResultWire> {
        let results: FxHashSet<ResultWire> = match &self.op {
            Operation::Measurement(measurement) => measurement
                .results
                .iter()
                .filter_map(|r| r.result.map(|res| ResultWire(r.qubit, res)))
                .collect(),
            Operation::Unitary(unitary) => unitary
                .targets
                .iter()
                .filter_map(|r| r.result.map(|res| ResultWire(r.qubit, res)))
                .collect(),
            Operation::Ket(_) => vec![],
        }
        .into_iter()
        .collect();
        results.into_iter().collect()
    }

    pub(crate) fn control_results(&self) -> Vec<ResultWire> {
        let results: FxHashSet<ResultWire> = match &self.op {
            Operation::Unitary(unitary) => unitary
                .controls
                .iter()
                .filter_map(|r| r.result.map(|res| ResultWire(r.qubit, res)))
                .collect(),
            Operation::Measurement(_) | Operation::Ket(_) => vec![],
        }
        .into_iter()
        .collect();
        results.into_iter().collect()
    }

    fn children(&self) -> Option<&Vec<Self>>
    where
        Self: std::marker::Sized,
    {
        if let OperationOrGroupKind::Group { children, .. } = &self.kind {
            Some(children)
        } else {
            None
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Self>>
    where
        Self: std::marker::Sized,
    {
        if let OperationOrGroupKind::Group { children, .. } = &mut self.kind {
            Some(children)
        } else {
            None
        }
    }

    pub(crate) fn new_group(scope_stack: ScopeStack, wire_map: &WireMap) -> Self {
        let mut control_result_ids_map = vec![];
        let mut control_result_registers = vec![];
        let mut metadata = None;

        if let Scope::ClassicallyControlled {
            control_result_ids, ..
        } = scope_stack.current_lexical_scope()
        {
            for result_id in control_result_ids {
                let result_wire = wire_map.result_wire(*result_id);
                let register = Register {
                    qubit: result_wire.0,
                    result: Some(result_wire.1),
                };
                control_result_ids_map.push((register.clone(), *result_id));
                control_result_registers.push(register);
            }

            metadata = Some(Metadata {
                control_result_ids: control_result_ids_map,
                ..Default::default()
            });
        }

        Self {
            kind: OperationOrGroupKind::Group {
                scope_stack,
                children: vec![],
            },
            op: Operation::Unitary(Unitary {
                // Most fields here are to be filled in later, in `into_operation`.
                gate: String::new(),
                args: vec![],
                children: vec![],
                targets: control_result_registers.clone(),
                controls: control_result_registers,
                is_adjoint: false,
                metadata,
                is_conditional: false,
            }),
            location: None,
        }
    }

    fn extend_target_qubits(&mut self, target_qubits: &[QubitWire]) {
        match &mut self.op {
            Operation::Measurement(_) => {}
            Operation::Unitary(unitary) => {
                unitary
                    .targets
                    .extend(target_qubits.iter().map(|q| Register {
                        qubit: q.0,
                        result: None,
                    }));
                unitary
                    .targets
                    .sort_unstable_by_key(|r| (r.qubit, r.result));
                unitary.targets.dedup();
            }
            Operation::Ket(ket) => {
                ket.targets.extend(target_qubits.iter().map(|q| Register {
                    qubit: q.0,
                    result: None,
                }));
            }
        }
    }

    fn extend_target_results(&mut self, target_results: &[ResultWire]) {
        {
            match &mut self.op {
                Operation::Measurement(measurement) => {
                    measurement
                        .results
                        .extend(target_results.iter().map(|r| Register {
                            qubit: r.0,
                            result: Some(r.1),
                        }));
                    measurement
                        .results
                        .sort_unstable_by_key(|reg| (reg.qubit, reg.result));
                    measurement.results.dedup();
                }
                Operation::Unitary(unitary) => {
                    unitary
                        .targets
                        .extend(target_results.iter().map(|r| Register {
                            qubit: r.0,
                            result: Some(r.1),
                        }));
                    unitary
                        .targets
                        .sort_unstable_by_key(|r| (r.qubit, r.result));
                    unitary.targets.dedup();
                }
                Operation::Ket(_) => {}
            }
        }
    }

    // fn extend_control_results(&mut self, control_results: &[ResultWire]) {
    //     match &mut self.op {
    //         Operation::Measurement(_) => {
    //             panic!("control results for measurement operations not supported");
    //         }
    //         Operation::Unitary(unitary) => {
    //             unitary
    //                 .controls
    //                 .extend(control_results.iter().map(|r| Register {
    //                     qubit: r.0,
    //                     result: Some(r.1),
    //                 }));
    //             unitary
    //                 .controls
    //                 .sort_unstable_by_key(|r| (r.qubit, r.result));
    //             unitary.controls.dedup();
    //         }
    //         Operation::Ket(_) => {
    //             panic!("control results for ket operations not supported");
    //         }
    //     }
    // }

    pub(crate) fn scope_stack_if_group(&self) -> Option<&ScopeStack> {
        if let OperationOrGroupKind::Group { scope_stack, .. } = &self.kind {
            Some(scope_stack)
        } else {
            None
        }
    }

    pub(crate) fn into_operation(
        mut self,
        source_lookup: &impl SourceLookup,
        loop_id_cache: &mut LoopIdCache,
    ) -> Operation {
        if let Some(location) = self.location {
            let package_offset =
                source_lookup.resolve_logical_stack_entry_location(location, loop_id_cache);

            if let Some(package_offset) = package_offset {
                let location = source_lookup.resolve_package_offset(&package_offset);
                self.op.source_location_mut().replace(location);
            }
        }

        match self.kind {
            OperationOrGroupKind::Single => self.op,
            OperationOrGroupKind::Group {
                scope_stack,
                children,
            } => {
                let Operation::Unitary(u) = &mut self.op else {
                    panic!("group operation should be a unitary")
                };
                let scope = scope_stack.resolve_scope(source_lookup, loop_id_cache);
                u.gate = scope.name.to_string();
                u.is_adjoint = scope.is_adjoint;
                let scope_location = scope
                    .location
                    .map(|loc| source_lookup.resolve_package_offset(&loc));

                u.is_conditional = scope.is_conditional;

                if u.metadata.is_none() {
                    u.metadata = Some(Metadata::default());
                }

                if let Some(md) = &mut u.metadata {
                    md.scope_location = scope_location;
                } else {
                    unreachable!("metadata should have been set");
                }

                u.children = vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(|o| o.into_operation(source_lookup, loop_id_cache))
                        .collect(),
                }];
                self.op
            }
        }
    }

    fn merge_inputs(&mut self, op: &OperationOrGroup) {
        self.extend_target_qubits(&op.all_qubits());
        self.extend_target_results(&op.target_results());
        self.extend_target_results(&op.control_results());
    }
}

/// Builds a list of circuit operations with a maximum operation limit.
/// Stops adding operations once the limit is exceeded.
///
/// Methods take `WireMap` as a parameter to resolve qubit and result IDs
/// to their corresponding wire positions in the circuit diagram.
pub(crate) struct OperationListBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations_container: OperationOrGroup, // TODO: weird
    user_package_ids: Vec<PackageId>,
    grouping_config: GroupingConfig,
}

#[derive(Clone, Copy)]
pub(crate) struct GroupingConfig {
    pub(crate) source_locations: bool,
    pub(crate) group_by_scope: bool,
}

impl OperationListBuilder {
    pub fn new(
        max_operations: usize,
        user_package_ids: Vec<PackageId>,
        group_by_scope: bool,
        source_locations: bool,
    ) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations_container: OperationOrGroup::new_group(
                ScopeStack::top(),
                &WireMap::default(),
            ),
            grouping_config: GroupingConfig {
                source_locations,
                group_by_scope,
            },
            user_package_ids,
        }
    }

    fn push_op(
        &mut self,
        op: OperationOrGroup,
        unfiltered_call_stack: LogicalStack,
        wire_map: &WireMap,
    ) {
        if self.max_ops_exceeded
            || self
                .operations_container
                .children()
                .expect("container should be a group")
                .len()
                >= self.max_ops
        {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        let op_call_stack =
            if self.grouping_config.group_by_scope || self.grouping_config.source_locations {
                retain_user_frames(&self.user_package_ids, unfiltered_call_stack)
            } else {
                LogicalStack::default()
            };

        add_scoped_op(
            &mut self.operations_container,
            &ScopeStack::top(),
            op,
            &op_call_stack,
            self.grouping_config.group_by_scope,
            self.grouping_config.source_locations,
            wire_map,
        );
    }

    fn operations(&self) -> &Vec<OperationOrGroup> {
        self.operations_container
            .children()
            .expect("container should be a group")
    }

    pub(crate) fn into_operations(self) -> Vec<OperationOrGroup> {
        let OperationOrGroupKind::Group { children, .. } = self.operations_container.kind else {
            panic!("container should be a group");
        };
        children
    }
}

pub(crate) struct GateInputs<'a> {
    pub(crate) targets: &'a [usize],
    pub(crate) controls: &'a [usize],
}

/// Trait representing a receiver of circuit operations that can accept
/// gates, measurements, and resets into an internal operation list.
pub(crate) trait OperationReceiver {
    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        call_stack: LogicalStack,
    );

    fn measurement(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        qubit: usize,
        result: usize,
        call_stack: LogicalStack,
    );

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, call_stack: LogicalStack);
}

impl OperationReceiver for OperationListBuilder {
    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        call_stack: LogicalStack,
    ) {
        let targets = inputs
            .targets
            .iter()
            .map(|q| wire_map.qubit_wire(*q))
            .collect::<Vec<_>>();
        let controls = inputs
            .controls
            .iter()
            .map(|q| wire_map.qubit_wire(*q))
            .collect::<Vec<_>>();
        self.push_op(
            OperationOrGroup::new_unitary(name, is_adjoint, &targets, &controls, args),
            call_stack,
            wire_map,
        );
    }

    fn measurement(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        qubit: usize,
        result: usize,
        call_stack: LogicalStack,
    ) {
        let qubit = wire_map.qubit_wire(qubit);
        let result = wire_map.result_wire(result);
        if name == "MResetZ" {
            self.push_op(
                OperationOrGroup::new_measurement("M", qubit, result),
                call_stack.clone(),
                wire_map,
            );
            self.push_op(OperationOrGroup::new_reset(qubit), call_stack, wire_map);
        } else {
            self.push_op(
                OperationOrGroup::new_measurement(name, qubit, result),
                call_stack.clone(),
                wire_map,
            );
        }
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, call_stack: LogicalStack) {
        let qubit = wire_map.qubit_wire(qubit);
        self.push_op(OperationOrGroup::new_reset(qubit), call_stack, wire_map);
    }
}

// TODO: this should be renamed to clarify purpose
pub struct LexicalScope {
    /// The start offset of the scope, used for navigation.
    pub(crate) location: Option<PackageOffset>,
    /// A display name for the scope.
    pub(crate) name: Rc<str>,
    /// Whether the scope represents an adjoint operation,
    /// used for display purposes.
    pub(crate) is_adjoint: bool,
    /// Whether the scope is conditionally applied. TODO: clarify wording
    pub(crate) is_conditional: bool,
}

pub(crate) fn add_scoped_op(
    current_container: &mut OperationOrGroup,
    current_scope_stack: &ScopeStack,
    mut op: OperationOrGroup,
    op_call_stack: &LogicalStack,
    group_by_scope: bool,
    set_source_location: bool,
    wire_map: &WireMap,
) {
    if set_source_location && let Some(called_at) = op_call_stack.0.last() {
        op.location = Some(*called_at.location());
    }

    let default = LogicalStack::default();
    let op_call_stack = if group_by_scope {
        op_call_stack
    } else {
        &default
    };

    let Some(relative_stack) = strip_scope_stack_prefix(op_call_stack, current_scope_stack) else {
        panic!(
            "op_call_stack should be a child of current_scope_stack\n  {op_call_stack:?}\n  {current_scope_stack:?}",
        );
    };

    if !relative_stack.0.is_empty() {
        if let Some(last_op) = current_container
            .children_mut()
            .expect("currentcontainer should be a group")
            .last_mut()
        {
            // See if we can add to the last scope inside the current container
            if let Some(last_scope_stack) = last_op.scope_stack_if_group()
                && strip_scope_stack_prefix(op_call_stack, last_scope_stack).is_some()
            {
                // The last scope matched, add to it
                let last_scope_stack = last_scope_stack.clone();

                // last_op.merge_inputs(&op);
                // let last_op_children = last_op.children_mut().expect("operation should be a group");

                // Recursively add to the children
                add_scoped_op(
                    last_op,
                    &last_scope_stack,
                    op.clone(),
                    op_call_stack,
                    group_by_scope,
                    set_source_location,
                    wire_map,
                );
                current_container.merge_inputs(&op);

                return;
            }
        }

        let op_scope_stack = scope_stack(op_call_stack);
        if *current_scope_stack != op_scope_stack {
            // Need to create a new scope group
            let mut scope_group = OperationOrGroup::new_group(op_scope_stack, wire_map);
            scope_group.merge_inputs(&op);
            *scope_group
                .children_mut()
                .expect("operation should be a group") = vec![op];

            let parent = LogicalStack(
                op_call_stack
                    .0
                    .split_last()
                    .expect("should have more than one frame")
                    .1
                    .to_vec(),
            );

            // Recursively add the new scope group to the current container
            add_scoped_op(
                current_container,
                current_scope_stack,
                scope_group.clone(),
                &parent,
                group_by_scope,
                set_source_location,
                wire_map,
            );
            current_container.merge_inputs(&scope_group);

            return;
        }
    }

    current_container.merge_inputs(&op);
    current_container
        .children_mut()
        .expect("current_container should be a group")
        .push(op);
}

pub(crate) fn retain_user_frames(
    user_package_ids: &[PackageId],
    mut location_stack: LogicalStack,
) -> LogicalStack {
    location_stack.0.retain(|location| {
        let package_id = location.package_id();
        // If no package ID, always include
        package_id.is_none_or(|package_id| {
            user_package_ids.is_empty() || user_package_ids.contains(&package_id)
        })
    });
    LogicalStack(location_stack.0)
}

/// Represents a scope in the call stack, tracking the caller chain and current scope identifier.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScopeStack {
    caller: LogicalStack,
    scope: Scope,
}

impl ScopeStack {
    pub(crate) fn new(caller: LogicalStack, scope: Scope) -> Self {
        Self { caller, scope }
    }
    pub(crate) fn caller(&self) -> &LogicalStack {
        &self.caller
    }

    pub(crate) fn current_lexical_scope(&self) -> &Scope {
        &self.scope
    }

    pub(crate) fn is_top(&self) -> bool {
        self.caller.0.is_empty() && self.scope == Scope::default()
    }

    pub(crate) fn top() -> Self {
        ScopeStack {
            caller: LogicalStack::default(),
            scope: Scope::default(),
        }
    }

    fn resolve_scope(
        &self,
        source_lookup: &impl SourceLookup,
        loop_id_cache: &mut LoopIdCache,
    ) -> LexicalScope {
        source_lookup.resolve_scope(&self.scope, loop_id_cache)
    }
}

/// Strips a scope stack prefix from a call stack.
///
/// The `full_call_stack` parameter represents a complete call stack, while
/// `prefix_scope_stack` represents a scope stack to match against.
///
/// If `prefix_scope_stack` is not a prefix of `full_call_stack`, this function returns `None`.
///
/// If it is a prefix, this function returns the remainder of `full_call_stack` after removing
/// the prefix, starting from the first location in the call stack that is in the scope of
/// `prefix_scope_stack.scope`.
pub(crate) fn strip_scope_stack_prefix(
    full_call_stack: &LogicalStack,
    prefix_scope_stack: &ScopeStack,
) -> Option<LogicalStack> {
    if prefix_scope_stack.is_top() {
        return Some(full_call_stack.clone());
    }

    if full_call_stack.0.len() > prefix_scope_stack.caller().0.len()
        && let Some(rest) = full_call_stack
            .0
            .strip_prefix(prefix_scope_stack.caller().0.as_slice())
        && rest[0].lexical_scope() == prefix_scope_stack.current_lexical_scope()
    {
        assert!(!rest.is_empty());
        return Some(LogicalStack(rest.to_vec()));
    }
    None
}

pub(crate) fn scope_stack(instruction_stack: &LogicalStack) -> ScopeStack {
    instruction_stack
        .0
        .split_last()
        .map_or(ScopeStack::top(), |(last, prefix)| ScopeStack {
            caller: LogicalStack(prefix.to_vec()),
            scope: last.lexical_scope().clone(),
        })
}

#[derive(Clone, Debug, Default, PartialEq)]
/// A "logical" stack trace. This is a processed version of a raw stack trace
/// captured from the evaluator.
/// This stack trace doesn't only contain calls to callables, but also entries into scopes
/// that are deemed to be interesting such as loops and lexical blocks.
pub struct LogicalStack(pub Vec<LogicalStackEntry>);

impl LogicalStack {
    #[must_use]
    pub fn from_evaluator_trace(trace: &[Frame]) -> Self {
        let call_stack = trace
            .iter()
            .flat_map(|frame| {
                let mut logical_stack = vec![LogicalStackEntry::new_call_site(
                    PackageOffset {
                        package_id: frame.id.package,
                        offset: frame.span.lo,
                    },
                    Scope::Callable(CallableId::Id(frame.id, frame.functor)),
                )];

                // Insert any loop frames
                if !frame.loop_iterations.is_empty() {
                    for loop_scope in &frame.loop_iterations {
                        let last = logical_stack.last_mut().expect("there should be a frame");
                        let last_call_site = last.location;
                        last.location = LogicalStackEntryLocation::Loop(LoopId::Id(
                            frame.id.package,
                            loop_scope.loop_expr,
                        ));
                        logical_stack.push(LogicalStackEntry::new(
                            last_call_site,
                            Scope::Loop(LoopId::Id(frame.id.package, loop_scope.loop_expr)),
                        ));
                        let last = logical_stack.last_mut().expect("there should be a frame");
                        let last_location = last.location;
                        last.location = LogicalStackEntryLocation::LoopIteration(
                            LoopId::Id(frame.id.package, loop_scope.loop_expr),
                            loop_scope.iteration_count,
                        );
                        logical_stack.push(LogicalStackEntry::new(
                            last_location,
                            Scope::LoopIteration(
                                LoopId::Id(frame.id.package, loop_scope.loop_expr),
                                loop_scope.iteration_count,
                            ),
                        ));
                    }
                }

                logical_stack
            })
            .collect::<Vec<_>>();

        LogicalStack(call_stack)
    }
}

/// An entry in a logical stack trace.
#[derive(Clone, Debug, PartialEq)]
pub struct LogicalStackEntry {
    /// Location of the "call" into the next entry.
    /// The location type should correspond to the next entry's scope, e.g. a `LogicalStackEntryLocation::Call`
    /// would be followed by a `Scope::Callable` in the stack trace.
    /// Used as a discriminator when grouping. Within a scope, each distinct call/loop should have a unique location.
    pub(crate) location: LogicalStackEntryLocation,
    /// The lexical scope of this stack trace entry.
    /// Instructions that share a scope will be grouped together in the circuit diagram.
    pub(crate) scope: Scope,
}

impl LogicalStackEntry {
    #[must_use]
    pub fn lexical_scope(&self) -> &Scope {
        &self.scope
    }

    #[must_use]
    pub fn location(&self) -> &LogicalStackEntryLocation {
        &self.location
    }

    #[must_use]
    pub fn package_id(&self) -> Option<PackageId> {
        match self.scope {
            Scope::Callable(
                CallableId::Source(PackageOffset { package_id, .. }, _)
                | CallableId::Id(
                    StoreItemId {
                        package: package_id,
                        ..
                    },
                    _,
                ),
            )
            | Scope::LoopIteration(
                LoopId::Id(package_id, _) | LoopId::Source(PackageOffset { package_id, .. }),
                _,
            )
            | Scope::Loop(
                LoopId::Id(package_id, _) | LoopId::Source(PackageOffset { package_id, .. }),
            ) => Some(package_id),
            Scope::Top | Scope::ClassicallyControlled { .. } => None,
        }
    }

    pub(crate) fn new_call_site(package_offset: PackageOffset, scope: Scope) -> Self {
        Self {
            location: LogicalStackEntryLocation::Source(package_offset),
            scope,
        }
    }

    pub(crate) fn new(location: LogicalStackEntryLocation, scope: Scope) -> Self {
        Self { location, scope }
    }
}

#[derive(Clone, Debug, Copy)]
/// In a stack trace, represents the location of each entry.
pub enum LogicalStackEntryLocation {
    /// A branch. The `Option<PackageOffset>` is the location of the branch instruction, if known.
    /// The `bool` indicates which branch (true or false).
    Branch(Option<PackageOffset>, bool),
    /// Source code location at the given package offset.
    Source(PackageOffset),
    /// A loop. The `ExprId` identifies the loop expression.
    Loop(LoopId),
    /// An iteration of a loop. The `usize` is the iteration count
    /// and is used to discriminate different iterations. The `ExprId` identifies
    /// the loop expression.
    LoopIteration(LoopId, usize),
    /// Location is unknown. Always unique.
    Unknown,
}

impl PartialEq for LogicalStackEntryLocation {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Branch(loc1, val1), Self::Branch(loc2, val2)) => loc1 == loc2 && val1 == val2,
            (Self::Source(loc1), Self::Source(loc2)) => loc1 == loc2,
            (Self::Loop(loop_id1), Self::Loop(loop_id2)) => loop_id1 == loop_id2,
            (Self::LoopIteration(loop_id1, iter1), Self::LoopIteration(loop_id2, iter2)) => {
                loop_id1 == loop_id2 && iter1 == iter2
            }
            // Unknowns are always unique
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Scope {
    #[default]
    /// The top-level scope.
    Top,
    /// A callable.
    Callable(CallableId),
    /// A loop. The `ExprId` identifies the loop expression.
    Loop(LoopId),
    /// A loop body.  The `ExprId` identifies the loop expression.
    /// The `usize` is the iteration count.
    LoopIteration(LoopId, usize),
    /// A conditional branch. The `String` is a label for the condition expression.
    ClassicallyControlled {
        label: String,
        control_result_ids: Vec<usize>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum CallableId {
    Id(StoreItemId, FunctorApp),
    Source(PackageOffset, Rc<str>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoopId {
    Id(PackageId, ExprId),
    Source(PackageOffset),
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub struct PackageOffset {
    pub package_id: PackageId,
    pub offset: u32,
}

#[cfg(test)]
pub(crate) struct LogicalStackWithSourceLookup<'a, S> {
    pub(crate) trace: LogicalStack,
    pub(crate) source_lookup: &'a S,
}

#[cfg(test)]
impl<S: SourceLookup> Display for LogicalStackWithSourceLookup<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.trace.0.is_empty() {
            write!(f, "[no stack]")?;
            return Ok(());
        }
        let mut loop_id_cache = Default::default();
        for (i, frame) in self.trace.0.iter().enumerate() {
            if i > 0 {
                write!(f, " -> ")?;
            }

            let scope = self
                .source_lookup
                .resolve_scope(&frame.scope, &mut loop_id_cache);
            write!(
                f,
                "{}{}",
                scope.name,
                if scope.is_adjoint { "†" } else { "" },
            )?;
            let package_offset = self
                .source_lookup
                .resolve_logical_stack_entry_location(frame.location, &mut loop_id_cache);
            if let Some(package_offset) = package_offset {
                let l = self.source_lookup.resolve_package_offset(&package_offset);
                write!(f, "@{}:{}:{}", l.file, l.line, l.column)?;
            }
            if let LogicalStackEntryLocation::LoopIteration(_, iteration) = frame.location {
                write!(f, "[{iteration}]")?;
            }
            if let LogicalStackEntryLocation::Branch(_, val) = frame.location {
                write!(f, "[{val}]")?;
            }
            if let LogicalStackEntryLocation::Unknown = frame.location {
                write!(f, "[unknown]")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct ScopeStackWithSourceLookup<'a, S> {
    pub(crate) scope_stack: ScopeStack,
    pub(crate) source_lookup: &'a S,
}

#[cfg(test)]
impl<S: SourceLookup> Display for ScopeStackWithSourceLookup<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} -> ",
            LogicalStackWithSourceLookup {
                trace: self.scope_stack.caller.clone(),
                source_lookup: self.source_lookup,
            }
        )?;
        let scope = self
            .source_lookup
            .resolve_scope(&self.scope_stack.scope, &mut Default::default());
        write!(
            f,
            "{}{}",
            scope.name,
            if scope.is_adjoint { "†" } else { "" },
        )?;
        Ok(())
    }
}
