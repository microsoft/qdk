// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "test_helpers.h"

static void circuit_lifecycle_clears_the_owning_slot(void) {
    QdkCircuit *circuit;
    QdkDiagnostic *error;
    require_success(qdk_circuit_new(2, 1, &circuit, &error), &error);
    assert(circuit != NULL);
    require_success(qdk_circuit_free(&circuit, &error), &error);
    assert(circuit == NULL);
    require_success(qdk_circuit_free(&circuit, &error), &error);
    require_success(qdk_circuit_free(NULL, &error), &error);
}

static void circuit_clone_outlives_the_original(void) {
    QdkCircuit *circuit = NULL;
    QdkCircuit *copy = NULL;
    QdkCircuit *second_copy = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_new(2, 1, &circuit, &error), &error);
    require_success(qdk_circuit_clone(circuit, &copy, &error), &error);
    assert(copy != NULL && copy != circuit);
    require_success(qdk_circuit_free(&circuit, &error), &error);
    require_success(qdk_circuit_clone(copy, &second_copy, &error), &error);
    require_success(qdk_circuit_free(&copy, &error), &error);
    require_success(qdk_circuit_free(&second_copy, &error), &error);
}

static void dependent_circuit_release_is_refused(void) {
    QdkCircuit *circuit = NULL;
    QdkParameter *parameter = NULL;
    QdkDiagnostic *error = NULL;
    require_success(qdk_circuit_new(1, 0, &circuit, &error), &error);
    require_success(qdk_circuit_parameter(circuit, (const uint8_t *)"theta", 5,
                                         &parameter, &error), &error);
    QdkCircuit *original = circuit;
    assert(qdk_circuit_free(&circuit, &error) == QDK_IN_USE);
    assert(circuit == original && error != NULL);
    qdk_diagnostic_free(&error);
    uint32_t id = UINT32_MAX;
    require_success(qdk_parameter_id(parameter, &id, &error), &error);
    assert(id == 0);
    require_success(qdk_parameter_free(&parameter, &error), &error);
    assert(parameter == NULL);
    require_success(qdk_circuit_free(&circuit, &error), &error);
}

int main(void) {
    circuit_lifecycle_clears_the_owning_slot();
    circuit_clone_outlives_the_original();
    dependent_circuit_release_is_refused();
    puts("3 C lifecycle tests passed");
    return 0;
}
