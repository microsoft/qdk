// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    circuit::{
        Circuit, Ket, Measurement, Operation, PackageOffset, Qubit, Register,
        ResolvedSourceLocation, SourceLocation, Unitary, operation_list_to_grid,
    },
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
use qsc_fir::fir::PackageId;
use qsc_frontend::compile::PackageStore;
use qsc_lowerer::map_fir_package_to_hir;
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
        let called_at = self.user_code_call_location(stack);
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

    fn measure(&mut self, stack: &[Frame], name: &str, q: usize, val: &val::Result) {
        let called_at = self.user_code_call_location(stack);
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

    fn reset(&mut self, stack: &[Frame], q: usize) {
        let called_at = self.user_code_call_location(stack);
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
            self.user_code_call_location(stack),
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
            circuit_builder: OperationListBuilder::new(config.max_operations),
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
            circuit_builder: OperationListBuilder::new(config.max_operations),
            next_result_id: 0,
            user_package_ids: user_package_ids.to_vec(),
        }
    }

    #[must_use]
    pub fn snapshot(&self, source_lookup: Option<&PackageStore>) -> Circuit {
        self.finish_circuit(self.circuit_builder.operations().clone(), source_lookup)
    }

    #[must_use]
    pub fn finish(mut self, source_lookup: Option<&PackageStore>) -> Circuit {
        let ops = replace(
            &mut self.circuit_builder,
            OperationListBuilder::new(self.config.max_operations),
        )
        .into_operations();

        self.finish_circuit(ops, source_lookup)
    }

    fn finish_circuit(
        &self,
        operations: Vec<Operation>,
        dbg_lookup: Option<&PackageStore>,
    ) -> Circuit {
        finish_circuit(
            self.wire_map_builder.wire_map.to_qubits(),
            operations,
            dbg_lookup,
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

fn finish_circuit(
    mut qubits: Vec<Qubit>,
    mut operations: Vec<Operation>,
    source_location_lookup: Option<&PackageStore>,
) -> Circuit {
    if let Some(source_location_lookup) = source_location_lookup {
        resolve_locations(&mut operations, source_location_lookup);

        for q in &mut qubits {
            for source_location in &mut q.declarations {
                resolve_source_location_if_unresolved(source_location, source_location_lookup);
            }
        }
    }

    let component_grid = operation_list_to_grid(operations, qubits.len());
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

fn resolve_source_location_if_unresolved(
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

#[derive(Clone, Debug, Copy)]
pub struct TracerConfig {
    /// Maximum number of operations the builder will add to the circuit
    pub max_operations: usize,
    /// Capture the source code locations of operations and qubit declarations
    /// in the circuit diagram
    pub source_locations: bool,
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

    fn to_qubits(&self) -> Vec<Qubit> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResultWire(pub usize, pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QubitWire(pub usize);

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

/// Builds a list of circuit operations with a maximum operation limit.
/// Stops adding operations once the limit is exceeded.
///
/// Methods take `WireMap` as a parameter to resolve qubit and result IDs
/// to their corresponding wire positions in the circuit diagram.
struct OperationListBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations: Vec<Operation>,
}

impl OperationListBuilder {
    fn new(max_operations: usize) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
        }
    }

    fn push_op(&mut self, op: Operation) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        self.operations.push(op);
    }

    fn operations(&self) -> &Vec<Operation> {
        &self.operations
    }

    fn into_operations(self) -> Vec<Operation> {
        self.operations
    }

    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        called_at: Option<PackageOffset>,
    ) {
        self.push_op(Operation::Unitary(Unitary {
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
        }));
    }

    fn m(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        called_at: Option<PackageOffset>,
    ) {
        let result_wire = wire_map.result_wire(result);

        self.push_op(Operation::Measurement(Measurement {
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
        }));
    }

    fn mresetz(
        &mut self,
        wire_map: &WireMap,
        qubit: usize,
        result: usize,
        called_at: Option<PackageOffset>,
    ) {
        let qubits: Vec<Register> = vec![Register {
            qubit: wire_map.qubit_wire(qubit).0,
            result: None,
        }];

        let resul_wire = wire_map.result_wire(result);
        let results = vec![Register {
            qubit: resul_wire.0,
            result: Some(resul_wire.1),
        }];

        self.push_op(Operation::Measurement(Measurement {
            gate: "MResetZ".to_string(),
            args: vec![],
            children: vec![],
            qubits: qubits.clone(),
            results,
            source: called_at.map(SourceLocation::Unresolved),
        }));

        self.push_op(Operation::Ket(Ket {
            gate: "0".to_string(),
            args: vec![],
            children: vec![],
            targets: qubits,
            source: called_at.map(SourceLocation::Unresolved),
        }));
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, called_at: Option<PackageOffset>) {
        self.push_op(Operation::Ket(Ket {
            gate: "0".to_string(),
            args: vec![],
            children: vec![],
            targets: vec![Register {
                qubit: wire_map.qubit_wire(qubit).0,
                result: None,
            }],
            source: called_at.map(SourceLocation::Unresolved),
        }));
    }
}

struct GateInputs<'a> {
    targets: &'a [usize],
    controls: &'a [usize],
}
