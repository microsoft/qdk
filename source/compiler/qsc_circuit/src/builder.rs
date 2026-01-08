// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
pub(crate) mod tests;

use crate::{
    circuit::{
        Circuit, ComponentColumn, Ket, Measurement, Metadata, Operation, PackageOffset, Qubit,
        Register, ResolvedSourceLocation, SourceLocation, Unitary, operation_list_to_grid,
    },
    group_qubits,
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
use qsc_fir::fir::{self, PackageId, StoreItemId};
use qsc_frontend::compile::PackageStore;
use qsc_hir::hir;
use qsc_lowerer::{map_fir_local_item_to_hir, map_fir_package_to_hir};
use rustc_hash::FxHashSet;
use std::{
    fmt::{Debug, Write},
    hash::Hash,
    mem::replace,
    rc::Rc,
};

pub(crate) type LogicalStackTrace = Vec<LocationMetadata>;
pub(crate) type LogicalStackTraceRef<'a> = &'a [LocationMetadata];

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
            &GateInputs {
                targets,
                control_qubits: controls,
                control_results: &[],
            },
            display_args,
            map_stack_frames_to_locations(stack),
        );
    }

    fn measure(&mut self, stack: &[Frame], name: &str, q: usize, val: &val::Result) {
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
            self.circuit_builder.mresetz(
                self.wire_map_builder.current(),
                q,
                r,
                map_stack_frames_to_locations(stack),
            );
        } else {
            self.circuit_builder.m(
                self.wire_map_builder.current(),
                q,
                r,
                map_stack_frames_to_locations(stack),
            );
        }
    }

    fn reset(&mut self, stack: &[Frame], q: usize) {
        self.classical_one_qubits
            .remove(&self.wire_map_builder.wire_map.qubit_wire(q));
        self.circuit_builder.reset(
            self.wire_map_builder.current(),
            q,
            map_stack_frames_to_locations(stack),
        );
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
                control_qubits: &[],
                control_results: &[],
            },
            if classical_args.is_empty() {
                vec![]
            } else {
                vec![classical_args]
            },
            map_stack_frames_to_locations(stack),
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
                config.user_code_only,
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

        CircuitTracer {
            config,
            wire_map_builder: WireMapBuilder::new(params),
            circuit_builder: OperationListBuilder::new(
                config.max_operations,
                user_package_ids.to_vec(),
                config.group_by_scope,
                config.source_locations,
                config.user_code_only,
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
                self.config.user_code_only,
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
        let mut operations: Vec<OperationOrGroup> = operations.to_vec();
        let mut qubits = self.wire_map_builder.wire_map.to_qubits();
        // We need to pass the original number of qubits, before any trimming, to finish the circuit below.
        let num_qubits = qubits.len();

        if self.config.prune_classical_qubits {
            // Remove qubits that are always classical.
            qubits.retain(|q| self.superposition_qubits.contains(&q.id.into()));

            // Remove operations that don't use any non-classical qubits.
            operations.retain_mut(|op| self.should_keep_operation_mut(op));
        }

        let operations = operations
            .iter()
            .map(|o| o.clone().into_operation(source_lookup))
            .collect();

        finish_circuit(
            qubits,
            operations,
            num_qubits,
            source_lookup,
            self.config.loop_detection,
            self.config.collapse_qubit_registers,
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
        if !self.config.source_locations || stack.is_empty() || self.user_package_ids.is_empty() {
            return None;
        }
        first_user_code_location(&self.user_package_ids, stack)
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

fn first_user_code_location(
    user_package_ids: &[PackageId],
    stack: &[Frame],
) -> Option<PackageOffset> {
    for frame in stack.iter().rev() {
        if user_package_ids.contains(&frame.id.package) {
            return Some(PackageOffset {
                package_id: frame.id.package,
                offset: frame.span.lo,
            });
        }
    }

    None
}

pub(crate) fn finish_circuit(
    mut qubits: Vec<Qubit>,
    mut operations: Vec<Operation>,
    _num_qubits: usize,
    source_location_lookup: &impl SourceLookup,
    loop_detection: bool,
    collapse_qubit_registers: bool,
) -> Circuit {
    resolve_locations(&mut operations, source_location_lookup);

    for q in &mut qubits {
        for source_location in &mut q.declarations {
            resolve_source_location_if_unresolved(source_location, source_location_lookup);
        }
    }

    let (operations, qubits) = if collapse_qubit_registers && qubits.len() > 2 {
        // TODO: dummy values for now
        group_qubits(operations, qubits, &[0, 1])
    } else {
        (operations, qubits)
    };

    let component_grid = operation_list_to_grid(operations, &qubits, loop_detection);
    Circuit {
        qubits,
        component_grid,
    }
}

pub trait SourceLookup {
    fn resolve_location(&self, package_offset: &PackageOffset) -> ResolvedSourceLocation;
    fn resolve_scope(&self, scope: ScopeId) -> LexicalScope;
}

impl SourceLookup for PackageStore {
    fn resolve_location(&self, package_offset: &PackageOffset) -> ResolvedSourceLocation {
        let package = self
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

    fn resolve_scope(&self, scope_id: ScopeId) -> LexicalScope {
        let ScopeId(item_id, functor_app) = scope_id;
        let package = self
            .get(map_fir_package_to_hir(item_id.package))
            .expect("package id must exist in store");

        let item = package
            .package
            .items
            .get(map_fir_local_item_to_hir(item_id.item))
            .expect("item id must exist in package");

        let hir::ItemKind::Callable(callable_decl) = &item.kind else {
            panic!("only callables should be in the stack");
        };

        // Use the proper spec declaration from the source code
        // depending on which functor application was used.
        let spec_decl = match (functor_app.adjoint, functor_app.controlled) {
            (false, 0) => Some(&callable_decl.body),
            (false, _) => callable_decl.ctl.as_ref(),
            (true, 0) => callable_decl.adj.as_ref(),
            (true, _) => callable_decl.ctl_adj.as_ref(),
        };

        let spec_decl = spec_decl.unwrap_or(&callable_decl.body);
        let scope_start_offset = spec_decl.span.lo;
        let scope_name = callable_decl.name.name.clone();

        LexicalScope::Callable {
            location: PackageOffset {
                package_id: item_id.package,
                offset: scope_start_offset,
            },
            name: scope_name,
            functor_app,
        }
    }
}

fn resolve_locations(operations: &mut [Operation], source_location_lookup: &impl SourceLookup) {
    for op in operations {
        let children_columns = op.children_mut();
        for column in children_columns {
            resolve_locations(&mut column.components, source_location_lookup);
        }

        if let Some(source) = op.source_location_mut() {
            resolve_source_location_if_unresolved(source, source_location_lookup);
        }

        if let Some(source) = op.scope_location_mut() {
            resolve_source_location_if_unresolved(source, source_location_lookup);
        }

        if let Operation::Unitary(Unitary {
            metadata:
                Some(Metadata {
                    scope_location: Some(scope_location),
                    ..
                }),
            ..
        }) = op
        {
            resolve_source_location_if_unresolved(scope_location, source_location_lookup);
        }
    }
}

pub(crate) fn resolve_source_location_if_unresolved(
    source_location: &mut SourceLocation,
    source_location_lookup: &impl SourceLookup,
) {
    match source_location {
        SourceLocation::Resolved(_) => {}
        SourceLocation::Unresolved(package_offset) => {
            let resolved_source_location = source_location_lookup.resolve_location(package_offset);
            *source_location = SourceLocation::Resolved(resolved_source_location);
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
    /// Detect repeated motifs in the circuit and group them into sub-circuits
    pub loop_detection: bool,
    /// Group operations according to call graph in the circuit diagram
    pub group_by_scope: bool,
    /// Collapse qubit registers into single qubits
    pub collapse_qubit_registers: bool,
    /// Prune purely classical or unused qubits
    pub prune_classical_qubits: bool,
    /// Filter to user code only when capturing source locations and grouping by scope
    pub user_code_only: bool,
}

impl TracerConfig {
    /// Set to the current UI limit + 1 so that it still triggers
    /// the "this circuit has too many gates" warning in the UI.
    /// (see npm\qsharp\ux\circuit.tsx)
    ///
    /// A more refined way to do this might be to communicate the
    /// "limit exceeded" state up to the UI somehow.
    const DEFAULT_MAX_OPERATIONS: usize = 10001;
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            max_operations: Self::DEFAULT_MAX_OPERATIONS,
            source_locations: true,
            loop_detection: false,
            group_by_scope: true,
            collapse_qubit_registers: false,
            prune_classical_qubits: false,
            user_code_only: true,
        }
    }
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

    pub fn to_qubits(&self) -> Vec<Qubit> {
        let mut qubits = vec![];
        for (QubitWire(wire_id), (declarations, results)) in self.qubit_wires.iter() {
            qubits.push(Qubit {
                id: wire_id,
                num_results: results.len(),
                declarations: declarations
                    .iter()
                    .map(|span| SourceLocation::Unresolved(*span))
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
    op: Operation,
}

fn map_stack_frames_to_locations(stack: &[Frame]) -> LogicalStackTrace {
    stack
        .iter()
        .map(|frame| {
            LocationMetadata::new(
                PackageOffset {
                    package_id: frame.id.package,
                    offset: frame.span.lo,
                },
                ScopeId(frame.id, frame.functor),
            )
        })
        .collect::<Vec<_>>()
}

#[derive(Clone, Debug)]
pub(crate) enum OperationOrGroupKind {
    Single,
    ScopeGroup {
        scope_stack: ScopeStack,
        children: Vec<OperationOrGroup>,
    },
    ConditionalGroup {
        label: String,
        children: Vec<OperationOrGroup>,
        scope_location: Option<PackageOffset>,
    },
}

impl OperationOrGroup {
    pub(crate) fn new_single(op: Operation) -> Self {
        Self {
            kind: OperationOrGroupKind::Single,
            op,
        }
    }

    pub(crate) fn new_unitary(
        name: &str,
        is_adjoint: bool,
        targets: &[QubitWire],
        controls: &[QubitWire],
        control_results: &[ResultWire],
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
                .chain(control_results.iter().map(|r| Register {
                    qubit: r.0,
                    result: Some(r.1),
                }))
                .collect(),
            is_adjoint,
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

    pub(crate) fn new_ket(qubit: QubitWire) -> Self {
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

    pub(crate) fn all_results(&self) -> Vec<ResultWire> {
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
        if let OperationOrGroupKind::ScopeGroup { children, .. } = &mut self.kind {
            Some(children)
        } else {
            None
        }
    }

    pub(crate) fn new_scope_group(scope_stack: ScopeStack, children: Vec<Self>) -> Self {
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
            kind: OperationOrGroupKind::ScopeGroup {
                scope_stack,
                children,
            },
            op: Operation::Unitary(Unitary {
                gate: String::new(), // TODO: to be filled in later
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
                metadata: None,
            }),
        }
    }

    pub(crate) fn new_conditional_group(
        label: String,
        scope_location: Option<PackageOffset>,
        children: Vec<Self>,
        control_results: Vec<ResultWire>,
        targets: Vec<QubitWire>,
    ) -> Self {
        Self {
            kind: OperationOrGroupKind::ConditionalGroup {
                label: label.clone(),
                children,
                scope_location,
            },
            op: Operation::Unitary(Unitary {
                gate: label,
                args: vec![],
                children: vec![],
                targets: targets
                    .into_iter()
                    .map(|q| Register {
                        qubit: q.0,
                        result: None,
                    })
                    .collect(),
                controls: control_results
                    .into_iter()
                    .map(|r| Register {
                        qubit: r.0,
                        result: Some(r.1),
                    })
                    .collect(),
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

    pub(crate) fn scope_stack_if_group(&self) -> Option<&ScopeStack> {
        if let OperationOrGroupKind::ScopeGroup { scope_stack, .. } = &self.kind {
            Some(scope_stack)
        } else {
            None
        }
    }

    pub(crate) fn set_location(&mut self, location: PackageOffset) {
        self.op
            .source_location_mut()
            .replace(SourceLocation::Unresolved(location));
    }

    pub(crate) fn into_operation(mut self, scope_resolver: &impl SourceLookup) -> Operation {
        match self.kind {
            OperationOrGroupKind::Single => self.op,
            OperationOrGroupKind::ScopeGroup {
                scope_stack,
                children,
            } => {
                let Operation::Unitary(u) = &mut self.op else {
                    panic!("group operation should be a unitary")
                };
                let scope = scope_stack.resolve_scope(scope_resolver);
                let scope_location = scope.location();
                u.gate = scope.name();
                if let Some(scope_location) = scope_location {
                    if u.metadata.is_none() {
                        u.metadata = Some(Metadata {
                            source: None,
                            scope_location: None,
                        });
                    }
                    u.metadata
                        .as_mut()
                        .expect("metadata should be set")
                        .scope_location = Some(SourceLocation::Unresolved(scope_location));
                }
                u.children = vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(|o| o.into_operation(scope_resolver))
                        .collect(),
                }];
                self.op
            }
            OperationOrGroupKind::ConditionalGroup {
                label,
                children,
                scope_location,
            } => {
                let Operation::Unitary(u) = &mut self.op else {
                    panic!("group operation should be a unitary")
                };
                u.gate = label;
                if let Some(location) = scope_location {
                    if u.metadata.is_none() {
                        u.metadata = Some(Metadata {
                            source: None,
                            scope_location: None,
                        });
                    }
                    u.metadata
                        .as_mut()
                        .expect("metadata should be set")
                        .scope_location = Some(SourceLocation::Unresolved(location));
                }
                u.children = vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(|o| o.into_operation(scope_resolver))
                        .collect(),
                }];
                self.op
            }
        }
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
    operations: Vec<OperationOrGroup>,
    user_package_ids: Vec<PackageId>,
    grouping_config: GroupingConfig,
}

#[derive(Clone, Copy)]
pub(crate) struct GroupingConfig {
    pub(crate) user_code_only: bool,
    pub(crate) source_locations: bool,
    pub(crate) group_by_scope: bool,
}

impl OperationListBuilder {
    pub fn new(
        max_operations: usize,
        user_package_ids: Vec<PackageId>,
        group_by_scope: bool,
        source_locations: bool,
        user_code_only: bool,
    ) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
            grouping_config: GroupingConfig {
                user_code_only,
                source_locations,
                group_by_scope,
            },
            user_package_ids,
        }
    }

    fn push_op(&mut self, op: OperationOrGroup, unfiltered_call_stack: LogicalStackTrace) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        let op_call_stack =
            if self.grouping_config.group_by_scope || self.grouping_config.source_locations {
                retain_user_frames(
                    self.grouping_config.user_code_only,
                    &self.user_package_ids,
                    unfiltered_call_stack,
                )
            } else {
                vec![]
            };

        add_scoped_op(
            &mut self.operations,
            &ScopeStack::top(),
            op,
            &op_call_stack,
            self.grouping_config.group_by_scope,
            self.grouping_config.source_locations,
        );
    }

    fn operations(&self) -> &Vec<OperationOrGroup> {
        &self.operations
    }

    pub(crate) fn into_operations(self) -> Vec<OperationOrGroup> {
        self.operations
    }

    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        call_stack: LogicalStackTrace,
    ) {
        let targets = inputs
            .targets
            .iter()
            .map(|q| wire_map.qubit_wire(*q))
            .collect::<Vec<_>>();
        let controls = inputs
            .control_qubits
            .iter()
            .map(|q| wire_map.qubit_wire(*q))
            .collect::<Vec<_>>();
        self.push_op(
            OperationOrGroup::new_unitary(name, is_adjoint, &targets, &controls, &[], args),
            call_stack,
        );
    }

    fn m(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        call_stack: LogicalStackTrace,
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
        call_stack: LogicalStackTrace,
    ) {
        let qubit = wire_map.qubit_wire(qubit);
        let result = wire_map.result_wire(result);
        self.push_op(
            OperationOrGroup::new_measurement("MResetZ", qubit, result),
            call_stack.clone(),
        );
        self.push_op(OperationOrGroup::new_ket(qubit), call_stack);
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, call_stack: LogicalStackTrace) {
        let qubit = wire_map.qubit_wire(qubit);
        self.push_op(OperationOrGroup::new_ket(qubit), call_stack);
    }
}

pub(crate) struct GateInputs<'a> {
    pub(crate) targets: &'a [usize],
    pub(crate) control_qubits: &'a [usize],
    pub(crate) control_results: &'a [usize],
}

// #[derive(Clone, Debug, PartialEq, Eq, Copy, Hash)]
// pub struct ScopeId(pub(crate) StoreItemId);

// impl Default for ScopeId {
//     fn default() -> Self {
//         ScopeId(StoreItemId {
//             package: usize::MAX.into(),
//             item: usize::MAX.into(),
//         })
//     }
// }

// #[derive(Clone, Debug, PartialEq, Eq)]
// pub enum LexicalScope {
//     Top,
//     Named {
//         name: Rc<str>,
//         location: PackageOffset,
//     },
// }

// impl LexicalScope {
//     pub(crate) fn top() -> Self {
//         LexicalScope::Top
//     }

//     pub(crate) fn location(&self) -> PackageOffset {
//         match self {
//             // TODO: handle Top case properly
//             LexicalScope::Top => PackageOffset {
//                 package_id: PackageId::CORE.successor().successor(),
//                 offset: 0,
//             },
//             LexicalScope::Named { location, .. } => *location,
//         }
//     }

//     pub(crate) fn name(&self) -> String {
//         match self {
//             LexicalScope::Top => "Top".to_string(),
//             LexicalScope::Named { name, .. } => name.to_string(),
//         }
//     }
// }

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct ScopeId(pub(crate) StoreItemId, pub(crate) FunctorApp);

impl Default for ScopeId {
    /// Default represents the "Top" scope
    fn default() -> Self {
        ScopeId(
            StoreItemId {
                package: usize::MAX.into(),
                item: usize::MAX.into(),
            },
            FunctorApp {
                adjoint: false,
                controlled: 0,
            },
        )
    }
}

impl Hash for ScopeId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        let FunctorApp {
            adjoint,
            controlled,
        } = self.1;
        adjoint.hash(state);
        controlled.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LexicalScope {
    Top,
    Callable {
        name: Rc<str>,
        functor_app: FunctorApp,
        location: PackageOffset,
    },
}

impl LexicalScope {
    fn top() -> Self {
        LexicalScope::Top
    }

    fn location(&self) -> Option<PackageOffset> {
        match self {
            LexicalScope::Top => None,
            LexicalScope::Callable { location, .. } => Some(*location),
        }
    }

    fn name(&self) -> String {
        match self {
            LexicalScope::Top => "Top".to_string(),
            LexicalScope::Callable { name, .. } => name.to_string(),
        }
    }

    // fn is_adjoint(&self) -> bool {
    //     match self {
    //         LexicalScope::Top => false,
    //         LexicalScope::Callable { functor_app, .. } => functor_app.adjoint,
    //     }
    // }
}

/// Inserts an operation into a hierarchical structure that mirrors the call stack.
///
/// This function collapses flat call stack traces into nested groups, creating a tree structure
/// where operations are organized by the lexical scopes (functions/operations) they were called from.
///
/// In principle, this is similar to how a profiling tool, such as flamegraph's stackCollapse,
/// would collapse a series of call stacks into a call hierarchy.
///
/// This allows circuit visualizations to show operations grouped by their calling context.
pub(crate) fn add_scoped_op(
    current_container: &mut Vec<OperationOrGroup>,
    current_scope_stack: &ScopeStack,
    mut op: OperationOrGroup,
    op_call_stack: &[LocationMetadata],
    group_by_scope: bool,
    set_source_location: bool,
) {
    if set_source_location && let Some(called_at) = op_call_stack.last() {
        op.set_location(called_at.source_location());
    }

    let op_call_stack = if group_by_scope { op_call_stack } else { &[] };

    let relative_stack = strip_scope_stack_prefix(
        op_call_stack,
        current_scope_stack,
    ).expect("op_call_stack_rel should be a suffix of op_call_stack_abs after removing current_scope_stack_abs");

    if !relative_stack.is_empty() {
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
                add_scoped_op(
                    last_op_children,
                    &last_scope_stack,
                    op,
                    op_call_stack,
                    group_by_scope,
                    set_source_location,
                );

                return;
            }
        }

        let op_scope_stack = scope_stack(op_call_stack);
        if *current_scope_stack != op_scope_stack {
            // Need to create a new scope group
            let scope_group = OperationOrGroup::new_scope_group(op_scope_stack, vec![op]);

            let parent = op_call_stack
                .split_last()
                .expect("should have more than one etc")
                .1
                .to_vec();
            // Recursively add the new scope group to the current container
            add_scoped_op(
                current_container,
                current_scope_stack,
                scope_group,
                &parent,
                group_by_scope,
                set_source_location,
            );

            return;
        }
    }

    current_container.push(op);
}

pub(crate) fn retain_user_frames(
    user_code_only: bool,
    user_package_ids: &[PackageId],
    mut location_stack: LogicalStackTrace,
) -> LogicalStackTrace {
    location_stack.retain(|location| {
        let package_id = location.package_id();
        !user_code_only || user_package_ids.is_empty() || user_package_ids.contains(&package_id)
    });

    location_stack
}

/// Represents a location in the source code along with its lexical scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocationMetadata {
    location: PackageOffset,
    scope_id: ScopeId,
}

impl LocationMetadata {
    pub(crate) fn new(location: PackageOffset, scope_id: ScopeId) -> Self {
        Self { location, scope_id }
    }

    fn lexical_scope(&self) -> ScopeId {
        self.scope_id
    }

    fn package_id(&self) -> fir::PackageId {
        self.location.package_id
    }

    pub(crate) fn source_location(&self) -> PackageOffset {
        self.location
    }
}

/// Represents a scope in the call stack, tracking the caller chain and current scope identifier.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScopeStack {
    caller: LogicalStackTrace,
    scope: ScopeId,
}

impl ScopeStack {
    fn caller(&self) -> &[LocationMetadata] {
        &self.caller
    }

    fn current_lexical_scope(&self) -> ScopeId {
        assert!(!self.is_top(), "top scope has no lexical scope");
        self.scope
    }

    fn is_top(&self) -> bool {
        self.caller.is_empty() && self.scope == ScopeId::default()
    }

    pub(crate) fn top() -> Self {
        ScopeStack {
            caller: Vec::new(),
            scope: ScopeId::default(),
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
pub(crate) fn strip_scope_stack_prefix(
    full_call_stack: &[LocationMetadata],
    prefix_scope_stack: &ScopeStack,
) -> Option<LogicalStackTrace> {
    if prefix_scope_stack.is_top() {
        return Some(full_call_stack.to_vec());
    }

    if full_call_stack.len() > prefix_scope_stack.caller().len()
        && let Some(rest) = full_call_stack.strip_prefix(prefix_scope_stack.caller())
        && rest[0].lexical_scope() == prefix_scope_stack.current_lexical_scope()
    {
        assert!(!rest.is_empty());
        return Some(rest.to_vec());
    }
    None
}

pub(crate) fn scope_stack(instruction_stack: LogicalStackTraceRef) -> ScopeStack {
    instruction_stack
        .split_last()
        .map_or(ScopeStack::top(), |(youngest, prefix)| ScopeStack {
            caller: prefix.to_vec(),
            scope: youngest.lexical_scope(),
        })
}
