// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package capi

/*
#include "qdk_circuit.h"
#include <stdlib.h>
QdkStatus register_go_pass(uint32_t, uintptr_t, QdkGraphPass **, QdkDiagnostic **);
QdkStatus append_h(QdkCircuit *, QdkDiagnostic **);
QdkStatus append_rx(QdkCircuit *, QdkParameter *, QdkDiagnostic **);
QdkStatus append_measure(QdkCircuit *, uint32_t, uint32_t, QdkDiagnostic **);
QdkStatus append_body_x(QdkBuilder *, uint32_t, QdkDiagnostic **);
QdkStatus replace_with_x(QdkPassDecision **, QdkDiagnostic **);
*/
import "C"

import (
	"errors"
	"fmt"
	"os"
	"runtime/cgo"
	"unsafe"
)

type circuit struct{ native *C.QdkCircuit }

const (
	profileBase       uint32 = C.QDK_PROFILE_BASE
	profileAdaptiveRI uint32 = C.QDK_PROFILE_ADAPTIVE_RI
	profileAdaptive   uint32 = C.QDK_PROFILE_ADAPTIVE
)

type graph struct{ native *C.QdkGraph }
type registration struct {
	native *C.QdkGraphPass
	token  cgo.Handle
}
type callbackState struct {
	mode          string
	payload       []byte
	calls         int
	nativeDeletes int
	callerDeletes int
	operation     string
}

func copied(bytes C.QdkBytes) []byte {
	return append([]byte(nil), unsafe.Slice((*byte)(unsafe.Pointer(bytes.data)), int(bytes.len))...)
}

func statusError(status C.QdkStatus, diagnostic *C.QdkDiagnostic) error {
	if diagnostic != nil {
		defer C.qdk_diagnostic_free(&diagnostic)
	}
	if status == C.QDK_OK {
		if diagnostic != nil {
			return fmt.Errorf("successful call returned a diagnostic")
		}
		return nil
	}
	if diagnostic == nil {
		return fmt.Errorf("native status %d without diagnostic", status)
	}
	var message C.QdkBytes
	if C.qdk_diagnostic_message(diagnostic, &message, nil) != C.QDK_OK {
		return fmt.Errorf("could not read diagnostic for status %d", status)
	}
	defer C.qdk_bytes_free(&message)
	return fmt.Errorf("%s", copied(message))
}

func newCircuit() (*circuit, error) {
	value := &circuit{}
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_new(1, 0, &value.native, &diagnostic)
	return value, statusError(status, diagnostic)
}

func (value *circuit) close() error {
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_free(&value.native, &diagnostic)
	return statusError(status, diagnostic)
}

func (value *circuit) clone() (*circuit, error) {
	copy := &circuit{}
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_clone(value.native, &copy.native, &diagnostic)
	return copy, statusError(status, diagnostic)
}

func (value *circuit) size() (int, error) {
	var info C.QdkCircuitInfo
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_info(value.native, &info, &diagnostic)
	return int(info.instructions), statusError(status, diagnostic)
}

func (value *circuit) h() error {
	var diagnostic *C.QdkDiagnostic
	status := C.append_h(value.native, &diagnostic)
	return statusError(status, diagnostic)
}

func (value *circuit) snapshot() ([]byte, error) {
	return value.snapshotFormat(C.QDK_FORMAT_CIRCUIT_BINARY)
}

func (value *circuit) snapshotText() ([]byte, error) {
	return value.snapshotFormat(C.QDK_FORMAT_CIRCUIT_SEXPR)
}

func (value *circuit) snapshotFormat(format C.uint32_t) ([]byte, error) {
	var bytes C.QdkBytes
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_export(value.native, format, 0, &bytes, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	defer C.qdk_bytes_free(&bytes)
	return copied(bytes), nil
}

func (value *circuit) qir(profile uint32) ([]byte, error) {
	var bytes C.QdkBytes
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_export(value.native, C.QDK_FORMAT_QIR, C.uint32_t(profile), &bytes, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	defer C.qdk_bytes_free(&bytes)
	return copied(bytes), nil
}

func feedbackCircuit() (value *circuit, err error) {
	value = &circuit{}
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_new(2, 2, &value.native, &diagnostic)
	if err = statusError(status, diagnostic); err != nil {
		return nil, err
	}
	defer func() {
		if err != nil {
			err = errors.Join(err, value.close())
			value = nil
		}
	}()
	if err = value.h(); err != nil {
		return value, err
	}
	status = C.append_measure(value.native, 0, 0, &diagnostic)
	if err = statusError(status, diagnostic); err != nil {
		return value, err
	}
	if err = value.feedbackX(); err != nil {
		return value, err
	}
	status = C.append_measure(value.native, 1, 1, &diagnostic)
	return value, statusError(status, diagnostic)
}

func (value *circuit) feedbackX() (err error) {
	var condition *C.QdkExpression
	var diagnostic *C.QdkDiagnostic
	input := C.QdkExpressionInput{kind: C.QDK_EXPR_CLBIT, clbit: 0}
	status := C.qdk_circuit_expression(value.native, &input, &condition, &diagnostic)
	if err = statusError(status, diagnostic); err != nil {
		return err
	}
	defer func() { err = errors.Join(err, statusError(C.qdk_expression_free(&condition, nil), nil)) }()
	var body *C.QdkBuilder
	options := C.QdkBodyOptions{kind: C.QDK_BODY_IF, condition: condition}
	status = C.qdk_circuit_begin(value.native, nil, &options, &body, &diagnostic)
	if err = statusError(status, diagnostic); err != nil {
		return err
	}
	finished := false
	defer func() {
		if !finished {
			err = errors.Join(err, statusError(C.qdk_builder_cancel(body, nil), nil))
		}
		err = errors.Join(err, statusError(C.qdk_builder_free(&body, nil), nil))
	}()
	status = C.append_body_x(body, 1, &diagnostic)
	if err = statusError(status, diagnostic); err != nil {
		return err
	}
	status = C.qdk_builder_finish(body, &diagnostic)
	finished = status == C.QDK_OK
	return statusError(status, diagnostic)
}

func (value *circuit) lower() (*graph, error) {
	result := &graph{}
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_to_graph(value.native, &result.native, &diagnostic)
	return result, statusError(status, diagnostic)
}

func (value *graph) close() error {
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_graph_free(&value.native, &diagnostic)
	return statusError(status, diagnostic)
}

func (value *graph) export(format C.uint32_t) ([]byte, error) {
	var bytes C.QdkBytes
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_graph_export(value.native, format, C.QDK_PROFILE_BASE, &bytes, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	defer C.qdk_bytes_free(&bytes)
	return copied(bytes), nil
}

func (value *graph) snapshot() ([]byte, error) { return value.export(C.QDK_FORMAT_GRAPH_BINARY) }
func (value *graph) qir() ([]byte, error)      { return value.export(C.QDK_FORMAT_QIR) }

func (value *graph) copiedOperationAfterClose() (operation string, err error) {
	var record *C.QdkGraphRecord
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_graph_record(value.native, C.QDK_GRAPH_NODE, 0, &record, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return "", err
	}
	defer func() { err = errors.Join(err, statusError(C.qdk_graph_record_free(&record, nil), nil)) }()
	if err := value.close(); err != nil {
		return "", err
	}
	var bytes C.QdkBytes
	status = C.qdk_graph_record_text(record, C.QDK_GRAPH_OPERATION, 0, &bytes, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return "", err
	}
	defer C.qdk_bytes_free(&bytes)
	return string(copied(bytes)), nil
}

func (value *circuit) symbolicConditional() (err error) {
	name := C.CString("theta")
	defer C.free(unsafe.Pointer(name))
	var parameter *C.QdkParameter
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_parameter(value.native, (*C.uint8_t)(unsafe.Pointer(name)), 5, &parameter, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return err
	}
	defer func() { err = errors.Join(err, statusError(C.qdk_parameter_free(&parameter, nil), nil)) }()
	input := C.QdkExpressionInput{kind: C.QDK_EXPR_LITERAL, literal: C.QdkLiteral{kind: C.QDK_VALUE_BOOL, boolean: 1}}
	var condition *C.QdkExpression
	status = C.qdk_circuit_expression(value.native, &input, &condition, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return err
	}
	defer func() { err = errors.Join(err, statusError(C.qdk_expression_free(&condition, nil), nil)) }()
	options := C.QdkBodyOptions{kind: C.QDK_BODY_IF, condition: condition}
	var body *C.QdkBuilder
	status = C.qdk_circuit_begin(value.native, nil, &options, &body, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return err
	}
	finished := false
	defer func() {
		if !finished {
			err = errors.Join(err, statusError(C.qdk_builder_cancel(body, nil), nil))
		}
		err = errors.Join(err, statusError(C.qdk_builder_free(&body, nil), nil))
	}()
	status = C.append_rx(value.native, parameter, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return err
	}
	status = C.qdk_builder_finish(body, &diagnostic)
	finished = status == C.QDK_OK
	return statusError(status, diagnostic)
}

func register(state *callbackState, valid bool) (*registration, error) {
	token := cgo.NewHandle(state)
	result := &registration{token: token}
	kind := C.uint32_t(C.QDK_PASS_REWRITE)
	if !valid {
		kind = ^C.uint32_t(0)
	}
	var diagnostic *C.QdkDiagnostic
	status := C.register_go_pass(kind, C.uintptr_t(token), &result.native, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		token.Delete()
		state.callerDeletes++
		return nil, err
	}
	return result, nil
}

func (value *registration) state() *callbackState {
	return value.token.Value().(*callbackState)
}

func (value *registration) close() error {
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_graph_pass_free(&value.native, &diagnostic)
	return statusError(status, diagnostic)
}

func (value *registration) run(input *graph) (output *graph, count int, err error) {
	result := &graph{}
	var report *C.QdkPassResult
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_graph_pass_run(value.native, input.native, &result.native, &report, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		if result.native != nil || report != nil {
			return nil, 0, fmt.Errorf("failure published a partial graph/report: %w", err)
		}
		return nil, 0, err
	}
	defer func() { err = errors.Join(err, statusError(C.qdk_pass_result_free(&report, nil), nil)) }()
	var info C.QdkPassResultInfo
	status = C.qdk_pass_result_info(report, &info, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, 0, errors.Join(err, result.close())
	}
	return result, int(info.callback_count), nil
}

//export goGraphCallback
func goGraphCallback(cookie C.uintptr_t, matched *C.QdkPassMatch, decision **C.QdkPassDecision, diagnostic **C.QdkDiagnostic) (status C.QdkStatus) {
	defer func() {
		if failure := recover(); failure != nil {
			if *diagnostic != nil {
				C.qdk_diagnostic_free(diagnostic)
			}
			message := fmt.Sprintf("Go callback panic: %v", failure)
			text := C.CString(message)
			defer C.free(unsafe.Pointer(text))
			C.qdk_diagnostic_new((*C.uint8_t)(unsafe.Pointer(text)), C.size_t(len(message)), diagnostic, nil)
			status = C.QDK_CALLBACK_ERROR
		}
	}()
	state := cgo.Handle(cookie).Value().(*callbackState)
	state.calls++
	state.operation = string(unsafe.Slice((*byte)(unsafe.Pointer(matched.operation.data)), int(matched.operation.len)))
	if state.mode == "panic" {
		// Transfer a fresh decision as well: the adapter must release it on failure.
		status = C.replace_with_x(decision, diagnostic)
		if status != C.QDK_OK {
			return status
		}
		panic("requested failure")
	}
	return C.replace_with_x(decision, diagnostic)
}

//export goContextFree
func goContextFree(cookie C.uintptr_t) {
	defer func() {
		if failure := recover(); failure != nil {
			fmt.Fprintf(os.Stderr, "context destructor failed: %v\n", failure)
			os.Exit(1)
		}
	}()
	handle := cgo.Handle(cookie)
	handle.Value().(*callbackState).nativeDeletes++
	handle.Delete()
}
