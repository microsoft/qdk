# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .test_circuits import *


def generate_repro_information(
    circuit: "QuantumCircuit", backend: "QSharpBackend", **options
):
    name = circuit.name
    profile_name = str(backend.options.target_profile)
    message = f"Error with Qiskit circuit '{name}'."
    message += "\n"
    message += f"Profile: {profile_name}"
    message += "\n"
    message += f"Options: {options}"
    message += "\n"

    try:
        qasm_source = backend._qasm(circuit, **options)
        message += "QASM source:"
        message += "\n"
        message += str(qasm_source)
    except Exception as ex:
        # if the conversion fails, print the circuit as a string
        # as a fallback since we won't have the qasm source
        message += "\nFailed converting QuantumCircuit to QASM:\n"
        message += str(ex)
        message += "\n"
        message += "QuantumCircuit rendered:"
        message += "\n"
        circuit_str = str(circuit.draw(output="text"))
        message += circuit_str
        return message

    try:
        qsharp_source = backend._qsharp(circuit, **options)
        message += "Q# source:"
        message += "\n"
        message += str(qsharp_source)
    except Exception as ex:
        message += "\nFailed converting QuantumCircuit to Q#:\n"
        message += str(ex)

    return message
