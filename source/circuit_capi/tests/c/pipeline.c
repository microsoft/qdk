// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"

static QdkPipeline *active_pipeline;
static QdkCircuit *active_input;
static QdkCircuit *independent_clone;
static size_t context_drops;
static uintptr_t order[5];
static size_t order_count;

static QdkString text(const char *value) {
    return (QdkString){(const uint8_t *)value, strlen(value)};
}

static void context_free(uintptr_t cookie) {
    assert(cookie != 0);
    context_drops++;
}

static void append_gate(QdkCircuit *circuit, uint32_t gate) {
    QdkDiagnostic *error = NULL;
    uint32_t target = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = gate,
        .qubits = &target, .qubit_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
}

static QdkStatus ordered_circuit(uintptr_t cookie, QdkCircuitCallbackScope *scope,
                                 QdkCircuit *working, QdkDiagnostic **error) {
    (void)scope;
    (void)error;
    assert(order_count < 5);
    order[order_count++] = cookie;
    QdkDiagnostic *failure = NULL;
    QdkCircuit *borrowed = working;
    assert(qdk_circuit_free(&borrowed, &failure) == QDK_BORROWED && borrowed == working);
    qdk_diagnostic_free(&failure);
    assert(qdk_pipeline_free(&active_pipeline, &failure) == QDK_ACTIVE);
    qdk_diagnostic_free(&failure);
    assert(qdk_circuit_free(&active_input, &failure) == QDK_ACTIVE);
    qdk_diagnostic_free(&failure);
    QdkPipelineResult *nested = NULL;
    assert(qdk_pipeline_run(active_pipeline, active_input, &nested, &failure) == QDK_ACTIVE);
    assert(nested == NULL);
    qdk_diagnostic_free(&failure);
    if (cookie == 1) {
        append_gate(working, QDK_GATE_X);
        require_success(qdk_circuit_clone(working, &independent_clone, &failure), &failure);
    } else if (cookie == 2) {
        append_gate(working, QDK_GATE_H);
    } else {
        assert(cookie == 5);
        append_gate(working, QDK_GATE_T);
    }
    return QDK_OK;
}

static QdkStatus ordered_graph(uintptr_t cookie, const QdkPassMatch *matched,
                               QdkPassDecision **decision, QdkDiagnostic **error) {
    assert(matched->operands == 1 && order_count < 5);
    order[order_count++] = cookie;
    if (cookie == 3) {
        QdkString operation = text("quantum.y");
        return qdk_pass_decision_new(QDK_DECISION_REPLACE, &operation, 1, decision, error);
    }
    assert(cookie == 4);
    QdkString operations[] = {text("quantum.z"), text("quantum.z")};
    return qdk_pass_decision_new(QDK_DECISION_EXPAND, operations, 2, decision, error);
}

static void native_pipeline_orders_anchors_and_reports_independent_outputs(void) {
    context_drops = order_count = 0;
    QdkDiagnostic *error = NULL;
    active_input = new_circuit(1, 0);
    require_success(qdk_pipeline_new(1, QDK_OPTIMIZATION_DEFAULT, &active_pipeline, &error), &error);
    require_success(qdk_pipeline_add_circuit(active_pipeline, QDK_ANCHOR_CIRCUIT_RAISED,
        text("outbound"), ordered_circuit, 5, context_free, &error), &error);
    QdkString y = text("quantum.y"), x = text("quantum.x");
    QdkGraphPassOptions expand = {.name = text("expand"), .kind = QDK_PASS_EXPAND,
        .operations = &y, .operation_count = 1};
    QdkGraphPassOptions rewrite = {.name = text("rewrite"), .kind = QDK_PASS_REWRITE,
        .operations = &x, .operation_count = 1};
    require_success(qdk_pipeline_add_graph(active_pipeline, QDK_ANCHOR_GRAPH_REWRITTEN,
        &expand, ordered_graph, 4, context_free, &error), &error);
    require_success(qdk_pipeline_add_circuit(active_pipeline, QDK_ANCHOR_CIRCUIT_BUILT,
        text("first"), ordered_circuit, 1, context_free, &error), &error);
    require_success(qdk_pipeline_add_circuit(active_pipeline, QDK_ANCHOR_CIRCUIT_BUILT,
        text("second"), ordered_circuit, 2, context_free, &error), &error);
    require_success(qdk_pipeline_add_graph(active_pipeline, QDK_ANCHOR_GRAPH_LOWERED,
        &rewrite, ordered_graph, 3, context_free, &error), &error);
    size_t count = 0;
    require_success(qdk_pipeline_count(active_pipeline, &count, &error), &error);
    assert(count == 7);
    QdkPassEntryInfo entry = {0};
    require_success(qdk_pipeline_entry(active_pipeline, 0, &entry, &error), &error);
    assert(entry.anchor == QDK_ANCHOR_CIRCUIT_BUILT && entry.built_in == 0);
    assert(entry.name.len == 5 && memcmp(entry.name.data, "first", 5) == 0);
    qdk_bytes_free(&entry.name);
    QdkPipelineResult *result = NULL;
    require_success(qdk_pipeline_run(active_pipeline, active_input, &result, &error), &error);
    assert(order_count == 5);
    for (size_t i = 0; i < order_count; ++i) assert(order[i] == i + 1);
    QdkPipelineResultInfo info = {0};
    require_success(qdk_pipeline_result_info(result, &info, &error), &error);
    assert(info.executed_count == 7 && info.changed_count == 5 && info.callback_count == 5);
    require_success(qdk_pipeline_result_entry(result, 3, &entry, &error), &error);
    assert(entry.anchor == QDK_ANCHOR_GRAPH_REWRITTEN && entry.built_in == 1);
    qdk_bytes_free(&entry.name);
    QdkBytes changed = {0};
    require_success(qdk_pipeline_result_changed(result, 4, &changed, &error), &error);
    assert(changed.len == 8 && memcmp(changed.data, "outbound", 8) == 0);
    qdk_bytes_free(&changed);
    QdkCircuit *produced = NULL;
    QdkGraph *graph = NULL;
    require_success(qdk_pipeline_result_circuit(result, &produced, &error), &error);
    require_success(qdk_pipeline_result_graph(result, &graph, &error), &error);
    require_success(qdk_pipeline_result_free(&result, &error), &error);
    require_success(qdk_pipeline_free(&active_pipeline, &error), &error);
    assert(context_drops == 5);
    QdkCircuitInfo circuit_info = {0};
    require_success(qdk_circuit_info(active_input, &circuit_info, &error), &error);
    assert(circuit_info.instructions == 0);
    require_success(qdk_circuit_info(produced, &circuit_info, &error), &error);
    assert(circuit_info.instructions == 4);
    QdkInstruction *last = instruction_at(produced, 3);
    expect_name(last, "t");
    free_instruction(&last);
    require_success(qdk_circuit_info(independent_clone, &circuit_info, &error), &error);
    assert(circuit_info.instructions == 1);
    QdkBytes qir = {0};
    require_success(qdk_graph_export(graph, QDK_FORMAT_QIR, QDK_PROFILE_BASE, &qir, &error), &error);
    assert(qir.len > 0);
    qdk_bytes_free(&qir);
    require_success(qdk_graph_free(&graph, &error), &error);
    free_circuit(&produced);
    free_circuit(&independent_clone);
    free_circuit(&active_input);
}

static QdkStatus scoped_children(uintptr_t cookie, QdkCircuitCallbackScope *scope,
                                 QdkCircuit *working, QdkDiagnostic **error) {
    QdkDiagnostic *failure = NULL;
    if (cookie == 13) {
        QdkCircuit *candidate = NULL;
        require_success(qdk_circuit_clone(working, &candidate, &failure), &failure);
        require_success(qdk_callback_offer(scope, &candidate, &failure), &failure);
        assert(candidate == NULL);
    }
    QdkParameter *parameter = NULL, *recovered = NULL;
    require_success(qdk_circuit_parameter(working, (const uint8_t *)"p", 1, &parameter, &failure), &failure);
    uint32_t id = 0;
    require_success(qdk_parameter_id(parameter, &id, &failure), &failure);
    require_success(qdk_circuit_parameter_at(working, id, &recovered, &failure), &failure);
    QdkAngle angle = {.kind = QDK_ANGLE_PARAMETER, .parameter = parameter};
    QdkAngleExpression *negation = NULL;
    require_success(qdk_circuit_angle(working, QDK_ANGLE_NEG, &angle, NULL, &negation, &failure), &failure);
    require_success(qdk_parameter_free(&parameter, &failure), &failure);
    QdkVariable *variable = NULL;
    require_success(qdk_circuit_variable(working, (const uint8_t *)"flag", 4, QDK_VALUE_BOOL, &variable, &failure), &failure);
    QdkVariableInfo variable_info = {0};
    require_success(qdk_variable_info(variable, &variable_info, &failure), &failure);
    QdkVariable *recovered_variable = NULL;
    require_success(qdk_circuit_variable_at(working, variable_info.id, &recovered_variable, &failure), &failure);
    qdk_bytes_free(&variable_info.name);
    QdkExpression *condition = make_expression(working, (QdkExpressionInput){.kind = QDK_EXPR_VARIABLE, .variable = variable});
    QdkExpression *not_condition = make_expression(working, (QdkExpressionInput){.kind = QDK_EXPR_NOT, .left = condition});
    QdkExpression *operand = NULL;
    require_success(qdk_expression_operand(not_condition, 0, &operand, &failure), &failure);
    QdkBuilder *outer = begin_body(working, NULL, QDK_BODY_BOX, NULL);
    QdkBuilder *inner = begin_body(working, outer, QDK_BODY_BOX, NULL);
    QdkBlock *block = NULL;
    require_success(qdk_builder_block(inner, &block, &failure), &failure);
    append_body_gate(inner, QDK_GATE_X, 0);
    uint32_t target = 0;
    angle = (QdkAngle){.kind = QDK_ANGLE_EXPRESSION, .expression = negation};
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &target, .qubit_count = 1, .params = &angle, .param_count = 1};
    require_success(qdk_builder_append(inner, &operation, &failure), &failure);
    QdkInstruction *record = NULL;
    require_success(qdk_block_instruction(block, 1, &record, &failure), &failure);
    QdkAngleExpression *recovered_angle = NULL;
    require_success(qdk_instruction_angle_expression(record, working, 0, &recovered_angle, &failure), &failure);
    free_instruction(&record);
    require_success(qdk_angle_expression_free(&negation, &failure), &failure);
    if (cookie == 13) return QDK_OK;
    require_success(qdk_builder_cancel(inner, &failure), &failure);
    require_success(qdk_builder_cancel(outer, &failure), &failure);
    QdkBuilder *owner = outer;
    assert(qdk_builder_free(&owner, &failure) == QDK_IN_USE && owner == outer);
    qdk_diagnostic_free(&failure);

    QdkLiteral value = {.kind = QDK_VALUE_FLOAT, .real = 0.25};
    QdkForOptions options = {.values = &value, .value_count = 1, .index_kind = QDK_LOOP_PARAMETER};
    QdkLoopIndex index = {0};
    QdkBuilder *loop = NULL;
    require_success(qdk_circuit_for_begin(working, NULL, &options, &loop, &index, &failure), &failure);
    require_success(qdk_builder_cancel(loop, &failure), &failure);
    value = (QdkLiteral){.kind = QDK_VALUE_INT, .integer = 1};
    options.index_kind = QDK_LOOP_RUNTIME;
    require_success(qdk_circuit_for_begin(working, NULL, &options, &loop, &index, &failure), &failure);
    require_success(qdk_builder_cancel(loop, &failure), &failure);
    QdkExpression *selector = make_expression(working, (QdkExpressionInput){.kind = QDK_EXPR_LITERAL, .literal = value});
    QdkBuilder *selected = begin_body(working, NULL, QDK_BODY_SWITCH, selector), *matched = NULL;
    require_success(qdk_switch_case(selected, &value, 1, 0, &matched, &failure), &failure);
    require_success(qdk_builder_cancel(matched, &failure), &failure);
    require_success(qdk_builder_cancel(selected, &failure), &failure);
    require_success(qdk_circuit_clone(working, &independent_clone, &failure), &failure);
    if (cookie == 12) {
        require_success(qdk_diagnostic_new((const uint8_t *)"callback failed", 15, error, &failure), &failure);
        return QDK_CALLBACK_ERROR;
    }
    assert(cookie == 11);
    return QDK_OK;
}

static void run_scoped_callback(uintptr_t mode, QdkStatus expected) {
    context_drops = 0;
    independent_clone = NULL;
    QdkDiagnostic *error = NULL;
    QdkPipeline *pipeline = NULL;
    QdkCircuit *circuit = new_circuit(1, 0);
    require_success(qdk_pipeline_new(0, QDK_OPTIMIZATION_NONE, &pipeline, &error), &error);
    require_success(qdk_pipeline_add_circuit(pipeline, QDK_ANCHOR_CIRCUIT_BUILT, text("scoped"),
        scoped_children, mode, context_free, &error), &error);
    QdkPipelineResult *result = NULL;
    QdkStatus status = qdk_pipeline_run(pipeline, circuit, &result, &error);
    if (expected == QDK_OK) {
        require_success(status, &error);
        QdkPipelineResultInfo info = {0};
        require_success(qdk_pipeline_result_info(result, &info, &error), &error);
        assert(info.callback_count == 1);
        require_success(qdk_pipeline_result_free(&result, &error), &error);
    } else {
        assert(status == expected && result == NULL && error != NULL);
        QdkBytes message = {0};
        assert(qdk_diagnostic_message(error, &message, NULL) == QDK_OK);
        if (mode == 12) {
            assert(message.len == 15 && memcmp(message.data, "callback failed", 15) == 0);
        } else {
            assert(message.len > 0);
        }
        qdk_bytes_free(&message);
        qdk_diagnostic_free(&error);
    }
    assert(context_drops == 0);
    require_success(qdk_pipeline_free(&pipeline, &error), &error);
    assert(context_drops == 1);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(circuit, &info, &error), &error);
    assert(info.instructions == 0);
    if (independent_clone != NULL) {
        require_success(qdk_circuit_info(independent_clone, &info, &error), &error);
        assert(info.instructions == 0);
        free_circuit(&independent_clone);
    }
    free_circuit(&circuit);
}

static void callback_success_disposes_remaining_dependents(void) {
    run_scoped_callback(11, QDK_OK);
}

static void callback_failure_disposes_remaining_dependents(void) {
    run_scoped_callback(12, QDK_CALLBACK_ERROR);
}

static void callback_unfinished_body_is_rejected_and_cleaned(void) {
    run_scoped_callback(13, QDK_CALLBACK_ERROR);
}

int main(void) {
    native_pipeline_orders_anchors_and_reports_independent_outputs();
    callback_success_disposes_remaining_dependents();
    callback_failure_disposes_remaining_dependents();
    callback_unfinished_body_is_rejected_and_cleaned();
    puts("4 C pipeline tests passed");
    return 0;
}
