// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc::{
    Backend, BackendResult,
    interpret::{self, GenericReceiver, Interpreter, Value},
};
use rand::RngExt;
use rustc_hash::FxHashMap;

use crate::{Trace, instruction_ids};

#[derive(Default)]
pub struct TraceBuilder {
    trace: Trace,
    qubit_id_map: FxHashMap<usize, usize>,
    post_select_measurements: FxHashMap<usize, bool>,
    repeat_frames: Vec<RepeatFrame>,
    free_list: Vec<usize>,
    next_free: usize,
    live_qubits: usize,
    max_live_qubits: usize,
}

enum PendingOperation {
    Gate {
        id: u64,
        qubits: Vec<u64>,
        params: Vec<f64>,
    },
    Block {
        repetitions: u64,
        operations: Vec<PendingOperation>,
    },
}

#[derive(Default)]
struct RepeatFrame {
    repetitions: u64,
    operations: Vec<PendingOperation>,
}

impl TraceBuilder {
    #[must_use]
    pub fn into_trace(mut self) -> Trace {
        self.trace.set_compute_qubits(self.max_live_qubits as u64);
        self.trace
    }

    fn on_allocate(&mut self, q: usize) {
        self.live_qubits += 1;
        self.max_live_qubits = self.max_live_qubits.max(self.live_qubits);

        // Keep compute qubit count conservative while building and exact on finalize.
        self.trace.set_compute_qubits(self.max_live_qubits as u64);

        // Ensure qubit indices are stable within the trace for readability.
        if q >= self.next_free {
            self.next_free = q + 1;
        }
    }

    fn map_qubit(&self, q: usize) -> u64 {
        self.qubit_id_map.get(&q).copied().unwrap_or(q) as u64
    }

    fn push_operation(&mut self, op: PendingOperation) {
        if let Some(frame) = self.repeat_frames.last_mut() {
            frame.operations.push(op);
        } else {
            Self::append_operation_to_block(self.trace.root_block_mut(), op);
        }
    }

    fn append_operation_to_block(block: &mut crate::Block, op: PendingOperation) {
        match op {
            PendingOperation::Gate { id, qubits, params } => {
                block.add_operation(id, qubits, params);
            }
            PendingOperation::Block {
                repetitions,
                operations,
            } => {
                let child = block.add_block(repetitions);
                for op in operations {
                    Self::append_operation_to_block(child, op);
                }
            }
        }
    }

    fn push_gate(&mut self, id: u64, qubits: Vec<u64>, params: Vec<f64>) {
        self.push_operation(PendingOperation::Gate { id, qubits, params });
    }

    fn load(&mut self, q: usize) {
        self.push_gate(
            instruction_ids::READ_FROM_MEMORY,
            vec![self.map_qubit(q), self.map_qubit(q)],
            vec![],
        );
    }

    fn store(&mut self, q: usize) {
        self.push_gate(
            instruction_ids::WRITE_TO_MEMORY,
            vec![self.map_qubit(q), self.map_qubit(q)],
            vec![],
        );
    }

    fn measurement_result(&mut self, q: usize) -> bool {
        self.post_select_measurements
            .remove(&q)
            .unwrap_or_else(|| rand::rng().random_bool(0.5))
    }
}

impl Backend for TraceBuilder {
    fn x(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::PAULI_X, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn cx(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::CX,
            vec![self.map_qubit(ctl), self.map_qubit(q)],
            vec![],
        );
        Ok(())
    }

    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::CCX,
            vec![
                self.map_qubit(ctl0),
                self.map_qubit(ctl1),
                self.map_qubit(q),
            ],
            vec![],
        );
        Ok(())
    }

    fn m(&mut self, q: usize) -> Result<BackendResult, String> {
        let val = self.measurement_result(q);
        self.push_gate(instruction_ids::MEAS_Z, vec![self.map_qubit(q)], vec![]);
        Ok(val.into())
    }

    fn mresetz(&mut self, q: usize) -> Result<BackendResult, String> {
        let val = self.measurement_result(q);
        self.push_gate(
            instruction_ids::MEAS_RESET_Z,
            vec![self.map_qubit(q)],
            vec![],
        );
        Ok(val.into())
    }

    fn reset(&mut self, _q: usize) -> Result<(), String> {
        Ok(())
    }

    fn qubit_allocate(&mut self) -> Result<usize, String> {
        let q = self.free_list.pop().unwrap_or(self.next_free);
        self.on_allocate(q);
        self.qubit_id_map.insert(q, q);
        Ok(q)
    }

    fn qubit_release(&mut self, q: usize) -> Result<bool, String> {
        self.live_qubits = self.live_qubits.saturating_sub(1);
        let q = self.qubit_id_map.remove(&q).unwrap_or(q);
        self.free_list.push(q);
        Ok(true)
    }

    fn h(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::H, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn cy(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::CY,
            vec![self.map_qubit(ctl), self.map_qubit(q)],
            vec![],
        );
        Ok(())
    }

    fn cz(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::CZ,
            vec![self.map_qubit(ctl), self.map_qubit(q)],
            vec![],
        );
        Ok(())
    }

    fn rx(&mut self, theta: f64, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::RX, vec![self.map_qubit(q)], vec![theta]);
        Ok(())
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::RXX,
            vec![self.map_qubit(q0), self.map_qubit(q1)],
            vec![theta],
        );
        Ok(())
    }

    fn ry(&mut self, theta: f64, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::RY, vec![self.map_qubit(q)], vec![theta]);
        Ok(())
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::RYY,
            vec![self.map_qubit(q0), self.map_qubit(q1)],
            vec![theta],
        );
        Ok(())
    }

    fn rz(&mut self, theta: f64, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::RZ, vec![self.map_qubit(q)], vec![theta]);
        Ok(())
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::RZZ,
            vec![self.map_qubit(q0), self.map_qubit(q1)],
            vec![theta],
        );
        Ok(())
    }

    fn sadj(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::S_DAG, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn s(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::S, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn sx(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::SQRT_X, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn swap(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        self.push_gate(
            instruction_ids::SWAP,
            vec![self.map_qubit(q0), self.map_qubit(q1)],
            vec![],
        );
        Ok(())
    }

    fn tadj(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::T_DAG, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn t(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::T, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn y(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::PAULI_Y, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn z(&mut self, q: usize) -> Result<(), String> {
        self.push_gate(instruction_ids::PAULI_Z, vec![self.map_qubit(q)], vec![]);
        Ok(())
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        let q0_id = self.qubit_id_map.remove(&q0).unwrap_or(q0);
        let q1_id = self.qubit_id_map.remove(&q1).unwrap_or(q1);
        self.qubit_id_map.insert(q0, q1_id);
        self.qubit_id_map.insert(q1, q0_id);

        let q0_post_select = self.post_select_measurements.remove(&q0);
        let q1_post_select = self.post_select_measurements.remove(&q1);
        if let Some(val) = q0_post_select {
            self.post_select_measurements.insert(q1, val);
        }
        if let Some(val) = q1_post_select {
            self.post_select_measurements.insert(q0, val);
        }

        Ok(())
    }

    fn qubit_is_zero(&mut self, _q: usize) -> Result<bool, String> {
        Ok(true)
    }

    fn custom_intrinsic(
        &mut self,
        name: &str,
        arg: Value,
        _globals: &impl qsc::fir::PackageStoreLookup,
    ) -> Option<Result<Value, String>> {
        match name {
            "BeginRepeatEstimatesInternal" => {
                let count = arg.unwrap_int();
                if count < 0 {
                    return Some(Err(format!("count must be non-negative, got {count}.")));
                }

                let repetitions = count as u64;
                self.repeat_frames.push(RepeatFrame {
                    repetitions,
                    operations: Vec::new(),
                });
                Some(Ok(Value::unit()))
            }
            "EndRepeatEstimatesInternal" => {
                let Some(frame) = self.repeat_frames.pop() else {
                    return Some(Err("cannot end repeat before beginning repeat".to_string()));
                };

                if frame.repetitions <= 1 {
                    for op in frame.operations {
                        self.push_operation(op);
                    }
                } else {
                    self.push_operation(PendingOperation::Block {
                        repetitions: frame.repetitions,
                        operations: frame.operations,
                    });
                }

                Some(Ok(Value::unit()))
            }
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            "EndEstimateCaching"
            | "GlobalPhase"
            | "ConfigurePauliNoise"
            | "ConfigureQubitLoss"
            | "ApplyIdleNoise"
            | "EnableMemoryComputeArchitecture" => Some(Ok(Value::unit())),
            "Load" => {
                let q = arg.unwrap_qubit().deref().0;
                self.load(q);
                Some(Ok(Value::unit()))
            }
            "Store" => {
                let q = arg.unwrap_qubit().deref().0;
                self.store(q);
                Some(Ok(Value::unit()))
            }
            "AccountForEstimatesInternal" => Some(Err(
                "AccountForEstimatesInternal is not supported in trace builder".to_string(),
            )),
            "PostSelectZ" => {
                let values = arg.unwrap_tuple();
                let [result, qubit] = std::array::from_fn(|i| values[i].clone());
                let Value::Result(BackendResult::Val(val)) = result else {
                    panic!("first argument to PostSelectZ should be a measurement result");
                };
                let qubit = qubit.unwrap_qubit().deref().0;
                self.post_select_measurements.insert(qubit, val);

                Some(Ok(Value::unit()))
            }
            _ => None,
        }
    }
}

pub fn trace_expr(
    interpreter: &mut Interpreter,
    expr: &str,
) -> Result<Trace, Vec<interpret::Error>> {
    let mut builder = TraceBuilder::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    interpreter
        .run_with_sim(&mut builder, &mut out, Some(expr), None)
        .map_err(|e| e.into_iter().collect::<Vec<_>>())?;

    Ok(builder.into_trace())
}

pub fn trace_call(
    interpreter: &mut Interpreter,
    callable: Value,
    args: Value,
) -> Result<Trace, Vec<interpret::Error>> {
    let mut builder = TraceBuilder::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    interpreter
        .invoke_with_sim(&mut builder, &mut out, callable, args, None)
        .map_err(|e| e.into_iter().collect::<Vec<_>>())?;

    Ok(builder.into_trace())
}
