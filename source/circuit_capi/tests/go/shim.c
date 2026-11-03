// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "qdk_circuit.h"
#include <string.h>

extern QdkStatus goGraphCallback(uintptr_t, QdkPassMatch *, QdkPassDecision **, QdkDiagnostic **);
extern void goContextFree(uintptr_t);

static QdkStatus callback(uintptr_t cookie, const QdkPassMatch *matched,
                          QdkPassDecision **decision, QdkDiagnostic **error) {
    return goGraphCallback(cookie, (QdkPassMatch *)matched, decision, error);
}

QdkStatus register_go_pass(uint32_t kind, uintptr_t cookie, QdkGraphPass **out, QdkDiagnostic **error) {
    QdkString operation = {(const uint8_t *)"quantum.h", 9};
    QdkGraphPassOptions options = {.name = {(const uint8_t *)"go", 2}, .kind = kind,
        .operations = &operation, .operation_count = 1};
    return qdk_graph_pass_new(&options, callback, cookie, goContextFree, out, error);
}

QdkStatus append_h(QdkCircuit *circuit, QdkDiagnostic **error) {
    uint32_t target = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_H,
        .qubits = &target, .qubit_count = 1};
    return qdk_circuit_append(circuit, &operation, error);
}

QdkStatus append_rx(QdkCircuit *circuit, QdkParameter *parameter, QdkDiagnostic **error) {
    uint32_t target = 0;
    QdkAngle angle = {.kind = QDK_ANGLE_PARAMETER, .parameter = parameter};
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &target, .qubit_count = 1, .params = &angle, .param_count = 1};
    return qdk_circuit_append(circuit, &operation, error);
}

QdkStatus append_measure(QdkCircuit *circuit, uint32_t qubit, uint32_t clbit, QdkDiagnostic **error) {
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_MEASURE_Z,
        .qubits = &qubit, .qubit_count = 1, .clbits = &clbit, .clbit_count = 1};
    return qdk_circuit_append(circuit, &operation, error);
}

QdkStatus append_body_x(QdkBuilder *body, uint32_t qubit, QdkDiagnostic **error) {
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_X,
        .qubits = &qubit, .qubit_count = 1};
    return qdk_builder_append(body, &operation, error);
}

QdkStatus replace_with_x(QdkPassDecision **decision, QdkDiagnostic **error) {
    QdkString operation = {(const uint8_t *)"quantum.x", 9};
    return qdk_pass_decision_new(QDK_DECISION_REPLACE, &operation, 1, decision, error);
}
