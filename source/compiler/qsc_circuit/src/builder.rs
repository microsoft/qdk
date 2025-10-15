// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    Config, Qubit,
    circuit::{Circuit, operation_list_to_grid},
    group_qubits,
    rir_to_circuit::{
        DbgLocationKind, Op, fill_in_dbg_metadata, resolve_location_if_unresolved,
        resolve_location_metadata, resolve_source_location_if_unresolved, to_source_location,
        tracer::{BlockBuilder, GateLabel, ResultRegister, WireId},
    },
};
use qsc_data_structures::{index_map::IndexMap, line_column::Encoding, span::Span};
use qsc_eval::{
    backend::{self, GateInputs, Tracer},
    val::{self, Value},
};
use qsc_fir::fir::PackageId;
use qsc_frontend::compile::PackageStore;
use qsc_partial_eval::rir::{
    self, DbgInfo, DbgMetadataScope, InstructionMetadata, MetadataPackageSpan,
};
use std::{fmt::Write, mem::replace, rc::Rc};

/// Backend implementation that builds a circuit representation.
pub struct CircuitBuilder {
    config: Config,
    register_map_builder: RegisterMapBuilder,
    block_builder: BlockBuilder,
    dbg_info: rir::DbgInfo,
}

impl Tracer for CircuitBuilder {
    fn gate(
        &mut self,
        name: &str,
        is_adjoint: bool,
        GateInputs {
            target_qubits,
            control_qubits,
        }: GateInputs,
        args: Vec<String>,
        metadata: Option<backend::DebugMetadata>,
    ) {
        let metadata = self.convert_if_source_locations_enabled(metadata);
        self.block_builder.gate(
            self.register_map_builder.current(),
            &GateLabel { name, is_adjoint },
            GateInputs {
                target_qubits,
                control_qubits,
            },
            &[],
            args,
            metadata,
        );
    }

    fn m(&mut self, q: usize, val: &val::Result, metadata: Option<backend::DebugMetadata>) {
        let metadata = self.convert_if_source_locations_enabled(metadata);
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.register_map_builder.result_allocate(),
        };
        self.register_map_builder.link_result_to_qubit(q, r);
        self.block_builder
            .m(self.register_map_builder.current(), q, r, metadata);
    }

    fn mresetz(&mut self, q: usize, val: &val::Result, metadata: Option<backend::DebugMetadata>) {
        let metadata = self.convert_if_source_locations_enabled(metadata);
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.register_map_builder.result_allocate(),
        };
        self.register_map_builder.link_result_to_qubit(q, r);
        self.block_builder
            .mresetz(self.register_map_builder.current(), q, r, metadata);
    }

    fn reset(&mut self, q: usize, metadata: Option<backend::DebugMetadata>) {
        let metadata = self.convert_if_source_locations_enabled(metadata);
        self.block_builder
            .reset(self.register_map_builder.current(), q, metadata);
    }

    fn qubit_allocate(&mut self, q: usize, metadata: Option<backend::DebugMetadata>) {
        let metadata = self.convert_if_source_locations_enabled(metadata);
        self.register_map_builder.map_qubit(q, metadata);
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize, _metadata: Option<backend::DebugMetadata>) {
        // TODO: metadata would be neat to add to the circuit
        self.register_map_builder.swap(q0, q1);
    }

    fn custom_intrinsic(
        &mut self,
        name: &str,
        arg: Value,
        metadata: Option<backend::DebugMetadata>,
    ) {
        // The qubit arguments are treated as the targets for custom gates.
        // Any remaining arguments will be kept in the display_args field
        // to be shown as part of the gate label when the circuit is rendered.
        let (qubit_args, classical_args) = self.split_qubit_args(arg);

        if qubit_args.is_empty() {
            // don't add a gate with no qubit targets
            return;
        }

        let metadata = metadata.map(|md| self.convert_metadata(&md));

        self.block_builder.gate(
            self.register_map_builder.current(),
            &GateLabel {
                name,
                is_adjoint: false,
            },
            GateInputs {
                target_qubits: qubit_args,
                control_qubits: vec![],
            },
            &[],
            if classical_args.is_empty() {
                vec![]
            } else {
                vec![classical_args]
            },
            metadata,
        );
    }

    fn qubit_release(&mut self, q: usize, _metadata: Option<backend::DebugMetadata>) {
        // TODO: metadata would be neat to add to the circuit
        self.register_map_builder.qubit_release(q);
    }
}

impl CircuitBuilder {
    #[must_use]
    pub fn new(config: Config) -> Self {
        CircuitBuilder {
            config,
            register_map_builder: RegisterMapBuilder::default(),
            block_builder: BlockBuilder::new(config.max_operations),
            dbg_info: DbgInfo {
                dbg_metadata_scopes: vec![DbgMetadataScope::SubProgram {
                    name: "program".into(),
                    location: MetadataPackageSpan {
                        package: 2,                  // TODO: - pass in user package ofc
                        span: Span { lo: 0, hi: 0 }, // pass in whole program or whatever
                    },
                }],

                ..Default::default()
            },
        }
    }

    #[must_use]
    pub fn snapshot(&self, dbg_lookup: Option<(&PackageStore, Encoding)>) -> Circuit {
        self.finish_circuit(self.block_builder.operations(), dbg_lookup)
    }

    #[must_use]
    pub fn finish(mut self, dbg_lookup: Option<(&PackageStore, Encoding)>) -> Circuit {
        let ops = replace(
            &mut self.block_builder,
            BlockBuilder::new(self.config.max_operations),
        )
        .into_operations();

        self.finish_circuit(&ops, dbg_lookup)
    }

    fn finish_circuit(
        &self,
        operations: &[Op],
        dbg_lookup: Option<(&PackageStore, Encoding)>,
    ) -> Circuit {
        finish_circuit(
            self.register_map_builder.register_map.to_qubits(),
            operations.to_vec(),
            &self.dbg_info,
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
    fn push_val(&mut self, arg: &Value, qubits: &mut Vec<usize>, classical_args: &mut String) {
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
        &mut self,
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
    fn push_vals(&mut self, vals: &[Value], qubits: &mut Vec<usize>, classical_args: &mut String) {
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

    fn push_dbg_location(&mut self, md: &backend::DebugMetadata) -> Option<usize> {
        let mut user_frame: Option<(PackageId, Span)> = None;
        let mut grab_span_and_stop = false;
        for frame in md.stack.iter().rev() {
            if grab_span_and_stop {
                if let Some(c) = user_frame {
                    user_frame = Some((c.0, frame.span));
                }
                break;
            }
            let caller_package = frame.caller;
            let instr_span = frame.span;

            // TODO: don't hardcode stdlib
            if caller_package != PackageId::CORE && caller_package != 1.into() {
                grab_span_and_stop = true;
            }
            user_frame.replace((caller_package, instr_span));
        }

        let (package, span) = user_frame?;

        let location = rir::MetadataPackageSpan {
            package: u32::try_from(usize::from(package)).expect("package id should fit in u32"),
            span,
        };
        let md = rir::DbgLocation {
            location,
            scope: 0, // TODO: fill in correct scope
            inlined_at: None,
        };
        self.dbg_info.dbg_locations.push(md);
        Some(self.dbg_info.dbg_locations.len() - 1)
    }

    fn convert_metadata(&mut self, metadata: &backend::DebugMetadata) -> rir::InstructionMetadata {
        let dbg_location = self.push_dbg_location(metadata);

        rir::InstructionMetadata { dbg_location }
    }

    fn convert_if_source_locations_enabled(
        &mut self,
        metadata: Option<backend::DebugMetadata>,
    ) -> Option<InstructionMetadata> {
        if !self.config.locations {
            return None;
        }
        metadata.map(|md| self.convert_metadata(&md))
    }
}

pub(crate) fn finish_circuit(
    mut qubits: Vec<(Qubit, Vec<DbgLocationKind>)>,
    mut operations: Vec<Op>,
    dbg_info: &DbgInfo,
    dbg_lookup: Option<(&PackageStore, Encoding)>,
    loop_detection: bool,
    collapse_qubit_registers: bool,
) -> Circuit {
    resolve_location_metadata(&mut operations, dbg_info);

    for (q, declarations) in &mut qubits {
        for d in declarations.iter_mut() {
            resolve_location_if_unresolved(dbg_info, d);
        }

        if !declarations.is_empty() {
            q.declarations = Some(declarations.iter().filter_map(to_source_location).collect());
        }
    }

    let mut qubits = qubits.into_iter().map(|(q, _)| q).collect::<Vec<_>>();

    let mut operations = operations
        .iter()
        .map(|o| o.clone().into())
        .collect::<Vec<_>>();

    if let Some((package_store, position_encoding)) = dbg_lookup {
        fill_in_dbg_metadata(&mut operations, package_store, position_encoding);

        for q in &mut qubits {
            if let Some(declarations) = &mut q.declarations {
                for source_location in declarations {
                    resolve_source_location_if_unresolved(
                        source_location,
                        package_store,
                        position_encoding,
                    );
                }
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
// Really similar to source/compiler/qsc_partial_eval/src/management.rs
pub(crate) struct RegisterMapBuilder {
    next_meas_id: usize, // ResultType
    next_qubit_wire_id: WireId,
    register_map: RegisterMap,
}

#[derive(Default)]
pub(crate) struct RegisterMap {
    qubit_map: IndexMap<usize, WireId>, // QubitType -> WireId
    qubit_measurements: IndexMap<WireId, (Vec<DbgLocationKind>, Vec<usize>)>, // WireId -> Vec<ResultType>
}

impl Default for RegisterMapBuilder {
    fn default() -> Self {
        Self {
            next_meas_id: 0,
            next_qubit_wire_id: WireId(0),
            register_map: RegisterMap::default(),
        }
    }
}

impl RegisterMap {
    pub(crate) fn qubit_register(&self, qubit_id: usize) -> WireId {
        self.qubit_map
            .get(qubit_id)
            .expect("qubit should already be mapped")
            .to_owned()
    }

    pub(crate) fn result_register(&self, result_id: usize) -> ResultRegister {
        self.qubit_measurements
            .iter()
            .find_map(|(WireId(qubit_register), (_, results))| {
                let r_idx = results.iter().position(|&r| r == result_id);
                r_idx.map(|r_idx| ResultRegister(qubit_register, r_idx))
            })
            .expect("result should already be mapped")
    }

    pub fn to_qubits(&self) -> Vec<(Qubit, Vec<DbgLocationKind>)> {
        let mut qubits = vec![];
        for (WireId(i), (declarations, rs)) in self.qubit_measurements.iter() {
            let num_results = rs.len();
            qubits.push((
                Qubit {
                    id: i,
                    num_results,
                    declarations: None,
                },
                declarations.clone(),
            ));
        }

        qubits
    }
}

impl RegisterMapBuilder {
    pub(crate) fn current(&self) -> &RegisterMap {
        &self.register_map
    }

    pub fn map_qubit(&mut self, qubit: usize, metadata: Option<InstructionMetadata>) {
        let location = metadata
            .and_then(|md| md.dbg_location)
            .map(DbgLocationKind::Unresolved);
        let mapped = self.next_qubit_wire_id;
        self.next_qubit_wire_id.0 += 1;
        self.register_map.qubit_map.insert(qubit, mapped);

        if let Some(q) = self.register_map.qubit_measurements.get_mut(mapped) {
            if let Some(location) = location {
                q.0.push(location);
            }
        } else {
            let l = location.map(|l| vec![l]).unwrap_or_default();
            self.register_map
                .qubit_measurements
                .insert(mapped, (l, vec![]));
        }
    }

    fn qubit_release(&mut self, q: usize) {
        // let curr = self.register_map.qubit_map.get(q);
        self.next_qubit_wire_id.0 -= 1;

        // can't rely on this because arrays are released in the order of allocation :/
        // assert_eq!(
        //     curr,
        //     Some(&(self.next_qubit_wire_id)),
        //     "can only release the most recently allocated qubit"
        // );
        self.register_map.qubit_map.remove(q);
    }

    pub fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        let mapped_q = self.register_map.qubit_register(q);
        let Some((_, measurements)) = self.register_map.qubit_measurements.get_mut(mapped_q) else {
            panic!("qubit should already be mapped");
        };
        if !measurements.contains(&r) {
            measurements.push(r);
        }
    }

    fn result_allocate(&mut self) -> usize {
        let id = self.next_meas_id;
        self.next_meas_id += 1;
        id
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        let q0_mapped = self.register_map.qubit_register(q0);
        let q1_mapped = self.register_map.qubit_register(q1);
        self.register_map.qubit_map.insert(q0, q1_mapped);
        self.register_map.qubit_map.insert(q1, q0_mapped);
    }

    pub(crate) fn into_register_map(self) -> RegisterMap {
        self.register_map
    }
}
