// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"
#include <math.h>

static void standard_gate_metadata_and_ordered_operands_are_complete(void) {
    const uint32_t tags[] = {
        QDK_GATE_I, QDK_GATE_X, QDK_GATE_Y, QDK_GATE_Z, QDK_GATE_H,
        QDK_GATE_S, QDK_GATE_SDG, QDK_GATE_SX, QDK_GATE_SXDG, QDK_GATE_T,
        QDK_GATE_TDG, QDK_GATE_CX, QDK_GATE_CY, QDK_GATE_CZ, QDK_GATE_SWAP,
        QDK_GATE_CCX, QDK_GATE_CSWAP, QDK_GATE_RX, QDK_GATE_RY, QDK_GATE_RZ,
        QDK_GATE_RXX, QDK_GATE_RYY, QDK_GATE_RZZ, QDK_GATE_CRX, QDK_GATE_CRY,
        QDK_GATE_CRZ, QDK_GATE_MEASURE_X, QDK_GATE_MEASURE_Y, QDK_GATE_MEASURE_Z,
        QDK_GATE_PREPARE_X, QDK_GATE_PREPARE_Y, QDK_GATE_PREPARE_Z
    };
    const char *names[] = {
        "i", "x", "y", "z", "h", "s", "sdg", "sx", "sxdg", "t", "tdg",
        "cx", "cy", "cz", "swap", "ccx", "cswap", "rx", "ry", "rz",
        "rxx", "ryy", "rzz", "crx", "cry", "crz", "measure_x", "measure_y",
        "measure_z", "prepare_x", "prepare_y", "prepare_z"
    };
    QdkCircuit *circuit = new_circuit(3, 1);
    QdkDiagnostic *error = NULL;
    const uint32_t qubits[] = {2, 0, 1};
    const uint32_t clbit = 0;
    QdkAngle angle = {.kind = QDK_ANGLE_LITERAL, .literal = 0.25};
    for (size_t i = 0; i < sizeof(tags) / sizeof(tags[0]); ++i) {
        QdkGateInfo gate = {0};
        require_success(qdk_gate_info(tags[i], &gate, &error), &error);
        assert(gate.name.len == strlen(names[i]));
        assert(memcmp(gate.name.data, names[i], gate.name.len) == 0);
        QdkOperation operation = {
            .kind = QDK_OPERATION_STANDARD, .gate = tags[i],
            .qubits = qubits, .qubit_count = gate.qubit_arity,
            .clbits = &clbit, .clbit_count = gate.clbit_arity,
            .params = &angle, .param_count = gate.param_arity
        };
        require_success(qdk_circuit_append(circuit, &operation, &error), &error);
        QdkInstruction *instruction = instruction_at(circuit, i);
        expect_name(instruction, names[i]);
        QdkInstructionInfo info = {0};
        require_success(qdk_instruction_info(instruction, &info, &error), &error);
        assert(info.kind == QDK_OPERATION_STANDARD && info.gate == tags[i]);
        assert(info.estimator_id == gate.estimator_id);
        assert(info.qubit_count == gate.qubit_arity);
        assert(info.clbit_count == gate.clbit_arity);
        assert(info.param_count == gate.param_arity);
        uint32_t copied[3] = {0};
        size_t needed = 0;
        require_success(qdk_instruction_qubits(instruction, copied, 3, &needed, &error), &error);
        assert(needed == gate.qubit_arity);
        assert(memcmp(copied, qubits, needed * sizeof(uint32_t)) == 0);
        free_instruction(&instruction);
        qdk_bytes_free(&gate.name);
    }
    free_circuit(&circuit);
}

static void registers_are_copied_with_fixed_nonoverlapping_ranges(void) {
    QdkRegisterInput registers[] = {
        {QDK_REGISTER_QUANTUM, (const uint8_t *)"left", 4, 2},
        {QDK_REGISTER_CLASSICAL, (const uint8_t *)"read", 4, 1},
        {QDK_REGISTER_QUANTUM, (const uint8_t *)"right", 5, 1}
    };
    QdkCircuit *circuit = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_from_registers(registers, 3, 2, &circuit, &error), &error);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(circuit, &info, &error), &error);
    assert(info.qubits == 3 && info.clbits == 3);
    assert(info.quantum_registers == 2 && info.classical_registers == 1);
    QdkRegisterInfo reg = {0};
    require_success(qdk_circuit_register(circuit, QDK_REGISTER_QUANTUM, 1, &reg, &error), &error);
    free_circuit(&circuit);
    assert(reg.start == 2 && reg.size == 1);
    assert(reg.name.len == 5 && memcmp(reg.name.data, "right", 5) == 0);
    qdk_bytes_free(&reg.name);
}

static void homogeneous_batch_reports_native_successful_prefix(void) {
    QdkCircuit *circuit = new_circuit(2, 0);
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_reserve(circuit, 10, &error), &error);
    const uint32_t qubits[] = {0, 1, 0, 7, 1, 0};
    QdkBatchProgress progress = {0};
    assert(qdk_circuit_extend_standard(circuit, QDK_GATE_CX, qubits, 6, &progress, &error) == QDK_INVALID_ARGUMENT);
    assert(progress.accepted == 1 && progress.failed_index == 1);
    qdk_diagnostic_free(&error);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(circuit, &info, &error), &error);
    assert(info.instructions == 1);
    free_circuit(&circuit);
}

static void mixed_batch_reports_a_checked_descriptor_failure(void) {
    QdkCircuit *circuit = new_circuit(2, 0);
    QdkDiagnostic *error = NULL;
    const uint32_t qubits[] = {1, 0};
    QdkGateOperands gates[] = {
        {QDK_GATE_H, qubits, 1}, {QDK_GATE_CX, qubits, 2},
        {UINT32_MAX, NULL, 0}, {QDK_GATE_X, qubits, 1}
    };
    QdkBatchProgress progress = {0};
    assert(qdk_circuit_extend(circuit, gates, 4, &progress, &error) == QDK_INVALID_ARGUMENT);
    assert(progress.accepted == 2 && progress.failed_index == 2);
    qdk_diagnostic_free(&error);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(circuit, &info, &error), &error);
    assert(info.instructions == 2);
    free_circuit(&circuit);
}

static void copied_parameter_record_checks_its_live_owner(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    QdkParameter *parameter = NULL;
    require_success(qdk_circuit_parameter(circuit, (const uint8_t *)"theta", 5, &parameter, &error), &error);
    QdkAngle angle = {.kind = QDK_ANGLE_PARAMETER, .parameter = parameter};
    uint32_t target = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_STANDARD, .gate = QDK_GATE_RX,
        .qubits = &target, .qubit_count = 1, .params = &angle, .param_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    QdkInstruction *instruction = instruction_at(circuit, 0);
    require_success(qdk_parameter_free(&parameter, &error), &error);
    QdkCircuit *other = new_circuit(1, 0);
    assert(qdk_instruction_parameter(instruction, other, 0, &parameter, &error) == QDK_WRONG_OWNER);
    assert(parameter == NULL);
    qdk_diagnostic_free(&error);
    require_success(qdk_instruction_parameter(instruction, circuit, 0, &parameter, &error), &error);
    require_success(qdk_parameter_free(&parameter, &error), &error);
    free_circuit(&other);
    free_circuit(&circuit);
    QdkStoredAngle stored = {0};
    size_t needed = 0;
    require_success(qdk_instruction_angles(instruction, &stored, 1, &needed, &error), &error);
    assert(needed == 1 && stored.kind == QDK_ANGLE_PARAMETER && stored.id == 0);
    free_instruction(&instruction);
}

static void custom_payload_is_copied_without_claiming_execution(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    uint32_t target = 0;
    QdkAngle angle = {.kind = QDK_ANGLE_LITERAL, .literal = INFINITY};
    uint8_t name[] = "custom";
    QdkOperation operation = {.kind = QDK_OPERATION_CUSTOM, .qubits = &target, .qubit_count = 1,
        .name = name, .name_len = 6, .params = &angle, .param_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    name[0] = 'X';
    QdkInstruction *instruction = instruction_at(circuit, 0);
    free_circuit(&circuit);
    expect_name(instruction, "custom");
    QdkStoredAngle stored = {0};
    size_t needed = 0;
    require_success(qdk_instruction_angles(instruction, &stored, 1, &needed, &error), &error);
    assert(stored.kind == QDK_ANGLE_LITERAL && isinf(stored.literal));
    free_instruction(&instruction);
}

static void matrix_payload_is_copied_in_row_major_order(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    uint32_t target = 0;
    QdkComplex matrix[] = {{0, 0}, {0, -1}, {0, 1}, {0, 0}};
    QdkOperation operation = {.kind = QDK_OPERATION_UNITARY, .qubits = &target,
        .qubit_count = 1, .matrix = matrix, .matrix_len = 4};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    QdkInstruction *instruction = instruction_at(circuit, 0);
    free_circuit(&circuit);
    QdkComplex copied[4] = {0};
    size_t needed = 0;
    require_success(qdk_instruction_matrix(instruction, copied, 4, &needed, &error), &error);
    assert(needed == 4 && copied[1].imag == -1 && copied[2].imag == 1);
    free_instruction(&instruction);
}

static void numeric_phase_survives_clone(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkCircuit *copy = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_set_global_phase(circuit, NAN, &error), &error);
    require_success(qdk_circuit_clone(circuit, &copy, &error), &error);
    QdkCircuitInfo info = {0};
    require_success(qdk_circuit_info(copy, &info, &error), &error);
    assert(isnan(info.global_phase));
    free_circuit(&copy);
    free_circuit(&circuit);
}

static void reset_uses_a_typed_record(void) {
    QdkCircuit *circuit = new_circuit(2, 0);
    QdkDiagnostic *error = NULL;
    uint32_t qubits[] = {1, 0};
    QdkOperation operation = {.kind = QDK_OPERATION_RESET, .qubits = qubits, .qubit_count = 1};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    QdkInstruction *instruction = instruction_at(circuit, 0);
    QdkInstructionInfo info = {0};
    require_success(qdk_instruction_info(instruction, &info, &error), &error);
    assert(info.kind == QDK_OPERATION_RESET && info.qubit_count == 1);
    free_instruction(&instruction);
    free_circuit(&circuit);
}

static void barrier_preserves_its_ordered_operands(void) {
    QdkCircuit *circuit = new_circuit(2, 0);
    QdkDiagnostic *error = NULL;
    uint32_t qubits[] = {1, 0};
    QdkOperation operation = {.kind = QDK_OPERATION_BARRIER, .qubits = qubits, .qubit_count = 2};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    QdkInstruction *instruction = instruction_at(circuit, 0);
    uint32_t copied[2] = {0};
    size_t needed = 0;
    require_success(qdk_instruction_qubits(instruction, copied, 2, &needed, &error), &error);
    assert(needed == 2 && copied[0] == 1 && copied[1] == 0);
    expect_name(instruction, "barrier");
    free_instruction(&instruction);
    free_circuit(&circuit);
}

static void delay_retains_duration_and_unit(void) {
    QdkCircuit *circuit = new_circuit(1, 0);
    QdkDiagnostic *error = NULL;
    uint32_t qubit = 0;
    QdkOperation operation = {.kind = QDK_OPERATION_DELAY, .qubits = &qubit, .qubit_count = 1,
        .duration = 2.5, .time_unit = QDK_TIME_US};
    require_success(qdk_circuit_append(circuit, &operation, &error), &error);
    QdkInstruction *delay = instruction_at(circuit, 0);
    QdkInstructionInfo info = {0};
    require_success(qdk_instruction_info(delay, &info, &error), &error);
    assert(info.kind == QDK_OPERATION_DELAY && info.time_unit == QDK_TIME_US && info.duration == 2.5);
    free_instruction(&delay);
    free_circuit(&circuit);
}

int main(void) {
    standard_gate_metadata_and_ordered_operands_are_complete();
    registers_are_copied_with_fixed_nonoverlapping_ranges();
    homogeneous_batch_reports_native_successful_prefix();
    mixed_batch_reports_a_checked_descriptor_failure();
    copied_parameter_record_checks_its_live_owner();
    custom_payload_is_copied_without_claiming_execution();
    matrix_payload_is_copied_in_row_major_order();
    numeric_phase_survives_clone();
    reset_uses_a_typed_record();
    barrier_preserves_its_ordered_operands();
    delay_retains_duration_and_unit();
    puts("11 C dense circuit tests passed");
    return 0;
}
