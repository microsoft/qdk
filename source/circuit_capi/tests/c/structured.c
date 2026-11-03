// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"

static void expect_theta_product(QdkAngleExpression *product) {
    QdkDiagnostic *error = NULL;
    QdkAngleNode nodes[3] = {0};
    size_t needed = 0;
    require_success(qdk_angle_expression_nodes(product, nodes, 3, &needed, &error), &error);
    assert(needed == 3 && nodes[2].operation == QDK_ANGLE_MUL);
    assert(nodes[2].left == 0 && nodes[2].right == 1);
    assert(nodes[0].kind == QDK_ANGLE_PARAMETER && nodes[0].parameter_id == 0);
    assert(nodes[1].kind == QDK_ANGLE_LITERAL && nodes[1].literal == 2.0);
}

static void restored_symbolic_arithmetic_is_inspectable_and_binds_a_new_circuit(void) {
    QdkCircuit *circuit = new_circuit(1, 0), *bound = NULL;
    QdkDiagnostic *error = NULL;
    QdkParameter *theta = NULL;
    require_success(qdk_circuit_parameter(circuit, (const uint8_t *)"theta", 5, &theta, &error), &error);
    QdkAngle left = {.kind = QDK_ANGLE_PARAMETER, .parameter = theta};
    QdkAngle right = {.kind = QDK_ANGLE_LITERAL, .literal = 2.0};
    QdkAngleExpression *product = NULL;
    require_success(qdk_circuit_angle(circuit, QDK_ANGLE_MUL, &left, &right, &product, &error), &error);
    QdkAngle angle = {.kind = QDK_ANGLE_EXPRESSION, .expression = product};
    uint32_t qubit = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &qubit, .qubit_count = 1, .params = &angle, .param_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    expect_theta_product(product);
    require_success(qdk_angle_expression_free(&product, &error), &error);
    require_success(qdk_parameter_free(&theta, &error), &error);
    QdkCircuit *restored = snapshot_roundtrip(circuit);
    free_circuit(&circuit);
    QdkInstruction *record = instruction_at(restored, 0);
    require_success(qdk_instruction_angle_expression(record, restored, 0, &product, &error), &error);
    expect_theta_product(product);
    require_success(qdk_circuit_clone(restored, &circuit, &error), &error);
    QdkAngleExpression *rejected = product;
    assert(qdk_instruction_angle_expression(record, circuit, 0, &rejected, &error) == QDK_WRONG_OWNER);
    assert(rejected == NULL && error != NULL);
    qdk_diagnostic_free(&error);
    require_success(qdk_angle_expression_free(&product, &error), &error);
    free_circuit(&restored);
    assert(qdk_instruction_angle_expression(record, circuit, 0, &rejected, &error) == QDK_WRONG_OWNER);
    assert(rejected == NULL && error != NULL);
    qdk_diagnostic_free(&error);
    free_instruction(&record);
    record = instruction_at(circuit, 0);
    assert(qdk_instruction_angle_expression(record, circuit, 1, &rejected, &error) == QDK_INVALID_ARGUMENT);
    assert(rejected == NULL && error != NULL);
    qdk_diagnostic_free(&error);
    require_success(qdk_instruction_angle_expression(record, circuit, 0, &product, &error), &error);
    free_instruction(&record);
    expect_theta_product(product);
    QdkCircuit *owner = circuit;
    assert(qdk_circuit_free(&circuit, &error) == QDK_IN_USE && circuit == owner);
    qdk_diagnostic_free(&error);
    require_success(qdk_circuit_parameter_at(circuit, 0, &theta, &error), &error);
    QdkBytes name = {0};
    require_success(qdk_parameter_name(theta, &name, &error), &error);
    assert(name.len == 5 && memcmp(name.data, "theta", 5) == 0);
    qdk_bytes_free(&name);
    size_t needed = 0;
    QdkParameterBinding binding = {theta, 0.25};
    require_success(qdk_circuit_bind(circuit, &binding, 1, &bound, &error), &error);
    uint32_t ids[1] = {0};
    require_success(qdk_circuit_parameters(circuit, ids, 1, &needed, &error), &error);
    assert(needed == 1 && ids[0] == 0);
    require_success(qdk_circuit_parameters(bound, NULL, 0, &needed, &error), &error);
    assert(needed == 0);
    QdkInstruction *instruction = instruction_at(bound, 0);
    QdkStoredAngle stored = {0};
    require_success(qdk_instruction_angles(instruction, &stored, 1, &needed, &error), &error);
    assert(stored.kind == QDK_ANGLE_LITERAL && stored.literal == 0.5);
    assert(qdk_instruction_angle_expression(instruction, bound, 0, &rejected, &error) == QDK_WRONG_VARIANT);
    assert(rejected == NULL && error != NULL);
    qdk_diagnostic_free(&error);
    free_instruction(&instruction);
    require_success(qdk_angle_expression_free(&product, &error), &error);
    require_success(qdk_parameter_free(&theta, &error), &error);
    free_circuit(&bound);
    free_circuit(&circuit);
}

static void if_else_preserves_separate_bodies_and_condition(void) {
    QdkCircuit *circuit = new_circuit(1, 1);
    QdkDiagnostic *error = NULL;
    QdkExpressionInput input = {.kind = QDK_EXPR_CLBIT, .clbit = 0};
    QdkExpression *condition = make_expression(circuit, input), *copied = NULL;
    QdkBuilder *then_body = begin_body(circuit, NULL, QDK_BODY_IF, condition), *else_body = NULL;
    append_body_gate(then_body, QDK_GATE_X, 0);
    finish_builder(then_body);
    require_success(qdk_builder_otherwise(then_body, &else_body, &error), &error);
    append_body_gate(else_body, QDK_GATE_H, 0);
    finish_builder(else_body);
    QdkInstruction *parent = instruction_at(circuit, 0);
    QdkInstruction *first = child_instruction(circuit, parent, 0, 0);
    QdkInstruction *second = child_instruction(circuit, parent, 1, 0);
    expect_name(first, "x");
    expect_name(second, "h");
    require_success(qdk_instruction_condition(parent, circuit, &copied, &error), &error);
    QdkExpressionInfo info = {0};
    require_success(qdk_expression_info(copied, &info, &error), &error);
    assert(info.kind == QDK_EXPR_CLBIT && info.value_type == QDK_VALUE_BOOL && info.clbit == 0);
    require_success(qdk_expression_free(&copied, &error), &error);
    require_success(qdk_expression_free(&condition, &error), &error);
    free_instruction(&first);
    free_instruction(&second);
    free_instruction(&parent);
    free_builder(&else_body);
    free_builder(&then_body);
    free_circuit(&circuit);
}

static void while_body_retains_its_stored_condition(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkExpression *condition = true_expression(circuit);
    QdkBuilder *body = begin_body(circuit, NULL, QDK_BODY_WHILE, condition);
    require_success(qdk_expression_free(&condition, &error), &error);
    append_body_gate(body, QDK_GATE_X, 0);
    finish_builder(body);
    QdkInstruction *parent = instruction_at(circuit, 0);
    QdkInstruction *child = child_instruction(circuit, parent, 0, 0);
    expect_name(parent, "while");
    expect_name(child, "x");
    QdkGraph *graph = NULL;
    assert(qdk_circuit_to_graph(circuit, &graph, &error) == QDK_INVALID_ARGUMENT);
    assert(graph == NULL);
    QdkBytes message = {0};
    assert(qdk_diagnostic_message(error, &message, NULL) == QDK_OK);
    const char *expected = "operation 'while' has no graph form";
    assert(message.len == strlen(expected) && memcmp(message.data, expected, message.len) == 0);
    qdk_bytes_free(&message);
    qdk_diagnostic_free(&error);
    free_instruction(&child);
    free_instruction(&parent);
    free_builder(&body);
    free_circuit(&circuit);
}

static void parameter_for_loop_uses_compile_time_angles(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkLiteral values[] = {{.kind = QDK_VALUE_FLOAT, .real = 0.25}, {.kind = QDK_VALUE_FLOAT, .real = 0.5}};
    QdkForOptions options = {.values = values, .value_count = 2, .index_kind = QDK_LOOP_PARAMETER};
    QdkLoopIndex index = {0};
    QdkBuilder *body = NULL;
    require_success(qdk_circuit_for_begin(circuit, NULL, &options, &body, &index, &error), &error);
    assert(index.kind == QDK_LOOP_PARAMETER && index.parameter != NULL && index.variable == NULL);
    QdkAngle angle = {.kind = QDK_ANGLE_PARAMETER, .parameter = index.parameter};
    uint32_t qubit = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &qubit, .qubit_count = 1, .params = &angle, .param_count = 1};
    require_success(qdk_builder_append(body, &operation, &error), &error);
    finish_builder(body);
    size_t needed = 0;
    require_success(qdk_circuit_parameters(circuit, NULL, 0, &needed, &error), &error);
    assert(needed == 0);
    QdkInstruction *parent = instruction_at(circuit, 0);
    QdkControlInfo info = {0};
    require_success(qdk_instruction_control(parent, &info, &error), &error);
    assert(info.kind == QDK_BODY_FOR && info.loop_index_kind == QDK_LOOP_PARAMETER);
    QdkLiteral copied[2] = {0};
    require_success(qdk_instruction_values(parent, 0, copied, 2, &needed, &error), &error);
    assert(needed == 2 && copied[1].kind == QDK_VALUE_FLOAT && copied[1].real == 0.5);
    free_instruction(&parent);
    free_builder(&body);
    require_success(qdk_parameter_free(&index.parameter, &error), &error);
    free_circuit(&circuit);
}

static void restored_runtime_loop_exposes_its_variable_metadata_and_condition(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkLiteral values[] = {{.kind = QDK_VALUE_INT, .integer = 1}};
    QdkForOptions options = {.values = values, .value_count = 1, .index_kind = QDK_LOOP_RUNTIME,
        .name = (const uint8_t *)"iteration", .name_len = 9};
    QdkLoopIndex index = {0};
    QdkBuilder *loop = NULL;
    require_success(qdk_circuit_for_begin(circuit, NULL, &options, &loop, &index, &error), &error);
    assert(index.variable != NULL && index.parameter == NULL);
    QdkExpression *variable = make_expression(circuit, (QdkExpressionInput){.kind = QDK_EXPR_VARIABLE, .variable = index.variable});
    QdkExpression *one = make_expression(circuit, (QdkExpressionInput){.kind = QDK_EXPR_LITERAL, .literal = values[0]});
    QdkExpression *equal = make_expression(circuit, (QdkExpressionInput){.kind = QDK_EXPR_BINARY, .operation = QDK_BINARY_EQ, .left = variable, .right = one});
    QdkBuilder *conditional = begin_body(circuit, loop, QDK_BODY_IF, equal);
    append_body_gate(conditional, QDK_GATE_X, 0);
    finish_builder(conditional);
    finish_builder(loop);
    QdkVariableInfo info = {0};
    require_success(qdk_variable_info(index.variable, &info, &error), &error);
    assert(info.value_type == QDK_VALUE_INT && info.name.len != 0);
    qdk_bytes_free(&info.name);
    QdkExpression *operand = NULL;
    require_success(qdk_expression_operand(equal, 0, &operand, &error), &error);
    QdkExpressionInfo expr = {0};
    require_success(qdk_expression_info(operand, &expr, &error), &error);
    assert(expr.kind == QDK_EXPR_VARIABLE && expr.variable_id == info.id);
    require_success(qdk_expression_free(&operand, &error), &error);
    require_success(qdk_expression_free(&equal, &error), &error);
    require_success(qdk_expression_free(&one, &error), &error);
    require_success(qdk_expression_free(&variable, &error), &error);
    require_success(qdk_variable_free(&index.variable, &error), &error);
    free_builder(&conditional);
    free_builder(&loop);
    QdkCircuit *restored = snapshot_roundtrip(circuit);
    free_circuit(&circuit);
    QdkInstruction *parent = instruction_at(restored, 0);
    QdkControlInfo control = {0};
    require_success(qdk_instruction_control(parent, &control, &error), &error);
    assert(control.kind == QDK_BODY_FOR && control.loop_index_kind == QDK_LOOP_RUNTIME);
    QdkVariable *recovered = NULL;
    require_success(qdk_circuit_variable_at(restored, control.loop_index_id, &recovered, &error), &error);
    require_success(qdk_variable_info(recovered, &info, &error), &error);
    assert(info.id == control.loop_index_id && info.value_type == QDK_VALUE_INT);
    assert(info.name.len == 9 && memcmp(info.name.data, "iteration", 9) == 0);
    qdk_bytes_free(&info.name);
    require_success(qdk_circuit_clone(restored, &circuit, &error), &error);
    QdkExpressionInput foreign = {.kind = QDK_EXPR_VARIABLE, .variable = recovered};
    QdkExpression *rejected = NULL;
    assert(qdk_circuit_expression(circuit, &foreign, &rejected, &error) == QDK_WRONG_OWNER);
    assert(rejected == NULL && error != NULL);
    qdk_diagnostic_free(&error);
    require_success(qdk_variable_free(&recovered, &error), &error);
    free_instruction(&parent);
    free_circuit(&restored);
    parent = instruction_at(circuit, 0);
    QdkInstruction *nested = child_instruction(circuit, parent, 0, 0);
    require_success(qdk_instruction_condition(nested, circuit, &equal, &error), &error);
    require_success(qdk_expression_operand(equal, 0, &operand, &error), &error);
    require_success(qdk_expression_info(operand, &expr, &error), &error);
    assert(expr.kind == QDK_EXPR_VARIABLE && expr.variable_id == control.loop_index_id);
    require_success(qdk_circuit_variable_at(circuit, expr.variable_id, &recovered, &error), &error);
    require_success(qdk_variable_info(recovered, &info, &error), &error);
    assert(info.id == expr.variable_id && info.value_type == QDK_VALUE_INT);
    assert(info.name.len == 9 && memcmp(info.name.data, "iteration", 9) == 0);
    qdk_bytes_free(&info.name);
    require_success(qdk_expression_free(&operand, &error), &error);
    require_success(qdk_expression_free(&equal, &error), &error);
    free_instruction(&nested);
    free_instruction(&parent);
    QdkVariable *invalid = recovered;
    assert(qdk_circuit_variable_at(circuit, UINT32_MAX, &invalid, &error) == QDK_INVALID_ARGUMENT);
    assert(invalid == NULL && error != NULL);
    qdk_diagnostic_free(&error);
    QdkCircuit *owner = circuit;
    assert(qdk_circuit_free(&circuit, &error) == QDK_IN_USE && circuit == owner);
    qdk_diagnostic_free(&error);
    require_success(qdk_variable_free(&recovered, &error), &error);
    free_circuit(&circuit);
}

static void switch_records_case_values_and_default_separately(void) {
    QdkCircuit *circuit = new_circuit(1, 1);
    QdkDiagnostic *error = NULL;
    QdkExpression *selector = make_expression(circuit, (QdkExpressionInput){.kind = QDK_EXPR_CLBIT, .clbit = 0});
    QdkBuilder *selected = begin_body(circuit, NULL, QDK_BODY_SWITCH, selector);
    QdkLiteral values[] = {{.kind = QDK_VALUE_INT, .integer = 1}};
    QdkBuilder *matched = NULL, *fallback = NULL;
    require_success(qdk_switch_case(selected, values, 1, 0, &matched, &error), &error);
    append_body_gate(matched, QDK_GATE_X, 0);
    finish_builder(matched);
    require_success(qdk_switch_case(selected, NULL, 0, 1, &fallback, &error), &error);
    append_body_gate(fallback, QDK_GATE_H, 0);
    finish_builder(fallback);
    finish_builder(selected);
    QdkInstruction *parent = instruction_at(circuit, 0);
    QdkControlInfo info = {0};
    require_success(qdk_instruction_control(parent, &info, &error), &error);
    assert(info.kind == QDK_BODY_SWITCH && info.case_count == 1 && info.has_default == 1);
    QdkLiteral copied = {0};
    size_t needed = 0;
    require_success(qdk_instruction_values(parent, 0, &copied, 1, &needed, &error), &error);
    assert(needed == 1 && copied.kind == QDK_VALUE_INT && copied.integer == 1);
    free_instruction(&parent);
    free_builder(&fallback);
    free_builder(&matched);
    free_builder(&selected);
    require_success(qdk_expression_free(&selector, &error), &error);
    free_circuit(&circuit);
}

static void box_label_and_readonly_block_survive_builder_release(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkBuilder *body = NULL;
    QdkBodyOptions options = {.kind = QDK_BODY_BOX, .label = (const uint8_t *)"stage", .label_len = 5};
    require_success(qdk_circuit_begin(circuit, NULL, &options, &body, &error), &error);
    append_body_gate(body, QDK_GATE_X, 0);
    QdkBlock *block = NULL;
    require_success(qdk_builder_block(body, &block, &error), &error);
    finish_builder(body);
    free_builder(&body);
    size_t count = 0;
    require_success(qdk_block_len(block, &count, &error), &error);
    assert(count == 1);
    QdkInstruction *parent = instruction_at(circuit, 0);
    QdkBytes label = {0};
    require_success(qdk_instruction_label(parent, &label, &error), &error);
    assert(label.len == 5 && memcmp(label.data, "stage", 5) == 0);
    qdk_bytes_free(&label);
    free_instruction(&parent);
    require_success(qdk_block_free(&block, &error), &error);
    free_circuit(&circuit);
}

static void moment_overlap_is_reported_without_publishing(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkBuilder *body = begin_body(circuit, NULL, QDK_BODY_MOMENT, NULL);
    append_body_gate(body, QDK_GATE_X, 0);
    append_body_gate(body, QDK_GATE_H, 0);
    assert(qdk_builder_finish(body, &error) == QDK_INVALID_ARGUMENT);
    qdk_diagnostic_free(&error);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(circuit, &info, &error), &error);
    assert(info.instructions == 0);
    free_builder(&body);
    free_circuit(&circuit);
}

static void break_reaches_an_enclosing_loop_through_a_conditional(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkExpression *condition = true_expression(circuit);
    QdkBuilder *loop = begin_body(circuit, NULL, QDK_BODY_WHILE, condition);
    QdkBuilder *branch = begin_body(circuit, loop, QDK_BODY_IF, condition);
    require_success(qdk_circuit_jump(circuit, QDK_JUMP_BREAK, &error), &error);
    finish_builder(branch);
    finish_builder(loop);
    QdkInstruction *top = instruction_at(circuit, 0);
    QdkInstruction *nested = child_instruction(circuit, top, 0, 0);
    QdkInstruction *jump = child_instruction(circuit, nested, 0, 0);
    expect_name(jump, "break");
    free_instruction(&jump);
    free_instruction(&nested);
    free_instruction(&top);
    free_builder(&branch);
    free_builder(&loop);
    require_success(qdk_expression_free(&condition, &error), &error);
    free_circuit(&circuit);
}

static void continue_requires_a_loop_context(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    assert(qdk_circuit_jump(circuit, QDK_JUMP_CONTINUE, &error) == QDK_INVALID_ARGUMENT);
    qdk_diagnostic_free(&error);
    QdkExpression *condition = true_expression(circuit);
    QdkBuilder *loop = begin_body(circuit, NULL, QDK_BODY_WHILE, condition);
    require_success(qdk_circuit_jump(circuit, QDK_JUMP_CONTINUE, &error), &error);
    finish_builder(loop);
    QdkInstruction *top = instruction_at(circuit, 0);
    QdkInstruction *jump = child_instruction(circuit, top, 0, 0);
    expect_name(jump, "continue");
    free_instruction(&jump);
    free_instruction(&top);
    free_builder(&loop);
    require_success(qdk_expression_free(&condition, &error), &error);
    free_circuit(&circuit);
}

static void unfinished_builder_release_is_refused(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkBuilder *body = begin_body(circuit, NULL, QDK_BODY_BOX, NULL);
    QdkBuilder *original = body;
    assert(qdk_builder_free(&body, &error) == QDK_UNFINISHED);
    assert(body == original);
    qdk_diagnostic_free(&error);
    require_success(qdk_builder_cancel(body, &error), &error);
    free_builder(&body);
    free_circuit(&circuit);
}

static void cancelled_builder_cannot_publish_or_edit(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkBuilder *body = begin_body(circuit, NULL, QDK_BODY_BOX, NULL);
    append_body_gate(body, QDK_GATE_X, 0);
    require_success(qdk_builder_cancel(body, &error), &error);
    assert(qdk_builder_finish(body, &error) == QDK_INVALID_STATE);
    qdk_diagnostic_free(&error);
    uint32_t qubit = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_H, .qubits = &qubit, .qubit_count = 1};
    assert(qdk_builder_append(body, &operation, &error) == QDK_INVALID_STATE);
    qdk_diagnostic_free(&error);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(circuit, &info, &error), &error);
    assert(info.instructions == 0);
    free_builder(&body);
    free_circuit(&circuit);
}

static void switch_release_with_live_case_is_refused(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkExpression *selector = true_expression(circuit);
    QdkBuilder *selected = begin_body(circuit, NULL, QDK_BODY_SWITCH, selector);
    QdkBuilder *body = NULL;
    require_success(qdk_switch_case(selected, NULL, 0, 1, &body, &error), &error);
    finish_builder(body);
    finish_builder(selected);
    QdkBuilder *original = selected;
    assert(qdk_builder_free(&selected, &error) == QDK_IN_USE);
    assert(selected == original);
    qdk_diagnostic_free(&error);
    free_builder(&body);
    free_builder(&selected);
    require_success(qdk_expression_free(&selector, &error), &error);
    free_circuit(&circuit);
}

int main(void) {
    restored_symbolic_arithmetic_is_inspectable_and_binds_a_new_circuit();
    if_else_preserves_separate_bodies_and_condition();
    while_body_retains_its_stored_condition();
    parameter_for_loop_uses_compile_time_angles();
    restored_runtime_loop_exposes_its_variable_metadata_and_condition();
    switch_records_case_values_and_default_separately();
    box_label_and_readonly_block_survive_builder_release();
    moment_overlap_is_reported_without_publishing();
    break_reaches_an_enclosing_loop_through_a_conditional();
    continue_requires_a_loop_context();
    unfinished_builder_release_is_refused();
    cancelled_builder_cannot_publish_or_edit();
    switch_release_with_live_case_is_refused();
    puts("13 C structured circuit tests passed");
    return 0;
}
