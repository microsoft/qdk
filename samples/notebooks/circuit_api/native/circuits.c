// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "qdk_circuit.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void check(QdkStatus status, QdkDiagnostic **error) {
    if (status != QDK_OK) {
        QdkBytes message = {0};
        fprintf(stderr, "QDK status %u: ", status);
        if (*error && qdk_diagnostic_message(*error, &message, NULL) == QDK_OK) {
            fwrite(message.data, 1, message.len, stderr);
            qdk_bytes_free(&message);
        }
        fputc('\n', stderr);
        qdk_diagnostic_free(error);
        exit(EXIT_FAILURE);
    }
}

static void gate(QdkCircuit *circuit, uint32_t name, const uint32_t *qubits, size_t count) {
    QdkDiagnostic *error = NULL;
    QdkOperation op = {.kind = QDK_OPERATION_STANDARD, .gate = name,
        .qubits = qubits, .qubit_count = count};
    check(qdk_circuit_append(circuit, &op, &error), &error);
}

static void measure(QdkCircuit *circuit, uint32_t qubit, uint32_t clbit) {
    QdkDiagnostic *error = NULL;
    QdkOperation op = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_MEASURE_Z,
        .qubits = &qubit, .qubit_count = 1, .clbits = &clbit, .clbit_count = 1};
    check(qdk_circuit_append(circuit, &op, &error), &error);
}

static void save(QdkCircuit *circuit, uint32_t format, uint32_t profile, const char *path) {
    QdkDiagnostic *error = NULL;
    QdkBytes bytes = {0};
    check(qdk_circuit_export(circuit, format, profile, &bytes, &error), &error);
    FILE *file = fopen(path, "wb");
    if (!file) { perror(path); exit(EXIT_FAILURE); }
    if (fwrite(bytes.data, 1, bytes.len, file) != bytes.len) {
        perror("write snapshot"); exit(EXIT_FAILURE);
    }
    if (fclose(file) != 0) { perror("close snapshot"); exit(EXIT_FAILURE); }
    printf("Wrote %s (%zu bytes)\n", path, bytes.len);
    qdk_bytes_free(&bytes);
}

int main(void) {
    QdkDiagnostic *error = NULL;
    QdkCircuit *base = NULL;
    check(qdk_circuit_new(2, 2, &base, &error), &error);
    uint32_t zero = 0, one = 1, pair[] = {0, 1};
    gate(base, QDK_GATE_H, &zero, 1);
    gate(base, QDK_GATE_CX, pair, 2);
    measure(base, 0, 0);
    measure(base, 1, 1);
    save(base, QDK_FORMAT_CIRCUIT_BINARY, 0, "base.qdk");
    save(base, QDK_FORMAT_CIRCUIT_SEXPR, 0, "base.sexpr");
    save(base, QDK_FORMAT_QIR, QDK_PROFILE_BASE, "base.ll");
    check(qdk_circuit_free(&base, &error), &error);

    QdkCircuit *adaptive = NULL;
    check(qdk_circuit_new(2, 2, &adaptive, &error), &error);
    gate(adaptive, QDK_GATE_H, &zero, 1);
    measure(adaptive, 0, 0);
    QdkExpressionInput input = {.kind = QDK_EXPR_CLBIT, .clbit = 0};
    QdkExpression *condition = NULL;
    check(qdk_circuit_expression(adaptive, &input, &condition, &error), &error);
    QdkBodyOptions options = {.kind = QDK_BODY_IF, .condition = condition};
    QdkBuilder *body = NULL;
    check(qdk_circuit_begin(adaptive, NULL, &options, &body, &error), &error);
    QdkOperation correction = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_X,
        .qubits = &one, .qubit_count = 1};
    check(qdk_builder_append(body, &correction, &error), &error);
    check(qdk_builder_finish(body, &error), &error);
    // Dependent handles are released before their owning circuit.
    check(qdk_builder_free(&body, &error), &error);
    check(qdk_expression_free(&condition, &error), &error);
    measure(adaptive, 1, 1);
    save(adaptive, QDK_FORMAT_CIRCUIT_BINARY, 0, "adaptive.qdk");
    save(adaptive, QDK_FORMAT_CIRCUIT_SEXPR, 0, "adaptive.sexpr");
    save(adaptive, QDK_FORMAT_QIR, QDK_PROFILE_ADAPTIVE, "adaptive.ll");
    check(qdk_circuit_free(&adaptive, &error), &error);

    // A native symbolic template can be bound and simulated later in Python.
    QdkCircuit *symbolic = NULL;
    QdkParameter *theta = NULL;
    check(qdk_circuit_new(1, 1, &symbolic, &error), &error);
    check(qdk_circuit_parameter(symbolic, (const uint8_t *)"theta", 5, &theta, &error), &error);
    QdkAngle angle = {.kind = QDK_ANGLE_PARAMETER, .parameter = theta};
    QdkOperation rotation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &zero, .qubit_count = 1, .params = &angle, .param_count = 1};
    check(qdk_circuit_append(symbolic, &rotation, &error), &error);
    measure(symbolic, 0, 0);
    save(symbolic, QDK_FORMAT_CIRCUIT_BINARY, 0, "symbolic.qdk");
    save(symbolic, QDK_FORMAT_CIRCUIT_SEXPR, 0, "symbolic.sexpr");
    check(qdk_parameter_free(&theta, &error), &error);
    check(qdk_circuit_free(&symbolic, &error), &error);
    return EXIT_SUCCESS;
}
