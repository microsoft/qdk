// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use std::slice;

thread_local! {
    static CONTEXT_DROPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

unsafe extern "C" fn count_context_drop(_cookie: QdkContext) {
    CONTEXT_DROPS.set(CONTEXT_DROPS.get() + 1);
}

unsafe extern "C" fn keep_match(
    _cookie: QdkContext,
    _matched: *const QdkPassMatch,
    _decision: *mut *mut QdkPassDecision,
    _error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    QDK_OK
}

unsafe extern "C" fn fail_match_after_deciding(
    _cookie: QdkContext,
    _matched: *const QdkPassMatch,
    decision: *mut *mut QdkPassDecision,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the adapter supplies initialized disjoint output slots.
    unsafe {
        let status = qdk_pass_decision_new(QDK_DECISION_REMOVE, ptr::null(), 0, decision, error);
        if status != QDK_OK {
            return status;
        }
        let status = qdk_diagnostic_new(b"callback failed".as_ptr(), 15, error, ptr::null_mut());
        if status != QDK_OK {
            return status;
        }
    }
    QDK_CALLBACK_ERROR
}

fn pass_options() -> QdkGraphPassOptions {
    QdkGraphPassOptions {
        name: QdkString::borrowed("test"),
        kind: QDK_PASS_REWRITE,
        operations: ptr::null(),
        operation_count: 0,
        use_budget: 0,
        budget: 0,
    }
}

#[test]
fn accepted_callback_context_is_destroyed_exactly_once() {
    CONTEXT_DROPS.set(0);
    let before = handle::live_handles();
    let mut pass = ptr::null_mut();
    let operations = [QdkString::borrowed("test")];
    let options = QdkGraphPassOptions {
        operations: operations.as_ptr(),
        operation_count: 1,
        ..pass_options()
    };
    let mut builder = qsc_qhdg::GraphBuilder::new();
    let region = builder.add_region(None, qsc_qhdg::RegionKind::ModuleOmega);
    builder
        .add_node(region, "test", Vec::new())
        .expect("test node");
    let graph = builder.finish();
    let snapshot = qsc_qhdg::canonical_graph_snapshot(&graph).expect("snapshot");
    let mut graph = QdkGraph::owned(graph);
    let mut result = ptr::null_mut();
    let mut rewritten = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: inputs and outputs are live and disjoint; callbacks never unwind.
    unsafe {
        assert_eq!(
            qdk_graph_pass_new(
                &raw const options,
                Some(fail_match_after_deciding),
                7,
                Some(count_context_drop),
                &raw mut pass,
                ptr::null_mut(),
            ),
            QDK_OK
        );
        assert_eq!(CONTEXT_DROPS.get(), 0);
        assert_eq!(
            qdk_graph_pass_run(
                pass,
                graph,
                &raw mut rewritten,
                &raw mut result,
                &raw mut error
            ),
            QDK_CALLBACK_ERROR
        );
        assert!(rewritten.is_null() && result.is_null());
        assert_eq!(message(error), "callback failed");
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(CONTEXT_DROPS.get(), 0);
        assert_eq!(
            handle::live_handles(),
            before + 2,
            "failed decision was disposed"
        );
        assert_eq!(
            qsc_qhdg::canonical_graph_snapshot((*graph).inner.inspect_inactive())
                .expect("snapshot"),
            snapshot
        );
        assert_eq!(qdk_graph_free(&raw mut graph, ptr::null_mut()), QDK_OK);
        assert_eq!(qdk_graph_pass_free(&raw mut pass, ptr::null_mut()), QDK_OK);
        assert!(pass.is_null());
        assert_eq!(CONTEXT_DROPS.get(), 1);
        assert_eq!(qdk_graph_pass_free(&raw mut pass, ptr::null_mut()), QDK_OK);
        assert_eq!(CONTEXT_DROPS.get(), 1);
    }
    assert_eq!(handle::live_handles(), before);
}

#[test]
fn failed_callback_registration_retains_caller_context_ownership() {
    CONTEXT_DROPS.set(0);
    let before = handle::live_handles();
    let mut pipeline = ptr::null_mut();
    let options = pass_options();
    let mut error = ptr::null_mut();
    // SAFETY: inputs and outputs are live and disjoint; callbacks never unwind.
    unsafe {
        assert_eq!(
            qdk_pipeline_new(0, QDK_OPTIMIZATION_NONE, &raw mut pipeline, &raw mut error),
            QDK_OK
        );
        assert_eq!(
            qdk_pipeline_add_graph(
                pipeline,
                QDK_ANCHOR_CIRCUIT_BUILT,
                &raw const options,
                Some(keep_match),
                7,
                Some(count_context_drop),
                &raw mut error
            ),
            QDK_INVALID_ARGUMENT
        );
        assert!(message(error).contains("circuit anchor"));
        qdk_diagnostic_free(&raw mut error);
        let mut count = 99;
        assert_eq!(
            qdk_pipeline_count(pipeline, &raw mut count, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(count, 0);
        assert_eq!(
            qdk_pipeline_free(&raw mut pipeline, ptr::null_mut()),
            QDK_OK
        );
    }
    assert_eq!(CONTEXT_DROPS.get(), 0);
    assert_eq!(handle::live_handles(), before);
}

unsafe fn parameter(circuit: *mut QdkCircuit, name: &[u8]) -> *mut QdkParameter {
    let mut result = ptr::null_mut();
    // SAFETY: caller provides a live circuit; name and output cover their extents.
    unsafe {
        assert_eq!(
            qdk_circuit_parameter(
                circuit,
                name.as_ptr(),
                name.len(),
                &raw mut result,
                ptr::null_mut()
            ),
            QDK_OK
        );
    }
    result
}

unsafe fn leave_recovered_angle_for_scope_cleanup(working: *mut QdkCircuit) {
    // SAFETY: caller provides a live callback input whose scope owns the
    // retained parameter and recovered expression.
    unsafe {
        let parameter = parameter(working, b"p");
        let angle = QdkAngle {
            kind: QDK_ANGLE_PARAMETER,
            parameter,
            ..Default::default()
        };
        let mut expression = ptr::null_mut();
        assert_eq!(
            qdk_circuit_angle(
                working,
                QDK_ANGLE_NEG,
                &raw const angle,
                ptr::null(),
                &raw mut expression,
                ptr::null_mut()
            ),
            QDK_OK
        );
        let qubit = 0;
        let stored_angle = QdkAngle {
            kind: QDK_ANGLE_EXPRESSION,
            expression,
            ..Default::default()
        };
        let operation = QdkOperation {
            kind: QDK_OPERATION_STANDARD,
            gate: QDK_GATE_RX,
            qubits: &raw const qubit,
            qubit_count: 1,
            params: &raw const stored_angle,
            param_count: 1,
            ..Default::default()
        };
        assert_eq!(
            qdk_circuit_append(working, &raw const operation, ptr::null_mut()),
            QDK_OK
        );
        let mut record = ptr::null_mut();
        assert_eq!(
            qdk_circuit_instruction(working, 0, &raw mut record, ptr::null_mut()),
            QDK_OK
        );
        let mut recovered_angle = ptr::null_mut();
        assert_eq!(
            qdk_instruction_angle_expression(
                record,
                working,
                0,
                &raw mut recovered_angle,
                ptr::null_mut()
            ),
            QDK_OK
        );
        assert_eq!(
            qdk_instruction_free(&raw mut record, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(
            qdk_angle_expression_free(&raw mut expression, ptr::null_mut()),
            QDK_OK
        );
    }
}

unsafe extern "C" fn offer_then_fail(
    _cookie: QdkContext,
    scope: *mut QdkCircuitCallbackScope,
    working: *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the adapter supplies live callback-duration handles. Every
    // assertion and injected unwind stays inside the boundary.
    unsafe {
        boundary(error, None, || {
            let mut candidate = ptr::null_mut();
            assert_eq!(
                qdk_circuit_clone(working, &raw mut candidate, ptr::null_mut()),
                QDK_OK
            );
            assert_eq!(
                qdk_callback_offer(scope, &raw mut candidate, ptr::null_mut()),
                QDK_OK
            );
            assert!(candidate.is_null());
            leave_recovered_angle_for_scope_cleanup(working);
            let mut variable = ptr::null_mut();
            assert_eq!(
                qdk_circuit_variable(
                    working,
                    b"v".as_ptr(),
                    1,
                    QDK_VALUE_BOOL,
                    &raw mut variable,
                    ptr::null_mut()
                ),
                QDK_OK
            );
            let mut recovered_variable = ptr::null_mut();
            assert_eq!(
                qdk_circuit_variable_at(working, 0, &raw mut recovered_variable, ptr::null_mut()),
                QDK_OK
            );
            let input = QdkExpressionInput {
                kind: QDK_EXPR_VARIABLE,
                variable,
                ..Default::default()
            };
            let mut condition = ptr::null_mut();
            assert_eq!(
                qdk_circuit_expression(
                    working,
                    &raw const input,
                    &raw mut condition,
                    ptr::null_mut()
                ),
                QDK_OK
            );
            let options = QdkBodyOptions {
                kind: QDK_BODY_IF,
                condition,
                ..Default::default()
            };
            let mut body = ptr::null_mut();
            let mut child = ptr::null_mut();
            assert_eq!(
                qdk_circuit_begin(
                    working,
                    ptr::null(),
                    &raw const options,
                    &raw mut body,
                    ptr::null_mut()
                ),
                QDK_OK
            );
            assert_eq!(
                qdk_circuit_begin(
                    working,
                    body,
                    &raw const options,
                    &raw mut child,
                    ptr::null_mut()
                ),
                QDK_OK
            );
            let mut block = ptr::null_mut();
            assert_eq!(
                qdk_builder_block(child, &raw mut block, ptr::null_mut()),
                QDK_OK
            );
            let mut value = (*working).inner.access()?;
            value.store_mut().set_global_phase(1.0);
            panic!("injected callback mutation panic");
        })
    }
}

#[test]
fn callback_failure_destroys_accepted_replacement_and_scoped_children() {
    let before = handle::live_handles();
    let mut circuit = circuit();
    let mut pipeline = ptr::null_mut();
    let mut result = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: test owns live handles and disjoint outputs. Callback catches its
    // injected panic before returning through the C ABI.
    unsafe {
        assert_eq!(
            qdk_pipeline_new(0, QDK_OPTIMIZATION_NONE, &raw mut pipeline, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(
            qdk_pipeline_add_circuit(
                pipeline,
                QDK_ANCHOR_CIRCUIT_BUILT,
                QdkString::borrowed("fail"),
                Some(offer_then_fail),
                0,
                None,
                ptr::null_mut()
            ),
            QDK_OK
        );
        assert_eq!(
            qdk_pipeline_run(pipeline, circuit, &raw mut result, &raw mut error),
            QDK_CALLBACK_ERROR
        );
        assert!(result.is_null());
        assert!(message(error).contains("internal Rust panic"));
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(
            handle::live_handles(),
            before + 2,
            "replacement and every dependent were destroyed"
        );
        assert_eq!(
            (*circuit)
                .inner
                .inspect_inactive()
                .store()
                .global_phase()
                .to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(
            qdk_pipeline_free(&raw mut pipeline, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(qdk_circuit_free(&raw mut circuit, ptr::null_mut()), QDK_OK);
    }
    assert_eq!(handle::live_handles(), before);
}

unsafe extern "C" fn checked_offer(
    _cookie: QdkContext,
    scope: *mut QdkCircuitCallbackScope,
    working: *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: adapter supplies live scope and input; assertions stay contained.
    unsafe {
        boundary(error, None, || {
            let mut borrowed = working;
            assert_eq!(
                qdk_callback_offer(scope, &raw mut borrowed, ptr::null_mut()),
                QDK_BORROWED
            );
            assert_eq!(borrowed, working);
            let mut candidate = ptr::null_mut();
            assert_eq!(
                qdk_circuit_clone(working, &raw mut candidate, ptr::null_mut()),
                QDK_OK
            );
            let address = candidate;
            let mut parameter = ptr::null_mut();
            assert_eq!(
                qdk_circuit_parameter(
                    candidate,
                    b"p".as_ptr(),
                    1,
                    &raw mut parameter,
                    ptr::null_mut()
                ),
                QDK_OK
            );
            assert_eq!(
                qdk_callback_offer(scope, &raw mut candidate, ptr::null_mut()),
                QDK_IN_USE
            );
            assert_eq!(candidate, address);
            assert_eq!(
                qdk_parameter_free(&raw mut parameter, ptr::null_mut()),
                QDK_OK
            );
            assert_eq!(
                qdk_callback_offer(scope, &raw mut candidate, ptr::null_mut()),
                QDK_OK
            );
            assert!(candidate.is_null());
            let mut second = ptr::null_mut();
            assert_eq!(
                qdk_circuit_clone(working, &raw mut second, ptr::null_mut()),
                QDK_OK
            );
            let address = second;
            assert_eq!(
                qdk_callback_offer(scope, &raw mut second, ptr::null_mut()),
                QDK_ALREADY_SET
            );
            assert_eq!(second, address);
            assert_eq!(qdk_circuit_free(&raw mut second, ptr::null_mut()), QDK_OK);
            let mut scoped_parameter = ptr::null_mut();
            assert_eq!(
                qdk_circuit_parameter(
                    working,
                    b"scoped".as_ptr(),
                    6,
                    &raw mut scoped_parameter,
                    ptr::null_mut()
                ),
                QDK_OK
            );
            Ok(())
        })
    }
}

#[test]
fn replacement_offer_preserves_rejected_ownership_and_moves_only_once() {
    let before = handle::live_handles();
    let mut circuit = circuit();
    let mut pipeline = ptr::null_mut();
    let mut result = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: this test owns live handles and disjoint output slots.
    unsafe {
        assert_eq!(
            qdk_pipeline_new(0, QDK_OPTIMIZATION_NONE, &raw mut pipeline, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(
            qdk_pipeline_add_circuit(
                pipeline,
                QDK_ANCHOR_CIRCUIT_BUILT,
                QdkString::borrowed("offer"),
                Some(checked_offer),
                0,
                None,
                ptr::null_mut()
            ),
            QDK_OK
        );
        let status = qdk_pipeline_run(pipeline, circuit, &raw mut result, &raw mut error);
        assert_eq!(
            status,
            QDK_OK,
            "{}",
            if error.is_null() {
                String::new()
            } else {
                message(error)
            }
        );
        assert_eq!(handle::live_handles(), before + 3);
        assert_eq!(
            qdk_pipeline_result_free(&raw mut result, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(
            qdk_pipeline_free(&raw mut pipeline, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(qdk_circuit_free(&raw mut circuit, ptr::null_mut()), QDK_OK);
    }
    assert_eq!(handle::live_handles(), before);
}

fn circuit() -> *mut QdkCircuit {
    let mut circuit = ptr::null_mut();
    // SAFETY: this helper supplies a valid empty output slot.
    unsafe {
        assert_eq!(
            qdk_circuit_new(2, 1, &raw mut circuit, ptr::null_mut()),
            QDK_OK
        );
    }
    circuit
}

#[test]
fn unknown_gate_tag_returns_an_empty_metadata_output() {
    let mut info = QdkGateInfo::default();
    let mut error = ptr::null_mut();
    // SAFETY: all output slots are writable and disjoint.
    unsafe {
        assert_eq!(
            qdk_gate_info(u32::MAX, &raw mut info, &raw mut error),
            QDK_INVALID_ARGUMENT
        );
        assert!(info.name.data.is_null());
        assert!(message(error).contains("unknown standard gate"));
        qdk_diagnostic_free(&raw mut error);
    }
}

#[test]
fn wrong_parameter_arity_does_not_append_an_instruction() {
    let mut circuit = circuit();
    let target = [0];
    let operation = QdkOperation {
        kind: QDK_OPERATION_STANDARD,
        gate: QDK_GATE_RX,
        qubits: target.as_ptr(),
        qubit_count: 1,
        ..Default::default()
    };
    let mut error = ptr::null_mut();
    let mut info = QdkCircuitInfo::default();
    // SAFETY: circuit and operand/output allocations are live and disjoint.
    unsafe {
        assert_eq!(
            qdk_circuit_append(circuit, &raw const operation, &raw mut error),
            QDK_INVALID_ARGUMENT
        );
        assert!(message(error).contains("expects 1 parameters"));
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(
            qdk_circuit_info(circuit, &raw mut info, &raw mut error),
            QDK_OK
        );
        assert_eq!(info.instructions, 0);
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
    }
}

#[test]
fn parameter_from_another_circuit_is_rejected() {
    let mut first = circuit();
    let mut second = circuit();
    let mut parameter = ptr::null_mut();
    let mut error = ptr::null_mut();
    let target = [0];
    // SAFETY: both owners and all disjoint input/output allocations are live.
    unsafe {
        assert_eq!(
            qdk_circuit_parameter(
                first,
                b"theta".as_ptr(),
                5,
                &raw mut parameter,
                &raw mut error
            ),
            QDK_OK
        );
        let angles = [QdkAngle {
            kind: QDK_ANGLE_PARAMETER,
            parameter,
            ..Default::default()
        }];
        let operation = QdkOperation {
            kind: QDK_OPERATION_STANDARD,
            gate: QDK_GATE_RX,
            qubits: target.as_ptr(),
            qubit_count: 1,
            params: angles.as_ptr(),
            param_count: 1,
            ..Default::default()
        };
        assert_eq!(
            qdk_circuit_append(second, &raw const operation, &raw mut error),
            QDK_WRONG_OWNER
        );
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(
            qdk_parameter_free(&raw mut parameter, &raw mut error),
            QDK_OK
        );
        assert_eq!(qdk_circuit_free(&raw mut first, &raw mut error), QDK_OK);
        assert_eq!(qdk_circuit_free(&raw mut second, &raw mut error), QDK_OK);
    }
}

#[test]
fn missing_instruction_clears_the_owning_output() {
    let mut circuit = circuit();
    let mut instruction = ptr::dangling_mut();
    let mut error = ptr::null_mut();
    // SAFETY: the sentinel is overwritten, never read; owner/outputs are live.
    unsafe {
        assert_eq!(
            qdk_circuit_instruction(circuit, 0, &raw mut instruction, &raw mut error),
            QDK_INVALID_ARGUMENT
        );
        assert!(instruction.is_null());
        assert!(message(error).contains("instruction 0 does not exist"));
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
    }
}

#[test]
fn matrix_query_rejects_a_non_unitary_record() {
    let mut circuit = circuit();
    let target = [0];
    let operation = QdkOperation {
        kind: QDK_OPERATION_STANDARD,
        gate: QDK_GATE_X,
        qubits: target.as_ptr(),
        qubit_count: 1,
        ..Default::default()
    };
    let mut instruction = ptr::null_mut();
    let mut needed = usize::MAX;
    let mut error = ptr::null_mut();
    // SAFETY: all handles and buffers are live and disjoint.
    unsafe {
        assert_eq!(
            qdk_circuit_append(circuit, &raw const operation, &raw mut error),
            QDK_OK
        );
        assert_eq!(
            qdk_circuit_instruction(circuit, 0, &raw mut instruction, &raw mut error),
            QDK_OK
        );
        assert_eq!(
            qdk_instruction_matrix(
                instruction,
                ptr::null_mut(),
                0,
                &raw mut needed,
                &raw mut error
            ),
            QDK_WRONG_VARIANT
        );
        assert_eq!(needed, 0);
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(
            qdk_instruction_free(&raw mut instruction, &raw mut error),
            QDK_OK
        );
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
    }
}

#[test]
fn panic_poison_allows_child_first_teardown_without_publishing() {
    let before = handle::live_handles();
    let mut circuit = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: all handles are live and owned by this test; the injected unwind
    // happens inside the boundary, never across an extern C frame.
    unsafe {
        assert_eq!(
            qdk_circuit_new(1, 0, &raw mut circuit, &raw mut error),
            QDK_OK
        );
        let owner = &(*circuit).inner;
        let options = QdkBodyOptions {
            kind: QDK_BODY_BOX,
            ..Default::default()
        };
        let mut body = ptr::null_mut();
        let mut child = ptr::null_mut();
        assert_eq!(
            qdk_circuit_begin(
                circuit,
                ptr::null(),
                &raw const options,
                &raw mut body,
                &raw mut error
            ),
            QDK_OK
        );
        assert_eq!(
            qdk_circuit_begin(
                circuit,
                body,
                &raw const options,
                &raw mut child,
                &raw mut error
            ),
            QDK_OK
        );
        assert_eq!(
            qdk_builder_free(&raw mut child, &raw mut error),
            QDK_UNFINISHED
        );
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(handle::live_handles(), before + 3);

        assert_eq!(
            boundary(&raw mut error, None, || {
                let mut value = owner.access()?;
                value.store_mut().set_global_phase(1.0);
                panic!("injected mutation panic")
            }),
            QDK_INTERNAL_ERROR
        );
        assert!(message(error).contains("panic"));
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(owner.access().err().expect("poisoned").status, QDK_POISONED);
        assert_eq!(qdk_builder_cancel(body, &raw mut error), QDK_POISONED);
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(
            qdk_circuit_free(&raw mut circuit, &raw mut error),
            QDK_IN_USE
        );
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(qdk_builder_free(&raw mut body, &raw mut error), QDK_IN_USE);
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(qdk_builder_free(&raw mut child, &raw mut error), QDK_OK);
        assert_eq!(qdk_builder_free(&raw mut body, &raw mut error), QDK_OK);
        assert!(child.is_null() && body.is_null());
        assert_eq!(handle::live_handles(), before + 1);
        assert!(owner.inspect_inactive().store().is_empty());
        assert!(
            owner.inspect_inactive().require_closed("save").is_err(),
            "teardown must not cancel native construction"
        );
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
        assert!(circuit.is_null());
        assert_eq!(handle::live_handles(), before);
    }
}

#[test]
fn completed_builder_cannot_be_finished_twice() {
    let mut circuit = circuit();
    let mut body = ptr::null_mut();
    let options = QdkBodyOptions {
        kind: QDK_BODY_BOX,
        ..Default::default()
    };
    let mut error = ptr::null_mut();
    // SAFETY: this test owns all handles and provides valid disjoint outputs.
    unsafe {
        assert_eq!(
            qdk_circuit_begin(
                circuit,
                ptr::null(),
                &raw const options,
                &raw mut body,
                &raw mut error
            ),
            QDK_OK
        );
        assert_eq!(qdk_builder_finish(body, &raw mut error), QDK_OK);
        assert_eq!(qdk_builder_finish(body, &raw mut error), QDK_INVALID_STATE);
        qdk_diagnostic_free(&raw mut error);
        let mut info = QdkCircuitInfo::default();
        assert_eq!(
            qdk_circuit_info(circuit, &raw mut info, &raw mut error),
            QDK_OK
        );
        assert_eq!(info.instructions, 1);
        assert_eq!(qdk_builder_free(&raw mut body, &raw mut error), QDK_OK);
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
    }
}

#[test]
fn open_body_export_returns_no_partial_output() {
    let mut circuit = circuit();
    let mut builder = ptr::null_mut();
    let options = QdkBodyOptions {
        kind: QDK_BODY_BOX,
        ..Default::default()
    };
    let mut bytes = QdkBytes {
        data: ptr::dangling_mut(),
        len: usize::MAX,
    };
    let mut error = ptr::null_mut();
    // SAFETY: live handles and disjoint output slots; the sentinel is overwritten.
    unsafe {
        assert_eq!(
            qdk_circuit_begin(
                circuit,
                ptr::null(),
                &raw const options,
                &raw mut builder,
                &raw mut error
            ),
            QDK_OK
        );
        assert_eq!(
            qdk_circuit_export(
                circuit,
                QDK_FORMAT_CIRCUIT_BINARY,
                0,
                &raw mut bytes,
                &raw mut error
            ),
            QDK_UNFINISHED
        );
        assert!(bytes.data.is_null() && bytes.len == 0);
        assert!(message(error).contains("control-flow block is open"));
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(qdk_builder_cancel(builder, &raw mut error), QDK_OK);
        assert_eq!(qdk_builder_free(&raw mut builder, &raw mut error), QDK_OK);
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
    }
}

fn message(diagnostic: *const QdkDiagnostic) -> String {
    let mut bytes = QdkBytes::default();
    // SAFETY: test owns the diagnostic and provides valid disjoint outputs.
    unsafe {
        assert_eq!(
            qdk_diagnostic_message(diagnostic, &raw mut bytes, ptr::null_mut()),
            QDK_OK
        );
        let text = if bytes.len == 0 {
            String::new()
        } else {
            String::from_utf8(slice::from_raw_parts(bytes.data, bytes.len).to_vec()).expect("UTF-8")
        };
        qdk_bytes_free(&raw mut bytes);
        text
    }
}

#[test]
fn null_input_returns_diagnostic_and_empty_output() {
    let mut output = ptr::dangling_mut::<QdkCircuit>();
    let mut error = ptr::null_mut();
    // SAFETY: output/error slots are valid; the sentinel is never dereferenced.
    unsafe {
        assert_eq!(
            qdk_circuit_clone(ptr::null(), &raw mut output, &raw mut error),
            QDK_INVALID_ARGUMENT
        );
        assert!(output.is_null());
        assert!(message(error).contains("circuit must be non-NULL"));
        qdk_diagnostic_free(&raw mut error);
    }
}

#[test]
fn empty_array_accepts_null_data() {
    let mut diagnostic = ptr::null_mut();
    // SAFETY: a zero-length input needs no allocation; output slot is valid.
    unsafe {
        assert_eq!(
            qdk_diagnostic_new(ptr::null(), 0, &raw mut diagnostic, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(message(diagnostic), "");
        qdk_diagnostic_free(&raw mut diagnostic);
    }
}

#[test]
fn invalid_utf8_is_reported_without_producing_a_message() {
    let data = [0xff];
    let mut diagnostic = ptr::dangling_mut();
    let mut error = ptr::null_mut();
    // SAFETY: input and output slots are live and disjoint.
    unsafe {
        assert_eq!(
            qdk_diagnostic_new(
                data.as_ptr(),
                data.len(),
                &raw mut diagnostic,
                &raw mut error
            ),
            QDK_INVALID_ARGUMENT
        );
        assert!(diagnostic.is_null());
        assert!(message(error).contains("not UTF-8"));
        qdk_diagnostic_free(&raw mut error);
    }
}

#[test]
fn length_overflow_is_rejected_before_accessing_data() {
    let mut diagnostic = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: the oversized length is rejected before the dangling sentinel
    // could be read; outputs are valid.
    unsafe {
        assert_eq!(
            qdk_diagnostic_new(
                ptr::dangling(),
                usize::MAX,
                &raw mut diagnostic,
                &raw mut error
            ),
            QDK_INVALID_ARGUMENT
        );
        assert!(diagnostic.is_null());
        assert!(message(error).contains("addressable range"));
        qdk_diagnostic_free(&raw mut error);
    }
}

#[test]
fn diagnostic_copy_reports_capacity_and_initializes_only_the_message() {
    let mut diagnostic = ptr::null_mut();
    let mut needed = 0;
    let mut error = ptr::null_mut();
    let mut buffer = [b'?'; 3];
    // SAFETY: all buffers and output slots are live and disjoint.
    unsafe {
        assert_eq!(
            qdk_diagnostic_new(b"failure".as_ptr(), 7, &raw mut diagnostic, ptr::null_mut()),
            QDK_OK
        );
        assert_eq!(
            qdk_diagnostic_copy(
                diagnostic,
                buffer.as_mut_ptr(),
                buffer.len(),
                &raw mut needed,
                &raw mut error
            ),
            QDK_BUFFER_TOO_SMALL
        );
        assert_eq!(needed, 7);
        assert_eq!(buffer, [b'?'; 3]);
        qdk_diagnostic_free(&raw mut error);
        let mut destination = [std::mem::MaybeUninit::<u8>::uninit(); 16];
        destination[15].write(b'?');
        assert_eq!(
            qdk_diagnostic_copy(
                diagnostic,
                destination.as_mut_ptr().cast(),
                destination.len(),
                &raw mut needed,
                &raw mut error
            ),
            QDK_OK
        );
        assert_eq!(needed, 7);
        assert!(error.is_null());
        assert_eq!(
            slice::from_raw_parts(destination.as_ptr().cast::<u8>(), needed),
            b"failure"
        );
        assert_eq!(destination[15].assume_init(), b'?');
        qdk_diagnostic_free(&raw mut diagnostic);
    }
}

#[test]
fn active_owner_cannot_be_reentered_or_freed() {
    let mut circuit = ptr::null_mut();
    let mut copy = ptr::dangling_mut();
    let mut error = ptr::null_mut();
    // SAFETY: this test owns the handle and slots. It keeps the handle live
    // until the active guard is dropped and never dereferences the sentinel.
    unsafe {
        assert_eq!(
            qdk_circuit_new(1, 0, &raw mut circuit, &raw mut error),
            QDK_OK
        );
        let original = circuit;
        let guard = (*circuit)
            .inner
            .access()
            .unwrap_or_else(|_| panic!("inactive circuit"));
        assert_eq!(
            qdk_circuit_clone(circuit, &raw mut copy, &raw mut error),
            QDK_ACTIVE
        );
        assert!(copy.is_null());
        qdk_diagnostic_free(&raw mut error);
        assert_eq!(
            qdk_circuit_free(&raw mut circuit, &raw mut error),
            QDK_ACTIVE
        );
        assert_eq!(circuit, original);
        qdk_diagnostic_free(&raw mut error);
        drop(guard);
        assert_eq!(qdk_circuit_free(&raw mut circuit, &raw mut error), QDK_OK);
        assert!(circuit.is_null());
    }
}
