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
    operations::QubitParamInfo,
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
use qsc_fir::fir::{self, PackageId, PackageStoreLookup};
use qsc_frontend::compile::PackageStore;
use qsc_lowerer::map_fir_package_to_hir;
use rustc_hash::FxHashSet;
use std::{fmt::Write, mem::replace, rc::Rc};

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
            &GateInputs { targets, controls },
            display_args,
            stack,
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
            self.circuit_builder
                .mresetz(self.wire_map_builder.current(), q, r, stack);
        } else {
            self.circuit_builder
                .m(self.wire_map_builder.current(), q, r, stack);
        }
    }

    fn reset(&mut self, stack: &[Frame], q: usize) {
        self.circuit_builder
            .reset(self.wire_map_builder.current(), q, stack);
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
            stack,
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
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
        }
    }

    #[must_use]
    pub fn with_qubit_input_params(
        config: TracerConfig,
        user_package_ids: &[PackageId],
        operation_qubit_params: Option<(PackageId, QubitParamInfo)>,
    ) -> Self {
        // Pre-initialize the qubit declaration locations for the operation's
        // input parameters. These will get allocated during execution, but
        // the declaration locations inferred from the callstacks will be meaningless
        // since those will be in the generated entry expression.
        let params = operation_qubit_params
            .map(|(package_id, info)| {
                let mut decls = vec![];
                for param in &info.qubit_params {
                    for _ in 0..param.elements {
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
            ),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
        }
    }

    #[must_use]
    pub fn snapshot(
        &self,
        source_lookup: Option<&PackageStore>,
        fir_package_store: Option<&fir::PackageStore>,
    ) -> Circuit {
        self.finish_circuit(
            self.circuit_builder.operations(),
            source_lookup,
            fir_package_store,
        )
    }

    #[must_use]
    pub fn finish(
        mut self,
        source_lookup: Option<&PackageStore>,
        fir_package_store: Option<&fir::PackageStore>,
    ) -> Circuit {
        let ops = replace(
            &mut self.circuit_builder,
            OperationListBuilder::new(self.config.max_operations, self.user_package_ids.clone()),
        )
        .into_operations();

        self.finish_circuit(&ops, source_lookup, fir_package_store)
    }

    fn finish_circuit(
        &self,
        operations: &[OperationWithCallStack],
        dbg_lookup: Option<&PackageStore>,
        fir_package_store: Option<&fir::PackageStore>,
    ) -> Circuit {
        let operations = if self.config.group_scopes
            && let Some(fir_package_store) = fir_package_store
        {
            // This has to take `Op` since it still contains logical stacks from dbg metadata
            self.circuit_builder
                .group_operations(fir_package_store, operations.to_vec())
        } else {
            operations.iter().map(|o| o.0.clone()).collect()
        };

        finish_circuit(
            self.wire_map_builder.current(),
            operations,
            dbg_lookup,
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
    source_location_lookup: Option<&PackageStore>,
    loop_detection: bool,
    collapse_qubit_registers: bool,
) -> Circuit {
    let mut qubits = wire_map.to_qubits();

    if let Some(source_location_lookup) = source_location_lookup {
        resolve_locations(&mut operations, source_location_lookup);

        for q in &mut qubits {
            for source_location in &mut q.declarations {
                resolve_source_location_if_unresolved(source_location, source_location_lookup);
            }
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

fn resolve_locations(operations: &mut [Operation], source_location_lookup: &PackageStore) {
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
    package_store: &PackageStore,
) {
    match source_location {
        SourceLocation::Resolved(_) => {}
        SourceLocation::Unresolved(package_offset) => {
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

            *source_location = SourceLocation::Resolved(ResolvedSourceLocation {
                file: source.name.to_string(),
                line: pos.line,
                column: pos.column,
            });
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
struct OperationWithScopeStack {
    kind: OperationWithScopeStackKind,
    op: Operation,
}

impl OperationWithScopeStack {
    fn into_operation(mut self) -> Operation {
        match self.kind {
            OperationWithScopeStackKind::Single => self.op,
            OperationWithScopeStackKind::Group {
                scope_span,
                scope_stack: _,
                children,
            } => {
                *self.op.children_mut() = vec![ComponentColumn {
                    components: children
                        .into_iter()
                        .map(OperationWithScopeStack::into_operation)
                        .collect(),
                }];
                *self.op.source_mut() = scope_span.map(SourceLocation::Unresolved);
                self.op
            }
        }
    }

    fn target_qubits(&self) -> Vec<QubitWire> {
        match &self.op {
            Operation::Measurement(_) => vec![],
            Operation::Unitary(unitary) => {
                unitary.targets.iter().map(|r| QubitWire(r.qubit)).collect()
            }
            Operation::Ket(ket) => ket.targets.iter().map(|r| QubitWire(r.qubit)).collect(),
        }
    }

    fn target_results(&self) -> Vec<ResultWire> {
        match &self.op {
            Operation::Measurement(measurement) => measurement
                .results
                .iter()
                .filter_map(|r| r.result.map(|res| ResultWire(r.qubit, res)))
                .collect(),
            Operation::Unitary(_) | Operation::Ket(_) => vec![],
        }
    }
}

#[derive(Clone, Debug)]
enum OperationWithScopeStackKind {
    Single,
    Group {
        scope_stack: Option<ScopeStack>,
        scope_span: Option<PackageOffset>, // TODO: can this be derived?
        children: Vec<OperationWithScopeStack>,
    },
}

#[derive(Clone, Debug)]
struct OperationWithCallStack(Operation, Vec<Frame>);

/// Builds a list of circuit operations with a maximum operation limit.
/// Stops adding operations once the limit is exceeded.
///
/// Methods take `WireMap` as a parameter to resolve qubit and result IDs
/// to their corresponding wire positions in the circuit diagram.
pub(crate) struct OperationListBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations: Vec<OperationWithCallStack>,
    source_locations: bool,
    user_package_ids: Vec<PackageId>,
}

impl OperationListBuilder {
    pub fn new(max_operations: usize, user_package_ids: Vec<PackageId>) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
            source_locations: true,
            user_package_ids,
        }
    }

    fn push_op(&mut self, op: Operation, stack: &[Frame]) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        self.operations
            .push(OperationWithCallStack(op, stack.to_vec()));
    }

    fn operations(&self) -> &Vec<OperationWithCallStack> {
        &self.operations
    }

    fn into_operations(self) -> Vec<OperationWithCallStack> {
        self.operations
    }

    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        stack: &[Frame],
    ) {
        let called_at = self.user_code_call_location(stack);
        self.push_op(
            Operation::Unitary(Unitary {
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
                    .controls
                    .iter()
                    .map(|q| Register {
                        qubit: wire_map.qubit_wire(*q).0,
                        result: None,
                    })
                    .collect(),
                is_adjoint,
                source: called_at.map(SourceLocation::Unresolved),
            }),
            stack,
        );
    }

    fn m(&mut self, wire_map: &WireMap, qubit: usize, result: usize, stack: &[Frame]) {
        let called_at = self.user_code_call_location(stack);
        let result_wire = wire_map.result_wire(result);

        self.push_op(
            Operation::Measurement(Measurement {
                gate: "M".to_string(),
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
                source: called_at.map(SourceLocation::Unresolved),
            }),
            stack,
        );
    }

    fn mresetz(&mut self, wire_map: &WireMap, qubit: usize, result: usize, stack: &[Frame]) {
        let called_at = self.user_code_call_location(stack);
        let qubits: Vec<Register> = vec![Register {
            qubit: wire_map.qubit_wire(qubit).0,
            result: None,
        }];

        let resul_wire = wire_map.result_wire(result);
        let results = vec![Register {
            qubit: resul_wire.0,
            result: Some(resul_wire.1),
        }];

        self.push_op(
            Operation::Measurement(Measurement {
                gate: "MResetZ".to_string(),
                args: vec![],
                children: vec![],
                qubits: qubits.clone(),
                results,
                source: called_at.map(SourceLocation::Unresolved),
            }),
            stack,
        );

        self.push_op(
            Operation::Ket(Ket {
                gate: "0".to_string(),
                args: vec![],
                children: vec![],
                targets: qubits,
                source: called_at.map(SourceLocation::Unresolved),
            }),
            stack,
        );
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, stack: &[Frame]) {
        let called_at = self.user_code_call_location(stack);
        self.push_op(
            Operation::Ket(Ket {
                gate: "0".to_string(),
                args: vec![],
                children: vec![],
                targets: vec![Register {
                    qubit: wire_map.qubit_wire(qubit).0,
                    result: None,
                }],
                source: called_at.map(SourceLocation::Unresolved),
            }),
            stack,
        );
    }

    fn group_operations(
        &self,
        fir_package_store: &fir::PackageStore,
        in_operations: Vec<OperationWithCallStack>,
    ) -> Vec<Operation> {
        let mut operations = vec![];
        for op in in_operations {
            let instruction_stack = self.instruction_logical_stack(fir_package_store, op.1);

            add_op(&mut operations, op.0, instruction_stack.as_ref());
        }
        operations
            .into_iter()
            .map(OperationWithScopeStack::into_operation)
            .collect()
    }

    fn user_code_call_location(&self, stack: &[Frame]) -> Option<PackageOffset> {
        if !self.source_locations || stack.is_empty() || self.user_package_ids.is_empty() {
            return None;
        }
        first_user_code_location(&self.user_package_ids, stack)
    }

    fn instruction_logical_stack(
        &self,
        fir_package_store: &fir::PackageStore,
        stack: Vec<Frame>,
    ) -> Option<InstructionStack> {
        let filtered = stack
            .into_iter()
            .filter_map(|frame| {
                if self.user_package_ids.contains(&frame.id.package) {
                    let item = fir_package_store.get_item(frame.id);
                    let (scope_offset, scope_name) = match &item.kind {
                        fir::ItemKind::Callable(callable_decl) => match &callable_decl
                            .implementation
                        {
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

                    let package_offset = PackageOffset {
                        package_id: frame.id.package,
                        offset: frame.span.lo,
                    };
                    let scope = LexicalScope {
                        location: PackageOffset {
                            package_id: frame.id.package,
                            offset: scope_offset,
                        },
                        name: scope_name,
                    };
                    Some((package_offset, scope))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if filtered.is_empty() {
            return None;
        }
        Some(InstructionStack(filtered))
    }
}

pub(crate) struct GateInputs<'a> {
    targets: &'a [usize],
    controls: &'a [usize],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InstructionStack(Vec<(PackageOffset, LexicalScope)>); // Can be empty

impl InstructionStack {
    fn scope_stack(&self) -> Option<ScopeStack> {
        self.0.split_last().map(|(top, prefix)| ScopeStack {
            caller: InstructionStack(prefix.to_vec()),
            scope: top.1.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ScopeStack {
    caller: InstructionStack,
    scope: LexicalScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LexicalScope {
    name: Rc<str>,
    location: PackageOffset,
    // item_id: StoreItemId,
}

fn add_op(
    block_operations: &mut Vec<OperationWithScopeStack>,
    op: Operation,
    instruction_stack: Option<&InstructionStack>,
) {
    let target_qubits = all_qubits(&op);
    let target_results = all_results(&op);
    let op_wrapper = OperationWithScopeStack {
        kind: OperationWithScopeStackKind::Single,
        op,
    };
    match instruction_stack {
        Some(instruction_stack) => {
            add_scoped_op(
                block_operations,
                None,
                op_wrapper,
                instruction_stack,
                target_qubits,
                target_results,
            );
        }
        None => block_operations.push(op_wrapper),
    }
}

fn all_qubits(op: &Operation) -> Vec<QubitWire> {
    let qubits: FxHashSet<QubitWire> = match &op {
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

fn all_results(op: &Operation) -> Vec<ResultWire> {
    let results: FxHashSet<ResultWire> = match &op {
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

#[allow(clippy::too_many_lines)]
fn add_scoped_op(
    current_scope_container: &mut Vec<OperationWithScopeStack>,
    current_scope: Option<ScopeStack>,
    op: OperationWithScopeStack,
    instruction_stack: &InstructionStack,
    target_qubits: Vec<QubitWire>,
    target_results: Vec<ResultWire>,
) {
    let full_instruction_stack = concat_stacks(current_scope.as_ref(), instruction_stack);
    let scope_stack = instruction_stack.scope_stack();

    if let Some(scope_stack) = scope_stack
        && Some(&scope_stack) != current_scope.as_ref()
    {
        // there is a scope
        if let Some(last_op) = current_scope_container.last_mut() {
            if let OperationWithScopeStackKind::Group {
                children: last_scope_children,
                scope_stack: Some(last_scope_stack),
                scope_span: _,
            } = &mut last_op.kind
            {
                if let Some(rest) = strip_stack_prefix(&full_instruction_stack, last_scope_stack) {
                    {
                        let target_qubits: &[QubitWire] = &target_qubits;
                        match &mut last_op.op {
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
                    };
                    {
                        let target_results: &[ResultWire] = &target_results;
                        match &mut last_op.op {
                            Operation::Measurement(measurement) => {
                                measurement.results.extend(target_results.iter().map(|r| {
                                    Register {
                                        qubit: r.0,
                                        result: Some(r.1),
                                    }
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
                    };

                    // Recursively add to the children
                    add_scoped_op(
                        last_scope_children,
                        Some(last_scope_stack.clone()),
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
        let scope_metadata = make_scope_metadata(&scope_stack);
        let label = scope_label(&scope_stack);
        let full_scope_stack = full_instruction_stack
            .scope_stack()
            .expect("we got here because we had a scope, so what the hell is this");

        if current_scope != Some(full_scope_stack.clone()) {
            let scope_group = OperationWithScopeStack {
                kind: OperationWithScopeStackKind::Group {
                    scope_stack: Some(full_scope_stack),
                    scope_span: Some(scope_metadata),
                    children: vec![op],
                },
                op: Operation::Unitary(Unitary {
                    gate: label,
                    args: vec![],
                    children: vec![],
                    targets: target_qubits
                        .iter()
                        .map(|q| Register {
                            qubit: q.0,
                            result: None,
                        })
                        .collect(),
                    controls: vec![],
                    is_adjoint: false,
                    source: None,
                }),
            };

            // create container for the prefix, and add to it
            add_scoped_op(
                current_scope_container,
                current_scope,
                scope_group.clone(),
                &scope_stack.caller,
                scope_group.target_qubits(),
                scope_group.target_results(),
            );
            return;
        }
    }
    // no scope, top level, just push to current operations
    current_scope_container.push(op);
}

fn concat_stacks(scope: Option<&ScopeStack>, tail: &InstructionStack) -> InstructionStack {
    match scope {
        Some(prefix) => {
            if let Some(first) = tail.0.first() {
                assert_eq!(
                    first.1, prefix.scope,
                    "concatenating stacks that don't seem to match"
                );
            }
            InstructionStack([prefix.caller.0.clone(), tail.0.clone()].concat())
        }
        None => tail.clone(),
    }
}

fn strip_stack_prefix(full: &InstructionStack, prefix: &ScopeStack) -> Option<InstructionStack> {
    if full.0.len() > prefix.caller.0.len() {
        if let Some(rest) = full.0.strip_prefix(prefix.caller.0.as_slice()) {
            let next_location = &rest[0];
            let next_scope = &next_location.1;
            if next_scope == &prefix.scope {
                return Some(InstructionStack(rest.to_vec()));
            }
        }
    }
    None
}

fn make_scope_metadata(scope_stack: &ScopeStack) -> PackageOffset {
    let scope_location = &scope_stack.scope;
    scope_location.location
}

fn scope_label(scope_stack: &ScopeStack) -> String {
    scope_name(&scope_stack.scope)
}

fn scope_name(scope_id: &LexicalScope) -> String {
    scope_id.name.to_string()
}
