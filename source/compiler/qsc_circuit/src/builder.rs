// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    ComponentColumn, Qubit,
    circuit::{
        Circuit, Ket, Measurement, Operation, PackageOffset, Register, ResolvedSourceLocation,
        SourceLocation, Unitary, operation_list_to_grid,
    },
    group_qubits,
    operations::QubitParam,
    operations::QubitParamInfo,
    rir_to_circuit::{DbgStuffExt, ScopeResolver, ScopeStack},
};
use qsc_data_structures::{
    index_map::IndexMap,
    line_column::{Encoding, Position},
};
use qsc_eval::{
    backend::Tracer,
    debug::Frame,
    val::{self, Value},
};
use qsc_fir::fir::{self, PackageId, PackageStoreLookup, StoreItemId};
use qsc_frontend::compile::PackageStore;
use qsc_lowerer::map_fir_package_to_hir;
use rustc_hash::FxHashSet;
use std::{
    fmt::{Display, Write},
    mem::replace,
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
            &full_call_stack(stack),
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
            self.circuit_builder.mresetz(
                self.wire_map_builder.current(),
                q,
                r,
                &full_call_stack(stack),
            );
        } else {
            self.circuit_builder.m(
                self.wire_map_builder.current(),
                q,
                r,
                &full_call_stack(stack),
            );
        }
    }

    fn reset(&mut self, stack: &[Frame], q: usize) {
        self.circuit_builder
            .reset(self.wire_map_builder.current(), q, &full_call_stack(stack));
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
            &full_call_stack(stack),
        );
    }

    fn is_stack_tracing_enabled(&self) -> bool {
        self.config.source_locations
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
                config.group_scopes,
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

        CircuitTracer {
            config,
            wire_map_builder: WireMapBuilder::new(params),
            circuit_builder: OperationListBuilder::new(
                config.max_operations,
                user_package_ids.to_vec(),
                config.group_scopes,
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
        }
    }

    #[must_use]
    pub fn snapshot(
        &self,
        source_lookup: &impl SourceLookup,
        fir_package_store: Option<&fir::PackageStore>,
    ) -> Circuit {
        self.finish_circuit_(
            self.circuit_builder.operations(),
            source_lookup,
            fir_package_store,
        )
    }

    #[must_use]
    pub fn finish(
        mut self,
        source_lookup: &impl SourceLookup,
        fir_package_store: Option<&fir::PackageStore>,
    ) -> Circuit {
        let ops = replace(
            &mut self.circuit_builder,
            OperationListBuilder::new(
                self.config.max_operations,
                self.user_package_ids.clone(),
                self.config.group_scopes,
            ),
        )
        .into_operations();

        self.finish_circuit_(&ops, source_lookup, fir_package_store)
    }

    fn finish_circuit_(
        &self,
        operations: &[OperationOrGroup],
        source_lookup: &impl SourceLookup,
        fir_package_store: Option<&fir::PackageStore>,
    ) -> Circuit {
        let operations = operations
            .iter()
            .map(|o| {
                OperationOrGroup::into_operation(o.clone(), &DbgStuffForEval {}, fir_package_store)
            })
            .collect();

        finish_circuit(
            self.wire_map_builder.current(),
            operations,
            source_lookup,
            self.config.loop_detection,
            self.config.collapse_qubit_registers,
        )
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
    wire_map: &WireMap,
    mut operations: Vec<Operation>,
    source_location_lookup: &impl SourceLookup,
    loop_detection: bool,
    collapse_qubit_registers: bool,
) -> Circuit {
    let mut qubits = wire_map.to_qubits();

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
}

fn resolve_locations(operations: &mut [Operation], source_location_lookup: &impl SourceLookup) {
    for op in operations {
        let children_columns = op.children_mut();
        for column in children_columns {
            resolve_locations(&mut column.components, source_location_lookup);
        }

        if let Some(source) = op.source_mut() {
            resolve_source_location_if_unresolved(source, source_location_lookup);
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
    pub group_scopes: bool,
    /// Collapse qubit registers into single qubits
    pub collapse_qubit_registers: bool,
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
            group_scopes: true,
            collapse_qubit_registers: false,
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
            .expect("qubit should already be mapped")
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
pub(crate) struct ResultWire(pub usize, pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct QubitWire(pub usize);

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
struct OperationOrGroup {
    kind: OperationOrGroupKind,
    op: Operation,
}

impl OperationOrGroup {
    fn single(op: Operation) -> Self {
        OperationOrGroup {
            kind: OperationOrGroupKind::Single,
            op,
        }
    }
}

fn full_call_stack(stack: &[Frame]) -> Vec<SourceLocationMetadata> {
    stack
        .iter()
        .map(|frame| SourceLocationMetadata {
            location: PackageOffset {
                package_id: frame.id.package,
                offset: frame.span.lo,
            },
            scope_id: ScopeId(frame.id),
        })
        .collect::<Vec<_>>()
}

#[derive(Clone, Debug)]
enum OperationOrGroupKind {
    Single,
    Group {
        scope_stack: ScopeStack<SourceLocationMetadata, ScopeId>,
        children: Vec<OperationOrGroup>,
    },
}

pub(crate) trait OperationOrGroupExt {
    type Scope: PartialEq + std::fmt::Display + std::fmt::Debug + Clone + Default;
    type SourceLocation: PartialEq + Clone + Sized;
    type DbgStuff<'a>: DbgStuffExt<SourceLocation = Self::SourceLocation, Scope = Self::Scope>;

    fn group(
        scope_stack: ScopeStack<Self::SourceLocation, Self::Scope>,
        children: Vec<Self>,
    ) -> Self
    where
        Self: std::marker::Sized;
    fn scope_stack_if_group(&self) -> Option<&ScopeStack<Self::SourceLocation, Self::Scope>>;

    #[allow(dead_code)]
    fn name(
        &self,
        dbg_stuff: &impl DbgStuffExt<SourceLocation = Self::SourceLocation, Scope = Self::Scope>,
    ) -> String;

    fn children_mut(&mut self) -> Option<&mut Vec<Self>>
    where
        Self: std::marker::Sized;

    fn set_location(&mut self, location: PackageOffset);

    fn all_qubits(&self) -> Vec<QubitWire>;
    fn all_results(&self) -> Vec<ResultWire>;
    fn extend_target_qubits(&mut self, target_qubits: &[QubitWire]);
    fn extend_target_results(&mut self, target_results: &[ResultWire]);

    fn into_operation(
        self,
        dbg_stuff: &Self::DbgStuff<'_>,
        scope_resolver: Option<&impl ScopeResolver<ScopeId = Self::Scope>>,
    ) -> Operation;
}

impl OperationOrGroupExt for OperationOrGroup {
    type Scope = ScopeId;
    type SourceLocation = SourceLocationMetadata;
    type DbgStuff<'a> = DbgStuffForEval;

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

    fn group(
        scope_stack: ScopeStack<Self::SourceLocation, Self::Scope>,
        children: Vec<Self>,
    ) -> Self {
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

        Self {
            kind: OperationOrGroupKind::Group {
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
                source: None,
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

    fn scope_stack_if_group(&self) -> Option<&ScopeStack<Self::SourceLocation, Self::Scope>> {
        if let OperationOrGroupKind::Group { scope_stack, .. } = &self.kind {
            Some(scope_stack)
        } else {
            None
        }
    }

    fn name(
        &self,
        dbg_stuff: &impl DbgStuffExt<SourceLocation = Self::SourceLocation, Scope = Self::Scope>,
    ) -> String {
        match &self.kind {
            OperationOrGroupKind::Single => self.op.gate(),
            OperationOrGroupKind::Group { scope_stack, .. } => scope_stack.fmt(dbg_stuff),
        }
    }

    fn set_location(&mut self, location: PackageOffset) {
        self.op
            .source_mut()
            .replace(SourceLocation::Unresolved(location));
    }

    fn into_operation(
        mut self,
        dbg_stuff: &Self::DbgStuff<'_>,
        scope_resolver: Option<&impl ScopeResolver<ScopeId = Self::Scope>>,
    ) -> Operation {
        match self.kind {
            OperationOrGroupKind::Single => self.op,
            OperationOrGroupKind::Group {
                scope_stack,
                children,
            } => {
                if let Some(scope_resolver) = scope_resolver {
                    let label = scope_stack.resolve_scope(scope_resolver).name();
                    *self.op.gate_mut() = label;
                    let scope_location = scope_stack.resolve_scope(scope_resolver).location();
                    *self.op.source_mut() = Some(SourceLocation::Unresolved(scope_location));
                }
                *self.op.children_mut() = vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(|o| o.into_operation(dbg_stuff, scope_resolver))
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
    source_locations: bool,
    user_package_ids: Vec<PackageId>,
    group_scopes: bool,
}

impl OperationListBuilder {
    pub fn new(
        max_operations: usize,
        user_package_ids: Vec<PackageId>,
        group_scopes: bool,
    ) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
            source_locations: true,
            user_package_ids,
            group_scopes,
        }
    }

    fn push_op(
        &mut self,
        op: OperationOrGroup,
        unfiltered_call_stack: Vec<SourceLocationMetadata>,
    ) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        add_op_with_grouping(
            self.source_locations,
            self.group_scopes,
            &self.user_package_ids,
            &DbgStuffForEval {},
            &mut self.operations,
            op,
            unfiltered_call_stack,
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
        call_stack: &[SourceLocationMetadata],
    ) {
        self.push_op(
            Self::new_unitary(wire_map, name, is_adjoint, inputs, args),
            call_stack.to_vec(),
        );
    }

    fn m(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        call_stack: &[SourceLocationMetadata],
    ) {
        self.push_op(
            Self::new_measurement("M", wire_map, qubit, result),
            call_stack.to_vec(),
        );
    }

    fn mresetz(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        call_stack: &[SourceLocationMetadata],
    ) {
        self.push_op(
            Self::new_measurement("MResetZ", wire_map, qubit, result),
            call_stack.to_vec(),
        );
        self.push_op(Self::new_ket(wire_map, qubit), call_stack.to_vec());
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, call_stack: &[SourceLocationMetadata]) {
        self.push_op(Self::new_ket(wire_map, qubit), call_stack.to_vec());
    }

    fn new_unitary(
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs<'_>,
        args: Vec<String>,
    ) -> OperationOrGroup {
        OperationOrGroup::single(Operation::Unitary(Unitary {
            gate: name.to_string(),
            args,
            children: vec![],
            targets: inputs
                .targets
                .iter()
                .map(|q| Register {
                    qubit: wire_map.qubit_wire(*q).0,
                    result: None,
                })
                .collect(),
            controls: inputs
                .control_qubits
                .iter()
                .map(|q| Register {
                    qubit: wire_map.qubit_wire(*q).0,
                    result: None,
                })
                .collect(),
            is_adjoint,
            source: None,
        }))
    }

    fn new_measurement(
        label: &str,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
    ) -> OperationOrGroup {
        let result_wire = wire_map.result_wire(result);

        OperationOrGroup::single(Operation::Measurement(Measurement {
            gate: label.to_string(),
            args: vec![],
            children: vec![],
            qubits: vec![Register {
                qubit: wire_map.qubit_wire(qubit).0,
                result: None,
            }],
            results: vec![Register {
                qubit: result_wire.0,
                result: Some(result_wire.1),
            }],
            source: None,
        }))
    }

    fn new_ket(wire_map: &WireMap, qubit: usize) -> OperationOrGroup {
        OperationOrGroup::single(Operation::Ket(Ket {
            gate: "0".to_string(),
            args: vec![],
            children: vec![],
            targets: vec![Register {
                qubit: wire_map.qubit_wire(qubit).0,
                result: None,
            }],
            source: None,
        }))
    }
}

pub(crate) struct GateInputs<'a> {
    pub(crate) targets: &'a [usize],
    pub(crate) control_qubits: &'a [usize],
    pub(crate) control_results: &'a [usize],
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub(crate) struct ScopeId(StoreItemId);

impl Default for ScopeId {
    fn default() -> Self {
        ScopeId(StoreItemId {
            package: usize::MAX.into(),
            item: usize::MAX.into(),
        })
    }
}

impl Display for ScopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // map integers to letters, like 0->A, 1->B, ..., 25->Z, 26->AA, etc.
        let mut n = usize::from(self.0.item);
        let mut letters = String::new();
        loop {
            let rem = n % 26;
            letters.push((b'A' + u8::try_from(rem).expect("n % 26 should fit in u8")) as char);
            n /= 26;
            if n == 0 {
                break;
            }
            n -= 1; // adjust for 0-based indexing
        }
        let rev_letters: String = letters.chars().rev().collect();
        write!(f, "{rev_letters}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceLocationMetadata {
    location: PackageOffset,
    scope_id: ScopeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LexicalScope {
    Top,
    Named {
        name: Rc<str>,
        location: PackageOffset,
    },
}

impl LexicalScope {
    pub(crate) fn top() -> Self {
        LexicalScope::Top
    }

    pub(crate) fn location(&self) -> PackageOffset {
        match self {
            // TODO: handle Top case properly
            LexicalScope::Top => PackageOffset {
                package_id: PackageId::CORE.successor().successor(),
                offset: 0,
            },
            LexicalScope::Named { location, .. } => *location,
        }
    }

    pub(crate) fn name(&self) -> String {
        match self {
            LexicalScope::Top => "Top".to_string(),
            LexicalScope::Named { name, .. } => name.to_string(),
        }
    }
}

pub(crate) fn add_op_with_grouping<OG: OperationOrGroupExt>(
    source_locations: bool,
    group_scopes: bool,
    user_package_ids: &[PackageId],
    dbg_stuff: &OG::DbgStuff<'_>,
    operations: &mut Vec<OG>,
    mut op: OG,
    unfiltered_call_stack: Vec<OG::SourceLocation>,
) {
    let op_call_stack = if group_scopes || source_locations {
        retain_user_frames(dbg_stuff, user_package_ids, unfiltered_call_stack)
    } else {
        vec![]
    };

    if source_locations && let Some(called_at) = op_call_stack.last() {
        op.set_location(dbg_stuff.source_location(called_at));
    }

    let op_call_stack = if group_scopes { op_call_stack } else { vec![] };

    // TODO: I'm pretty sure this is wrong if we have a NO call stack operation
    // in between call-stacked operations. We should probably unscope those. Add tests.

    add_scoped_op(
        dbg_stuff,
        operations,
        &ScopeStack::top(),
        op,
        &op_call_stack,
    );
}

fn add_scoped_op<OG: OperationOrGroupExt>(
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
            if let Some(last_scope_stack_abs) = last_op.scope_stack_if_group()
                && dbg_stuff
                    .strip_scope_stack_prefix(op_call_stack, last_scope_stack_abs)
                    .is_some()
            {
                let last_scope_stack_abs = last_scope_stack_abs.clone();

                // The last scope matched, add to it
                last_op.extend_target_qubits(&op.all_qubits());
                last_op.extend_target_results(&op.all_results());
                let last_op_children = last_op.children_mut().expect("operation should be a group");

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

fn retain_user_frames<SourceLocation>(
    dbg_stuff: &impl DbgStuffExt<SourceLocation = SourceLocation>,
    user_package_ids: &[PackageId],
    mut location_stack: Vec<SourceLocation>,
) -> Vec<SourceLocation> {
    location_stack.retain(|location| {
        let package_id = dbg_stuff.package_id(location);
        user_package_ids.is_empty() || user_package_ids.contains(&package_id)
    });

    location_stack
}

impl ScopeResolver for fir::PackageStore {
    type ScopeId = ScopeId;

    fn resolve_scope(&self, scope_id: &Self::ScopeId) -> LexicalScope {
        let item = self.get_item(scope_id.0);
        let (scope_offset, scope_name) = match &item.kind {
            fir::ItemKind::Callable(callable_decl) => match &callable_decl.implementation {
                fir::CallableImpl::Intrinsic => {
                    panic!("intrinsic callables should not be in the stack")
                }
                fir::CallableImpl::Spec(spec_impl) => {
                    (spec_impl.body.span.lo, callable_decl.name.name.clone())
                } // TODO: other specializations?
                fir::CallableImpl::SimulatableIntrinsic(_) => {
                    panic!("simulatable intrinsic callables should not be in the stack")
                }
            },
            _ => panic!("only callables should be in the stack"),
        };

        LexicalScope::Named {
            location: PackageOffset {
                package_id: scope_id.0.package,
                offset: scope_offset,
            },
            name: scope_name,
        }
    }
}

struct DbgStuffForEval {}

impl DbgStuffExt for DbgStuffForEval {
    type SourceLocation = SourceLocationMetadata;
    type Scope = ScopeId;

    fn lexical_scope(&self, location: &Self::SourceLocation) -> Self::Scope {
        location.scope_id
    }

    fn package_id(&self, location: &Self::SourceLocation) -> PackageId {
        location.location.package_id
    }

    fn source_location(&self, location: &Self::SourceLocation) -> PackageOffset {
        location.location
    }
}
