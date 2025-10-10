// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    Config, Qubit,
    circuit::{Circuit, Operation, operation_list_to_grid},
    rir_to_circuit::tracer::{BlockBuilder, GateInputs, QubitRegister, ResultRegister},
};
use qsc_data_structures::index_map::IndexMap;
use qsc_eval::{
    backend::Tracer,
    val::{self, Value},
};
use std::{fmt::Write, mem::replace, rc::Rc};

/// Backend implementation that builds a circuit representation.
pub struct CircuitBuilder {
    config: Config,
    register_map_builder: RegisterMapBuilder,
    block_builder: BlockBuilder,
}

impl Tracer for CircuitBuilder {
    fn gate(
        &mut self,
        name: &str,
        is_adjoint: bool,
        target_qubits: Vec<usize>,
        control_qubits: Vec<usize>,
        control_results: Vec<usize>,
        args: Vec<String>,
    ) {
        self.block_builder.gate(
            self.register_map_builder.current(),
            name,
            is_adjoint,
            GateInputs {
                target_qubits,
                control_qubits,
                control_results,
            },
            args,
            None,
        );
    }

    fn m(&mut self, q: usize, val: &val::Result) {
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.register_map_builder.result_allocate(),
        };
        self.register_map_builder.link_result_to_qubit(q, r);
        self.block_builder
            .m(self.register_map_builder.current(), q, r, None);
    }

    fn mresetz(&mut self, q: usize, val: &val::Result) {
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.register_map_builder.result_allocate(),
        };
        self.register_map_builder.link_result_to_qubit(q, r);
        self.block_builder
            .mresetz(self.register_map_builder.current(), q, r, None);
    }

    fn reset(&mut self, q: usize) {
        self.block_builder
            .reset(self.register_map_builder.current(), q, None);
    }

    fn qubit_allocate(&mut self, q: usize) {
        self.register_map_builder.map_qubit(q);
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) {
        self.register_map_builder.swap(q0, q1);
    }

    fn custom_intrinsic(&mut self, name: &str, arg: Value) {
        // The qubit arguments are treated as the targets for custom gates.
        // Any remaining arguments will be kept in the display_args field
        // to be shown as part of the gate label when the circuit is rendered.
        let (qubit_args, classical_args) = self.split_qubit_args(arg);

        self.block_builder.gate(
            self.register_map_builder.current(),
            name,
            false,
            GateInputs {
                target_qubits: qubit_args,
                control_qubits: vec![],
                control_results: vec![],
            },
            if classical_args.is_empty() {
                vec![]
            } else {
                vec![classical_args]
            },
            None,
        );
    }

    fn qubit_release(&mut self, q: usize) {
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
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Circuit {
        let mut operations = vec![];
        operations.extend(
            self.block_builder
                .operations()
                .iter()
                .map(|op| op.clone().into()),
        );
        self.finish_circuit(&operations)
    }

    #[must_use]
    pub fn finish(mut self) -> Circuit {
        let ops = replace(
            &mut self.block_builder,
            BlockBuilder::new(self.config.max_operations),
        )
        .into_operations();

        self.finish_circuit(
            ops.iter()
                .map(|o| o.clone().into())
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    fn finish_circuit(&self, operations: &[Operation]) -> Circuit {
        let qubits = self.register_map_builder.to_qubits();

        Circuit {
            component_grid: operation_list_to_grid(
                operations.to_vec(),
                &qubits,
                self.config.loop_detection,
            ),
            qubits,
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
