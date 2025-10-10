// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    Config, Qubit,
    circuit::{Circuit, operation_list_to_grid},
    rir_to_circuit::{
        Op, fill_in_dbg_metadata,
        tracer::{BlockBuilder, QubitRegister, ResultRegister},
    },
};
use qsc_data_structures::{index_map::IndexMap, line_column::Encoding, span::Span};
use qsc_eval::{
    backend::{self, GateInputs, Tracer},
    val::{self, Value},
};
use qsc_frontend::compile::PackageStore;
use qsc_partial_eval::rir::{self, DbgInfo, DbgMetadataScope, MetadataPackageSpan};
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
        metadata: Option<backend::InstructionMetadata>,
    ) {
        let metadata = metadata.map(|md| self.convert_metadata(&md));

        self.block_builder.gate(
            self.register_map_builder.current(),
            name,
            is_adjoint,
            GateInputs {
                target_qubits,
                control_qubits,
            },
            vec![],
            args,
            metadata,
        );
    }

    fn m(&mut self, q: usize, val: &val::Result, metadata: Option<backend::InstructionMetadata>) {
        let metadata = metadata.map(|md| self.convert_metadata(&md));
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.register_map_builder.result_allocate(),
        };
        self.register_map_builder.link_result_to_qubit(q, r);
        self.block_builder
            .m(self.register_map_builder.current(), q, r, metadata);
    }

    fn mresetz(
        &mut self,
        q: usize,
        val: &val::Result,
        metadata: Option<backend::InstructionMetadata>,
    ) {
        let metadata = metadata.map(|md| self.convert_metadata(&md));
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.register_map_builder.result_allocate(),
        };
        self.register_map_builder.link_result_to_qubit(q, r);
        self.block_builder
            .mresetz(self.register_map_builder.current(), q, r, metadata);
    }

    fn reset(&mut self, q: usize, metadata: Option<backend::InstructionMetadata>) {
        let metadata = metadata.map(|md| self.convert_metadata(&md));
        self.block_builder
            .reset(self.register_map_builder.current(), q, metadata);
    }

    fn qubit_allocate(&mut self, q: usize, _metadata: Option<backend::InstructionMetadata>) {
        // TODO: metadata would be neat to add to the circuit
        self.register_map_builder.map_qubit(q);
    }

    fn qubit_swap_id(
        &mut self,
        q0: usize,
        q1: usize,
        _metadata: Option<backend::InstructionMetadata>,
    ) {
        // TODO: metadata would be neat to add to the circuit
        self.register_map_builder.swap(q0, q1);
    }

    fn custom_intrinsic(
        &mut self,
        name: &str,
        arg: Value,
        metadata: Option<backend::InstructionMetadata>,
    ) {
        // The qubit arguments are treated as the targets for custom gates.
        // Any remaining arguments will be kept in the display_args field
        // to be shown as part of the gate label when the circuit is rendered.
        let (qubit_args, classical_args) = self.split_qubit_args(arg);

        let metadata = metadata.map(|md| self.convert_metadata(&md));

        self.block_builder.gate(
            self.register_map_builder.current(),
            name,
            false,
            GateInputs {
                target_qubits: qubit_args,
                control_qubits: vec![],
            },
            vec![],
            if classical_args.is_empty() {
                vec![]
            } else {
                vec![classical_args]
            },
            metadata,
        );
    }

    fn qubit_release(&mut self, q: usize, _metadata: Option<backend::InstructionMetadata>) {
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
        let qubits = self.register_map_builder.to_qubits();
        let mut operations = operations.to_vec();

        if let Some((package_store, position_encoding)) = dbg_lookup {
            fill_in_dbg_metadata(
                &self.dbg_info,
                &mut operations,
                package_store,
                position_encoding,
            );
        }

        let component_grid = operation_list_to_grid(
            operations
                .iter()
                .map(|o| o.clone().into())
                .collect::<Vec<_>>(),
            &qubits,
            self.config.loop_detection,
        );
        Circuit {
            qubits,
            component_grid,
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

    fn push_dbg_location(&mut self, md: &backend::InstructionMetadata) -> usize {
        let md = rir::DbgLocation {
            location: rir::MetadataPackageSpan {
                package: u32::try_from(usize::from(md.location.package))
                    .expect("package id should fit in u32"),
                span: md.location.span,
            },
            scope: 0, // TODO: fill in correct scope
            inlined_at: None,
        };
        self.dbg_info.dbg_locations.push(md);
        self.dbg_info.dbg_locations.len() - 1
    }

    fn convert_metadata(
        &mut self,
        metadata: &backend::InstructionMetadata,
    ) -> rir::InstructionMetadata {
        let dbg_location = self.push_dbg_location(metadata);

        rir::InstructionMetadata {
            dbg_location: Some(dbg_location),
        }
    }
}
// Really similar to source/compiler/qsc_partial_eval/src/management.rs
pub(crate) struct RegisterMapBuilder {
    next_meas_id: usize, // ResultType
    next_qubit_wire_id: QubitRegister,
    register_map: RegisterMap,
}

#[derive(Default)]
pub(crate) struct RegisterMap {
    qubit_map: IndexMap<usize, QubitRegister>, // QubitType -> QubitRegister
    qubit_measurements: IndexMap<QubitRegister, Vec<usize>>, // QubitRegister -> Vec<ResultType>
}

impl Default for RegisterMapBuilder {
    fn default() -> Self {
        Self {
            next_meas_id: 0,
            next_qubit_wire_id: QubitRegister(0),
            register_map: RegisterMap::default(),
        }
    }
}

impl RegisterMap {
    pub(crate) fn qubit_register(&self, qubit_id: usize) -> QubitRegister {
        self.qubit_map
            .get(qubit_id)
            .expect("qubit should already be mapped")
            .to_owned()
    }

    pub(crate) fn result_register(&self, result_id: usize) -> ResultRegister {
        self.qubit_measurements
            .iter()
            .find_map(|(QubitRegister(qubit_register), results)| {
                let r_idx = results.iter().position(|&r| r == result_id);
                r_idx.map(|r_idx| ResultRegister(qubit_register, r_idx))
            })
            .expect("result should already be mapped")
    }

    pub fn into_qubits(self) -> Vec<Qubit> {
        let mut qubits = vec![];

        for (QubitRegister(i), rs) in self.qubit_measurements.iter() {
            let num_results = rs.len();
            qubits.push(Qubit { id: i, num_results });
        }

        qubits
    }
}

impl RegisterMapBuilder {
    pub(crate) fn current(&self) -> &RegisterMap {
        &self.register_map
    }

    pub fn map_qubit(&mut self, qubit: usize) {
        let mapped = self.next_qubit_wire_id;
        self.next_qubit_wire_id.0 += 1;
        self.register_map.qubit_map.insert(qubit, mapped);

        if self.register_map.qubit_measurements.get(mapped).is_none() {
            self.register_map.qubit_measurements.insert(mapped, vec![]);
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
        let Some(v) = self.register_map.qubit_measurements.get_mut(mapped_q) else {
            panic!("qubit should already be mapped");
        };
        if !v.contains(&r) {
            v.push(r);
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

    fn to_qubits(&self) -> Vec<Qubit> {
        let mut qubits = vec![];
        // add qubit declarations
        for (QubitRegister(i), rs) in self.register_map.qubit_measurements.iter() {
            let num_results = rs.len();
            qubits.push(Qubit { id: i, num_results });
        }

        qubits
    }

    pub(crate) fn into_register_map(self) -> RegisterMap {
        self.register_map
    }
}
