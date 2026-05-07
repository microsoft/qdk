# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from enum import Enum


class Errors(Enum):
    UNRESTRICTED_INVALID_QIR_TARGET = 1
    RUN_TERMINATED_WITHOUT_OUTPUT = 2
    FAILED_TO_EXPORT_QASM = 3
    MISSING_NUMBER_OF_SHOTS = 4
    INPUT_MUST_BE_QC = 5
    ONLY_ONE_CIRCUIT_ALLOWED = 6

    def __str__(self):
        if self == Errors.UNRESTRICTED_INVALID_QIR_TARGET:
            return "The Unrestricted profile is not valid when generating QIR."
        elif self == Errors.RUN_TERMINATED_WITHOUT_OUTPUT:
            return "Run terminated without valid output."
        elif self == Errors.FAILED_TO_EXPORT_QASM:
            return "Failed to export QASM source."
        elif self == Errors.MISSING_NUMBER_OF_SHOTS:
            return "The number of shots must be specified."
        elif self == Errors.INPUT_MUST_BE_QC:
            return "Input must be a QuantumCircuit."
        elif self == Errors.ONLY_ONE_CIRCUIT_ALLOWED:
            return "Only one QuantumCircuit can be estimated at a time."
        else:
            return "Unknown option."
