// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::batcher::{Batcher, DEFAULT_MAX_BATCH_SIZE};
use crate::{
    Parameter::{Float, QubitId, ResultId},
    QuantumBackend, QuantumBackendInstruction,
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
pub fn run_qsharp_call(
    interpreter: &mut Interpreter,
    callable: Value,
    args: Value,
    backend: &mut impl QuantumBackend,
) -> Result<(), QsharpRunError> {
    let mut qs_backend = QsBackendReverseShim::new(backend).map_err(QsharpRunError::Backend)?;
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
    let mut qs_backend = QsBackendReverseShim::new(backend).map_err(QsharpRunError::Backend)?;
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    interpreter
        .run_with_sim(&mut qs_backend, &mut out, Some(expr), None)
        .map_err(QsharpRunError::Interpreter)?;

    qs_backend.batcher.finish().map_err(QsharpRunError::Backend)
}

/// Adapts classical Q# evaluator calls to buffered quantum instructions.
struct QsBackendReverseShim<'a> {
    batcher: Batcher<'a>,
    qubits: Vec<usize>,
    free_list: Vec<usize>,
}

impl<'a> QsBackendReverseShim<'a> {
    /// Creates a batcher and an empty logical-to-backend qubit map.
    fn new(backend: &'a mut impl QuantumBackend) -> Result<Self, String> {
        Ok(Self {
            batcher: Batcher::new(backend, DEFAULT_MAX_BATCH_SIZE)?,
            qubits: Vec::new(),
            free_list: Vec::new(),
        })
    }

    /// Measures into a reusable result slot and resolves it before execution continues.
    fn measure(&mut self, name: &'static str, q: usize) -> Result<val::Result, String> {
        self.batcher.push(QuantumBackendInstruction {
            instruction_name: name,
            parameters: smallvec![QubitId(self.qubits[q]), ResultId(0)],
        })?;
        self.batcher.read_result(0).map(val::Result::Val)
    }
}

macro_rules! gates {
    ($($name:ident($($q:ident),+));+ $(;)?) => {$(
        /// Buffers the corresponding gate using mapped backend qubit IDs.
        fn $name(&mut self, $($q: usize),+) -> Result<(), String> {
            self.batcher.push(QuantumBackendInstruction {
                instruction_name: stringify!($name),
                parameters: smallvec![$(QubitId(self.qubits[$q])),+],
            })
        }
    )+};
}

macro_rules! rotations {
    ($($name:ident($($q:ident),+));+ $(;)?) => {$(
        /// Buffers a rotation with qubit operands preceding the angle.
        fn $name(&mut self, theta: f64, $($q: usize),+) -> Result<(), String> {
            self.batcher.push(QuantumBackendInstruction {
                instruction_name: stringify!($name),
                parameters: smallvec![$(QubitId(self.qubits[$q])),+, Float(theta)],
            })
        }
    )+};
}

impl Backend for QsBackendReverseShim<'_> {
    gates! {
        ccx(ctl0, ctl1, q); cx(ctl, q); cy(ctl, q); cz(ctl, q);
        h(q); reset(q); sadj(q); s(q); sx(q); swap(q0, q1);
        tadj(q); t(q); x(q); y(q); z(q)
    }
    rotations! { rx(q); rxx(q0, q1); ry(q); ryy(q0, q1); rz(q); rzz(q0, q1) }

    /// Measures a qubit and returns a concrete result to the evaluator.
    fn m(&mut self, q: usize) -> Result<val::Result, String> {
        self.measure("mz_store", q)
    }

    /// Measures and resets a qubit before returning its concrete result.
    fn mresetz(&mut self, q: usize) -> Result<val::Result, String> {
        self.measure("mz_store_reset", q)
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

    /// Selects full execution for arithmetic helpers and rejects other custom intrinsics.
    fn custom_intrinsic(
        &mut self,
        name: &str,
        _arg: Value,
        _globals: &impl qsc::fir::PackageStoreLookup,
    ) -> Option<Result<Value, String>> {
        match name {
            "IsResourceEstimating" => Some(Ok(Value::Bool(false))),
            _ => Some(Err(format!("Custom intrinsic `{name}` is not supported"))),
        }
    }

    /// Leaves backend randomness unchanged because no seed instruction exists.
    fn set_seed(&mut self, _seed: Option<u64>) {}
}
