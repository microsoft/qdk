// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{Batcher, DEFAULT_MAX_BATCH_SIZE, OutputRecord, OutputValue, RunOutput};
use crate::{
    Parameter::{self, Float, QubitId},
    QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
};
use qdk_simulators::{
    MeasurementResult, OutputRecord as SimulatorOutputRecord, Simulator,
    bytecode::{AdaptiveProgram, runtime::run_shot},
};
use smallvec::{SmallVec, smallvec};

/// Executes one shot of already-lowered adaptive QIR through a batched backend.
///
/// The caller must supply well-formed bytecode as required by `run_shot`; this
/// function does not parse QIR or validate bytecode control flow and table indices.
/// Measurements flush the batch immediately and populate local result slots.
/// Gates flush at capacity or successful completion. Backend state is not reset.
///
/// Output contains only leaf records: bytecode execution does not retain labels,
/// array/tuple markers, or the entry point's return value.
/// Movement, loss, and noise operations are unsupported and return errors.
///
/// Since `Simulator` methods cannot return errors, the first adapter error is
/// retained until interpretation finishes. Later backend calls are suppressed
/// and unsubmitted instructions are discarded. Backend changes are not rolled back.
pub fn run_adaptive_program(
    program: &AdaptiveProgram<u64>,
    backend: &mut impl QuantumBackend,
) -> Result<RunOutput, String> {
    let mut simulator = BackendSimulator {
        batcher: Batcher::new(backend, DEFAULT_MAX_BATCH_SIZE),
        num_qubits: program.num_qubits as usize,
        measurements: vec![MeasurementResult::Zero; program.num_results as usize],
        error: None,
    };
    let records = run_shot(program, &mut simulator);
    if let Some(error) = simulator.error {
        return Err(error);
    }
    simulator.batcher.finish()?;

    let records = records
        .into_iter()
        .map(|record| {
            let value = match record {
                SimulatorOutputRecord::Result(MeasurementResult::Zero) => {
                    OutputValue::Result(false)
                }
                SimulatorOutputRecord::Result(MeasurementResult::One) => OutputValue::Result(true),
                SimulatorOutputRecord::Result(MeasurementResult::Loss) => {
                    return Err("QEP output cannot represent a lost measurement".into());
                }
                SimulatorOutputRecord::Bool(value) => OutputValue::Bool(value),
                SimulatorOutputRecord::Int(value) => OutputValue::Int(value),
                SimulatorOutputRecord::Double(value) => OutputValue::Double(value),
            };
            Ok(OutputRecord { value, label: None })
        })
        .collect::<Result<_, String>>()?;
    Ok(RunOutput {
        records,
        return_value: None,
    })
}

/// Adapts the infallible simulator interface to a fallible QEP backend.
struct BackendSimulator<'a, B> {
    batcher: Batcher<&'a mut B>,
    num_qubits: usize,
    measurements: Vec<MeasurementResult>,
    error: Option<String>,
}

impl<B: QuantumBackend> BackendSimulator<'_, B> {
    /// Retains the first error so no later instruction reaches the backend.
    fn fail(&mut self, error: String) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    /// Rejects operations not represented by QEP's instruction set.
    fn unsupported(&mut self, operation: &str) {
        self.fail(format!("QEP adaptive runner does not support {operation}"));
    }

    /// Checks qubit operands before submitting an instruction.
    fn instruction(
        &mut self,
        instruction_name: &'static str,
        parameters: SmallVec<[Parameter; 3]>,
    ) -> Option<QuantumBackendInstruction> {
        if self.error.is_some() {
            return None;
        }
        for parameter in &parameters {
            if let QubitId(id) = parameter
                && *id >= self.num_qubits
            {
                self.fail(format!("Qubit ID {id} exceeds the declared qubit count"));
                return None;
            }
        }
        Some(QuantumBackendInstruction {
            instruction_name,
            parameters,
        })
    }

    /// Buffers a gate, retaining any failure from a capacity flush.
    fn send(&mut self, name: &'static str, parameters: SmallVec<[Parameter; 3]>) {
        if let Some(instruction) = self.instruction(name, parameters)
            && let Err(error) = self.batcher.push(instruction)
        {
            self.fail(error);
        }
    }

    /// Checks that a local result slot is within the program's resource bounds.
    fn valid_result(&mut self, result_id: usize) -> bool {
        if self.error.is_some() {
            return false;
        }
        if result_id >= self.measurements.len() {
            self.fail(format!(
                "Result ID {result_id} exceeds the declared result count"
            ));
            return false;
        }
        true
    }

    /// Resolves an immediate measurement and caches it for bytecode result reads.
    fn measure(&mut self, name: &'static str, target: usize, result_id: usize) {
        if !self.valid_result(result_id) {
            return;
        }
        let Some(instruction) = self.instruction(name, smallvec![QubitId(target)]) else {
            return;
        };
        match self.batcher.push_with_result(instruction) {
            Ok(QuantumBackendResult::Zero) => {
                self.measurements[result_id] = MeasurementResult::Zero;
            }
            Ok(QuantumBackendResult::One) => {
                self.measurements[result_id] = MeasurementResult::One;
            }
            Ok(QuantumBackendResult::Error) => {
                self.fail("QIR backend returned an error result".into());
            }
            Err(error) => self.fail(error),
        }
    }
}

macro_rules! gates {
    ($($method:ident => $name:literal($($qubit:ident),+));+ $(;)?) => {$(
        /// Buffers the corresponding QEP gate.
        fn $method(&mut self, $($qubit: usize),+) {
            self.send($name, smallvec![$(QubitId($qubit)),+]);
        }
    )+};
}

macro_rules! rotations {
    ($($method:ident($($qubit:ident),+));+ $(;)?) => {$(
        /// Buffers a rotation with qubit operands preceding its angle.
        fn $method(&mut self, angle: f64, $($qubit: usize),+) {
            self.send(stringify!($method), smallvec![$(QubitId($qubit)),+, Float(angle)]);
        }
    )+};
}

impl<B: QuantumBackend> Simulator for BackendSimulator<'_, B> {
    type Noise = ();
    type StateDumpData = ();

    /// Rejects construction without the borrowed backend required by this adapter.
    fn new(_num_qubits: usize, _num_results: usize, _seed: u32, _noise: ()) -> Self {
        panic!("BackendSimulator must be constructed with a borrowed QuantumBackend");
    }

    gates! {
        x => "x"(q); y => "y"(q); z => "z"(q); h => "h"(q);
        s => "s"(q); s_adj => "sadj"(q); sx => "sx"(q);
        t => "t"(q); t_adj => "tadj"(q);
        cx => "cx"(control, target); cy => "cy"(control, target);
        cz => "cz"(control, target); swap => "swap"(q1, q2);
        resetz => "reset"(q)
    }
    rotations! { rx(q); ry(q); rz(q); rxx(q1, q2); ryy(q1, q2); rzz(q1, q2) }

    /// Decomposes inverse square-root X into QEP-supported Clifford gates.
    fn sx_adj(&mut self, target: usize) {
        self.h(target);
        self.s_adj(target);
        self.h(target);
    }

    /// Flushes a measurement barrier and writes its local result slot.
    fn mz(&mut self, target: usize, result_id: usize) {
        self.measure("mz", target, result_id);
    }

    /// Flushes a measurement-and-reset barrier and writes its local result slot.
    fn mresetz(&mut self, target: usize, result_id: usize) {
        self.measure("mz_reset", target, result_id);
    }

    /// Rejects movement, which has no QEP instruction.
    fn mov(&mut self, _target: usize) {
        self.unsupported("movement");
    }

    /// Rejects correlated noise, which must be implemented by the backend.
    fn correlated_noise_intrinsic(&mut self, _intrinsic_id: u32, _targets: &[usize]) {
        self.unsupported("correlated noise intrinsics");
    }

    /// Exposes local result slots for classical control flow and output recording.
    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    /// Takes the cached result slots.
    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }

    /// Rejects state capture, which is not exposed by `QuantumBackend`.
    fn state_dump(&self) -> &() {
        panic!("QEP adaptive runner does not support state capture");
    }

    /// Rejects loss inspection, which has no QEP instruction.
    fn peek_loss(&mut self, _target: usize, _result_id: usize) {
        self.unsupported("loss inspection");
    }

    /// Rejects explicit loss noise, which has no QEP instruction.
    fn apply_loss_noise(&mut self, _p_loss: f64, _target: usize) {
        self.unsupported("loss noise");
    }

    /// Rejects explicit readout noise, which has no QEP instruction.
    fn apply_readout_noise(&mut self, _p_zero_as_one: f64, _p_one_as_zero: f64, _result_id: usize) {
        self.unsupported("readout noise");
    }

    /// Writes a classical value to a local result slot without contacting the backend.
    fn write_result(&mut self, value: bool, result_id: usize) {
        if self.valid_result(result_id) {
            self.measurements[result_id] = if value {
                MeasurementResult::One
            } else {
                MeasurementResult::Zero
            };
        }
    }
}
