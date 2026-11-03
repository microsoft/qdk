// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"

static size_t calls;
static size_t drops;
static QdkGraphPass *active_pass;
static QdkGraph *active_graph;

static QdkString text(const char *value) {
    return (QdkString){(const uint8_t *)value, strlen(value)};
}

static int named(QdkString actual, const char *expected) {
    return actual.len == strlen(expected) && memcmp(actual.data, expected, actual.len) == 0;
}

static void destroy_context(uintptr_t cookie) {
    assert(cookie == 123);
    drops++;
}

static void append_gate(QdkCircuit *circuit, uint32_t gate) {
    QdkDiagnostic *error = NULL;
    uint32_t qubit = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = gate, .qubits = &qubit, .qubit_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
}

static QdkGraph *lower_and_free(QdkCircuit **circuit) {
    QdkDiagnostic *error = NULL;
    QdkGraph *graph = NULL;
    require_success(qdk_circuit_to_graph(*circuit, &graph, &error), &error);
    free_circuit(circuit);
    return graph;
}

static void free_graph(QdkGraph **graph) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_graph_free(graph, &error), &error);
}

static QdkCircuit *raise_graph(QdkGraph *graph) {
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = NULL;
    QdkGraphOrigins *origins = NULL;
    require_success(qdk_graph_to_circuit(graph, &circuit, &origins, &error), &error);
    require_success(qdk_graph_origins_free(&origins, &error), &error);
    return circuit;
}

static QdkStatus patch(uintptr_t cookie, const QdkPassMatch *matched,
                       QdkPassDecision **decision, QdkDiagnostic **error) {
    assert(cookie == 123 && matched->operands == 1);
    calls++;
    QdkDiagnostic *reentry_error = NULL;
    QdkGraph *output = NULL;
    QdkPassResult *result = NULL;
    assert(qdk_graph_free(&active_graph, &reentry_error) == QDK_ACTIVE);
    qdk_diagnostic_free(&reentry_error);
    assert(qdk_graph_pass_free(&active_pass, &reentry_error) == QDK_ACTIVE);
    qdk_diagnostic_free(&reentry_error);
    assert(qdk_graph_pass_run(active_pass, active_graph, &output, &result, &reentry_error) == QDK_ACTIVE);
    assert(output == NULL && result == NULL);
    qdk_diagnostic_free(&reentry_error);
    if (named(matched->operation, "quantum.x")) {
        QdkString operation = text("quantum.y");
        return qdk_pass_decision_new(QDK_DECISION_REPLACE, &operation, 1, decision, error);
    }
    if (named(matched->operation, "quantum.z")) {
        return qdk_pass_decision_new(QDK_DECISION_REMOVE, NULL, 0, decision, error);
    }
    assert(named(matched->operation, "quantum.h"));
    return QDK_OK;
}

static void patch_decisions_cross_once_per_match_and_leave_input_unchanged(void) {
    calls = drops = 0;
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = new_circuit(1, 0);
    for (size_t i = 0; i < 128; ++i) append_gate(circuit, QDK_GATE_T);
    append_gate(circuit, QDK_GATE_X);
    append_gate(circuit, QDK_GATE_Z);
    append_gate(circuit, QDK_GATE_H);
    active_graph = lower_and_free(&circuit);
    QdkGraphInfo dimensions = {0};
    require_success(qdk_graph_info(active_graph, &dimensions, &error), &error);
    assert(dimensions.node_count > 128);
    QdkBytes before = {0}, after = {0};
    require_success(qdk_graph_export(active_graph, QDK_FORMAT_GRAPH_BINARY, 0, &before, &error), &error);
    QdkString operations[] = {text("quantum.x"), text("quantum.z"), text("quantum.h")};
    QdkGraphPassOptions options = {.name = text("patch"), .kind = QDK_PASS_REWRITE,
        .operations = operations, .operation_count = 3};
    require_success(qdk_graph_pass_new(&options, patch, 123, destroy_context, &active_pass, &error), &error);
    QdkGraph *rewritten = NULL;
    QdkPassResult *result = NULL;
    require_success(qdk_graph_pass_run(active_pass, active_graph, &rewritten, &result, &error), &error);
    QdkPassResultInfo info = {0};
    require_success(qdk_pass_result_info(result, &info, &error), &error);
    assert(calls == 3 && info.callback_count == 3 && info.applications == 2 && info.outcome == QDK_PASS_COMMITTED);
    require_success(qdk_graph_export(active_graph, QDK_FORMAT_GRAPH_BINARY, 0, &after, &error), &error);
    assert(before.len == after.len && memcmp(before.data, after.data, before.len) == 0);
    qdk_bytes_free(&before);
    qdk_bytes_free(&after);
    circuit = raise_graph(rewritten);
    QdkCircuitInfo circuit_info = {0};
    require_success(qdk_circuit_info(circuit, &circuit_info, &error), &error);
    assert(circuit_info.instructions == 130);
    QdkInstruction *last = instruction_at(circuit, 129), *replacement = instruction_at(circuit, 128);
    expect_name(last, "h");
    expect_name(replacement, "y");
    free_instruction(&last);
    free_instruction(&replacement);
    free_circuit(&circuit);
    require_success(qdk_pass_result_free(&result, &error), &error);
    free_graph(&rewritten);
    free_graph(&active_graph);
    assert(drops == 0);
    require_success(qdk_graph_pass_free(&active_pass, &error), &error);
    assert(drops == 1);
}

static QdkStatus fragment(uintptr_t cookie, const QdkPassMatch *matched,
                          QdkPassDecision **decision, QdkDiagnostic **error) {
    assert(cookie == 123 && named(matched->operation, "quantum.x"));
    calls++;
    QdkString operations[] = {text("quantum.h"), text("quantum.z")};
    return qdk_pass_decision_new(QDK_DECISION_EXPAND, operations, 2, decision, error);
}

static void fragment_callback_expands_a_matched_operand_thread(void) {
    calls = drops = 0;
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = new_circuit(1, 0);
    append_gate(circuit, QDK_GATE_X);
    QdkGraph *graph = lower_and_free(&circuit), *expanded = NULL;
    QdkString operation = text("quantum.x");
    QdkGraphPassOptions options = {.name = text("expand"), .kind = QDK_PASS_EXPAND,
        .operations = &operation, .operation_count = 1};
    QdkGraphPass *pass = NULL;
    require_success(qdk_graph_pass_new(&options, fragment, 123, destroy_context, &pass, &error), &error);
    QdkPassResult *result = NULL;
    require_success(qdk_graph_pass_run(pass, graph, &expanded, &result, &error), &error);
    QdkPassResultInfo info = {0};
    require_success(qdk_pass_result_info(result, &info, &error), &error);
    assert(calls == 1 && info.callback_count == 1 && info.applications == 1 && info.outcome == QDK_PASS_COMMITTED);
    circuit = raise_graph(expanded);
    QdkInstruction *first = instruction_at(circuit, 0), *second = instruction_at(circuit, 1);
    expect_name(first, "h");
    expect_name(second, "z");
    free_instruction(&first);
    free_instruction(&second);
    free_circuit(&circuit);
    free_graph(&expanded);
    free_graph(&graph);
    require_success(qdk_pass_result_free(&result, &error), &error);
    require_success(qdk_graph_pass_free(&pass, &error), &error);
    assert(drops == 1);
}

static QdkStatus replace(uintptr_t cookie, const QdkPassMatch *matched,
                         QdkPassDecision **decision, QdkDiagnostic **error) {
    assert(cookie == 123 && named(matched->operation, "quantum.x"));
    calls++;
    QdkString operation = text("quantum.y");
    return qdk_pass_decision_new(QDK_DECISION_REPLACE, &operation, 1, decision, error);
}

static void rewrite_budget_reports_unapplied_matches_without_extra_callbacks(void) {
    calls = drops = 0;
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = new_circuit(1, 0);
    append_gate(circuit, QDK_GATE_X);
    append_gate(circuit, QDK_GATE_X);
    append_gate(circuit, QDK_GATE_X);
    QdkGraph *graph = lower_and_free(&circuit), *rewritten = NULL;
    QdkString operation = text("quantum.x");
    QdkGraphPassOptions options = {.name = text("budget"), .kind = QDK_PASS_REWRITE,
        .operations = &operation, .operation_count = 1, .use_budget = 1, .budget = 1};
    QdkGraphPass *pass = NULL;
    require_success(qdk_graph_pass_new(&options, replace, 123, destroy_context, &pass, &error), &error);
    QdkPassResult *result = NULL;
    require_success(qdk_graph_pass_run(pass, graph, &rewritten, &result, &error), &error);
    QdkPassResultInfo info = {0};
    require_success(qdk_pass_result_info(result, &info, &error), &error);
    assert(info.outcome == QDK_PASS_BUDGET_EXHAUSTED && info.applications == 1 && info.pending == 2);
    assert(info.callback_count == 3 && calls == 3);
    require_success(qdk_pass_result_free(&result, &error), &error);
    free_graph(&rewritten);
    free_graph(&graph);
    require_success(qdk_graph_pass_free(&pass, &error), &error);
    assert(drops == 1);
}

int main(void) {
    patch_decisions_cross_once_per_match_and_leave_input_unchanged();
    fragment_callback_expands_a_matched_operand_thread();
    rewrite_budget_reports_unapplied_matches_without_extra_callbacks();
    puts("3 C graph pass tests passed");
    return 0;
}
