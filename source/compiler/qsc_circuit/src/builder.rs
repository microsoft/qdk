// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    Config, Qubit,
    circuit::{Circuit, Operation, operation_list_to_grid},
    rir_to_circuit::tracer::{
        BlockBuilder, GateInputs, QubitRegister, RegisterMap, ResultRegister,
    },
};
use qsc_data_structures::index_map::IndexMap;
use qsc_eval::{
    backend::Tracer,
    val::{self, Value},
};
use std::{fmt::Write, mem::replace, rc::Rc};

/// Backend implementation that builds a circuit representation.
pub struct Builder {
    config: Config,
    remapper: Remapper,
    tracer: BlockBuilder,
}

impl Tracer for Builder {
    fn gate(
        &mut self,
        name: &str,
        is_adjoint: bool,
        target_qubits: Vec<usize>,
        control_qubits: Vec<usize>,
        control_results: Vec<usize>,
        args: Vec<String>,
    ) {
        self.tracer.gate(
            &self.remapper,
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
            val::Result::Loss | val::Result::Val(_) => self.remapper.result_allocate(),
        };
        self.remapper.link_result_to_qubit(q, r);
        self.tracer.m(&self.remapper, q, r, None);
    }

    fn mresetz(&mut self, q: usize, val: &val::Result) {
        let r = match val {
            val::Result::Id(id) => *id,
            val::Result::Loss | val::Result::Val(_) => self.remapper.result_allocate(),
        };
        self.remapper.link_result_to_qubit(q, r);
        self.tracer.mresetz(&self.remapper, q, r, None);
    }

    fn reset(&mut self, q: usize) {
        self.tracer.reset(&self.remapper, q, None);
    }

    fn qubit_allocate(&mut self, q: usize) {
        self.remapper.map_qubit(q);
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) {
        self.remapper.swap(q0, q1);
    }

    fn custom_intrinsic(&mut self, name: &str, arg: Value) {
        // The qubit arguments are treated as the targets for custom gates.
        // Any remaining arguments will be kept in the display_args field
        // to be shown as part of the gate label when the circuit is rendered.
        let (qubit_args, classical_args) = self.split_qubit_args(arg);

        self.tracer.gate(
            &self.remapper,
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
}

impl Builder {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Builder {
            config,
            remapper: Remapper::default(),
            tracer: BlockBuilder::new(config.max_operations),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Circuit {
        let mut operations = vec![];
        operations.extend(self.tracer.operations().iter().map(|op| op.clone().into()));
        self.finish_circuit(&operations)
    }

    #[must_use]
    pub fn finish(mut self) -> Circuit {
        let ops = replace(
            &mut self.tracer,
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
        let qubits = self.remapper.to_qubits();

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

/// Provides support for qubit id allocation, measurement and
/// reset operations for Base Profile targets.
///
/// Since qubit reuse is disallowed, a mapping is maintained
/// from allocated qubit ids to hardware qubit ids. Each time
/// a qubit is reset, it is remapped to a fresh hardware qubit.
///
/// Note that even though qubit reset & reuse is disallowed,
/// qubit ids are still reused for new allocations.
/// Measurements are tracked and deferred.
pub(crate) struct Remapper {
    next_meas_id: usize, // ResultType
    next_qubit_wire_id: QubitRegister,
    qubit_map: IndexMap<usize, QubitRegister>, // QubitType -> QubitRegister
    qubit_measurements: IndexMap<QubitRegister, Vec<usize>>, // QubitRegister -> Vec<ResultType>
}

impl Default for Remapper {
    fn default() -> Self {
        Self {
            next_meas_id: 0,
            next_qubit_wire_id: QubitRegister(0),
            qubit_map: IndexMap::new(),
            qubit_measurements: IndexMap::new(),
        }
    }
}

impl Remapper {
    pub fn map_qubit(&mut self, qubit: usize) {
        let mapped = self.next_qubit_wire_id;
        self.next_qubit_wire_id.0 += 1;
        self.qubit_map.insert(qubit, mapped);
    }

    pub fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        let mapped_q = self.qubit_register(q);
        let v = if let Some(v) = self.qubit_measurements.get_mut(mapped_q) {
            v
        } else {
            self.qubit_measurements.insert(mapped_q, vec![]);
            self.qubit_measurements.get_mut(mapped_q).expect("")
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
        let q0_mapped = self.qubit_register(q0);
        let q1_mapped = self.qubit_register(q1);
        self.qubit_map.insert(q0, q1_mapped);
        self.qubit_map.insert(q1, q0_mapped);
    }

    #[must_use]
    fn num_qubits(&self) -> usize {
        self.next_qubit_wire_id.0
    }

    fn num_measurements_for_qubit(&self, qubit: QubitRegister) -> usize {
        self.qubit_measurements
            .get(qubit)
            .map(Vec::len)
            .unwrap_or_default()
    }

    pub fn into_qubits(self) -> Vec<Qubit> {
        let mut qubits = vec![];

        // add qubit declarations
        for i in 0..self.num_qubits() {
            let num_measurements = self.num_measurements_for_qubit(QubitRegister(i));
            qubits.push(Qubit {
                id: i,
                num_results: num_measurements,
            });
        }

        qubits
    }

    fn to_qubits(&self) -> Vec<Qubit> {
        let mut qubits = vec![];

        // add qubit declarations
        for i in 0..self.num_qubits() {
            let num_measurements = self.num_measurements_for_qubit(QubitRegister(i));
            qubits.push(Qubit {
                id: i,
                num_results: num_measurements,
            });
        }
        qubits
    }
}

impl RegisterMap for Remapper {
    type ResultType = usize;
    type QubitType = usize;

    fn qubit_register(&self, qubit_id: Self::QubitType) -> QubitRegister {
        self.qubit_map
            .get(qubit_id)
            .expect("qubit should already be mapped")
            .to_owned()
    }

    fn result_register(&self, result_id: Self::ResultType) -> ResultRegister {
        self.qubit_measurements
            .iter()
            .find_map(|(QubitRegister(qubit_register), results)| {
                let r_idx = results.iter().position(|&r| r == result_id);
                r_idx.map(|r_idx| ResultRegister(qubit_register, r_idx))
            })
            .expect("result should already be mapped")
    }
}
