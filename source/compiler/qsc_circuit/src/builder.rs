// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    Config,
    circuit::{Circuit, Ket, Measurement, Operation, Register, Unitary, operation_list_to_grid},
};
use num_bigint::BigUint;
use num_complex::Complex;
use qsc_data_structures::index_map::IndexMap;
use qsc_eval::{
    backend::Backend,
    val::{self, Value},
};
use std::{fmt::Write, mem::take, rc::Rc};

/// Backend implementation that builds a circuit representation.
pub struct Builder {
    max_ops_exceeded: bool,
    operations: Vec<Operation>,
    config: Config,
    remapper: Remapper,
}

impl Backend for Builder {
    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) {
        let ctl0 = self.map(ctl0);
        let ctl1 = self.map(ctl1);
        let q = self.map(q);
        self.push_gate(controlled_gate("X", [ctl0, ctl1], [q]));
    }

    fn cx(&mut self, ctl: usize, q: usize) {
        let ctl = self.map(ctl);
        let q = self.map(q);
        self.push_gate(controlled_gate("X", [ctl], [q]));
    }

    fn cy(&mut self, ctl: usize, q: usize) {
        let ctl = self.map(ctl);
        let q = self.map(q);
        self.push_gate(controlled_gate("Y", [ctl], [q]));
    }

    fn cz(&mut self, ctl: usize, q: usize) {
        let ctl = self.map(ctl);
        let q = self.map(q);
        self.push_gate(controlled_gate("Z", [ctl], [q]));
    }

    fn h(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("H", [q]));
    }

    fn m(&mut self, q: usize) -> val::Result {
        let mapped_q = self.map(q);
        // In the Circuit schema, result id is per-qubit
        let res_id = self.num_measurements_for_qubit(mapped_q);
        let id = self.remapper.m(q);

        self.push_gate(measurement_gate(mapped_q.0, res_id));
        id.into()
    }

    fn mresetz(&mut self, q: usize) -> val::Result {
        let mapped_q = self.map(q);
        // In the Circuit schema, result id is per-qubit
        let res_id = self.num_measurements_for_qubit(mapped_q);
        // We don't actually need the Remapper since we're not
        // remapping any qubits, but it's handy for keeping track of measurements
        let id = self.remapper.m(q);

        // Ideally MResetZ would be atomic but we don't currently have
        // a way to visually represent that. So decompose it into
        // a measurement and a reset gate.
        self.push_gate(measurement_gate(mapped_q.0, res_id));
        self.push_gate(ket_gate("0", [mapped_q]));
        id.into()
    }

    fn reset(&mut self, q: usize) {
        let mapped_q = self.map(q);
        self.push_gate(ket_gate("0", [mapped_q]));
    }

    fn rx(&mut self, theta: f64, q: usize) {
        let q = self.map(q);
        self.push_gate(rotation_gate("Rx", theta, [q]));
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        self.push_gate(rotation_gate("Rxx", theta, [q0, q1]));
    }

    fn ry(&mut self, theta: f64, q: usize) {
        let q = self.map(q);
        self.push_gate(rotation_gate("Ry", theta, [q]));
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        self.push_gate(rotation_gate("Ryy", theta, [q0, q1]));
    }

    fn rz(&mut self, theta: f64, q: usize) {
        let q = self.map(q);
        self.push_gate(rotation_gate("Rz", theta, [q]));
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        self.push_gate(rotation_gate("Rzz", theta, [q0, q1]));
    }

    fn sadj(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(adjoint_gate("S", [q]));
    }

    fn s(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("S", [q]));
    }

    fn sx(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("SX", [q]));
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        self.push_gate(gate("SWAP", [q0, q1]));
    }

    fn tadj(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(adjoint_gate("T", [q]));
    }

    fn t(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("T", [q]));
    }

    fn x(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("X", [q]));
    }

    fn y(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("Y", [q]));
    }

    fn z(&mut self, q: usize) {
        let q = self.map(q);
        self.push_gate(gate("Z", [q]));
    }

    fn qubit_allocate(&mut self) -> usize {
        self.remapper.qubit_allocate()
    }

    fn qubit_release(&mut self, q: usize) -> bool {
        self.remapper.qubit_release(q);
        true
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) {
        self.remapper.swap(q0, q1);
    }

    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        (Vec::new(), 0)
    }

    fn qubit_is_zero(&mut self, _q: usize) -> bool {
        // We don't simulate quantum execution here. So we don't know if the qubit
        // is zero or not. Returning true avoids potential panics.
        true
    }

    fn custom_intrinsic(&mut self, name: &str, arg: Value) -> Option<Result<Value, String>> {
        // The qubit arguments are treated as the targets for custom gates.
        // Any remaining arguments will be kept in the display_args field
        // to be shown as part of the gate label when the circuit is rendered.
        let (qubit_args, classical_args) = self.split_qubit_args(arg);

        self.push_gate(custom_gate(
            name,
            &qubit_args,
            if classical_args.is_empty() {
                vec![]
            } else {
                vec![classical_args]
            },
        ));

        match name {
            // Special case this known intrinsic to match the simulator
            // behavior, so that our samples will work
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            _ => Some(Ok(Value::unit())),
        }
    }
}

impl Builder {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Builder {
            max_ops_exceeded: false,
            operations: vec![],
            config,
            remapper: Remapper::default(),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Circuit {
        let operations = self.operations.clone();
        self.finish_circuit(operations)
    }

    #[must_use]
    pub fn finish(mut self) -> Circuit {
        let operations = take(&mut self.operations);
        self.finish_circuit(operations)
    }

    fn map(&mut self, qubit: usize) -> WireId {
        self.remapper.map(qubit)
    }

    fn push_gate(&mut self, gate: Operation) {
        if self.max_ops_exceeded || self.operations.len() >= self.config.max_operations {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }
        self.operations.push(gate);
    }

    fn num_measurements_for_qubit(&self, qubit: WireId) -> usize {
        self.remapper
            .qubit_measurement_counts
            .get(qubit)
            .copied()
            .unwrap_or_default()
    }

    fn finish_circuit(&self, operations: Vec<Operation>) -> Circuit {
        let mut qubits = vec![];

        // add qubit declarations
        for i in 0..self.remapper.num_qubits() {
            let num_measurements = self.num_measurements_for_qubit(WireId(i));
            qubits.push(crate::circuit::Qubit {
                id: i,
                num_results: num_measurements,
            });
        }

        Circuit {
            component_grid: operation_list_to_grid(operations, qubits.len()),
            qubits,
        }
    }

    /// Splits the qubit arguments from classical arguments so that the qubits
    /// can be treated as the targets for custom gates.
    /// The classical arguments get formatted into a comma-separated list.
    fn split_qubit_args(&mut self, arg: Value) -> (Vec<WireId>, String) {
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
    fn push_val(&mut self, arg: &Value, qubits: &mut Vec<WireId>, classical_args: &mut String) {
        match arg {
            Value::Array(vals) => {
                self.push_list::<'[', ']'>(vals, qubits, classical_args);
            }
            Value::Tuple(vals, _) => {
                self.push_list::<'(', ')'>(vals, qubits, classical_args);
            }
            Value::Qubit(q) => {
                qubits.push(self.map(q.deref().0));
            }
            v => {
                let _ = write!(classical_args, "{v}");
            }
        }
        qubits.sort_unstable_by_key(|q| q.0);
        qubits.dedup_by_key(|q| q.0);
    }

    /// Pushes all qubit values into `qubits`, and formats all
    /// classical values into `classical_args` as a list.
    fn push_list<const OPEN: char, const CLOSE: char>(
        &mut self,
        vals: &[Value],
        qubits: &mut Vec<WireId>,
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
    fn push_vals(&mut self, vals: &[Value], qubits: &mut Vec<WireId>, classical_args: &mut String) {
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
#[derive(Default)]
struct Remapper {
    next_meas_id: usize,
    next_qubit_id: usize,
    next_qubit_wire_id: WireId,
    qubit_map: IndexMap<usize, WireId>,
    qubit_measurement_counts: IndexMap<WireId, usize>,
}

impl Remapper {
    fn map(&mut self, qubit: usize) -> WireId {
        if let Some(mapped) = self.qubit_map.get(qubit) {
            *mapped
        } else {
            let mapped = self.next_qubit_wire_id;
            self.next_qubit_wire_id.0 += 1;
            self.qubit_map.insert(qubit, mapped);
            mapped
        }
    }

    fn m(&mut self, q: usize) -> usize {
        let mapped_q = self.map(q);
        let id = self.get_meas_id();
        match self.qubit_measurement_counts.get_mut(mapped_q) {
            Some(count) => *count += 1,
            None => {
                self.qubit_measurement_counts.insert(mapped_q, 1);
            }
        }
        id
    }

    fn qubit_allocate(&mut self) -> usize {
        let id = self.next_qubit_id;
        self.next_qubit_id += 1;
        let _ = self.map(id);
        id
    }

    fn qubit_release(&mut self, _q: usize) {
        self.next_qubit_id -= 1;
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        let q0_mapped = self.map(q0);
        let q1_mapped = self.map(q1);
        self.qubit_map.insert(q0, q1_mapped);
        self.qubit_map.insert(q1, q0_mapped);
    }

    #[must_use]
    fn num_qubits(&self) -> usize {
        self.next_qubit_wire_id.0
    }

    #[must_use]
    fn get_meas_id(&mut self) -> usize {
        let id = self.next_meas_id;
        self.next_meas_id += 1;
        id
    }
}

#[derive(Copy, Clone, Default)]
struct WireId(pub usize);

impl From<usize> for WireId {
    fn from(id: usize) -> Self {
        WireId(id)
    }
}

impl From<WireId> for usize {
    fn from(id: WireId) -> Self {
        id.0
    }
}

fn gate<const N: usize>(name: &str, targets: [WireId; N]) -> Operation {
    Operation::Unitary(Unitary {
        gate: name.into(),
        args: vec![],
        is_adjoint: false,
        controls: vec![],
        targets: targets.iter().map(|q| Register::quantum(q.0)).collect(),
        children: vec![],
    })
}

fn adjoint_gate<const N: usize>(name: &str, targets: [WireId; N]) -> Operation {
    Operation::Unitary(Unitary {
        gate: name.into(),
        args: vec![],
        is_adjoint: true,
        controls: vec![],
        targets: targets.iter().map(|q| Register::quantum(q.0)).collect(),
        children: vec![],
    })
}

fn controlled_gate<const M: usize, const N: usize>(
    name: &str,
    controls: [WireId; M],
    targets: [WireId; N],
) -> Operation {
    Operation::Unitary(Unitary {
        gate: name.into(),
        args: vec![],
        is_adjoint: false,
        controls: controls.iter().map(|q| Register::quantum(q.0)).collect(),
        targets: targets.iter().map(|q| Register::quantum(q.0)).collect(),
        children: vec![],
    })
}

fn measurement_gate(qubit: usize, result: usize) -> Operation {
    Operation::Measurement(Measurement {
        gate: "Measure".into(),
        args: vec![],
        qubits: vec![Register::quantum(qubit)],
        results: vec![Register::classical(qubit, result)],
        children: vec![],
    })
}

fn ket_gate<const N: usize>(name: &str, targets: [WireId; N]) -> Operation {
    Operation::Ket(Ket {
        gate: name.into(),
        args: vec![],
        targets: targets.iter().map(|q| Register::quantum(q.0)).collect(),
        children: vec![],
    })
}

fn rotation_gate<const N: usize>(name: &str, theta: f64, targets: [WireId; N]) -> Operation {
    Operation::Unitary(Unitary {
        gate: name.into(),
        args: vec![format!("{theta:.4}")],
        is_adjoint: false,
        controls: vec![],
        targets: targets.iter().map(|q| Register::quantum(q.0)).collect(),
        children: vec![],
    })
}

fn custom_gate(name: &str, targets: &[WireId], args: Vec<String>) -> Operation {
    Operation::Unitary(Unitary {
        gate: name.into(),
        args,
        is_adjoint: false,
        controls: vec![],
        targets: targets.iter().map(|q| Register::quantum(q.0)).collect(),
        children: vec![],
    })
}
