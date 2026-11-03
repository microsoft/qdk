// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "qdk_circuit.h"
#include <string.h>

_Static_assert(QDK_PROFILE_BASE == 1 && QDK_PROFILE_ADAPTIVE_RI == 2 &&
    QDK_PROFILE_ADAPTIVE_RIF == 3 && QDK_PROFILE_ADAPTIVE == 4 &&
    QDK_PROFILE_UNRESTRICTED == 5, "update the Fortran profile constants");

static int64_t invalid(QdkDiagnostic **error, const char *message) {
    QdkStatus status = qdk_diagnostic_new((const uint8_t *)message, strlen(message), error, NULL);
    return status == QDK_OK ? QDK_INVALID_ARGUMENT : status;
}

static int index_ok(int64_t value) {
    return value >= 0 && (uint64_t)value <= UINT32_MAX;
}

int64_t qdk_f_new(int64_t qubits, int64_t clbits, QdkCircuit **out, QdkDiagnostic **error) {
    *out = NULL;
    *error = NULL;
    if (!index_ok(qubits) || !index_ok(clbits)) {
        return invalid(error, "dimensions must be nonnegative and fit uint32_t");
    }
    return qdk_circuit_new((uint32_t)qubits, (uint32_t)clbits, out, error);
}

int64_t qdk_f_clone(const QdkCircuit *c, QdkCircuit **out, QdkDiagnostic **e) {
    return qdk_circuit_clone(c, out, e);
}

int64_t qdk_f_close(QdkCircuit **c, QdkDiagnostic **e) {
    return qdk_circuit_free(c, e);
}

int64_t qdk_f_append(QdkCircuit *c, QdkBuilder *body, int64_t operation,
                    int64_t first, int64_t second, int64_t bit, QdkDiagnostic **e) {
    *e = NULL;
    if (!index_ok(first) || !index_ok(second) || !index_ok(bit)) {
        return invalid(e, "indices must be nonnegative and fit uint32_t");
    }
    uint32_t qubits[] = {(uint32_t)first, (uint32_t)second};
    uint32_t clbit = (uint32_t)bit;
    QdkOperation op = {.kind = QDK_OPERATION_STANDARD, .qubits = qubits, .qubit_count = 1};
    switch (operation) {
        case 0: op.gate = QDK_GATE_H; break;
        case 1: op.gate = QDK_GATE_X; break;
        case 2: op.gate = QDK_GATE_CX; op.qubit_count = 2; break;
        case 3:
            op.gate = QDK_GATE_MEASURE_Z;
            op.clbits = &clbit;
            op.clbit_count = 1;
            break;
        default: return invalid(e, "unknown Fortran binding operation");
    }
    return body ? qdk_builder_append(body, &op, e) : qdk_circuit_append(c, &op, e);
}

int64_t qdk_f_clbit(QdkCircuit *c, int64_t bit, QdkExpression **out, QdkDiagnostic **e) {
    *out = NULL;
    *e = NULL;
    if (!index_ok(bit)) return invalid(e, "clbit must be nonnegative and fit uint32_t");
    QdkExpressionInput input = {.kind = QDK_EXPR_CLBIT, .clbit = (uint32_t)bit};
    return qdk_circuit_expression(c, &input, out, e);
}

int64_t qdk_f_begin(QdkCircuit *c, const QdkExpression *condition, QdkBuilder **out, QdkDiagnostic **e) {
    QdkBodyOptions options = {.kind = QDK_BODY_IF, .condition = condition};
    return qdk_circuit_begin(c, NULL, &options, out, e);
}

int64_t qdk_f_finish(QdkBuilder *body, QdkDiagnostic **e) { return qdk_builder_finish(body, e); }
int64_t qdk_f_cancel(QdkBuilder *body, QdkDiagnostic **e) { return qdk_builder_cancel(body, e); }
int64_t qdk_f_body_free(QdkBuilder **body, QdkDiagnostic **e) { return qdk_builder_free(body, e); }
int64_t qdk_f_expression_free(QdkExpression **expression, QdkDiagnostic **e) {
    return qdk_expression_free(expression, e);
}

int64_t qdk_f_export(const QdkCircuit *c, int64_t format, int64_t profile, QdkBytes *out, QdkDiagnostic **e) {
    *out = (QdkBytes){0};
    *e = NULL;
    uint32_t tag;
    switch (format) {
        case 0: tag = QDK_FORMAT_CIRCUIT_BINARY; break;
        case 1: tag = QDK_FORMAT_CIRCUIT_SEXPR; break;
        case 2: tag = QDK_FORMAT_QIR; break;
        default: return invalid(e, "unknown Fortran binding export format");
    }
    if (!index_ok(profile)) return invalid(e, "profile must be nonnegative and fit uint32_t");
    return qdk_circuit_export(c, tag, (uint32_t)profile, out, e);
}

int64_t qdk_f_import(int64_t text, const uint8_t *data, size_t len, QdkCircuit **out, QdkDiagnostic **e) {
    return qdk_circuit_import(text ? QDK_FORMAT_CIRCUIT_SEXPR : QDK_FORMAT_CIRCUIT_BINARY, data, len, out, e);
}

int64_t qdk_f_message(const QdkDiagnostic *e, QdkBytes *out) {
    return qdk_diagnostic_message(e, out, NULL);
}
