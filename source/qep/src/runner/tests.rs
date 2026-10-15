// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{OutputRecord, OutputValue, RunOutput, execute, run_qir};
use crate::{
    BatchResult, DEFAULT_MAX_BATCH_SIZE, QirInstruction, QuantumBackend, QuantumBackendResult,
    SequentialCountingProfiler,
};
use inkwell::{context::Context, memory_buffer::MemoryBuffer};
use rustc_hash::FxHashMap;
use std::{collections::VecDeque, fmt::Write, path::PathBuf};

const ATTRIBUTES: &str = r#"
attributes #0 = { "entry_point" "required_num_qubits"="3" "required_num_results"="3" }
"#;

/// Creates a backend for runner tests that do not inspect backend state.
fn test_backend() -> SequentialCountingProfiler {
    SequentialCountingProfiler::new(1, rustc_hash::FxHashMap::default())
}

#[derive(Debug, PartialEq)]
struct Event(&'static str, Vec<usize>, Option<f64>);

#[derive(Default)]
struct Recorder {
    events: Vec<Event>,
    measurements: VecDeque<bool>,
    results: FxHashMap<usize, Option<bool>>,
    fail_on: Option<&'static str>,
    panic_on: Option<&'static str>,
}

impl Recorder {
    /// Records a backend call and injects configured failures.
    fn gate(
        &mut self,
        name: &'static str,
        qubits: &[usize],
        angle: Option<f64>,
    ) -> Result<(), String> {
        self.events.push(Event(name, qubits.to_vec(), angle));
        assert_ne!(self.panic_on, Some(name), "test backend panic");
        if self.fail_on == Some(name) {
            Err(format!("{name} failed"))
        } else {
            Ok(())
        }
    }
}

macro_rules! gates {
    ($($name:ident($($q:ident),+));+ $(;)?) => {$(
        /// Records a non-rotation gate invocation.
        fn $name(&mut self, $($q: usize),+) -> Result<(), String> {
            self.gate(stringify!($name), &[$($q),+], None)
        }
    )+};
}
macro_rules! rotations {
    ($($name:ident($($q:ident),+));+ $(;)?) => {$(
        /// Records a rotation gate invocation.
        fn $name(&mut self, angle: f64, $($q: usize),+) -> Result<(), String> {
            self.gate(stringify!($name), &[$($q),+], Some(angle))
        }
    )+};
}
impl Recorder {
    gates! {
        ccx(a, b, c); cx(a, b); cy(a, b); cz(a, b);
        h(a); s(a); sadj(a); sx(a); t(a); tadj(a); x(a); y(a); z(a);
        swap(a, b); reset(a)
    }
    rotations! { rx(a); rxx(a, b); ry(a); ryy(a, b); rz(a); rzz(a, b) }

    /// Records a deferred measurement issuance.
    fn mz(&mut self, q: usize, result: usize) -> Result<(), String> {
        self.gate("mz", &[q, result], None)?;
        self.results.insert(result, None);
        Ok(())
    }

    /// Records a deferred measurement-and-reset issuance.
    fn mresetz(&mut self, q: usize, result: usize) -> Result<(), String> {
        self.gate("mresetz", &[q, result], None)?;
        self.results.insert(result, None);
        Ok(())
    }

    /// Resolves and caches a supplied test measurement.
    fn read_result(&mut self, result: usize) -> Result<bool, String> {
        self.gate("read_result", &[result], None)?;
        let slot = self
            .results
            .get_mut(&result)
            .ok_or("No test result issued")?;
        if let Some(value) = slot {
            return Ok(*value);
        }
        let value = self
            .measurements
            .pop_front()
            .ok_or("No test measurement supplied")?;
        *slot = Some(value);
        Ok(value)
    }
}

impl QuantumBackend for Recorder {
    /// Records instructions and supplies each requested measurement in read order.
    fn execute_batch(&mut self, instructions: &[QirInstruction]) -> BatchResult {
        let mut batch_result = BatchResult::default();
        for instruction in instructions.iter().copied() {
            let result = match instruction {
                QirInstruction::ccx(a, b, c) => self.ccx(a, b, c),
                QirInstruction::cx(a, b) => self.cx(a, b),
                QirInstruction::cy(a, b) => self.cy(a, b),
                QirInstruction::cz(a, b) => self.cz(a, b),
                QirInstruction::rx(angle, target) => self.rx(angle, target),
                QirInstruction::rxx(angle, a, b) => self.rxx(angle, a, b),
                QirInstruction::ry(angle, target) => self.ry(angle, target),
                QirInstruction::ryy(angle, a, b) => self.ryy(angle, a, b),
                QirInstruction::rz(angle, target) => self.rz(angle, target),
                QirInstruction::rzz(angle, a, b) => self.rzz(angle, a, b),
                QirInstruction::h(target) => self.h(target),
                QirInstruction::s(target) => self.s(target),
                QirInstruction::sadj(target) => self.sadj(target),
                QirInstruction::sx(target) => self.sx(target),
                QirInstruction::t(target) => self.t(target),
                QirInstruction::tadj(target) => self.tadj(target),
                QirInstruction::x(target) => self.x(target),
                QirInstruction::y(target) => self.y(target),
                QirInstruction::z(target) => self.z(target),
                QirInstruction::swap(a, b) => self.swap(a, b),
                QirInstruction::reset(target) => self.reset(target),
                QirInstruction::mz(target, result) => self.mz(target, result),
                QirInstruction::mresetz(target, result) => self.mresetz(target, result),
                QirInstruction::read_result(result) => {
                    let value = self
                        .read_result(result)
                        .unwrap_or_else(|error| panic!("{error}"));
                    batch_result.results.push(QuantumBackendResult::from(value));
                    Ok(())
                }
            };
            result.unwrap_or_else(|error| panic!("{error}"));
        }
        batch_result
    }
}

#[test]
/// Returns ordered results for repeated and interleaved reads without restricting their position.
fn recorder_supports_multiple_reads_per_batch() {
    use QirInstruction::{h, mresetz, mz, read_result, x};
    use QuantumBackendResult::{One, Zero};

    let mut backend = Recorder {
        measurements: VecDeque::from([true, false, true]),
        ..Default::default()
    };
    let empty = backend.execute_batch(&[h(0)]);
    assert!(empty.results.is_empty());
    assert!(!empty.results.spilled());

    let multiple = backend.execute_batch(&[
        mz(0, 0),
        read_result(0),
        x(1),
        mresetz(1, 1),
        read_result(1),
        read_result(0),
        h(0),
    ]);
    assert_eq!(multiple.results.as_slice(), &[One, Zero, One]);
    assert_eq!(backend.measurements, VecDeque::from([true]));
    assert_eq!(backend.events.last(), Some(&Event("h", vec![0], None)));

    let single = backend.execute_batch(&[mz(0, 0), read_result(0), x(0)]);
    assert_eq!(single.results.as_slice(), &[One]);
    assert!(!single.results.spilled());
    assert!(backend.measurements.is_empty());
    assert_eq!(backend.events.last(), Some(&Event("x", vec![0], None)));
}

/// Parses and executes an in-memory QIR test fixture.
fn run(source: &str, backend: &mut impl QuantumBackend) -> Result<RunOutput, String> {
    let source = format!("{source}\n{ATTRIBUTES}");
    let context = Context::create();
    let buffer = MemoryBuffer::create_from_memory_range_copy(source.as_bytes(), "test");
    let module = context
        .create_module_from_ir(buffer)
        .map_err(|e| e.to_string())?;
    // SAFETY: Tests supply known QIR with valid memory access and C ABIs.
    unsafe { execute(&context, &module, backend, DEFAULT_MAX_BATCH_SIZE) }
}

#[test]
/// Verifies that every supported intrinsic preserves its arguments.
fn dispatches_every_intrinsic_with_original_arguments() -> Result<(), String> {
    let cases = [
        ("ccx__body", "ccx", vec![0, 1, 2], None),
        ("cx__body", "cx", vec![1, 2], None),
        ("cnot__body", "cx", vec![2, 0], None),
        ("cy__body", "cy", vec![1, 2], None),
        ("cz__body", "cz", vec![1, 2], None),
        ("rx__body", "rx", vec![1], Some(0.25)),
        ("rxx__body", "rxx", vec![1, 2], Some(-0.5)),
        ("ry__body", "ry", vec![1], Some(0.25)),
        ("ryy__body", "ryy", vec![1, 2], Some(-0.5)),
        ("rz__body", "rz", vec![1], Some(0.25)),
        ("rzz__body", "rzz", vec![1, 2], Some(-0.5)),
        ("h__body", "h", vec![1], None),
        ("s__body", "s", vec![1], None),
        ("s__adj", "sadj", vec![1], None),
        ("sx__body", "sx", vec![1], None),
        ("t__body", "t", vec![1], None),
        ("t__adj", "tadj", vec![1], None),
        ("x__body", "x", vec![1], None),
        ("y__body", "y", vec![1], None),
        ("z__body", "z", vec![1], None),
        ("swap__body", "swap", vec![2, 1], None),
        ("reset__body", "reset", vec![1], None),
    ];
    let mut declarations = String::new();
    let mut calls = String::new();
    let mut expected = Vec::new();
    for (symbol, method, qubits, angle) in cases {
        let mut types = Vec::new();
        let mut values = Vec::new();
        if let Some(angle) = angle {
            types.push("double");
            values.push(format!("double {angle}"));
        }
        for qubit in &qubits {
            types.push("ptr");
            values.push(format!("ptr inttoptr (i64 {qubit} to ptr)"));
        }
        writeln!(
            declarations,
            "declare void @__quantum__qis__{symbol}({})",
            types.join(", ")
        )
        .expect("writing to a string");
        writeln!(
            calls,
            "call void @__quantum__qis__{symbol}({})",
            values.join(", ")
        )
        .expect("writing to a string");
        expected.push(Event(method, qubits, angle));
    }
    for (symbol, method) in [
        ("m__body", "mz"),
        ("mz__body", "mz"),
        ("mresetz__body", "mresetz"),
    ] {
        writeln!(
            declarations,
            "declare void @__quantum__qis__{symbol}(ptr, ptr)"
        )
        .expect("writing to a string");
        writeln!(
            calls,
            "call void @__quantum__qis__{symbol}(ptr null, ptr null)"
        )
        .expect("writing to a string");
        expected.push(Event(method, vec![0, 0], None));
    }
    let mut backend = Recorder {
        measurements: VecDeque::from([false, true, false]),
        ..Default::default()
    };
    let output = run(
        &format!("{declarations}\ndefine void @main() #0 {{\n{calls} ret void\n}}"),
        &mut backend,
    )?;
    assert_eq!(backend.events, expected);
    assert_eq!(backend.measurements.len(), 3);
    assert_eq!(output.return_value, None);
    assert!(output.records.is_empty());
    run(
        &format!("{declarations}\ndefine void @main() #0 {{\n{calls} ret void\n}}"),
        &mut test_backend(),
    )?;
    Ok(())
}

#[test]
/// Verifies that instrumentation preserves internal call return values.
fn internal_call_return_values_survive_loop_phi_rewriting() -> Result<(), String> {
    let mut backend = Recorder::default();
    let output = run(
        r#"
        declare void @__quantum__qis__h__body(ptr) #1
        define i64 @step(i64 %count) #1 {
            call void @__quantum__qis__h__body(ptr null) #1
            %next = add i64 %count, 1
            ret i64 %next
        }
        define i64 @main() #0 {
        entry:
            br label %loop
        loop:
            %count = phi i64 [0, %entry], [%next, %loop]
            %next = call i64 @step(i64 %count) #1
            %done = icmp eq i64 %next, 3
            br i1 %done, label %exit, label %loop
        exit:
            ret i64 %next
        }
        attributes #1 = { nounwind willreturn memory(none) }
        "#,
        &mut backend,
    )?;
    assert_eq!(output.return_value, Some(3));
    assert_eq!(
        backend.events,
        (0..3)
            .map(|_| Event("h", vec![0], None))
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
/// Verifies that adaptive loops preserve PHIs and measurement values.
fn adaptive_loop_preserves_phis_and_measurement_values() -> Result<(), String> {
    let mut backend = Recorder {
        measurements: VecDeque::from([false, false, true]),
        ..Default::default()
    };
    let output = run(
        r#"
        declare void @__quantum__qis__m__body(ptr, ptr)
        declare i1 @__quantum__qis__read_result__body(ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare void @__quantum__rt__int_record_output(i64, ptr)
        define i64 @main() #0 {
        entry:
            br label %loop
        loop:
            %count = phi i64 [0, %entry], [%next, %loop]
            call void @__quantum__qis__m__body(ptr null, ptr null)
            %done = call i1 @__quantum__qis__read_result__body(ptr null)
            %next = add i64 %count, 1
            br i1 %done, label %exit, label %loop
        exit:
            call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__int_record_output(i64 %next, ptr null)
            ret i64 7
        }
    "#,
        &mut backend,
    )?;
    assert_eq!(output.return_value, Some(7));
    assert_eq!(
        output.records,
        vec![OutputRecord {
            value: OutputValue::Int(3),
            label: None
        }]
    );
    assert_eq!(
        backend.events,
        vec![
            Event("mz", vec![0, 0], None),
            Event("read_result", vec![0], None),
            Event("mz", vec![0, 0], None),
            Event("read_result", vec![0], None),
            Event("mz", vec![0, 0], None),
            Event("read_result", vec![0], None),
            Event("x", vec![2], None),
        ]
    );
    Ok(())
}

#[test]
/// Verifies that measurements resolve only when their values are consumed.
fn defers_resolution_until_result_reads_or_output() -> Result<(), String> {
    let mut backend = Recorder {
        measurements: VecDeque::from([true, false]),
        ..Default::default()
    };
    let output = run(
        r#"
        declare void @__quantum__qis__mz__body(ptr, ptr)
        declare void @__quantum__qis__mresetz__body(ptr, ptr)
        declare void @__quantum__qis__h__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__bool_record_output(i1, ptr)
        define void @main() #0 {
            call void @__quantum__qis__mz__body(ptr null, ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr null)
            %a = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
            %b = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__bool_record_output(i1 %a, ptr null)
            call void @__quantum__rt__bool_record_output(i1 %b, ptr null)
            call void @__quantum__rt__result_record_output(ptr null, ptr null)
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
            ret void
        }
        "#,
        &mut backend,
    )?;
    assert_eq!(
        backend.events,
        vec![
            Event("mz", vec![0, 1], None),
            Event("h", vec![2], None),
            Event("mresetz", vec![2, 0], None),
            Event("read_result", vec![1], None),
            Event("read_result", vec![1], None),
            Event("read_result", vec![0], None),
            Event("read_result", vec![1], None),
        ]
    );
    assert_eq!(
        output
            .records
            .into_iter()
            .map(|record| record.value)
            .collect::<Vec<_>>(),
        vec![
            OutputValue::Bool(true),
            OutputValue::Bool(true),
            OutputValue::Result(false),
            OutputValue::Result(true),
        ]
    );
    assert!(backend.measurements.is_empty());
    Ok(())
}

#[test]
/// Verifies structured output labels and values.
fn records_structured_output() -> Result<(), String> {
    let mut backend = Recorder {
        measurements: VecDeque::from([true]),
        ..Default::default()
    };
    let output = run(
        r#"
        @label = private constant [5 x i8] c"test\00"
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__mz__body(ptr, ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__tuple_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__bool_record_output(i1, ptr)
        declare void @__quantum__rt__int_record_output(i64, ptr)
        declare void @__quantum__rt__double_record_output(double, ptr)
        define void @main() #0 {
            call void @__quantum__rt__initialize(ptr null)
            call void @__quantum__qis__mz__body(ptr null, ptr null)
            %value = call i1 @__quantum__rt__read_result(ptr null)
            call void @__quantum__rt__tuple_record_output(i64 2, ptr @label)
            call void @__quantum__rt__array_record_output(i64 3, ptr null)
            call void @__quantum__rt__result_record_output(ptr null, ptr @label)
            call void @__quantum__rt__bool_record_output(i1 %value, ptr null)
            call void @__quantum__rt__int_record_output(i64 -42, ptr null)
            call void @__quantum__rt__double_record_output(double 0.125, ptr null)
            ret void
        }
    "#,
        &mut backend,
    )?;
    let expected = [
        (OutputValue::Tuple(2), Some("test")),
        (OutputValue::Array(3), None),
        (OutputValue::Result(true), Some("test")),
        (OutputValue::Bool(true), None),
        (OutputValue::Int(-42), None),
        (OutputValue::Double(0.125), None),
    ];
    assert_eq!(
        output.records,
        expected
            .into_iter()
            .map(|(value, label)| OutputRecord {
                value,
                label: label.map(str::to_owned)
            })
            .collect::<Vec<_>>()
    );
    Ok(())
}

const MEASURE_HELPER: &str = r#"
    declare void @__quantum__qis__m__body(ptr, ptr)
    declare i1 @__quantum__rt__read_result(ptr)
    declare void @__quantum__qis__x__body(ptr)
    define i1 @measure() {
        call void @__quantum__qis__m__body(ptr null, ptr null)
        %result = call i1 @__quantum__rt__read_result(ptr null)
        ret i1 %result
    }
    define void @main() #0 {
    entry:
        br label %loop
    loop:
        %done = call i1 @measure()
        br i1 %done, label %exit, label %loop
    exit:
        call void @__quantum__qis__x__body(ptr null)
        ret void
    }
"#;

const MEASURE_ONCE: &str = r#"
    declare void @__quantum__qis__m__body(ptr, ptr)
    declare i1 @__quantum__rt__read_result(ptr)
    define void @main() #0 {
        call void @__quantum__qis__m__body(ptr null, ptr null)
        %result = call i1 @__quantum__rt__read_result(ptr null)
        ret void
    }
"#;

#[test]
/// Verifies that result initialization does not leak across invocations.
fn previous_invocations_do_not_initialize_current_result_slots() -> Result<(), String> {
    let mut backend = Recorder {
        measurements: VecDeque::from([true]),
        ..Default::default()
    };
    run(MEASURE_HELPER, &mut backend)?;
    backend.events.clear();
    let error = run(
        r#"
        declare i1 @__quantum__rt__read_result(ptr)
        define void @main() #0 {
            %value = call i1 @__quantum__rt__read_result(ptr null)
            ret void
        }
        "#,
        &mut backend,
    )
    .expect_err("result must be issued in the current invocation");
    assert!(error.contains("has not been written"), "{error}");
    assert!(backend.events.is_empty());
    Ok(())
}

#[test]
/// Verifies that backend panics fail terminating QIR without deadlocking.
fn backend_panics_terminate_execution_without_deadlocking() {
    for method in ["mz", "read_result"] {
        for panic in [false, true] {
            let mut backend = Recorder {
                fail_on: (!panic).then_some(method),
                panic_on: panic.then_some(method),
                ..Default::default()
            };
            let error =
                run(MEASURE_ONCE, &mut backend).expect_err("backend panic should fail execution");
            assert!(error.contains("QIR backend panicked"), "{error}");
        }
    }
}

#[test]
/// Verifies that indirect calls and reserved definitions are rejected.
fn rejects_indirect_calls_and_reserved_symbols() {
    let cases = [
        (
            r#"
            define void @invoke(ptr %function) { call void %function() ret void }
            define void @main() #0 { ret void }
        "#,
            "Indirect calls",
        ),
        (
            r#"
            define void @__quantum__qis__h__body(ptr %q) { ret void }
            define void @main() #0 { ret void }
        "#,
            "Unsupported QIR function",
        ),
        (
            r#"
            define void @__qep_has_error() { ret void }
            define void @main() #0 { ret void }
        "#,
            "Unsupported QIR function",
        ),
        (
            r#"
            @external = external global i64
            define void @main() #0 { ret void }
        "#,
            "Unsupported QIR global",
        ),
    ];
    for (source, expected) in cases {
        let error = run(source, &mut test_backend()).expect_err("unsupported input");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
/// Verifies that a backend must return a value for a result-reading batch.
fn backend_must_return_requested_result() {
    struct MissingResultBackend;

    impl QuantumBackend for MissingResultBackend {
        /// Intentionally violates the batch result protocol.
        fn execute_batch(&mut self, _instructions: &[QirInstruction]) -> BatchResult {
            BatchResult::default()
        }
    }

    let mut backend = MissingResultBackend;
    let error = run(
        r#"
        declare void @__quantum__qis__m__body(ptr, ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        define void @main() #0 {
            call void @__quantum__qis__m__body(ptr null, ptr null)
            %result = call i1 @__quantum__rt__read_result(ptr null)
            ret void
        }
    "#,
        &mut backend,
    )
    .expect_err("missing backend result should fail execution");
    assert!(
        error.contains("returned no result for a batch ending in read_result"),
        "{error}"
    );
}

#[test]
/// Propagates backend error results through QIR execution.
fn backend_error_results_terminate_execution() {
    struct ErrorBackend;

    impl QuantumBackend for ErrorBackend {
        /// Reports an error for every submitted batch.
        fn execute_batch(&mut self, _instructions: &[QirInstruction]) -> BatchResult {
            BatchResult {
                results: [QuantumBackendResult::Error].into_iter().collect(),
            }
        }
    }

    let error =
        run(MEASURE_ONCE, &mut ErrorBackend).expect_err("backend error should fail execution");
    assert!(
        error.contains("QIR backend returned an error result"),
        "{error}"
    );
}

#[test]
/// Verifies that invalid resource IDs and unwritten reads fail.
fn rejects_invalid_ids_and_unwritten_results() {
    let cases = [
        (
            "declare void @__quantum__qis__h__body(ptr)",
            "call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))",
            "Qubit ID 3",
        ),
        (
            "declare void @__quantum__qis__m__body(ptr, ptr)",
            "call void @__quantum__qis__m__body(ptr null, ptr inttoptr (i64 3 to ptr))",
            "Result ID 3",
        ),
        (
            "declare i1 @__quantum__rt__read_result(ptr)",
            "%r = call i1 @__quantum__rt__read_result(ptr null)",
            "has not been written",
        ),
    ];
    for (declaration, call, expected) in cases {
        let mut backend = Recorder::default();
        let error = run(
            &format!("{declaration}\ndefine void @main() #0 {{\n{call}\nret void\n}}"),
            &mut backend,
        )
        .expect_err("invalid QIR operation");
        assert!(error.contains(expected), "{error}");
        assert!(backend.events.is_empty());
    }
}

#[test]
/// Verifies that unsupported declarations fail before execution.
fn rejects_unsupported_declarations_and_signatures_before_execution() {
    for (declaration, expected) in [
        ("declare void @unknown()", "Unsupported QIR function"),
        (
            "declare ptr @__quantum__rt__qubit_allocate()",
            "Unsupported QIR function",
        ),
        (
            "declare void @__quantum__qis__h__body(i64)",
            "Unsupported signature",
        ),
        (
            "declare ptr @__quantum__qis__m__body(ptr)",
            "Unsupported signature",
        ),
        (
            "declare void @__quantum__rt__write_result(ptr, i1)",
            "Unsupported QIR function",
        ),
    ] {
        let error = run(
            &format!("{declaration}\ndefine void @main() #0 {{ ret void }}"),
            &mut test_backend(),
        )
        .expect_err("unsupported declaration");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
/// Verifies rejection of dynamic management and invalid entry points.
fn rejects_dynamic_management_and_invalid_entry_points() {
    let cases = [
        ("define void @main() { ret void }", "exactly one"),
        (
            "define void @main() #0 { ret void }\ndefine void @other() #0 { ret void }",
            "exactly one",
        ),
        (
            "define void @main(i64 %arg) #0 { ret void }",
            "take no arguments",
        ),
        ("define i32 @main() #0 { ret i32 0 }", "return void or i64"),
        (
            r#"define void @main() #1 { ret void }
            attributes #1 = { "entry_point" "required_num_qubits"="3" }"#,
            "required_num_results",
        ),
        (
            r#"define void @main() #0 { ret void }
            !llvm.module.flags = !{!0}
            !0 = !{i32 1, !"dynamic_qubit_management", i1 true}"#,
            "Dynamic",
        ),
        (
            r#"define void @main() #0 { ret void }
            !llvm.module.flags = !{!0}
            !0 = !{i32 1, !"dynamic_result_management", i1 true}"#,
            "Dynamic",
        ),
    ];
    for (source, expected) in cases {
        let error = run(source, &mut test_backend()).expect_err("invalid input");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
/// Verifies support for QIR 1 pointers and static management flags.
fn accepts_qir1_typed_pointers_and_static_management_flags() -> Result<(), String> {
    let mut backend = Recorder {
        measurements: VecDeque::from([true]),
        ..Default::default()
    };
    let output = run(
        r#"
        %Qubit = type opaque
        %Result = type opaque
        declare void @__quantum__qis__mz__body(%Qubit*, %Result*)
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        define void @main() #0 {
            call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
            call void @__quantum__rt__result_record_output(%Result* null, i8* null)
            ret void
        }
        !llvm.module.flags = !{!0, !1}
        !0 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !1 = !{i32 1, !"dynamic_result_management", i1 false}
    "#,
        &mut backend,
    )?;
    assert_eq!(
        output.records,
        vec![OutputRecord {
            value: OutputValue::Result(true),
            label: None
        }]
    );
    Ok(())
}

struct TestFile(PathBuf);

impl TestFile {
    /// Creates a unique temporary path with the requested extension.
    fn new(extension: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "qep-{}-{}.{extension}",
            std::process::id(),
            rand::random::<u64>()
        )))
    }
}

impl Drop for TestFile {
    /// Removes the temporary fixture file when it leaves scope.
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0) {
            eprintln!(
                "Failed to clean up QEP test file {}: {error}",
                self.0.display()
            );
        }
    }
}

#[test]
/// Verifies execution of both textual IR and bitcode files.
fn runs_text_and_bitcode_files() -> Result<(), String> {
    let source = format!(
        r#"
        declare void @__quantum__qis__h__body(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        define i64 @main() #0 {{
            call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
            call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr null)
            call void @__quantum__rt__result_record_output(ptr null, ptr null)
            ret i64 42
        }}
        {ATTRIBUTES}
    "#
    );
    let context = Context::create();
    let module = context
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            source.as_bytes(),
            "test",
        ))
        .map_err(|e| e.to_string())?;
    let text = TestFile::new("ll");
    let bitcode = TestFile::new("bc");
    std::fs::write(&text.0, source).map_err(|e| e.to_string())?;
    assert!(module.write_bitcode_to_path(&bitcode.0));
    for path in [&text.0, &bitcode.0] {
        let mut backend = Recorder {
            measurements: VecDeque::from([true]),
            ..Default::default()
        };
        // SAFETY: The fixture uses valid QIR gate and output-recording calls.
        let output = unsafe { run_qir(path, &mut backend) }?;
        assert_eq!(output.return_value, Some(42));
        assert_eq!(
            backend.events,
            vec![
                Event("h", vec![2], None),
                Event("mz", vec![2, 0], None),
                Event("read_result", vec![0], None)
            ]
        );
        assert_eq!(
            output.records,
            vec![OutputRecord {
                value: OutputValue::Result(true),
                label: None
            }]
        );
    }
    Ok(())
}

#[test]
/// Verifies that missing and malformed input files fail cleanly.
fn rejects_missing_and_malformed_files() -> Result<(), String> {
    let missing = std::env::temp_dir().join(format!("missing-qep-{}.ll", rand::random::<u64>()));
    // SAFETY: The file does not exist, so nothing can execute.
    assert!(unsafe { run_qir(missing, &mut test_backend()) }.is_err());
    let malformed = TestFile::new("ll");
    std::fs::write(&malformed.0, "not LLVM IR").map_err(|e| e.to_string())?;
    // SAFETY: Parsing fails before execution.
    assert!(unsafe { run_qir(&malformed.0, &mut test_backend()) }.is_err());
    Ok(())
}

#[test]
/// Verifies that independent backends can execute concurrently.
fn runs_independent_backends_concurrently() {
    let tasks: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let mut backend = Recorder {
                    measurements: VecDeque::from([true]),
                    ..Default::default()
                };
                run(MEASURE_HELPER, &mut backend).expect("task execution");
                assert_eq!(
                    backend.events,
                    vec![
                        Event("mz", vec![0, 0], None),
                        Event("read_result", vec![0], None),
                        Event("x", vec![0], None)
                    ]
                );
            })
        })
        .collect();
    for task in tasks {
        task.join().expect("task should not panic");
    }
}
