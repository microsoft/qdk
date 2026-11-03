// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"

static int contains(QdkBytes bytes, const char *needle) {
    size_t length = strlen(needle);
    if (length > bytes.len) return 0;
    for (size_t i = 0; i <= bytes.len - length; ++i) {
        if (memcmp(bytes.data + i, needle, length) == 0) return 1;
    }
    return 0;
}

static void complete_snapshot_roundtrip_retains_dynamic_and_symbolic_data(void) {
    QdkRegisterInput registers[] = {
        {QDK_REGISTER_QUANTUM, (const uint8_t *)"q", 1, 2},
        {QDK_REGISTER_CLASSICAL, (const uint8_t *)"read", 4, 1}
    };
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = NULL, *copy = NULL;
    require_success(qdk_circuit_from_registers(registers, 2, 0, &circuit, &error), &error);
    QdkExpression *condition = true_expression(circuit);
    QdkBuilder *body = begin_body(circuit, NULL, QDK_BODY_IF, condition);
    QdkParameter *theta = NULL;
    require_success(qdk_circuit_parameter(circuit, (const uint8_t *)"theta", 5, &theta, &error), &error);
    uint32_t target = 1;
    QdkAngle angle = {.kind = QDK_ANGLE_PARAMETER, .parameter = theta};
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &target, .qubit_count = 1, .params = &angle, .param_count = 1};
    require_success(qdk_builder_append(body, &operation, &error), &error);
    finish_builder(body);
    QdkBytes original = {0}, restored = {0};
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_CIRCUIT_BINARY, 0, &original, &error), &error);
    require_success(qdk_circuit_import(QDK_FORMAT_CIRCUIT_BINARY, original.data, original.len, &copy, &error), &error);
    require_success(qdk_circuit_export(copy, QDK_FORMAT_CIRCUIT_BINARY, 0, &restored, &error), &error);
    assert(original.len == restored.len && memcmp(original.data, restored.data, original.len) == 0);
    assert(original.len > 10 && memcmp(original.data, "QDKCIRCT\001\n", 10) == 0);
    qdk_bytes_free(&original);
    qdk_bytes_free(&restored);
    require_success(qdk_circuit_export(copy, QDK_FORMAT_CIRCUIT_SEXPR, 0, &restored, &error), &error);
    assert(contains(restored, "(name \"read\")") && contains(restored, "\"theta\""));
    qdk_bytes_free(&restored);
    free_circuit(&copy);
    free_builder(&body);
    require_success(qdk_parameter_free(&theta, &error), &error);
    require_success(qdk_expression_free(&condition, &error), &error);
    free_circuit(&circuit);
}

static void text_snapshot_roundtrip_matches_binary(void) {
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = NULL, *restored = NULL;
    require_success(qdk_circuit_new(1, 0, &circuit, &error), &error);
    QdkBytes text = {0}, binary = {0}, roundtrip = {0};
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_CIRCUIT_SEXPR, 0, &text, &error), &error);
    assert(contains(text, "QDKCIRCT(\n(qdk 1 "));
    require_success(qdk_circuit_import(QDK_FORMAT_CIRCUIT_SEXPR, text.data, text.len, &restored, &error), &error);
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_CIRCUIT_BINARY, 0, &binary, &error), &error);
    require_success(qdk_circuit_export(restored, QDK_FORMAT_CIRCUIT_BINARY, 0, &roundtrip, &error), &error);
    assert(binary.len == roundtrip.len && memcmp(binary.data, roundtrip.data, binary.len) == 0);
    qdk_bytes_free(&text);
    qdk_bytes_free(&binary);
    qdk_bytes_free(&roundtrip);
    free_circuit(&restored);
    free_circuit(&circuit);
}

static void qasm_import_can_compile_and_render_the_same_circuit(void) {
    const char *source = "OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[2] q;\nbit[2] c;\nh q[0];\ncx q[0], q[1];\nc = measure q;\n";
    QdkCircuit *circuit = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_import(QDK_FORMAT_QASM, (const uint8_t *)source, strlen(source), &circuit, &error), &error);
    QdkBytes bytes = {0};
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_QASM, 0, &bytes, &error), &error);
    assert(contains(bytes, "OPENQASM"));
    qdk_bytes_free(&bytes);
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_QIR, QDK_PROFILE_BASE, &bytes, &error), &error);
    assert(contains(bytes, "__quantum__qis__h__body"));
    qdk_bytes_free(&bytes);
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_TEXT, 0, &bytes, &error), &error);
    assert(bytes.len != 0 && contains(bytes, "H"));
    qdk_bytes_free(&bytes);
    require_success(qdk_circuit_export(circuit, QDK_FORMAT_DIAGRAM_JSON, 0, &bytes, &error), &error);
    assert(contains(bytes, "qubits") && !contains(bytes, "schema_version"));
    qdk_bytes_free(&bytes);
    assert(qdk_circuit_export(circuit, QDK_FORMAT_QIR, UINT32_MAX, &bytes, &error) == QDK_INVALID_ARGUMENT);
    assert(bytes.data == NULL && bytes.len == 0);
    qdk_diagnostic_free(&error);
    free_circuit(&circuit);
}

int main(void) {
    complete_snapshot_roundtrip_retains_dynamic_and_symbolic_data();
    text_snapshot_roundtrip_matches_binary();
    qasm_import_can_compile_and_render_the_same_circuit();
    puts("3 C output tests passed");
    return 0;
}
