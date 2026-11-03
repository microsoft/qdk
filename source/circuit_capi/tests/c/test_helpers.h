// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#ifndef QDK_CAPI_TEST_HELPERS_H
#define QDK_CAPI_TEST_HELPERS_H

#include "qdk_circuit.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static inline void require_success(QdkStatus status, QdkDiagnostic **error) {
    if (status != QDK_OK) {
        QdkBytes message = {0};
        if (qdk_diagnostic_message(*error, &message, NULL) == QDK_OK) {
            fwrite(message.data, 1, message.len, stderr);
            fputc('\n', stderr);
            qdk_bytes_free(&message);
        }
        qdk_diagnostic_free(error);
        abort();
    }
    assert(*error == NULL);
}

static inline QdkCircuit *new_circuit(uint32_t qubits, uint32_t clbits) {
    QdkCircuit *circuit = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_new(qubits, clbits, &circuit, &error), &error);
    return circuit;
}

static inline void free_circuit(QdkCircuit **circuit) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_free(circuit, &error), &error);
}

static inline QdkInstruction *instruction_at(QdkCircuit *circuit, size_t index) {
    QdkInstruction *instruction = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_instruction(circuit, index, &instruction, &error), &error);
    return instruction;
}

static inline QdkCircuit *snapshot_roundtrip(QdkCircuit *circuit) {
    QdkDiagnostic *error = NULL;
    QdkBytes bytes = {0};
    QdkCircuit *restored = NULL;
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_CIRCUIT_BINARY, 0, &bytes, &error), &error);
    require_success(qdk_circuit_import(QDK_FORMAT_CIRCUIT_BINARY, bytes.data, bytes.len, &restored, &error), &error);
    qdk_bytes_free(&bytes);
    return restored;
}

static inline void free_instruction(QdkInstruction **instruction) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_instruction_free(instruction, &error), &error);
}

static inline void expect_name(QdkInstruction *instruction, const char *name) {
    QdkBytes bytes = {0};
    QdkDiagnostic *error = NULL;
    require_success(qdk_instruction_name(instruction, &bytes, &error), &error);
    assert(bytes.len == strlen(name));
    assert(memcmp(bytes.data, name, bytes.len) == 0);
    qdk_bytes_free(&bytes);
}

static inline QdkExpression *make_expression(QdkCircuit *circuit, QdkExpressionInput input) {
    QdkExpression *expression = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_expression(circuit, &input, &expression, &error), &error);
    return expression;
}

static inline QdkExpression *true_expression(QdkCircuit *circuit) {
    QdkExpressionInput input = {.kind = QDK_EXPR_LITERAL,
        .literal = {.kind = QDK_VALUE_BOOL, .boolean = 1}};
    return make_expression(circuit, input);
}

static inline QdkBuilder *begin_body(QdkCircuit *circuit, QdkBuilder *parent,
                                    uint32_t kind, QdkExpression *condition) {
    QdkBuilder *builder = NULL;
    QdkDiagnostic *error = NULL;
    QdkBodyOptions options = {.kind = kind, .condition = condition};
    require_success(qdk_circuit_begin(circuit, parent, &options, &builder, &error), &error);
    return builder;
}

static inline void finish_builder(QdkBuilder *builder) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_builder_finish(builder, &error), &error);
}

static inline void free_builder(QdkBuilder **builder) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_builder_free(builder, &error), &error);
}

static inline void append_body_gate(QdkBuilder *builder, uint32_t gate, uint32_t qubit) {
    QdkDiagnostic *error = NULL;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = gate,
        .qubits = &qubit, .qubit_count = 1};
    require_success(qdk_builder_append(builder, &operation, &error), &error);
}

static inline QdkInstruction *child_instruction(QdkCircuit *circuit,
                                               QdkInstruction *parent,
                                               size_t branch, size_t index) {
    QdkDiagnostic *error = NULL;
    QdkBlock *block = NULL;
    QdkInstruction *instruction = NULL;
    require_success(qdk_instruction_block(parent, circuit, branch, &block, &error), &error);
    require_success(qdk_block_instruction(block, index, &instruction, &error), &error);
    require_success(qdk_block_free(&block, &error), &error);
    return instruction;
}

#endif
