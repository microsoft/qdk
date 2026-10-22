// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{QsBackendReverseShim, QsharpRunError, run_qsharp_call, run_qsharp_expr};
use crate::{
    BatchResult, DEFAULT_MAX_BATCH_SIZE, QsBackendShim, QuantumBackend, QuantumBackendInstruction,
    QuantumBackendResult,
};
use qsc::{
    Backend, LanguageFeatures, PackageType, SourceMap, TargetCapabilityFlags,
    interpret::{GenericReceiver, Interpreter, Value},
};
use qsc_eval::backend::SparseSim;
use std::collections::VecDeque;

#[derive(Default)]
struct RecordingBackend {
    batches: Vec<Vec<QuantumBackendInstruction>>,
    measurements: VecDeque<bool>,
    failure: Option<String>,
}

impl QuantumBackend for RecordingBackend {
    /// Records batches and supplies concrete measurements at result barriers.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        self.batches.push(instructions.to_vec());
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let mut result = BatchResult::default();
        for instruction in instructions {
            crate::validate_instruction(instruction);
            if instruction.instruction_name == "read_result" {
                result.results.push(QuantumBackendResult::from(
                    self.measurements
                        .pop_front()
                        .ok_or("No test measurement supplied")?,
                ));
            }
        }
        Ok(result)
    }
}

/// Compiles a callable expression in an unrestricted interpreter.
fn callable(expr: &str) -> (Interpreter, Value) {
    let capabilities = TargetCapabilityFlags::all();
    let (std_id, store) = qsc::compile::package_store_with_stdlib(capabilities);
    let mut interpreter = Interpreter::new(
        SourceMap::default(),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .expect("interpreter should initialize");
    let mut sink = std::io::sink();
    let value = interpreter
        .eval_fragments(&mut GenericReceiver::new(&mut sink), expr)
        .expect("callable should compile");
    (interpreter, value)
}

#[test]
/// Forwards every gate with canonical operand ordering in a single batch.
fn forwards_all_gates() -> Result<(), String> {
    let mut backend = RecordingBackend::default();
    let mut shim = QsBackendReverseShim::new(&mut backend)?;
    let a = shim.qubit_allocate()?;
    let b = shim.qubit_allocate()?;
    let c = shim.qubit_allocate()?;
    shim.ccx(a, b, c)?;
    shim.cx(a, b)?;
    shim.cy(a, b)?;
    shim.cz(a, b)?;
    shim.h(a)?;
    shim.s(a)?;
    shim.sadj(a)?;
    shim.sx(a)?;
    shim.t(a)?;
    shim.tadj(a)?;
    shim.x(a)?;
    shim.y(a)?;
    shim.z(a)?;
    shim.swap(a, b)?;
    shim.reset(a)?;
    shim.rx(0.1, a)?;
    shim.rxx(0.2, a, b)?;
    shim.ry(0.3, a)?;
    shim.ryy(0.4, a, b)?;
    shim.rz(0.5, a)?;
    shim.rzz(0.6, a, b)?;
    shim.batcher.finish()?;

    assert_eq!(backend.batches.len(), 1);
    assert_eq!(
        backend.batches[0]
            .iter()
            .map(|instruction| format!("{instruction:?}"))
            .collect::<Vec<_>>(),
        [
            "ccx 0 1 2",
            "cx 0 1",
            "cy 0 1",
            "cz 0 1",
            "h 0",
            "s 0",
            "sadj 0",
            "sx 0",
            "t 0",
            "tadj 0",
            "x 0",
            "y 0",
            "z 0",
            "swap 0 1",
            "reset 0",
            "rx 0 0.1",
            "rxx 0 1 0.2",
            "ry 0 0.3",
            "ryy 0 1 0.4",
            "rz 0 0.5",
            "rzz 0 1 0.6",
        ]
    );
    Ok(())
}

#[test]
/// Flushes full batches and the final partial batch without extra instructions.
fn batches_at_capacity() -> Result<(), String> {
    let mut backend = RecordingBackend::default();
    let mut shim = QsBackendReverseShim::new(&mut backend)?;
    let q = shim.qubit_allocate()?;
    for _ in 0..=DEFAULT_MAX_BATCH_SIZE {
        shim.h(q)?;
    }
    shim.batcher.finish()?;
    assert_eq!(backend.batches.len(), 2);
    assert_eq!(backend.batches[0].len(), DEFAULT_MAX_BATCH_SIZE);
    assert_eq!(backend.batches[1].len(), 1);
    Ok(())
}

#[test]
/// Resolves measurements in order, reusing a result slot only after reading it.
fn measurement_barriers() -> Result<(), String> {
    let mut backend = RecordingBackend {
        measurements: VecDeque::from([true, false]),
        ..Default::default()
    };
    let mut shim = QsBackendReverseShim::new(&mut backend)?;
    let q = shim.qubit_allocate()?;
    shim.x(q)?;
    assert_eq!(shim.m(q)?, qsc_eval::val::Result::Val(true));
    assert_eq!(shim.mresetz(q)?, qsc_eval::val::Result::Val(false));
    shim.h(q)?;
    shim.batcher.finish()?;
    assert_eq!(
        backend
            .batches
            .iter()
            .map(|batch| batch.iter().map(|i| format!("{i:?}")).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [
            vec!["x 0", "mz_store 0 0", "read_result 0"],
            vec!["mz_store_reset 0 0", "read_result 0"],
            vec!["h 0"],
        ]
    );
    Ok(())
}

#[test]
/// Preserves physical mappings across ID swaps and logical qubit reuse.
fn reuses_swapped_qubits() -> Result<(), String> {
    let mut backend = RecordingBackend::default();
    let mut shim = QsBackendReverseShim::new(&mut backend)?;
    let a = shim.qubit_allocate()?;
    let b = shim.qubit_allocate()?;
    shim.x(a)?;
    shim.qubit_swap_id(a, b)?;
    shim.y(a)?;
    assert!(shim.qubit_release(a)?);
    assert_eq!(shim.qubit_allocate()?, a);
    shim.z(a)?;
    shim.h(b)?;
    let c = shim.qubit_allocate()?;
    shim.t(c)?;
    shim.batcher.finish()?;
    assert_eq!(
        backend.batches[0]
            .iter()
            .map(|i| format!("{i:?}"))
            .collect::<Vec<_>>(),
        ["x 0", "y 1", "z 1", "h 0", "t 2"]
    );
    Ok(())
}

#[test]
/// Rejects unavailable service operations without measuring or fabricating state.
fn rejects_unsupported_services() -> Result<(), String> {
    let mut backend = RecordingBackend::default();
    let mut shim = QsBackendReverseShim::new(&mut backend)?;
    let q = shim.qubit_allocate()?;
    assert!(
        shim.qubit_is_zero(q)
            .expect_err("unsupported")
            .contains("zero checks")
    );
    assert!(
        shim.capture_quantum_state()
            .expect_err("unsupported")
            .contains("state capture")
    );
    shim.set_seed(Some(42));
    shim.batcher.finish()?;
    assert!(backend.batches.is_empty());
    Ok(())
}

#[test]
/// Invokes a callable with arguments and follows backend-dependent control flow.
fn invokes_callable_with_adaptive_control_flow() {
    let (mut interpreter, value) = callable(
        "flip => {
            use q = Qubit();
            if flip { X(q); }
            if MResetZ(q) != One { fail \"Expected One\"; }
            if MResetZ(q) != Zero { fail \"Expected Zero after reset\"; }
        }",
    );
    let mut backend = QsBackendShim::new(SparseSim::new());
    run_qsharp_call(&mut interpreter, value, Value::Bool(true), &mut backend)
        .expect("adaptive execution should succeed");
}

#[test]
/// Delivers trailing gates even when no measurement requires a flush.
fn flushes_at_end_of_invocation() {
    let (mut interpreter, value) = callable("() => { use q = Qubit(); H(q); Reset(q); }");
    let mut backend = RecordingBackend::default();
    run_qsharp_call(&mut interpreter, value, Value::unit(), &mut backend)
        .expect("execution should succeed");
    assert_eq!(backend.batches.len(), 1);
    assert_eq!(
        backend.batches[0]
            .iter()
            .map(|i| i.instruction_name)
            .collect::<Vec<_>>(),
        ["h", "reset"]
    );
}

#[test]
/// Surfaces errors from the final flush rather than returning success.
fn propagates_final_flush_error() {
    let (mut interpreter, value) = callable("() => { use q = Qubit(); H(q); Reset(q); }");
    let mut backend = RecordingBackend {
        failure: Some("backend unavailable".into()),
        ..Default::default()
    };
    let error = run_qsharp_call(&mut interpreter, value, Value::unit(), &mut backend)
        .expect_err("backend failure");
    assert!(
        matches!(error, QsharpRunError::Backend(ref message) if message == "backend unavailable")
    );
}

#[test]
/// Preserves interpreter diagnostics when a measurement batch fails.
fn propagates_measurement_error() {
    let (mut interpreter, value) = callable("() => { use q = Qubit(); let _ = MResetZ(q); }");
    let mut backend = RecordingBackend {
        failure: Some("measurement failed".into()),
        ..Default::default()
    };
    let error = run_qsharp_call(&mut interpreter, value, Value::unit(), &mut backend)
        .expect_err("measurement failure");
    assert!(matches!(error, QsharpRunError::Interpreter(ref errors)
        if format!("{errors:?}").contains("measurement failed")));
}

#[test]
/// Does not submit buffered instructions after a Q# failure.
fn discards_pending_instructions_on_failure() {
    let (mut interpreter, value) = callable(
        "operation Fails() : Unit { use q = Qubit(); H(q); fail \"stop execution\"; } Fails",
    );
    let mut backend = RecordingBackend::default();
    assert!(matches!(
        run_qsharp_call(&mut interpreter, value, Value::unit(), &mut backend),
        Err(QsharpRunError::Interpreter(_))
    ));
    assert!(backend.batches.is_empty());
}

#[test]
/// Reports unknown custom intrinsics explicitly through interpreter diagnostics.
fn rejects_custom_intrinsics() {
    let (mut interpreter, value) = callable(
        "operation Unsupported() : Unit { body intrinsic; }
         () => { Unsupported(); }",
    );
    let mut backend = RecordingBackend::default();
    let error = run_qsharp_call(&mut interpreter, value, Value::unit(), &mut backend)
        .expect_err("custom intrinsic is unsupported");
    assert!(matches!(error, QsharpRunError::Interpreter(ref errors)
        if format!("{errors:?}").contains("Custom intrinsic `Unsupported` is not supported")));
    assert!(backend.batches.is_empty());
}

#[test]
/// Evaluates an expression using declarations already compiled in the interpreter.
fn runs_string_expression() {
    let (mut interpreter, _) =
        callable("operation Workload() : Unit { use q = Qubit(); X(q); Reset(q); } Workload");
    let mut backend = RecordingBackend::default();
    run_qsharp_expr(&mut interpreter, "Workload()", &mut backend)
        .expect("expression execution should succeed");
    assert_eq!(backend.batches.len(), 1);
    assert_eq!(
        backend.batches[0]
            .iter()
            .map(|i| format!("{i:?}"))
            .collect::<Vec<_>>(),
        ["x 0", "reset 0"]
    );
}

#[test]
/// Flushes measurements in expressions before evaluating adaptive branches.
fn runs_adaptive_string_expression() {
    let (mut interpreter, _) = callable("() => {}");
    let mut backend = QsBackendShim::new(SparseSim::new());
    run_qsharp_expr(
        &mut interpreter,
        "{ use q = Qubit(); X(q); if MResetZ(q) != One { fail \"Expected One\"; } }",
        &mut backend,
    )
    .expect("adaptive expression execution should succeed");
}

#[test]
/// Returns compilation errors without submitting instructions to the backend.
fn propagates_string_compilation_errors() {
    let (mut interpreter, _) = callable("() => {}");
    let mut backend = RecordingBackend::default();
    assert!(matches!(
        run_qsharp_expr(&mut interpreter, "MissingOperation()", &mut backend),
        Err(QsharpRunError::Interpreter(_))
    ));
    assert!(backend.batches.is_empty());
}

#[test]
/// Reports errors flushing instructions buffered by a string expression.
fn propagates_string_final_flush_error() {
    let (mut interpreter, _) = callable("() => {}");
    let mut backend = RecordingBackend {
        failure: Some("backend unavailable".into()),
        ..Default::default()
    };
    let error = run_qsharp_expr(
        &mut interpreter,
        "{ use q = Qubit(); X(q); Reset(q); }",
        &mut backend,
    )
    .expect_err("backend failure");
    assert!(
        matches!(error, QsharpRunError::Backend(ref message) if message == "backend unavailable")
    );
}
