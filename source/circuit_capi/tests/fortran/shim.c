// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "qdk_circuit.h"
#include <limits.h>
#include <stdio.h>
#include <string.h>

static int status_code(QdkStatus status) {
    if (status > INT_MAX) {
        fprintf(stderr, "QDK status cannot be represented as a Fortran C_INT: %u\n", status);
        return -1;
    }
    return (int)status;
}

static int invalid(const char *message, QdkDiagnostic **error) {
    QdkStatus status = qdk_diagnostic_new((const uint8_t *)message, strlen(message), error, NULL);
    return status_code(status == QDK_OK ? QDK_INVALID_ARGUMENT : status);
}

static int valid_index(int value) {
    return value >= 0 && (uintmax_t)value <= UINT32_MAX;
}

int f_circuit_new(int qubits, int clbits, QdkCircuit **out, QdkDiagnostic **error) {
    *out = NULL;
    *error = NULL;
    if (!valid_index(qubits) || !valid_index(clbits))
        return invalid("circuit dimensions must fit a nonnegative uint32_t", error);
    return status_code(qdk_circuit_new((uint32_t)qubits, (uint32_t)clbits, out, error));
}

int f_circuit_clone(const QdkCircuit *circuit, QdkCircuit **out, QdkDiagnostic **error) {
    return status_code(qdk_circuit_clone(circuit, out, error));
}

int f_circuit_free(QdkCircuit **circuit, QdkDiagnostic **error) {
    return status_code(qdk_circuit_free(circuit, error));
}

int f_append_h(QdkCircuit *circuit, int qubit, QdkDiagnostic **error) {
    *error = NULL;
    if (!valid_index(qubit)) return invalid("qubit must fit a nonnegative uint32_t", error);
    uint32_t target = (uint32_t)qubit;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_H,
        .qubits = &target, .qubit_count = 1};
    return status_code(qdk_circuit_append(circuit, &operation, error));
}

int f_measure(QdkCircuit *circuit, int qubit, int clbit, QdkDiagnostic **error) {
    *error = NULL;
    if (!valid_index(qubit) || !valid_index(clbit))
        return invalid("measurement indices must fit a nonnegative uint32_t", error);
    uint32_t target = (uint32_t)qubit, result = (uint32_t)clbit;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_MEASURE_Z,
        .qubits = &target, .qubit_count = 1, .clbits = &result, .clbit_count = 1};
    return status_code(qdk_circuit_append(circuit, &operation, error));
}

int f_clbit(QdkCircuit *circuit, int clbit, QdkExpression **out, QdkDiagnostic **error) {
    *out = NULL;
    *error = NULL;
    if (!valid_index(clbit)) return invalid("clbit must fit a nonnegative uint32_t", error);
    QdkExpressionInput input = {.kind = QDK_EXPR_CLBIT, .clbit = (uint32_t)clbit};
    return status_code(qdk_circuit_expression(circuit, &input, out, error));
}

int f_expression_free(QdkExpression **expression, QdkDiagnostic **error) {
    return status_code(qdk_expression_free(expression, error));
}

int f_begin_if(QdkCircuit *circuit, const QdkExpression *condition,
               QdkBuilder **out, QdkDiagnostic **error) {
    QdkBodyOptions options = {.kind = QDK_BODY_IF, .condition = condition};
    return status_code(qdk_circuit_begin(circuit, NULL, &options, out, error));
}

int f_body_x(QdkBuilder *body, int qubit, QdkDiagnostic **error) {
    *error = NULL;
    if (!valid_index(qubit)) return invalid("qubit must fit a nonnegative uint32_t", error);
    uint32_t target = (uint32_t)qubit;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_X,
        .qubits = &target, .qubit_count = 1};
    return status_code(qdk_builder_append(body, &operation, error));
}

int f_builder_finish(QdkBuilder *body, QdkDiagnostic **error) {
    return status_code(qdk_builder_finish(body, error));
}

int f_builder_cancel(QdkBuilder *body, QdkDiagnostic **error) {
    return status_code(qdk_builder_cancel(body, error));
}

int f_builder_free(QdkBuilder **body, QdkDiagnostic **error) {
    return status_code(qdk_builder_free(body, error));
}

int f_qir_base(const QdkCircuit *circuit, QdkBytes *out, QdkDiagnostic **error) {
    return status_code(qdk_circuit_export(circuit, QDK_FORMAT_QIR, QDK_PROFILE_BASE, out, error));
}

int f_qir_adaptive_ri(const QdkCircuit *circuit, QdkBytes *out, QdkDiagnostic **error) {
    return status_code(qdk_circuit_export(circuit, QDK_FORMAT_QIR, QDK_PROFILE_ADAPTIVE_RI, out, error));
}

int f_qir_adaptive(const QdkCircuit *circuit, QdkBytes *out, QdkDiagnostic **error) {
    return status_code(qdk_circuit_export(circuit, QDK_FORMAT_QIR, QDK_PROFILE_ADAPTIVE, out, error));
}

int f_diagnostic_message(const QdkDiagnostic *error, QdkBytes *out) {
    return status_code(qdk_diagnostic_message(error, out, NULL));
}

int f_status_in_use(void) {
    return status_code(QDK_IN_USE);
}

int f_status_invalid_argument(void) {
    return status_code(QDK_INVALID_ARGUMENT);
}
