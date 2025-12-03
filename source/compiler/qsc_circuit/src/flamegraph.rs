// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    GroupScopesOptions, Operation, TracerConfig,
    circuit::{
        Ket, Measurement, Metadata, PackageOffset, Register, ResolvedSourceLocation,
        SourceLocation, Unitary,
    },
    operations::QubitParam,
};
use qsc_data_structures::{
    functors::FunctorApp,
    index_map::IndexMap,
    line_column::{Encoding, Position},
};
use qsc_eval::{
    GigaStack,
    backend::Tracer,
    val::{self, Value},
};
use qsc_fir::fir::{self, BlockId, ExprId, LoopScopeId, PackageId, PackageLookup, StoreItemId};
use qsc_frontend::compile::{self};
use qsc_hir::hir;
use qsc_lowerer::{map_fir_local_item_to_hir, map_fir_package_to_hir};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Debug, Display, Write},
    hash::Hash,
    mem::{replace, take},
    rc::Rc,
};

/// Representation of a quantum circuit.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stacks {
    name: String,
    value: usize,
    children: Vec<Stacks>,
}

impl Display for Stacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Simple recursive display
        fn fmt_helper(
            s: &Stacks,
            f: &mut std::fmt::Formatter<'_>,
            indent: usize,
        ) -> std::fmt::Result {
            for _ in 0..indent {
                write!(f, "  ")?;
            }
            writeln!(f, "{} ({})", s.name, s.value)?;
            for child in &s.children {
                fmt_helper(child, f, indent + 1)?;
            }
            Ok(())
        }
        fmt_helper(self, f, 0)
    }
}

/// Circuit builder that implements the `Tracer` trait to build a circuit
/// while tracing execution.
pub struct Profiler {
    config: TracerConfig,
    wire_map_builder: WireMapBuilder,
    circuit_builder: OperationListBuilder,
    next_result_id: usize,
    user_package_ids: Vec<PackageId>,
}

impl Tracer for Profiler {
    fn qubit_allocate(&mut self, stack: &GigaStack, q: usize) {
        let declared_at = self.user_code_call_location(stack);
        self.wire_map_builder.map_qubit(q, declared_at);
    }

    fn qubit_release(&mut self, _stack: &GigaStack, q: usize) {
        self.wire_map_builder.unmap_qubit(q);
    }

    fn qubit_swap_id(&mut self, _stack: &GigaStack, q0: usize, q1: usize) {
        self.wire_map_builder.swap(q0, q1);
    }

    fn gate(
        &mut self,
        stack: &GigaStack,
        name: &str,
        is_adjoint: bool,
        targets: &[usize],
        controls: &[usize],
        theta: Option<f64>,
    ) {
        let called_at = SymbolicStackTrace::from_blended_stacks(stack);
        let display_args: Vec<String> = theta.map(|p| format!("{p:.4}")).into_iter().collect();
        self.circuit_builder.gate(
            self.wire_map_builder.current(),
            name,
            is_adjoint,
            &GateInputs { targets, controls },
            display_args,
            called_at,
        );
    }

    fn measure(&mut self, stack: &GigaStack, name: &str, q: usize, val: &val::Result) {
        let called_at = SymbolicStackTrace::from_blended_stacks(stack);
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
            self.circuit_builder
                .mresetz(self.wire_map_builder.current(), q, r, called_at);
        } else {
            self.circuit_builder
                .m(self.wire_map_builder.current(), q, r, called_at);
        }
    }

    fn reset(&mut self, stack: &GigaStack, q: usize) {
        let called_at = SymbolicStackTrace::from_blended_stacks(stack);
        self.circuit_builder
            .reset(self.wire_map_builder.current(), q, called_at);
    }

    fn custom_intrinsic(&mut self, stack: &GigaStack, name: &str, arg: Value) {
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
            SymbolicStackTrace::from_blended_stacks(stack),
        );
    }

    fn is_stack_tracing_enabled(&self) -> bool {
        self.config.source_locations // || self.config.group_scopes
    }
}

impl Profiler {
    #[must_use]
    pub fn new(config: TracerConfig, user_package_ids: &[PackageId]) -> Self {
        Profiler {
            config,
            wire_map_builder: WireMapBuilder::new(vec![]),
            circuit_builder: OperationListBuilder::new(
                config.max_operations,
                user_package_ids.to_vec(),
                config.group_scopes,
                config.source_locations,
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
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
        let params = operation_qubit_params
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
            .unwrap_or_default();

        Profiler {
            config,
            wire_map_builder: WireMapBuilder::new(params),
            circuit_builder: OperationListBuilder::new(
                config.max_operations,
                user_package_ids.to_vec(),
                config.group_scopes,
                config.source_locations,
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
        }
    }

    #[must_use]
    pub fn snapshot(&self, source_lookup: &impl SourceLookup) -> Stacks {
        self.finish_circuit(self.circuit_builder.operations(), source_lookup)
    }

    #[must_use]
    pub fn finish(mut self, source_lookup: &impl SourceLookup) -> Stacks {
        let ops = replace(
            &mut self.circuit_builder,
            OperationListBuilder::new(
                self.config.max_operations,
                self.user_package_ids.clone(),
                self.config.group_scopes,
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
    ) -> Stacks {
        let mut operations = operations.to_vec();

        if self.config.group_scopes == GroupScopesOptions::GroupScopes {
            // Collapse unnecessary loop scopes
            collapse_unnecessary_loop_scopes(&mut operations);
        }

        finish_circuit(operations, source_lookup)
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

    fn user_code_call_location(&self, stack: &GigaStack) -> Option<PackageOffset> {
        if !self.config.source_locations || stack.0.is_empty() || self.user_package_ids.is_empty() {
            return None;
        }
        first_user_code_location(&self.user_package_ids, stack)
    }
}

fn first_user_code_location(
    user_package_ids: &[PackageId],
    stack: &GigaStack,
) -> Option<PackageOffset> {
    for frame in stack.0.iter().rev() {
        if user_package_ids.contains(&frame.id.package) {
            return Some(PackageOffset {
                package_id: frame.id.package,
                offset: frame.span.lo,
            });
        }
    }

    None
}

fn finish_circuit(operations: Vec<OperationOrGroup>, source_lookup: &impl SourceLookup) -> Stacks {
    let root = if operations.len() == 1
        && matches!(operations[0].kind, OperationOrGroupKind::Group { .. })
    {
        operations
            .into_iter()
            .next()
            .expect("expected exactly one operation")
    } else {
        OperationOrGroup::new_group(ScopeStack::top(), operations)
    };

    operation_to_stacks(root, source_lookup)
}

fn operation_to_stacks(
    mut operation: OperationOrGroup,
    source_lookup: &impl SourceLookup,
) -> Stacks {
    if let OperationOrGroupKind::Group {
        children: ops,
        scope_stack,
    } = &mut operation.kind
    {
        let mut children = vec![];
        let mut value = 0;
        for op in ops.drain(..) {
            if let OperationOrGroupKind::Group { .. } = op.kind {
                let child = operation_to_stacks(op, source_lookup);
                value += child.value;
                children.push(child);
            } else {
                value += 1;
            };
        }

        let Operation::Unitary(u) = &mut operation.op else {
            panic!("group operation should be a unitary")
        };

        let scope = scope_stack.resolve_scope(source_lookup);
        u.gate = scope.name();
        if scope.is_adjoint() {
            u.gate = format!("{}†", u.gate);
        }

        Stacks {
            name: operation.op.gate(),
            value,
            children,
        }
    } else {
        panic!("cannot process non-group operation");
    }
}

fn collapse_unnecessary_loop_scopes(operations: &mut Vec<OperationOrGroup>) {
    let mut ops = vec![];
    for mut op in operations.drain(..) {
        match &mut op.kind {
            OperationOrGroupKind::Single => {}
            OperationOrGroupKind::Group { children, .. } => {
                collapse_unnecessary_loop_scopes(children);
            }
        }

        if let Some(children) = collapse_and_return_children_if_unnecessary(&mut op) {
            ops.extend(children);
        } else {
            ops.push(op);
        }
    }
    *operations = ops;
}

fn collapse_and_return_children_if_unnecessary(
    op: &mut OperationOrGroup,
) -> Option<Vec<OperationOrGroup>> {
    if let OperationOrGroupKind::Group {
        scope_stack,
        children,
    } = &mut op.kind
        && let Scope::Loop(_, LoopScope::Outer(_)) = scope_stack.current_lexical_scope()
    {
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
    }
    None
}

pub trait SourceLookup {
    fn resolve_location(&self, package_offset: &PackageOffset) -> ResolvedSourceLocation;
    fn resolve_scope(&self, scope: Scope) -> LexicalScope;
    fn resolve_block(&self, block: BlockId) -> String;
}

impl SourceLookup for (&compile::PackageStore, &fir::PackageStore) {
    fn resolve_location(&self, package_offset: &PackageOffset) -> ResolvedSourceLocation {
        let package_store = self.0;
        let package = package_store
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

        ResolvedSourceLocation {
            file: source.name.to_string(),
            line: pos.line,
            column: pos.column,
        }
    }

    fn resolve_scope(&self, scope_id: Scope) -> LexicalScope {
        match scope_id {
            Scope::Callable(store_item_id, functor_app) => {
                let package_store = self.0;
                let package = package_store
                    .get(map_fir_package_to_hir(store_item_id.package))
                    .expect("package id must exist in store");

                let item = package
                    .package
                    .items
                    .get(map_fir_local_item_to_hir(store_item_id.item))
                    .expect("item id must exist in package");

                let (scope_offset, scope_name) = match &item.kind {
                    hir::ItemKind::Callable(callable_decl) => {
                        let spec_decl = if functor_app.adjoint {
                            callable_decl.adj.as_ref().unwrap_or(&callable_decl.body)
                        } else {
                            &callable_decl.body
                        };

                        (spec_decl.span.lo, callable_decl.name.name.clone())
                    }
                    _ => panic!("only callables should be in the stack"),
                };

                LexicalScope::Callable {
                    location: PackageOffset {
                        package_id: store_item_id.package,
                        offset: scope_offset,
                    },
                    name: scope_name,
                    functor_app,
                }
            }
            Scope::Loop(package_id, scope) => {
                let package_store = self.1;
                let package = package_store.get(package_id);
                match scope {
                    LoopScope::Outer(expr_id) => {
                        let expr = package.get_expr(expr_id);
                        let expr_contents = self
                            .0
                            .get(map_fir_package_to_hir(package_id))
                            .and_then(|p| p.sources.find_by_offset(expr.span.lo))
                            .map(|s| {
                                s.contents[(expr.span.lo - s.offset) as usize
                                    ..(expr.span.hi - s.offset) as usize]
                                    .to_string()
                            });

                        LexicalScope::LoopOuter {
                            label: format!("loop: {}", expr_contents.unwrap_or_default()),
                        }
                    }
                    LoopScope::Iteration(_, i) => LexicalScope::LoopIteration {
                        label: format!("({i})"),
                    },
                }
            }
            Scope::Top => LexicalScope::Top,
        }
    }

    fn resolve_block(&self, block: BlockId) -> String {
        let package_store = self.1;
        let package = package_store.get(2.into()); // TODO obvs

        match package.blocks.get(block) {
            Some(b) => {
                // TODO
                let contents = self
                    .0
                    .get(2.into())
                    .and_then(|p| p.sources.find_by_offset(b.span.lo))
                    .map(|s| {
                        s.contents[(b.span.lo - s.offset) as usize..(b.span.hi - s.offset) as usize]
                            .to_string()
                    });
                format!("block at {} {}", b.span.lo, contents.unwrap_or_default())
            }
            None => {
                format!("unknown block {block:?}")
            }
        }
    }
}

/// Maps qubit IDs to their corresponding wire IDs and tracks measurement results
/// along with their source locations.
#[derive(Default)]
struct WireMap {
    /// Maps qubit IDs to their assigned wire IDs.
    qubits: IndexMap<usize, QubitWire>,
    /// Maps wire IDs to their declaration locations and measurement result IDs.
    qubit_wires: IndexMap<QubitWire, (Vec<PackageOffset>, Vec<usize>)>,
}

impl WireMap {
    fn qubit_wire(&self, qubit_id: usize) -> QubitWire {
        self.qubits
            .get(qubit_id)
            .expect("qubit should already be mapped")
            .to_owned()
    }

    fn result_wire(&self, result_id: usize) -> ResultWire {
        self.qubit_wires
            .iter()
            .find_map(|(QubitWire(qubit_wire), (_, results))| {
                let r_idx = results.iter().position(|&r| r == result_id);
                r_idx.map(|r_idx| ResultWire(qubit_wire, r_idx))
            })
            .expect("result should already be mapped")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ResultWire(usize, usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct QubitWire(usize);

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
struct WireMapBuilder {
    next_qubit_wire_id: QubitWire,
    wire_map: WireMap,
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

    fn current(&self) -> &WireMap {
        &self.wire_map
    }

    fn map_qubit(&mut self, qubit: usize, declared_at: Option<PackageOffset>) {
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

    fn unmap_qubit(&mut self, q: usize) {
        // Simple behavior assuming qubits are always released in reverse order of allocation
        self.next_qubit_wire_id.0 -= 1;
        self.wire_map.qubits.remove(q);
    }

    fn link_result_to_qubit(&mut self, q: usize, r: usize) {
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

#[derive(Clone)]
struct OperationOrGroup {
    kind: OperationOrGroupKind,
    op: Operation,
}

#[derive(Clone, Default, PartialEq)]
struct SymbolicStackTrace(Vec<SymbolicStackTraceEntry>);

impl SymbolicStackTrace {
    fn from_blended_stacks(stack: &GigaStack) -> Self {
        let call_stack = stack
            .0
            .iter()
            .enumerate()
            .flat_map(|(frame_idx, frame)| {
                let loop_stack = stack
                    .1
                    .iter()
                    .filter(|s| s.frame_id == frame_idx + 1 && s.is_loop)
                    .cloned()
                    .collect::<Vec<_>>();

                let mut stack = vec![SymbolicStackTraceEntry::new_call_site(
                    PackageOffset {
                        package_id: frame.id.package,
                        offset: frame.span.lo,
                    },
                    Scope::Callable(frame.id, frame.functor),
                )];

                // Insert any loop frames
                let mut iteration_count: usize = 0;
                for loop_frame in loop_stack {
                    let loop_scope_id = loop_frame.loop_scope.expect("loop scope should exist");
                    if let LoopScopeId::Outer(_, i) = loop_scope_id {
                        iteration_count = i;
                    }

                    let last = stack.last_mut().expect("there should be a frame");
                    let last_call_discriminator = last.call_site;

                    let loop_scope = match &loop_scope_id {
                        LoopScopeId::Outer(expr_id, _) => LoopScope::Outer(*expr_id),
                        LoopScopeId::Body(block_id) => {
                            LoopScope::Iteration(*block_id, iteration_count)
                        }
                        LoopScopeId::None => panic!("loop scope should exist"),
                    };

                    last.call_site = match &loop_scope_id {
                        LoopScopeId::Outer(expr_id, _) => {
                            CallSite::Loop(frame.id.package, *expr_id)
                        }
                        LoopScopeId::Body(_) => CallSite::LoopIteration(iteration_count),
                        LoopScopeId::None => panic!("loop scope should exist"),
                    };

                    stack.push(SymbolicStackTraceEntry::new(
                        last_call_discriminator,
                        Scope::Loop(frame.id.package, loop_scope),
                    ));
                }
                stack
            })
            .collect::<Vec<_>>();

        SymbolicStackTrace(call_stack)
    }
}

#[derive(Clone)]
enum OperationOrGroupKind {
    Single,
    Group {
        scope_stack: ScopeStack,
        children: Vec<OperationOrGroup>,
    },
}

impl OperationOrGroup {
    fn new_single(op: Operation) -> Self {
        Self {
            kind: OperationOrGroupKind::Single,
            op,
        }
    }

    fn new_unitary(
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
            metadata: Some(Metadata {
                source: None,
                scope_location: None,
            }),
        }))
    }

    fn new_measurement(label: &str, qubit: QubitWire, result: ResultWire) -> Self {
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

    fn new_ket(qubit: QubitWire) -> Self {
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

    fn all_qubits(&self) -> Vec<QubitWire> {
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

    fn all_results(&self) -> Vec<ResultWire> {
        let results: FxHashSet<ResultWire> = match &self.op {
            Operation::Measurement(measurement) => measurement
                .results
                .iter()
                .filter_map(|r| r.result.map(|res| ResultWire(r.qubit, res)))
                .collect(),
            Operation::Unitary(unitary) => unitary
                .targets
                .iter()
                .chain(unitary.controls.iter())
                .filter_map(|r| r.result.map(|res| ResultWire(r.qubit, res)))
                .collect(),
            Operation::Ket(_) => vec![],
        }
        .into_iter()
        .collect();
        results.into_iter().collect()
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

    fn new_group(scope_stack: ScopeStack, children: Vec<Self>) -> Self {
        let all_qubits = children
            .iter()
            .flat_map(OperationOrGroup::all_qubits)
            .collect::<FxHashSet<QubitWire>>()
            .into_iter()
            .collect::<Vec<QubitWire>>();

        let all_results = children
            .iter()
            .flat_map(OperationOrGroup::all_results)
            .collect::<FxHashSet<ResultWire>>()
            .into_iter()
            .collect::<Vec<ResultWire>>();

        Self {
            kind: OperationOrGroupKind::Group {
                scope_stack,
                children,
            },
            op: Operation::Unitary(Unitary {
                gate: String::new(), // to be filled in later
                args: vec![],
                children: vec![],
                targets: all_qubits
                    .iter()
                    .map(|q| Register {
                        qubit: q.0,
                        result: None,
                    })
                    .chain(all_results.iter().map(|r| Register {
                        qubit: r.0,
                        result: Some(r.1),
                    }))
                    .collect(),
                controls: vec![],
                is_adjoint: false,
                metadata: Some(Metadata {
                    source: None,
                    scope_location: None,
                }),
            }),
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

    fn scope_stack_if_group(&self) -> Option<&ScopeStack> {
        if let OperationOrGroupKind::Group { scope_stack, .. } = &self.kind {
            Some(scope_stack)
        } else {
            None
        }
    }

    fn set_location(&mut self, location: PackageOffset) {
        self.op
            .source_location_mut()
            .replace(SourceLocation::Unresolved(location));
    }
}

/// Builds a list of circuit operations with a maximum operation limit.
/// Stops adding operations once the limit is exceeded.
///
/// Methods take `WireMap` as a parameter to resolve qubit and result IDs
/// to their corresponding wire positions in the circuit diagram.
struct OperationListBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations: Vec<OperationOrGroup>,
    source_locations: bool,
    user_package_ids: Vec<PackageId>,
    group_scopes: GroupScopesOptions,
}

impl OperationListBuilder {
    fn new(
        max_operations: usize,
        user_package_ids: Vec<PackageId>,
        group_scopes: GroupScopesOptions,
        source_locations: bool,
    ) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
            source_locations,
            user_package_ids,
            group_scopes,
        }
    }

    fn push_op(&mut self, mut op: OperationOrGroup, unfiltered_call_stack: SymbolicStackTrace) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        let op_call_stack = if self.group_scopes.is_grouping_enabled() || self.source_locations {
            retain_user_frames(&self.user_package_ids, unfiltered_call_stack)
        } else {
            SymbolicStackTrace(vec![])
        };

        if self.source_locations
            && let Some(called_at) = op_call_stack.0.last()
            && let Some(source_location) = called_at.source_location()
        {
            op.set_location(source_location);
        }

        let default = SymbolicStackTrace::default();
        add_scoped_op(
            &mut self.operations,
            &ScopeStack::top(),
            op,
            if self.group_scopes.is_grouping_enabled() {
                &op_call_stack
            } else {
                &default
            },
        );
    }

    fn operations(&self) -> &Vec<OperationOrGroup> {
        &self.operations
    }

    fn into_operations(self) -> Vec<OperationOrGroup> {
        self.operations
    }

    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        call_stack: SymbolicStackTrace,
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
        );
    }

    fn m(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        call_stack: SymbolicStackTrace,
    ) {
        let qubit = wire_map.qubit_wire(qubit);
        let result = wire_map.result_wire(result);
        self.push_op(
            OperationOrGroup::new_measurement("M", qubit, result),
            call_stack,
        );
    }

    fn mresetz(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        call_stack: SymbolicStackTrace,
    ) {
        let qubit = wire_map.qubit_wire(qubit);
        let result = wire_map.result_wire(result);
        self.push_op(
            OperationOrGroup::new_measurement("MResetZ", qubit, result),
            call_stack.clone(),
        );
        self.push_op(OperationOrGroup::new_ket(qubit), call_stack);
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, call_stack: SymbolicStackTrace) {
        let qubit = wire_map.qubit_wire(qubit);
        self.push_op(OperationOrGroup::new_ket(qubit), call_stack);
    }
}

struct GateInputs<'a> {
    targets: &'a [usize],
    controls: &'a [usize],
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum Scope {
    Top,
    Callable(StoreItemId, FunctorApp),
    Loop(PackageId, LoopScope),
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum LoopScope {
    Outer(ExprId),
    Iteration(BlockId, usize),
}

impl Default for Scope {
    /// Default represents the "Top" scope
    fn default() -> Self {
        Scope::Top
    }
}

impl Hash for Scope {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Scope::Top => {
                0u8.hash(state);
            }
            Scope::Callable(store_item_id, functor_app) => {
                1u8.hash(state);
                store_item_id.hash(state);
                let FunctorApp {
                    adjoint,
                    controlled,
                } = *functor_app;
                adjoint.hash(state);
                controlled.hash(state);
            }
            Scope::Loop(package_id, block_id) => {
                2u8.hash(state);
                package_id.hash(state);

                match block_id {
                    LoopScope::Outer(expr_id) => {
                        0u8.hash(state);
                        expr_id.hash(state);
                    }
                    LoopScope::Iteration(block_id, i) => {
                        1u8.hash(state);
                        block_id.hash(state);
                        i.hash(state);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LexicalScope {
    Top,
    Callable {
        name: Rc<str>,
        functor_app: FunctorApp,
        location: PackageOffset,
    },
    LoopIteration {
        label: String,
    },
    LoopOuter {
        label: String,
    },
}

impl LexicalScope {
    fn top() -> Self {
        LexicalScope::Top
    }

    fn name(&self) -> String {
        match self {
            LexicalScope::Top => "root".to_string(),
            LexicalScope::Callable { name, .. } => name.to_string(),
            LexicalScope::LoopOuter { label } | LexicalScope::LoopIteration { label } => {
                label.clone()
            }
        }
    }

    fn is_adjoint(&self) -> bool {
        match self {
            LexicalScope::Callable { functor_app, .. } => functor_app.adjoint,
            LexicalScope::Top
            | LexicalScope::LoopOuter { .. }
            | LexicalScope::LoopIteration { .. } => false,
        }
    }
}

fn add_scoped_op(
    current_container: &mut Vec<OperationOrGroup>,
    current_scope_stack: &ScopeStack,
    op: OperationOrGroup,
    op_call_stack: &SymbolicStackTrace,
) {
    let relative_stack = strip_scope_stack_prefix(
        op_call_stack,
        current_scope_stack,
    ).expect("op_call_stack_rel should be a suffix of op_call_stack_abs after removing current_scope_stack_abs");

    if !relative_stack.0.is_empty() {
        if let Some(last_op) = current_container.last_mut() {
            // See if we can add to the last scope inside the current container
            if let Some(last_scope_stack) = last_op.scope_stack_if_group()
                && strip_scope_stack_prefix(op_call_stack, last_scope_stack).is_some()
            {
                // The last scope matched, add to it
                let last_scope_stack = last_scope_stack.clone();

                last_op.extend_target_qubits(&op.all_qubits());
                last_op.extend_target_results(&op.all_results());
                let last_op_children = last_op.children_mut().expect("operation should be a group");

                // Recursively add to the children
                add_scoped_op(last_op_children, &last_scope_stack, op, op_call_stack);

                return;
            }
        }

        let op_scope_stack = scope_stack(&op_call_stack.0);
        if *current_scope_stack != op_scope_stack {
            // Need to create a new scope group
            let scope_group = OperationOrGroup::new_group(op_scope_stack, vec![op]);

            let parent = SymbolicStackTrace(
                op_call_stack
                    .0
                    .split_last()
                    .expect("should have more than one etc")
                    .1
                    .to_vec(),
            );

            // Recursively add the new scope group to the current container
            add_scoped_op(current_container, current_scope_stack, scope_group, &parent);
            return;
        }
    }

    current_container.push(op);
}

fn retain_user_frames(
    user_package_ids: &[PackageId],
    mut location_stack: SymbolicStackTrace,
) -> SymbolicStackTrace {
    location_stack.0.retain(|location| {
        let package_id = location.package_id();
        user_package_ids.is_empty() || user_package_ids.contains(&package_id)
    });

    SymbolicStackTrace(location_stack.0)
}

/// Represents a location in the source code along with its lexical scope.
#[derive(Clone, Debug, PartialEq)]
struct SymbolicStackTraceEntry {
    /// Used as a discriminator. Within a scope, each distinct call should have a unique location.
    call_site: CallSite,
    scope: Scope,
}

impl SymbolicStackTraceEntry {
    fn new_call_site(location: PackageOffset, scope_id: Scope) -> Self {
        Self {
            call_site: CallSite::Call(location),
            scope: scope_id,
        }
    }

    fn new(call_discriminator: CallSite, scope_id: Scope) -> Self {
        Self {
            call_site: call_discriminator,
            scope: scope_id,
        }
    }

    fn lexical_scope(&self) -> Scope {
        self.scope
    }

    fn source_location(&self) -> Option<PackageOffset> {
        match &self.call_site {
            CallSite::Call(location) => Some(*location),
            CallSite::Loop(_, _) | CallSite::LoopIteration(_) => None,
        }
    }

    fn package_id(&self) -> fir::PackageId {
        match self.scope {
            Scope::Top => panic!("top scope has no package"),
            Scope::Callable(store_item_id, _) => store_item_id.package,
            Scope::Loop(package_id, _) => package_id,
        }
    }
}

#[derive(Clone, Debug, Copy, PartialEq)]
enum CallSite {
    Call(PackageOffset),
    Loop(PackageId, ExprId),
    LoopIteration(usize),
}

/// Represents a scope in the call stack, tracking the caller chain and current scope identifier.
#[derive(Clone, PartialEq)]
struct ScopeStack {
    caller: SymbolicStackTrace,
    scope: Scope,
}

impl ScopeStack {
    fn caller(&self) -> &[SymbolicStackTraceEntry] {
        &self.caller.0
    }

    fn current_lexical_scope(&self) -> Scope {
        assert!(!self.is_top(), "top scope has no lexical scope");
        self.scope
    }

    fn is_top(&self) -> bool {
        self.caller.0.is_empty() && self.scope == Scope::default()
    }

    fn top() -> Self {
        ScopeStack {
            caller: SymbolicStackTrace::default(),
            scope: Scope::default(),
        }
    }

    fn resolve_scope(&self, scope_resolver: &impl SourceLookup) -> LexicalScope {
        if self.is_top() {
            LexicalScope::top()
        } else {
            scope_resolver.resolve_scope(self.scope)
        }
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
fn strip_scope_stack_prefix(
    full_call_stack: &SymbolicStackTrace,
    prefix_scope_stack: &ScopeStack,
) -> Option<SymbolicStackTrace> {
    if prefix_scope_stack.is_top() {
        return Some(full_call_stack.clone());
    }

    if full_call_stack.0.len() > prefix_scope_stack.caller().len()
        && let Some(rest) = full_call_stack.0.strip_prefix(prefix_scope_stack.caller())
        && rest[0].lexical_scope() == prefix_scope_stack.current_lexical_scope()
    {
        assert!(!rest.is_empty());
        return Some(SymbolicStackTrace(rest.to_vec()));
    }
    None
}

fn scope_stack(instruction_stack: &[SymbolicStackTraceEntry]) -> ScopeStack {
    instruction_stack
        .split_last()
        .map_or(ScopeStack::top(), |(youngest, prefix)| ScopeStack {
            caller: SymbolicStackTrace(prefix.to_vec()),
            scope: youngest.lexical_scope(),
        })
}

impl Display for OperationOrGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let OperationOrGroupKind::Group {
            scope_stack,
            children,
        } = &self.kind
        {
            write!(f, "group(")?;
            write!(f, "{}", scope_stack.current_lexical_scope())?;
            write!(f, ", {} children", children.len())?;
            write!(f, ")-")?;
        }
        write!(
            f,
            "{}({}){:?}",
            self.op.gate(),
            self.op.args().join(","),
            self.all_qubits()
                .into_iter()
                .map(|q| q.0)
                .collect::<Vec<_>>()
        )?;
        Ok(())
    }
}

impl Display for ScopeStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.caller)?;
        if !self.caller.0.is_empty() {
            write!(f, " -> ")?;
        }
        write!(f, "{}", self.scope)
    }
}

impl Display for SymbolicStackTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, frame) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " -> ")?;
            }
            write!(f, "{frame}")?;
        }
        Ok(())
    }
}

impl Display for SymbolicStackTraceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@", self.scope,)?;
        match &self.call_site {
            CallSite::Call(location) => {
                write!(f, "({}-{})", location.package_id, location.offset)
            }
            CallSite::Loop(package_id, expr_id) => {
                write!(f, "loop_loc({package_id}-{expr_id})")
            }
            CallSite::LoopIteration(i) => {
                write!(f, "loop_iter({i})")
            }
        }
    }
}

impl Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Top => write!(f, "top"),
            Scope::Callable(i, _) => {
                write!(f, "callable{}-{}", i.package, i.item)
            }
            Scope::Loop(_, LoopScope::Iteration(_, i)) => {
                write!(f, "loop-iter({i})")
            }
            Scope::Loop(_, LoopScope::Outer(_)) => write!(f, "loop-outer"),
        }
    }
}
