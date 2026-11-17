// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::super::batcher::{Batcher, DEFAULT_MAX_BATCH_SIZE};
use crate::{
    InstructionId,
    Parameter::{Float, QubitId, ResultId},
    QuantumBackend, QuantumBackendInstruction, instructions as instr,
};
use num_bigint::BigUint;
use num_complex::Complex;
use qsc::{
    Backend,
    interpret::{self, GenericReceiver, Interpreter, Value},
};
use qsc_eval::val;
use smallvec::smallvec;

/// An interpreter diagnostic or an error executing a backend batch.
#[derive(Debug, thiserror::Error)]
pub enum QsharpRunError {
    #[error("Q# evaluation failed: {0:?}")]
    Interpreter(Vec<interpret::Error>),
    #[error("Q# backend failed: {0}")]
    Backend(String),
}

/// Invokes a Q# callable with the supplied arguments and batched quantum backend.
///
/// Discards the callable's return value and classical output. Pending instructions
/// are flushed on success; execution errors discard instructions not yet submitted.
/// Backend changes are not rolled back and backend state is not reset between calls.
/// Qubit allocation and ID swaps are local bookkeeping. Release trusts the caller
/// to reset qubits before releasing them. State capture and non-destructive zero
/// checks are unsupported, and quantum seeds cannot be forwarded to the backend.
/// Global phase is discarded because it has no corresponding backend instruction.
/// Memory/compute architecture configuration and Load/Store annotations are ignored.
pub fn run_qsharp_call(
    interpreter: &mut Interpreter,
    callable: Value,
    args: Value,
    backend: &mut impl QuantumBackend,
) -> Result<(), QsharpRunError> {
    let mut qs_backend = QsBackendReverseShim::new(backend);
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    interpreter
        .invoke_with_sim(&mut qs_backend, &mut out, callable, args, None)
        .map_err(QsharpRunError::Interpreter)?;

    qs_backend.batcher.finish().map_err(QsharpRunError::Backend)
}

/// Runs a Q# expression with the supplied interpreter and batched quantum backend.
///
/// Uses the same output, batching, and qubit-lifecycle behavior as [`run_qsharp_call`].
pub fn run_qsharp_expr(
    interpreter: &mut Interpreter,
    expr: &str,
    backend: &mut impl QuantumBackend,
) -> Result<(), QsharpRunError> {
    let mut qs_backend = QsBackendReverseShim::new(backend);
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    interpreter
        .run_with_sim(&mut qs_backend, &mut out, Some(expr), None)
        .map_err(QsharpRunError::Interpreter)?;

    qs_backend.batcher.finish().map_err(QsharpRunError::Backend)
}

/// Adapts classical Q# evaluator calls to buffered quantum instructions.
struct QsBackendReverseShim<'a> {
    batcher: Batcher<&'a mut dyn QuantumBackend>,
    qubits: Vec<usize>,
    free_list: Vec<usize>,
}

impl<'a> QsBackendReverseShim<'a> {
    /// Creates a batcher and an empty logical-to-backend qubit map.
    fn new(backend: &'a mut dyn QuantumBackend) -> Self {
        Self {
            batcher: Batcher::new(backend, DEFAULT_MAX_BATCH_SIZE),
            qubits: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Measures into a reusable result slot and resolves it before execution continues.
    fn measure(&mut self, instruction_id: InstructionId, q: usize) -> Result<val::Result, String> {
        self.batcher.push(QuantumBackendInstruction {
            instruction_id,
            parameters: smallvec![QubitId(self.qubits[q]), ResultId(0)],
        })?;
        self.batcher.read_result(0).map(val::Result::Val)
    }
}

macro_rules! gates {
    ($($name:ident => $id:ident($($q:ident),+));+ $(;)?) => {$(
        /// Buffers the corresponding gate using mapped backend qubit IDs.
        fn $name(&mut self, $($q: usize),+) -> Result<(), String> {
            self.batcher.push(QuantumBackendInstruction {
                instruction_id: instr::$id,
                parameters: smallvec![$(QubitId(self.qubits[$q])),+],
            })
        }
    )+};
}

macro_rules! rotations {
    ($($name:ident => $id:ident($($q:ident),+));+ $(;)?) => {$(
        /// Buffers a rotation with qubit operands preceding the angle.
        fn $name(&mut self, theta: f64, $($q: usize),+) -> Result<(), String> {
            self.batcher.push(QuantumBackendInstruction {
                instruction_id: instr::$id,
                parameters: smallvec![$(QubitId(self.qubits[$q])),+, Float(theta)],
            })
        }
    )+};
}

impl Backend for QsBackendReverseShim<'_> {
    gates! {
        ccx => CCX(ctl0, ctl1, q); cx => CX(ctl, q); cy => CY(ctl, q); cz => CZ(ctl, q);
        h => H(q); reset => RESET(q); sadj => SADJ(q); s => S(q); sx => SX(q); swap => SWAP(q0, q1);
        tadj => TADJ(q); t => T(q); x => X(q); y => Y(q); z => Z(q)
    }
    rotations! { rx => RX(q); rxx => RXX(q0, q1); ry => RY(q); ryy => RYY(q0, q1); rz => RZ(q); rzz => RZZ(q0, q1) }

    /// Measures a qubit and returns a concrete result to the evaluator.
    fn m(&mut self, q: usize) -> Result<val::Result, String> {
        self.measure(instr::MZ_STORE, q)
    }

    /// Measures and resets a qubit before returning its concrete result.
    fn mresetz(&mut self, q: usize) -> Result<val::Result, String> {
        self.measure(instr::MZ_STORE_RESET, q)
    }

    /// Reuses a released logical qubit or assigns a new backend slot.
    fn qubit_allocate(&mut self) -> Result<usize, String> {
        Ok(self.free_list.pop().unwrap_or_else(|| {
            let q = self.qubits.len();
            self.qubits.push(q);
            q
        }))
    }

    /// Recycles a qubit, trusting the Q# reset-before-release contract.
    fn qubit_release(&mut self, q: usize) -> Result<bool, String> {
        self.free_list.push(q);
        Ok(true)
    }

    /// Relabels qubits without emitting a physical SWAP gate.
    fn qubit_swap_id(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        self.qubits.swap(q0, q1);
        Ok(())
    }

    /// Rejects state capture, which is not exposed by the quantum instruction set.
    fn capture_quantum_state(&mut self) -> Result<(Vec<(BigUint, Complex<f64>)>, usize), String> {
        Err("QuantumBackend does not support quantum state capture".into())
    }

    /// Rejects zero checks rather than substituting a destructive measurement.
    fn qubit_is_zero(&mut self, _q: usize) -> Result<bool, String> {
        Err("QuantumBackend does not support non-destructive zero checks".into())
    }

    /// Executes cacheable blocks normally and ignores global phase and memory annotations.
    fn custom_intrinsic(
        &mut self,
        name: &str,
        _arg: Value,
        _globals: &impl qsc::fir::PackageStoreLookup,
    ) -> Option<Result<Value, String>> {
        match name {
            "GlobalPhase"
            | "EndEstimateCaching"
            | "EnableMemoryComputeArchitecture"
            | "Load"
            | "Store" => Some(Ok(Value::unit())),
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            "IsResourceEstimating" => Some(Ok(Value::Bool(false))),
            _ => Some(Err(format!("Custom intrinsic `{name}` is not supported"))),
        }
    }

    /// Leaves backend randomness unchanged because no seed instruction exists.
    fn set_seed(&mut self, _seed: Option<u64>) {}
}
