// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Package circuit provides experimental Go bindings for the QDK circuit C API.
// Circuits require explicit Close calls and must not be used concurrently.
package circuit

/*
#cgo LDFLAGS: -lqdk_circuit_capi
#include "qdk_circuit.h"

static inline QdkStatus append_standard(
    QdkCircuit *circuit, QdkBuilder *body, uint32_t gate,
    const uint32_t *qubits, size_t qubit_count,
    const uint32_t *clbits, size_t clbit_count, QdkDiagnostic **error) {
    QdkOperation operation = {
        .kind = QDK_OPERATION_STANDARD, .gate = gate,
        .qubits = qubits, .qubit_count = qubit_count,
        .clbits = clbits, .clbit_count = clbit_count
    };
    if (body != NULL) {
        return qdk_builder_append(body, &operation, error);
    }
    return qdk_circuit_append(circuit, &operation, error);
}
*/
import "C"

import (
	"errors"
	"fmt"
	"unsafe"
)

// ErrClosed indicates a closed circuit or a body used outside its callback.
var ErrClosed = errors.New("circuit handle is closed")

// Error contains a native status code and its diagnostic.
type Error struct {
	Code    uint32
	Message string
}

func (e *Error) Error() string {
	return fmt.Sprintf("QDK status %d: %s", e.Code, e.Message)
}

// Profile selects the capabilities available when compiling QIR.
type Profile uint32

const (
	Base         Profile = C.QDK_PROFILE_BASE
	AdaptiveRI   Profile = C.QDK_PROFILE_ADAPTIVE_RI
	AdaptiveRIF  Profile = C.QDK_PROFILE_ADAPTIVE_RIF
	Adaptive     Profile = C.QDK_PROFILE_ADAPTIVE
	Unrestricted Profile = C.QDK_PROFILE_UNRESTRICTED
)

// Circuit owns a native circuit. Copies share its lifetime; use Clone to create
// an independent owner. The zero value is closed. Methods are not thread-safe.
type Circuit struct {
	// A shared slot makes Close invalidate value copies as well as pointer aliases.
	native **C.QdkCircuit
}

// Body is valid only during the callback passed to IfOne.
type Body struct {
	native **C.QdkBuilder
}

func index(value int) (C.uint32_t, error) {
	if value < 0 || uint64(value) > uint64(^uint32(0)) {
		return 0, fmt.Errorf("circuit index or dimension %d is outside uint32 range", value)
	}
	return C.uint32_t(value), nil
}

func indices(values []int) ([]C.uint32_t, error) {
	result := make([]C.uint32_t, len(values))
	for i, value := range values {
		converted, err := index(value)
		if err != nil {
			return nil, err
		}
		result[i] = converted
	}
	return result, nil
}

func copyBytes(bytes C.QdkBytes) ([]byte, error) {
	if uint64(bytes.len) > uint64(^uint(0)>>1) {
		return nil, errors.New("native buffer length exceeds Go int")
	}
	if bytes.len > 0 && bytes.data == nil {
		return nil, errors.New("nonempty native buffer has a nil data pointer")
	}
	view := unsafe.Slice((*byte)(unsafe.Pointer(bytes.data)), int(bytes.len))
	return append([]byte(nil), view...), nil
}

func statusError(status C.QdkStatus, diagnostic *C.QdkDiagnostic) error {
	if diagnostic != nil {
		defer C.qdk_diagnostic_free(&diagnostic)
	}
	if status == C.QDK_OK {
		if diagnostic != nil {
			return errors.New("QDK returned a diagnostic on success")
		}
		return nil
	}
	if diagnostic == nil {
		return &Error{Code: uint32(status), Message: "no diagnostic returned"}
	}
	var message C.QdkBytes
	defer C.qdk_bytes_free(&message)
	if C.qdk_diagnostic_message(diagnostic, &message, nil) != C.QDK_OK {
		return &Error{Code: uint32(status), Message: "could not read diagnostic"}
	}
	data, err := copyBytes(message)
	if err != nil {
		return fmt.Errorf("read diagnostic for QDK status %d: %w", status, err)
	}
	return &Error{Code: uint32(status), Message: string(data)}
}

func (c *Circuit) handle() (*C.QdkCircuit, error) {
	if c == nil || c.native == nil || *c.native == nil {
		return nil, ErrClosed
	}
	return *c.native, nil
}

// New allocates a circuit with zero-based qubit and classical-bit indices.
func New(qubits, clbits int) (*Circuit, error) {
	q, err := index(qubits)
	if err != nil {
		return nil, err
	}
	bits, err := index(clbits)
	if err != nil {
		return nil, err
	}
	var native *C.QdkCircuit
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_new(q, bits, &native, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	return &Circuit{native: &native}, nil
}

// Close releases the circuit. Repeated calls are harmless. A refused release
// (for example during IfOne) returns an error and leaves the circuit usable.
func (c *Circuit) Close() error {
	if c == nil || c.native == nil {
		return nil
	}
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_free(c.native, &diagnostic)
	return statusError(status, diagnostic)
}

// Clone returns an independent circuit that must be closed separately.
func (c *Circuit) Clone() (*Circuit, error) {
	handle, err := c.handle()
	if err != nil {
		return nil, err
	}
	var native *C.QdkCircuit
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_clone(handle, &native, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	return &Circuit{native: &native}, nil
}

func appendGate(circuit *C.QdkCircuit, body *C.QdkBuilder, gate C.uint32_t, qubits, clbits []int) error {
	qs, err := indices(qubits)
	if err != nil {
		return err
	}
	bits, err := indices(clbits)
	if err != nil {
		return err
	}
	var diagnostic *C.QdkDiagnostic
	status := C.append_standard(
		circuit, body, gate,
		unsafe.SliceData(qs), C.size_t(len(qs)),
		unsafe.SliceData(bits), C.size_t(len(bits)), &diagnostic,
	)
	return statusError(status, diagnostic)
}

func (c *Circuit) append(gate C.uint32_t, qubits, clbits []int) error {
	handle, err := c.handle()
	if err != nil {
		return err
	}
	return appendGate(handle, nil, gate, qubits, clbits)
}

// H appends a Hadamard gate.
func (c *Circuit) H(qubit int) error { return c.append(C.QDK_GATE_H, []int{qubit}, nil) }

// X appends a Pauli X gate.
func (c *Circuit) X(qubit int) error { return c.append(C.QDK_GATE_X, []int{qubit}, nil) }

// CX appends a controlled X gate.
func (c *Circuit) CX(control, target int) error {
	return c.append(C.QDK_GATE_CX, []int{control, target}, nil)
}

// Measure measures a qubit in the Z basis into a classical bit.
func (c *Circuit) Measure(qubit, clbit int) error {
	return c.append(C.QDK_GATE_MEASURE_Z, []int{qubit}, []int{clbit})
}

func (b *Body) append(gate C.uint32_t, qubits, clbits []int) error {
	if b == nil || b.native == nil || *b.native == nil {
		return ErrClosed
	}
	return appendGate(nil, *b.native, gate, qubits, clbits)
}

// H appends a Hadamard gate to the body.
func (b *Body) H(qubit int) error { return b.append(C.QDK_GATE_H, []int{qubit}, nil) }

// X appends a Pauli X gate to the body.
func (b *Body) X(qubit int) error { return b.append(C.QDK_GATE_X, []int{qubit}, nil) }

// CX appends a controlled X gate to the body.
func (b *Body) CX(control, target int) error {
	return b.append(C.QDK_GATE_CX, []int{control, target}, nil)
}

// Measure appends a Z-basis measurement to the body.
func (b *Body) Measure(qubit, clbit int) error {
	return b.append(C.QDK_GATE_MEASURE_Z, []int{qubit}, []int{clbit})
}

// IfOne appends a conditional body executed when clbit is One. The callback
// must use Body to append operations and return any error. Failure cancels the
// unfinished body. Builders and expressions are released before this returns.
func (c *Circuit) IfOne(clbit int, apply func(*Body) error) (err error) {
	handle, err := c.handle()
	if err != nil {
		return err
	}
	bit, err := index(clbit)
	if err != nil {
		return err
	}
	if apply == nil {
		return errors.New("IfOne requires a body callback")
	}
	var diagnostic *C.QdkDiagnostic
	var condition *C.QdkExpression
	input := C.QdkExpressionInput{kind: C.QDK_EXPR_CLBIT, clbit: bit}
	status := C.qdk_circuit_expression(handle, &input, &condition, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return err
	}
	defer func() {
		status := C.qdk_expression_free(&condition, &diagnostic)
		err = errors.Join(err, statusError(status, diagnostic))
	}()
	var native *C.QdkBuilder
	options := C.QdkBodyOptions{kind: C.QDK_BODY_IF, condition: condition}
	status = C.qdk_circuit_begin(handle, nil, &options, &native, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return err
	}
	finished := false
	defer func() {
		if !finished {
			status := C.qdk_builder_cancel(native, &diagnostic)
			err = errors.Join(err, statusError(status, diagnostic))
		}
		status := C.qdk_builder_free(&native, &diagnostic)
		err = errors.Join(err, statusError(status, diagnostic))
	}()
	if err := apply(&Body{native: &native}); err != nil {
		return err
	}
	status = C.qdk_builder_finish(native, &diagnostic)
	err = statusError(status, diagnostic)
	finished = err == nil
	return err
}

func (c *Circuit) export(format C.uint32_t, profile Profile) ([]byte, error) {
	handle, err := c.handle()
	if err != nil {
		return nil, err
	}
	var bytes C.QdkBytes
	defer C.qdk_bytes_free(&bytes)
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_export(handle, format, C.uint32_t(profile), &bytes, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	return copyBytes(bytes)
}

// MarshalBinary returns an owned Go copy of the exact binary snapshot.
func (c *Circuit) MarshalBinary() ([]byte, error) {
	return c.export(C.QDK_FORMAT_CIRCUIT_BINARY, 0)
}

// MarshalText returns an owned Go copy of the UTF-8 S-expression snapshot.
func (c *Circuit) MarshalText() ([]byte, error) {
	return c.export(C.QDK_FORMAT_CIRCUIT_SEXPR, 0)
}

// QIR compiles the circuit for the selected target profile.
func (c *Circuit) QIR(profile Profile) (string, error) {
	data, err := c.export(C.QDK_FORMAT_QIR, profile)
	return string(data), err
}

func importCircuit(format C.uint32_t, data []byte) (*Circuit, error) {
	var native *C.QdkCircuit
	var diagnostic *C.QdkDiagnostic
	status := C.qdk_circuit_import(format, (*C.uint8_t)(unsafe.Pointer(unsafe.SliceData(data))),
		C.size_t(len(data)), &native, &diagnostic)
	if err := statusError(status, diagnostic); err != nil {
		return nil, err
	}
	return &Circuit{native: &native}, nil
}

// FromBinary restores a circuit from a binary snapshot. Bound untrusted input
// size before calling; circuit import has no caller-independent byte limit.
func FromBinary(data []byte) (*Circuit, error) {
	return importCircuit(C.QDK_FORMAT_CIRCUIT_BINARY, data)
}

// FromText restores a circuit from a UTF-8 S-expression snapshot. Bound
// untrusted input size before calling.
func FromText(data []byte) (*Circuit, error) {
	return importCircuit(C.QDK_FORMAT_CIRCUIT_SEXPR, data)
}
