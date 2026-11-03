// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"

static QdkString text(const char *value) {
    return (QdkString){(const uint8_t *)value, strlen(value)};
}

static void expect_bytes(QdkBytes bytes, const char *expected) {
    assert(bytes.len == strlen(expected));
    assert(memcmp(bytes.data, expected, bytes.len) == 0);
    qdk_bytes_free(&bytes);
}

static void free_graph(QdkGraph **graph) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_graph_free(graph, &error), &error);
}

static QdkGraphRecord *record_at(QdkGraph *graph, uint32_t kind, size_t index) {
    QdkDiagnostic *error = NULL;
    QdkGraphRecord *record = NULL;
    require_success(qdk_graph_record(graph, kind, index, &record, &error), &error);
    return record;
}

static void free_record(QdkGraphRecord **record) {
    QdkDiagnostic *error = NULL;
    require_success(qdk_graph_record_free(record, &error), &error);
}

static QdkGraphType *graph_type(uint32_t kind, const QdkGraphType **children, size_t count) {
    QdkDiagnostic *error = NULL;
    QdkGraphType *type = NULL;
    require_success(qdk_graph_type_new(kind, children, count, &type, &error), &error);
    return type;
}

static uint32_t graph_node(QdkGraphBuilder *builder, uint32_t region, const char *operation) {
    QdkDiagnostic *error = NULL;
    uint32_t node = 0;
    require_success(qdk_graph_builder_node(builder, region, text(operation), NULL, 0, &node, &error), &error);
    return node;
}

static uint32_t graph_port(QdkGraphBuilder *builder, uint32_t node, uint32_t direction,
                           QdkGraphType *type, uint32_t usage) {
    QdkDiagnostic *error = NULL;
    uint32_t port = 0;
    QdkGraphPortInput input = {node, direction, QDK_PORT_VALUE, type, QDK_STAGE_GENERATION, usage};
    require_success(qdk_graph_builder_port(builder, &input, &port, &error), &error);
    return port;
}

static QdkGraph *finish_graph(QdkGraphBuilder **builder) {
    QdkDiagnostic *error = NULL;
    QdkGraph *graph = NULL;
    require_success(qdk_graph_builder_finish(*builder, &graph, &error), &error);
    require_success(qdk_graph_builder_free(builder, &error), &error);
    return graph;
}

static void graph_records_copy_nested_types_effects_payloads_and_interfaces(void) {
    QdkDiagnostic *error = NULL;
    QdkGraphBuilder *builder = NULL;
    require_success(qdk_graph_builder_new(&builder, &error), &error);
    uint32_t outer = 0, inner = 0, producer = 0;
    require_success(qdk_graph_builder_region(builder, QDK_REGION_MODULE, (QdkOptionalId){0}, &outer, &error), &error);
    require_success(qdk_graph_builder_region(builder, QDK_REGION_GAMMA, (QdkOptionalId){1, outer}, &inner, &error), &error);
    QdkGraphEffectInput effect = {QDK_EFFECT_HOST, QDK_EFFECT_READ, 1, 1, text("resource")};
    require_success(qdk_graph_builder_node(builder, outer, text("fixture.producer"), &effect, 1, &producer, &error), &error);
    uint32_t consumer = graph_node(builder, outer, "fixture.consumer");
    uint32_t nested = graph_node(builder, inner, "fixture.body");
    require_success(qdk_graph_builder_region_owner(builder, inner, producer, &error), &error);
    require_success(qdk_graph_builder_symbol(builder, producer, text("package"), text("item"), &error), &error);
    require_success(qdk_graph_builder_demand(builder, consumer, &error), &error);

    QdkGraphType *integer = graph_type(QDK_TYPE_INT, NULL, 0);
    const QdkGraphType *one_child[] = {integer};
    QdkGraphType *location = graph_type(QDK_TYPE_LOCATION, one_child, 1);
    const QdkGraphType *tuple_children[] = {integer, location};
    QdkGraphType *tuple = graph_type(QDK_TYPE_TUPLE, tuple_children, 2);
    const QdkGraphType *array_child[] = {tuple};
    QdkGraphType *array = graph_type(QDK_TYPE_ARRAY, array_child, 1);
    uint32_t produced = graph_port(builder, producer, QDK_PORT_OUTPUT, array, QDK_USAGE_COPYABLE);
    uint32_t consumed = graph_port(builder, consumer, QDK_PORT_INPUT, array, QDK_USAGE_COPYABLE);
    uint32_t captured = graph_port(builder, nested, QDK_PORT_INPUT, array, QDK_USAGE_COPYABLE);
    require_success(qdk_graph_builder_connect(builder, (QdkGraphEdge){produced, consumed, QDK_EDGE_VALUE}, &error), &error);
    require_success(qdk_graph_builder_bind(builder, inner, (QdkGraphBinding){produced, captured, QDK_INTERFACE_CAPTURE}, &error), &error);

    QdkGraphAngle *parameter = NULL, *negation = NULL, *literal = NULL, *sum = NULL;
    QdkGraphAngleInput input = {.kind = QDK_GRAPH_ANGLE_PARAMETER, .name = text("theta")};
    require_success(qdk_graph_angle_new(&input, &parameter, &error), &error);
    input = (QdkGraphAngleInput){.kind = QDK_GRAPH_ANGLE_EXPRESSION, .operation = QDK_ANGLE_NEG, .left = parameter};
    require_success(qdk_graph_angle_new(&input, &negation, &error), &error);
    input = (QdkGraphAngleInput){.kind = QDK_GRAPH_ANGLE_LITERAL, .literal = 2.0};
    require_success(qdk_graph_angle_new(&input, &literal, &error), &error);
    input = (QdkGraphAngleInput){.kind = QDK_GRAPH_ANGLE_EXPRESSION, .operation = QDK_ANGLE_ADD, .left = negation, .right = literal};
    require_success(qdk_graph_angle_new(&input, &sum, &error), &error);
    QdkGraphPayload payload = {.kind = QDK_PAYLOAD_ANGLE, .angle = sum,
        .origin = {QDK_ORIGIN_INSTRUCTION, 7, 0, 0}};
    require_success(qdk_graph_builder_payload(builder, producer, &payload, &error), &error);
    QdkGraph *graph = finish_graph(&builder);
    QdkGraphRecord *node = record_at(graph, QDK_GRAPH_NODE, 0);
    QdkGraphRecord *port = record_at(graph, QDK_GRAPH_PORT, 0);
    QdkGraphRecord *region = record_at(graph, QDK_GRAPH_REGION, 1);
    QdkGraphEdge edge = {0};
    size_t needed = 0;
    require_success(qdk_graph_edges(graph, &edge, 1, &needed, &error), &error);
    assert(needed == 1 && edge.source == produced && edge.target == consumed && edge.kind == QDK_EDGE_VALUE);
    QdkCircuit *unsupported = NULL;
    QdkGraphOrigins *origins = NULL;
    assert(qdk_graph_to_circuit(graph, &unsupported, &origins, &error) == QDK_INVALID_ARGUMENT);
    assert(unsupported == NULL && origins == NULL);
    QdkBytes refusal = {0};
    assert(qdk_diagnostic_message(error, &refusal, NULL) == QDK_OK);
    expect_bytes(refusal, "operation 'fixture.producer' has no circuit form");
    qdk_diagnostic_free(&error);
    free_graph(&graph);
    require_success(qdk_graph_type_free(&integer, &error), &error);
    require_success(qdk_graph_type_free(&location, &error), &error);
    require_success(qdk_graph_type_free(&tuple, &error), &error);
    require_success(qdk_graph_type_free(&array, &error), &error);
    require_success(qdk_graph_angle_free(&parameter, &error), &error);
    require_success(qdk_graph_angle_free(&negation, &error), &error);
    require_success(qdk_graph_angle_free(&literal, &error), &error);
    require_success(qdk_graph_angle_free(&sum, &error), &error);

    QdkGraphNodeInfo info = {0};
    require_success(qdk_graph_node_info(node, &info, &error), &error);
    assert(info.id == producer && info.region == outer && info.output_count == 1);
    assert(info.has_symbol == 1 && info.effect_count == 1 && info.payload_kind == QDK_PAYLOAD_ANGLE);
    assert(info.origin.kind == QDK_ORIGIN_INSTRUCTION && info.origin.index == 7);
    QdkBytes bytes = {0};
    require_success(qdk_graph_record_text(node, QDK_GRAPH_OPERATION, 0, &bytes, &error), &error);
    expect_bytes(bytes, "fixture.producer");
    require_success(qdk_graph_record_text(node, QDK_GRAPH_SYMBOL_PACKAGE, 0, &bytes, &error), &error);
    expect_bytes(bytes, "package");
    require_success(qdk_graph_record_text(node, QDK_GRAPH_SYMBOL_ITEM, 0, &bytes, &error), &error);
    expect_bytes(bytes, "item");
    require_success(qdk_graph_record_text(node, QDK_GRAPH_EFFECT_RESOURCE, 0, &bytes, &error), &error);
    expect_bytes(bytes, "resource");
    QdkGraphEffectInfo copied_effect = {0};
    require_success(qdk_graph_record_effects(node, &copied_effect, 1, &needed, &error), &error);
    assert(needed == 1 && copied_effect.domain == QDK_EFFECT_HOST && copied_effect.mode == QDK_EFFECT_READ);
    assert(copied_effect.observable == 1 && copied_effect.has_resource == 1);
    uint32_t copied_port = 0;
    require_success(qdk_graph_record_ports(node, QDK_GRAPH_OUTPUTS, &copied_port, 1, &needed, &error), &error);
    assert(needed == 1 && copied_port == produced);
    require_success(qdk_graph_record_angle(node, &sum, &error), &error);
    free_record(&node);
    QdkGraphAngleInfo angle_info = {0};
    require_success(qdk_graph_angle_info(sum, &angle_info, &error), &error);
    assert(angle_info.operation == QDK_ANGLE_ADD && angle_info.child_count == 2 && angle_info.parameter_count == 1);
    require_success(qdk_graph_angle_child(sum, 0, &negation, &error), &error);
    require_success(qdk_graph_angle_info(negation, &angle_info, &error), &error);
    assert(angle_info.operation == QDK_ANGLE_NEG && angle_info.child_count == 1);
    require_success(qdk_graph_angle_parameter(sum, 0, &bytes, &error), &error);
    expect_bytes(bytes, "theta");
    require_success(qdk_graph_angle_child(sum, 1, &literal, &error), &error);
    require_success(qdk_graph_angle_info(literal, &angle_info, &error), &error);
    assert(angle_info.kind == QDK_GRAPH_ANGLE_LITERAL && angle_info.literal == 2.0);
    require_success(qdk_graph_angle_free(&sum, &error), &error);
    require_success(qdk_graph_angle_free(&negation, &error), &error);
    require_success(qdk_graph_angle_free(&literal, &error), &error);

    QdkGraphRegionInfo region_info = {0};
    require_success(qdk_graph_region_info(region, &region_info, &error), &error);
    assert(region_info.kind == QDK_REGION_GAMMA && region_info.parent.present == 1 && region_info.parent.value == outer);
    assert(region_info.owner.present == 1 && region_info.owner.value == producer && region_info.binding_count == 1);
    QdkGraphBinding binding = {0};
    require_success(qdk_graph_record_bindings(region, &binding, 1, &needed, &error), &error);
    assert(needed == 1 && binding.outer == produced && binding.inner == captured && binding.role == QDK_INTERFACE_CAPTURE);
    require_success(qdk_graph_record_ports(region, QDK_GRAPH_ARGUMENTS, NULL, 0, &needed, &error), &error);
    assert(needed == 0);
    require_success(qdk_graph_record_exits(region, NULL, 0, &needed, &error), &error);
    assert(needed == 0);
    free_record(&region);
    QdkGraphPortInfo port_info = {0};
    require_success(qdk_graph_port_info(port, &port_info, &error), &error);
    assert(port_info.id == produced && port_info.node == producer && port_info.region == outer);
    assert(port_info.direction == QDK_PORT_OUTPUT && port_info.kind == QDK_PORT_VALUE);
    assert(port_info.stage == QDK_STAGE_GENERATION && port_info.usage == QDK_USAGE_COPYABLE);
    require_success(qdk_graph_record_type(port, &array, &error), &error);
    free_record(&port);
    QdkGraphTypeInfo type_info = {0};
    require_success(qdk_graph_type_info(array, &type_info, &error), &error);
    assert(type_info.kind == QDK_TYPE_ARRAY && type_info.child_count == 1);
    require_success(qdk_graph_type_child(array, 0, &tuple, &error), &error);
    require_success(qdk_graph_type_info(tuple, &type_info, &error), &error);
    assert(type_info.kind == QDK_TYPE_TUPLE && type_info.child_count == 2);
    require_success(qdk_graph_type_child(tuple, 1, &location, &error), &error);
    require_success(qdk_graph_type_info(location, &type_info, &error), &error);
    assert(type_info.kind == QDK_TYPE_LOCATION && type_info.child_count == 1);
    require_success(qdk_graph_type_child(location, 0, &integer, &error), &error);
    require_success(qdk_graph_type_info(integer, &type_info, &error), &error);
    assert(type_info.kind == QDK_TYPE_INT && type_info.child_count == 0);
    require_success(qdk_graph_type_free(&array, &error), &error);
    require_success(qdk_graph_type_free(&tuple, &error), &error);
    require_success(qdk_graph_type_free(&location, &error), &error);
    require_success(qdk_graph_type_free(&integer, &error), &error);
}

static void locally_accepted_fanout_survives_finish_with_verifier_diagnostics(void) {
    QdkDiagnostic *error = NULL;
    QdkGraphBuilder *builder = NULL;
    require_success(qdk_graph_builder_new(&builder, &error), &error);
    uint32_t region = 0, rejected_port = 99;
    require_success(qdk_graph_builder_region(builder, QDK_REGION_MODULE, (QdkOptionalId){0}, &region, &error), &error);
    QdkGraphType *qubit = graph_type(QDK_TYPE_QUBIT, NULL, 0);
    const QdkGraphType *children[] = {qubit};
    QdkGraphType *array = graph_type(QDK_TYPE_ARRAY, children, 1);
    uint32_t producer = graph_node(builder, region, "quantum.allocate_array");
    QdkGraphPortInput copyable = {producer, QDK_PORT_OUTPUT, QDK_PORT_VALUE, array, QDK_STAGE_GENERATION, QDK_USAGE_COPYABLE};
    assert(qdk_graph_builder_port(builder, &copyable, &rejected_port, &error) == QDK_INVALID_ARGUMENT);
    assert(rejected_port == 0);
    qdk_diagnostic_free(&error);
    uint32_t source = graph_port(builder, producer, QDK_PORT_OUTPUT, array, QDK_USAGE_AFFINE);
    uint32_t first = graph_node(builder, region, "quantum.apply_all");
    uint32_t second = graph_node(builder, region, "quantum.release_array");
    uint32_t left = graph_port(builder, first, QDK_PORT_INPUT, array, QDK_USAGE_AFFINE);
    uint32_t right = graph_port(builder, second, QDK_PORT_INPUT, array, QDK_USAGE_AFFINE);
    require_success(qdk_graph_builder_connect(builder, (QdkGraphEdge){source, left, QDK_EDGE_VALUE}, &error), &error);
    require_success(qdk_graph_builder_connect(builder, (QdkGraphEdge){source, right, QDK_EDGE_VALUE}, &error), &error);
    QdkGraph *graph = NULL, *duplicate = NULL;
    require_success(qdk_graph_builder_finish(builder, &graph, &error), &error);
    assert(qdk_graph_builder_finish(builder, &duplicate, &error) == QDK_INVALID_STATE);
    assert(duplicate == NULL);
    qdk_diagnostic_free(&error);
    require_success(qdk_graph_builder_free(&builder, &error), &error);
    QdkGraphDiagnostics *diagnostics = NULL;
    size_t count = 0;
    require_success(qdk_graph_verify(graph, &diagnostics, &count, &error), &error);
    assert(count == 1);
    free_graph(&graph);
    QdkBytes code = {0}, message = {0};
    QdkOptionalId node = {0};
    require_success(qdk_graph_diagnostic(diagnostics, 0, &code, &message, &node, &error), &error);
    expect_bytes(code, "QHDG-OWNERSHIP-FANOUT-001");
    expect_bytes(message, "affine capability has multiple uses");
    assert(node.present == 1 && node.value == producer);
    require_success(qdk_graph_diagnostics_free(&diagnostics, &error), &error);
    require_success(qdk_graph_type_free(&array, &error), &error);
    require_success(qdk_graph_type_free(&qubit, &error), &error);
}

static QdkGraphRecord *symbolic_node(QdkGraph *graph, QdkGraphNodeInfo *info) {
    QdkDiagnostic *error = NULL;
    QdkGraphInfo dimensions = {0};
    require_success(qdk_graph_info(graph, &dimensions, &error), &error);
    for (size_t i = 0; i < dimensions.node_count; ++i) {
        QdkGraphRecord *record = record_at(graph, QDK_GRAPH_NODE, i);
        require_success(qdk_graph_node_info(record, info, &error), &error);
        if (info->payload_kind == QDK_PAYLOAD_ANGLE) return record;
        free_record(&record);
    }
    abort();
}

static void partial_graph_binding_preserves_symbolic_conditional_identity(void) {
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = new_circuit(1, 1), *raised = NULL;
    uint32_t target = 0;
    QdkOperation measure = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_MEASURE_Z,
        .qubits = &target, .qubit_count = 1, .clbits = &target, .clbit_count = 1};
    require_success(qdk_circuit_append(circuit, &measure, &error), &error);
    QdkParameter *theta = NULL, *phi = NULL;
    require_success(qdk_circuit_parameter(circuit, (const uint8_t *)"theta", 5, &theta, &error), &error);
    require_success(qdk_circuit_parameter(circuit, (const uint8_t *)"phi", 3, &phi, &error), &error);
    QdkAngle left = {.kind = QDK_ANGLE_PARAMETER, .parameter = theta};
    QdkAngle right = {.kind = QDK_ANGLE_PARAMETER, .parameter = phi};
    QdkAngleExpression *sum = NULL;
    require_success(qdk_circuit_angle(circuit, QDK_ANGLE_ADD, &left, &right, &sum, &error), &error);
    QdkExpression *condition = make_expression(circuit, (QdkExpressionInput){.kind = QDK_EXPR_CLBIT, .clbit = 0});
    QdkBuilder *body = begin_body(circuit, NULL, QDK_BODY_IF, condition);
    QdkAngle angle = {.kind = QDK_ANGLE_EXPRESSION, .expression = sum};
    QdkOperation rotate = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &target, .qubit_count = 1, .params = &angle, .param_count = 1};
    require_success(qdk_builder_append(body, &rotate, &error), &error);
    finish_builder(body);
    QdkGraph *graph = NULL, *partial = NULL, *bound = NULL;
    require_success(qdk_circuit_to_graph(circuit, &graph, &error), &error);
    free_builder(&body);
    require_success(qdk_expression_free(&condition, &error), &error);
    require_success(qdk_angle_expression_free(&sum, &error), &error);
    require_success(qdk_parameter_free(&theta, &error), &error);
    require_success(qdk_parameter_free(&phi, &error), &error);
    free_circuit(&circuit);
    QdkBytes original = {0}, unchanged = {0};
    require_success(qdk_graph_export(graph, QDK_FORMAT_GRAPH_BINARY, 0, &original, &error), &error);
    QdkGraphParameterBinding binding = {text("theta"), 0.25};
    require_success(qdk_graph_bind(graph, &binding, 1, &partial, &error), &error);
    QdkGraphInfo dimensions = {0}, partial_dimensions = {0};
    require_success(qdk_graph_info(graph, &dimensions, &error), &error);
    require_success(qdk_graph_info(partial, &partial_dimensions, &error), &error);
    assert(dimensions.region_count == partial_dimensions.region_count);
    assert(dimensions.node_count == partial_dimensions.node_count);
    assert(dimensions.port_count == partial_dimensions.port_count);
    assert(dimensions.edge_count == partial_dimensions.edge_count);
    QdkGraphEdge *edges = calloc(dimensions.edge_count, sizeof(*edges));
    QdkGraphEdge *partial_edges = calloc(dimensions.edge_count, sizeof(*partial_edges));
    assert(edges != NULL && partial_edges != NULL);
    size_t edge_count = 0;
    require_success(qdk_graph_edges(graph, edges, dimensions.edge_count, &edge_count, &error), &error);
    require_success(qdk_graph_edges(partial, partial_edges, dimensions.edge_count, &edge_count, &error), &error);
    for (size_t i = 0; i < edge_count; ++i) {
        assert(edges[i].source == partial_edges[i].source);
        assert(edges[i].target == partial_edges[i].target);
        assert(edges[i].kind == partial_edges[i].kind);
    }
    free(edges);
    free(partial_edges);
    require_success(qdk_graph_export(graph, QDK_FORMAT_GRAPH_BINARY, 0, &unchanged, &error), &error);
    assert(original.len == unchanged.len && memcmp(original.data, unchanged.data, original.len) == 0);
    qdk_bytes_free(&original);
    qdk_bytes_free(&unchanged);
    QdkGraphNodeInfo before = {0}, after = {0};
    QdkGraphRecord *source = symbolic_node(graph, &before);
    QdkGraphRecord *result = symbolic_node(partial, &after);
    assert(before.id == after.id && before.region == after.region && before.origin.index == after.origin.index);
    assert(before.origin.kind == after.origin.kind);
    QdkGraphAngle *remaining = NULL;
    require_success(qdk_graph_record_angle(result, &remaining, &error), &error);
    QdkGraphAngleInfo info = {0};
    require_success(qdk_graph_angle_info(remaining, &info, &error), &error);
    assert(info.parameter_count == 1 && info.operation == QDK_ANGLE_ADD);
    QdkBytes name = {0};
    require_success(qdk_graph_angle_parameter(remaining, 0, &name, &error), &error);
    expect_bytes(name, "phi");
    require_success(qdk_graph_angle_free(&remaining, &error), &error);
    free_record(&source);
    free_record(&result);
    QdkGraphOrigins *origins = NULL;
    require_success(qdk_graph_to_circuit(partial, &raised, &origins, &error), &error);
    QdkGraphOrigin copied[2] = {0};
    size_t needed = 0;
    require_success(qdk_graph_origins_copy(origins, copied, 2, &needed, &error), &error);
    assert(needed == 2 && copied[0].kind == QDK_ORIGIN_INSTRUCTION && copied[0].index == 0);
    assert(copied[1].kind == QDK_ORIGIN_INSTRUCTION && copied[1].index == 1);
    QdkInstruction *conditional = instruction_at(raised, 1);
    expect_name(conditional, "if_else");
    free_instruction(&conditional);
    free_circuit(&raised);
    require_success(qdk_graph_origins_free(&origins, &error), &error);
    binding = (QdkGraphParameterBinding){text("phi"), 0.5};
    require_success(qdk_graph_bind(partial, &binding, 1, &bound, &error), &error);
    QdkBytes qir = {0}, qasm = {0};
    require_success(qdk_graph_export(bound, QDK_FORMAT_QIR, QDK_PROFILE_ADAPTIVE_RIF, &qir, &error), &error);
    require_success(qdk_graph_export(bound, QDK_FORMAT_QASM, 0, &qasm, &error), &error);
    assert(qir.len > 0 && qasm.len > 0);
    qdk_bytes_free(&qir);
    qdk_bytes_free(&qasm);
    free_graph(&bound);
    free_graph(&partial);
    free_graph(&graph);
}

static void graph_snapshots_preserve_identity_and_reject_digest_corruption(void) {
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = new_circuit(1, 0);
    uint32_t target = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_H,
        .qubits = &target, .qubit_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    QdkGraph *graph = NULL, *clone = NULL, *restored = NULL;
    require_success(qdk_circuit_to_graph(circuit, &graph, &error), &error);
    free_circuit(&circuit);
    require_success(qdk_graph_clone(graph, &clone, &error), &error);
    QdkBytes snapshot = {0}, roundtrip = {0};
    require_success(qdk_graph_export(graph, QDK_FORMAT_GRAPH_BINARY, 0, &snapshot, &error), &error);
    require_success(qdk_graph_export(clone, QDK_FORMAT_GRAPH_BINARY, 0, &roundtrip, &error), &error);
    assert(snapshot.len == roundtrip.len && memcmp(snapshot.data, roundtrip.data, snapshot.len) == 0);
    qdk_bytes_free(&roundtrip);
    require_success(qdk_graph_import(snapshot.data, snapshot.len, &restored, &error), &error);
    require_success(qdk_graph_export(restored, QDK_FORMAT_GRAPH_BINARY, 0, &roundtrip, &error), &error);
    assert(snapshot.len == roundtrip.len && memcmp(snapshot.data, roundtrip.data, snapshot.len) == 0);
    qdk_bytes_free(&roundtrip);
    free_graph(&restored);
    qdk_bytes_free(&snapshot);
    require_success(qdk_graph_export(graph, QDK_FORMAT_GRAPH_SEXPR, 0, &snapshot, &error), &error);
    size_t changed = 0;
    for (size_t i = 0; i + 9 <= snapshot.len; ++i) {
        if (memcmp(snapshot.data + i, "quantum.h", 9) == 0) {
            snapshot.data[i + 8] = 'x';
            changed++;
        }
    }
    assert(changed == 1);
    assert(qdk_graph_import(snapshot.data, snapshot.len, &restored, &error) == QDK_INVALID_ARGUMENT);
    assert(restored == NULL);
    QdkBytes message = {0};
    assert(qdk_diagnostic_message(error, &message, NULL) == QDK_OK);
    expect_bytes(message, "snapshot digest does not match its payload");
    qdk_diagnostic_free(&error);
    qdk_bytes_free(&snapshot);
    free_graph(&clone);
    free_graph(&graph);
}

static void graph_text_snapshot_roundtrip_matches_binary(void) {
    QdkDiagnostic *error = NULL;
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkGraph *graph = NULL, *restored = NULL;
    require_success(qdk_circuit_to_graph(circuit, &graph, &error), &error);
    QdkBytes text = {0}, binary = {0}, roundtrip = {0};
    require_success(qdk_graph_export(graph, QDK_FORMAT_GRAPH_SEXPR, 0, &text, &error), &error);
    assert(text.len > 10 && memcmp(text.data, "QDKQHDG!(\n", 10) == 0);
    require_success(qdk_graph_import(text.data, text.len, &restored, &error), &error);
    require_success(qdk_graph_export(graph, QDK_FORMAT_GRAPH_BINARY, 0, &binary, &error), &error);
    require_success(qdk_graph_export(restored, QDK_FORMAT_GRAPH_BINARY, 0, &roundtrip, &error), &error);
    assert(binary.len == roundtrip.len && memcmp(binary.data, roundtrip.data, binary.len) == 0);
    qdk_bytes_free(&text);
    qdk_bytes_free(&binary);
    qdk_bytes_free(&roundtrip);
    free_graph(&restored);
    free_graph(&graph);
    free_circuit(&circuit);
}

int main(void) {
    graph_records_copy_nested_types_effects_payloads_and_interfaces();
    locally_accepted_fanout_survives_finish_with_verifier_diagnostics();
    partial_graph_binding_preserves_symbolic_conditional_identity();
    graph_snapshots_preserve_identity_and_reject_digest_corruption();
    graph_text_snapshot_roundtrip_matches_binary();
    puts("5 C graph tests passed");
    return 0;
}
