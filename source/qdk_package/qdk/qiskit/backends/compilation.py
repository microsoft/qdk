# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from qiskit import QuantumCircuit


class Compilation(dict):
    def __init__(self, circuit: QuantumCircuit, qasm: str, time_taken: str):
        super().__init__()
        self["circuit"] = circuit
        self["qasm"] = qasm
        self["compilation_time_taken"] = time_taken

    @property
    def circuit(self) -> QuantumCircuit:
        return self["circuit"]

    @circuit.setter
    def circuit(self, value: QuantumCircuit):
        self["circuit"] = value

    @property
    def qasm(self) -> str:
        return self["qasm"]

    @qasm.setter
    def qasm(self, value: str):
        self["qasm"] = value

    @property
    def time_taken(self) -> str:
        return self["compilation_time_taken"]

    @time_taken.setter
    def time_taken(self, value: str):
        self["compilation_time_taken"] = value
